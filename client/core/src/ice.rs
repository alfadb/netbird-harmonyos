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
//! over to [`ProtectedUdpFdSource`]; empty means fail-closed
//! [`ManagementError::Network`], never an unprotected dial). The feed
//! immediately duplicates the number and retains an owned copy; the shell
//! closes its original. Each take is auditable via
//! `ProtectedUdpFdSource::taken()` (the mgmt `taken == attempts` contract).
//!
//! STAGE 2/4 (official-alignment audit §2.7): the ICE plane no longer gathers
//! per interface and no longer binds a socket per host candidate — upstream's
//! mobile shape is ONE shared UDPMux socket (`client/internal/engine_generic.go:15-16`,
//! `UniversalUDPMuxDefault`), and that is what the whole plane uses now
//! ([`mux_host_candidates`], [`gather_srflx_round`]). The per-interface gather
//! (`gather_candidates`/`gather_interface_round`/`GatheredSocket`) was deleted in
//! stage 4; the provider source survives as the protected-DNS resolver for STUN
//! hostnames and, on the host, as the ONE mux socket's supply.
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

use std::collections::{HashMap, VecDeque};
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::management::ManagementError;
use crate::mgmtsock::{dup_socket_fd, SocketSeamError};
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

    /// HOST-ONLY (N12a) — extra host-type candidate for an EXTERNALLY
    /// REACHABLE address of this peer (port-mapping scenario: the pod's
    /// 10.98.0.180 is unreachable from the peer LAN, so the operator
    /// explicitly advertises the mapped `host-lan-ip:port`).
    ///
    /// Wire form is the REGULAR host shape ([`Candidate::marshal`], the
    /// `Body.payload` grammar): same type (`host`), same component, no
    /// `raddr`/`rport`. Priority is ONE local-preference step BELOW a
    /// genuine host candidate (`LOCAL_PREFERENCE - 1` ⇒ −256): the peer
    /// ranks advertised candidates "same family as host, slightly lower" —
    /// above srflx (type preference 100), below every real interface
    /// address, so an advertised entry never outranks a path that probably
    /// works directly, yet still gets checked and can be selected when it
    /// is the only reachable one. The trade-off (documented in
    /// `docs/self-hosted-interop-plan.md` §端口映射): advertising at full
    /// host priority would make peers prefer the mapped address even where
    /// a direct path exists (needless hairpin through the port mapping);
    /// demoting it further (srflx level) would delay setup in the exact
    /// topology this feature exists for.
    pub fn advertised_host_candidate(addr: [u8; 4], port: u16) -> Self {
        let address = ipv4_str(addr);
        let priority = priority_with_local_pref(CandidateType::Host, LOCAL_PREFERENCE - 1);
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

    /// The wire form `Body.payload` carries (pion `Candidate.Marshal()`):
    /// `candidate:<foundation> <component> udp <priority> <address> <port>
    /// typ <type> [raddr <addr> rport <port>] generation 0` — the RFC 8839
    /// candidate-attribute grammar; trailing extension attributes are legal
    /// and parsers ignore what they do not need. Official pion Marshal omits
    /// the leading `candidate:`, and its UnmarshalCandidate parses this
    /// prefixed form all the same (the prefix rides in the opaque
    /// foundation), so this shape interops in both directions.
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

    /// Inverse of [`Candidate::marshal`]. The `candidate:` prefix is
    /// OPTIONAL: upstream signaler.go forwards `candidate.Marshal()` verbatim
    /// and pion's Marshal (candidate_base.go, v2/v4 alike) emits the bare
    /// RFC 8839 attribute body — `42765714 1 udp 2130706431 192.168.50.3
    /// 51820 typ host` — with no prefix, so official peers arrive
    /// prefix-less (the device run rejected every one as `missing-prefix`).
    /// Also accepts the `a=`-prefixed form and ignores unknown extension
    /// attributes. Anything else is a [`ManagementError::Parse`] with a
    /// stable `candidate:*` token (remote peers can send arbitrary bytes;
    /// a bad candidate drops, nothing panics).
    pub fn unmarshal(payload: &str) -> Result<Self, ManagementError> {
        let bad = |tok: &'static str, detail: String| {
            ManagementError::Parse(format!("candidate:{tok} '{detail}'"))
        };
        let s = payload.trim();
        let s = s.strip_prefix("a=").unwrap_or(s);
        // Optional per the official wire format above. Symmetric on the send
        // side: pion's UnmarshalCandidate parses our prefixed form too (the
        // prefix rides in the opaque, space-free foundation token).
        let s = s.strip_prefix("candidate:").unwrap_or(s);
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
///
/// `pub(crate)` since N5b: `ice_session.rs` needs the same formula for the
/// prflx PRIORITY carried in Binding Requests (RFC 8445 §7.1.2.2: the
/// request's PRIORITY is the local candidate's prflx-type priority).
pub(crate) fn priority_for(typ: CandidateType) -> u32 {
    priority_with_local_pref(typ, LOCAL_PREFERENCE)
}

/// The §5.1.2.1 formula at an explicit local preference — used by
/// [`Candidate::advertised_host_candidate`] to sit exactly one local-
/// preference step below the regular host candidates.
fn priority_with_local_pref(typ: CandidateType, local_pref: u32) -> u32 {
    (1 << 24) * typ.type_preference() + (1 << 8) * local_pref + (256 - COMPONENT_ID)
}

/// Parse one `--advertise-candidate` / `advertised_candidates[]` entry —
/// `"ip:port"` with a STRICT dotted-quad IPv4 literal and a port in
/// `1..=65535` — and build the advertised host candidate ([`Candidate::
/// advertised_host_candidate`]). HOST-ONLY (N12a): the device path never
/// feeds this; malformed input is an explicit [`ManagementError::Request`]
/// (status 0 — local validation), never a guessed candidate.
pub fn parse_advertised_candidate(s: &str) -> Result<Candidate, ManagementError> {
    let bad = |why: String| {
        ManagementError::Request {
            status: 0,
            message: format!("advertised-candidate: '{s}' {why} (expected ip:port)"),
        }
    };
    let s = s.trim();
    let Some((ip, port_raw)) = s.rsplit_once(':') else {
        return Err(bad("lacks a :port".into()));
    };
    if ip.starts_with('[') {
        return Err(bad("ipv6 is unsupported (udp4-only)".into()));
    }
    let octets: Vec<&str> = ip.split('.').collect();
    if octets.len() != 4 {
        return Err(bad("address is not a dotted quad".into()));
    }
    let mut addr = [0u8; 4];
    for (i, o) in octets.iter().enumerate() {
        if o.is_empty() || o.len() > 3 || !o.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad("address is not a dotted quad".into()));
        }
        addr[i] = o.parse().map_err(|_| bad("octet out of range".into()))?;
    }
    let port: u16 = port_raw
        .parse()
        .map_err(|_| bad("port out of range".into()))?;
    if port == 0 {
        return Err(bad("port 0".into()));
    }
    Ok(Candidate::advertised_host_candidate(addr, port))
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

/// dlopen the C runtime for `getifaddrs`/`getaddrinfo`. `libc.so` is the
/// musl/OHOS soname; glibc hosts ship only `libc.so.6` as the loaded DSO
/// (`libc.so` there is a linker-script text stub from libc6-dev, which
/// `dlopen` rejects with "invalid ELF header"). Try the OHOS name first,
/// fall back to the glibc soname — the host-side interop CLI (N10) runs on
/// glibc and must not fail interface enumeration (`interface-enum:` /
/// `stun-resolve:` errors are the observed symptom). Handle may still be
/// null when neither name loads; callers keep their fail-closed errors.
unsafe fn dlopen_libc() -> *mut core::ffi::c_void {
    let handle = sys::dlopen(b"libc.so\0".as_ptr(), sys::RTLD_NOW);
    if !handle.is_null() {
        return handle;
    }
    sys::dlopen(b"libc.so.6\0".as_ptr(), sys::RTLD_NOW)
}

impl InterfaceSource for SystemInterfaces {
    fn list(&self) -> Result<Vec<InterfaceAddr>, ManagementError> {
        Ok(ifaddrs_entries()?
            .into_iter()
            .map(|(name, addr, _mask)| InterfaceAddr { name, addr })
            .collect())
    }
}

/// One `getifaddrs` AF_INET entry: interface name, address, netmask.
type IfAddr = (String, [u8; 4], [u8; 4]);

/// Enumerate this machine's IPv4 interface addresses (name, addr, netmask) via
/// `getifaddrs` — the ONE enumeration point behind both
/// [`SystemInterfaces::list`] (host candidates) and [`system_overlay_nets`]
/// (the overlay-source drop, which needs the netmask upstream reads from
/// `wgaddr.Address.Network`). Fail-closed on a missing symbol or a failed call.
pub fn ifaddrs_entries() -> Result<Vec<IfAddr>, ManagementError> {
    let local = |tok: &'static str| ManagementError::Request { status: 0, message: tok.to_string() };
    unsafe {
        let libc = dlopen_libc();
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
        let mut out: Vec<IfAddr> = Vec::new();
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
                    let mask = if ia.ifa_netmask.is_null() {
                        [255, 255, 255, 255]
                    } else {
                        (*(ia.ifa_netmask as *const sys::sockaddr_in)).sin_addr
                    };
                    out.push((
                        String::from_utf8_lossy(&raw[..len]).into_owned(),
                        sin.sin_addr,
                        mask,
                    ));
                }
            }
            cur = ia.ifa_next;
        }
        freeifaddrs(head);
        Ok(out)
    }
}

/// The client's OWN overlay network(s), in the shape upstream's
/// `UniversalUDPMuxDefault` drops overlay-sourced STUN by
/// (`client/iface/udpmux/universal.go:160-173`: `wg.Network.Contains(src)` /
/// `wg.IPv6Net.Contains(src)`).
///
/// Upstream takes the network from the WireGuard interface's address object.
/// Ours is the same fact read from the same place: every interface the ICE
/// filter REJECTS carries an address we must never treat as a STUN server — the
/// platform tunnel is `vpn-tun` (`NetBirdVpnConfig.ets:45-46`, in
/// [`DEFAULT_INTERFACE_BLACKLIST`]) with the device's `100.108.x.x/…` overlay
/// address. `lo*` is excluded on purpose: a loopback source is not an overlay
/// peer, and the host harnesses (the nbinterop CLI, the crate's own tests) run
/// STUN fixtures over loopback.
pub fn system_overlay_nets(blacklist: &[&str]) -> Vec<crate::ice_mux::OverlayNet> {
    let Ok(entries) = ifaddrs_entries() else { return Vec::new() };
    let mut out: Vec<crate::ice_mux::OverlayNet> = Vec::new();
    for (name, addr, mask) in entries {
        if name.starts_with("lo") || interface_allowed(&name, blacklist) {
            continue;
        }
        let bits = u32::from_be_bytes(mask).count_ones() as u8;
        let net = crate::ice_mux::OverlayNet {
            addr: (u32::from_be_bytes(addr) & u32::from_be_bytes(mask)).to_be_bytes(),
            prefix: bits,
        };
        if !out.contains(&net) {
            out.push(net);
        }
    }
    out
}


/// `DefaultInterfaceBlacklist` verbatim from upstream
/// (`client/internal/profilemanager/config.go:56-59`): the tunnel interface
/// (`wt0` first — `client/iface/configurer/name.go:6`) plus the usual
/// virtual/tunnel/docker names. Prefix-matched by [`interface_allowed`].
///
/// `vpn-tun` is the HarmonyOS platform tunnel interface and carries the
/// device's OWN overlay address, so leaving it out made the tunnel a host
/// candidate: one extra gather socket + one extra long-lived bind socket per
/// peer (a third of a 6-socket budget, 0049 diagnostics report
/// `diag-0049/locals-failed.md` §5.1-4) plus a useless overlay candidate
/// advertised to the peer. Name is not a guess: `client/entry/src/main/ets/
/// vpnextensionability/NetBirdVpnConfig.ets:45-46,:89` pins the platform VPN
/// interface name to `'vpn-tun'`, and `/proc/net/dev` on the device shows
/// exactly that (`docs/n2h-device-round-20260926-findings.md:10`).
pub const DEFAULT_INTERFACE_BLACKLIST: [&str; 15] = [
    "wt0", "wt", "utun", "tun0", "zt", "ZeroTier", "wg", "ts",
    "Tailscale", "tailscale", "docker", "veth", "br-", "lo", "vpn-tun",
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
/// protected; production feeds duplicate immediately and own the queued
/// copy while the shell closes its original. Borrowed test providers use the
/// default `take_owned()` implementation to duplicate on demand; `Err` =
/// fail-closed, the gather records the error and produces NO candidate on
/// that path (governance §二.4 — STUN/ICE sockets protect before use, and
/// there is no unprotected fallback).
pub trait UdpSocketSource: Send + Sync {
    fn take_fd(&self) -> Result<i32, SocketSeamError>;
    fn take_owned(&self) -> Result<OwnedFd, SocketSeamError> {
        let fd = self.take_fd()?;
        Ok(unsafe { OwnedFd::from_raw_fd(dup_socket_fd(fd)?) })
    }
    /// How many sockets a `take_owned` would succeed on right now, when the
    /// source can count them up front. `None` = unknown: the caller must
    /// proceed and rely on the per-take `NoSocket` fail-closed path (the
    /// default keeps every borrowed/test provider working). Production
    /// overrides it so a consumer can observe the queue depth before consuming
    /// anything.
    fn available(&self) -> Option<usize> {
        None
    }
    fn resolve_ipv4(&self, host: &str) -> Result<[u8; 4], ManagementError> {
        resolve_ipv4(host)
    }
}

/// Queue-backed production source — same shape, counters and audit surface
/// as mgmtsock's `ProtectedSocketFdSource` (feed on demand, `taken()`
/// audited against attempts in tests).
#[derive(Debug)]
pub struct ProtectedUdpFdSource {
    queue: Mutex<VecDeque<OwnedFd>>,
    taken: AtomicU64,
    dns: OnceLock<(Arc<crate::socket_supply::SocketSupply>, tokio::runtime::Handle)>,
    resolved: Arc<Mutex<HashMap<String, [u8; 4]>>>,
    dns_epoch: Arc<AtomicU64>,
}

impl ProtectedUdpFdSource {
    pub fn new_with_fd(fd: i32) -> Result<Self, SocketSeamError> {
        let mut q = VecDeque::with_capacity(2);
        if fd >= 0 {
            q.push_back(unsafe { OwnedFd::from_raw_fd(dup_socket_fd(fd)?) });
        }
        Ok(ProtectedUdpFdSource {
            queue: Mutex::new(q), taken: AtomicU64::new(0), dns: OnceLock::new(),
            resolved: Arc::new(Mutex::new(HashMap::new())), dns_epoch: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn attach_dns(&self, supply: Arc<crate::socket_supply::SocketSupply>, runtime: tokio::runtime::Handle) {
        let _ = self.dns.set((supply, runtime));
    }

    pub fn refresh_stun_dns(&self, servers: &[StunServer]) {
        let generation = self.dns_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.resolved.lock().expect("stun dns cache").clear();
        let Some((supply, runtime)) = self.dns.get() else { return };
        for server in servers {
            let host = server.host.clone();
            if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
                self.resolved.lock().expect("stun dns cache").insert(host, ip.octets());
                continue;
            }
            let cache = self.resolved.clone();
            let epoch = self.dns_epoch.clone();
            let supply = supply.clone();
            runtime.spawn(async move {
                if let Ok(ip) = crate::protected_dns::resolve(&host, &supply).await {
                    if epoch.load(Ordering::Acquire) == generation {
                        cache.lock().expect("stun dns cache").insert(host, ip.octets());
                    }
                }
            });
        }
    }

    /// Shell-side resupply: retain a protected UDP socket duplicate now.
    pub fn feed(&self, fd: i32) -> Result<(), SocketSeamError> {
        let owned = unsafe { OwnedFd::from_raw_fd(dup_socket_fd(fd)?) };
        self.queue.lock().expect("udp fd queue").push_back(owned);
        Ok(())
    }

    /// Discard owned pending sockets from the previous physical network.
    pub fn clear_pending(&self) {
        self.queue.lock().expect("udp fd queue").clear();
    }

    /// Sockets currently queued (observability, mgmt parity).
    pub fn pending(&self) -> usize {
        self.queue.lock().expect("udp fd queue").len()
    }

    /// 006 resource probe: the same queue depth, but WITHOUT ever blocking.
    /// `None` = a gather/feed/take holds the queue right now (WouldBlock), so
    /// the probe reports the depth as unknown (`null`) instead of waiting on a
    /// lock that the data plane may be using. Poison is recovered exactly like
    /// [`Self::pending`] would (panic=abort in release, so poison is a
    /// host/test artifact) — same rule as `connector::try_lock_poison`.
    pub fn pending_try(&self) -> Option<usize> {
        match self.queue.try_lock() {
            Ok(guard) => Some(guard.len()),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner().len()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }

    /// 006 test seam: hold the queue lock so a concurrent probe observes the
    /// busy (`None`) branch. Production code never calls this.
    #[cfg(test)]
    pub(crate) fn hold_queue_for_test(&self) -> std::sync::MutexGuard<'_, VecDeque<OwnedFd>> {
        self.queue.lock().expect("udp fd queue")
    }

    /// How many sockets handed out — pinned to the gather's socket attempts
    /// (every interface round re-acquires; never reuses a consumed fd).
    pub fn taken(&self) -> u64 {
        self.taken.load(Ordering::Acquire)
    }
}

impl UdpSocketSource for ProtectedUdpFdSource {
    fn resolve_ipv4(&self, host: &str) -> Result<[u8; 4], ManagementError> {
        if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() { return Ok(ip.octets()); }
        if self.dns.get().is_none() {
            #[cfg(not(target_env = "ohos"))]
            return resolve_ipv4(host);
        }
        self.resolved.lock().expect("stun dns cache").get(host).copied()
            .ok_or_else(|| ManagementError::Network("stun:protected-dns-pending".into()))
    }

    /// Exact count of queued (already protected, unbound) sockets — the
    /// orchestrator's starvation guard reads this instead of probing the
    /// queue with a doomed `take_owned`.
    fn available(&self) -> Option<usize> {
        Some(self.pending())
    }

    fn take_fd(&self) -> Result<i32, SocketSeamError> {
        self.take_owned().map(IntoRawFd::into_raw_fd)
    }

    fn take_owned(&self) -> Result<OwnedFd, SocketSeamError> {
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
/// `pub(crate)` since N5b: `ice_session.rs` takes sockets from the same
/// provider seam and maps failures onto the same class.
pub(crate) fn seam_to_management(e: SocketSeamError) -> ManagementError {
    ManagementError::Network(format!("protected-udp: {} (errno={})", e.token(), e.errno()))
}

/// `O_NONBLOCK` on the DUP copy — mgmtsock's documented idiom (the shared
/// open-file description flips the provider's flag too; providers must not
/// do blocking I/O after hand-over).
///
/// `pub(crate)` since N5b: `ice_session.rs` prepares its long-lived check
/// sockets with the same idiom.
pub(crate) fn set_nonblock(fd: sys::c_int) -> Result<(), SocketSeamError> {
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

/// Per-STUN-exchange deadline — upstream's value, not ours (official-alignment
/// audit §2.2/§2.7 + §3 U8).
///
/// pion's default is `defaultSTUNGatherTimeout = 5 * time.Second`
/// (netbirdio/ice@6202be846b51 `agent_config.go:41-42`, applied at `:245` and
/// consumed by the srflx gather at `gather.go:526`). NetBird overrides the
/// OTHER pion agent timers — keepalive 4 s, disconnected 6 s, failed 6 s,
/// relay-acceptance 2 s (`client/internal/peer/ice/agent.go:22-28`, applied
/// `:37-61`) — and deliberately leaves this one alone. The old 3000 ms was
/// ours, invented when the exchange ran inline in the pump and every
/// millisecond was a tick of the shared schedule; with the gather off the pump
/// (stage 1) the upstream value costs nothing else and is the number a silent
/// STUN server is actually allowed to cost.
///
/// NB: the audit's §2.7 "explicit non-changes" list says *do not retune*
/// `DEFAULT_STUN_TIMEOUT_MS`; that instruction predates the stage-1 directive
/// ("use upstream's value"), which this constant follows. Reverting is one
/// line if the owner prefers the old value.
pub const DEFAULT_STUN_TIMEOUT_MS: u64 = 5000;

/// Gather parameters. `blacklist` defaults to [`DEFAULT_INTERFACE_BLACKLIST`]
/// at the call site; `servers` is the parsed `netbird_config.stuns`.
pub struct GatherConfig<'a> {
    pub blacklist: &'a [&'a str],
    pub servers: &'a [StunServer],
    pub timeout_ms: u64,
}

/// The interface entries the ICE plane will actually advertise: blacklist
/// prefix filter (same [`interface_allowed`] the gather applies) plus the
/// `(name, addr)` dedup for `getifaddrs`' repeated multi-flag entries, in
/// enumeration order.
pub fn allowed_ifaces(
    cfg: &GatherConfig,
    ifaces: &dyn InterfaceSource,
) -> Result<Vec<InterfaceAddr>, ManagementError> {
    let all = ifaces.list()?;
    let mut out = Vec::new();
    let mut seen: Vec<(String, [u8; 4])> = Vec::new();
    for iface in all {
        if !interface_allowed(&iface.name, cfg.blacklist) {
            continue;
        }
        if seen.iter().any(|(n, a)| *n == iface.name && *a == iface.addr) {
            continue; // getifaddrs repeats entries (multi-flag)
        }
        seen.push((iface.name.clone(), iface.addr));
        out.push(iface);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// mux-shaped gathering — official-alignment audit §2.7 stage 2
// ---------------------------------------------------------------------------

/// Host candidates off the shared mux: one per allowed interface address, ALL
/// carrying the mux's single port — upstream `gatherCandidatesLocalUDPMux`
/// (pion fork `gather.go:331-405`: `localAddresses := a.udpMux.GetListenAddresses()`
/// then `Port: udpAddr.Port` for every one of them). Computed and emitted
/// immediately — no STUN, no socket, no waiting (the stage-1 per-interface
/// socket + STUN round is gone).
///
/// Dedup by (address, port) mirrors upstream's `existingConfigs` guard
/// (`gather.go:373-376`): `getifaddrs` repeats multi-flag entries and two of
/// them must not become two identical candidates.
pub fn mux_host_candidates(ifaces: &[InterfaceAddr], port: u16) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    for iface in ifaces {
        if out.iter().any(|c| c.port == port && c.address == ipv4_str(iface.addr)) {
            continue;
        }
        out.push(Candidate::host_candidate(iface.addr, port));
    }
    out
}

/// The srflx candidates one mux mapping yields: upstream
/// `gatherCandidatesSrflxUDPMux` (fork `gather.go:491-513`) builds ONE srflx
/// candidate per (network type, STUN url, mux listen address) with
/// `RelAddr: localAddr.IP` / `RelPort: localAddr.Port` — i.e. the base is the
/// MUX listen address (interface address × mux port), not a socket that
/// happened to answer. Every listen address shares the one cached mapping, so
/// the `raddr` differs while the mapped `address:port` is identical: that is
/// exactly what upstream advertises, and RFC 8445 §6.1.2.4 lets the peer dedup.
pub fn srflx_candidates_for_listen_addrs(
    ifaces: &[InterfaceAddr],
    port: u16,
    mapped: ([u8; 4], u16),
) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    for iface in ifaces {
        let base = Candidate::host_candidate(iface.addr, port);
        let cand = Candidate::srflx_candidate(&base, mapped.0, mapped.1);
        if out.iter().any(|c| c.address == cand.address && c.port == cand.port
            && c.related_address == cand.related_address)
        {
            continue;
        }
        out.push(cand);
    }
    out
}

/// One (mux, STUN server) exchange and how it ended — the stage-0 unit
/// (`N5c_ICE|gather-stun`) is now the mux's exchange instead of a per-interface
/// socket's: upstream's srflx gather is one `GetXORMappedAddr` call per
/// (network type, STUN url, mux listen address) with the in-flight de-dup
/// collapsing them onto ONE STUN round per server (fork `gather.go:491-513`,
/// `universal.go:222-270`).
#[derive(Debug)]
pub struct SrflxExchange {
    pub server: StunServer,
    pub resolved: Option<([u8; 4], u16)>,
    pub ms: u64,
    pub outcome: Result<([u8; 4], u16), ManagementError>,
}

/// Everything ONE srflx round produced. Never `Err`: a dead server is a
/// recorded per-server outcome (upstream logs one warning per failed exchange
/// and keeps gathering, `gather.go:526-531`), and the mux's cache makes a
/// second server's answer irrelevant to the first's failure.
#[derive(Debug, Default)]
pub struct SrflxGather {
    pub exchanges: Vec<SrflxExchange>,
    /// `("mux/<host>:<port>", error)` per failed exchange — the same
    /// `(context, error)` shape the retired per-interface gather reported.
    pub errors: Vec<(String, ManagementError)>,
}

/// Run ONE srflx round through the shared mux: resolve each STUN server, ask
/// the mux for the shared XOR-MAPPED-ADDRESS and record the outcome. This is
/// what a stage-2 gather WORKER runs (one per negotiation, off the pump; the
/// mux de-duplicates across peers and across the round's own callers).
pub fn gather_srflx_round(
    servers: &[StunServer],
    mux: &crate::ice_mux::IceMux,
    socks: &dyn UdpSocketSource,
    timeout_ms: u64,
) -> SrflxGather {
    let mut out = SrflxGather::default();
    for server in servers {
        let started = sys::mono_ms();
        let ctx = format!("mux/{}:{}", server.host, server.port);
        let resolved = match socks.resolve_ipv4(&server.host) {
            Ok(ip) => Some((ip, server.port)),
            Err(e) => {
                out.errors.push((format!("mux/{}", server.host), e.clone()));
                out.exchanges.push(SrflxExchange {
                    server: server.clone(),
                    resolved: None,
                    ms: sys::mono_ms().saturating_sub(started),
                    outcome: Err(e),
                });
                continue;
            }
        };
        let addr = resolved.expect("just set");
        let outcome = mux.get_xor_mapped_addr(addr, timeout_ms);
        let ms = sys::mono_ms().saturating_sub(started);
        if let Err(e) = &outcome {
            out.errors.push((ctx, e.clone()));
        }
        out.exchanges.push(SrflxExchange { server: server.clone(), resolved, ms, outcome });
    }
    out
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
    #[cfg(target_env = "ohos")]
    return Err(fail("stun:protected-dns-required".into()));
    #[cfg(not(target_env = "ohos"))]
    unsafe {
        let libc = dlopen_libc();
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
// ---------------------------------------------------------------------------

#[cfg(test)]
mod feed_ownership_tests {
    use super::{ProtectedUdpFdSource, UdpSocketSource};
    use crate::sys;
    use std::os::fd::{FromRawFd, IntoRawFd};

    #[test]
    fn protected_stun_dns_cache_is_used_and_refresh_discards_old_address() {
        let source = ProtectedUdpFdSource::new_with_fd(-1).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        source.attach_dns(std::sync::Arc::new(crate::socket_supply::SocketSupply::new()), runtime.handle().clone());
        assert!(source.resolve_ipv4("stun.example.invalid").is_err());
        source.resolved.lock().unwrap().insert("stun.example.invalid".into(), [192, 0, 2, 9]);
        assert_eq!(source.resolve_ipv4("stun.example.invalid").unwrap(), [192, 0, 2, 9]);
        source.refresh_stun_dns(&[]);
        assert!(source.resolve_ipv4("stun.example.invalid").is_err());
        assert_eq!(source.resolve_ipv4("192.0.2.10").unwrap(), [192, 0, 2, 10]);
    }

    #[test]
    fn ice_feed_survives_shell_close_and_fd_number_reuse() {
        let original = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = original.local_addr().unwrap().port();
        let raw = original.into_raw_fd();
        let source = ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source");
        source.feed(raw).unwrap();
        unsafe { sys::close(raw) };
        let replacement = unsafe { sys::socket(sys::AF_INET, sys::SOCK_STREAM, 0) };
        assert!(replacement >= 0);
        // Occupy the freed number ATOMICALLY (the mgmtsock twin test carries
        // the full rationale): `F_DUPFD_CLOEXEC` never closes a descriptor a
        // parallel libtest thread grabbed in the window, while `dup2` would —
        // and the robbed thread's `OwnedFd` drop aborts the whole process with
        // "IO Safety violation: owned file descriptor already closed".
        let pinned = unsafe { sys::fcntl(replacement, sys::F_DUPFD_CLOEXEC, raw) };
        assert!(pinned >= 0, "pin the replacement at/above {raw}: errno={}", sys::errno());
        let fed = unsafe { std::net::UdpSocket::from_raw_fd(source.take_owned().unwrap().into_raw_fd()) };
        assert_eq!(fed.local_addr().unwrap().port(), port);
        unsafe { sys::close(pinned); sys::close(replacement); }
    }

    #[test]
    fn invalid_seed_fd_is_fallible() {
        assert!(ProtectedUdpFdSource::new_with_fd(i32::MAX).is_err());
    }

    #[test]
    fn ice_clear_pending_discards_owned_copy() {
        let original = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let source = ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source");
        let raw = original.into_raw_fd();
        source.feed(raw).unwrap();
        unsafe { sys::close(raw) };
        assert_eq!(source.pending(), 1);
        source.clear_pending();
        assert_eq!(source.pending(), 0);
        assert!(source.take_owned().is_err());
    }

    /// 006: the resource probe's queue read must NEVER wait on the queue lock.
    /// While a gather/feed holds it, `pending_try` answers `None` (unknown)
    /// immediately; released, it answers the real depth again.
    #[test]
    fn ice_pending_try_does_not_block_on_a_held_queue() {
        let source = ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source");
        assert_eq!(source.pending_try(), Some(0));
        let held = source.hold_queue_for_test();
        let t0 = std::time::Instant::now();
        assert_eq!(source.pending_try(), None, "a held queue is unknown, not 0");
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(1),
            "pending_try must not block on the queue lock"
        );
        drop(held);
        assert_eq!(source.pending_try(), Some(0));
    }
}

#[cfg(test)]
mod interface_filter_tests {
    use super::{
        allowed_ifaces, interface_allowed, mux_host_candidates, GatherConfig, InterfaceAddr,
        StaticInterfaces, DEFAULT_INTERFACE_BLACKLIST, DEFAULT_STUN_TIMEOUT_MS,
    };

    /// 0049 §5.1-4：设备 overlay 接口必须不进 host 候选。`vpn-tun` 曾不在
    /// 黑名单里 → 设备自己的 overlay 地址（100.108.144.165）被当第三个
    /// 「可用接口」并对外 advertise 一个隧道内候选。命名证据见
    /// [`DEFAULT_INTERFACE_BLACKLIST`] 的文档（ets 常量 + 设备
    /// `/proc/net/dev`），不是猜测。
    ///
    /// STAGE 4：候选由共享 mux 端口 × 允许接口清单直接算出
    /// （[`mux_host_candidates`]），不再按接口取 socket，所以这里只钉
    /// 「谁进清单、谁进候选」。
    #[test]
    fn overlay_tun_interface_never_becomes_a_host_candidate() {
        // 前缀匹配语义（上游 filter.go:19-24）：`vpn-tun` 同时挡住
        // `vpn-tun0` 一类变体；物理接口不受影响。
        assert!(!interface_allowed("vpn-tun", &DEFAULT_INTERFACE_BLACKLIST));
        assert!(!interface_allowed("vpn-tun0", &DEFAULT_INTERFACE_BLACKLIST));
        assert!(interface_allowed("wlan0", &DEFAULT_INTERFACE_BLACKLIST));

        let ifaces = StaticInterfaces(vec![
            InterfaceAddr { name: "wlan0".into(), addr: [127, 0, 0, 1] },
            InterfaceAddr { name: "vpn-tun".into(), addr: [127, 0, 0, 3] },
        ]);
        let cfg = GatherConfig {
            blacklist: &DEFAULT_INTERFACE_BLACKLIST,
            servers: &[],
            timeout_ms: DEFAULT_STUN_TIMEOUT_MS,
        };
        let listed = allowed_ifaces(&cfg, &ifaces).expect("enumerate");
        assert_eq!(listed.len(), 1, "only the physical interface may host: {listed:?}");

        let got = mux_host_candidates(&listed, 41000);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].address, "127.0.0.1");
        assert!(
            got.iter().all(|c| c.address != "127.0.0.3"),
            "the tun interface's (overlay-range) address must never be advertised"
        );
    }

    /// 预算函数必须与真正广告的接口清单逐项一致（否则「先算后取」
    /// 的守护会算错账）。
    #[test]
    fn allowed_ifaces_matches_the_advertised_candidates() {
        let ifaces = StaticInterfaces(vec![
            InterfaceAddr { name: "wlan0".into(), addr: [127, 0, 0, 1] },
            InterfaceAddr { name: "wlan0".into(), addr: [127, 0, 0, 1] }, // getifaddrs 重复项
            InterfaceAddr { name: "vpn-tun".into(), addr: [100, 108, 144, 165] },
            InterfaceAddr { name: "lo".into(), addr: [127, 0, 0, 1] },
            InterfaceAddr { name: "eth0".into(), addr: [127, 0, 0, 2] },
        ]);
        let cfg = GatherConfig {
            blacklist: &DEFAULT_INTERFACE_BLACKLIST,
            servers: &[],
            timeout_ms: DEFAULT_STUN_TIMEOUT_MS,
        };
        let listed = allowed_ifaces(&cfg, &ifaces).expect("enumerate");
        assert_eq!(
            listed.iter().map(|i| (i.name.as_str(), i.addr)).collect::<Vec<_>>(),
            vec![("wlan0", [127, 0, 0, 1]), ("eth0", [127, 0, 0, 2])]
        );
        let got = mux_host_candidates(&listed, 41000);
        assert_eq!(got.len(), listed.len(), "one host candidate per allowed interface");
    }
}

/// Candidate wire-form interop with the official stack. The official format
/// is pion `Candidate.Marshal()` forwarded verbatim by signaler.go
/// (`Payload: candidate.Marshal()`), and pion's Marshal (candidate_base.go,
/// v2.3.x and v4.0.x identical here) emits the RFC 8839 attribute body
/// WITHOUT a `candidate:` prefix — `<foundation> <component> <net> <prio>
/// <addr> <port> typ <type> [tcptype ..] [raddr .. rport ..]`. The
/// prefix-less samples below are that grammar; prefixed samples are our own
/// [`Candidate::marshal`] shape. Basis: the device run logged every official
/// candidate as `reason=missing-prefix` while its diagnostic field positions
/// (transport=2nd, address=5th, typ=8th token) matched this layout exactly.
#[cfg(test)]
mod candidate_wire_tests {
    use super::{Candidate, CandidateType};

    #[test]
    fn pion_host_candidate_without_prefix_parses() {
        let c = Candidate::unmarshal("42765714 1 udp 2130706431 192.168.50.3 51820 typ host")
            .expect("prefix-less host candidate must parse");
        assert_eq!(c.foundation, "42765714");
        assert_eq!(c.component, 1);
        assert_eq!(c.transport, "udp");
        assert_eq!(c.priority, 2130706431);
        assert_eq!((c.address.as_str(), c.port), ("192.168.50.3", 51820));
        assert_eq!(c.typ, CandidateType::Host);
        assert!(c.related_address.is_none() && c.related_port.is_none());
    }

    /// pion appends `raddr <addr> rport <port>` for srflx (the base).
    #[test]
    fn pion_srflx_candidate_with_raddr_rport_parses() {
        let c = Candidate::unmarshal(
            "3711056448 1 udp 1694498815 203.0.113.7 51820 typ srflx raddr 192.168.50.3 rport 51820",
        )
        .expect("prefix-less srflx candidate must parse");
        assert_eq!(c.typ, CandidateType::Srflx);
        assert_eq!(c.address, "203.0.113.7");
        assert_eq!(
            (c.related_address.as_deref(), c.related_port),
            (Some("192.168.50.3"), Some(51820))
        );
    }

    /// RFC 8839 extension attributes (`ufrag`, `network-id`, `network-cost`,
    /// `generation`, …) are legal trailers — ignored, with or without the
    /// prefix, and under the `a=` SDP form.
    #[test]
    fn extension_attributes_are_ignored_with_and_without_prefix() {
        for wire in [
            "42765714 1 udp 2130706431 192.168.50.3 51820 typ host ufrag FRzs network-id 3 network-cost 50",
            "candidate:42765714 1 udp 2130706431 192.168.50.3 51820 typ host ufrag FRzs generation 0 network-id 3 network-cost 50",
            "a=candidate:42765714 1 udp 2130706431 192.168.50.3 51820 typ host generation 0",
        ] {
            let c = Candidate::unmarshal(wire).unwrap_or_else(|e| panic!("'{wire}': {e:?}"));
            assert_eq!((c.address.as_str(), c.typ), ("192.168.50.3", CandidateType::Host));
        }
    }

    /// relay/prflx round-trip losslessly at the wire layer (the session
    /// layer decides separately what to pair).
    #[test]
    fn relay_and_prflx_types_parse() {
        let c = Candidate::unmarshal(
            "97046741 1 udp 50331391 198.51.100.2 3478 typ relay raddr 203.0.113.7 rport 51820",
        )
        .expect("relay candidate");
        assert_eq!(c.typ, CandidateType::Relay);
        let c = Candidate::unmarshal(
            "61170560 1 udp 1845501695 203.0.113.9 40000 typ prflx raddr 192.168.50.3 rport 51820",
        )
        .expect("prflx candidate");
        assert_eq!(c.typ, CandidateType::Prflx);
    }

    /// Official peers are dual-stack and send IPv6 host candidates without a
    /// zone id (pion strips it in Marshal). The wire layer accepts them;
    /// UDP4-only pairing in ice_session is a separate, deliberate cut.
    #[test]
    fn ipv6_candidate_parses_at_the_wire_layer() {
        let c = Candidate::unmarshal("489324071 1 udp 2130706431 fe80::a1b2:c3d4:e5f6 51820 typ host")
            .expect("IPv6 host candidate");
        assert_eq!(c.address, "fe80::a1b2:c3d4:e5f6");
        assert_eq!(c.typ, CandidateType::Host);
    }

    /// Both accepted forms must describe the SAME candidate — the prefix
    /// carries no information.
    #[test]
    fn prefixed_and_prefix_less_forms_agree() {
        let prefixed =
            "candidate:42765714 1 udp 2130706431 192.168.50.3 51820 typ host generation 0";
        let bare = "42765714 1 udp 2130706431 192.168.50.3 51820 typ host";
        assert_eq!(
            Candidate::unmarshal(prefixed).expect("prefixed"),
            Candidate::unmarshal(bare).expect("bare")
        );
    }

    /// Our own marshal output keeps round-tripping through the same grammar.
    #[test]
    fn marshal_round_trips() {
        let c = Candidate::host_candidate([192, 168, 50, 3], 51820);
        assert_eq!(Candidate::unmarshal(&c.marshal()).unwrap(), c);
        let base = Candidate::host_candidate([192, 168, 50, 3], 51820);
        let s = Candidate::srflx_candidate(&base, [203, 0, 113, 7], 51820);
        assert_eq!(Candidate::unmarshal(&s.marshal()).unwrap(), s);
    }

    /// Truly malformed input is still rejected (each with a stable
    /// `candidate:*` token — asserted as plain failure here).
    #[test]
    fn malformed_candidates_are_rejected() {
        for bad in [
            "",
            "   ",
            "candidate:",                                            // prefix, no body
            "nonsense",                                              // single token
            "42765714 1",                                            // truncated
            "42765714 1 udp",                                        // truncated
            "42765714 1 udp 2130706431",                             // no address/port
            "42765714 1 udp 2130706431 192.168.50.3",                // no port/typ
            "42765714 1 udp 2130706431 192.168.50.3 51820",          // no typ
            "42765714 1 udp 2130706431 192.168.50.3 51820 typ galaxy", // unknown type
            "42765714 1 tcp 2130706431 192.168.50.3 51820 typ host",   // non-udp
            "42765714 1 udp 2130706431 192.168.50.3 99999 typ host",   // port > 65535
            "42765714 x udp 2130706431 192.168.50.3 51820 typ host",   // component NaN
            "42765714 1 udp 2130706431 192.168.50.3 51820 typ srflx raddr 192.168.50.3", // raddr w/o rport
        ] {
            assert!(Candidate::unmarshal(bad).is_err(), "must be rejected: '{bad}'");
        }
    }
}
