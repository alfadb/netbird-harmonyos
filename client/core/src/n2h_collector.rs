// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # n2h_collector — N2-H DEVICE-side evidence collector (increments I0 + I1 + I3 + I4)
//!
//! Spec (single source): `docs/n2h-device-collector-design-20260917.md` —
//! §2.4 (HiLog chunk channel), §4.0 (freeze assembly), §4.2 (probe sender),
//! §4.4 (`/proc` reconciliation), §7 I0/I1/I3/I4, §3 (frozen document
//! alignment). Those four increments live here; the window state machine, the
//! receipt matrix, the verdict wiring and the napi exports are increment I5 and
//! do not exist yet.
//!
//! - **I0 — chunk encoder.** The evidence document is split into 512-byte raw
//!   slices; each slice becomes one frozen HiLog line
//!   `N2H_EVIDENCE|seq=i/n|sha256=<full document>|b64=<slice>`. The receiver
//!   reassembles the slices and refuses a document whose digest disagrees, so
//!   a dropped line is DETECTED (never silently shipped); recovery is replay —
//!   re-emitting the same document is byte-identical, which is why the encoder
//!   is pure and carries no state.
//! - **I1 — freeze assembly.** The 7 endpoint classes of
//!   [`crate::n2h::EndpointKind::ALL`] are assembled, from caller-injected
//!   sources only (shell exclusion set ∪ core session knowledge), into the
//!   `frozen_endpoints[]` array of the evidence document with the D7 `source`
//!   token and — for a reasoned absence — `absent_reason`.
//! - **I3 — device probe sender.** One FRESH wildcard-bound socket per probe
//!   (`0.0.0.0:0`, deliberately NOT the host's `127.0.0.1:0`), payload =
//!   [`probe_id`] `N2H-<kind>-<nonce>` for UDP/TCP and the 12-byte transaction
//!   id for STUN; the transport follows the kind's `proto()` column, so a probe
//!   can never use a transport the frozen set did not declare. The unverified
//!   `dns-nxdomain-echo` receipt method exists but is OFF by default (design
//!   §4.0/§9-U5).
//! - **I4 — `/proc` reconciliation.** The `vpn-tun` row of `/proc/net/dev` (or
//!   the operator's `hdc` capture as the D-class fallback), paired with the
//!   core WG counters in the frozen order (`/proc` FIRST), audited by BOTH
//!   identities of design §4.4 (bytes AND packets) with a bounded resample.
//!   The read seam is injectable, so no unit test touches the real `/proc`.
//!
//! I0/I1 are deliberately IO-free and take every external fact as a parameter
//! (clock included). I3/I4 own the module's only two IO surfaces and both are
//! design-mandated (the probe sockets of §4.2, the `/proc` read of §4.4); the
//! sampling discipline around them (frozen order, bounded resample, honest
//! unreconciled result) is what keeps a device window auditable instead of
//! best-effort.
//!
//! This module renders **no** JSON: the document is always
//! [`crate::n2h::Evidence::to_json`], the one renderer shared by the host tool
//! and the device collector, so the design §3 field order cannot drift between
//! them (frozen JSON discipline).
//!
//! **Governance scope (binding, same clauses as `n2h.rs` — design §6.1):** this module
//! is an evidence COLLECTOR, not a policy change. It does not satisfy the
//! original §二.4 per-socket-protect clause (recorded as UNSAT, see
//! [`crate::n2h::UNSAT_NOTE`]); `n2h-pass` is NOT a pass of the original N2
//! criteria, and N2-H itself only takes effect after explicit user approval
//! and a formal governance-document revision. See
//! `docs/n2h-isolation-evidence-notes.md`.
//!
//! **Isolation:** compiled into the cdylib (like `n2h`, `client/core/src/lib.rs`),
//! but INERT — nothing calls into this module unless the collector entry points
//! are armed (I5), it contains no routing/exclusion/VPN lifecycle code, and the
//! shipped cdylib behavior is unchanged. The collector's sources are injected
//! by the caller: this module reads no connector state and contains no
//! routing/exclusion/VPN lifecycle code, and its only filesystem read is the
//! `/proc/net/dev` ledger (§4.4) plus the operator capture it falls back to.

use crate::hilog;
use crate::n2h::{
    probe_id, EndpointKind, FiveTuple, FrozenEndpoint, IfaceSnapshot, WireMarker, WindowCounters,
};
use crate::util::{base64, sha256_hex};
use crate::wg_device::{TunTeeTotals, WgDeviceStats};
use std::net::SocketAddr;

// ---------------------------------------------------------------------------
// I0 — chunk encoder (design §2.4-A)
// ---------------------------------------------------------------------------

/// Raw document bytes carried per chunk line. 512 B is the conservative value
/// of the design (§2.4/U6: the device HiLog single-line limit is not measured
/// yet); base64 expands it to at most 684 characters.
pub const CHUNK_PAYLOAD_BYTES: usize = 512;

/// Frozen marker prefix of every evidence chunk line — the host-side capture
/// greps exactly this token (design §6.4-3).
pub const CHUNK_MARKER: &str = "N2H_EVIDENCE";

/// One numbered chunk of an encoded evidence document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceChunk {
    /// 0-based position: the `i` of `seq=i/n`.
    pub seq: usize,
    /// Chunk count of the document: the `n` of `seq=i/n`. An empty document
    /// still travels as ONE empty chunk, so "no chunk line at all" stays
    /// distinguishable from "document with zero bytes".
    pub total: usize,
    /// SHA-256 (lowercase hex) of the FULL pre-split document, repeated on
    /// every line — a mutated or spliced line is detectable on its own.
    pub sha256: String,
    /// RFC 4648 base64 of this chunk's raw slice (the alphabet contains no
    /// `|`, so the line stays parseable by the frozen `|`-separated reader).
    pub b64: String,
}

impl EvidenceChunk {
    /// `N2H_EVIDENCE|seq=i/n|sha256=<full doc>|b64=<slice>` — the exact line
    /// shape frozen by design §2.4-A.
    pub fn line(&self) -> String {
        format!(
            "{}|seq={}/{}|sha256={}|b64={}",
            CHUNK_MARKER, self.seq, self.total, self.sha256, self.b64
        )
    }
}

/// Split a document into its chunk lines (design §2.4-A: 512 B raw per chunk).
///
/// Pure and deterministic: the same document always yields byte-identical
/// lines, which is exactly what makes a replay safe (the receiver's digest
/// check cannot tell a first send from a re-send).
pub fn encode_evidence(doc: &[u8]) -> Vec<EvidenceChunk> {
    let sha256 = sha256_hex(doc);
    let total = if doc.is_empty() {
        1
    } else {
        (doc.len() + CHUNK_PAYLOAD_BYTES - 1) / CHUNK_PAYLOAD_BYTES
    };
    (0..total)
        .map(|seq| {
            let start = seq * CHUNK_PAYLOAD_BYTES;
            let end = (start + CHUNK_PAYLOAD_BYTES).min(doc.len());
            let slice = if doc.is_empty() { &doc[..] } else { &doc[start..end] };
            EvidenceChunk {
                seq,
                total,
                sha256: sha256.clone(),
                b64: base64(slice),
            }
        })
        .collect()
}

/// Encode the document and write every chunk line to the frozen HiLog channel;
/// returns the chunk count (the `n` a caller records / re-serves).
///
/// Calling it again IS the replay (design §2.4-R2: a line lost to the device
/// log level or buffer rotation is recovered by re-emitting the document, not
/// by repairing a partial one).
pub fn emit_evidence(doc: &[u8]) -> usize {
    let chunks = encode_evidence(doc);
    for chunk in &chunks {
        hilog::emit(&chunk.line());
    }
    chunks.len()
}

/// Why a set of chunk lines could not be reassembled into the document. Every
/// variant is a real capture failure mode (lost line, spliced/duplicated line,
/// truncated or mutated payload), and each one must fail the evidence rather
/// than degrade it silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkError {
    /// No chunk lines at all (capture found nothing).
    NoChunks,
    /// A line does not match the frozen `N2H_EVIDENCE|seq=i/n|sha256=…|b64=…`
    /// shape (foreign marker, missing field, out-of-range `seq`).
    MalformedLine(String),
    /// Two lines disagree about the total chunk count — spliced captures.
    TotalMismatch { first: usize, other: usize },
    /// Two lines disagree about the full-document digest — spliced captures.
    DigestDisagreement { seq: usize },
    /// The same `seq` arrived twice, so at least one line is not from this run.
    Duplicate { seq: usize },
    /// A needed `seq` never arrived (the HiLog line was dropped).
    Missing { seq: usize },
    /// A payload field is not valid RFC 4648 base64.
    BadBase64 { seq: usize },
    /// Reassembly succeeded but the document does not hash to the digest every
    /// line declared — the payload was mutated in transit.
    Sha256Mismatch { declared: String, actual: String },
}

impl std::fmt::Display for ChunkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkError::NoChunks => write!(f, "no {CHUNK_MARKER} chunk lines"),
            ChunkError::MalformedLine(line) => write!(f, "malformed chunk line: {line}"),
            ChunkError::TotalMismatch { first, other } => write!(
                f,
                "chunk total disagrees across lines: {first} vs {other}"
            ),
            ChunkError::DigestDisagreement { seq } => write!(
                f,
                "chunk {seq} declares a different full-document sha256 than the other lines"
            ),
            ChunkError::Duplicate { seq } => write!(f, "duplicate chunk seq {seq}"),
            ChunkError::Missing { seq } => write!(f, "missing chunk seq {seq}"),
            ChunkError::BadBase64 { seq } => write!(f, "chunk seq {seq} is not valid base64"),
            ChunkError::Sha256Mismatch { declared, actual } => write!(
                f,
                "reassembled document sha256 {actual} does not match the declared {declared}"
            ),
        }
    }
}

impl std::error::Error for ChunkError {}

/// Reassemble a document from captured chunk lines (host-side step of the
/// design §2.4 pipeline: grep `N2H_EVIDENCE` → join by seq → verify digest).
///
/// Order-independent: `seq` carries the position, so the capture may hand the
/// lines in any order. Every failure mode of [`ChunkError`] is enforced here —
/// a partial or mutated capture never becomes a document.
pub fn decode_evidence(lines: &[&str]) -> Result<Vec<u8>, ChunkError> {
    if lines.is_empty() {
        return Err(ChunkError::NoChunks);
    }
    let mut total: Option<usize> = None;
    let mut digest: Option<String> = None;
    let mut slots: Vec<Option<String>> = Vec::new();
    for line in lines {
        let (seq, line_total, line_sha, b64) = parse_chunk_line(line)?;
        match total {
            None => {
                total = Some(line_total);
                slots = vec![None; line_total];
            }
            Some(known) if known != line_total => {
                return Err(ChunkError::TotalMismatch { first: known, other: line_total })
            }
            _ => {}
        }
        match &digest {
            None => digest = Some(line_sha),
            Some(known) if *known != line_sha => {
                return Err(ChunkError::DigestDisagreement { seq })
            }
            _ => {}
        }
        if slots[seq].is_some() {
            return Err(ChunkError::Duplicate { seq });
        }
        slots[seq] = Some(b64);
    }
    let total = match total {
        Some(t) => t,
        None => return Err(ChunkError::NoChunks),
    };
    let mut raw = Vec::new();
    for seq in 0..total {
        let part = match &slots[seq] {
            Some(p) => p,
            None => return Err(ChunkError::Missing { seq }),
        };
        let bytes = match base64_decode(part) {
            Some(b) => b,
            None => return Err(ChunkError::BadBase64 { seq }),
        };
        raw.extend_from_slice(&bytes);
    }
    let declared = digest.unwrap_or_default();
    let actual = sha256_hex(&raw);
    if declared != actual {
        return Err(ChunkError::Sha256Mismatch { declared, actual });
    }
    Ok(raw)
}

/// Parse one frozen chunk line into `(seq, total, sha256, b64)`.
fn parse_chunk_line(line: &str) -> Result<(usize, usize, String, String), ChunkError> {
    let bad = || ChunkError::MalformedLine(line.to_string());
    let mut fields = line.split('|');
    if fields.next() != Some(CHUNK_MARKER) {
        return Err(bad());
    }
    let seq_field = fields.next().ok_or_else(bad)?;
    let sha_field = fields.next().ok_or_else(bad)?;
    let b64_field = fields.next().ok_or_else(bad)?;
    // b64/sha256 cannot contain '|', so any extra field means a foreign line
    if fields.next().is_some() {
        return Err(bad());
    }
    let (seq_str, total_str) = seq_field
        .strip_prefix("seq=")
        .and_then(|pair| pair.split_once('/'))
        .ok_or_else(bad)?;
    let seq: usize = seq_str.parse().map_err(|_| bad())?;
    let total: usize = total_str.parse().map_err(|_| bad())?;
    if total == 0 || seq >= total {
        return Err(bad());
    }
    let sha = sha_field.strip_prefix("sha256=").ok_or_else(bad)?;
    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let b64 = b64_field.strip_prefix("b64=").ok_or_else(bad)?;
    Ok((seq, total, sha.to_ascii_lowercase(), b64.to_string()))
}

/// Decode RFC 4648 standard base64 (`=`-padded) — the inverse of
/// [`crate::util::base64`], local to this module because the capture side must
/// verify its own transport. Rejects wrong length, stray padding and any
/// non-alphabet byte instead of guessing.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut i = 0;
    while i < bytes.len() {
        let quantum = &bytes[i..i + 4];
        let pad = quantum.iter().filter(|b| **b == b'=').count();
        // '=' only ever closes the FINAL quantum, and never more than two of them
        if pad > 2 || (pad > 0 && i + 4 != bytes.len()) {
            return None;
        }
        let mut vals = [0u32; 4];
        for (j, b) in quantum.iter().enumerate() {
            vals[j] = if *b == b'=' { 0 } else { b64_value(*b)? as u32 };
        }
        let n = (vals[0] << 18) | (vals[1] << 12) | (vals[2] << 6) | vals[3];
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
        i += 4;
    }
    Some(out)
}

fn b64_value(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// I1 — freeze assembly (design §4.0, `frozen_endpoints[]` of §3.2)
// ---------------------------------------------------------------------------

/// Source token: the shell-side exclusion set (ets `deriveEndpointExclusions`,
/// design D7). Injected first for a kind, and the token that keeps the slot
/// when a core source resolves to the same endpoint — the shell set is the
/// artifact C2 cross-checks inside the same document.
pub const SOURCE_SHELL_EXCLUSION: &str = "shell-exclusion";
/// Source token: the sync-delivered network map (STUN URIs, DNS servers).
pub const SOURCE_CORE_NETWORK_MAP: &str = "core-network-map";
/// Source token: `relay_advertised.advertised_urls` from the core status
/// ("advertised == excluded", runbook :195).
pub const SOURCE_CORE_RELAY_ADVERTISED: &str = "core-relay-advertised";
/// Source token: `netbird_config.signal` of the sync-delivered network map.
pub const SOURCE_CORE_SIGNAL_CONFIG: &str = "core-signal-config";
/// Source token: the core WG device's own outer socket (`wg_peers[].endpoint`,
/// §4.0) — the host-side token of the same fact, reused verbatim.
pub const SOURCE_WG_DEVICE_OUTER_SOCKET: &str = "wg-device-outer-socket";
/// Source token: the management address core actually dialled. §4.0 unions the
/// shell set with it; D7 did not name a token for this core fallback, so the
/// dialled-address provenance gets its own (recorded as a design deviation).
pub const SOURCE_CORE_DIAL_ADDRESS: &str = "core-dial-address";

/// Literal absence reason for `turn`: this build has no TURN path (`peer_conn.rs`
/// rejects `turn:` URIs), so a TURN URI in the network map is recorded as a
/// REASONED absence of an optional kind — never a freeze gap (design §4.0).
pub const TURN_REJECTED_REASON: &str = "turn: URI rejected by this build (no turn path)";

/// One endpoint handed in by a source, already resolved by that source (this
/// module does no resolution and no IO).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InjectedEndpoint {
    /// Host as the source knew it (domain or literal address).
    pub host: String,
    pub port: u16,
    /// The `ip:port` the route exclusion installs / the probe targets. Empty
    /// means the source had no separate resolved form; `host:port` is used.
    pub resolved: String,
}

impl InjectedEndpoint {
    /// Endpoint without a separately resolved form (`resolved` = `host:port`).
    pub fn new(host: &str, port: u16) -> Self {
        InjectedEndpoint { host: host.to_string(), port, resolved: String::new() }
    }

    /// Endpoint whose source already resolved it (shell exclusion / dial IP).
    pub fn with_resolved(host: &str, port: u16, resolved: &str) -> Self {
        InjectedEndpoint { host: host.to_string(), port, resolved: resolved.to_string() }
    }

    /// The value frozen into `resolved` (design §3.2: the per-/32 the exclusion
    /// installs).
    fn resolved_value(&self) -> String {
        if self.resolved.trim().is_empty() {
            format!("{}:{}", self.host, self.port)
        } else {
            self.resolved.clone()
        }
    }

    /// A freeze needs both an address and a port; an incomplete injection is
    /// dropped fail-closed (the kind then records a reasoned absence).
    fn usable(&self) -> bool {
        !self.host.trim().is_empty() && self.port != 0
    }
}

/// One entry of the shell-side exclusion set, injected verbatim by the caller
/// (ets shape `{kind,host,port,resolved}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellExclusion {
    pub kind: EndpointKind,
    pub host: String,
    pub port: u16,
    /// `ip:port` the shell resolved and excluded.
    pub resolved: String,
}

impl ShellExclusion {
    /// Shell exclusion without a separately resolved form.
    pub fn new(kind: EndpointKind, host: &str, port: u16) -> Self {
        ShellExclusion { kind, host: host.to_string(), port, resolved: String::new() }
    }

    /// Shell exclusion carrying the shell-resolved address.
    pub fn with_resolved(kind: EndpointKind, host: &str, port: u16, resolved: &str) -> Self {
        ShellExclusion {
            kind,
            host: host.to_string(),
            port,
            resolved: resolved.to_string(),
        }
    }

    fn endpoint(&self) -> InjectedEndpoint {
        InjectedEndpoint {
            host: self.host.clone(),
            port: self.port,
            resolved: self.resolved.clone(),
        }
    }
}

/// Every freeze source of the device collector, injected by the caller
/// (design §4.0 table, one field per row) plus the freeze instant — the clock
/// is injected too, so assembly is reproducible in tests.
#[derive(Debug, Clone, Default)]
pub struct FreezeSources {
    /// Shell-side exclusion set (C1/C2 cross-check input; today 3/7 kinds —
    /// the other 4 come from core, which is the B-2 finding recorded honestly).
    pub shell_exclusions: Vec<ShellExclusion>,
    /// Core's dialled management address (`management_url` in use).
    pub management_dial: Option<InjectedEndpoint>,
    /// `netbird_config.signal` of the sync-delivered network map.
    pub signal_config: Option<InjectedEndpoint>,
    /// `relay_advertised.advertised_urls` (each call re-resolves; a moved
    /// address is a new freeze).
    pub relay_advertised: Vec<InjectedEndpoint>,
    /// Network-map `stuns[].uri`, already parsed to host/port.
    pub stun_uris: Vec<InjectedEndpoint>,
    /// Network-map `turns[].uri`; non-empty means a TURN URI existed and was
    /// rejected by this build (see [`TURN_REJECTED_REASON`]).
    pub turn_uris: Vec<InjectedEndpoint>,
    /// `connector_status().wg.wg_peers[].endpoint` of the live session.
    pub wg_peer_endpoints: Vec<InjectedEndpoint>,
    /// Network-map DNS (`dns.servers[]` + `interface_dns`).
    pub dns_servers: Vec<InjectedEndpoint>,
    /// Freeze instant = arm instant (§4.0): unix clock + `sys::mono_ms()`.
    pub frozen_at_unix: i64,
    pub frozen_at_mono: u64,
}

impl FreezeSources {
    /// Core-side candidates of one kind, as `(endpoint, source token)` pairs in
    /// injected order. `Turn` has none: this build has no TURN path, so a TURN
    /// URI is never a freeze target (the absence reason carries the fact).
    fn core_candidates(&self, kind: EndpointKind) -> Vec<(InjectedEndpoint, &'static str)> {
        let mut out: Vec<(InjectedEndpoint, &'static str)> = Vec::new();
        match kind {
            EndpointKind::Management => {
                if let Some(ep) = &self.management_dial {
                    out.push((ep.clone(), SOURCE_CORE_DIAL_ADDRESS));
                }
            }
            EndpointKind::Signal => {
                if let Some(ep) = &self.signal_config {
                    out.push((ep.clone(), SOURCE_CORE_SIGNAL_CONFIG));
                }
            }
            EndpointKind::Relay => {
                for ep in &self.relay_advertised {
                    out.push((ep.clone(), SOURCE_CORE_RELAY_ADVERTISED));
                }
            }
            EndpointKind::Stun => {
                for ep in &self.stun_uris {
                    out.push((ep.clone(), SOURCE_CORE_NETWORK_MAP));
                }
            }
            EndpointKind::Turn => {}
            EndpointKind::WgPeer => {
                for ep in &self.wg_peer_endpoints {
                    out.push((ep.clone(), SOURCE_WG_DEVICE_OUTER_SOCKET));
                }
            }
            EndpointKind::Dns => {
                for ep in &self.dns_servers {
                    out.push((ep.clone(), SOURCE_CORE_NETWORK_MAP));
                }
            }
        }
        out
    }

    /// Why this kind is not frozen. Always non-empty: a required kind without a
    /// reason would be an unattributable freeze gap for the judge
    /// (`n2h.rs:832-846`), and an optional kind without one is treated as a gap.
    fn absent_reason(&self, kind: EndpointKind) -> String {
        match kind {
            EndpointKind::Turn => {
                if self.turn_uris.is_empty() {
                    "no turn endpoint in this session's network map (optional kind)".to_string()
                } else {
                    TURN_REJECTED_REASON.to_string()
                }
            }
            EndpointKind::Relay => "no relay endpoint: the shell exclusion set has no 'relay' \
                 entry and relay_advertised.advertised_urls is empty (optional kind)"
                .to_string(),
            EndpointKind::Management => "no management endpoint: the shell exclusion set has no \
                 'management' entry and the core dial address was not obtained"
                .to_string(),
            EndpointKind::Signal => "no signal endpoint: the shell exclusion set has no 'signal' \
                 entry and netbird_config.signal is absent"
                .to_string(),
            EndpointKind::Stun => "no stun endpoint: the network map's stuns[] is empty".to_string(),
            EndpointKind::WgPeer => "no wg_peer endpoint: no live session peer endpoint \
                 (fail-closed: the collector is not armed without a ready session)"
                .to_string(),
            EndpointKind::Dns => "no dns endpoint: the network map's dns.servers[] and \
                 interface_dns are empty"
                .to_string(),
        }
    }
}

/// Assemble `frozen_endpoints[]` (design §4.0) from injected sources.
///
/// Invariants, all of which the judge in `n2h.rs` relies on:
/// - every [`EndpointKind`] appears at least once — a kind with no usable
///   source appears as `present:false` WITH an `absent_reason` (the gap is
///   recorded, never omitted);
/// - entries are ordered by [`EndpointKind::ALL`], so identical inputs always
///   render an identical document;
/// - several endpoints of one kind (two resolver addresses, two peers) each get
///   their own entry, in injected order — the freeze is per resolved endpoint;
/// - an endpoint already frozen for that kind at the same `resolved` value is
///   not duplicated, and the shell entry (injected first) keeps the slot;
/// - `turn` is NEVER present (`TURN_REJECTED_REASON`) and a present entry never
///   carries an `absent_reason`.
pub fn assemble_freezes(src: &FreezeSources) -> Vec<FrozenEndpoint> {
    let mut out: Vec<FrozenEndpoint> = Vec::new();
    for kind in EndpointKind::ALL {
        // turn has no freeze path in this build: recording it as present would
        // claim an endpoint the probe sender cannot even reach (peer_conn.rs
        // rejects turn:), so the reasoned absence is the honest shape.
        if kind == EndpointKind::Turn {
            out.push(absent_entry(kind, &src.absent_reason(kind), src));
            continue;
        }
        let mut candidates: Vec<(InjectedEndpoint, &'static str)> = Vec::new();
        for exclusion in src.shell_exclusions.iter().filter(|e| e.kind == kind) {
            candidates.push((exclusion.endpoint(), SOURCE_SHELL_EXCLUSION));
        }
        candidates.extend(src.core_candidates(kind));

        let mut frozen_resolved: Vec<String> = Vec::new();
        for (ep, source) in candidates {
            if !ep.usable() {
                continue;
            }
            let resolved = ep.resolved_value();
            if frozen_resolved.iter().any(|r| *r == resolved) {
                continue;
            }
            frozen_resolved.push(resolved.clone());
            out.push(FrozenEndpoint {
                kind: Some(kind),
                present: true,
                host: ep.host.clone(),
                port: ep.port,
                // transport follows the kind exactly as on the host side; the
                // probe sender (I3) must use the same one
                proto: kind.proto().to_string(),
                source: source.to_string(),
                resolved,
                absent_reason: String::new(),
                frozen_at_unix: src.frozen_at_unix,
                frozen_at_mono: src.frozen_at_mono,
            });
        }
        if frozen_resolved.is_empty() {
            out.push(absent_entry(kind, &src.absent_reason(kind), src));
        }
    }
    out
}

/// `present:false` entry: value fields stay empty exactly like the host-side
/// `push_absent` shape (`n2h.rs:1844-1853`).
fn absent_entry(kind: EndpointKind, reason: &str, src: &FreezeSources) -> FrozenEndpoint {
    FrozenEndpoint {
        kind: Some(kind),
        present: false,
        absent_reason: reason.to_string(),
        frozen_at_unix: src.frozen_at_unix,
        frozen_at_mono: src.frozen_at_mono,
        ..FrozenEndpoint::default()
    }
}

// ---------------------------------------------------------------------------
// I3 — device probe sender (design §4.2)
// ---------------------------------------------------------------------------

/// The wildcard bind of every device probe socket (design §4.2: `0.0.0.0:0`,
/// one fresh socket per probe).
///
/// Deliberately NOT the host sender's `127.0.0.1:0` (`n2h.rs:1135`): the
/// device-validation run-2 regression was a loopback-bound probe socket whose
/// every send to a real, non-loopback outer endpoint failed with EINVAL
/// (`n2h.rs:1163-1170`, regression test `n2h.rs:2373-2396`), and the design
/// freezes the wildcard for the device path.
pub const PROBE_BIND_WILDCARD: &str = "0.0.0.0:0";

/// Wait window for a probe's response-side receipt. The arm options carry the
/// deployed value (`opts.probe_wait_ms`, design §5.1 default 800); this is the
/// value for a caller that has none.
pub const PROBE_WAIT_MS_DEFAULT: u64 = 800;

/// TCP probe connect timeout — the host probe's bound (`n2h.rs:1147`), so a
/// probe against a black-holed endpoint fails inside the same bounded time
/// instead of stalling the window.
pub const PROBE_CONNECT_TIMEOUT_MS: u64 = 2_000;

/// One probe as it left the App process (the `probes[]`取数层 of design §3.2).
///
/// No clock is taken here: the caller stamps `sent_at_unix`/`sent_at_mono` at
/// its own call site, exactly like the host driver does (`n2h.rs:1383-1384`).
/// `Err` from any sender means the probe never left — the caller records it as
/// `sent:false` plus a reason, never as a silent miss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentProbe {
    /// `N2H-<kind>-<nonce>` (built by [`probe_id`], shared with the host); for
    /// the STUN probe `N2H-stun-<txid hex>` — the txid IS the unique payload
    /// there, because the Binding Request wire format is fixed.
    pub probe_id: String,
    /// The probe's own five-tuple. Unique because every probe leaves from its
    /// OWN fresh ephemeral socket (design §4.2); the `src` is the probe
    /// socket's getsockname — `0.0.0.0:<port>` for the UDP probes, the real
    /// source for TCP (see [`wildcard_udp`] for why the wildcard address is
    /// reported verbatim rather than reverse-engineered).
    pub tuple: FiveTuple,
    /// Unique on-the-wire marker: the ASCII probe id, or the 12-byte STUN
    /// transaction id ([`WireMarker`], the same match the TUN scanner uses).
    pub marker: WireMarker,
    pub payload_len: usize,
    /// STUN only: the endpoint-reported physical source from a txid-matched
    /// Binding Success Response (design §4.0 `stun-binding-response`); `None`
    /// means no receipt inside the wait window — recorded as a gap, never
    /// invented.
    pub mapped: Option<([u8; 4], u16)>,
    /// DNS echo only (default OFF): the resolver carried our qname back.
    pub echo_received: bool,
}

/// Bind one fresh WILDCARD UDP socket for a probe (design §4.2: `0.0.0.0:0`,
/// never the host's loopback bind).
///
/// The socket stays UNCONNECTED on purpose: the design matches a receipt by the
/// probe's unique payload (the STUN txid, §4.2/§4.0) and ignores the reply's
/// origin, so `recv_from` is used. A connected socket would add a kernel-level
/// source-address filter the design does not have — it could silently drop a
/// legitimate response from a multi-homed or anycast endpoint and turn a real
/// receipt into a gap.
///
/// The consequence is that this socket's `local_addr` is
/// `0.0.0.0:<ephemeral port>`: the address is the wildcard bind (the PORT is
/// what makes the probe's five-tuple unique). That is the same shape the host's
/// wildcard-bound STUN sender already reports (`n2h.rs:1169-1171`), and the
/// design prescribes exactly this getsockname reading for the wg_peer receipt's
/// `physical_src` (§4.0, "probe socket getsockname", host shape
/// `n2h.rs:1474-1486`). The address a packet actually carries is the
/// kernel-chosen route source; it is deliberately NOT reverse-engineered here
/// (which would cost a second, connected socket per probe) — whichever source
/// the endpoint sees is what the receipt side reports, as the design asks.
fn wildcard_udp() -> Result<std::net::UdpSocket, String> {
    std::net::UdpSocket::bind(PROBE_BIND_WILDCARD).map_err(|e| format!("probe socket bind: {e}"))
}

/// Send one ASCII-marker UDP probe from a fresh wildcard-bound socket
/// (design §4.2: the payload IS the probe id).
pub fn send_udp_probe(dst: SocketAddr, marker: &str) -> Result<SentProbe, String> {
    let sock = wildcard_udp()?;
    let src = sock.local_addr().map_err(|e| format!("probe socket addr: {e}"))?;
    let n = sock.send_to(marker.as_bytes(), dst).map_err(|e| format!("probe send: {e}"))?;
    Ok(SentProbe {
        probe_id: marker.to_string(),
        tuple: FiveTuple { proto: "udp".into(), src: src.to_string(), dst: dst.to_string() },
        marker: WireMarker::Ascii(marker.to_string()),
        payload_len: n,
        mapped: None,
        echo_received: false,
    })
}

/// Send one ASCII-marker TCP probe (connect + write + shutdown, the host
/// shapes at `n2h.rs:1145-1154`). The connect is what gives every TCP probe its
/// own source port; the write succeeds only for a listener that actually
/// accepted, which is the outer-path reachability this probe asserts.
pub fn send_tcp_probe(dst: SocketAddr, marker: &str) -> Result<SentProbe, String> {
    use std::io::Write;
    let stream = std::net::TcpStream::connect_timeout(
        &dst,
        std::time::Duration::from_millis(PROBE_CONNECT_TIMEOUT_MS),
    )
    .map_err(|e| format!("probe connect: {e}"))?;
    let src = stream.local_addr().map_err(|e| format!("probe addr: {e}"))?;
    let mut s = stream;
    s.write_all(marker.as_bytes()).map_err(|e| format!("probe write: {e}"))?;
    let _ = s.shutdown(std::net::Shutdown::Write);
    Ok(SentProbe {
        probe_id: marker.to_string(),
        tuple: FiveTuple { proto: "tcp".into(), src: src.to_string(), dst: dst.to_string() },
        marker: WireMarker::Ascii(marker.to_string()),
        payload_len: marker.len(),
        mapped: None,
        echo_received: false,
    })
}

/// Send one STUN Binding probe and wait up to `wait_ms` for the txid-matched
/// Binding Success Response (design §4.2: the 12-byte transaction id is the
/// unique payload; §4.0: the response's XOR-MAPPED-ADDRESS is the
/// endpoint-reported physical source).
///
/// The wait is a bounded blocking receive (`set_read_timeout`), not a poll
/// loop: a datagram that never comes costs the window exactly `wait_ms`.
pub fn send_stun_probe(
    dst: SocketAddr,
    txid: [u8; 12],
    wait_ms: u64,
) -> Result<SentProbe, String> {
    use crate::stun::{build_binding_request, parse_binding_response, StunReply, TransactionId};
    let sock = wildcard_udp()?;
    let src = sock.local_addr().map_err(|e| format!("probe socket addr: {e}"))?;
    let _ = sock.set_read_timeout(Some(std::time::Duration::from_millis(wait_ms.max(1))));
    let req = build_binding_request(TransactionId(txid));
    sock.send_to(&req, dst).map_err(|e| format!("stun probe send: {e}"))?;
    let mut mapped = None;
    let mut buf = [0u8; 2048];
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
    while std::time::Instant::now() < deadline {
        match sock.recv_from(&mut buf) {
            Ok((n, _from)) => {
                if let Ok(StunReply::Mapped { addr, port }) =
                    parse_binding_response(&buf[..n], &TransactionId(txid))
                {
                    mapped = Some((addr, port));
                    break;
                }
                // mismatched datagram: keep waiting until the deadline
            }
            // timed out / would block: the bounded wait is over
            Err(_) => break,
        }
    }
    Ok(SentProbe {
        probe_id: probe_id("stun", &crate::util::hex_lower(&txid)),
        tuple: FiveTuple { proto: "udp".into(), src: src.to_string(), dst: dst.to_string() },
        marker: WireMarker::StunTxid(txid),
        payload_len: req.len(),
        mapped,
        echo_received: false,
    })
}

/// One outer probe of `kind` against the frozen endpoint `dst` (design §4.2),
/// with the identity and entropy this module owns: a fresh 24-hex nonce from
/// `/dev/urandom` ([`crate::n2h::fresh_nonce_hex`]) for the ASCII probes, a
/// fresh STUN transaction id for `stun`.
///
/// The transport follows `kind.proto()` — the same column the freeze recorded
/// (`FrozenEndpoint::proto`) — so the probe cannot silently travel on a
/// transport the frozen set did not declare.
///
/// `dns` deliberately takes the SAME ASCII UDP-marker shape as the other UDP
/// kinds: the unverified `dns-nxdomain-echo` method stays behind
/// [`send_dns_echo_probe`] and is never the default path (design §4.0/§9-U5).
pub fn send_probe(
    kind: EndpointKind,
    dst: SocketAddr,
    wait_ms: u64,
) -> Result<SentProbe, String> {
    if kind == EndpointKind::Stun {
        let txid = crate::stun::random_transaction_id()
            .map_err(|e| format!("stun probe entropy: {e:?}"))?;
        return send_stun_probe(dst, txid.0, wait_ms);
    }
    let nonce = crate::n2h::fresh_nonce_hex()?;
    let id = probe_id(kind.name(), &nonce);
    match kind.proto() {
        "tcp" => send_tcp_probe(dst, &id),
        _ => send_udp_probe(dst, &id),
    }
}

/// Suffix of the optional DNS echo probe's qname (design §4.0: the query name
/// is `<marker>.n2h-invalid`, so a resolver echoing the QNAME back proves it
/// saw our query).
pub const DNS_ECHO_SUFFIX: &str = ".n2h-invalid";

/// The qname of one DNS echo probe. The caller passes the probe id
/// (`N2H-<kind>-<nonce>`, `[A-Za-z0-9-]`, ≤ 33 octets), so the joined first
/// label stays inside the 63-octet DNS label limit.
pub fn dns_echo_qname(marker: &str) -> String {
    format!("{marker}{DNS_ECHO_SUFFIX}")
}

/// DNS wire name: length-prefixed labels plus the root terminator. `None` for
/// an empty label, a label over 63 octets, a name over 255 octets, or a byte
/// outside the host-name alphabet (plus the underscore seen in the wild) — a
/// malformed query is never sent.
fn dns_wire_name(qname: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for label in qname.split('.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        if !label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return None;
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    if out.len() > 255 {
        return None;
    }
    Some(out)
}

/// Query id of one DNS echo probe: FNV-1a 16-bit of the qname. Deterministic
/// (a pure function of the qname) so a receipt is reproducible in a test; it
/// only correlates a reply with our query — the receipt itself is the echoed
/// QNAME.
fn dns_query_id(qname: &str) -> u16 {
    let mut h: u32 = 0x811c_9dc5;
    for b in qname.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    (h & 0xffff) as u16
}

/// The A-query wire form of `qname` (RD set, one question, no EDNS).
fn build_dns_a_query(qname: &str) -> Option<Vec<u8>> {
    let name = dns_wire_name(qname)?;
    let mut q = Vec::with_capacity(12 + name.len() + 4);
    q.extend_from_slice(&dns_query_id(qname).to_be_bytes());
    q.extend_from_slice(&0x0100u16.to_be_bytes()); // flags: RD
    q.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    q.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    q.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    q.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    q.extend_from_slice(&name);
    q.extend_from_slice(&1u16.to_be_bytes()); // QTYPE A
    q.extend_from_slice(&1u16.to_be_bytes()); // QCLASS IN
    Some(q)
}

/// The receipt test of the DNS echo method: a datagram that is a DNS RESPONSE
/// (QR set) for our query id AND carries our qname in wire form. The question
/// section is never name-compressed (RFC 1035 §4.1.4), so a verbatim wire-form
/// search is a sound "the resolver echoed this QNAME back" test. The RCODE is
/// deliberately not required: an NXDOMAIN answer proves delivery exactly as
/// well as a NOERROR one (design §4.0).
pub fn dns_echo_receipt(response: &[u8], qname: &str) -> bool {
    if response.len() < 12 || response[2] & 0x80 == 0 {
        return false;
    }
    if u16::from_be_bytes([response[0], response[1]]) != dns_query_id(qname) {
        return false;
    }
    match dns_wire_name(qname) {
        Some(name) => crate::n2h::frame_contains_marker(response, &name),
        None => false,
    }
}

/// Send one DNS echo probe (design §4.0 `dns-nxdomain-echo`; **default OFF** —
/// no real resolver has been measured with this method, §9-U5) and wait up to
/// `wait_ms` for a response carrying our qname back. Enabling it is the
/// caller's explicit decision, and a miss stays a receipt gap (§4.0), never a
/// pass.
///
/// The marker appears verbatim inside the query (`<marker>.n2h-invalid`), so
/// the same [`WireMarker::Ascii`] match that guards the other probes also
/// detects this query if it leaks into the tunnel.
pub fn send_dns_echo_probe(
    dst: SocketAddr,
    marker: &str,
    wait_ms: u64,
) -> Result<SentProbe, String> {
    let qname = dns_echo_qname(marker);
    let query = build_dns_a_query(&qname)
        .ok_or_else(|| format!("dns echo qname '{qname}' is not encodable"))?;
    let sock = wildcard_udp()?;
    let src = sock.local_addr().map_err(|e| format!("probe socket addr: {e}"))?;
    let _ = sock.set_read_timeout(Some(std::time::Duration::from_millis(wait_ms.max(1))));
    sock.send_to(&query, dst).map_err(|e| format!("dns echo probe send: {e}"))?;
    let mut echo_received = false;
    let mut buf = [0u8; 2048];
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
    while std::time::Instant::now() < deadline {
        match sock.recv_from(&mut buf) {
            Ok((n, _from)) => {
                if dns_echo_receipt(&buf[..n], &qname) {
                    echo_received = true;
                    break;
                }
            }
            Err(_) => break,
        }
    }
    Ok(SentProbe {
        probe_id: marker.to_string(),
        tuple: FiveTuple { proto: "udp".into(), src: src.to_string(), dst: dst.to_string() },
        marker: WireMarker::Ascii(marker.to_string()),
        payload_len: query.len(),
        mapped: None,
        echo_received,
    })
}

// ---------------------------------------------------------------------------
// I4 — /proc reconciliation (design §4.4, the B-6 landing)
// ---------------------------------------------------------------------------

/// The platform TUN interface name — ONE constant for the three places the
/// design freezes the same literal: `tun_negative.scanned_sides` (D3),
/// `counters[].side` (D8) and the `/proc/net/dev` row (B-6).
pub const TUN_IFACE_NAME: &str = "vpn-tun";

/// The kernel-side ledger (§4.4's independent second book).
pub const PROC_NET_DEV_PATH: &str = "/proc/net/dev";

/// `counters-source=` token: the /proc ledger read in-process (the device path).
pub const COUNTERS_SOURCE_PROC: &str = "proc-net-dev";
/// `counters-source=` token: the operator's `hdc shell "cat /proc/net/dev"`
/// capture — the D-class fallback when the App cannot read `/proc` (§9-U1).
pub const COUNTERS_SOURCE_HDC_FALLBACK: &str = "hdc-proc-fallback";
/// `counters-source=` token: the I2 tee totals standing in for `/proc` in the
/// OFFLINE harness only (§3.2; never a device-run caliber).
pub const COUNTERS_SOURCE_TEE_FALLBACK: &str = "tee-fallback";

/// Bounded resampling bound (design §4.4: "有界重采样 ≤3 次"; a pairing that is
/// still inconsistent after this many extra reads is recorded, never repaired).
pub const MAX_PAIRING_RESAMPLES: u32 = 3;

/// The four ledger counters of one `/proc/net/dev` row (the only ones the
/// identity needs; errors/drops stay in the file, unread).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcIfaceRow {
    pub rx_bytes: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub tx_packets: u64,
}

/// Parse the `iface` row out of `/proc/net/dev` text (fixture-injectable: the
/// caller supplies the text, so the unit tests never read the real file).
///
/// `None` when the interface has no row OR its row is unusable (< 16 numeric
/// fields): both mean "no kernel ledger", and the caller records the gap
/// rather than reconciling against a guess. The name must match EXACTLY after
/// trimming (`vpn-tun0` / `xvpn-tun` are different interfaces).
pub fn parse_proc_net_dev(text: &str, iface: &str) -> Option<ProcIfaceRow> {
    for line in text.lines() {
        let (name, rest) = match line.split_once(':') {
            Some(v) => v,
            None => continue, // header / separator lines carry no ':'
        };
        if name.trim() != iface {
            continue;
        }
        let nums: Vec<u64> =
            rest.split_whitespace().filter_map(|f| f.parse::<u64>().ok()).collect();
        if nums.len() < 16 {
            return None; // matched but malformed: fail closed, do not guess
        }
        // /proc/net/dev column order: RX bytes, RX packets, …(6 more)… then TX
        return Some(ProcIfaceRow {
            rx_bytes: nums[0],
            rx_packets: nums[1],
            tx_bytes: nums[8],
            tx_packets: nums[9],
        });
    }
    None
}

/// The read seam (design §4.4 + §8). Production reads the real
/// `/proc/net/dev` and, when that fails, the operator-provisioned `hdc`
/// capture; tests inject fixture TEXT, so no unit test depends on the real
/// `/proc` (design §8's injectable-seam rule).
pub trait LedgerReads {
    /// Direct in-process read (the App path). `Err` when permission or
    /// availability fails — §9-U1 is an UNVERIFIED platform fact and is
    /// handled as a real failure, never as a guess.
    fn read_proc_net_dev(&mut self) -> Result<String, String>;
    /// The operator's `hdc shell "cat /proc/net/dev"` capture text (D-class
    /// fallback, design §2.4-D/§4.4). `Err` when none was provisioned.
    fn read_proc_hdc_capture(&mut self) -> Result<String, String>;
    /// The core WG device counters (the `wg{}` block's source) — read SECOND
    /// (design §4.4 pairing order).
    fn read_wg_stats(&mut self) -> Result<WgDeviceStats, String>;
}

/// Production ledgers. The `/proc` read is direct; the fallback is a file the
/// operator dropped on the device (`hdc file send`, §6.4-5), `None` when not
/// provisioned; the wg ledger comes from a caller closure (I5 passes the
/// connector's device stats handle — this module never reads the connector).
pub struct DeviceLedgerReads<F> {
    /// Device path of the operator capture (e.g.
    /// `/data/local/tmp/netbird-n2h-proc-net-dev`).
    pub hdc_capture_path: Option<std::path::PathBuf>,
    /// Reads the live [`WgDeviceStats`].
    pub wg_stats: F,
}

impl<F: FnMut() -> Result<WgDeviceStats, String>> LedgerReads for DeviceLedgerReads<F> {
    fn read_proc_net_dev(&mut self) -> Result<String, String> {
        std::fs::read_to_string(PROC_NET_DEV_PATH).map_err(|e| format!("{PROC_NET_DEV_PATH}: {e}"))
    }

    fn read_proc_hdc_capture(&mut self) -> Result<String, String> {
        let path = self
            .hdc_capture_path
            .as_ref()
            .ok_or_else(|| "no hdc /proc capture provisioned".to_string())?;
        std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn read_wg_stats(&mut self) -> Result<WgDeviceStats, String> {
        let f = &mut self.wg_stats;
        f()
    }
}

/// Read the `vpn-tun` row through the two allowed paths, in order: the direct
/// read first, the `hdc` capture second (design §4.4). Returns the row plus
/// the `counters-source=` token the caller records in `notes[]`.
///
/// Both paths failing is an `Err` carrying both reasons: no counters window
/// exists then, and the caller records that gap (`n2h-inconclusive`,
/// `n2h.rs:904-910`) instead of reconciling against nothing.
pub fn read_proc_row<R: LedgerReads + ?Sized>(
    reads: &mut R,
) -> Result<(ProcIfaceRow, &'static str), String> {
    let direct = reads.read_proc_net_dev();
    if let Ok(text) = &direct {
        if let Some(row) = parse_proc_net_dev(text, TUN_IFACE_NAME) {
            return Ok((row, COUNTERS_SOURCE_PROC));
        }
    }
    let direct_note = match &direct {
        Ok(_) => format!("{PROC_NET_DEV_PATH} carries no usable '{TUN_IFACE_NAME}' row"),
        Err(e) => format!("{PROC_NET_DEV_PATH} read failed: {e}"),
    };
    match reads.read_proc_hdc_capture() {
        Ok(text) => parse_proc_net_dev(&text, TUN_IFACE_NAME)
            .map(|row| (row, COUNTERS_SOURCE_HDC_FALLBACK))
            .ok_or_else(|| {
                format!("{direct_note}; the hdc capture has no usable '{TUN_IFACE_NAME}' row either")
            }),
        Err(e) => Err(format!("{direct_note}; the hdc capture is unavailable: {e}")),
    }
}

/// One paired read of the two ledgers, in the FROZEN order (design §4.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PairedRead {
    pub at_mono: u64,
    pub at_unix: i64,
    /// The `/proc/net/dev` `vpn-tun` row — the kernel ledger, read FIRST.
    pub tun: ProcIfaceRow,
    /// The core WG counters — read SECOND.
    pub wg: WgDeviceStats,
    /// Which `/proc` path answered (`counters-source=` token).
    pub source: &'static str,
}

impl PairedRead {
    /// The document-side snapshot of this read (the D8/D9 caliber).
    pub fn snapshot(&self) -> IfaceSnapshot {
        iface_snapshot(&self.wg, &self.tun, self.at_mono, self.at_unix)
    }
}

/// Build the document-side counter snapshot (design D8/D9) from a wg-ledger
/// reading plus a `/proc`-shaped row — the SAME mapping the offline harness's
/// `counters-source=tee-fallback` caliber uses (§3.2), so the device path and
/// the offline path cannot drift:
/// `delivered_bytes`/`delivered_frames` = /proc RX, `injected_frames` = /proc
/// TX packets (D9: in a real system the OS injects into the TUN, not the
/// collector).
pub fn iface_snapshot(
    wg: &WgDeviceStats,
    tun: &ProcIfaceRow,
    at_mono: u64,
    at_unix: i64,
) -> IfaceSnapshot {
    IfaceSnapshot {
        at_mono,
        at_unix,
        wg_tx_packets: wg.tx_packets,
        wg_tx_bytes: wg.tx_bytes,
        wg_rx_packets: wg.rx_packets,
        wg_rx_bytes_to_tun: wg.rx_bytes_to_tun,
        wg_handshake_initiations: wg.handshake_initiations,
        wg_no_route_drops: wg.no_route_drops,
        wg_unknown_peer_drops: wg.unknown_peer_drops,
        wg_decrypt_errors: wg.decrypt_errors,
        wg_send_errors: wg.send_errors,
        wg_tun_write_errors: wg.tun_write_errors,
        tun_injected_frames: tun.tx_packets,
        tun_delivered_frames: tun.rx_packets,
        tun_delivered_bytes: tun.rx_bytes,
    }
}

/// Map the I2 tee totals onto the `/proc`-shaped row — the OFFLINE harness's
/// substitute when `/proc` is unavailable (design §3.2: "`tun.*` 以 tee 独立
/// 计数替位"). Direction mapping follows D9: `from-os` = OS→TUN = injected,
/// `to-os` = core→TUN delivered.
pub fn tee_fallback_row(totals: &TunTeeTotals) -> ProcIfaceRow {
    ProcIfaceRow {
        rx_bytes: totals.to_os_bytes,
        rx_packets: totals.to_os_frames,
        tx_bytes: totals.from_os_bytes,
        tx_packets: totals.from_os_frames,
    }
}

/// The `notes[]` line naming the ledger path that answered (design §4.4/§3.2:
/// the fallback caliber must be declared, never implied).
pub fn counters_source_note(source: &str) -> String {
    format!("counters-source={source}")
}

/// The two reconciliation identities of design §4.4 on one window delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterIdentities {
    /// The FROZEN identity (`RECONCILIATION_NOTE`, `WindowCounters::reconciled`,
    /// `n2h.rs:560-563`): `delta.wg.rx_bytes_to_tun == delta.tun.delivered_bytes`.
    pub bytes: bool,
    /// The design's STRENGTHENING (§4.4/B-6): `delta.wg.rx_packets ==
    /// delta.tun.delivered_frames` — the self-consistency check of the paired
    /// snapshot, which the byte identity alone can miss when frames of unequal
    /// length cross the pairing gap.
    pub packets: bool,
}

impl CounterIdentities {
    /// Both identities hold — the pairing is self-consistent.
    pub fn hold(&self) -> bool {
        self.bytes && self.packets
    }
}

/// Evaluate both identities on one delta.
pub fn counter_identities(delta: &IfaceSnapshot) -> CounterIdentities {
    CounterIdentities {
        bytes: delta.wg_rx_bytes_to_tun == delta.tun_delivered_bytes,
        packets: delta.wg_rx_packets == delta.tun_delivered_frames,
    }
}

/// One paired read of the two ledgers in the frozen order: the `/proc/net/dev`
/// row FIRST, the wg stats SECOND (design §4.4).
///
/// The order is part of the pairing protocol. The two reads are µs–ms apart, so
/// a frame delivered to the TUN between them lands in one ledger's delta and
/// not the other's; [`close_window`] resamples exactly that, and freezing the
/// order keeps a captured session reproducible.
pub fn paired_read<R: LedgerReads + ?Sized>(
    reads: &mut R,
    at_mono: u64,
    at_unix: i64,
) -> Result<PairedRead, String> {
    let (tun, source) = read_proc_row(reads)?;
    let wg = reads.read_wg_stats().map_err(|e| format!("wg stats read: {e}"))?;
    Ok(PairedRead { at_mono, at_unix, tun, wg, source })
}

/// Window OPEN: one paired read = the `before` edge. No delta exists yet, so
/// there is nothing to resample here.
pub fn open_window<R: LedgerReads + ?Sized>(
    reads: &mut R,
    at_mono: u64,
    at_unix: i64,
) -> Result<PairedRead, String> {
    paired_read(reads, at_mono, at_unix)
}

/// One completed counters window (design §4.4): THE `counters[]` element plus
/// the pairing audit the caller records.
#[derive(Debug, Clone)]
pub struct WindowSample {
    /// Straight into `Evidence::counters` (I5); `side` = [`TUN_IFACE_NAME`]
    /// (D8: one element, one platform TUN).
    pub counters: WindowCounters,
    /// Identity status of the FINAL paired delta.
    pub identities: CounterIdentities,
    /// Extra paired reads of the close edge (bound: ≤ [`MAX_PAIRING_RESAMPLES`]).
    pub resamples: u32,
    /// `counters-source=` token of the accepted read.
    pub source: &'static str,
}

impl WindowSample {
    /// Both identities hold on the final delta — reconciled in the design's
    /// strengthened sense (bytes AND packets).
    pub fn consistent(&self) -> bool {
        self.identities.hold()
    }
}

/// Window CLOSE (design §4.4 pairing discipline): take the `after` paired read
/// and audit its delta against `before`; if either identity fails — a frame
/// crossed between the two reads of this edge — re-take the WHOLE edge, at most
/// [`MAX_PAIRING_RESAMPLES`] extra times, and accept the first self-consistent
/// pairing.
///
/// A pairing that never becomes self-consistent is RETURNED, not repaired:
/// `identities` says which identity broke and `resamples` how many bounded
/// attempts were spent, so I5 records `reconciled:false` honestly (§4.4: 不平
/// 即如实 inconclusive，不修补、不重试到平为止).
///
/// `at_mono`/`at_unix` are the edge instant taken by the caller: the re-reads
/// happen inside the same edge (µs–ms scale) and do not define a new one.
pub fn close_window<R: LedgerReads + ?Sized>(
    reads: &mut R,
    before: &PairedRead,
    at_mono: u64,
    at_unix: i64,
) -> Result<WindowSample, String> {
    let mut resamples = 0u32;
    loop {
        let read = paired_read(reads, at_mono, at_unix)?;
        let counters = WindowCounters {
            side: TUN_IFACE_NAME.to_string(),
            before: before.snapshot(),
            after: read.snapshot(),
        };
        let identities = counter_identities(&counters.delta());
        if identities.hold() || resamples >= MAX_PAIRING_RESAMPLES {
            return Ok(WindowSample { counters, identities, resamples, source: read.source });
        }
        resamples += 1;
    }
}

// ---------------------------------------------------------------------------
// tests — I0/I1 logic is pure; the I3/I4 tests use loopback sockets and
// fixture text only (no real /proc, no device, no clock)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- I0 ----

    #[test]
    fn chunk_roundtrip_reassembles_document_at_slice_boundaries() {
        let doc: Vec<u8> = (0..1300u32).map(|i| (i % 251) as u8).collect();
        let chunks = encode_evidence(&doc);
        assert_eq!(chunks.len(), 3, "512 + 512 + 276 raw bytes");
        assert_eq!(
            chunks.iter().map(|c| c.b64.len()).collect::<Vec<_>>(),
            vec![684, 684, 368],
            "base64 expansion per slice"
        );
        let digest = sha256_hex(&doc);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.seq, i);
            assert_eq!(chunk.total, 3);
            assert_eq!(chunk.sha256, digest, "the FULL-document digest is on every line");
        }
        let lines: Vec<String> = chunks.iter().map(|c| c.line()).collect();
        assert!(lines[0].starts_with(&format!("{CHUNK_MARKER}|seq=0/3|sha256=")), "{}", lines[0]);
        let refs: Vec<&str> = lines.iter().map(|l| l.as_str()).collect();
        assert_eq!(decode_evidence(&refs).expect("reassembles"), doc);
        // capture order does not matter: seq carries the position
        assert_eq!(decode_evidence(&[refs[2], refs[0], refs[1]]).expect("reassembles"), doc);

        // slice boundaries: empty / exactly one slice / one byte over
        let empty = encode_evidence(&[]);
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].b64, "");
        assert_eq!(empty[0].line(), empty[0].line());
        assert!(decode_evidence(&[empty[0].line().as_str()]).expect("empty doc").is_empty());
        assert_eq!(encode_evidence(&[7u8; CHUNK_PAYLOAD_BYTES]).len(), 1);
        assert_eq!(encode_evidence(&[7u8; CHUNK_PAYLOAD_BYTES + 1]).len(), 2);
        for len in [1usize, 2, 3, 4, 511, 512, 513, 1024, 1025] {
            let doc = vec![0x5Au8; len];
            let lines: Vec<String> = encode_evidence(&doc).iter().map(|c| c.line()).collect();
            let refs: Vec<&str> = lines.iter().map(|l| l.as_str()).collect();
            assert_eq!(decode_evidence(&refs).expect("roundtrip"), doc, "len={len}");
        }

        // real output for the acceptance transcript
        println!("[I0] total={} first-line={}", chunks.len(), lines[0]);
        println!("[I0] reassembled {} bytes, sha256={digest}", doc.len());
    }

    #[test]
    fn chunk_sha256_detects_payload_and_digest_tampering() {
        let doc: Vec<u8> = (0..700u32).map(|i| (i % 97) as u8).collect();
        let chunks = encode_evidence(&doc);
        assert_eq!(chunks.len(), 2);
        let lines: Vec<String> = chunks.iter().map(|c| c.line()).collect();
        let digest = sha256_hex(&doc);

        // mutate one payload byte (still valid base64, different document)
        let mut tampered = lines[1].clone();
        let b64_at = tampered.find("|b64=").expect("b64 field") + "|b64=".len();
        let flipped = if tampered.as_bytes()[b64_at] == b'A' { 'B' } else { 'A' };
        tampered.replace_range(b64_at..b64_at + 1, &flipped.to_string());
        match decode_evidence(&[lines[0].as_str(), tampered.as_str()]) {
            Err(ChunkError::Sha256Mismatch { declared, actual }) => {
                assert_eq!(declared, digest);
                assert_ne!(actual, digest);
            }
            other => panic!("payload tampering must be detected, got {other:?}"),
        }

        // mutate the declared digest on ONE of two lines → lines disagree
        let mut digest_tampered = lines[1].clone();
        let sha_at = digest_tampered.find("sha256=").expect("sha field") + "sha256=".len();
        let other = if digest_tampered.as_bytes()[sha_at] == b'0' { '1' } else { '0' };
        digest_tampered.replace_range(sha_at..sha_at + 1, &other.to_string());
        assert_eq!(
            decode_evidence(&[lines[0].as_str(), digest_tampered.as_str()]).unwrap_err(),
            ChunkError::DigestDisagreement { seq: 1 }
        );

        // single-chunk document with a mutated declared digest → recomputed
        // digest disagrees, i.e. the claim is refused, not repaired
        let single = encode_evidence(&doc[..100]);
        let mut line = single[0].line();
        let sha_at = line.find("sha256=").expect("sha field") + "sha256=".len();
        let other = if line.as_bytes()[sha_at] == b'0' { '1' } else { '0' };
        line.replace_range(sha_at..sha_at + 1, &other.to_string());
        assert!(matches!(
            decode_evidence(&[line.as_str()]).unwrap_err(),
            ChunkError::Sha256Mismatch { .. }
        ));
    }

    #[test]
    fn chunk_reassembly_rejects_missing_duplicate_and_malformed_lines() {
        let doc: Vec<u8> = (0..1300u32).map(|i| (i % 13) as u8).collect();
        let chunks = encode_evidence(&doc);
        let lines: Vec<String> = chunks.iter().map(|c| c.line()).collect();
        let (l0, l1, l2) = (lines[0].as_str(), lines[1].as_str(), lines[2].as_str());

        // dropped middle line / dropped last line → the exact missing seq
        assert_eq!(decode_evidence(&[l0, l2]).unwrap_err(), ChunkError::Missing { seq: 1 });
        assert_eq!(decode_evidence(&[l0, l1]).unwrap_err(), ChunkError::Missing { seq: 2 });
        // repeated line → capture is not one run
        assert_eq!(
            decode_evidence(&[l0, l0, l1, l2]).unwrap_err(),
            ChunkError::Duplicate { seq: 0 }
        );
        // spliced captures: total and full-document digest must agree
        let total_splice = lines[1].replace("seq=1/3", "seq=1/4");
        assert_eq!(
            decode_evidence(&[l0, total_splice.as_str()]).unwrap_err(),
            ChunkError::TotalMismatch { first: 3, other: 4 }
        );
        // nothing captured / foreign marker / invalid payload / out-of-range seq
        assert_eq!(decode_evidence(&[]).unwrap_err(), ChunkError::NoChunks);
        assert!(matches!(
            decode_evidence(&["N1BDISC_CHUNK|stream=dlerror|index=0|count=1"]).unwrap_err(),
            ChunkError::MalformedLine(_)
        ));
        let bad_b64 = format!("{CHUNK_MARKER}|seq=0/1|sha256={}|b64=***", sha256_hex(&doc));
        assert_eq!(
            decode_evidence(&[bad_b64.as_str()]).unwrap_err(),
            ChunkError::BadBase64 { seq: 0 }
        );
        let oob = format!("{CHUNK_MARKER}|seq=3/3|sha256={}|b64=", sha256_hex(&doc));
        assert!(matches!(
            decode_evidence(&[oob.as_str()]).unwrap_err(),
            ChunkError::MalformedLine(_)
        ));
    }

    // ---- I1 ----

    fn all_seven_sources() -> FreezeSources {
        FreezeSources {
            shell_exclusions: vec![
                ShellExclusion::with_resolved(
                    EndpointKind::Management,
                    "mgmt.example",
                    443,
                    "203.0.113.7:443",
                ),
                ShellExclusion::with_resolved(
                    EndpointKind::Signal,
                    "signal.example",
                    10000,
                    "203.0.113.8:10000",
                ),
                ShellExclusion::with_resolved(
                    EndpointKind::Relay,
                    "relay.example",
                    28443,
                    "203.0.113.9:28443",
                ),
            ],
            management_dial: Some(InjectedEndpoint::with_resolved(
                "203.0.113.7",
                443,
                "203.0.113.7:443",
            )),
            signal_config: None,
            relay_advertised: vec![InjectedEndpoint::with_resolved(
                "relay.example",
                28443,
                "203.0.113.9:28443",
            )],
            stun_uris: vec![InjectedEndpoint::new("stun.example", 3478)],
            turn_uris: vec![InjectedEndpoint::new("turn.example", 3478)],
            wg_peer_endpoints: vec![InjectedEndpoint::new("198.51.100.4", 51820)],
            dns_servers: vec![
                InjectedEndpoint::new("10.0.0.53", 53),
                InjectedEndpoint::new("10.0.0.54", 53),
            ],
            frozen_at_unix: 1_756_000_000,
            frozen_at_mono: 123_456,
        }
    }

    #[test]
    fn freeze_assembly_covers_all_seven_kinds_in_frozen_order() {
        let frozen = assemble_freezes(&all_seven_sources());
        // first occurrence of each kind follows EndpointKind::ALL
        let mut firsts: Vec<&str> = Vec::new();
        for f in &frozen {
            let name = f.kind.expect("every entry names its kind").name();
            if !firsts.contains(&name) {
                firsts.push(name);
            }
        }
        assert_eq!(
            firsts,
            EndpointKind::ALL.iter().map(|k| k.name()).collect::<Vec<_>>()
        );
        // this fixture: one endpoint per kind plus a second DNS server, and the
        // normative reasoned absence of turn
        assert_eq!(frozen.len(), 8);
        for f in &frozen {
            assert_eq!(f.frozen_at_unix, 1_756_000_000);
            assert_eq!(f.frozen_at_mono, 123_456);
            let kind = f.kind.expect("kind");
            if f.present {
                assert_eq!(f.proto, kind.proto(), "{} transport", kind.name());
                assert!(!f.host.is_empty() && f.port != 0 && !f.resolved.is_empty());
                assert!(f.source.starts_with("shell-exclusion") || f.source.starts_with("core-")
                    || f.source == SOURCE_WG_DEVICE_OUTER_SOCKET,
                    "unexpected source token {source}", source = f.source);
                assert!(f.absent_reason.is_empty(), "a present endpoint has no reason");
            } else {
                assert!(!f.absent_reason.is_empty(), "{} absence must be reasoned", kind.name());
                assert!(f.host.is_empty() && f.port == 0 && f.resolved.is_empty());
                assert!(f.source.is_empty() && f.proto.is_empty());
            }
        }
        let turn = frozen.iter().find(|f| f.kind == Some(EndpointKind::Turn)).expect("turn entry");
        assert!(!turn.present);
        assert_eq!(turn.absent_reason, TURN_REJECTED_REASON);
    }

    #[test]
    fn freeze_assembly_records_missing_required_kind_with_reason() {
        let mut src = all_seven_sources();
        src.shell_exclusions.retain(|e| e.kind != EndpointKind::Signal);
        src.signal_config = None;
        let frozen = assemble_freezes(&src);
        let signal = frozen
            .iter()
            .find(|f| f.kind == Some(EndpointKind::Signal))
            .expect("the gap is recorded, not omitted");
        assert!(!signal.present);
        assert!(signal.absent_reason.contains("signal"), "{}", signal.absent_reason);
        assert!(EndpointKind::Signal.required(), "signal is a required kind");
        assert!(signal.host.is_empty() && signal.port == 0 && signal.resolved.is_empty());
        assert!(signal.source.is_empty() && signal.proto.is_empty());

        // an injected endpoint without host/port is not a usable freeze either
        let mut src2 = all_seven_sources();
        src2.shell_exclusions.retain(|e| e.kind != EndpointKind::Signal);
        src2.signal_config = Some(InjectedEndpoint::default());
        let frozen2 = assemble_freezes(&src2);
        let signal2 = frozen2.iter().find(|f| f.kind == Some(EndpointKind::Signal)).unwrap();
        assert!(!signal2.present);
        assert!(!signal2.absent_reason.is_empty());
    }

    #[test]
    fn freeze_assembly_optional_kinds_absent_with_reason_are_not_gaps() {
        // turn: reasoned absence even though the network map carried a turn URI
        let frozen = assemble_freezes(&all_seven_sources());
        let turn = frozen.iter().find(|f| f.kind == Some(EndpointKind::Turn)).unwrap();
        assert!(!turn.present);
        assert_eq!(turn.absent_reason, TURN_REJECTED_REASON);
        assert!(!EndpointKind::Turn.required(), "turn is optional");

        // relay with no source at all: optional kind, reasoned absence
        let mut src = all_seven_sources();
        src.shell_exclusions.retain(|e| e.kind != EndpointKind::Relay);
        src.relay_advertised.clear();
        let frozen = assemble_freezes(&src);
        let relay = frozen.iter().find(|f| f.kind == Some(EndpointKind::Relay)).unwrap();
        assert!(!relay.present);
        assert!(!relay.absent_reason.is_empty());
        assert!(!EndpointKind::Relay.required(), "relay is optional");
        // …and every REQUIRED kind is still present in the same fixture
        for kind in EndpointKind::ALL.iter().filter(|k| k.required()) {
            let f = frozen.iter().find(|f| f.kind == Some(*kind)).expect("required kind entry");
            assert!(f.present, "required kind {} must be frozen here", kind.name());
        }

        // a shell turn entry (not produced by today's 3/7 derivation) still
        // never becomes a freeze: the design has no turn path
        let mut src3 = all_seven_sources();
        src3.shell_exclusions.push(ShellExclusion::new(EndpointKind::Turn, "turn.example", 3478));
        let frozen3 = assemble_freezes(&src3);
        let turns: Vec<_> = frozen3.iter().filter(|f| f.kind == Some(EndpointKind::Turn)).collect();
        assert_eq!(turns.len(), 1);
        assert!(!turns[0].present);
        assert_eq!(turns[0].absent_reason, TURN_REJECTED_REASON);
    }

    #[test]
    fn freeze_assembly_annotates_every_source_token() {
        // shell-sourced kinds keep the shell token (the C2 cross-check artifact)
        let frozen = assemble_freezes(&all_seven_sources());
        let source_of = |kind: EndpointKind| {
            frozen.iter().find(|f| f.kind == Some(kind)).expect("kind entry").source.clone()
        };
        assert_eq!(source_of(EndpointKind::Management), SOURCE_SHELL_EXCLUSION);
        assert_eq!(source_of(EndpointKind::Signal), SOURCE_SHELL_EXCLUSION);
        assert_eq!(source_of(EndpointKind::Relay), SOURCE_SHELL_EXCLUSION);
        assert_eq!(source_of(EndpointKind::Stun), SOURCE_CORE_NETWORK_MAP);
        assert_eq!(source_of(EndpointKind::WgPeer), SOURCE_WG_DEVICE_OUTER_SOCKET);
        assert_eq!(source_of(EndpointKind::Dns), SOURCE_CORE_NETWORK_MAP);

        // core-only fallbacks (no shell exclusion for these kinds)
        let core_only = FreezeSources {
            management_dial: Some(InjectedEndpoint::new("203.0.113.7", 443)),
            signal_config: Some(InjectedEndpoint::new("signal.example", 10000)),
            relay_advertised: vec![InjectedEndpoint::new("relay.example", 28443)],
            stun_uris: vec![InjectedEndpoint::new("stun.example", 3478)],
            wg_peer_endpoints: vec![InjectedEndpoint::new("198.51.100.4", 51820)],
            dns_servers: vec![InjectedEndpoint::new("10.0.0.53", 53)],
            frozen_at_unix: 1_756_000_001,
            frozen_at_mono: 999,
            ..FreezeSources::default()
        };
        let frozen = assemble_freezes(&core_only);
        let entry = |kind: EndpointKind| {
            frozen.iter().find(|f| f.kind == Some(kind)).expect("kind entry").clone()
        };
        assert_eq!(entry(EndpointKind::Management).source, SOURCE_CORE_DIAL_ADDRESS);
        assert_eq!(entry(EndpointKind::Signal).source, SOURCE_CORE_SIGNAL_CONFIG);
        assert_eq!(entry(EndpointKind::Relay).source, SOURCE_CORE_RELAY_ADVERTISED);
        // transport follows the kind; resolved falls back to host:port when the
        // core source had no separately resolved form
        assert_eq!(entry(EndpointKind::Management).proto, "tcp");
        assert_eq!(entry(EndpointKind::Stun).proto, "udp");
        assert_eq!(entry(EndpointKind::Management).resolved, "203.0.113.7:443");
        assert_eq!(entry(EndpointKind::Management).frozen_at_unix, 1_756_000_001);
        assert_eq!(entry(EndpointKind::Management).frozen_at_mono, 999);
    }

    #[test]
    fn freeze_assembly_merges_shell_and_core_sources_without_duplicate_resolved_endpoints() {
        // the shell entry and the core dial address are the SAME resolved /32:
        // one frozen endpoint, and the shell token keeps the slot
        let frozen = assemble_freezes(&all_seven_sources());
        let mgmt: Vec<_> =
            frozen.iter().filter(|f| f.kind == Some(EndpointKind::Management)).collect();
        assert_eq!(mgmt.len(), 1);
        assert_eq!(mgmt[0].source, SOURCE_SHELL_EXCLUSION);
        assert_eq!(mgmt[0].host, "mgmt.example");
        assert_eq!(mgmt[0].resolved, "203.0.113.7:443");

        // distinct resolved addresses of one kind are all frozen (union; the
        // exclusion installs per resolved /32) in injected order
        let mut src = all_seven_sources();
        src.relay_advertised = vec![InjectedEndpoint::new("relay-alt.example", 28443)];
        let frozen = assemble_freezes(&src);
        let relays: Vec<_> = frozen.iter().filter(|f| f.kind == Some(EndpointKind::Relay)).collect();
        assert_eq!(relays.len(), 2);
        assert_eq!(relays[0].source, SOURCE_SHELL_EXCLUSION);
        assert_eq!(relays[1].source, SOURCE_CORE_RELAY_ADVERTISED);
        // multiple DNS servers stay separate entries, injected order preserved
        let dns: Vec<_> = frozen.iter().filter(|f| f.kind == Some(EndpointKind::Dns)).collect();
        assert_eq!(
            dns.iter().map(|f| f.host.as_str()).collect::<Vec<_>>(),
            vec!["10.0.0.53", "10.0.0.54"]
        );
    }

    #[test]
    fn assembled_freezes_render_in_the_frozen_document_field_order() {
        // the section is rendered by n2h::Evidence::to_json — the ONE renderer
        // for host and device documents — so this pins design §3 field order
        // and key spelling without duplicating a renderer here.
        let mut evidence = crate::n2h::Evidence::default();
        evidence.frozen_endpoints = assemble_freezes(&all_seven_sources());
        let json = evidence.to_json();
        let marker = "\"frozen_endpoints\":[";
        let start = json.find(marker).expect("section starts") + marker.len();
        let end = json[start..].find("],\"probes\"").expect("section ends") + start;
        let section = &json[start..end];
        assert!(
            section.starts_with(
                "{\"kind\":\"management\",\"present\":true,\"host\":\"mgmt.example\",\"port\":443,\
                 \"proto\":\"tcp\",\"source\":\"shell-exclusion\",\"resolved\":\"203.0.113.7:443\",\
                 \"absent_reason\":\"\",\"frozen_at_unix\":1756000000,\"frozen_at_mono\":123456}"
            ),
            "{section}"
        );
        assert!(
            section.contains(&format!(
                "{{\"kind\":\"turn\",\"present\":false,\"host\":\"\",\"port\":0,\"proto\":\"\",\
                 \"source\":\"\",\"resolved\":\"\",\"absent_reason\":\"{TURN_REJECTED_REASON}\",\
                 \"frozen_at_unix\":1756000000,\"frozen_at_mono\":123456}}"
            )),
            "{section}"
        );
        assert!(section.contains("\"kind\":\"wg_peer\""), "{section}");
        assert!(section.contains("\"kind\":\"dns\""), "{section}");
        println!("[I1] frozen_endpoints=[{section}]");
    }

    // ---- I3 ----

    /// One probe id of a fixed, valid shape: `N2H-<kind>-<24 hex nonce>`.
    fn id_of(kind: EndpointKind, nonce_seed: u8) -> String {
        probe_id(kind.name(), &format!("{nonce_seed:02x}").repeat(12))
    }

    #[test]
    fn udp_probe_lands_on_a_local_sink_with_the_probe_marker() {
        let mut sink = crate::n2h::UdpSink::bind(false).expect("udp sink");
        let dst = sink.local_addr().expect("sink addr");
        let id = id_of(EndpointKind::Management, 0x01);
        let sent = send_udp_probe(dst, &id).expect("udp probe");

        // five-tuple: a fresh wildcard-bound socket — the reported source is
        // the bind's getsockname (design §4.0/§4.2 shape), the PORT is what
        // makes it unique
        assert_eq!(sent.tuple.proto, "udp");
        assert_eq!(sent.tuple.dst, dst.to_string());
        let src: SocketAddr = sent.tuple.src.parse().expect("src parses");
        assert_ne!(src.port(), 0, "an ephemeral source port");
        assert!(src.ip().is_unspecified(), "the reported src is the wildcard bind: {src}");
        assert_eq!(sent.probe_id, id);
        assert_eq!(sent.marker, WireMarker::Ascii(id.clone()));
        assert_eq!(sent.payload_len, id.len());
        assert_eq!(sent.mapped, None, "a UDP marker probe has no STUN mapping");

        // the sink (design §8's faithful fake endpoint) saw exactly the marker
        sink.drain();
        let rec = sink.observed(&sent.marker).expect("sink recorded the marker");
        assert_eq!(rec.payload, id.as_bytes(), "payload IS the probe id (design §4.2)");
        assert_eq!(rec.src.port(), src.port(), "the reported source PORT is the on-wire one");
        assert_eq!(rec.src.ip(), dst.ip(), "the wire carries the kernel's route source");
        assert_ne!(rec.src.ip(), src.ip(), "…while the reported ADDRESS is the bind itself");
        println!(
            "[I3] udp probe: id={id} tuple={}/{}/{} bytes={}",
            sent.tuple.proto, sent.tuple.src, sent.tuple.dst, sent.payload_len
        );
    }

    #[test]
    fn every_probe_leaves_from_its_own_socket_so_five_tuples_and_markers_are_unique() {
        let mut udp_sink = crate::n2h::UdpSink::bind(false).expect("udp sink");
        let mut tcp_sink = crate::n2h::TcpSink::bind().expect("tcp sink");
        let udp_dst = udp_sink.local_addr().expect("udp addr");
        let tcp_dst = tcp_sink.local_addr().expect("tcp addr");

        let ids: Vec<String> = (0..3).map(|i| id_of(EndpointKind::Signal, 0x10 + i)).collect();
        let mut tuples = Vec::new();
        for id in &ids {
            tuples.push(send_udp_probe(udp_dst, id).expect("udp probe").tuple);
        }
        for id in &ids[..2] {
            tuples.push(send_tcp_probe(tcp_dst, id).expect("tcp probe").tuple);
        }

        // five tuples, five distinct (proto, src, dst) — every probe from its
        // OWN fresh socket (design §4.2), including the two on the same
        // tcp dst:port
        assert_eq!(tuples.len(), 5);
        let mut seen = std::collections::HashSet::new();
        for t in &tuples {
            assert!(
                seen.insert(format!("{}|{}|{}", t.proto, t.src, t.dst)),
                "duplicate five-tuple {t:?}"
            );
        }
        // …and the payload markers are unique too (fresh nonce per probe)
        assert_eq!(ids.len(), 3);
        assert_ne!(ids[0], ids[1]);

        udp_sink.drain();
        for id in &ids {
            assert!(
                udp_sink.observed(&WireMarker::Ascii(id.clone())).is_some(),
                "udp sink saw {id}"
            );
        }
        tcp_sink.drain();
        for id in &ids[..2] {
            let rec = tcp_sink
                .observed(&WireMarker::Ascii(id.clone()))
                .unwrap_or_else(|| panic!("tcp sink saw {id}"));
            assert_eq!(rec.payload, id.as_bytes(), "connect+write put the marker on the wire");
        }
        println!(
            "[I3] 5 probes, 5 unique tuples: {}",
            tuples
                .iter()
                .map(|t| format!("{}/{}/{}", t.proto, t.src, t.dst))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    #[test]
    fn udp_probe_from_a_wildcard_socket_reaches_a_non_loopback_local_endpoint() {
        // The device-validation run-2 regression (n2h.rs:2373-2396): a
        // loopback-bound probe socket made every send to a real, non-loopback
        // endpoint fail with EINVAL. The device sender binds wildcard, so the
        // same destination must work — and the packet must leave from the
        // route's source address.
        let routing = std::net::UdpSocket::bind(PROBE_BIND_WILDCARD).expect("wildcard bind");
        // no packet is sent: connect() only resolves the route
        if routing.connect("192.0.2.1:9").is_err() {
            println!("[I3] wildcard routing check skipped: no route information on this host");
            return;
        }
        let local = routing.local_addr().expect("local addr").ip();
        if local.is_loopback() || local.is_unspecified() {
            println!("[I3] wildcard routing check skipped: no non-loopback IPv4 source");
            return;
        }
        let sink = std::net::UdpSocket::bind((local, 0)).expect("sink on the local address");
        sink.set_nonblocking(true).expect("nonblocking sink");
        let id = id_of(EndpointKind::WgPeer, 0x2a);
        let sent = send_udp_probe(sink.local_addr().expect("sink addr"), &id)
            .expect("probe to a non-loopback local endpoint must not fail at send");
        let reported: SocketAddr = sent.tuple.src.parse().expect("src");
        assert_ne!(reported.port(), 0);
        assert!(reported.ip().is_unspecified(), "reported src is the wildcard bind: {reported}");
        let mut buf = [0u8; 256];
        let (n, from) = sink.recv_from(&mut buf).expect("the probe arrived");
        assert_eq!(&buf[..n], id.as_bytes());
        assert_eq!(from.ip(), local, "the kernel used the route source towards {local}");
        assert_eq!(from.port(), reported.port(), "…from the probe's own ephemeral port");
        println!(
            "[I3] wildcard probe -> {local}: n={n} wire_src={from} reported_src={reported}"
        );
    }

    #[test]
    fn stun_probe_is_a_real_binding_request_and_parses_the_txid_matched_mapping() {
        let txid = [0x5au8; 12];
        let (dst, responder) = spawn_stun_responder(txid, [203, 0, 113, 9], 7777);
        let sent = send_stun_probe(dst, txid, 1_000).expect("stun probe");
        let request = responder.join().expect("responder thread");

        // what went out is a real Binding Request carrying OUR txid
        assert_eq!(&request[0..2], &crate::stun::BINDING_REQUEST.to_be_bytes());
        assert_eq!(&request[4..8], &crate::stun::MAGIC_COOKIE.to_be_bytes());
        assert_eq!(&request[8..20], &txid);
        assert_eq!(sent.payload_len, request.len());
        assert_eq!(sent.marker, WireMarker::StunTxid(txid));
        assert_eq!(sent.probe_id, format!("N2H-stun-{}", "5a".repeat(12)));
        // …and the txid-matched response's XOR/MAPPED address is reported
        assert_eq!(sent.mapped, Some(([203, 0, 113, 9], 7777)));
        println!(
            "[I3] stun probe: id={} len={} mapped={:?} tuple={}",
            sent.probe_id, sent.payload_len, sent.mapped, sent.tuple.src
        );
    }

    #[test]
    fn send_probe_follows_the_transport_the_freeze_recorded() {
        let udp_sink = crate::n2h::UdpSink::bind(false).expect("udp sink");
        let tcp_sink = crate::n2h::TcpSink::bind().expect("tcp sink");

        // tcp kinds go out over TCP, udp kinds over UDP (kind.proto() is the
        // frozen column the freezes recorded)
        let relay = send_probe(EndpointKind::Relay, tcp_sink.local_addr().unwrap(), 200)
            .expect("relay tcp probe");
        assert_eq!(relay.tuple.proto, "tcp");
        assert_eq!(EndpointKind::Relay.proto(), "tcp");
        let peer = send_probe(EndpointKind::WgPeer, udp_sink.local_addr().unwrap(), 200)
            .expect("wg_peer udp probe");
        assert_eq!(peer.tuple.proto, "udp");
        // id shape: N2H-<kind>-<24 hex nonce>
        let nonce = peer.probe_id.strip_prefix("N2H-wg_peer-").expect("id shape");
        assert_eq!(nonce.len(), 24);
        assert!(nonce.bytes().all(|b| b.is_ascii_hexdigit()), "{nonce}");

        // stun: a fresh 12-byte txid is the marker; no responder here → no
        // receipt, recorded as None rather than invented
        let stun = send_probe(EndpointKind::Stun, udp_sink.local_addr().unwrap(), 200)
            .expect("stun probe");
        assert!(stun.probe_id.starts_with("N2H-stun-"), "{}", stun.probe_id);
        assert!(matches!(stun.marker, WireMarker::StunTxid(_)));
        assert_eq!(stun.mapped, None, "no responder → no receipt (a gap, not a pass)");
        assert_ne!(stun.marker, peer.marker, "every probe's payload is unique");
        println!(
            "[I3] dispatch: relay={}/{} wg_peer={} stun={}",
            relay.tuple.proto, relay.probe_id, peer.probe_id, stun.probe_id
        );
    }

    #[test]
    fn dns_echo_probe_receives_its_own_qname_back_and_foreign_answers_do_not_count() {
        let marker = id_of(EndpointKind::Dns, 0x3c);
        let qname = dns_echo_qname(&marker);
        assert_eq!(qname, format!("{marker}.n2h-invalid"));

        let (dst, responder) = spawn_dns_responder();
        let sent = send_dns_echo_probe(dst, &marker, 1_000).expect("dns echo probe");
        let query = responder.join().expect("responder thread");
        assert!(sent.echo_received, "the mirrored question section IS the receipt");
        assert_eq!(sent.marker, WireMarker::Ascii(marker.clone()));
        assert_eq!(sent.payload_len, query.len());
        assert_eq!(sent.tuple.proto, "udp");

        // the query on the wire: our query id, one question, our qname
        assert_eq!(&query[0..2], &dns_query_id(&qname).to_be_bytes());
        assert_eq!(&query[4..6], &1u16.to_be_bytes(), "QDCOUNT=1");
        let name = dns_wire_name(&qname).expect("encodable");
        assert_eq!(&query[12..12 + name.len()], &name[..], "question carries our qname");
        assert_eq!(&query[query.len() - 4..], &[0, 1, 0, 1], "QTYPE A, QCLASS IN");

        // pure receipt test: the answer must be a RESPONSE for OUR qname
        let answer = mirror_as_nxdomain(&query);
        assert!(dns_echo_receipt(&answer, &qname));
        let mut not_a_response = answer.clone();
        not_a_response[2] &= 0x7f; // clear QR
        assert!(!dns_echo_receipt(&not_a_response, &qname), "a query is not a receipt");
        assert!(!dns_echo_receipt(&answer[..8], &qname), "a truncated header is not a receipt");
        assert!(
            !dns_echo_receipt(&answer, &dns_echo_qname(&id_of(EndpointKind::Dns, 0x3d))),
            "another probe's qname is not this probe's receipt"
        );
        assert!(!dns_echo_receipt(&answer, "not a name!"), "an unencodable qname never matches");
        println!(
            "[I3] dns echo: qname={qname} query_len={} echo_received={}",
            sent.payload_len, sent.echo_received
        );
    }

    /// A one-shot STUN responder: one bounded blocking receive (no polling, no
    /// sleep), then a real Binding Success Response carrying a plain
    /// MAPPED-ADDRESS (which [`crate::stun::parse_binding_response`] accepts,
    /// `stun.rs`'s RFC 3489 fallback case). Returns the address to probe and
    /// the request bytes it saw.
    fn spawn_stun_responder(
        txid: [u8; 12],
        mapped_ip: [u8; 4],
        mapped_port: u16,
    ) -> (SocketAddr, std::thread::JoinHandle<Vec<u8>>) {
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").expect("responder bind");
        sock.set_read_timeout(Some(std::time::Duration::from_secs(5))).expect("read timeout");
        let addr = sock.local_addr().expect("responder addr");
        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            let (n, from) = sock.recv_from(&mut buf).expect("responder recv");
            let request = buf[..n].to_vec();
            let mut resp = Vec::new();
            resp.extend_from_slice(&crate::stun::BINDING_SUCCESS.to_be_bytes());
            resp.extend_from_slice(&12u16.to_be_bytes()); // one 12-byte attribute
            resp.extend_from_slice(&crate::stun::MAGIC_COOKIE.to_be_bytes());
            resp.extend_from_slice(&txid);
            resp.extend_from_slice(&crate::stun::ATTR_MAPPED.to_be_bytes());
            resp.extend_from_slice(&8u16.to_be_bytes());
            resp.extend_from_slice(&[0x00, 0x01]); // reserved + IPv4 family
            resp.extend_from_slice(&mapped_port.to_be_bytes());
            resp.extend_from_slice(&mapped_ip);
            sock.send_to(&resp, from).expect("responder send");
            request
        });
        (addr, handle)
    }

    /// A one-shot DNS responder: one bounded blocking receive, then the answer
    /// a resolver sends for an unknown name — the query mirrored back with QR
    /// set and RCODE=NXDOMAIN (RFC 1035 §4.1.1), i.e. the same question
    /// section, which is exactly the `dns-nxdomain-echo` receipt.
    fn spawn_dns_responder() -> (SocketAddr, std::thread::JoinHandle<Vec<u8>>) {
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").expect("responder bind");
        sock.set_read_timeout(Some(std::time::Duration::from_secs(5))).expect("read timeout");
        let addr = sock.local_addr().expect("responder addr");
        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            let (n, from) = sock.recv_from(&mut buf).expect("responder recv");
            let query = buf[..n].to_vec();
            sock.send_to(&mirror_as_nxdomain(&query), from).expect("responder send");
            query
        });
        (addr, handle)
    }

    /// Mirror a query into the NXDOMAIN answer that echoes its question section.
    fn mirror_as_nxdomain(query: &[u8]) -> Vec<u8> {
        let mut r = query.to_vec();
        r[2] = 0x81; // QR | RD
        r[3] = 0x83; // RA | RCODE = NXDOMAIN
        r
    }

    // ---- I4 ----

    /// A `/proc/net/dev`-shaped fixture row builder: the header lines are part
    /// of the real file and must not confuse the row parser. The default
    /// numbers are the D1 device-run values (`docs/n13-device-relay-evidence-
    /// 20260916.md`: vpn-tun RX 4624 B / 58 pkts, TX 688 B / 6 pkts).
    fn proc_text(rx_bytes: u64, rx_packets: u64, tx_bytes: u64, tx_packets: u64) -> String {
        format!(
            "Inter-|   Receive                                                |  Transmit\n\
             \x20face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n\
             \x20   lo: 1234567    8901    0    0    0     0          0         0  1234567    8901    0    0    0     0       0          0\n\
             vpn-tun: {rx_bytes} {rx_packets} 0 0 0 0 0 0 {tx_bytes} {tx_packets} 0 0 0 0 0 0\n"
        )
    }

    fn wg_stats(rx_packets: u64, rx_bytes_to_tun: u64) -> WgDeviceStats {
        WgDeviceStats {
            rx_packets,
            rx_bytes_to_tun,
            tx_packets: 6,
            tx_bytes: 688,
            ..WgDeviceStats::default()
        }
    }

    /// Scripted [`LedgerReads`] fixture: each script is consumed front-to-back
    /// and its LAST element repeats forever, so a never-converging resample
    /// case stays bounded and every read is logged in order.
    struct FixtureReads {
        direct: Vec<Result<String, String>>,
        capture: Vec<Result<String, String>>,
        wg: Vec<Result<WgDeviceStats, String>>,
        step: [usize; 3],
        log: Vec<&'static str>,
    }

    fn pick<'a, T>(items: &'a [T], step: &mut usize) -> &'a T {
        assert!(!items.is_empty(), "a fixture script must not be empty");
        let i = (*step).min(items.len() - 1);
        *step += 1;
        &items[i]
    }

    impl FixtureReads {
        fn new(
            direct: Vec<Result<String, String>>,
            capture: Vec<Result<String, String>>,
            wg: Vec<Result<WgDeviceStats, String>>,
        ) -> FixtureReads {
            FixtureReads { direct, capture, wg, step: [0; 3], log: Vec::new() }
        }
    }

    impl LedgerReads for FixtureReads {
        fn read_proc_net_dev(&mut self) -> Result<String, String> {
            self.log.push("proc-direct");
            pick(&self.direct, &mut self.step[0]).clone()
        }
        fn read_proc_hdc_capture(&mut self) -> Result<String, String> {
            self.log.push("proc-hdc");
            pick(&self.capture, &mut self.step[1]).clone()
        }
        fn read_wg_stats(&mut self) -> Result<WgDeviceStats, String> {
            self.log.push("wg");
            pick(&self.wg, &mut self.step[2]).clone()
        }
    }

    #[test]
    fn proc_net_dev_parses_the_vpn_tun_row_and_nothing_else() {
        let fixture = proc_text(4624, 58, 688, 6);
        let row = parse_proc_net_dev(&fixture, TUN_IFACE_NAME).expect("vpn-tun row");
        assert_eq!(row, ProcIfaceRow { rx_bytes: 4624, rx_packets: 58, tx_bytes: 688, tx_packets: 6 });

        // D8/D9 caliber: RX bytes/packets are the delivered pair (the second
        // ledger of the identity), TX packets is injected_frames
        let snap = iface_snapshot(&wg_stats(58, 4624), &row, 111, 1_756_000_111);
        assert_eq!(snap.tun_delivered_bytes, 4624);
        assert_eq!(snap.tun_delivered_frames, 58);
        assert_eq!(snap.tun_injected_frames, 6);
        assert_eq!(snap.wg_rx_bytes_to_tun, 4624);
        assert_eq!(snap.wg_rx_packets, 58);
        assert_eq!((snap.at_mono, snap.at_unix), (111, 1_756_000_111));

        // exact name match: a name that merely CONTAINS the literal is another
        // interface, and a missing/unusable row is a gap, not a guess
        assert!(parse_proc_net_dev("vpn-tun0: 1 2 0 0 0 0 0 0 3 4 0 0 0 0 0 0\n", TUN_IFACE_NAME)
            .is_none());
        assert!(parse_proc_net_dev(" xvpn-tun : 1 2 0 0 0 0 0 0 3 4 0 0 0 0 0 0\n", TUN_IFACE_NAME)
            .is_none());
        assert!(parse_proc_net_dev(&fixture, "wg0").is_none());
        assert!(parse_proc_net_dev("vpn-tun: 1 2 3\n", TUN_IFACE_NAME).is_none());
        assert!(parse_proc_net_dev("Inter-|   Receive  |  Transmit\n", TUN_IFACE_NAME).is_none());

        // a counter beyond 32 bits must not wrap (long-lived interface)
        let big = parse_proc_net_dev(
            "vpn-tun: 4294967296 7 0 0 0 0 0 0 8 9 0 0 0 0 0 0\n",
            TUN_IFACE_NAME,
        )
        .expect("row");
        assert_eq!(big.rx_bytes, 4_294_967_296);
        println!("[I4] parsed vpn-tun row {row:?} -> tun.injected_frames={} \
                  tun.delivered_frames={} tun.delivered_bytes={}",
            snap.tun_injected_frames, snap.tun_delivered_frames, snap.tun_delivered_bytes);
    }

    #[test]
    fn paired_read_reads_proc_before_wg_and_reports_the_source_token() {
        let mut reads = FixtureReads::new(
            vec![Ok(proc_text(4624, 58, 688, 6))],
            vec![Err("no hdc capture provisioned".into())],
            vec![Ok(wg_stats(58, 4624))],
        );
        let paired = paired_read(&mut reads, 7, 1_756_000_007).expect("paired read");
        assert_eq!(reads.log, vec!["proc-direct", "wg"], "design §4.4 freezes /proc FIRST");
        assert_eq!(paired.source, COUNTERS_SOURCE_PROC);
        assert_eq!(paired.tun.rx_bytes, 4624);
        assert_eq!(paired.wg.rx_bytes_to_tun, 4624);
        assert_eq!(paired.snapshot().tun_delivered_bytes, 4624);
        assert_eq!(paired.snapshot().wg_rx_bytes_to_tun, 4624);
        assert_eq!(counters_source_note(paired.source), "counters-source=proc-net-dev");

        // a wg-ledger failure is an Err (I5 records the counters gap), and the
        // /proc read still happened first
        let mut reads = FixtureReads::new(
            vec![Ok(proc_text(4624, 58, 688, 6))],
            vec![Err("no hdc capture provisioned".into())],
            vec![Err("connector not ready".into())],
        );
        let err = paired_read(&mut reads, 7, 0).unwrap_err();
        assert!(err.contains("connector not ready"), "{err}");
        assert_eq!(reads.log, vec!["proc-direct", "wg"]);
        println!("[I4] paired read source={} wg-error-path='{err}'", paired.source);
    }

    #[test]
    fn a_self_consistent_pairing_reconciles_on_both_identities() {
        // before: RX 4624 B / 58 pkts; after: RX 4724 B / 60 pkts — the same
        // +100 B / +2 frames in the wg ledger
        let mut reads = FixtureReads::new(
            vec![Ok(proc_text(4624, 58, 688, 6)), Ok(proc_text(4724, 60, 700, 7))],
            vec![Err("no hdc capture provisioned".into())],
            vec![Ok(wg_stats(58, 4624)), Ok(wg_stats(60, 4724))],
        );
        let before = open_window(&mut reads, 100, 1_756_000_100).expect("before edge");
        let sample = close_window(&mut reads, &before, 200, 1_756_000_200).expect("after edge");

        assert!(sample.consistent());
        assert!(sample.identities.bytes && sample.identities.packets);
        assert_eq!(sample.resamples, 0, "a first-try pairing needs no resample");
        assert_eq!(sample.source, COUNTERS_SOURCE_PROC);
        assert_eq!(sample.counters.side, TUN_IFACE_NAME, "D8: side = vpn-tun");
        // the FROZEN identity the judge reads (n2h::WindowCounters::reconciled)
        assert!(sample.counters.reconciled());
        let d = sample.counters.delta();
        assert_eq!((d.wg_rx_bytes_to_tun, d.tun_delivered_bytes), (100, 100));
        assert_eq!((d.wg_rx_packets, d.tun_delivered_frames), (2, 2));
        assert_eq!((d.tun_injected_frames, before.tun.tx_packets), (1, 6));
        println!(
            "[I4] window side={} resamples={} bytes={} packets={} delta(wg_rx_bytes_to_tun={}, \
             tun_delivered_bytes={}, wg_rx_packets={}, tun_delivered_frames={}, tun_injected_frames={})",
            sample.counters.side,
            sample.resamples,
            sample.identities.bytes,
            sample.identities.packets,
            d.wg_rx_bytes_to_tun,
            d.tun_delivered_bytes,
            d.wg_rx_packets,
            d.tun_delivered_frames,
            d.tun_injected_frames
        );
    }

    #[test]
    fn the_packet_identity_catches_a_break_the_byte_identity_misses() {
        // A frame crossed between the two reads of the close edge and it is
        // 100 bytes: wg saw 2 frames / +100 B, /proc saw 1 frame / +100 B. The
        // frozen byte identity is satisfied; the design's packet identity is
        // not — which is exactly what it is for.
        let skewed = IfaceSnapshot {
            wg_rx_packets: 2,
            wg_rx_bytes_to_tun: 100,
            tun_delivered_frames: 1,
            tun_delivered_bytes: 100,
            ..IfaceSnapshot::default()
        };
        let ids = counter_identities(&skewed);
        assert!(ids.bytes, "the byte identity alone would accept this pairing");
        assert!(!ids.packets, "…and the packet identity refuses it");
        assert!(!ids.hold());
        let counters = WindowCounters {
            side: TUN_IFACE_NAME.to_string(),
            before: IfaceSnapshot::default(),
            after: skewed,
        };
        assert!(counters.reconciled(), "n2h::WindowCounters::reconciled is bytes-only");

        // the sampler must therefore resample the close edge until it is
        // self-consistent: read #1 is the skewed pairing above, read #2 agrees
        let mut reads = FixtureReads::new(
            vec![
                Ok(proc_text(4624, 58, 688, 6)),
                Ok(proc_text(4724, 59, 700, 7)),
                Ok(proc_text(4724, 60, 700, 7)),
            ],
            vec![Err("no hdc capture provisioned".into())],
            vec![Ok(wg_stats(58, 4624)), Ok(wg_stats(60, 4724)), Ok(wg_stats(60, 4724))],
        );
        let before = open_window(&mut reads, 100, 0).expect("before edge");
        let sample = close_window(&mut reads, &before, 200, 0).expect("after edge");
        assert!(sample.consistent(), "the resample found a self-consistent pairing");
        assert_eq!(sample.resamples, 1, "one extra paired read was enough");
        assert_eq!(
            reads.log,
            vec!["proc-direct", "wg", "proc-direct", "wg", "proc-direct", "wg"],
            "each resample re-takes BOTH legs of the edge, in order"
        );
        println!(
            "[I4] packet-identity break on the first pairing (bytes=true packets=false) -> \
             resampled {} time(s) -> consistent={}",
            sample.resamples,
            sample.consistent()
        );
    }

    #[test]
    fn a_pairing_that_never_becomes_self_consistent_is_recorded_within_the_bound() {
        // Every close read has the wg ledger one frame / 100 B ahead of /proc:
        // a frame crosses the pairing gap every time. The window must be
        // RECORDED as unreconciled after exactly the bounded resamples.
        let mut reads = FixtureReads::new(
            vec![Ok(proc_text(4624, 58, 688, 6)), Ok(proc_text(4624, 58, 688, 6))],
            vec![Err("no hdc capture provisioned".into())],
            vec![Ok(wg_stats(58, 4624)), Ok(wg_stats(59, 4724))],
        );
        let before = open_window(&mut reads, 100, 0).expect("before edge");
        let sample = close_window(&mut reads, &before, 200, 0).expect("after edge");

        assert!(!sample.consistent(), "an inconsistent pairing is never repaired");
        assert!(!sample.identities.packets, "wg saw the frame, /proc did not");
        assert!(!sample.identities.bytes);
        assert_eq!(sample.resamples, MAX_PAIRING_RESAMPLES, "1 initial + 3 bounded re-reads");
        assert_eq!(
            reads.log.iter().filter(|s| **s == "wg").count(),
            2 + MAX_PAIRING_RESAMPLES as usize,
            "the before edge + 1 initial close read + exactly the bounded resamples"
        );
        // the recorded delta is the LAST read, untouched
        let d = sample.counters.delta();
        assert_eq!((d.wg_rx_bytes_to_tun, d.tun_delivered_bytes), (100, 0));
        println!(
            "[I4] unreconciled window recorded: resamples={} bytes={} packets={} \
             delta(wg_rx_bytes_to_tun={}, tun_delivered_bytes={})",
            sample.resamples,
            sample.identities.bytes,
            sample.identities.packets,
            d.wg_rx_bytes_to_tun,
            d.tun_delivered_bytes
        );
    }

    #[test]
    fn proc_read_falls_back_to_the_hdc_capture_and_reports_which_path_answered() {
        // 1. the App's own read is refused (§9-U1 unverified) → the operator's
        //    `hdc shell "cat /proc/net/dev"` capture answers, and its caliber is
        //    named for notes[]
        let mut reads = FixtureReads::new(
            vec![Err("EACCES: /proc/net/dev".into())],
            vec![Ok(proc_text(4624, 58, 688, 6))],
            vec![Ok(wg_stats(58, 4624))],
        );
        let (row, source) = read_proc_row(&mut reads).expect("the hdc fallback answers");
        assert_eq!(row, ProcIfaceRow { rx_bytes: 4624, rx_packets: 58, tx_bytes: 688, tx_packets: 6 });
        assert_eq!(source, COUNTERS_SOURCE_HDC_FALLBACK);
        assert_eq!(reads.log, vec!["proc-direct", "proc-hdc"]);
        assert_eq!(counters_source_note(source), "counters-source=hdc-proc-fallback");

        // 2. the direct read succeeds but the restricted /proc view has no
        //    vpn-tun row → the capture is still consulted
        let mut reads = FixtureReads::new(
            vec![Ok("Inter-|   Receive |  Transmit\n    lo: 1 2 0 0 0 0 0 0 3 4 0 0 0 0 0 0\n"
                .into())],
            vec![Ok(proc_text(4624, 58, 688, 6))],
            vec![Ok(wg_stats(58, 4624))],
        );
        assert_eq!(read_proc_row(&mut reads).expect("fallback").1, COUNTERS_SOURCE_HDC_FALLBACK);

        // 3. both paths fail → Err naming BOTH reasons, and the wg ledger is
        //    never read without a /proc row (no counters[] element is built
        //    from half a pairing)
        let mut reads = FixtureReads::new(
            vec![Err("EACCES: /proc/net/dev".into())],
            vec![Err("no hdc capture provisioned".into())],
            vec![Ok(wg_stats(58, 4624))],
        );
        let err = read_proc_row(&mut reads).unwrap_err();
        assert!(err.contains("EACCES") && err.contains("no hdc capture provisioned"), "{err}");
        assert_eq!(reads.log, vec!["proc-direct", "proc-hdc"], "no wg read without a /proc row");
        println!("[I4] dual-path fallback: source={source} both-fail='{err}'");
    }

    #[test]
    fn tee_fallback_row_maps_the_i2_totals_onto_the_proc_caliber() {
        let totals = TunTeeTotals {
            from_os_frames: 6,
            from_os_bytes: 688,
            to_os_frames: 58,
            to_os_bytes: 4624,
        };
        let row = tee_fallback_row(&totals);
        assert_eq!(row, ProcIfaceRow { rx_bytes: 4624, rx_packets: 58, tx_bytes: 688, tx_packets: 6 });
        // the offline substitute produces the same document-side caliber as a
        // real /proc row…
        let offline = iface_snapshot(&wg_stats(58, 4624), &row, 5, 6);
        let real = iface_snapshot(
            &wg_stats(58, 4624),
            &parse_proc_net_dev(&proc_text(4624, 58, 688, 6), TUN_IFACE_NAME).expect("row"),
            5,
            6,
        );
        assert_eq!(offline, real);
        // …and it must declare itself (design §3.2)
        assert_eq!(
            counters_source_note(COUNTERS_SOURCE_TEE_FALLBACK),
            "counters-source=tee-fallback"
        );
        println!(
            "[I4] tee-fallback row {row:?} == /proc caliber; note='{}'",
            counters_source_note(COUNTERS_SOURCE_TEE_FALLBACK)
        );
    }
}
