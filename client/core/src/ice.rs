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
    /// Warm start only — upstream keeps no STUN DNS cache at all: a hit saves
    /// the on-demand lookup of [`UdpSocketSource::resolve_ipv4`], a miss is
    /// resolved right there. Never a gate.
    resolved: Arc<Mutex<HashMap<String, [u8; 4]>>>,
    dns_epoch: Arc<AtomicU64>,
    /// Destination port of the protected DNS query — production is the DNS
    /// port ([`crate::protected_dns::DNS_PORT`]) and this field does not exist.
    /// The in-crate tests need a fake resolver and a host test cannot bind the
    /// privileged port, so they move it (see
    /// [`ProtectedUdpFdSource::set_dns_port_for_test`]).
    #[cfg(test)]
    dns_port: std::sync::atomic::AtomicU16,
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
            #[cfg(test)]
            dns_port: std::sync::atomic::AtomicU16::new(crate::protected_dns::DNS_PORT),
        })
    }

    pub fn attach_dns(&self, supply: Arc<crate::socket_supply::SocketSupply>, runtime: tokio::runtime::Handle) {
        let _ = self.dns.set((supply, runtime));
    }

    /// Warm the STUN host cache for `servers`. Since F1 this is a pure
    /// optimisation — [`UdpSocketSource::resolve_ipv4`] resolves a miss on
    /// demand — so a refresh can only ever make the next gather cheaper, never
    /// gate it. Two corrections over the pre-F1 shape (report §5 F2):
    ///
    /// * an EMPTY list is a no-op. Its caller is
    ///   `connector.rs::physical_network_changed` with no new management
    ///   address (`connector.rs:5932-5935`), i.e. exactly the transition that
    ///   also empties the shell-fed DNS server list: clearing the cache there
    ///   left the plane with nothing cached and nothing resolving. Upstream's
    ///   analogue is that a network change never empties the OS resolver.
    /// * a failed (or epoch-discarded) lookup is logged, never swallowed:
    ///   the old `if let Ok(ip)` produced no line at all, which is why the
    ///   device logs showed 100 % srflx-less rounds with zero resolution
    ///   attempts.
    pub fn refresh_stun_dns(&self, servers: &[StunServer]) {
        let Some((supply, runtime)) = self.dns.get() else { return };
        if servers.is_empty() { return; }
        let generation = self.dns_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.resolved.lock().expect("stun dns cache").clear();
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
                match crate::protected_dns::resolve(&host, &supply).await {
                    Ok(ip) => {
                        if epoch.load(Ordering::Acquire) == generation {
                            cache.lock().expect("stun dns cache").insert(host, ip.octets());
                        } else {
                            // Keep the epoch guard's intent observable: a newer
                            // refresh owns the cache now, and F1 re-resolves
                            // this host on the next gather instead of leaving
                            // it permanently missing.
                            crate::hilog::emit(&format!(
                                "N5c_ICE|stun-dns|host={host}|outcome=stale|generation={generation}"
                            ));
                        }
                    }
                    Err(e) => crate::hilog::emit(&format!(
                        "N5c_ICE|stun-dns|host={host}|outcome=failed|reason={e}"
                    )),
                }
            });
        }
    }

    /// F1 (report §5): upstream resolves each STUN URL INSIDE the gather
    /// (`gather.go:513` / `:585` → `stdnet.go:218` → `:109-112` `LookupNetIP`,
    /// bounded by `dnsResolveTimeout`) and only that one exchange fails when
    /// the lookup does (`gather.go:515` `Debugf`). This is that lookup for our
    /// protected resolver: a cache miss resolves now, on the gather worker
    /// (never on the pump — `peer_conn.rs` `ThreadGatherExec`), and the result
    /// warms the cache for the next gather. The failure is a real error token
    /// (`socket-supply-rejected` / `dns:no-physical-server` / `dns:timeout`),
    /// never the old `stun:protected-dns-pending` sentinel, and it disables
    /// nothing: the next gather resolves again.
    fn resolve_on_demand(&self, host: &str) -> Result<[u8; 4], ManagementError> {
        let Some((supply, runtime)) = self.dns.get() else {
            // No protected supply: identical to the pre-F1 no-supply branch —
            // the host keeps the OS resolver, the device has none.
            return resolve_ipv4(host);
        };
        match protected_lookup(host, supply, self.dns_port(), runtime) {
            Ok(ip) => {
                self.resolved.lock().expect("stun dns cache").insert(host.to_string(), ip.octets());
                Ok(ip.octets())
            }
            Err(reason) => {
                crate::hilog::emit(&format!(
                    "N5c_ICE|stun-dns|host={host}|outcome=failed|reason={reason}"
                ));
                Err(ManagementError::Network(format!("stun-resolve: {reason}")))
            }
        }
    }

    /// The port [`Self::resolve_on_demand`] queries — see the `dns_port` field.
    #[cfg(not(test))]
    fn dns_port(&self) -> u16 {
        crate::protected_dns::DNS_PORT
    }

    #[cfg(test)]
    fn dns_port(&self) -> u16 {
        self.dns_port.load(Ordering::Relaxed)
    }

    /// Test seam: point the protected resolver at a fake DNS server (a host
    /// test cannot bind [`crate::protected_dns::DNS_PORT`]). Production code
    /// never calls this.
    #[cfg(test)]
    pub(crate) fn set_dns_port_for_test(&self, port: u16) {
        self.dns_port.store(port, Ordering::Relaxed);
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

/// Ceiling on ONE on-demand protected lookup on the gather path — upstream's
/// `dnsResolveTimeout = 30 * time.Second` (`client/internal/stdnet/stdnet.go:25`,
/// applied to every `LookupNetIP` at `:109-112`, hence to each gather's
/// `ResolveUDPAddr` at pion fork `gather.go:513` / `:585`). It is a ceiling,
/// not a budget: the protected supply's own 8 s request timeout
/// (`socket_supply.rs:19`) and the 1.5 s per-server query timeout
/// (`protected_dns.rs:18`) normally end a doomed lookup far sooner, exactly as
/// they do in the pre-resolved path.
pub const DNS_RESOLVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn dns_timeout_reason() -> String {
    format!("dns:timeout after {}s", DNS_RESOLVE_TIMEOUT.as_secs())
}

/// One protected A lookup, awaited by a SYNCHRONOUS gather worker: upstream's
/// gather goroutine blocking on `ResolveUDPAddr` (`gather.go:513`), with
/// upstream's bound. Returns the seam's own reason token on failure so the
/// caller logs the real cause.
fn protected_lookup(
    host: &str,
    supply: &Arc<crate::socket_supply::SocketSupply>,
    port: u16,
    runtime: &tokio::runtime::Handle,
) -> Result<std::net::Ipv4Addr, String> {
    let in_runtime = tokio::runtime::Handle::try_current().is_ok();
    let current_thread = runtime.runtime_flavor() == tokio::runtime::RuntimeFlavor::CurrentThread;
    if in_runtime || current_thread {
        // Called from inside a runtime (the deterministic gather executors run
        // the round inline on the pump's thread) or on a current-thread handle,
        // which cannot drive its own IO/timers from another thread: drive the
        // lookup on a private current-thread runtime on a scoped thread, so
        // neither the caller's thread nor its drivers are needed. Production is
        // never this branch.
        return std::thread::scope(|scope| {
            scope
                .spawn(|| private_lookup(host, supply, port))
                .join()
                .unwrap_or_else(|_| Err("dns:worker-panicked".to_string()))
        });
    }
    runtime
        .block_on(async {
            // The timeout is BUILT inside the runtime context: tokio 1.53
            // constructs its `Sleep` eagerly, and an argument-position
            // `timeout(..)` would be evaluated before `block_on` enters.
            tokio::time::timeout(
                DNS_RESOLVE_TIMEOUT,
                crate::protected_dns::resolve_on_port(host, supply, port),
            )
            .await
        })
        .map_err(|_| dns_timeout_reason())
        .and_then(|outcome| outcome.map_err(|e| e.to_string()))
}

fn private_lookup(
    host: &str,
    supply: &Arc<crate::socket_supply::SocketSupply>,
    port: u16,
) -> Result<std::net::Ipv4Addr, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("dns:runtime-unavailable: {e}"))?;
    runtime
        .block_on(async {
            tokio::time::timeout(
                DNS_RESOLVE_TIMEOUT,
                crate::protected_dns::resolve_on_port(host, supply, port),
            )
            .await
        })
        .map_err(|_| dns_timeout_reason())
        .and_then(|outcome| outcome.map_err(|e| e.to_string()))
}

impl UdpSocketSource for ProtectedUdpFdSource {
    /// Upstream shape (report §3/§5 F1): the gather path resolves the STUN
    /// hostname itself. [`Self::resolved`] is only a warm start — a hit uses
    /// it, a miss performs the protected lookup now (bounded by
    /// [`DNS_RESOLVE_TIMEOUT`], upstream's `dnsResolveTimeout`) and returns its
    /// real result. There is no "pending" state: nothing was ever in flight
    /// under the old sentinel, so a failure here is reported once, for this
    /// exchange, and the next gather tries again.
    fn resolve_ipv4(&self, host: &str) -> Result<[u8; 4], ManagementError> {
        if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() { return Ok(ip.octets()); }
        if let Some(ip) = self.resolved.lock().expect("stun dns cache").get(host).copied() {
            return Ok(ip);
        }
        self.resolve_on_demand(host)
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

/// Test-only seams shared by the in-crate test modules (`ice`, `peer_conn`) —
/// never compiled into the cdylib or the integration-test lib.
#[cfg(test)]
pub(crate) mod test_support {
    use crate::socket_supply::{Purpose, SocketSupply};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// A fake DNS server on an ephemeral port (a host test cannot bind
    /// [`crate::protected_dns::DNS_PORT`]): answers every A query with `answer`
    /// in the exact wire shape `protected_dns::response` validates, or — with
    /// `None` — reads and drops the query, i.e. a resolver that eats the whole
    /// 1.5 s query timeout (`protected_dns.rs:18`).
    pub(crate) fn fake_dns_server(runtime: &tokio::runtime::Handle, answer: Option<[u8; 4]>) -> u16 {
        let server = std::net::UdpSocket::bind("127.0.0.1:0").expect("fake dns bind");
        server.set_nonblocking(true).expect("nonblocking fake dns");
        let port = server.local_addr().expect("fake dns addr").port();
        runtime.spawn(async move {
            let server = tokio::net::UdpSocket::from_std(server).expect("async fake dns");
            let mut buf = [0u8; 512];
            loop {
                let Ok((n, peer)) = server.recv_from(&mut buf).await else { return };
                let Some(ip) = answer else { continue };
                let mut reply = buf[..n].to_vec();
                if reply.len() < 12 { continue; }
                reply[2] = 0x81; // QR=1, RD copied, RA=0 — the flags the parser accepts
                reply[3] = 0x80; // RCODE=0
                reply[7] = 1;    // ANCOUNT=1
                reply.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 1, 0, 4]);
                reply.extend_from_slice(&ip);
                let _ = server.send_to(&reply, peer).await;
            }
        });
        port
    }

    /// Shell-side provider for the protected DNS sockets the core asks for: a
    /// plain thread (the core waits on a socket it cannot provide itself),
    /// handing out a real AF_INET/SOCK_DGRAM socket — or rejecting while
    /// `reject` is set, exactly like the shell during a network transition.
    pub(crate) struct DnsSocketProvider {
        /// Requests seen (one per lookup attempt).
        pub(crate) seen: Arc<AtomicU64>,
        reject: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl DnsSocketProvider {
        pub(crate) fn start(supply: Arc<SocketSupply>) -> Self {
            let seen = Arc::new(AtomicU64::new(0));
            let reject = Arc::new(AtomicBool::new(false));
            let stop = Arc::new(AtomicBool::new(false));
            let thread = std::thread::spawn({
                let seen = seen.clone();
                let reject = reject.clone();
                let stop = stop.clone();
                move || {
                    while !stop.load(Ordering::Relaxed) {
                        let mut idle = true;
                        while let Some((id, purpose)) = supply.poll_request() {
                            idle = false;
                            if purpose != Purpose::Dns { continue; }
                            seen.fetch_add(1, Ordering::Relaxed);
                            if reject.load(Ordering::Relaxed) {
                                supply.reply(id, -1, "protect-failed");
                                continue;
                            }
                            let raw = unsafe { crate::sys::socket(crate::sys::AF_INET, crate::sys::SOCK_DGRAM, 0) };
                            if raw < 0 {
                                supply.reply(id, -1, "protect-failed");
                                continue;
                            }
                            let fd = unsafe { OwnedFd::from_raw_fd(raw) };
                            let _ = supply.reply(id, fd.as_raw_fd(), "protected");
                        }
                        if idle { std::thread::sleep(Duration::from_millis(1)); }
                    }
                }
            });
            DnsSocketProvider { seen, reject, stop, thread: Some(thread) }
        }

        pub(crate) fn set_reject(&self, value: bool) {
            self.reject.store(value, Ordering::Relaxed);
        }

        /// Bounded wait for `n` observed requests (the lookup itself runs on
        /// the gather worker / the caller, so the test thread only waits for an
        /// observable counter — never for a socket).
        pub(crate) fn wait_seen(&self, n: u64) -> bool {
            let deadline = Instant::now() + Duration::from_secs(5);
            while self.seen.load(Ordering::Relaxed) < n {
                if Instant::now() >= deadline { return false; }
                std::thread::sleep(Duration::from_millis(1));
            }
            true
        }
    }

    impl Drop for DnsSocketProvider {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() { let _ = thread.join(); }
        }
    }
}

#[cfg(test)]
mod feed_ownership_tests {
    use super::{ProtectedUdpFdSource, UdpSocketSource};
    use crate::sys;
    use std::os::fd::{FromRawFd, IntoRawFd};

    /// F1 + F2 (report §5/§6.2): a cache HIT is used, a MISS is resolved on
    /// demand (here: it fails with the seam's real reason, because the supply
    /// has no DNS server), and `refresh_stun_dns(&[])` — the
    /// `physical_network_changed` call with no new management address
    /// (`connector.rs:5932-5935`) — is a no-op that leaves a warm cache warm.
    /// Pre-fix this test pinned the opposite: an empty refresh wiped the entry
    /// and the next gather answered `stun:protected-dns-pending` for the rest
    /// of the process.
    #[test]
    fn empty_refresh_keeps_a_warm_cache_and_a_miss_is_a_real_error() {
        let source = ProtectedUdpFdSource::new_with_fd(-1).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        source.attach_dns(std::sync::Arc::new(crate::socket_supply::SocketSupply::new()), runtime.handle().clone());
        let err = source.resolve_ipv4("stun.example.invalid").unwrap_err().to_string();
        assert!(err.contains("dns:no-physical-server"), "a miss reports its real cause: {err}");
        assert!(!err.contains("pending"), "the terminal sentinel is gone: {err}");
        source.resolved.lock().unwrap().insert("stun.example.invalid".into(), [192, 0, 2, 9]);
        assert_eq!(source.resolve_ipv4("stun.example.invalid").unwrap(), [192, 0, 2, 9]);
        source.refresh_stun_dns(&[]);
        assert_eq!(
            source.resolve_ipv4("stun.example.invalid").unwrap(),
            [192, 0, 2, 9],
            "an empty refresh must not wipe the cache"
        );
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

/// F1/F2 (report §5, §6.1-6.5): the resolution behaviour of the protected STUN
/// hostname source — on-demand resolution on the gather path, a failure that
/// costs one exchange, a warm cache that is never wiped by an empty refresh.
#[cfg(test)]
mod stun_dns_tests {
    use super::test_support::{fake_dns_server, DnsSocketProvider};
    use super::{gather_srflx_round, ProtectedUdpFdSource, StunServer, UdpSocketSource};
    use crate::socket_supply::SocketSupply;
    use std::net::Ipv4Addr;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    /// A multi-thread runtime on purpose: the production gather worker is a
    /// plain thread (`peer_conn::ThreadGatherExec`) blocking on this handle,
    /// and the fake DNS server must make progress on the runtime's own workers.
    fn dns_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("dns test runtime")
    }

    fn server(host: &str) -> StunServer {
        StunServer { host: host.to_string(), port: 3478 }
    }

    /// Report §6.1/§6.6: with NO refresh event at all, a gather-path miss
    /// resolves on demand through the protected supply (upstream
    /// `gather.go:513`), and the cache is only a warm start — a hit costs no
    /// lookup. The pre-fix outcome (`Err("stun:protected-dns-pending")` with no
    /// request on the supply) is unreachable.
    #[test]
    fn a_gather_path_miss_resolves_on_demand_and_the_cache_is_only_a_warm_start() {
        let runtime = dns_runtime();
        let supply = Arc::new(SocketSupply::new());
        let port = fake_dns_server(runtime.handle(), Some([203, 0, 113, 9]));
        supply.set_dns_servers(vec![Ipv4Addr::LOCALHOST]);
        let provider = DnsSocketProvider::start(supply.clone());
        let source = ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source");
        source.attach_dns(supply.clone(), runtime.handle().clone());
        source.set_dns_port_for_test(port);

        // The gather path itself (the round calls this at `gather_srflx_round`).
        assert_eq!(source.resolve_ipv4("stun.example.invalid").unwrap(), [203, 0, 113, 9]);
        assert!(provider.wait_seen(1), "the miss must issue a protected DNS request");
        assert_eq!(provider.seen.load(Ordering::Relaxed), 1);

        // Warm start: the next gather is answered from the cache.
        assert_eq!(source.resolve_ipv4("stun.example.invalid").unwrap(), [203, 0, 113, 9]);
        assert_eq!(provider.seen.load(Ordering::Relaxed), 1, "a hit costs no lookup");

        // And a round with a fresh hostname asks again — resolution is per
        // gather, not per process.
        let mux = crate::ice_mux::IceMux::open_wildcard(0).expect("mux");
        let mux_port = mux.local_port();
        let round = gather_srflx_round(&[server("other.example.invalid")], &mux, &source, 50);
        assert_eq!(round.exchanges.len(), 1);
        assert!(
            round.exchanges[0].resolved.is_some(),
            "the round resolved the host on its own: {:?}",
            round.exchanges[0]
        );
        assert!(provider.wait_seen(2), "the round issued its own lookup");
    }

    /// Report §6.1/§6.3 (the PC capture, report §1.4): a refresh whose
    /// protected DNS socket the shell REJECTS is a per-exchange failure with
    /// the seam's real reason — not a state — so the very next gather for the
    /// same hostname resolves.
    #[test]
    fn a_rejected_lookup_is_not_terminal_and_the_next_gather_retries() {
        let runtime = dns_runtime();
        let supply = Arc::new(SocketSupply::new());
        let port = fake_dns_server(runtime.handle(), Some([198, 51, 100, 7]));
        supply.set_dns_servers(vec![Ipv4Addr::LOCALHOST]);
        let provider = DnsSocketProvider::start(supply.clone());
        provider.set_reject(true);
        let source = ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source");
        source.attach_dns(supply.clone(), runtime.handle().clone());
        source.set_dns_port_for_test(port);

        source.refresh_stun_dns(&[server("stun.example.invalid")]);
        assert!(provider.wait_seen(1), "the refresh did start a lookup");
        assert!(
            !source.resolved.lock().unwrap().contains_key("stun.example.invalid"),
            "a rejected lookup caches nothing"
        );

        // The failed lookup is a real error with the real reason ...
        let err = source.resolve_ipv4("rejected.example.invalid").unwrap_err().to_string();
        assert!(err.contains("socket-protect-failed"), "the seam's reason is surfaced: {err}");
        assert!(!err.contains("pending"), "the terminal sentinel is gone: {err}");
        assert!(
            !source.resolved.lock().unwrap().contains_key("rejected.example.invalid"),
            "a failure caches nothing"
        );

        // ... and it disables nothing: the next gather resolves.
        provider.set_reject(false);
        assert_eq!(source.resolve_ipv4("rejected.example.invalid").unwrap(), [198, 51, 100, 7]);
        assert!(
            provider.seen.load(Ordering::Relaxed) >= 3,
            "the retry issued a fresh lookup: {}",
            provider.seen.load(Ordering::Relaxed)
        );
    }

    /// Report §6.5: an empty shell-fed DNS server list is the seam's own
    /// `dns:no-physical-server` error (logged — see `protected_dns.rs`), never
    /// a "pending" state; and once the shell feeds a working list the NEXT
    /// GATHER succeeds with no refresh event and no timer in between (F1).
    #[test]
    fn an_empty_dns_server_list_is_a_real_error_and_a_later_feed_recovers() {
        let runtime = dns_runtime();
        let supply = Arc::new(SocketSupply::new());
        let port = fake_dns_server(runtime.handle(), Some([192, 0, 2, 30]));
        let provider = DnsSocketProvider::start(supply.clone());
        let source = ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source");
        source.attach_dns(supply.clone(), runtime.handle().clone());
        source.set_dns_port_for_test(port);

        let err = source.resolve_ipv4("stun.example.invalid").unwrap_err().to_string();
        assert!(err.contains("dns:no-physical-server"), "real reason: {err}");
        assert_eq!(provider.seen.load(Ordering::Relaxed), 0, "no socket request without a server");

        supply.set_dns_servers(vec![Ipv4Addr::LOCALHOST]);
        assert_eq!(source.resolve_ipv4("stun.example.invalid").unwrap(), [192, 0, 2, 30]);
        assert!(provider.wait_seen(1), "the next gather asked the supply itself");
    }

    /// Report §6.4: two refreshes in a row (the epoch guard may discard the
    /// first one's late result) leave NO host permanently missing — every miss
    /// re-resolves on demand.
    #[test]
    fn interleaved_refreshes_do_not_lose_a_host() {
        let runtime = dns_runtime();
        let supply = Arc::new(SocketSupply::new());
        let port = fake_dns_server(runtime.handle(), Some([203, 0, 113, 40]));
        supply.set_dns_servers(vec![Ipv4Addr::LOCALHOST]);
        let _provider = DnsSocketProvider::start(supply.clone());
        let source = ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source");
        source.attach_dns(supply.clone(), runtime.handle().clone());
        source.set_dns_port_for_test(port);

        source.refresh_stun_dns(&[server("first.example.invalid")]);
        source.refresh_stun_dns(&[server("second.example.invalid")]);
        assert_eq!(source.resolve_ipv4("first.example.invalid").unwrap(), [203, 0, 113, 40]);
        assert_eq!(source.resolve_ipv4("second.example.invalid").unwrap(), [203, 0, 113, 40]);
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
