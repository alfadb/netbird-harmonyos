// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # n2h_collector — N2-H DEVICE-side evidence collector (increments I0 + I1)
//!
//! Spec (single source): `docs/n2h-device-collector-design-20260917.md` —
//! §2.4 (HiLog chunk channel), §4.0 (freeze assembly), §7 I0/I1, §3 (frozen
//! document alignment). Only those two increments live here; the window state
//! machine, probe sender, `/proc` reconciliation and the napi exports are
//! later increments (I3–I5) and do not exist yet.
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
//!
//! Both increments are deliberately IO-free and take every external fact as a
//! parameter (clock included). That is what makes the device assembly testable
//! offline on the host (design §8) and keeps the device path's fail-closed
//! arms outside this module.
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
//! shipped cdylib behavior is unchanged. Sources are injected by the caller;
//! this module never reads the connector, a socket, `/proc` or the filesystem.

use crate::hilog;
use crate::n2h::{EndpointKind, FrozenEndpoint};
use crate::util::{base64, sha256_hex};

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
// tests (pure logic only — no sockets, no clock, no /proc; the device state
// machine is a later increment)
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
}
