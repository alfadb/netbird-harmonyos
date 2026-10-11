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
//! closes its original. A SUCCESSFUL interface round no longer closes its
//! bound socket: it leaves as a [`GatheredSocket`] whose `OwnedFd` the
//! caller adopts (the session's `add_local_candidate_bound`), so the
//! advertised host candidate AND the srflx mapping derived from it belong to
//! the SAME still-live socket. Every failure path drops the taken fd exactly
//! once. Each interface round
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

use std::collections::{HashMap, VecDeque};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

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
    /// overrides it so the orchestrator can size a gather round BEFORE
    /// consuming anything — one gather round takes exactly ONE socket per
    /// interface and KEEPS it (the bound socket is handed to the session via
    /// [`GatheredSocket`], not re-acquired), and a half-fed attempt burns the
    /// batch it was given (see `PeerIceOrchestrator::ensure_locals`).
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

/// One live host candidate: the protected socket the gather bound and used
/// for the interface's STUN exchanges, handed to the caller INSTEAD of being
/// closed.
///
/// Ownership contract (the 0052 defect this type exists for): a successful
/// interface round keeps its bound socket alive, so the advertised host
/// candidate and every srflx mapping derived from it refer to the SAME
/// socket. The pre-fix gather closed this fd before returning and
/// `ensure_locals` then bound a SECOND socket on a fresh ephemeral port —
/// the signaled host port belonged to a socket the STUN exchange never used,
/// and the srflx `raddr/rport` base pointed at a dead mapping.
///
/// Dropping `fd` closes it (exactly once). [`crate::ice_session::
/// IceSession::add_local_candidate_bound`] adopts it for the session's
/// lifetime after validating the bound address/port.
#[derive(Debug)]
pub struct GatheredSocket {
    /// The host candidate bound on `fd`: interface address + the
    /// `getsockname` port.
    pub cand: Candidate,
    /// The live, bound, `O_NONBLOCK` socket. Its number is
    /// `std::os::fd::AsRawFd::as_raw_fd` when a borrowed number is needed.
    pub fd: OwnedFd,
}


/// Everything one gather round produced. Partial success IS success: one
/// dead interface / server / provider slot must not sink the others
/// (upstream pion logs per-candidate failures and keeps gathering) — but
/// nothing is silently swallowed either, every failure is listed in
/// [`GatherResult::errors`].
///
/// Not `Clone`/`PartialEq`: [`GatheredSocket`] owns live sockets.
#[derive(Debug)]
pub struct GatherResult {
    /// Host candidates (one per usable interface address).
    pub host: Vec<Candidate>,
    /// srflx candidates; `raddr`/`rport` reference their base host
    /// candidate.
    pub srflx: Vec<Candidate>,
    /// (context, error) for every failed interface / server attempt,
    /// context = `"<iface>"` or `"<iface>/<host>:<port>"`.
    pub errors: Vec<(String, ManagementError)>,
    /// One live bound socket per successful interface round, in the same
    /// order as the `host` candidates it produced (`sockets[i].cand ==
    /// host[i]` for the host candidates; a round either yields both or
    /// neither). The caller MUST adopt or drop them — dropping closes the
    /// socket, which is exactly what the defect fix stops doing before the
    /// session has taken it over.
    pub sockets: Vec<GatheredSocket>,
}

/// The interface entries ONE gather round will actually visit: blacklist
/// prefix filter (same [`interface_allowed`] the gather applies) plus the
/// `(name, addr)` dedup for `getifaddrs`' repeated multi-flag entries, in
/// enumeration order.
///
/// Shared with the caller's socket budget on purpose: a gather round takes
/// exactly ONE socket per entry here (`gather_interface`) and KEEPS it (the
/// bound socket leaves as a [`GatheredSocket`]), so any caller that must
/// decide *before* consuming whether the protected-socket pool can pay for
/// the round has to count exactly what the gather will walk — two
/// implementations would drift.
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
    let all = allowed_ifaces(cfg, ifaces)?;
    let mut host = Vec::new();
    let mut srflx = Vec::new();
    let mut errors: Vec<(String, ManagementError)> = Vec::new();
    let mut sockets: Vec<GatheredSocket> = Vec::new();

    // Resolve STUN hostnames once per gather round (IPv4); a dead hostname
    // is a recorded per-server error, not a gather failure.
    let mut resolved: Vec<([u8; 4], u16, &StunServer)> = Vec::new();
    for server in cfg.servers {
        match socks.resolve_ipv4(&server.host) {
            Ok(ip) => resolved.push((ip, server.port, server)),
            Err(e) => errors.push((format!("-/{}", server.host), e)),
        }
    }

    for iface in all {
        match gather_interface(
            &iface,
            &resolved,
            socks,
            cfg.timeout_ms,
            &mut host,
            &mut srflx,
            &mut errors,
        ) {
            Ok(Some(sock)) => sockets.push(sock),
            // Per-interface failure already recorded by the callee; keep
            // gathering the remaining interfaces.
            Ok(None) => {}
            Err(e) => errors.push((iface.name.clone(), e)),
        }
    }

    if host.is_empty() && srflx.is_empty() && !errors.is_empty() {
        // No candidate at all: `sockets` is empty by construction (a
        // successful round always pushes its host candidate), so nothing
        // leaks on this return.
        return Err(errors.remove(0).1);
    }
    Ok(GatherResult { host, srflx, errors, sockets })
}

/// One interface = one fresh protected fd = one bound socket = one host
/// candidate + its STUN exchanges. On SUCCESS the bound socket is handed
/// back as a [`GatheredSocket`] (still live, so the advertised host port and
/// the srflx base are backed by it); every failure path drops the fd exactly
/// once and records an error — the provider's original number is never
/// touched (borrowed-number contract).
fn gather_interface(
    iface: &InterfaceAddr,
    servers: &[([u8; 4], u16, &StunServer)],
    socks: &dyn UdpSocketSource,
    timeout_ms: u64,
    host_out: &mut Vec<Candidate>,
    srflx_out: &mut Vec<Candidate>,
    errors_out: &mut Vec<(String, ManagementError)>,
) -> Result<Option<GatheredSocket>, ManagementError> {
    // STAGE 0 (official-alignment audit §2.7) — make the round measurable.
    // Every later stage's claim ("the tick is no longer bounded by STUN",
    // "one silent interface costs the schedule nothing") is a statement about
    // these two numbers, and they did not exist: a device gather produced one
    // `locals-gathered` line with no timing and no per-interface outcome. Both
    // lines are emitted here, where the round actually ends, in the existing
    // `N5c_ICE|<kind>|k=v|...` marker format. Observability only: nothing in
    // this function's decisions reads them.
    let round_start = sys::mono_ms();
    let srflx_before = srflx_out.len();
    // `gather-iface` = one interface round: how long it took, what it produced
    // and HOW it ended (`outcome=`), so a silent/blocked interface is a fact
    // instead of an inference (audit U1: "chba0's STUN silence is inferred,
    // not instrumented").
    macro_rules! iface_outcome {
        ($port:expr, $host:expr, $outcome:expr) => {
            crate::hilog::emit(&format!(
                "N5c_ICE|gather-iface|iface={}|addr={}|port={}|ms={}|host={}|srflx={}|errors={}|outcome={}",
                iface.name,
                std::net::Ipv4Addr::from(iface.addr),
                $port,
                sys::mono_ms().saturating_sub(round_start),
                $host,
                srflx_out.len().saturating_sub(srflx_before),
                errors_out.len(),
                $outcome,
            ))
        };
    }
    // `gather-stun` = one (interface, STUN server) exchange — upstream's own
    // unit of concurrency (fork `gather.go:491-513`) and of failure
    // (`gather.go:526-531` logs exactly one line per failed exchange).
    macro_rules! stun_outcome {
        ($server:expr, $ms:expr, $outcome:expr) => {
            crate::hilog::emit(&format!(
                "N5c_ICE|gather-stun|iface={}|server={}|ms={}|outcome={}",
                iface.name, $server, $ms, $outcome
            ))
        };
    }

    // The ONE owned copy for this round: syscalls go through its raw number,
    // and ownership either leaves via the returned `GatheredSocket` or is
    // dropped here (exactly one close).
    let owned = match socks.take_owned() {
        Ok(o) => o,
        Err(e) => {
            iface_outcome!(0, 0, "no-socket");
            return Err(seam_to_management(e));
        }
    };
    let dup = owned.as_raw_fd();
    let close_and_record = |owned: OwnedFd,
                            e: ManagementError,
                            errors_out: &mut Vec<(String, ManagementError)>| {
        crate::n2_socket_trace::closed(owned.as_raw_fd());
        drop(owned); // the single close for this failure path
        errors_out.push((iface.name.clone(), e));
    };

    if let Err(e) = set_nonblock(dup) {
        close_and_record(owned, seam_to_management(e), errors_out);
        iface_outcome!(0, 0, "nonblock-failed");
        return Ok(None);
    }
    // Bind to THE interface address (host candidate binding) with an
    // ephemeral port; getsockname is the port the host candidate carries
    // (the srflx base).
    let local = sys::sockaddr_in::new(iface.addr, 0);
    if unsafe { sys::bind(dup, &local, core::mem::size_of::<sys::sockaddr_in>() as u32) } == -1 {
        close_and_record(
            owned,
            ManagementError::Network(format!("stun-bind-failed (errno={})", sys::errno())),
            errors_out,
        );
        iface_outcome!(0, 0, "bind-failed");
        return Ok(None);
    }
    let mut name_addr = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut name_len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    if unsafe { sys::getsockname(dup, &mut name_addr, &mut name_len) } == -1 {
        close_and_record(
            owned,
            ManagementError::Network(format!("stun-getsockname-failed (errno={})", sys::errno())),
            errors_out,
        );
        iface_outcome!(0, 0, "getsockname-failed");
        return Ok(None);
    }
    let bound_port = u16::from_be(name_addr.sin_port);
    let base = Candidate::host_candidate(iface.addr, bound_port);
    host_out.push(base.clone());

    if servers.is_empty() {
        // No STUN server: the socket is STILL the live base of the host
        // candidate, so it must survive the gather (pre-fix it was closed
        // here, which is what made the signaled port dead).
        iface_outcome!(bound_port, 1, "no-stun-server");
        return Ok(Some(GatheredSocket { cand: base, fd: owned }));
    }

    // One transaction per server, all riding THIS interface's socket; the
    // dispatch loop below matches responses to servers by transaction id,
    // so interleaved answers can never be lost (single Outstanding per
    // socket would drop the other server's reply).
    let mut outstanding: Vec<(usize, TransactionId)> = Vec::new();
    let mut targets: Vec<(usize, sys::sockaddr_in, TransactionId)> = Vec::new();
    // STAGE 0: per-server send wall clock, so `gather-stun|ms=` reports the
    // exchange (send → reply/timeout) of THIS (interface, server) pair.
    let mut sent_at: Vec<u64> = vec![0; servers.len()];
    for (idx, (ip, port, server)) in servers.iter().enumerate() {
        let txn = match stun::random_transaction_id() {
            Ok(t) => t,
            Err(e) => {
                stun_outcome!(format!("{}:{}", server.host, server.port), 0, "txn-failed");
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
        if crate::n2_socket_trace::armed() {
            let remote = std::net::SocketAddrV4::new(std::net::Ipv4Addr::from(addr.sin_addr),
                u16::from_be(addr.sin_port));
            crate::n2_socket_trace::io_begin(dup, remote.into());
        }
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
                if crate::n2_socket_trace::armed() {
                    let remote = std::net::SocketAddrV4::new(std::net::Ipv4Addr::from(addr.sin_addr),
                        u16::from_be(addr.sin_port));
                    let txn_id: String = txn.0.iter().map(|b| format!("{b:02x}")).collect();
                    crate::n2_socket_trace::first_io(dup, remote.into(), "sent", &format!("stun_txn_{txn_id}"));
                }
                sent = true;
                break;
            }
            if sys::errno() != sys::EINTR {
                break;
            }
        }
        sent_at[*idx] = sys::mono_ms(); // STAGE 0: exchange clock starts at the send
        if !sent {
            if crate::n2_socket_trace::armed() {
                let remote = std::net::SocketAddrV4::new(std::net::Ipv4Addr::from(addr.sin_addr),
                    u16::from_be(addr.sin_port));
                crate::n2_socket_trace::first_io(dup, remote.into(), "failed", "stun_send");
            }
            let errno = sys::errno();
            outstanding.retain(|(i, _)| i != idx);
            stun_outcome!(format!("{}:{}", servers[*idx].2.host, servers[*idx].2.port), 0, "send-failed");
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
                stun_outcome!(
                    format!("{}:{}", servers[idx].2.host, servers[idx].2.port),
                    sys::mono_ms().saturating_sub(sent_at[idx]),
                    "poll-failed"
                );
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
        // N2 v4.1 P7: the one-shot STUN gather socket's outer-source receipt.
        // STUN servers are not WG peers, so `peer=` is `unknown` here by
        // construction (the address of the server we asked is a different
        // fact, already on the `N2_SOCK_STUN` receipt). Fixed `--ice-port`
        // mode never gathers, so this seam is reachable in ephemeral mode.
        if crate::n2_outer_receipt::armed() {
            crate::n2_outer_receipt::note_udp(
                crate::n2_socket_trace::local_addr(dup),
                std::net::SocketAddr::from((
                    std::net::Ipv4Addr::from(src.sin_addr),
                    u16::from_be(src.sin_port),
                )),
                datagram,
                "unknown",
            );
        }
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
        let ms = sys::mono_ms().saturating_sub(sent_at[idx]);
        match stun::parse_binding_response(datagram, &txn) {
            Ok(StunReply::Mapped { addr, port }) => {
                if crate::n2_socket_trace::armed() {
                    let txn_id: String = txn.0.iter().map(|b| format!("{b:02x}")).collect();
                    let local = crate::n2_socket_trace::local_addr(dup).map_or_else(|| "unknown".into(), |a| a.to_string());
                    crate::hilog::emit(&format!("N2_SOCK_STUN|dup_fd={dup}|txn_id={txn_id}|server={}:{}|local={local}|mapped={}:{}|mono_ms={}|diag=true|is_evidence=false",
                        std::net::Ipv4Addr::from(servers[idx].0), servers[idx].1,
                        std::net::Ipv4Addr::from(addr), port, sys::mono_ms()));
                }
                // STAGE 0: the srflx exchange's own timing + mapping.
                crate::hilog::emit(&format!(
                    "N5c_ICE|gather-stun|iface={}|server={}:{}|ms={ms}|outcome=mapped|mapped={}:{}",
                    iface.name,
                    server.host,
                    server.port,
                    std::net::Ipv4Addr::from(addr),
                    port
                ));
                srflx_out.push(Candidate::srflx_candidate(&base, addr, port));
            }
            Ok(StunReply::Error(code)) => {
                stun_outcome!(format!("{}:{}", server.host, server.port), ms, format!("error-code={code}"));
                errors_out.push((ctx, ManagementError::Server { status: code }));
            }
            Err(e) => {
                stun_outcome!(format!("{}:{}", server.host, server.port), ms, "unparsable");
                errors_out.push((ctx, e));
            }
        }
    }
    for (idx, _) in outstanding.drain(..) {
        let server = servers[idx].2;
        stun_outcome!(
            format!("{}:{}", server.host, server.port),
            sys::mono_ms().saturating_sub(sent_at[idx]),
            "timeout"
        );
        errors_out.push((
            format!("{}/{}:{}", iface.name, server.host, server.port),
            ManagementError::Timeout,
        ));
    }
    // STUN done (answers and/or recorded timeouts): the socket stays live
    // and leaves with the host candidate it backs.
    iface_outcome!(bound_port, 1, "ok");
    Ok(Some(GatheredSocket { cand: base, fd: owned }))
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
        allowed_ifaces, gather_candidates, interface_allowed, GatherConfig, InterfaceAddr,
        ProtectedUdpFdSource, StaticInterfaces, DEFAULT_INTERFACE_BLACKLIST,
        DEFAULT_STUN_TIMEOUT_MS,
    };
    use crate::sys;

    /// 0049 §5.1-4：设备 overlay 接口必须不进 host 候选。`vpn-tun` 曾不在
    /// 黑名单里 → 设备自己的 overlay 地址（100.108.144.165）被当第三个
    /// 「可用接口」：每 peer 多烧 2 枚 socket（gather 1 + 绑定长留 1），
    /// 并对外 advertise 一个隧道内候选。命名证据见
    /// [`DEFAULT_INTERFACE_BLACKLIST`] 的文档（ets 常量 + 设备
    /// `/proc/net/dev`），不是猜测。
    #[test]
    fn overlay_tun_interface_never_becomes_a_host_candidate() {
        // 前缀匹配语义（上游 filter.go:19-24）：`vpn-tun` 同时挡住
        // `vpn-tun0` 一类变体；物理接口不受影响。
        assert!(!interface_allowed("vpn-tun", &DEFAULT_INTERFACE_BLACKLIST));
        assert!(!interface_allowed("vpn-tun0", &DEFAULT_INTERFACE_BLACKLIST));
        assert!(interface_allowed("wlan0", &DEFAULT_INTERFACE_BLACKLIST));

        let ifaces = StaticInterfaces(vec![
            // 物理接口用本机可绑定的回环地址代位（gather 按接口地址 bind；
            // 这里只验证「谁进候选」，不验证具体网卡）。tun 项刻意也用**可
            // 绑定**地址（设备上 overlay 地址 100.108.144.165 在本机不可
            // 绑定），这样「它没进候选」只能归因于接口名黑名单，而不是
            // bind 失败。
            InterfaceAddr { name: "wlan0".into(), addr: [127, 0, 0, 1] },
            InterfaceAddr { name: "vpn-tun".into(), addr: [127, 0, 0, 3] },
        ]);
        // 预算两枚（每个接口一枚），只为证明 overlay 那枚不会被取走。
        let socks = ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source");
        for _ in 0..2 {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) }; // 源留住 dup；原 fd 归测试，喂完即关
        }
        let cfg = GatherConfig {
            blacklist: &DEFAULT_INTERFACE_BLACKLIST,
            servers: &[],
            timeout_ms: DEFAULT_STUN_TIMEOUT_MS,
        };
        assert_eq!(allowed_ifaces(&cfg, &ifaces).expect("enumerate").len(), 1);

        let got = gather_candidates(&cfg, &ifaces, &socks).expect("gather");
        assert_eq!(got.host.len(), 1, "only the physical interface may host: {:?}", got.host);
        assert_eq!(got.host[0].address, "127.0.0.1");
        assert!(
            got.host.iter().all(|c| c.address != "127.0.0.3"),
            "the tun interface's (overlay-range) address must never be advertised"
        );
        // 黑名单接口不消耗任何 socket（这正是 0049 每 peer 少烧 2 枚的来源）。
        assert_eq!(socks.taken(), 1, "blacklisted interface must not take a socket");
        assert_eq!(socks.pending(), 1);
    }

    /// 预算函数必须与 gather 真正走的接口清单逐项一致（否则「先算后取」
    /// 的守护会算错账）。
    #[test]
    fn allowed_ifaces_matches_the_gather_walk() {
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

        let socks = ProtectedUdpFdSource::new_with_fd(-1).expect("empty udp source");
        for _ in 0..2 {
            let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(fd >= 0, "socket() failed");
            socks.feed(fd).expect("feed");
            unsafe { sys::close(fd) };
        }
        let got = gather_candidates(&cfg, &ifaces, &socks).expect("gather");
        // gather 每接口恰好一枚（servers 为空时用完即关）→ 枚数 == 预算数。
        assert_eq!(socks.taken() as usize, listed.len());
        assert_eq!(got.host.len(), listed.len());
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
