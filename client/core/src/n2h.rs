// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # n2h — N2-H route-exclusion ISOLATION EVIDENCE tool (host-side)
//!
//! Turns "外层流量未入隧道" from an inference into OBSERVABLE evidence for one
//! connection session, per the cross-vendor T0 ruling (2026-09-14, gpt-5.6-sol
//! seat) and its draft clause **N2-H**: when per-socket bypass is unavailable
//! (on this platform `VpnConnection.protect(fd)` / `protectProcessNet()` are
//! both blocked by the non-requestable `ohos.permission.MANAGE_VPN` —
//! install error 9568289), a minimal endpoint-derived route exclusion may be
//! evaluated using PACKAGE-LEVEL NEGATIVE TUN EVIDENCE + a TUNNEL POSITIVE
//! CONTROL + ENDPOINT-SIDE DELIVERY PROOF + COUNTER RECONCILIATION.
//!
//! **Governance scope (binding):** this module is an evidence COLLECTOR, not
//! a policy change. It does not satisfy the original §二.4 per-socket-protect
//! clause (recorded as UNSAT, see [`UNSAT_NOTE`]); `n2h-pass` is NOT a pass of
//! the original N2 criteria, and N2-H itself only takes effect after explicit
//! user approval and a formal governance-document revision. See
//! `docs/n2h-isolation-evidence-notes.md`.
//!
//! ## What the evidence covers (T0 requirements → fields)
//!
//! 1. per-protocol unique probes (management / signal / STUN / TURN / relay /
//!    WG peer / DNS), unique five-tuple AND unique payload per connection and
//!    reconnect → [`Evidence::frozen_endpoints`] + [`Evidence::probes`];
//! 2. TUN package-level negative evidence: none of the outer probes' unique
//!    payload markers may appear in any parsed TUN frame →
//!    [`Evidence::tun_negative`];
//! 3. positive control: a probe aimed at the peer's OVERLAY address MUST be
//!    observed inside the tunnel → [`Evidence::tunnel_positive_control`]
//!    (a missing positive control forces `n2h-inconclusive`, never pass);
//! 4. endpoint-side proof: a sink / peer device must record RECEIVING each
//!    outer probe together with the physical source address →
//!    [`Evidence::endpoint_side`];
//! 5. counter reconciliation across the isolation window (device stats vs an
//!    independent drain of the TUN interface) → [`Evidence::counters`];
//! 6. post-revocation re-run: re-execute the tool after VPN teardown (the
//!    tool covers ONE window per run; the caller repeats it);
//! 7. route tables / `/proc/net/route` / config read-back / API return values
//!    are auxiliary ONLY — none of them can produce `n2h-pass` here.
//!
//! ## Isolation
//!
//! Host-side analysis tool, same standing as `host_sockets`: the device path
//! never calls into this module and the shipped cdylib behavior is unchanged;
//! the loopback driver exists so the full evidence pipeline is executable and
//! testable offline (two real [`crate::wg_device::WgDevice`] instances over
//! loopback + mock endpoint sinks; synthetic fixed test keys only). TUN here
//! is the socketpair stand-in — the frame parser is format-compatible with
//! the real device path (plain IPv4 frames in/out of [`crate::tun::TunFd`]).

use std::net::SocketAddr;
use std::time::SystemTime;

use crate::sys;
use crate::util::{hex_lower, jbool, jinum, jnum, jstr, json_escape};

// ---------------------------------------------------------------------------
// fixed vocabulary
// ---------------------------------------------------------------------------

/// Tool identity field in the evidence document.
pub const TOOL_NAME: &str = "n2h-isolation-evidence";
/// Evidence document schema (bump on breaking field changes).
pub const SCHEMA_VERSION: u64 = 1;

/// Verdict: the isolation claim is PROVEN for this session window.
pub const VERDICT_PASS: &str = "n2h-pass";
/// Verdict: the claim is FALSIFIED (an outer probe payload appeared in the
/// tunnel — route exclusion leaked).
pub const VERDICT_FAIL: &str = "n2h-fail";
/// Verdict: the claim could NOT be established (freeze gaps, missing positive
/// control, missing endpoint-side receipt, counter mismatch). Never a pass.
pub const VERDICT_INCONCLUSIVE: &str = "n2h-inconclusive";

/// The fixed UNSAT record. The original per-socket-protect obligation is NOT
/// met on this platform; route exclusion is a DIFFERENT mechanism awaiting
/// formal approval, and `n2h-pass` must never be read as the original N2 pass.
pub const UNSAT_NOTE: &str = "逐 socket protect: UNSAT/未满足（MANAGE_VPN 受限 + promise 不 settle）；替代: route-exclusion；非原 N2 判据 pass";

/// Fixed residual scope: what this tool does NOT cover (rendered into every
/// evidence document; extends the verdict's meaning, never shrinks it).
pub const RESIDUAL_SCOPE: &[&str] = &[
    "域名/CDN: 端点按冻结时刻的解析结果枚举; 域名重解析/CDN 换址后的新外层端点不在本证据内",
    "DNS 上游: DNS 查询的上游递归路径不受本工具观测",
    "动态候选: srflx/relayed (STUN/TURN 派生) 候选及其外层映射未在主机侧覆盖",
    "重连换址: 本证据只覆盖单次连接会话窗口; 撤销后复验与重连后的新端点须重新执行本工具",
    "非 LAN/NAT 拓扑: 主机侧 loopback/单机环境不代表 NAT/多宿主下的物理路径",
    "宽前缀旁路: 仅证明被枚举端点(按 /32 冻结)的零命中; 更宽路由前缀下的旁路不在覆盖内",
    "端点侧日志: 真实服务端的投递日志须运维侧提供; 主机侧只覆盖 sink 记录/对端设备计数",
    "IPv6: TUN 帧解析与端点枚举均为 IPv4-only",
];

/// How the TUN negative scan works (rendered into the evidence document so
/// the parsing method travels with the claim).
pub const TUN_SCAN_METHOD: &str = "IPv4 frame parse (version/IHL, proto, 5-tuple) + verbatim substring search of each probe's unique payload marker (N2H-<kind>-<nonce>, nonce from /dev/urandom) across the FULL frame bytes of every frame read from the TUN interface; any outer-probe marker hit falsifies the claim";

/// The counter reconciliation identity (device counter vs independent drain).
pub const RECONCILIATION_NOTE: &str = "delta.wg.rx_bytes_to_tun == delta.tun.delivered_bytes (device-side plaintext-to-TUN counter vs an independent byte count of frames drained from the TUN interface)";

// ---------------------------------------------------------------------------
// endpoint model
// ---------------------------------------------------------------------------

/// Outer endpoint families per the N2-H draft enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointKind {
    Management,
    Signal,
    Stun,
    Turn,
    Relay,
    WgPeer,
    Dns,
}

impl EndpointKind {
    /// Stable token used in JSON and in probe ids (`N2H-<kind>-<nonce>`).
    pub fn name(&self) -> &'static str {
        match self {
            EndpointKind::Management => "management",
            EndpointKind::Signal => "signal",
            EndpointKind::Stun => "stun",
            EndpointKind::Turn => "turn",
            EndpointKind::Relay => "relay",
            EndpointKind::WgPeer => "wg_peer",
            EndpointKind::Dns => "dns",
        }
    }

    /// Transport the outer probe uses against this endpoint family.
    ///
    /// T0 relay-increment ruling (2026-09-14, `grok-4.6` seat): the NetBird
    /// relay is **WSS over TCP** in the deployment we validate
    /// (`rels://home.alfadb.cn:28443`), so labelling `Relay` as UDP made the
    /// frozen set unable to falsify an outer hairpin. QUIC-only relays would
    /// be UDP and must be derived from the URI scheme at freeze time; until
    /// that exists, the probe default is the production transport (tcp).
    pub fn proto(&self) -> &'static str {
        match self {
            EndpointKind::Management | EndpointKind::Signal | EndpointKind::Relay => "tcp",
            _ => "udp",
        }
    }

    /// Kinds that MUST be frozen for the evidence to be complete. `Turn` and
    /// `Relay` are optional ONLY with an explicit `absent_reason` (a
    /// deployment may genuinely have none — the absence is then recorded and
    /// auditable); every other kind always exists in a NetBird session.
    pub fn required(&self) -> bool {
        !matches!(self, EndpointKind::Turn | EndpointKind::Relay)
    }

    pub const ALL: [EndpointKind; 7] = [
        EndpointKind::Management,
        EndpointKind::Signal,
        EndpointKind::Stun,
        EndpointKind::Turn,
        EndpointKind::Relay,
        EndpointKind::WgPeer,
        EndpointKind::Dns,
    ];

    pub fn from_name(s: &str) -> Option<EndpointKind> {
        EndpointKind::ALL.iter().find(|k| k.name() == s).copied()
    }
}

/// One frozen outer endpoint (or the explicit, reasoned absence of one).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrozenEndpoint {
    pub kind: Option<EndpointKind>,
    pub present: bool,
    pub host: String,
    pub port: u16,
    /// "tcp" | "udp" (empty when absent).
    pub proto: String,
    /// Where the endpoint came from: network-map / config / cli / harness sink.
    pub source: String,
    /// Resolved `ip:port` (the route exclusion installs per resolved /32).
    pub resolved: String,
    /// Why the endpoint is absent (REQUIRED when `present == false`).
    pub absent_reason: String,
    pub frozen_at_unix: i64,
    pub frozen_at_mono: u64,
}

impl FrozenEndpoint {
    fn to_json(&self) -> String {
        let kind = self.kind.map(|k| k.name()).unwrap_or("unknown");
        format!(
            "{{{},{},{},{},{},{},{},{},{},{}}}",
            jstr("kind", kind),
            jbool("present", self.present),
            jstr("host", &self.host),
            jinum("port", self.port as i64),
            jstr("proto", &self.proto),
            jstr("source", &self.source),
            jstr("resolved", &self.resolved),
            jstr("absent_reason", &self.absent_reason),
            jinum("frozen_at_unix", self.frozen_at_unix),
            jinum("frozen_at_mono", self.frozen_at_mono as i64),
        )
    }
}

// ---------------------------------------------------------------------------
// probes
// ---------------------------------------------------------------------------

/// Unique on-the-wire payload marker of one probe. ASCII markers are the
/// probe id itself (`N2H-<kind>-<nonce>`); the STUN probe's unique payload is
/// its 12-byte transaction id (the Binding Request wire format is fixed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireMarker {
    Ascii(String),
    StunTxid([u8; 12]),
}

impl WireMarker {
    /// Verbatim substring match over the full frame/datagram bytes.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        let marker: &[u8] = match self {
            WireMarker::Ascii(s) => s.as_bytes(),
            WireMarker::StunTxid(t) => t,
        };
        bytes.len() >= marker.len() && bytes.windows(marker.len()).any(|w| w == marker)
    }

    /// JSON shape: `{"format":"ascii"|"hex","value":"..."}`.
    fn to_json(&self) -> String {
        match self {
            WireMarker::Ascii(s) => {
                format!("{{{},{}}}", jstr("format", "ascii"), jstr("value", s))
            }
            WireMarker::StunTxid(t) => format!(
                "{{{},{}}}",
                jstr("format", "hex"),
                jstr("value", &hex_lower(t))
            ),
        }
    }
}

/// Protocol five-tuple of one probe (unique per probe: every probe leaves
/// from its OWN ephemeral-bound socket).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FiveTuple {
    /// "udp" | "tcp" | "ipv4" (the positive control is a raw IPv4 frame).
    pub proto: String,
    pub src: String,
    pub dst: String,
}

impl FiveTuple {
    fn to_json(&self) -> String {
        format!(
            "{{{},{},{}}}",
            jstr("proto", &self.proto),
            jstr("src", &self.src),
            jstr("dst", &self.dst),
        )
    }
}

/// One outer-path probe (expected path: OUTER — must NOT enter the tunnel).
#[derive(Debug, Clone, Default)]
pub struct OuterProbe {
    pub probe_id: String,
    pub kind: Option<EndpointKind>,
    pub tuple: FiveTuple,
    /// Always "outer" for [`OuterProbe`].
    pub expected_path: &'static str,
    pub sent: bool,
    pub sent_at_unix: i64,
    pub sent_at_mono: u64,
    pub marker: Option<WireMarker>,
    pub payload_len: usize,
}

impl OuterProbe {
    fn to_json(&self) -> String {
        let kind = self.kind.map(|k| k.name()).unwrap_or("unknown");
        let marker = self.marker.as_ref().map(|m| m.to_json()).unwrap_or_else(|| "null".into());
        format!(
            "{{{},{},{},{},{},{},{},{},{}}}",
            jstr("probe_id", &self.probe_id),
            jstr("kind", kind),
            format!("\"tuple\":{}", self.tuple.to_json()),
            jstr("expected_path", self.expected_path),
            jbool("sent", self.sent),
            jinum("sent_at_unix", self.sent_at_unix),
            jinum("sent_at_mono", self.sent_at_mono as i64),
            format!("\"payload_marker\":{marker}"),
            jnum("payload_len", self.payload_len as u64),
        )
    }
}

/// One observed tunnel frame hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunHit {
    pub probe_id: String,
    /// Which TUN end observed it ("a" / "b" in the loopback driver).
    pub side: String,
    pub src: [u8; 4],
    pub dst: [u8; 4],
    pub sport: u16,
    pub dport: u16,
    pub frame_len: usize,
    pub observed_at_mono: u64,
}

impl TunHit {
    fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{}}}",
            jstr("probe_id", &self.probe_id),
            jstr("side", &self.side),
            jstr("src", &ipv4_str(self.src)),
            jstr("dst", &ipv4_str(self.dst)),
            jinum("sport", self.sport as i64),
            jinum("dport", self.dport as i64),
            jinum("frame_len", self.frame_len as i64),
        )
    }
}

fn ipv4_str(a: [u8; 4]) -> String {
    format!("{}.{}.{}.{}", a[0], a[1], a[2], a[3])
}

/// TUN package-level negative scan result.
#[derive(Debug, Clone, Default)]
pub struct TunScanResult {
    /// TRUE when ANY outer-probe marker was found in any scanned frame.
    pub hit: bool,
    pub frames_scanned: u64,
    pub hits: Vec<TunHit>,
    pub sides: Vec<String>,
}

impl TunScanResult {
    fn to_json(&self) -> String {
        let hits: Vec<String> = self.hits.iter().map(|h| h.to_json()).collect();
        let sides: Vec<String> = self.sides.iter().map(|s| format!("\"{s}\"")).collect();
        format!(
            "{{{},{},{},{},{},{}}}",
            jbool("hit", self.hit),
            jnum("frames_scanned", self.frames_scanned),
            format!("\"hits\":[{}]", hits.join(",")),
            format!("\"scanned_sides\":[{}]", sides.join(",")),
            format!("\"method\":\"{}\"", json_escape(TUN_SCAN_METHOD)),
            format!(
                "\"criteria\":\"{}\"",
                json_escape("每个 outer 探针的唯一载荷标识不得出现在任何 TUN 帧中; 任一命中即 n2h-fail")
            ),
        )
    }
}

/// The tunnel positive control: a probe aimed at the peer's OVERLAY address
/// that MUST be observed inside the tunnel.
#[derive(Debug, Clone, Default)]
pub struct PosctlProbe {
    pub probe_id: String,
    /// Direction label ("a2b" / "b2a" in the loopback driver).
    pub direction: String,
    pub tuple: FiveTuple,
    /// Always "tunnel" for [`PosctlProbe`].
    pub expected_path: &'static str,
    pub sent: bool,
    pub sent_at_mono: u64,
    pub observed: Option<TunHit>,
    pub note: String,
}

impl PosctlProbe {
    fn to_json(&self) -> String {
        let observed = self
            .observed
            .as_ref()
            .map(|h| h.to_json())
            .unwrap_or_else(|| "null".into());
        format!(
            "{{{},{},{},{},{},{},{},{}}}",
            jstr("probe_id", &self.probe_id),
            jstr("direction", &self.direction),
            format!("\"tuple\":{}", self.tuple.to_json()),
            jstr("expected_path", self.expected_path),
            jbool("sent", self.sent),
            jinum("sent_at_mono", self.sent_at_mono as i64),
            format!("\"observed_in_tunnel\":{observed}"),
            jstr("note", &self.note),
        )
    }
}

/// Positive-control evidence block.
#[derive(Debug, Clone, Default)]
pub struct PosctlEvidence {
    pub probes: Vec<PosctlProbe>,
}

impl PosctlEvidence {
    /// TRUE iff at least one positive control exists, every one is sent, and
    /// every sent one was observed inside the tunnel.
    pub fn observed_in_tunnel(&self) -> bool {
        !self.probes.is_empty()
            && self.probes.iter().all(|p| p.sent && p.observed.is_some())
    }

    fn to_json(&self) -> String {
        let probes: Vec<String> = self.probes.iter().map(|p| p.to_json()).collect();
        format!(
            "{{{},{},{}}}",
            jbool("observed_in_tunnel", self.observed_in_tunnel()),
            jstr("expected_path", "tunnel"),
            format!("\"probes\":[{}]", probes.join(",")),
        )
    }
}

// ---------------------------------------------------------------------------
// endpoint-side receipts
// ---------------------------------------------------------------------------

/// Endpoint-side proof that one outer probe was RECEIVED (with the physical
/// source address the endpoint saw).
#[derive(Debug, Clone, Default)]
pub struct EndpointReceipt {
    pub probe_id: String,
    pub kind: String,
    pub observed: bool,
    /// Physical source address as recorded at the endpoint (or, for the
    /// peer-device counter method, as bound on the probe socket — the
    /// `physical_src_source` field says which).
    pub physical_src: String,
    /// "endpoint-reported" (sink recvfrom / TCP peer_addr / STUN XOR-MAPPED)
    /// or "probe-socket-getsockname" (counter method; NOT endpoint-reported).
    pub physical_src_source: String,
    pub at_unix: i64,
    /// Observation method: sink-record / stun-binding-response / peer-device-offpath-drop.
    pub method: String,
    pub detail: String,
}

impl EndpointReceipt {
    fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{},{}}}",
            jstr("probe_id", &self.probe_id),
            jstr("kind", &self.kind),
            jbool("observed", self.observed),
            jstr("physical_src", &self.physical_src),
            jstr("physical_src_source", &self.physical_src_source),
            jinum("at_unix", self.at_unix),
            jstr("method", &self.method),
            jstr("detail", &self.detail),
        )
    }
}

// ---------------------------------------------------------------------------
// counters
// ---------------------------------------------------------------------------

/// One snapshot of the tunnel-interface counters (device stats + an
/// independent count of frames drained from the TUN interface).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IfaceSnapshot {
    pub at_mono: u64,
    pub at_unix: i64,
    // REAL WgDeviceStats fields (verbatim semantics, wg_device.rs)
    pub wg_tx_packets: u64,
    pub wg_tx_bytes: u64,
    pub wg_rx_packets: u64,
    pub wg_rx_bytes_to_tun: u64,
    pub wg_handshake_initiations: u64,
    pub wg_no_route_drops: u64,
    pub wg_unknown_peer_drops: u64,
    pub wg_decrypt_errors: u64,
    pub wg_send_errors: u64,
    pub wg_tun_write_errors: u64,
    // TUN-stand-in side (collector-counted, independent of the device)
    pub tun_injected_frames: u64,
    pub tun_delivered_frames: u64,
    pub tun_delivered_bytes: u64,
}

impl IfaceSnapshot {
    fn minus(&self, before: &IfaceSnapshot) -> IfaceSnapshot {
        IfaceSnapshot {
            at_mono: self.at_mono.saturating_sub(before.at_mono),
            at_unix: self.at_unix,
            wg_tx_packets: self.wg_tx_packets.saturating_sub(before.wg_tx_packets),
            wg_tx_bytes: self.wg_tx_bytes.saturating_sub(before.wg_tx_bytes),
            wg_rx_packets: self.wg_rx_packets.saturating_sub(before.wg_rx_packets),
            wg_rx_bytes_to_tun: self.wg_rx_bytes_to_tun.saturating_sub(before.wg_rx_bytes_to_tun),
            wg_handshake_initiations: self
                .wg_handshake_initiations
                .saturating_sub(before.wg_handshake_initiations),
            wg_no_route_drops: self.wg_no_route_drops.saturating_sub(before.wg_no_route_drops),
            wg_unknown_peer_drops: self
                .wg_unknown_peer_drops
                .saturating_sub(before.wg_unknown_peer_drops),
            wg_decrypt_errors: self.wg_decrypt_errors.saturating_sub(before.wg_decrypt_errors),
            wg_send_errors: self.wg_send_errors.saturating_sub(before.wg_send_errors),
            wg_tun_write_errors: self.wg_tun_write_errors.saturating_sub(before.wg_tun_write_errors),
            tun_injected_frames: self.tun_injected_frames.saturating_sub(before.tun_injected_frames),
            tun_delivered_frames: self
                .tun_delivered_frames
                .saturating_sub(before.tun_delivered_frames),
            tun_delivered_bytes: self
                .tun_delivered_bytes
                .saturating_sub(before.tun_delivered_bytes),
        }
    }

    fn to_json(&self) -> String {
        let n = |k: &str, v: u64| format!("\"{k}\":{v}");
        format!(
            "{{{},\"at_unix\":{},\"wg\":{{{},{},{},{},{},{},{},{},{},{}}},\"tun\":{{{},{},{}}}}}",
            n("at_mono", self.at_mono),
            self.at_unix,
            n("tx_packets", self.wg_tx_packets),
            n("tx_bytes", self.wg_tx_bytes),
            n("rx_packets", self.wg_rx_packets),
            n("rx_bytes_to_tun", self.wg_rx_bytes_to_tun),
            n("handshake_initiations", self.wg_handshake_initiations),
            n("no_route_drops", self.wg_no_route_drops),
            n("unknown_peer_drops", self.wg_unknown_peer_drops),
            n("decrypt_errors", self.wg_decrypt_errors),
            n("send_errors", self.wg_send_errors),
            n("tun_write_errors", self.wg_tun_write_errors),
            n("injected_frames", self.tun_injected_frames),
            n("delivered_frames", self.tun_delivered_frames),
            n("delivered_bytes", self.tun_delivered_bytes),
        )
    }
}

/// One isolation window: snapshots before/after with the delta.
#[derive(Debug, Clone, Default)]
pub struct WindowCounters {
    /// Which interface instance ("a" / "b" in the loopback driver).
    pub side: String,
    pub before: IfaceSnapshot,
    pub after: IfaceSnapshot,
}

impl WindowCounters {
    pub fn delta(&self) -> IfaceSnapshot {
        self.after.minus(&self.before)
    }

    /// The reconciliation identity: the device's plaintext-to-TUN byte
    /// counter must equal the independently drained TUN byte count.
    pub fn reconciled(&self) -> bool {
        let d = self.delta();
        d.wg_rx_bytes_to_tun == d.tun_delivered_bytes
    }

    fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{}}}",
            jstr("side", &self.side),
            format!("\"before\":{}", self.before.to_json()),
            format!("\"after\":{}", self.after.to_json()),
            format!("\"delta\":{}", self.delta().to_json()),
            jbool("reconciled", self.reconciled()),
            jstr("reconciliation", RECONCILIATION_NOTE),
        )
    }
}

// ---------------------------------------------------------------------------
// TUN frame parsing (format-compatible with the device TunFd path)
// ---------------------------------------------------------------------------

/// Parsed view of one TUN frame (borrowing). IPv4-only (the platform path is
/// IPv4; see `residual_scope` for the v6 exclusion).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ipv4FrameView<'a> {
    pub src: [u8; 4],
    pub dst: [u8; 4],
    /// IP protocol number (17 = UDP, 6 = TCP).
    pub proto: u8,
    pub sport: u16,
    pub dport: u16,
    /// L4 payload (after the UDP header / TCP data offset).
    pub payload: &'a [u8],
}

/// Parse one IPv4 TUN frame. `None` for short / non-IPv4 / truncated frames —
/// the parser never panics on hostile input.
pub fn parse_ipv4_frame(frame: &[u8]) -> Option<Ipv4FrameView<'_>> {
    if frame.len() < 20 {
        return None;
    }
    let v_ihl = frame[0];
    if v_ihl >> 4 != 4 {
        return None;
    }
    let ihl = (v_ihl & 0x0f) as usize * 4;
    if ihl < 20 || frame.len() < ihl {
        return None;
    }
    // honor the declared total length but never read past the buffer
    let total = u16::from_be_bytes([frame[2], frame[3]]) as usize;
    let end = total.clamp(ihl, frame.len());
    let proto = frame[9];
    let src = [frame[12], frame[13], frame[14], frame[15]];
    let dst = [frame[16], frame[17], frame[18], frame[19]];
    let l4 = &frame[ihl..end];
    let (sport, dport, payload) = match proto {
        17 => {
            if l4.len() < 8 {
                (0u16, 0u16, &l4[l4.len()..])
            } else {
                (
                    u16::from_be_bytes([l4[0], l4[1]]),
                    u16::from_be_bytes([l4[2], l4[3]]),
                    &l4[8..],
                )
            }
        }
        6 => {
            if l4.len() < 20 {
                (0u16, 0u16, &l4[l4.len()..])
            } else {
                let off = (l4[12] >> 4) as usize * 4;
                if off < 20 || off > l4.len() {
                    (0u16, 0u16, &l4[l4.len()..])
                } else {
                    (
                        u16::from_be_bytes([l4[0], l4[1]]),
                        u16::from_be_bytes([l4[2], l4[3]]),
                        &l4[off..],
                    )
                }
            }
        }
        _ => (0u16, 0u16, l4),
    };
    Some(Ipv4FrameView { src, dst, proto, sport, dport, payload })
}

/// Verbatim substring test: does `frame` contain `marker` anywhere?
pub fn frame_contains_marker(frame: &[u8], marker: &[u8]) -> bool {
    !marker.is_empty() && frame.len() >= marker.len() && frame.windows(marker.len()).any(|w| w == marker)
}

/// Streaming TUN-frame scanner: feed every frame read from the TUN interface;
/// collect per-probe hits. Outer-probe hits are the NEGATIVE evidence
/// violation; positive-control hits are the EXPECTED observation.
#[derive(Debug, Default)]
pub struct TunnelScanner {
    entries: Vec<ScanEntry>,
    frames_scanned: u64,
    sides: Vec<String>,
}

#[derive(Debug)]
struct ScanEntry {
    probe_id: String,
    marker: WireMarker,
    /// false = outer probe (a hit is a VIOLATION), true = positive control.
    positive: bool,
    hits: Vec<TunHit>,
}

impl TunnelScanner {
    /// Register one probe marker for scanning.
    pub fn register(&mut self, probe_id: &str, marker: WireMarker, positive: bool) {
        self.entries.push(ScanEntry {
            probe_id: probe_id.to_string(),
            marker,
            positive,
            hits: Vec::new(),
        });
    }

    /// Scan one frame read from the TUN interface end `side`.
    pub fn scan(&mut self, side: &str, frame: &[u8], now_mono: u64) {
        self.frames_scanned += 1;
        if std::env::var_os("N2H_TRACE").is_some() {
            let t = parse_ipv4_frame(frame);
            eprintln!(
                "[n2h-trace] scan side={side} len={} tuple={:?} payload_prefix={:?}",
                frame.len(),
                t.as_ref().map(|v| (v.src, v.dst, v.sport, v.dport)),
                frame.get(28..core::cmp::min(frame.len(), 60))
                    .map(|b| String::from_utf8_lossy(b).to_string()),
            );
        }
        if !self.sides.iter().any(|s| s == side) {
            self.sides.push(side.to_string());
        }
        for e in &mut self.entries {
            if e.marker.matches(frame) {
                let (src, dst, sport, dport) = match parse_ipv4_frame(frame) {
                    Some(v) => (v.src, v.dst, v.sport, v.dport),
                    None => ([0, 0, 0, 0], [0, 0, 0, 0], 0, 0),
                };
                e.hits.push(TunHit {
                    probe_id: e.probe_id.clone(),
                    side: side.to_string(),
                    src,
                    dst,
                    sport,
                    dport,
                    frame_len: frame.len(),
                    observed_at_mono: now_mono,
                });
            }
        }
    }

    /// Hits recorded for one probe id.
    pub fn hits_for(&self, probe_id: &str) -> Vec<TunHit> {
        self.entries
            .iter()
            .filter(|e| e.probe_id == probe_id)
            .flat_map(|e| e.hits.iter().cloned())
            .collect()
    }

    /// Violations: outer-probe markers seen in the tunnel.
    pub fn outer_hits(&self) -> Vec<TunHit> {
        self.entries
            .iter()
            .filter(|e| !e.positive)
            .flat_map(|e| e.hits.iter().cloned())
            .collect()
    }

    pub fn frames_scanned(&self) -> u64 {
        self.frames_scanned
    }

    pub fn sides(&self) -> Vec<String> {
        self.sides.clone()
    }
}

// ---------------------------------------------------------------------------
// ids / time
// ---------------------------------------------------------------------------

/// Fresh nonce from `/dev/urandom` (24 hex chars) — the same fail-closed
/// entropy discipline as `stun::random_transaction_id`.
pub fn fresh_nonce_hex() -> Result<String, String> {
    let id = crate::stun::random_transaction_id()
        .map_err(|e| format!("nonce entropy unavailable: {e:?}"))?;
    Ok(hex_lower(&id.0))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Build one unique probe id: `N2H-<kind>-<nonce>`.
pub fn probe_id(kind: &str, nonce: &str) -> String {
    format!("N2H-{kind}-{nonce}")
}

// ---------------------------------------------------------------------------
// evidence document + verdict
// ---------------------------------------------------------------------------

/// The full evidence document for ONE connection-session isolation window.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    pub mode: String,
    /// Session descriptor (scenario label; no secret material).
    pub session: String,
    pub generated_unix: i64,
    pub started_mono: u64,
    pub finished_mono: u64,
    pub frozen_endpoints: Vec<FrozenEndpoint>,
    pub probes: Vec<OuterProbe>,
    pub tunnel_positive_control: PosctlEvidence,
    pub tun_negative: TunScanResult,
    pub endpoint_side: Vec<EndpointReceipt>,
    pub counters: Vec<WindowCounters>,
    /// Computed by [`Evidence::judge`] at build time; kept in the document.
    pub verdict: String,
    pub reasons: Vec<String>,
    pub notes: Vec<String>,
}

impl Evidence {
    /// THE verdict engine. Order matters:
    /// 1. any outer-probe marker in the tunnel → `n2h-fail` (direct
    ///    falsification beats everything else);
    /// 2. freeze gaps (missing required endpoint / unexplained optional
    ///    absence) → inconclusive;
    /// 3. probe-send gaps → inconclusive;
    /// 4. positive control not sent / not observed → inconclusive (NO pass
    ///    without a positive control — rule out a blind observation path);
    /// 5. endpoint-side receipt missing → inconclusive (outer-path delivery
    ///    unproven);
    /// 6. counter reconciliation mismatch → inconclusive;
    /// 7. otherwise → `n2h-pass`.
    pub fn judge(&self) -> (String, Vec<String>) {
        let mut reasons: Vec<String> = Vec::new();

        // 1. negative evidence violation = fail, first and unconditional
        if !self.tun_negative.hits.is_empty() {
            for h in &self.tun_negative.hits {
                reasons.push(format!(
                    "tunnel negative evidence violated: outer probe {} observed in tunnel (side={}, {}:{} -> {}:{}, frame_len={})",
                    h.probe_id, h.side, ipv4_str(h.src), h.sport, ipv4_str(h.dst), h.dport, h.frame_len
                ));
            }
            return (VERDICT_FAIL.to_string(), reasons);
        }

        // 2. freeze completeness
        for kind in EndpointKind::ALL {
            let entry = self.frozen_endpoints.iter().find(|f| f.kind == Some(kind));
            match entry {
                None => reasons.push(format!(
                    "endpoint-freeze incomplete: kind '{}' not frozen at all",
                    kind.name()
                )),
                Some(f) if !f.present => {
                    if f.absent_reason.trim().is_empty() {
                        reasons.push(format!(
                            "endpoint-freeze incomplete: kind '{}' absent without recorded reason",
                            kind.name()
                        ));
                    } else if kind.required() {
                        reasons.push(format!(
                            "endpoint-freeze incomplete: required kind '{}' missing ({})",
                            kind.name(),
                            f.absent_reason
                        ));
                    }
                    // optional kind + reason → recorded, auditable, no gap
                }
                _ => {}
            }
        }

        // 3. every present endpoint must have its probe SENT
        for f in self.frozen_endpoints.iter().filter(|f| f.present) {
            let kind = f.kind.map(|k| k.name()).unwrap_or("unknown");
            match self.probes.iter().find(|p| p.kind == f.kind) {
                None => reasons.push(format!("probe missing: no outer probe for kind '{kind}'")),
                Some(p) if !p.sent => {
                    reasons.push(format!("probe not sent: {} ({kind})", p.probe_id))
                }
                _ => {}
            }
        }

        // 4. positive control
        if self.tunnel_positive_control.probes.is_empty() {
            reasons.push(
                "positive control missing: no tunnel positive-control probe exists — \
                 the tunnel observation path itself is unproven"
                    .to_string(),
            );
        } else {
            for p in &self.tunnel_positive_control.probes {
                if !p.sent {
                    reasons.push(format!(
                        "positive control {} not sent — no pass without a positive control",
                        p.probe_id
                    ));
                } else if p.observed.is_none() {
                    reasons.push(format!(
                        "positive control {} never observed in the tunnel — the tunnel \
                         observation path is unproven; a clean negative scan is not interpretable",
                        p.probe_id
                    ));
                }
            }
        }

        // 5. endpoint-side receipts for every sent probe (a probe may carry
        // several receipt records — e.g. an initial "response not yet seen"
        // entry later confirmed; ANY observed record counts)
        for p in self.probes.iter().filter(|p| p.sent) {
            let kind = p.kind.map(|k| k.name()).unwrap_or("unknown");
            let observed = self
                .endpoint_side
                .iter()
                .any(|r| r.probe_id == p.probe_id && r.observed);
            if !observed {
                reasons.push(format!(
                    "endpoint-side receipt missing for {} ({kind}): outer-path delivery unproven",
                    p.probe_id
                ));
            }
        }

        // 6. counter reconciliation
        if self.counters.is_empty() {
            reasons.push(
                "counter reconciliation missing: no interface-counter window was collected"
                    .to_string(),
            );
        }
        for w in &self.counters {
            if !w.reconciled() {
                let d = w.delta();
                reasons.push(format!(
                    "counter reconciliation mismatch on side '{}': wg.rx_bytes_to_tun delta {} != tun delivered bytes {}",
                    w.side, d.wg_rx_bytes_to_tun, d.tun_delivered_bytes
                ));
            }
        }

        if reasons.is_empty() {
            (VERDICT_PASS.to_string(), reasons)
        } else {
            (VERDICT_INCONCLUSIVE.to_string(), reasons)
        }
    }

    /// Full JSON document (`n2h-isolation-evidence.json` shape). Parseable by
    /// the crate's strict reader; contains NO secret material (probe ids,
    /// addresses, counters, public tokens only).
    pub fn to_json(&self) -> String {
        let frozen: Vec<String> = self.frozen_endpoints.iter().map(|f| f.to_json()).collect();
        let probes: Vec<String> = self.probes.iter().map(|p| p.to_json()).collect();
        let receipts: Vec<String> = self.endpoint_side.iter().map(|r| r.to_json()).collect();
        let counters: Vec<String> = self.counters.iter().map(|c| c.to_json()).collect();
        let reasons: Vec<String> = self
            .reasons
            .iter()
            .map(|r| format!("\"{}\"", json_escape(r)))
            .collect();
        let notes: Vec<String> = self
            .notes
            .iter()
            .map(|r| format!("\"{}\"", json_escape(r)))
            .collect();
        let residual: Vec<String> = RESIDUAL_SCOPE
            .iter()
            .map(|r| format!("\"{}\"", json_escape(r)))
            .collect();
        format!(
            "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
            jstr("tool", TOOL_NAME),
            jnum("schema", SCHEMA_VERSION),
            jstr("mode", &self.mode),
            jstr("session", &self.session),
            jinum("generated_unix", self.generated_unix),
            jinum("started_mono", self.started_mono as i64),
            jinum("finished_mono", self.finished_mono as i64),
            format!("\"frozen_endpoints\":[{}]", frozen.join(",")),
            format!("\"probes\":[{}]", probes.join(",")),
            format!("\"tunnel_positive_control\":{}", self.tunnel_positive_control.to_json()),
            format!("\"tun_negative\":{}", self.tun_negative.to_json()),
            format!("\"endpoint_side\":[{}]", receipts.join(",")),
            format!("\"counters\":[{}]", counters.join(",")),
            jstr("verdict", &self.verdict),
            format!("\"reasons\":[{}]", reasons.join(",")),
            format!(
                "\"notes\":[{}],\"residual_scope\":[{}],{}",
                notes.join(","),
                residual.join(","),
                jstr("unsat_note", UNSAT_NOTE),
            ),
        )
    }
}

// ---------------------------------------------------------------------------
// endpoint sinks (host-side mock endpoints for the offline driver)
// ---------------------------------------------------------------------------

/// One datagram/bytes record captured by a sink.
#[derive(Debug, Clone)]
pub struct SinkRecord {
    pub payload: Vec<u8>,
    pub src: SocketAddr,
    pub at_unix: i64,
}

/// UDP sink; `stun_reply` makes it answer Binding Requests with a real
/// XOR-MAPPED-ADDRESS success response (the same codec the ICE gather uses).
pub struct UdpSink {
    socket: std::net::UdpSocket,
    stun_reply: bool,
    records: Vec<SinkRecord>,
}

impl UdpSink {
    pub fn bind(stun_reply: bool) -> Result<UdpSink, String> {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0")
            .map_err(|e| format!("udp sink bind: {e}"))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| format!("udp sink nonblock: {e}"))?;
        Ok(UdpSink { socket, stun_reply, records: Vec::new() })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, String> {
        self.socket.local_addr().map_err(|e| format!("udp sink addr: {e}"))
    }

    /// Drain every pending datagram (non-blocking), recording and — for the
    /// STUN sink — answering each Binding Request.
    pub fn drain(&mut self) {
        let mut buf = [0u8; 2048];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, src)) => {
                    let payload = buf[..n].to_vec();
                    if self.stun_reply && n >= 20 && payload[0..2] == crate::stun::BINDING_REQUEST.to_be_bytes() {
                        let mut txid = [0u8; 12];
                        txid.copy_from_slice(&payload[8..20]);
                        let resp = build_binding_response(&txid, src);
                        let _ = self.socket.send_to(&resp, src);
                    }
                    self.records.push(SinkRecord { payload, src, at_unix: unix_now() });
                }
                Err(_) => break,
            }
        }
    }

    /// Whether any recorded datagram matches `marker`.
    pub fn observed(&self, marker: &WireMarker) -> Option<&SinkRecord> {
        self.records.iter().find(|r| marker.matches(&r.payload))
    }
}

/// TCP sink (accept + read-to-EOF, non-blocking).
pub struct TcpSink {
    listener: std::net::TcpListener,
    records: Vec<SinkRecord>,
}

impl TcpSink {
    pub fn bind() -> Result<TcpSink, String> {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| format!("tcp sink bind: {e}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("tcp sink nonblock: {e}"))?;
        Ok(TcpSink { listener, records: Vec::new() })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, String> {
        self.listener.local_addr().map_err(|e| format!("tcp sink addr: {e}"))
    }

    /// Accept every pending connection and read each to EOF (bounded).
    pub fn drain(&mut self) {
        loop {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    let _ = stream.set_nonblocking(true);
                    let mut payload = Vec::new();
                    let mut spins = 0usize;
                    let mut buf = [0u8; 2048];
                    use std::io::Read;
                    let mut stream = stream;
                    loop {
                        match stream.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                payload.extend_from_slice(&buf[..n]);
                                if payload.len() > 64 * 1024 {
                                    break;
                                }
                            }
                            Err(_) => {
                                spins += 1;
                                if spins > 32 {
                                    break;
                                }
                                std::thread::sleep(std::time::Duration::from_millis(2));
                            }
                        }
                    }
                    self.records.push(SinkRecord { payload, src: peer, at_unix: unix_now() });
                }
                Err(_) => break,
            }
        }
    }

    pub fn observed(&self, marker: &WireMarker) -> Option<&SinkRecord> {
        self.records.iter().find(|r| marker.matches(&r.payload))
    }
}

/// Build a minimal Binding Success Response carrying XOR-MAPPED-ADDRESS
/// (mirror of the stun.rs test fixture math; the probe side parses it back
/// with the REAL `stun::parse_binding_response`).
fn build_binding_response(txid: &[u8; 12], mapped: SocketAddr) -> Vec<u8> {
    let cookie = crate::stun::MAGIC_COOKIE;
    let (ip, port) = match mapped {
        SocketAddr::V4(v4) => (*v4.ip(), v4.port()),
        _ => ([0, 0, 0, 0].into(), 0), // IPv4-only environment
    };
    let xor_port = port ^ (cookie >> 16) as u16;
    let xor_ip = [
        ip.octets()[0] ^ (cookie >> 24) as u8,
        ip.octets()[1] ^ (cookie >> 16) as u8,
        ip.octets()[2] ^ (cookie >> 8) as u8,
        ip.octets()[3] ^ cookie as u8,
    ];
    let mut out = Vec::with_capacity(32);
    out.extend_from_slice(&crate::stun::BINDING_SUCCESS.to_be_bytes());
    out.extend_from_slice(&12u16.to_be_bytes()); // one 12-byte attribute body
    out.extend_from_slice(&cookie.to_be_bytes());
    out.extend_from_slice(txid);
    out.extend_from_slice(&crate::stun::ATTR_XOR_MAPPED.to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes());
    out.push(0); // reserved
    out.push(0x01); // IPv4
    out.extend_from_slice(&xor_port.to_be_bytes());
    out.extend_from_slice(&xor_ip);
    out
}

// ---------------------------------------------------------------------------
// probe senders
// ---------------------------------------------------------------------------

/// Send one ASCII-marker UDP probe from a FRESH ephemeral socket.
fn send_udp_probe(dst: SocketAddr, marker: &str) -> Result<(FiveTuple, usize), String> {
    let sock = std::net::UdpSocket::bind("127.0.0.1:0")
        .map_err(|e| format!("probe socket bind: {e}"))?;
    let src = sock.local_addr().map_err(|e| format!("probe socket addr: {e}"))?;
    let n = sock
        .send_to(marker.as_bytes(), dst)
        .map_err(|e| format!("probe send: {e}"))?;
    Ok((FiveTuple { proto: "udp".into(), src: src.to_string(), dst: dst.to_string() }, n))
}

/// Send one ASCII-marker TCP probe (connect + write + shutdown).
fn send_tcp_probe(dst: SocketAddr, marker: &str) -> Result<(FiveTuple, usize), String> {
    use std::io::Write;
    let stream = std::net::TcpStream::connect_timeout(&dst, std::time::Duration::from_secs(2))
        .map_err(|e| format!("probe connect: {e}"))?;
    let src = stream.local_addr().map_err(|e| format!("probe addr: {e}"))?;
    let mut s = stream;
    s.write_all(marker.as_bytes()).map_err(|e| format!("probe write: {e}"))?;
    let _ = s.shutdown(std::net::Shutdown::Write);
    Ok((FiveTuple { proto: "tcp".into(), src: src.to_string(), dst: dst.to_string() }, marker.len()))
}

/// Send one STUN Binding probe; wait up to `wait_ms` for the matched
/// response and return the endpoint-reported physical mapping.
fn send_stun_probe(
    dst: SocketAddr,
    txid: [u8; 12],
    wait_ms: u64,
) -> Result<(FiveTuple, Option<([u8; 4], u16)>), String> {
    use crate::stun::{build_binding_request, parse_binding_response, TransactionId};
    // Wildcard bind, NOT 127.0.0.1 (regression found in device-validation
    // run 2): a loopback-bound probe socket makes every send to a real,
    // non-loopback outer endpoint fail with EINVAL ("stun probe send:
    // Invalid argument") because the kernel rejects a source address that is
    // invalid for the route. The wildcard lets the kernel pick a valid source.
    let sock = std::net::UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("probe socket bind: {e}"))?;
    let src = sock.local_addr().map_err(|e| format!("probe socket addr: {e}"))?;
    let _ = sock.set_read_timeout(Some(std::time::Duration::from_millis(wait_ms.max(1))));
    let req = build_binding_request(TransactionId(txid));
    sock.send_to(&req, dst).map_err(|e| format!("stun probe send: {e}"))?;
    let mut buf = [0u8; 2048];
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
    let mut mapped = None;
    while std::time::Instant::now() < deadline {
        match sock.recv_from(&mut buf) {
            Ok((n, _from)) => {
                if let Ok(crate::stun::StunReply::Mapped { addr, port }) =
                    parse_binding_response(&buf[..n], &TransactionId(txid))
                {
                    mapped = Some((addr, port));
                    break;
                }
                // mismatched datagram: keep waiting until the deadline
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }
    Ok((
        FiveTuple { proto: "udp".into(), src: src.to_string(), dst: dst.to_string() },
        mapped,
    ))
}

// ---------------------------------------------------------------------------
// the offline dual-instance loopback driver
// ---------------------------------------------------------------------------

/// Fault-injection / scenario switches (also the CLI's `--fault-*` flags).
/// Every fault is RECORDED in the evidence (notes) — the document never
/// pretends the fault did not happen.
#[derive(Debug, Clone, Default)]
pub struct LoopbackOpts {
    /// Simulate a freeze failure for one endpoint kind (recorded as an
    /// absent-with-reason entry; a required kind forces `n2h-inconclusive`
    /// and the session is NOT established — fail-closed).
    pub omit_kind: Option<EndpointKind>,
    /// 反例 A: replay one outer probe's exact payload into the tunnel with
    /// the peer's overlay address as dst (a genuine route leak) — the tool
    /// MUST catch it and return `n2h-fail`.
    pub leak_outer_probe_into_tunnel: bool,
    /// 反例 B: route the positive control OUTSIDE the peer's allowed_ips
    /// (sent, but it can never traverse — simulated tunnel loss) — the tool
    /// MUST return `n2h-inconclusive`.
    pub suppress_positive_control: bool,
}

const VPN_A: [u8; 4] = [10, 77, 0, 1];
const VPN_B: [u8; 4] = [10, 77, 0, 2];
const POSCTL_SPORT: u16 = 40000;
const POSCTL_DPORT: u16 = 40001;
/// Handshake / observation deadline (poll loop with 10 ms beats — never a
/// long sleep as the primary mechanism).
const SESSION_DEADLINE_MS: u64 = 4_000;
const OBSERVE_DEADLINE_MS: u64 = 3_000;
/// Consecutive 10 ms pumping rounds after the observation goal first holds,
/// so in-flight frames/sink records land inside the window before it closes.
const OBSERVE_SETTLE_ROUNDS: u32 = 15;

/// Run the FULL evidence pipeline against a dual-instance loopback session:
/// two real WG devices (synthetic fixed test keys), mock endpoint sinks for
/// every outer family, freeze → probes → positive controls → scan →
/// receipts → counters → verdict.
pub fn run_loopback_check(opts: &LoopbackOpts) -> Result<Evidence, String> {
    let started_mono = sys::mono_ms();
    let mut notes: Vec<String> = Vec::new();
    let mut evidence = Evidence {
        mode: "loopback".into(),
        session: "dual-instance loopback WG pair (synthetic fixed test keys); TUN = socketpair stand-in"
            .into(),
        generated_unix: unix_now(),
        started_mono,
        ..Evidence::default()
    };

    // ---- endpoint sinks (mock "servers", loopback ephemeral) ----
    let mut mgmt_sink = TcpSink::bind()?;
    let mut sig_sink = TcpSink::bind()?;
    let mut stun_sink = UdpSink::bind(true)?;
    let mut turn_sink = UdpSink::bind(false)?;
    // Relay rides WSS/TCP in the validated deployment (T0 relay ruling
    // 2026-09-14), so the loopback sink must accept TCP — an UDP sink here
    // made the relay probe panic against a datagram socket.
    let mut relay_sink = TcpSink::bind()?;
    let mut dns_sink = UdpSink::bind(false)?;

    // ---- WG pair: two real devices, loopback outer sockets, TUN stand-ins ----
    let mut bag = crate::host_sockets::FdBag::new();
    let fa = crate::host_sockets::open_udp_ephemeral().map_err(|e| format!("wg socket a: {e}"))?;
    bag.keep(fa);
    let fb = crate::host_sockets::open_udp_ephemeral().map_err(|e| format!("wg socket b: {e}"))?;
    bag.keep(fb);
    let port_a = crate::host_sockets::udp_bound_port(fa).map_err(|e| format!("port a: {e}"))?;
    let port_b = crate::host_sockets::udp_bound_port(fb).map_err(|e| format!("port b: {e}"))?;
    let (tun_a, hand_a) =
        crate::host_sockets::open_tun_standby_pair().map_err(|e| format!("tun a: {e}"))?;
    bag.keep(tun_a);
    bag.keep(hand_a);
    let (tun_b, hand_b) =
        crate::host_sockets::open_tun_standby_pair().map_err(|e| format!("tun b: {e}"))?;
    bag.keep(tun_b);
    bag.keep(hand_b);

    // synthetic FIXED test keys (credential discipline: never real secrets)
    const SECRET_A: [u8; 32] = [0xA5; 32];
    const SECRET_B: [u8; 32] = [0xB6; 32];
    let b64 = |bytes: &[u8]| crate::util::base64(bytes);
    let pub_of = |secret: &[u8; 32]| -> String {
        let k = boringtun::ffi::x25519_public_key(boringtun::ffi::x25519_key { key: *secret });
        crate::util::base64(&k.key)
    };
    let pub_a = pub_of(&SECRET_A);
    let pub_b = pub_of(&SECRET_B);

    let mut cfg_a = crate::wg_device::WgDeviceConfig::new(b64(&SECRET_A));
    cfg_a.hs_retry_ms = 100;
    cfg_a.hs_deadline_ms = 3_000;
    cfg_a.keepalive_ms = u64::MAX / 2;
    cfg_a.session_max_ms = u64::MAX / 2;
    let mut cfg_b = cfg_a.clone();
    cfg_b.local_secret_b64 = b64(&SECRET_B);

    let tun_a_fd = crate::tun::TunFd::dup_from_raw(tun_a).map_err(|e| format!("tun dup a: {e:?}"))?;
    let tun_b_fd = crate::tun::TunFd::dup_from_raw(tun_b).map_err(|e| format!("tun dup b: {e:?}"))?;
    let mut dev_a = crate::wg_device::WgDevice::adopt(cfg_a, fa, tun_a_fd)
        .map_err(|e| format!("adopt a: {e}"))?;
    let mut dev_b = crate::wg_device::WgDevice::adopt(cfg_b, fb, tun_b_fd)
        .map_err(|e| format!("adopt b: {e}"))?;
    dev_a
        .set_peers(&[crate::wg_device::WgPeerSpec::new(pub_b.clone(), vec![(VPN_B, 32)])])
        .map_err(|e| format!("peers a: {e}"))?;
    dev_b
        .set_peers(&[crate::wg_device::WgPeerSpec::new(pub_a.clone(), vec![(VPN_A, 32)])])
        .map_err(|e| format!("peers b: {e}"))?;

    // ---- FREEZE (before the session exists — N2-H ordering) ----
    let now_u = unix_now();
    let now_m = sys::mono_ms();
    let freeze = |kind: EndpointKind, host: &str, port: u16, source: &str,
                      opts: &LoopbackOpts, evidence: &mut Evidence| {
        let omitted = opts.omit_kind == Some(kind);
        evidence.frozen_endpoints.push(FrozenEndpoint {
            kind: Some(kind),
            present: !omitted,
            host: if omitted { String::new() } else { host.to_string() },
            port: if omitted { 0 } else { port },
            proto: if omitted { String::new() } else { kind.proto().to_string() },
            source: if omitted { String::new() } else { source.to_string() },
            resolved: if omitted { String::new() } else { format!("{host}:{port}") },
            absent_reason: if omitted {
                "fault-injected: endpoint not obtained at freeze time (simulated freeze failure)".into()
            } else {
                String::new()
            },
            frozen_at_unix: now_u,
            frozen_at_mono: now_m,
        });
    };
    freeze(EndpointKind::Management, &mgmt_sink.local_addr()?.ip().to_string(), mgmt_sink.local_addr()?.port(), "loopback-mock-sink", opts, &mut evidence);
    freeze(EndpointKind::Signal, &sig_sink.local_addr()?.ip().to_string(), sig_sink.local_addr()?.port(), "loopback-mock-sink", opts, &mut evidence);
    freeze(EndpointKind::Stun, &stun_sink.local_addr()?.ip().to_string(), stun_sink.local_addr()?.port(), "loopback-mock-sink", opts, &mut evidence);
    freeze(EndpointKind::Turn, &turn_sink.local_addr()?.ip().to_string(), turn_sink.local_addr()?.port(), "loopback-mock-sink", opts, &mut evidence);
    freeze(EndpointKind::Relay, &relay_sink.local_addr()?.ip().to_string(), relay_sink.local_addr()?.port(), "loopback-mock-sink", opts, &mut evidence);
    freeze(EndpointKind::Dns, &dns_sink.local_addr()?.ip().to_string(), dns_sink.local_addr()?.port(), "loopback-mock-sink", opts, &mut evidence);
    freeze(EndpointKind::WgPeer, "127.0.0.1", port_b, "wg-device-outer-socket", opts, &mut evidence);
    let freeze_complete = evidence
        .frozen_endpoints
        .iter()
        .all(|f| f.present || (!f.kind.map(|k| k.required()).unwrap_or(true) && !f.absent_reason.is_empty()));

    // ---- outer probes (unique five-tuple AND unique payload each) ----
    let mut scanner = TunnelScanner::default();
    let send_probe = |kind: EndpointKind, dst: SocketAddr, evidence: &mut Evidence,
                          scanner: &mut TunnelScanner| -> Result<(), String> {
        let nonce = fresh_nonce_hex()?;
        let id = probe_id(kind.name(), &nonce);
        let (tuple, len, marker) = if kind == EndpointKind::Stun {
            let txid = crate::stun::random_transaction_id()
                .map_err(|e| format!("stun probe entropy: {e:?}"))?;
            let id = probe_id(kind.name(), &hex_lower(&txid.0));
            let (tuple, mapped) = send_stun_probe(dst, txid.0, 400)?;
            let marker = WireMarker::StunTxid(txid.0);
            // endpoint-side evidence: txid-matched Binding Response carrying
            // the endpoint-reported physical source mapping
            let receipt = EndpointReceipt {
                probe_id: id.clone(),
                kind: kind.name().to_string(),
                observed: mapped.is_some(),
                physical_src: mapped
                    .map(|(a, p)| format!("{}:{}", ipv4_str(a), p))
                    .unwrap_or_default(),
                physical_src_source: "endpoint-reported".into(),
                at_unix: unix_now(),
                method: "stun-binding-response".into(),
                detail: if mapped.is_some() {
                    "txid-matched Binding Success Response (XOR-MAPPED-ADDRESS = physical source as seen by the endpoint)".into()
                } else {
                    "no txid-matched Binding Response within the probe wait window (record kept; a later sink record may still confirm)".into()
                },
            };
            evidence.endpoint_side.push(receipt);
            scanner.register(&id, marker.clone(), false);
            evidence.probes.push(OuterProbe {
                probe_id: id,
                kind: Some(kind),
                tuple,
                expected_path: "outer",
                sent: true,
                sent_at_unix: unix_now(),
                sent_at_mono: sys::mono_ms(),
                marker: Some(marker),
                payload_len: 20,
            });
            return Ok(());
        } else {
            let (tuple, len) = match kind.proto() {
                "tcp" => send_tcp_probe(dst, &id)?,
                _ => send_udp_probe(dst, &id)?,
            };
            (tuple, len, WireMarker::Ascii(id.clone()))
        };
        scanner.register(&id, marker.clone(), false);
        evidence.probes.push(OuterProbe {
            probe_id: id,
            kind: Some(kind),
            tuple,
            expected_path: "outer",
            sent: true,
            sent_at_unix: unix_now(),
            sent_at_mono: sys::mono_ms(),
            marker: Some(marker),
            payload_len: len,
        });
        Ok(())
    };

    // probe-send failures are recorded as not-sent probes (fail-closed), never swallowed
    let mut try_send = |kind: EndpointKind, dst: SocketAddr, evidence: &mut Evidence,
                        scanner: &mut TunnelScanner| {
        let nonce = fresh_nonce_hex().unwrap_or_else(|_| "000000000000000000000000".into());
        let id = probe_id(kind.name(), &nonce);
        match send_probe(kind, dst, evidence, scanner) {
            Ok(()) => {}
            Err(e) => {
                notes.push(format!("probe send failure for {kind:?}: {e}"));
                evidence.endpoint_side.push(EndpointReceipt {
                    probe_id: id.clone(),
                    kind: kind.name().to_string(),
                    observed: false,
                    method: "none".into(),
                    ..EndpointReceipt::default()
                });
                evidence.probes.push(OuterProbe {
                    probe_id: id,
                    kind: Some(kind),
                    expected_path: "outer",
                    sent: false,
                    ..OuterProbe::default()
                });
            }
        }
    };

    if evidence.frozen_endpoints.iter().find(|f| f.kind == Some(EndpointKind::Management)).map(|f| f.present) == Some(true) {
        try_send(EndpointKind::Management, mgmt_sink.local_addr()?, &mut evidence, &mut scanner);
    }
    if evidence.frozen_endpoints.iter().find(|f| f.kind == Some(EndpointKind::Signal)).map(|f| f.present) == Some(true) {
        try_send(EndpointKind::Signal, sig_sink.local_addr()?, &mut evidence, &mut scanner);
    }
    if evidence.frozen_endpoints.iter().find(|f| f.kind == Some(EndpointKind::Stun)).map(|f| f.present) == Some(true) {
        try_send(EndpointKind::Stun, stun_sink.local_addr()?, &mut evidence, &mut scanner);
    }
    if evidence.frozen_endpoints.iter().find(|f| f.kind == Some(EndpointKind::Turn)).map(|f| f.present) == Some(true) {
        try_send(EndpointKind::Turn, turn_sink.local_addr()?, &mut evidence, &mut scanner);
    }
    if evidence.frozen_endpoints.iter().find(|f| f.kind == Some(EndpointKind::Relay)).map(|f| f.present) == Some(true) {
        try_send(EndpointKind::Relay, relay_sink.local_addr()?, &mut evidence, &mut scanner);
    }
    if evidence.frozen_endpoints.iter().find(|f| f.kind == Some(EndpointKind::Dns)).map(|f| f.present) == Some(true) {
        try_send(EndpointKind::Dns, dns_sink.local_addr()?, &mut evidence, &mut scanner);
    }

    // ---- counters: the isolation window opens right BEFORE the outer
    // probes fly (every probe effect — including the off-path drop the WG
    // peer probe causes at the peer device — stays inside the window) ----
    let mut tun_injected = 0u64;
    let mut delivered = [(0u64, 0u64); 2]; // (frames, bytes) per side a/b
    let before_a = snapshot_a(&dev_a, tun_injected, delivered[0]);
    let before_b = snapshot_b(&dev_b, tun_injected, delivered[1]);

    // wg-peer outer probe (from a fresh socket → off-path at B's demux)
    if evidence.frozen_endpoints.iter().find(|f| f.kind == Some(EndpointKind::WgPeer)).map(|f| f.present) == Some(true) {
        let nonce = fresh_nonce_hex()?;
        let id = probe_id("wg_peer", &nonce);
        let dst = SocketAddr::from(([127, 0, 0, 1], port_b));
        match send_udp_probe(dst, &id) {
            Ok((tuple, len)) => {
                let marker = WireMarker::Ascii(id.clone());
                scanner.register(&id, marker.clone(), false);
                evidence.endpoint_side.push(EndpointReceipt {
                    probe_id: id.clone(),
                    kind: "wg_peer".into(),
                    observed: false, // confirmed later from B's device counters
                    physical_src: tuple.src.clone(),
                    physical_src_source: "probe-socket-getsockname".into(),
                    at_unix: unix_now(),
                    method: "peer-device-offpath-drop".into(),
                    detail: format!(
                        "datagram sent to the frozen WG peer outer endpoint {tuple:?}; \
                         receipt = the peer device's unknown_peer_drops delta (observed below)"
                    ),
                });
                evidence.probes.push(OuterProbe {
                    probe_id: id,
                    kind: Some(EndpointKind::WgPeer),
                    tuple,
                    expected_path: "outer",
                    sent: true,
                    sent_at_unix: unix_now(),
                    sent_at_mono: sys::mono_ms(),
                    marker: Some(marker),
                    payload_len: len,
                });
            }
            Err(e) => {
                notes.push(format!("probe send failure for wg_peer: {e}"));
                evidence.probes.push(OuterProbe {
                    probe_id: id,
                    kind: Some(EndpointKind::WgPeer),
                    expected_path: "outer",
                    sent: false,
                    ..OuterProbe::default()
                });
            }
        }
    }

    // ---- establish the session ONLY on a complete freeze (N2-H fail-closed) ----
    let mut session_established = false;
    if freeze_complete {
        dev_a
            .set_endpoint(&pub_b, [127, 0, 0, 1], port_b, sys::mono_ms())
            .map_err(|e| format!("endpoint a: {e}"))?;
        dev_b
            .set_endpoint(&pub_a, [127, 0, 0, 1], port_a, sys::mono_ms())
            .map_err(|e| format!("endpoint b: {e}"))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(SESSION_DEADLINE_MS);
        while std::time::Instant::now() < deadline {
            let now = sys::mono_ms();
            dev_a.service_tun(now);
            dev_a.service_udp(now);
            dev_a.tick(now);
            dev_b.service_tun(now);
            dev_b.service_udp(now);
            dev_b.tick(now);
            if dev_a.tunnel_ready() && dev_b.tunnel_ready() {
                session_established = true;
                break;
            }
            sys::sleep_ms(10);
        }
        if !session_established {
            notes.push("session NOT established within the deadline (fail-closed: no positive control possible)".into());
        }
    } else {
        notes.push("freeze incomplete — session NOT established (fail-closed per N2-H draft)".into());
    }

    // ---- positive controls + (fault A) the leak ----
    let mut posctl: Vec<PosctlProbe> = Vec::new();
    if session_established {
        let directions = [
            ("a2b", hand_a, VPN_A, VPN_B, "b"),
            ("b2a", hand_b, VPN_B, VPN_A, "a"),
        ];
        for (label, hand, src, dst, observed_side) in directions {
            let nonce = fresh_nonce_hex()?;
            let id = probe_id("posctl", &nonce);
            let dst_ip = if opts.suppress_positive_control && label == "a2b" {
                [10, 77, 0, 9] // OUTSIDE allowed_ips: sent, can never traverse
            } else {
                dst
            };
            let payload = format!("{id}|positive-control");
            let frame = crate::host_sockets::build_ipv4_udp_packet(
                src,
                dst_ip,
                POSCTL_SPORT,
                POSCTL_DPORT,
                payload.as_bytes(),
            );
            let (n, _e) = sys::write_fd(hand, &frame);
            let sent = n > 0;
            if sent {
                tun_injected += 1;
            }
            if opts.suppress_positive_control && label == "a2b" {
                notes.push(
                    "fault-injected: positive control a2b routed outside allowed_ips (simulated tunnel loss)".into(),
                );
            }
            scanner.register(&id, WireMarker::Ascii(id.clone()), true);
            posctl.push(PosctlProbe {
                probe_id: id,
                direction: label.to_string(),
                tuple: FiveTuple {
                    proto: "ipv4".into(),
                    src: format!("{}:{POSCTL_SPORT}", ipv4_str(src)),
                    dst: format!("{}:{POSCTL_DPORT}", ipv4_str(dst_ip)),
                },
                expected_path: "tunnel",
                sent,
                sent_at_mono: sys::mono_ms(),
                observed: None,
                note: if opts.suppress_positive_control && label == "a2b" {
                    "fault-injected: routed outside allowed_ips (no_route drop expected)".into()
                } else {
                    String::new()
                },
                ..PosctlProbe::default()
            });
            let _ = observed_side;
        }
    }

    // 反例 A: replay one outer probe's payload into the tunnel with the
    // peer overlay dst — a genuine leak the scan must catch on B's side
    if opts.leak_outer_probe_into_tunnel && session_established {
        let leaked = evidence
            .probes
            .iter()
            .find(|p| p.kind == Some(EndpointKind::Signal))
            .or_else(|| evidence.probes.iter().find(|p| p.kind.is_some()));
        if let Some(p) = leaked {
            if let Some(WireMarker::Ascii(marker)) = &p.marker {
                let frame = crate::host_sockets::build_ipv4_udp_packet(
                    VPN_A,
                    VPN_B,
                    POSCTL_SPORT,
                    POSCTL_DPORT,
                    marker.as_bytes(),
                );
                let (n, _e) = sys::write_fd(hand_a, &frame);
                if n > 0 {
                    tun_injected += 1;
                    notes.push(format!(
                        "fault-injected: outer probe {} payload replayed INTO the tunnel (overlay dst) — must be caught as a leak",
                        p.probe_id
                    ));
                }
            }
        }
    }

    // ---- observation loop: drain hands (scan), sinks (receipts), device B
    // counters (wg-peer receipt) until complete or deadline ----
    // The wg-peer receipt baseline is the WINDOW's opening snapshot, so the
    // off-path drop the probe caused is attributable inside the window.
    let unknown_drops_before_b = before_b.wg_unknown_peer_drops;
    let observe_deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(OBSERVE_DEADLINE_MS);
    let wg_peer_probe = evidence
        .probes
        .iter()
        .find(|p| p.kind == Some(EndpointKind::WgPeer))
        .map(|p| (p.probe_id.clone(), p.marker.clone()));
    let mut wg_peer_confirmed = wg_peer_probe.is_none();
    let mut settle_rounds = 0u32;
    while std::time::Instant::now() < observe_deadline {
        let now = sys::mono_ms();
        dev_a.service_tun(now);
        dev_a.service_udp(now);
        dev_a.tick(now);
        dev_b.service_tun(now);
        dev_b.service_udp(now);
        dev_b.tick(now);

        // drain BOTH TUN stand-in hands through the scanner
        for (side, hand, idx) in [("a", hand_a, 0usize), ("b", hand_b, 1usize)] {
            loop {
                let (ret, _e, rev) = sys::poll1(hand, sys::POLLIN, 0);
                if ret <= 0 || (rev & sys::POLLIN) == 0 {
                    break;
                }
                let mut buf = [0u8; 2048];
                let (n, _errno) = sys::read_fd(hand, &mut buf);
                if n <= 0 {
                    break;
                }
                let frame = &buf[..n as usize];
                delivered[idx].0 += 1;
                delivered[idx].1 += frame.len() as u64;
                scanner.scan(side, frame, now);
            }
        }

        // sinks: match receipts
        mgmt_sink.drain();
        sig_sink.drain();
        stun_sink.drain();
        turn_sink.drain();
        relay_sink.drain();
        dns_sink.drain();
        for probe in &evidence.probes {
            if probe.kind == Some(EndpointKind::WgPeer) {
                continue; // receipt comes from B's device counters
            }
            let Some(marker) = &probe.marker else { continue };
            if evidence.endpoint_side.iter().any(|r| r.probe_id == probe.probe_id && r.observed) {
                continue;
            }
            let (sink_label, found): (&str, Option<&SinkRecord>) = match probe.kind {
                Some(EndpointKind::Management) => ("management-sink", mgmt_sink.observed(marker)),
                Some(EndpointKind::Signal) => ("signal-sink", sig_sink.observed(marker)),
                Some(EndpointKind::Stun) => ("stun-sink", stun_sink.observed(marker)),
                Some(EndpointKind::Turn) => ("turn-sink", turn_sink.observed(marker)),
                Some(EndpointKind::Relay) => ("relay-sink", relay_sink.observed(marker)),
                Some(EndpointKind::Dns) => ("dns-sink", dns_sink.observed(marker)),
                _ => ("none", None),
            };
            if let Some(rec) = found {
                evidence.endpoint_side.push(EndpointReceipt {
                    probe_id: probe.probe_id.clone(),
                    kind: probe.kind.map(|k| k.name()).unwrap_or("unknown").to_string(),
                    observed: true,
                    physical_src: rec.src.to_string(),
                    physical_src_source: "endpoint-reported".into(),
                    at_unix: rec.at_unix,
                    method: "sink-record".into(),
                    detail: format!("{sink_label} received the probe payload ({} bytes)", rec.payload.len()),
                });
            }
        }

        // wg-peer receipt: B's device must have counted the off-path datagram
        if !wg_peer_confirmed {
            if let Some((id, marker)) = &wg_peer_probe {
                let drops = dev_b.stats().unknown_peer_drops;
                if drops > unknown_drops_before_b {
                    if let Some(r) = evidence.endpoint_side.iter_mut().find(|r| r.probe_id == *id) {
                        r.observed = true;
                        r.detail = format!(
                            "probe datagram counted at the peer device's outer socket: unknown_peer_drops delta = {} (off-path demux drop — the outer path carried it)",
                            drops - unknown_drops_before_b
                        );
                    }
                    let _ = marker;
                    wg_peer_confirmed = true;
                }
            }
        }

        // positive-control observations from the scanner
        for p in posctl.iter_mut() {
            if p.sent && p.observed.is_none() {
                p.observed = scanner.hits_for(&p.probe_id).first().cloned();
            }
        }

        // done? Keep pumping a short SETTLE tail after the goal is first
        // reached: frames still in flight (and any late sink records) must
        // land INSIDE the window before it closes. Bounded, no long sleeps.
        let all_receipts = evidence.probes.iter().filter(|p| p.sent).all(|p| {
            p.kind == Some(EndpointKind::WgPeer) && wg_peer_confirmed
                || evidence.endpoint_side.iter().any(|r| r.probe_id == p.probe_id && r.observed)
        }) && wg_peer_confirmed;
        let all_posctl = posctl.iter().filter(|p| p.sent).all(|p| p.observed.is_some());
        if all_receipts && all_posctl {
            settle_rounds += 1;
            if settle_rounds >= OBSERVE_SETTLE_ROUNDS {
                break;
            }
        } else {
            settle_rounds = 0;
        }
        sys::sleep_ms(10);
    }

    evidence.tunnel_positive_control = PosctlEvidence { probes: posctl };

    // ---- window closes; assemble ----
    let after_a = snapshot_a(&dev_a, tun_injected, delivered[0]);
    let after_b = snapshot_b(&dev_b, tun_injected, delivered[1]);
    evidence.counters = vec![
        WindowCounters { side: "a".into(), before: before_a, after: after_a },
        WindowCounters { side: "b".into(), before: before_b, after: after_b },
    ];
    evidence.tun_negative = TunScanResult {
        hit: !scanner.outer_hits().is_empty(),
        frames_scanned: scanner.frames_scanned(),
        hits: scanner.outer_hits(),
        sides: scanner.sides(),
    };
    evidence.finished_mono = sys::mono_ms();
    evidence.notes = notes;
    let (verdict, reasons) = evidence.judge();
    evidence.verdict = verdict;
    evidence.reasons = reasons;
    Ok(evidence)
}

fn snapshot_a(dev: &crate::wg_device::WgDevice, injected: u64, delivered: (u64, u64)) -> IfaceSnapshot {
    from_stats(dev.stats(), injected, delivered)
}

fn snapshot_b(dev: &crate::wg_device::WgDevice, injected: u64, delivered: (u64, u64)) -> IfaceSnapshot {
    from_stats(dev.stats(), injected, delivered)
}

fn from_stats(s: crate::wg_device::WgDeviceStats, injected: u64, delivered: (u64, u64)) -> IfaceSnapshot {
    IfaceSnapshot {
        at_mono: sys::mono_ms(),
        at_unix: unix_now(),
        wg_tx_packets: s.tx_packets,
        wg_tx_bytes: s.tx_bytes,
        wg_rx_packets: s.rx_packets,
        wg_rx_bytes_to_tun: s.rx_bytes_to_tun,
        wg_handshake_initiations: s.handshake_initiations,
        wg_no_route_drops: s.no_route_drops,
        wg_unknown_peer_drops: s.unknown_peer_drops,
        wg_decrypt_errors: s.decrypt_errors,
        wg_send_errors: s.send_errors,
        wg_tun_write_errors: s.tun_write_errors,
        tun_injected_frames: injected,
        tun_delivered_frames: delivered.0,
        tun_delivered_bytes: delivered.1,
    }
}

// ---------------------------------------------------------------------------
// pre-connect freeze check (CLI `--config` mode): freeze + outer probes only;
// no session exists yet, so the verdict is never n2h-pass (no tunnel, no
// positive control). Useful as the pre-connection half of N2-H.
// ---------------------------------------------------------------------------

/// Freeze endpoints from a config document (management_url + `--outer-endpoint`
/// CLI entries) and probe what resolved. Reads NO secret material (the config
/// is parsed as plain JSON; only `management_url` is extracted).
pub fn run_preconnect_freeze(
    config_path: &str,
    outer_endpoints: &[(EndpointKind, String, u16)],
) -> Result<Evidence, String> {
    let started_mono = sys::mono_ms();
    let mut evidence = Evidence {
        mode: "preconnect-freeze".into(),
        session: format!("pre-connect endpoint freeze check (config: {config_path}; no tunnel session)"),
        generated_unix: unix_now(),
        started_mono,
        ..Evidence::default()
    };
    let mut notes: Vec<String> = Vec::new();

    // management from the config document (no secrets read)
    let text = std::fs::read_to_string(config_path)
        .map_err(|e| crate::host_sockets::cli_io_message("config file", config_path, &e))?;
    let doc = crate::config::parse_document(&text)
        .map_err(|e| format!("config file '{config_path}': invalid JSON: {e:?}"))?;
    let mgmt_url = match &doc {
        crate::config::Json::Obj(entries) => entries
            .iter()
            .find(|(k, _)| k == "management_url")
            .and_then(|(_, v)| match v {
                crate::config::Json::Str(s) => Some(s.clone()),
                _ => None,
            }),
        _ => None,
    };
    let now_u = unix_now();
    let now_m = sys::mono_ms();
    let push_absent = |kind: EndpointKind, reason: &str, evidence: &mut Evidence| {
        evidence.frozen_endpoints.push(FrozenEndpoint {
            kind: Some(kind),
            present: false,
            absent_reason: reason.to_string(),
            frozen_at_unix: now_u,
            frozen_at_mono: now_m,
            ..FrozenEndpoint::default()
        });
    };

    if let Some(url) = mgmt_url {
        match crate::host_sockets::parse_management_endpoint(&url) {
            Ok((_tls, host, port)) => {
                match crate::host_sockets::resolve_ipv4_host(&host, port) {
                    Ok(addr) => evidence.frozen_endpoints.push(FrozenEndpoint {
                        kind: Some(EndpointKind::Management),
                        present: true,
                        host: host.clone(),
                        port,
                        proto: "tcp".into(),
                        source: "config".into(),
                        resolved: addr.to_string(),
                        absent_reason: String::new(),
                        frozen_at_unix: now_u,
                        frozen_at_mono: now_m,
                    }),
                    Err(e) => {
                        push_absent(EndpointKind::Management, &format!("resolve failed for '{host}': {e}"), &mut evidence);
                    }
                }
            }
            Err(e) => push_absent(EndpointKind::Management, &format!("management_url unparseable: {e}"), &mut evidence),
        }
    } else {
        push_absent(EndpointKind::Management, "no management_url in config document", &mut evidence);
    }

    // CLI-supplied endpoints
    for (kind, host, port) in outer_endpoints {
        match crate::host_sockets::resolve_ipv4_host(host, *port) {
            Ok(addr) => evidence.frozen_endpoints.push(FrozenEndpoint {
                kind: Some(*kind),
                present: true,
                host: host.clone(),
                port: *port,
                proto: kind.proto().into(),
                source: "cli".into(),
                resolved: addr.to_string(),
                absent_reason: String::new(),
                frozen_at_unix: now_u,
                frozen_at_mono: now_m,
            }),
            Err(e) => notes.push(format!(
                "endpoint {kind:?} {host}:{port} resolve failed ({e}) — fail-closed, not frozen"
            )),
        }
    }

    // every kind the session would need but a pre-connect check cannot know
    for kind in EndpointKind::ALL {
        if evidence.frozen_endpoints.iter().any(|f| f.kind == Some(kind)) {
            continue;
        }
        push_absent(
            kind,
            "pre-connect freeze: endpoint only knowable from the sync-delivered network map (no session yet)",
            &mut evidence,
        );
    }

    // probe what resolved (receipts come only from operator-provided sinks,
    // except STUN responses, which are self-authenticating)
    let frozen: Vec<(EndpointKind, SocketAddr)> = evidence
        .frozen_endpoints
        .iter()
        .filter(|f| f.present)
        .filter_map(|f| {
            f.kind.map(|k| {
                f.resolved
                    .parse::<SocketAddr>()
                    .map(|a| (k, a))
                    .map_err(|e| format!("frozen endpoint {}: bad resolved addr: {e}", k.name()))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (kind, addr) in frozen {
        let nonce = fresh_nonce_hex()?;
        let id = probe_id(kind.name(), &nonce);
        let probe_result = if kind == EndpointKind::Stun {
            let txid = crate::stun::random_transaction_id()
                .map_err(|e| format!("stun probe entropy: {e:?}"))?;
            let id = probe_id(kind.name(), &hex_lower(&txid.0));
            let (tuple, mapped) = send_stun_probe(addr, txid.0, 800)?;
            evidence.endpoint_side.push(EndpointReceipt {
                probe_id: id.clone(),
                kind: kind.name().into(),
                observed: mapped.is_some(),
                physical_src: mapped
                    .map(|(a, p)| format!("{}:{}", ipv4_str(a), p))
                    .unwrap_or_default(),
                physical_src_source: "endpoint-reported".into(),
                at_unix: unix_now(),
                method: "stun-binding-response".into(),
                detail: "txid-matched Binding Success Response".into(),
            });
            evidence.probes.push(OuterProbe {
                probe_id: id.clone(),
                kind: Some(kind),
                tuple,
                expected_path: "outer",
                sent: true,
                sent_at_unix: unix_now(),
                sent_at_mono: sys::mono_ms(),
                marker: Some(WireMarker::StunTxid(txid.0)),
                payload_len: 20,
            });
            continue;
        } else {
            match kind.proto() {
                "tcp" => send_tcp_probe(addr, &id),
                _ => send_udp_probe(addr, &id),
            }
        };
        match probe_result {
            Ok((tuple, len)) => {
                // no sink on the host side for a real endpoint: the receipt
                // stays UNOBSERVED — operator-supplied server logs close the
                // gap (documented residual); the freeze check itself stays honest
                evidence.endpoint_side.push(EndpointReceipt {
                    probe_id: id.clone(),
                    kind: kind.name().into(),
                    observed: false,
                    physical_src: tuple.src.clone(),
                    physical_src_source: "probe-socket-getsockname".into(),
                    at_unix: unix_now(),
                    method: "none".into(),
                    detail: "probe sent; endpoint-side receipt requires an operator-provided sink/server log (see residual_scope)".into(),
                });
                evidence.probes.push(OuterProbe {
                    probe_id: id.clone(),
                    kind: Some(kind),
                    tuple,
                    expected_path: "outer",
                    sent: true,
                    sent_at_unix: unix_now(),
                    sent_at_mono: sys::mono_ms(),
                    marker: Some(WireMarker::Ascii(id)),
                    payload_len: len,
                });
            }
            Err(e) => {
                notes.push(format!("probe send failure for {kind:?}: {e}"));
                evidence.probes.push(OuterProbe {
                    probe_id: id,
                    kind: Some(kind),
                    expected_path: "outer",
                    sent: false,
                    ..OuterProbe::default()
                });
            }
        }
    }

    evidence.finished_mono = sys::mono_ms();
    evidence.notes = notes;
    let (verdict, reasons) = evidence.judge();
    evidence.verdict = verdict;
    evidence.reasons = reasons;
    Ok(evidence)
}

// ---------------------------------------------------------------------------
// tests (pure logic + codec; no sockets here — the driver is covered by
// tests/n2h_isolation.rs)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(src: [u8; 4], dst: [u8; 4], sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
        crate::host_sockets::build_ipv4_udp_packet(src, dst, sport, dport, payload)
    }

    #[test]
    fn ipv4_frame_parser_extracts_tuple_and_payload() {
        let f = frame([10, 1, 2, 3], [10, 4, 5, 6], 1234, 5678, b"hello-marker");
        let v = parse_ipv4_frame(&f).expect("parses");
        assert_eq!(v.src, [10, 1, 2, 3]);
        assert_eq!(v.dst, [10, 4, 5, 6]);
        assert_eq!(v.proto, 17);
        assert_eq!(v.sport, 1234);
        assert_eq!(v.dport, 5678);
        assert_eq!(v.payload, b"hello-marker");
        // short / version / truncated shapes are rejected, never panic
        assert!(parse_ipv4_frame(&f[..12]).is_none());
        assert!(parse_ipv4_frame(&{
            let mut g = f.clone();
            g[0] = 0x65; // version 6 inside a "v4" path
            g
        })
        .is_none());
        // a real TCP frame (20-byte header, data offset 5) parses ports+payload
        let mut tcp = Vec::new();
        tcp.extend_from_slice(&[0x45, 0]);
        let total: u16 = 40 + 3;
        tcp.extend_from_slice(&total.to_be_bytes());
        tcp.extend_from_slice(&[0, 1, 0, 0, 64, 6, 0, 0]); // proto 6 = TCP
        tcp.extend_from_slice(&[10, 0, 0, 1]);
        tcp.extend_from_slice(&[10, 0, 0, 2]);
        tcp.extend_from_slice(&80u16.to_be_bytes()); // sport
        tcp.extend_from_slice(&81u16.to_be_bytes()); // dport
        tcp.extend_from_slice(&[0, 0, 0, 1]); // seq
        tcp.extend_from_slice(&[0, 0, 0, 2]); // ack
        tcp.push(0x50); // data offset 5 (20-byte header)
        tcp.push(0x18); // flags PSH|ACK
        tcp.extend_from_slice(&[0, 64]); // window
        tcp.extend_from_slice(&[0, 0]); // checksum
        tcp.extend_from_slice(&[0, 0]); // urgent pointer
        tcp.extend_from_slice(b"abc");
        let v = parse_ipv4_frame(&tcp).expect("tcp parses");
        assert_eq!(v.proto, 6);
        assert_eq!(v.sport, 80);
        assert_eq!(v.dport, 81);
        assert_eq!(v.payload, b"abc");
    }

    #[test]
    fn marker_matching_is_verbatim_substring() {
        let f = frame([10, 0, 0, 1], [10, 0, 0, 2], 1, 2, b"xN2H-signal-abc");
        assert!(frame_contains_marker(&f, b"N2H-signal-abc"));
        assert!(WireMarker::Ascii("N2H-signal-abc".into()).matches(&f));
        assert!(!WireMarker::Ascii("N2H-signal-other".into()).matches(&f));
        assert!(!frame_contains_marker(&f, b""));
        assert!(!frame_contains_marker(b"tiny", b"tiny-but-longer"));
        let mut txid = [0u8; 12];
        txid.copy_from_slice(&[9u8; 12]);
        assert!(WireMarker::StunTxid(txid).matches(&[1, 2, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 7]));
    }

    #[test]
    fn probe_ids_are_unique_across_a_burst() {
        let mut seen = std::collections::HashSet::new();
        for kind in EndpointKind::ALL {
            for _ in 0..8 {
                let id = probe_id(kind.name(), &fresh_nonce_hex().unwrap());
                assert!(id.starts_with(&format!("N2H-{}-", kind.name())), "{id}");
                assert!(seen.insert(id), "probe id collision");
            }
        }
    }

    #[test]
    fn stun_response_fixture_parses_with_the_real_codec() {
        use std::net::IpAddr;
        let txid = [7u8; 12];
        let mapped = SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 7)), 51820);
        let resp = build_binding_response(&txid, mapped);
        let parsed = crate::stun::parse_binding_response(
            &resp,
            &crate::stun::TransactionId(txid),
        )
        .expect("parses");
        match parsed {
            crate::stun::StunReply::Mapped { addr, port } => {
                assert_eq!(addr, [192, 0, 2, 7]);
                assert_eq!(port, 51820);
            }
            other => panic!("expected Mapped, got {other:?}"),
        }
    }

    #[test]
    fn counters_reconcile_identity() {
        let before = IfaceSnapshot { wg_rx_bytes_to_tun: 100, tun_delivered_bytes: 50, ..Default::default() };
        let mut after = before;
        after.wg_rx_bytes_to_tun = 133;
        after.tun_delivered_bytes = 83;
        let w = WindowCounters { side: "a".into(), before, after };
        assert!(w.reconciled(), "33 == 33");
        let mut after_bad = after;
        after_bad.tun_delivered_bytes = 82;
        let w_bad = WindowCounters { side: "a".into(), before, after: after_bad };
        assert!(!w_bad.reconciled(), "33 != 32");
        assert_eq!(w.delta().wg_rx_bytes_to_tun, 33);
    }

    fn empty_evidence() -> Evidence {
        Evidence::default()
    }

    fn complete_freeze() -> Vec<FrozenEndpoint> {
        EndpointKind::ALL
            .iter()
            .map(|k| FrozenEndpoint {
                kind: Some(*k),
                present: true,
                host: "127.0.0.1".into(),
                port: 100,
                proto: k.proto().into(),
                source: "test".into(),
                resolved: "127.0.0.1:100".into(),
                absent_reason: String::new(),
                frozen_at_unix: 0,
                frozen_at_mono: 0,
            })
            .collect()
    }

    fn sent_probe(kind: EndpointKind) -> OuterProbe {
        OuterProbe {
            probe_id: format!("N2H-{}-test", kind.name()),
            kind: Some(kind),
            tuple: FiveTuple { proto: kind.proto().into(), src: "127.0.0.1:1".into(), dst: "127.0.0.1:2".into() },
            expected_path: "outer",
            sent: true,
            sent_at_unix: 0,
            sent_at_mono: 0,
            marker: Some(WireMarker::Ascii(format!("N2H-{}-test", kind.name()))),
            payload_len: 4,
        }
    }

    fn observed_receipt(probe: &OuterProbe) -> EndpointReceipt {
        EndpointReceipt {
            probe_id: probe.probe_id.clone(),
            kind: probe.kind.map(|k| k.name()).unwrap_or("unknown").into(),
            observed: true,
            physical_src: "127.0.0.1:1".into(),
            physical_src_source: "endpoint-reported".into(),
            at_unix: 0,
            method: "sink-record".into(),
            detail: "test".into(),
        }
    }

    fn passing_evidence() -> Evidence {
        let probes: Vec<OuterProbe> = EndpointKind::ALL.iter().map(|k| sent_probe(*k)).collect();
        let receipts = probes.iter().map(observed_receipt).collect();
        let reconciled = WindowCounters {
            side: "a".into(),
            before: IfaceSnapshot { wg_rx_bytes_to_tun: 10, tun_delivered_bytes: 10, ..Default::default() },
            after: IfaceSnapshot { wg_rx_bytes_to_tun: 43, tun_delivered_bytes: 43, ..Default::default() },
        };
        Evidence {
            frozen_endpoints: complete_freeze(),
            probes,
            tunnel_positive_control: PosctlEvidence {
                probes: vec![PosctlProbe {
                    probe_id: "N2H-posctl-x".into(),
                    direction: "a2b".into(),
                    tuple: FiveTuple { proto: "ipv4".into(), src: "10.77.0.1:40000".into(), dst: "10.77.0.2:40001".into() },
                    expected_path: "tunnel",
                    sent: true,
                    sent_at_mono: 0,
                    observed: Some(TunHit {
                        probe_id: "N2H-posctl-x".into(),
                        side: "b".into(),
                        src: VPN_A,
                        dst: VPN_B,
                        sport: POSCTL_SPORT,
                        dport: POSCTL_DPORT,
                        frame_len: 33,
                        observed_at_mono: 1,
                    }),
                    note: String::new(),
                }],
            },
            tun_negative: TunScanResult { hit: false, frames_scanned: 4, hits: vec![], sides: vec!["a".into(), "b".into()] },
            endpoint_side: receipts,
            counters: vec![reconciled],
            ..empty_evidence()
        }
    }

    #[test]
    fn verdict_pass_requires_every_leg() {
        let ev = passing_evidence();
        let (v, reasons) = ev.judge();
        assert_eq!(v, VERDICT_PASS, "{reasons:?}");
        assert!(reasons.is_empty());

        // missing endpoint-side receipt for ONE probe → inconclusive
        let mut ev = passing_evidence();
        ev.endpoint_side.retain(|r| !r.probe_id.contains("dns"));
        let (v, reasons) = ev.judge();
        assert_eq!(v, VERDICT_INCONCLUSIVE);
        assert!(reasons.iter().any(|r| r.contains("dns")), "{reasons:?}");

        // unreconciled counters → inconclusive
        let mut ev = passing_evidence();
        ev.counters[0].after.tun_delivered_bytes += 1;
        let (v, reasons) = ev.judge();
        assert_eq!(v, VERDICT_INCONCLUSIVE);
        assert!(reasons.iter().any(|r| r.contains("reconciliation mismatch")), "{reasons:?}");

        // no counters at all → inconclusive
        let mut ev = passing_evidence();
        ev.counters.clear();
        let (v, _) = ev.judge();
        assert_eq!(v, VERDICT_INCONCLUSIVE);
    }

    #[test]
    fn verdict_fail_on_any_outer_hit_even_with_other_gaps() {
        let mut ev = passing_evidence();
        ev.tunnel_positive_control.probes.clear(); // would be inconclusive...
        ev.tun_negative = TunScanResult {
            hit: true,
            frames_scanned: 9,
            hits: vec![TunHit {
                probe_id: "N2H-dns-leak".into(),
                side: "b".into(),
                src: [10, 77, 0, 1],
                dst: [10, 77, 0, 2],
                sport: 1,
                dport: 2,
                frame_len: 40,
                observed_at_mono: 5,
            }],
            sides: vec!["b".into()],
        };
        let (v, reasons) = ev.judge();
        assert_eq!(v, VERDICT_FAIL, "direct falsification must win");
        assert!(reasons[0].contains("N2H-dns-leak"), "{reasons:?}");
    }

    #[test]
    fn missing_positive_control_is_never_a_pass() {
        // absent entirely
        let mut ev = passing_evidence();
        ev.tunnel_positive_control = PosctlEvidence::default();
        let (v, reasons) = ev.judge();
        assert_eq!(v, VERDICT_INCONCLUSIVE);
        assert!(reasons.iter().any(|r| r.contains("positive control")), "{reasons:?}");

        // sent but never observed (the 反例 B shape)
        let mut ev = passing_evidence();
        ev.tunnel_positive_control.probes[0].observed = None;
        let (v, reasons) = ev.judge();
        assert_eq!(v, VERDICT_INCONCLUSIVE);
        assert!(reasons.iter().any(|r| r.contains("never observed")), "{reasons:?}");

        // recorded but not sent
        let mut ev = passing_evidence();
        ev.tunnel_positive_control.probes[0].sent = false;
        let (v, _) = ev.judge();
        assert_eq!(v, VERDICT_INCONCLUSIVE);
    }

    #[test]
    fn freeze_gaps_name_the_missing_kind() {
        // required kind absent WITH reason → still inconclusive (fail-closed)
        let mut ev = passing_evidence();
        ev.frozen_endpoints.iter_mut().for_each(|f| {
            if f.kind == Some(EndpointKind::Signal) {
                f.present = false;
                f.absent_reason = "not obtained".into();
            }
        });
        ev.probes.retain(|p| p.kind != Some(EndpointKind::Signal));
        ev.endpoint_side.retain(|r| r.kind != "signal");
        let (v, reasons) = ev.judge();
        assert_eq!(v, VERDICT_INCONCLUSIVE);
        assert!(reasons.iter().any(|r| r.contains("'signal'")), "{reasons:?}");

        // optional kind absent WITH reason → no gap
        let mut ev = passing_evidence();
        ev.frozen_endpoints.iter_mut().for_each(|f| {
            if f.kind == Some(EndpointKind::Turn) {
                f.present = false;
                f.absent_reason = "none configured in this deployment".into();
            }
        });
        ev.probes.retain(|p| p.kind != Some(EndpointKind::Turn));
        ev.endpoint_side.retain(|r| r.kind != "turn");
        let (v, reasons) = ev.judge();
        assert_eq!(v, VERDICT_PASS, "{reasons:?}");

        // optional kind absent WITHOUT reason → gap
        let mut ev = passing_evidence();
        ev.frozen_endpoints.iter_mut().for_each(|f| {
            if f.kind == Some(EndpointKind::Relay) {
                f.present = false;
                f.absent_reason = String::new();
            }
        });
        ev.probes.retain(|p| p.kind != Some(EndpointKind::Relay));
        ev.endpoint_side.retain(|r| r.kind != "relay");
        let (v, _) = ev.judge();
        assert_eq!(v, VERDICT_INCONCLUSIVE);
    }

    #[test]
    fn evidence_json_is_parseable_and_forbidden_word_free() {
        let mut ev = passing_evidence();
        let (v, reasons) = ev.judge();
        ev.verdict = v;
        assert!(!ev.verdict.is_empty(), "{reasons:?}");
        let json = ev.to_json();
        let parsed = crate::config::parse_document(&json);
        assert!(matches!(parsed, Ok(crate::config::Json::Obj(_))), "json must parse: {json}");
        for forbidden in ["protect pass", "N2 pass", "等同逐 socket protect", "waived", "N/A"] {
            assert!(!json.contains(forbidden), "forbidden record token '{forbidden}' in evidence");
        }
        assert!(json.contains("\"verdict\":\"n2h-pass\""), "{json}");
        assert!(json.contains("UNSAT/未满足"), "{json}");
        assert!(json.contains("route-exclusion"), "{json}");
        assert!(json.contains("\"hit\":false"), "{json}");
        // probe id format
        assert!(json.contains("N2H-management-"), "{json}");
    }

    #[test]
    fn endpoint_kind_table_is_stable() {
        assert_eq!(EndpointKind::from_name("wg_peer"), Some(EndpointKind::WgPeer));
        assert_eq!(EndpointKind::from_name("nope"), None);
        assert!(EndpointKind::Signal.required());
        assert!(!EndpointKind::Turn.required());
        assert!(!EndpointKind::Relay.required());
        assert_eq!(EndpointKind::Signal.proto(), "tcp");
        assert_eq!(EndpointKind::Stun.proto(), "udp");
        // T0 relay ruling: the validated deployment relays over WSS/TCP — the
        // frozen set must say tcp or an outer hairpin cannot be falsified.
        assert_eq!(EndpointKind::Relay.proto(), "tcp");
    }

    #[test]
    fn stun_probe_reaches_a_non_loopback_local_endpoint() {
        // Regression (device-validation run 2): the probe socket used to bind
        // 127.0.0.1, so sending to any real, non-loopback outer endpoint
        // failed with EINVAL — surfaced by the CLI as
        // "stun probe send: Invalid argument (os error 22)". A wildcard bind
        // lets the kernel pick a source address valid for the route.
        let routing_probe = std::net::UdpSocket::bind("0.0.0.0:0").expect("bind");
        // No packets are sent: connect() only asks the kernel which source
        // address it WOULD use towards a routable destination.
        if routing_probe.connect("192.0.2.1:9").is_err() {
            return; // no route information on this host: nothing to assert
        }
        let local = routing_probe.local_addr().expect("local addr").ip();
        if local.is_loopback() || local.is_unspecified() {
            return; // host without a routable non-loopback IPv4: skip
        }
        let dst: SocketAddr = (local, 9).into();
        let out = send_stun_probe(dst, [0x5au8; 12], 50);
        assert!(
            out.is_ok(),
            "stun probe to the non-loopback local address {dst} must not fail at send: {:?}",
            out.err()
        );
    }
}
