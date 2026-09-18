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

use crate::config::Json;
use crate::hilog;
use crate::n2h::{
    probe_id, EndpointKind, EndpointReceipt, Evidence, FiveTuple, FrozenEndpoint, IfaceSnapshot,
    OuterProbe, PosctlEvidence, PosctlProbe, TunHit, TunScanResult, TunnelScanner, WireMarker,
    WindowCounters,
};
use crate::util::{base64, jbool, jinum, jstr, sha256_hex};
use crate::wg_device::{TunFrameDir, TunFrameTee, TunTeeTotals, WgDeviceStats};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

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
    /// Network-map DNS (`dns.servers[]`, v3.1 caliber: only the entries that
    /// do NOT fall inside the overlay's own coverage — `interface_dns` is
    /// never frozen, preregistration :68).
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
            EndpointKind::Dns => "no dns endpoint: the network map has no dns.servers[] entry \
                 outside the overlay's own coverage (v3.1 caliber; interface_dns is never a \
                 freeze target)"
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
// I5 — scan tee, window state machine, verdict wiring, napi JSON surface
// ---------------------------------------------------------------------------

/// The device document's `mode` (design D1).
pub const MODE_DEVICE: &str = "device";
/// The device document's `session` descriptor (design D2).
pub const SESSION_DEVICE: &str =
    "device session; TUN = platform vpn-tun fd (dup); outer probes from app process";

/// Window phases of the `n2h_collector_status()` document (§5.1).
pub const PHASE_ARMED: &str = "armed";
pub const PHASE_PROBING: &str = "probing";
pub const PHASE_OBSERVING: &str = "observing";
pub const PHASE_CLOSED: &str = "closed";

/// Arm-option defaults (§5.1: `posctl_port:40001`, `probe_wait_ms:800`).
pub const DEFAULT_POSCTL_PORT: u16 = 40_001;
/// Positive-control payload suffix — the host payload shape
/// `N2H-posctl-<nonce>|positive-control` (`n2h.rs:1558`).
pub const POSCTL_PAYLOAD_SUFFIX: &str = "|positive-control";
/// Probe-id kind token of the positive control (the host uses the same one).
pub const POSCTL_KIND_TOKEN: &str = "posctl";

/// The datagram payload of ONE positive control: `N2H-posctl-<nonce>` +
/// [`POSCTL_PAYLOAD_SUFFIX`] (design §4.3; the host shape `n2h.rs:1558`).
///
/// Single source of the wire form for BOTH ends of the leg: the device
/// collector sends it ([`send_posctl`], I3/I5) and the host-side peer endpoint
/// (`nbinterop n2h-peer`, I6) recognizes it with [`posctl_marker`] — so the
/// sender and the receipt test cannot drift apart.
pub fn posctl_payload(probe_id: &str) -> String {
    format!("{probe_id}{POSCTL_PAYLOAD_SUFFIX}")
}

/// The strict receipt test of a posctl payload: the marker of
/// [`probe_id`]`(POSCTL_KIND_TOKEN, nonce)` — `N2H-posctl-<24 hex nonce>` —
/// followed by `|positive-control` and NOTHING else. `Some(marker)` = the
/// substring the TUN scanner registers (`build_posctl`), `None` = not a posctl
/// probe.
///
/// Strict on purpose: the I6 peer echoes ONLY a well-formed posctl probe, so
/// the endpoint can never be turned into a reflector for unrelated traffic
/// (design §6's explicit, default-off diagnostic discipline). Wrong kind
/// token, a nonce that is not 24 hex, a missing suffix and any trailing byte
/// all answer `None`.
pub fn posctl_marker(payload: &[u8]) -> Option<&str> {
    let prefix = format!("N2H-{POSCTL_KIND_TOKEN}-");
    let text = std::str::from_utf8(payload).ok()?;
    let marker = text.strip_suffix(POSCTL_PAYLOAD_SUFFIX)?;
    let nonce = marker.strip_prefix(&prefix)?;
    if nonce.len() != 24 || !nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(marker)
}

/// The ONE acceptance test of an operator-announced b2a marker (§13.10's ops
/// relay): the arm option carries the id form the I6 announcement line prints
/// (`b2a_probe_id=N2H-posctl-<24 hex>`), and it goes through the very validator
/// the peer's datagram must pass ([`posctl_marker`]) — so an announced marker
/// can never have a shape this device could not attribute, and no second
/// accepted form can appear beside the frozen wire one.
///
/// `Err(reason)` = refuse the value. The caller ([`ArmOptions::parse_json`])
/// turns that into `invalid-opts`: a half-understood announcement must not open
/// a window, and the external value never reaches a document (only the sanitized
/// hilog detail does).
pub fn announced_b2a_marker(raw: &str) -> Result<String, String> {
    match posctl_marker(posctl_payload(raw).as_bytes()) {
        Some(marker) if marker == raw => Ok(marker.to_string()),
        _ => Err(format!(
            "peer_b2a_probe_id is not an N2H-posctl-<24 hex> marker (got '{}')",
            crate::util::sanitize_marker_field(raw, 96)
        )),
    }
}

/// Hard bound of ONE window (arm → closed). A fuse, not a target: the probe
/// legs are individually bounded (`PROBE_CONNECT_TIMEOUT_MS` /
/// `probe_wait_ms`) and the observation loop has its own deadline; this is the
/// outer bound the state machine checks between legs (§2.2: 全程 deadline 有界).
pub const WINDOW_DEADLINE_MS: u64 = 60_000;
/// Observation deadline after the probes are out (the host's
/// `OBSERVE_DEADLINE_MS` shape, `n2h.rs:1229`).
pub const OBSERVE_DEADLINE_MS: u64 = 5_000;
/// Observation-loop beat (`sys::sleep_ms`), the host's 10 ms shape.
pub const OBSERVE_BEAT_MS: u64 = 10;
/// Consecutive beats after the observation goal first holds, so in-flight
/// frames and late receipts land INSIDE the window (host
/// `OBSERVE_SETTLE_ROUNDS`, `n2h.rs:1232`).
pub const OBSERVE_SETTLE_ROUNDS: u32 = 15;

/// Core-side HiLog markers (`N2H_COLLECTOR` family, design §6.4-3). Every line
/// carries `diag=true|is_evidence=false`: an arm/close ACTION is never gate
/// evidence, only the reviewed document is (§6.1).
pub const MARK_ARM: &str = "N2H_COLLECTOR_ARM";
pub const MARK_CLOSE: &str = "N2H_COLLECTOR_CLOSE";
pub const MARK_DISARM: &str = "N2H_COLLECTOR_DISARM";

/// Arm error tokens — the four of §5.1 plus the two shapes the design did not
/// name (malformed opts, window thread could not start). Both extra tokens are
/// fail-closed answers the caller can act on; nothing else is ever returned.
pub const ARM_ERR_NO_CONNECTOR: &str = "no-connector";
pub const ARM_ERR_DEVICE_NOT_UP: &str = "device-not-up";
pub const ARM_ERR_TUNNEL_NOT_READY: &str = "tunnel-not-ready";
pub const ARM_ERR_ALREADY_ARMED: &str = "already-armed";
pub const ARM_ERR_INVALID_OPTS: &str = "invalid-opts";
pub const ARM_ERR_WINDOW_FAILED: &str = "window-failed";

/// Evidence error tokens of §5.1 (`{error:"no-evidence"}`) plus the
/// out-of-range answer for a chunk index the document does not have.
pub const EVIDENCE_ERR_NONE: &str = "no-evidence";
pub const EVIDENCE_ERR_RANGE: &str = "chunk-out-of-range";

/// The device path of the operator's `hdc shell "cat /proc/net/dev"` capture
/// (the D-class fallback of §4.4; `hdc file send` per §6.4-5).
pub const N2H_HDC_CAPTURE_PATH: &str = "/data/local/tmp/netbird-n2h-proc-net-dev";

/// Poison-tolerant lock (the crate's convention: a poisoned mutex must not
/// take the NAPI boundary down — `panic=abort` in release).
trait LockPoison<T> {
    fn lock_poison(&self) -> MutexGuard<'_, T>;
}

impl<T> LockPoison<T> for Mutex<T> {
    fn lock_poison(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// ---------------------------------------------------------------------------
// clock seam (design U9: the clock is injected, tests never sleep on the clock)
// ---------------------------------------------------------------------------

/// The window's clock. Injected so the state machine's timestamps are a seam
/// (design U9), never read from the ambient clock inside the logic.
pub trait CollectorClock: Send + Sync + 'static {
    /// `sys::mono_ms()` (monotonic milliseconds).
    fn mono_ms(&self) -> u64;
    /// Unix seconds (the document's `*_unix` fields).
    fn unix_s(&self) -> i64;
}

/// Production clock.
pub struct SystemClock;

impl CollectorClock for SystemClock {
    fn mono_ms(&self) -> u64 {
        crate::sys::mono_ms()
    }

    fn unix_s(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

/// The production clock handle.
pub fn system_clock() -> Arc<dyn CollectorClock> {
    Arc::new(SystemClock)
}

// ---------------------------------------------------------------------------
// the scan tee (§4.1): the production `TunFrameTee` implementation
// ---------------------------------------------------------------------------

/// One marker hit with its crossing direction. The frozen renderer
/// (`n2h::TunHit::to_json`) has no direction field and §3.1/§5.2 forbid
/// changing it, so the direction travels in `notes[]` instead (design D4's
/// information, recorded where the design allows additions).
#[derive(Debug, Clone)]
pub struct DirectedHit {
    pub probe_id: String,
    pub direction: TunFrameDir,
    pub hit: TunHit,
}

/// One consistent read of the tee (window close).
#[derive(Debug, Clone)]
pub struct TeeSnapshot {
    /// Both-direction frame/byte totals (the `tee-fallback` caliber's source,
    /// §3.2).
    pub totals: TunTeeTotals,
    /// Frames scanned (`tun_negative.frames_scanned`, both directions).
    pub frames_scanned: u64,
    /// Scan sides seen (always `["vpn-tun"]`, D3).
    pub sides: Vec<String>,
    /// Outer-probe hits (the violation list; a hit ⇒ `n2h-fail`).
    pub outer_hits: Vec<TunHit>,
    /// First hit per (probe id, direction).
    pub directed: Vec<DirectedHit>,
}

/// The collector's scan tee (design §4.1): the object attached to the live
/// `WgDevice` through the I2 seam. Per frame it does exactly what the host
/// scan does — count the direction, feed the shared [`TunnelScanner`], and
/// keep the first hit per (probe, direction) so the positive control's two
/// observation points (§4.3/D5) stay distinguishable.
///
/// The critical section is short (count + substring scan + occasional copy);
/// the lock is the tee's own, never the device's.
pub struct CollectorTee {
    inner: Mutex<TeeInner>,
    clock: Arc<dyn CollectorClock>,
}

struct TeeEntry {
    probe_id: String,
    marker: WireMarker,
    /// false = outer probe (a hit is a violation), true = positive control.
    positive: bool,
}

struct TeeInner {
    totals: TunTeeTotals,
    scanner: TunnelScanner,
    entries: Vec<TeeEntry>,
    directed: Vec<DirectedHit>,
}

impl CollectorTee {
    pub fn new(clock: Arc<dyn CollectorClock>) -> Self {
        CollectorTee {
            inner: Mutex::new(TeeInner {
                totals: TunTeeTotals::default(),
                scanner: TunnelScanner::default(),
                entries: Vec::new(),
                directed: Vec::new(),
            }),
            clock,
        }
    }

    /// Register one probe marker for the window (design §4.1: registered at
    /// arm/send, so pre-window frames can never be attributed to a probe).
    /// The shared [`TunnelScanner`] is the negative-scan authority; the tee's
    /// own entry table only splits the hits by direction.
    pub fn register(&self, probe_id: &str, marker: WireMarker, positive: bool) {
        let mut g = self.inner.lock_poison();
        g.scanner.register(probe_id, marker.clone(), positive);
        g.entries.push(TeeEntry { probe_id: probe_id.to_string(), marker, positive });
    }

    /// Both-direction totals (the `tee-fallback` row §3.2).
    pub fn totals(&self) -> TunTeeTotals {
        self.inner.lock_poison().totals
    }

    /// Frames scanned in the window (both directions).
    pub fn frames_scanned(&self) -> u64 {
        self.inner.lock_poison().scanner.frames_scanned()
    }

    /// Violations seen so far (live status).
    pub fn outer_hit_count(&self) -> usize {
        self.inner.lock_poison().scanner.outer_hits().len()
    }

    /// First observed hit of one probe id in ONE direction (the D5
    /// observation point split).
    pub fn directed_hit(&self, probe_id: &str, dir: TunFrameDir) -> Option<TunHit> {
        self.inner
            .lock_poison()
            .directed
            .iter()
            .find(|d| d.probe_id == probe_id && d.direction == dir)
            .map(|d| d.hit.clone())
    }

    /// One consistent snapshot for the document (window close).
    pub fn snapshot(&self) -> TeeSnapshot {
        let g = self.inner.lock_poison();
        TeeSnapshot {
            totals: g.totals,
            frames_scanned: g.scanner.frames_scanned(),
            sides: g.scanner.sides(),
            outer_hits: g.scanner.outer_hits(),
            directed: g.directed.clone(),
        }
    }
}

impl TunFrameTee for CollectorTee {
    fn on_tun_frame(&self, dir: TunFrameDir, frame: &[u8]) {
        let now = self.clock.mono_ms();
        let mut g = self.inner.lock_poison();
        match dir {
            TunFrameDir::FromOs => {
                g.totals.from_os_frames += 1;
                g.totals.from_os_bytes += frame.len() as u64;
            }
            TunFrameDir::ToOs => {
                g.totals.to_os_frames += 1;
                g.totals.to_os_bytes += frame.len() as u64;
            }
        }
        // the shared host scanner is the negative-scan authority (§4.1)
        g.scanner.scan(TUN_IFACE_NAME, frame, now);
        let matched: Vec<(String, bool)> = g
            .entries
            .iter()
            .filter(|e| e.marker.matches(frame))
            .map(|e| (e.probe_id.clone(), e.positive))
            .collect();
        for (probe_id, _positive) in matched {
            if g.directed.iter().any(|d| d.probe_id == probe_id && d.direction == dir) {
                continue;
            }
            let (src, dst, sport, dport) = match crate::n2h::parse_ipv4_frame(frame) {
                Some(v) => (v.src, v.dst, v.sport, v.dport),
                None => ([0, 0, 0, 0], [0, 0, 0, 0], 0, 0),
            };
            g.directed.push(DirectedHit {
                probe_id: probe_id.clone(),
                direction: dir,
                hit: TunHit {
                    probe_id,
                    side: TUN_IFACE_NAME.to_string(),
                    src,
                    dst,
                    sport,
                    dport,
                    frame_len: frame.len(),
                    observed_at_mono: now,
                },
            });
        }
    }
}

/// `from-os` / `to-os` token (the direction fact of D4).
pub fn direction_token(dir: TunFrameDir) -> &'static str {
    match dir {
        TunFrameDir::FromOs => "from-os",
        TunFrameDir::ToOs => "to-os",
    }
}

// ---------------------------------------------------------------------------
// host seam (design U9) + the process registry the napi exports resolve
// ---------------------------------------------------------------------------

/// Data-plane readiness read (the §2.2 arm gate).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostDataplane {
    /// `connector_status().wg.device_up`: the WG device was constructed from
    /// both feeds and the data plane is pumping.
    pub device_up: bool,
    /// `connector_status().wg.ready`: ≥1 established, non-expired session.
    pub ready: bool,
}

/// Core-session freeze facts (design §4.0's `source` column), read at arm.
#[derive(Debug, Clone, Default)]
pub struct HostFreezeFacts {
    /// Management address core dials (the shell-resolved address of the
    /// protected-socket path, `connector::CoreFreezeInputs::management_dial`).
    pub management_dial: Option<InjectedEndpoint>,
    /// `netbird_config.signal`.
    pub signal_config: Option<InjectedEndpoint>,
    /// `relay_advertised.advertised_urls` ∪ `relay.urls`.
    pub relay_advertised: Vec<InjectedEndpoint>,
    /// Network-map `stuns[].uri`, as parsed by the ICE orchestrator
    /// (`connector::CoreFreezeInputs::stuns`).
    pub stun_uris: Vec<InjectedEndpoint>,
    /// Network-map `turns[].uri` (this build rejects `turn:`, §4.0).
    pub turn_uris: Vec<InjectedEndpoint>,
    /// `wg.wg_peers[].endpoint` — the session's outer UDP endpoints.
    pub wg_peer_endpoints: Vec<InjectedEndpoint>,
    /// Network-map DNS (`dns.servers[]`, v3.1 caliber: only the servers that
    /// do NOT fall inside the overlay's own coverage — see
    /// [`overlay_ranges`]; `interface_dns` never enters this set).
    pub dns_servers: Vec<InjectedEndpoint>,
    /// Freeze-source notes the mapping wants carried into the evidence
    /// `notes[]` (v3.1 C1 dns caliber: the overlay-internal exclusions and
    /// every `dns.servers[]` entry that could not become a strict IPv4
    /// freeze). Empty = nothing to record.
    pub notes: Vec<String>,
    /// Peer overlay addresses (network-map `peers[].vpn_addresses`) — the a2b
    /// positive control's destination (§4.3).
    pub peer_overlay: Vec<[u8; 4]>,
    /// This device's own overlay address (network-map `address`).
    pub own_overlay: Option<[u8; 4]>,
}

/// The counterparty's report (§4.3). For the b2a marker the production carrier
/// is the OPERATOR relay registered in §13.10 (the arm option
/// `peer_b2a_probe_id`, written by the operator and handed to [`WgFeedHost`] as
/// the arm opens the window); a missing report is recorded as a receipt gap,
/// never as an unproven positive control pretended observed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerReport {
    /// The b2a posctl marker the counterparty announced. Without it the
    /// device cannot attribute a to-os frame to the peer's probe, so the b2a
    /// leg is recorded not-sent (a gap) — never guessed.
    pub b2a_probe_id: Option<String>,
    /// The counterparty's own `unknown_peer_drops` (§4.0's wg_peer receipt).
    ///
    /// This is the PEER's counter, a live value read across the window; it is
    /// NOT this device's own `unknown_peer_drops` — that different quantity is
    /// the I6 `wg_stats()` read (the device's own outer socket) and the two must
    /// never be substituted for one another. The production host leaves this
    /// `None` (the peer's counter rides the operator-side status line, §13.10):
    /// the wg_peer receipt then stays a recorded gap, never a false receipt.
    pub unknown_peer_drops: Option<u64>,
}

/// One endpoint-side receipt fact observed by the host seam beyond the
/// collector's own STUN self-proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptFact {
    pub physical_src: String,
    pub physical_src_source: String,
    pub method: String,
    pub detail: String,
}

/// Everything the window needs from the process it runs in (design U9).
///
/// Production: [`WgFeedHost`] (the connector's WG feed + the connector JSON
/// documents). Offline harness: a test type — the seam exists exactly so the
/// collector never reads `connector` state itself.
pub trait CollectorHost: Send + Sync + 'static {
    /// §2.2 arm gate: fail-closed input (no ready session ⇒ no arm).
    fn dataplane(&self) -> Result<HostDataplane, String>;
    /// §4.0 core-side freeze facts, read at the arm instant.
    fn freeze_facts(&self) -> Result<HostFreezeFacts, String>;
    /// §4.1 attach the scan tee to the live device. `Err` = no device to
    /// observe ⇒ the arm is refused (no window without a tee).
    fn attach_tee(&self, tee: Arc<dyn TunFrameTee>) -> Result<(), String>;
    /// §2.2 window close: detach (idempotent; `Ok` when nothing is attached).
    fn detach_tee(&self) -> Result<(), String>;
    /// ONE paired counters read in the frozen order (§4.4). The collector
    /// resamples this exact call on the close edge, so a device host hands in
    /// the `/proc` path and an offline host the `tee-fallback` caliber (§3.2).
    fn paired_read(&self, at_mono: u64, at_unix: i64) -> Result<PairedRead, String>;
    /// §4.3 counterparty facts. Empty by default: the device records the gaps.
    fn peer_report(&self) -> PeerReport {
        PeerReport::default()
    }
    /// §13.10: hand the ARM-TIME peer announcement to the host, once per
    /// accepted arm. Called after every fail-closed gate passed and after the
    /// tee attach, BEFORE the window thread starts (the window reads
    /// [`Self::peer_report`] on its first beats); the value already went
    /// through [`announced_b2a_marker`], so a host stores it verbatim.
    ///
    /// An empty report means "the control file carried no marker".
    /// [`WgFeedHost`] — whose ONLY peer source is this relay — replaces its
    /// stored facts wholesale, so a window can never read the previous window's
    /// announcement. A host that holds an independent live channel (the offline
    /// harness models §4.3's peer status line) keeps its own facts when the arm
    /// carried nothing. Defaulted: an injected host that has no such channel
    /// stays inert and answers `PeerReport::default()`.
    fn note_peer_announcement(&self, _report: PeerReport) {}
    /// The host's LIVE WG device counters, when it has a device up. This is the
    /// read the I6 peer-side status channel needs: the counterparty's own
    /// `unknown_peer_drops` is what proves the device's `wg_peer` outer probe
    /// reached the peer's socket (§4.0's `peer-device-offpath-drop`), and the
    /// host CLI has no other public path to it.
    ///
    /// `None` = no device up (or a host that does not expose counters) — the
    /// caller records the gap, never a guess. Defaulted so an injected offline
    /// host keeps compiling and answers honestly.
    fn wg_stats(&self) -> Option<WgDeviceStats> {
        None
    }
    /// Endpoint-side observation of one probe (§4.0 matrix) beyond the
    /// collector's own STUN self-proof. `None` by default — the device's
    /// management/signal/relay endpoints are real services whose delivery
    /// logs are operator-side (§4.0), so the gap is recorded honestly.
    fn probe_receipt(&self, _probe_id: &str, _marker: &WireMarker) -> Option<ReceiptFact> {
        None
    }
}

static HOST: Mutex<Option<Arc<dyn CollectorHost>>> = Mutex::new(None);

/// Install the process-side host seam (idempotent replace; returns whether a
/// previous host was replaced). This is the ONE call the production wiring
/// needs — see [`WgFeedHost`].
pub fn install_host(host: Arc<dyn CollectorHost>) -> bool {
    HOST.lock_poison().replace(host).is_some()
}

/// Remove the installed host (idempotent; returns whether one was installed).
pub fn uninstall_host() -> bool {
    HOST.lock_poison().take().is_some()
}

/// The installed host, if any.
pub fn installed_host() -> Option<Arc<dyn CollectorHost>> {
    HOST.lock_poison().clone()
}

// ---------------------------------------------------------------------------
// arm options (§5.1)
// ---------------------------------------------------------------------------

/// The shell's dns BRANCH RECORD (v3.1 C1 dns caliber / R-4, preregistration
/// :68): which of the two — and only two — branches produced the dns freeze
/// set, the verbatim physical-attribution grounds, and whether the shell
/// recorded a dns required-class gap.
///
/// The shell derives all three (it is the only side that can call
/// `@ohos.net.connection`); the core validates the frozen token shapes and
/// renders them into the evidence document's `notes[]` verbatim, so a reviewer
/// can compare the arm options, the hilog marker and the document line by
/// line. A missing record is a dns gap for the judge (the fatal wiring is a
/// later increment) — it is never invented here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellDnsReport {
    /// The ONE branch taken: [`DNS_BRANCH_DEFAULT`],
    /// [`DNS_BRANCH_ALL_NETS_FILTERED`] or [`DNS_BRANCH_NONE`].
    pub branch: String,
    /// Verbatim grounds, e.g.
    /// `handle=101;networkCap-has-NOT_VPN=15;bearerTypes-no-BEARER_VPN=4`.
    pub physical_evidence: String,
    /// `Some` iff the shell recorded a dns required-class gap; the token names
    /// the cause (`api-error-code-201`, `no-physical-net`, `dnses-empty`,
    /// `dnses-not-ipv4`).
    pub gap_reason: Option<String>,
}

/// The `dns-branch` value of the unique default-network branch (step 2 held).
pub const DNS_BRANCH_DEFAULT: &str = "default";
/// The `dns-branch` value of the UNIQUE all-nets filtered branch (step 3, taken
/// only when step 2 did not hold).
pub const DNS_BRANCH_ALL_NETS_FILTERED: &str = "all-nets-filtered";
/// The `dns-branch` value when neither branch produced resolvers.
pub const DNS_BRANCH_NONE: &str = "none";

impl ShellDnsReport {
    /// The three frozen `branch` values (an allow-list: anything else refuses
    /// the arm rather than entering the document).
    pub const BRANCHES: [&'static str; 3] =
        [DNS_BRANCH_DEFAULT, DNS_BRANCH_ALL_NETS_FILTERED, DNS_BRANCH_NONE];

    /// The `notes[]` lines, verbatim and in this order.
    pub fn notes(&self) -> Vec<String> {
        let mut out = vec![
            format!("dns-branch={}", self.branch),
            format!("dns-physical-evidence={}", self.physical_evidence),
        ];
        if let Some(reason) = &self.gap_reason {
            out.push(format!("dns-gap={reason}"));
        }
        out
    }
}

/// Validate one shell-reported dns token: printable ASCII, no whitespace and
/// no `|` (the notes field separator — a value carrying it could forge a
/// second note field) and bounded, so the arm option can never smuggle
/// arbitrary text into the evidence document.
fn dns_report_token(field: &str, raw: &str) -> Result<String, String> {
    if raw.is_empty() || raw.len() > 96 {
        return Err(format!("dns.{field} must be 1..=96 characters"));
    }
    if !raw.bytes().all(|b| (0x21..=0x7e).contains(&b) && b != b'|') {
        return Err(format!("dns.{field} carries a character outside the frozen token shape"));
    }
    Ok(raw.to_string())
}

/// `n2h_collector_arm(optsJson)` options (§5.1). `shell_exclusions` is the
/// shell's own exclusion resolution (D7 `shell-exclusion`, the artifact C2
/// cross-checks in the SAME document); the rest are bounded window knobs.
#[derive(Debug, Clone)]
pub struct ArmOptions {
    pub shell_exclusions: Vec<ShellExclusion>,
    pub posctl_port: u16,
    /// Response-side wait of one probe (STUN/DNS) and the receipt window knob.
    pub probe_wait_ms: u64,
    /// Operator-reported build sha256 (§3.3 fallback). Empty = not reported;
    /// I7 injects the embedded value.
    pub build_sha256_reported: String,
    /// §13.10's ops relay: the b2a marker the counterparty announced (the id
    /// form the I6 `n2h-peer` line prints). `None` = the control file carried
    /// no such key ⇒ the b2a leg stays a reasoned gap (the pre-existing
    /// semantics). A present value is REFUSED unless [`announced_b2a_marker`]
    /// accepts it.
    pub peer_b2a_probe_id: Option<String>,
    /// v3.1's dns branch record (see [`ShellDnsReport`]). `None` = the shell
    /// reported no branch at all: no `dns-branch=` line enters the document
    /// (a missing record is the judge's dns gap, never a fabricated one).
    pub dns: Option<ShellDnsReport>,
    /// N2-H I3a (v3.1 §2.1 R-2①): the evidence generation G — the gen of the
    /// last ACCEPTED `create()` of the shell session, as reported by the arm.
    /// `None` = the shell reported none (the control file / arm document
    /// carried no such key): the document then carries the field with no
    /// value (a gap for the judge's three-way `gen` comparison), never a
    /// fabricated generation.
    pub exclusion_generation: Option<u64>,
}

impl Default for ArmOptions {
    fn default() -> Self {
        ArmOptions {
            shell_exclusions: Vec::new(),
            posctl_port: DEFAULT_POSCTL_PORT,
            probe_wait_ms: PROBE_WAIT_MS_DEFAULT,
            build_sha256_reported: String::new(),
            peer_b2a_probe_id: None,
            dns: None,
            exclusion_generation: None,
        }
    }
}

impl ArmOptions {
    /// Parse the arm options JSON strictly (`crate::config::parse_document`).
    /// An empty string means "no options" (all defaults); anything malformed
    /// is REFUSED — a half-understood arm request must not open a window.
    pub fn parse_json(text: &str) -> Result<ArmOptions, String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(ArmOptions::default());
        }
        let doc = crate::config::parse_document(trimmed)
            .map_err(|e| format!("arm options are not a JSON document: {e}"))?;
        let Json::Obj(fields) = &doc else {
            return Err("arm options must be a JSON object".to_string());
        };
        let mut opts = ArmOptions::default();
        if let Some(v) = obj_get(fields, "posctl_port") {
            let port = as_u64(v).ok_or("posctl_port must be a non-negative integer")?;
            if port == 0 || port > u16::MAX as u64 {
                return Err(format!("posctl_port {port} is not a usable port"));
            }
            opts.posctl_port = port as u16;
        }
        if let Some(v) = obj_get(fields, "probe_wait_ms") {
            let wait = as_u64(v).ok_or("probe_wait_ms must be a non-negative integer")?;
            if wait == 0 {
                return Err("probe_wait_ms 0 is not a usable wait window".to_string());
            }
            opts.probe_wait_ms = wait;
        }
        if let Some(v) = obj_get(fields, "build_sha256_reported") {
            let sha = as_str(v).ok_or("build_sha256_reported must be a string")?;
            opts.build_sha256_reported = sha.to_string();
        }
        if let Some(v) = obj_get(fields, "peer_b2a_probe_id") {
            // §13.10: the announced marker is external input; it is accepted
            // only in the frozen wire shape and anything else refuses the arm.
            let raw = as_str(v).ok_or("peer_b2a_probe_id must be a string")?;
            opts.peer_b2a_probe_id = Some(announced_b2a_marker(raw)?);
        }
        if let Some(v) = obj_get(fields, "exclusion_generation") {
            // N2-H I3a (v3.1 §2.1 R-2①): the shell's accepted generation —
            // a non-negative integer or nothing. A value of another type is
            // refused (fail-closed, the same discipline as every other
            // option); an ABSENT key is allowed and stays `None`, which the
            // document renders as a value-less field (the judge's C2 gap)
            // rather than as a generation core invented.
            opts.exclusion_generation =
                Some(as_u64(v).ok_or("exclusion_generation must be a non-negative integer")?);
        }
        if let Some(v) = obj_get(fields, "dns") {
            // v3.1's branch record: the branch token is an allow-list and the
            // two text fields are strictly shaped — a half-understood record
            // must not reach the evidence document, so anything else refuses
            // the arm (fail-closed, same discipline as every other option).
            let entry = as_obj(v).ok_or("dns must be an object")?;
            let branch =
                obj_get(entry, "branch").and_then(as_str).ok_or("dns.branch is missing")?;
            if !ShellDnsReport::BRANCHES.contains(&branch) {
                return Err(format!(
                    "dns.branch '{branch}' is not one of the frozen branch tokens"
                ));
            }
            let evidence = obj_get(entry, "physical_evidence")
                .and_then(as_str)
                .ok_or("dns.physical_evidence is missing")?;
            let gap_reason = match obj_get(entry, "gap_reason") {
                Some(g) => {
                    let raw = as_str(g).ok_or("dns.gap_reason must be a string")?;
                    Some(dns_report_token("gap_reason", raw)?)
                }
                None => None,
            };
            opts.dns = Some(ShellDnsReport {
                branch: branch.to_string(),
                physical_evidence: dns_report_token("physical_evidence", evidence)?,
                gap_reason,
            });
        }
        if let Some(v) = obj_get(fields, "shell_exclusions") {
            let Json::Arr(items) = v else {
                return Err("shell_exclusions must be an array".to_string());
            };
            for item in items {
                let Json::Obj(entry) = item else {
                    return Err("each shell exclusion must be an object".to_string());
                };
                let kind_name = obj_get(entry, "kind")
                    .and_then(as_str)
                    .ok_or("shell exclusion 'kind' is missing")?;
                let kind = EndpointKind::from_name(kind_name)
                    .ok_or_else(|| format!("unknown shell exclusion kind '{kind_name}'"))?;
                let host = obj_get(entry, "host")
                    .and_then(as_str)
                    .ok_or("shell exclusion 'host' is missing")?;
                let port = obj_get(entry, "port")
                    .and_then(as_u64)
                    .ok_or("shell exclusion 'port' is missing")?;
                if host.trim().is_empty() || port == 0 || port > u16::MAX as u64 {
                    return Err(format!("shell exclusion '{host}' has no usable host/port"));
                }
                let resolved =
                    obj_get(entry, "resolved").and_then(as_str).unwrap_or("").to_string();
                opts.shell_exclusions.push(ShellExclusion::with_resolved(
                    kind,
                    host,
                    port as u16,
                    &resolved,
                ));
            }
        }
        Ok(opts)
    }
}

// ---------------------------------------------------------------------------
// window state machine (§2.2)
// ---------------------------------------------------------------------------

struct WindowState {
    phase: &'static str,
    opened_mono: u64,
    deadline_mono: u64,
    closed_mono: u64,
    probes_sent: usize,
    receipts_observed: usize,
    verdict: Option<String>,
    document: Option<String>,
    sha256: String,
    chunks: Vec<EvidenceChunk>,
}

struct Window {
    state: Mutex<WindowState>,
    tee: Arc<CollectorTee>,
    host: Arc<dyn CollectorHost>,
    options: ArmOptions,
    clock: Arc<dyn CollectorClock>,
    facts: HostFreezeFacts,
    freeze: Vec<FrozenEndpoint>,
    disarmed: AtomicBool,
    arm_notes: Mutex<Vec<String>>,
}

impl Window {
    fn is_open(&self) -> bool {
        self.state.lock_poison().phase != PHASE_CLOSED
    }

    fn disarmed(&self) -> bool {
        self.disarmed.load(Ordering::Acquire)
    }

    fn set_phase(&self, phase: &'static str) {
        self.state.lock_poison().phase = phase;
    }

    fn set_counts(&self, probes_sent: usize, receipts_observed: usize) {
        let mut st = self.state.lock_poison();
        st.probes_sent = probes_sent;
        st.receipts_observed = receipts_observed;
    }
}

static WINDOW: Mutex<Option<Arc<Window>>> = Mutex::new(None);

fn window_slot() -> MutexGuard<'static, Option<Arc<Window>>> {
    WINDOW.lock_poison()
}

/// `n2h_collector_arm(optsJson)` (design §5.1). Synchronous: it validates,
/// freezes, attaches the tee and starts the bounded window thread; the window
/// itself runs off the caller's thread and is observed through
/// [`status_json`] / [`evidence_json`].
///
/// Fail-closed order (§2.2): no host seam → `no-connector`; not fed →
/// `device-not-up`; no established session → `tunnel-not-ready`; a window
/// already open → `already-armed`; no device to attach the tee to →
/// `no-connector` (a window without frame observation would be a false
/// measurement).
pub fn arm_json(opts_json: &str) -> String {
    let options = match ArmOptions::parse_json(opts_json) {
        Ok(o) => o,
        Err(e) => return arm_err(ARM_ERR_INVALID_OPTS, &e),
    };
    {
        let slot = window_slot();
        if let Some(open) = slot.as_ref().filter(|w| w.is_open()) {
            let opened = open.state.lock_poison().opened_mono;
            return arm_err(ARM_ERR_ALREADY_ARMED, &format!("window opened at mono={opened}"));
        }
    }
    let Some(host) = installed_host() else {
        return arm_err(ARM_ERR_NO_CONNECTOR, "no host seam installed in this process");
    };
    let dataplane = match host.dataplane() {
        Ok(d) => d,
        Err(e) => return arm_err(ARM_ERR_NO_CONNECTOR, &e),
    };
    if !dataplane.device_up {
        return arm_err(ARM_ERR_DEVICE_NOT_UP, "wg.device_up is false");
    }
    if !dataplane.ready {
        return arm_err(ARM_ERR_TUNNEL_NOT_READY, "wg.ready is false (no established session)");
    }
    let clock = system_clock();
    let mut arm_notes: Vec<String> = Vec::new();
    // v3.1 C1 dns: the shell's branch record is carried into the document
    // verbatim (branch + physical evidence + the gap it recorded, if any)
    if let Some(dns) = &options.dns {
        arm_notes.extend(dns.notes());
    }
    let facts = match host.freeze_facts() {
        Ok(f) => f,
        Err(e) => {
            // a freeze-fact read failure is a RECORDED gap, not an arm refusal:
            // the document must show which kinds could not be frozen (§4.0)
            arm_notes.push(format!("core freeze facts unavailable at arm: {e}"));
            HostFreezeFacts::default()
        }
    };
    // the mapping's own notes (v3.1 dns caliber: overlay-internal exclusions
    // and every unusable `dns.servers[]` entry) travel with the facts
    arm_notes.extend(facts.notes.iter().cloned());
    let sources = FreezeSources {
        shell_exclusions: options.shell_exclusions.clone(),
        management_dial: facts.management_dial.clone(),
        signal_config: facts.signal_config.clone(),
        relay_advertised: facts.relay_advertised.clone(),
        stun_uris: facts.stun_uris.clone(),
        turn_uris: facts.turn_uris.clone(),
        wg_peer_endpoints: facts.wg_peer_endpoints.clone(),
        dns_servers: facts.dns_servers.clone(),
        frozen_at_unix: clock.unix_s(),
        frozen_at_mono: clock.mono_ms(),
    };
    let freeze = assemble_freezes(&sources);
    let tee = Arc::new(CollectorTee::new(clock.clone()));
    if let Err(e) = host.attach_tee(tee.clone() as Arc<dyn TunFrameTee>) {
        return arm_err(ARM_ERR_NO_CONNECTOR, &format!("tee attach failed: {e}"));
    }
    // §13.10: the operator-relayed announcement reaches the host seam HERE —
    // after every fail-closed gate and the tee attach succeeded, before the
    // window thread exists. Always called (an arm without the key hands in an
    // empty report), so a window never reads a previous window's announcement;
    // a refused arm above never touches the store.
    host.note_peer_announcement(PeerReport {
        b2a_probe_id: options.peer_b2a_probe_id.clone(),
        unknown_peer_drops: None,
    });
    let opened_mono = clock.mono_ms();
    let window = Arc::new(Window {
        state: Mutex::new(WindowState {
            phase: PHASE_ARMED,
            opened_mono,
            deadline_mono: opened_mono + WINDOW_DEADLINE_MS,
            closed_mono: 0,
            probes_sent: 0,
            receipts_observed: 0,
            verdict: None,
            document: None,
            sha256: String::new(),
            chunks: Vec::new(),
        }),
        tee,
        host,
        options,
        clock,
        facts,
        freeze,
        disarmed: AtomicBool::new(false),
        arm_notes: Mutex::new(arm_notes),
    });
    *window_slot() = Some(window.clone());
    let thread_window = window.clone();
    let spawned = std::thread::Builder::new()
        .name("n2h-collector".into())
        .spawn(move || run_window(thread_window));
    if let Err(e) = spawned {
        let _ = window.host.detach_tee();
        // the window never existed: take the announcement back out, so no
        // refused arm leaves a stored marker behind (§13.10)
        window.host.note_peer_announcement(PeerReport::default());
        *window_slot() = None;
        return arm_err(ARM_ERR_WINDOW_FAILED, &format!("{e}"));
    }
    hilog::emit(&format!(
        "{MARK_ARM}|requestId=core|opened_mono={opened_mono}|deadline_mono={}|frozen={}|shell_exclusions={}|diag=true|is_evidence=false",
        opened_mono + WINDOW_DEADLINE_MS,
        window.freeze.len(),
        window.options.shell_exclusions.len()
    ));
    format!("{{{},{},\"opened_mono\":{opened_mono}}}", jbool("ok", true), jbool("armed", true))
}

/// `n2h_collector_status()` (design §5.1). Never armed → the §6.1 shape
/// `{ok:false,error:"not-armed"}`; otherwise the §5.1 status object.
pub fn status_json() -> String {
    let slot = window_slot();
    let Some(w) = slot.as_ref() else {
        return format!("{{{},{}}}", jbool("ok", false), jstr("error", "not-armed"));
    };
    let st = w.state.lock_poison();
    let armed = st.phase != PHASE_CLOSED;
    let verdict = match (&st.verdict, armed) {
        (Some(v), false) => format!(",\"verdict\":\"{v}\""),
        _ => String::new(),
    };
    format!(
        "{{{},\"armed\":{},\"phase\":\"{}\",\"frames_scanned\":{},\"hits\":{},\"probes_sent\":{},\
         \"receipts_observed\":{},\"window\":{{\"opened_mono\":{},\"deadline_mono\":{},\
         \"closed_mono\":{}}}{verdict}}}",
        jbool("ok", true),
        armed,
        st.phase,
        w.tee.frames_scanned(),
        w.tee.outer_hit_count(),
        st.probes_sent,
        st.receipts_observed,
        st.opened_mono,
        st.deadline_mono,
        st.closed_mono,
    )
}

/// `n2h_collector_evidence(chunk)` (design §5.1): `-1` = the chunk count,
/// otherwise one chunk. Repeatable by design — the same document always
/// re-encodes byte-identically, which IS the hilog-line replay (§2.4-R2).
pub fn evidence_json(chunk: i64) -> String {
    let slot = window_slot();
    let Some(w) = slot.as_ref() else {
        return format!("{{{}}}", jstr("error", EVIDENCE_ERR_NONE));
    };
    let st = w.state.lock_poison();
    if st.phase != PHASE_CLOSED {
        return format!("{{{}}}", jstr("phase", "running"));
    }
    let total = st.chunks.len();
    if chunk < 0 {
        return format!(
            "{{\"seq\":-1,\"total\":{total},{},\"b64\":\"\"}}",
            jstr("sha256", &st.sha256)
        );
    }
    match st.chunks.get(chunk as usize) {
        Some(c) => format!(
            "{{{},\"total\":{},{},{}}}",
            jinum("seq", c.seq as i64),
            total,
            jstr("sha256", &c.sha256),
            jstr("b64", &c.b64)
        ),
        None => format!("{{{}}}", jstr("error", EVIDENCE_ERR_RANGE)),
    }
}

/// `n2h_collector_disarm()` (design §5.1/§2.2 中止). For an OPEN window it
/// detaches the tee synchronously (the data-plane cost ends with the call),
/// discards the partial window and records the abort. A window that already
/// CLOSED is left alone: its document is final and stays cached for the
/// §2.4-R2 replay (only an *unclosed* product is discarded).
/// Idempotent: `{ok:true}` always.
pub fn disarm_json() -> String {
    let phase = window_slot().as_ref().map(|w| w.state.lock_poison().phase);
    let Some(phase) = phase else {
        return format!("{{{}}}", jbool("ok", true));
    };
    if phase == PHASE_CLOSED {
        let (frames, hits) = match window_slot().as_ref() {
            Some(w) => (w.tee.frames_scanned(), w.tee.outer_hit_count()),
            None => (0, 0),
        };
        hilog::emit(&format!(
            "{MARK_DISARM}|phase=closed|frames_scanned={frames}|hits={hits}|tee_detached=true|\
             discarded=false|retained=true|note=finalized document kept for replay|\
             diag=true|is_evidence=false"
        ));
        return format!("{{{}}}", jbool("ok", true));
    }
    let Some(w) = window_slot().take() else {
        return format!("{{{}}}", jbool("ok", true));
    };
    w.disarmed.store(true, Ordering::Release);
    let detach = w.host.detach_tee();
    let frames = w.tee.frames_scanned();
    let error = detach.err().unwrap_or_default();
    hilog::emit(&format!(
        "{MARK_DISARM}|phase={phase}|frames_scanned={frames}|tee_detached={}|discarded=true|\
         error={error}|diag=true|is_evidence=false",
        error.is_empty()
    ));
    format!("{{{}}}", jbool("ok", true))
}

fn arm_err(token: &str, detail: &str) -> String {
    hilog::emit(&format!(
        "{MARK_ARM}|refused|error={token}|detail={}|diag=true|is_evidence=false",
        crate::util::sanitize_marker_field(detail, 200)
    ));
    format!("{{{},{}}}", jbool("ok", false), jstr("error", token))
}

// ---------------------------------------------------------------------------
// the window thread (§2.2's lifecycle: freeze → before → probes → posctl →
// settle → after → judge → chunked output)
// ---------------------------------------------------------------------------

fn run_window(win: Arc<Window>) {
    // I7: the build identity is no longer a `notes[]` line (the I5 placeholder
    // announced its own replacement, §13.6) — the trailing `collector{}` block
    // below carries the digest AND its source, and is the document's last key.
    let mut notes = win.arm_notes.lock_poison().clone();

    // -- window OPEN: the `before` edge of the counters pairing (§4.4) --
    let open_mono = win.clock.mono_ms();
    let open_unix = win.clock.unix_s();
    let before = match win.host.paired_read(open_mono, open_unix) {
        Ok(b) => Some(b),
        Err(e) => {
            notes.push(format!("window-open counters read failed (no before edge): {e}"));
            None
        }
    };
    let peer_baseline = win.host.peer_report().unknown_peer_drops;

    // -- outer probes: one per PRESENT frozen endpoint, transport = the kind --
    win.set_phase(PHASE_PROBING);
    let mut probes: Vec<OuterProbe> = Vec::new();
    let mut receipts: Vec<EndpointReceipt> = Vec::new();
    for f in win.freeze.iter().filter(|f| f.present) {
        let Some(kind) = f.kind else { continue };
        if win.disarmed() {
            return;
        }
        let deadline_mono = win.state.lock_poison().deadline_mono;
        if win.clock.mono_ms() >= deadline_mono {
            notes.push(format!(
                "window deadline {WINDOW_DEADLINE_MS} ms reached before the '{}' probe was sent (bounded window; remaining probes recorded not-sent)",
                kind.name()
            ));
            break;
        }
        let fallback_nonce = crate::n2h::fresh_nonce_hex()
            .unwrap_or_else(|_| "000000000000000000000000".to_string());
        let fallback_id = probe_id(kind.name(), &fallback_nonce);
        let dst: SocketAddr = match f.resolved.parse() {
            Ok(a) => a,
            Err(_) => {
                notes.push(format!(
                    "probe not sent for '{}': frozen resolved '{}' is not an ip:port literal (the device sender targets shell-resolved endpoints, §4.2)",
                    kind.name(),
                    f.resolved
                ));
                probes.push(OuterProbe {
                    probe_id: fallback_id.clone(),
                    kind: Some(kind),
                    expected_path: "outer",
                    sent: false,
                    ..OuterProbe::default()
                });
                receipts.push(unobserved_receipt(
                    &fallback_id,
                    kind,
                    "none",
                    "probe never left: the frozen endpoint is not an ip:port literal",
                ));
                continue;
            }
        };
        match send_probe(kind, dst, win.options.probe_wait_ms) {
            Ok(sent) => {
                win.tee.register(&sent.probe_id, sent.marker.clone(), false);
                probes.push(OuterProbe {
                    probe_id: sent.probe_id.clone(),
                    kind: Some(kind),
                    tuple: sent.tuple.clone(),
                    expected_path: "outer",
                    sent: true,
                    sent_at_unix: win.clock.unix_s(),
                    sent_at_mono: win.clock.mono_ms(),
                    marker: Some(sent.marker.clone()),
                    payload_len: sent.payload_len,
                });
                receipts.push(initial_receipt(&win, kind, &sent));
                win.set_counts(
                    probes.iter().filter(|p| p.sent).count(),
                    receipts.iter().filter(|r| r.observed).count(),
                );
            }
            Err(e) => {
                notes.push(format!("probe send failure for '{}' ({dst}): {e}", kind.name()));
                probes.push(OuterProbe {
                    probe_id: fallback_id.clone(),
                    kind: Some(kind),
                    expected_path: "outer",
                    sent: false,
                    ..OuterProbe::default()
                });
                receipts.push(unobserved_receipt(
                    &fallback_id,
                    kind,
                    "none",
                    "probe never left the process",
                ));
            }
        }
    }

    // -- positive control (§4.3): a2b is ours, b2a is the counterparty's --
    win.set_phase(PHASE_OBSERVING);
    let mut posctl = build_posctl(&win, &mut notes);
    observe(&win, &probes, &mut receipts, &mut posctl, peer_baseline, &mut notes);

    // -- window CLOSE (§2.2): the `after` edge FIRST, then the tee detach
    //    ("窗口关 = tee detach、证据文档定稿") --
    let close_mono = win.clock.mono_ms();
    let close_unix = win.clock.unix_s();
    let sample = match &before {
        Some(b) => match close_window_host(&win.host, b, close_mono, close_unix) {
            Ok(s) => Some(s),
            Err(e) => {
                notes.push(format!("window-close counters read failed (no after edge): {e}"));
                None
            }
        },
        None => None,
    };
    if let Err(e) = win.host.detach_tee() {
        notes.push(format!("tee detach failed at window close: {e}"));
    }
    if win.disarmed() {
        return;
    }
    let scan = win.tee.snapshot();
    let evidence = assemble_device_evidence(WindowMaterial {
        freeze: win.freeze.clone(),
        probes,
        posctl,
        receipts,
        scan,
        sample,
        notes,
        opened_mono: win.state.lock_poison().opened_mono,
        closed_mono: close_mono,
        generated_unix: win.clock.unix_s(),
    });
    let document = with_collector_block(
        &evidence.to_json(),
        &win.options.build_sha256_reported,
        win.options.exclusion_generation,
    );
    let sha256 = crate::util::sha256_hex(document.as_bytes());
    let chunks = encode_evidence(document.as_bytes());
    for chunk in &chunks {
        hilog::emit(&chunk.line());
    }
    let (verdict, frames, hits, sent, observed) = {
        let mut st = win.state.lock_poison();
        st.phase = PHASE_CLOSED;
        st.closed_mono = win.clock.mono_ms();
        st.verdict = Some(evidence.verdict.clone());
        st.sha256 = sha256.clone();
        st.chunks = chunks.clone();
        st.document = Some(document);
        (
            evidence.verdict.clone(),
            evidence.tun_negative.frames_scanned,
            evidence.tun_negative.hits.len(),
            evidence.probes.iter().filter(|p| p.sent).count(),
            evidence.endpoint_side.iter().filter(|r| r.observed).count(),
        )
    };
    win.set_counts(sent, observed);
    hilog::emit(&format!(
        "{MARK_CLOSE}|phase=closed|verdict={verdict}|frames_scanned={frames}|hits={hits}|\
         probes_sent={sent}|receipts_observed={observed}|chunks={}|sha256={sha256}|\
         diag=true|is_evidence=false",
        chunks.len()
    ));
}

/// The first `endpoint_side[]` record of one sent probe (design §4.0 matrix):
/// STUN proves itself (txid-matched Binding Response, endpoint-reported);
/// wg_peer's receipt is the counterparty's counter, recorded in the host's own
/// pre-confirmation shape (`n2h.rs:1474-1486`) and flipped by the observe loop;
/// every other kind is a recorded gap (`method:"none"`) until a host seam
/// observes it.
fn initial_receipt(win: &Arc<Window>, kind: EndpointKind, sent: &SentProbe) -> EndpointReceipt {
    if kind == EndpointKind::Stun {
        let (observed, physical_src, detail) = match sent.mapped {
            Some((ip, port)) => (
                true,
                format!("{}.{}.{}.{}:{port}", ip[0], ip[1], ip[2], ip[3]),
                "txid-matched Binding Success Response (XOR/MAPPED-ADDRESS = the physical source the endpoint saw)"
                    .to_string(),
            ),
            None => (
                false,
                String::new(),
                format!(
                    "no txid-matched Binding Response within the {} ms probe wait (record kept; a later response may still confirm)",
                    win.options.probe_wait_ms
                ),
            ),
        };
        return EndpointReceipt {
            probe_id: sent.probe_id.clone(),
            kind: kind.name().to_string(),
            observed,
            physical_src,
            physical_src_source: "endpoint-reported".into(),
            at_unix: win.clock.unix_s(),
            method: "stun-binding-response".into(),
            detail,
        };
    }
    if let Some(fact) = win.host.probe_receipt(&sent.probe_id, &sent.marker) {
        return EndpointReceipt {
            probe_id: sent.probe_id.clone(),
            kind: kind.name().to_string(),
            observed: true,
            physical_src: fact.physical_src,
            physical_src_source: fact.physical_src_source,
            at_unix: win.clock.unix_s(),
            method: fact.method,
            detail: fact.detail,
        };
    }
    if kind == EndpointKind::WgPeer {
        return EndpointReceipt {
            probe_id: sent.probe_id.clone(),
            kind: kind.name().to_string(),
            observed: false,
            physical_src: sent.tuple.src.clone(),
            physical_src_source: "probe-socket-getsockname".into(),
            at_unix: win.clock.unix_s(),
            method: "peer-device-offpath-drop".into(),
            detail: "datagram sent to the frozen WG peer outer endpoint; the receipt is the \
                     counterparty's own unknown_peer_drops delta (§4.0) — not yet observed"
                .into(),
        };
    }
    unobserved_receipt(
        &sent.probe_id,
        kind,
        "none",
        "outer-path delivery unproven from inside the device: this endpoint's delivery log is \
         operator-side (§4.0)",
    )
}

fn unobserved_receipt(
    probe_id: &str,
    kind: EndpointKind,
    method: &str,
    detail: &str,
) -> EndpointReceipt {
    EndpointReceipt {
        probe_id: probe_id.to_string(),
        kind: kind.name().to_string(),
        observed: false,
        physical_src: String::new(),
        physical_src_source: String::new(),
        at_unix: 0,
        method: method.to_string(),
        detail: detail.to_string(),
    }
}

/// Build both positive-control legs (§4.3/D5):
/// - **a2b**: the collector SENDS `N2H-posctl-<nonce>|positive-control` to the
///   peer overlay address; observed iff the from-os entrance frame AND the
///   echo return in to-os are both seen;
/// - **b2a**: the counterparty sends the payload; the device can only scan for
///   a marker it was TOLD, so the leg is `sent:true` only when the operator
///   relay of §13.10 handed the announced marker in through the arm options
///   ([`CollectorHost::peer_report`]) — otherwise it is a recorded gap (never
///   guessed, never replaced by the a2b echo).
fn build_posctl(win: &Arc<Window>, notes: &mut Vec<String>) -> Vec<PosctlProbe> {
    let mut out: Vec<PosctlProbe> = Vec::new();
    let peer_ip = win.facts.peer_overlay.first().copied();
    match peer_ip {
        Some(ip) => {
            let dst = SocketAddr::from((ip, win.options.posctl_port));
            let nonce = crate::n2h::fresh_nonce_hex()
                .unwrap_or_else(|_| "000000000000000000000000".to_string());
            let id = probe_id(POSCTL_KIND_TOKEN, &nonce);
            match send_posctl(dst, &id) {
                Ok((_n, src)) => {
                    win.tee.register(&id, WireMarker::Ascii(id.clone()), true);
                    out.push(PosctlProbe {
                        probe_id: id,
                        direction: "a2b".into(),
                        tuple: FiveTuple {
                            proto: "ipv4".into(),
                            src,
                            dst: dst.to_string(),
                        },
                        expected_path: "tunnel",
                        sent: true,
                        sent_at_mono: win.clock.mono_ms(),
                        observed: None,
                        note: "awaiting the from-os tunnel-entrance frame AND the echo return in to-os (§4.3/D5)".into(),
                    });
                }
                Err(e) => {
                    notes.push(format!("posctl a2b send failure to {dst}: {e}"));
                    out.push(PosctlProbe {
                        probe_id: id,
                        direction: "a2b".into(),
                        expected_path: "tunnel",
                        sent: false,
                        note: format!("send failed: {e}"),
                        ..PosctlProbe::default()
                    });
                }
            }
        }
        None => {
            notes.push(
                "posctl a2b not sent: no peer overlay address in the freeze facts (§4.3)".into(),
            );
            out.push(PosctlProbe {
                direction: "a2b".into(),
                expected_path: "tunnel",
                sent: false,
                note: "no peer overlay address (network-map peers[].vpn_addresses empty)".into(),
                ..PosctlProbe::default()
            });
        }
    }
    let report = win.host.peer_report();
    match report.b2a_probe_id {
        Some(id) => {
            win.tee.register(&id, WireMarker::Ascii(id.clone()), true);
            let own = win
                .facts
                .own_overlay
                .map(|a| format!("{}.{}.{}.{}:{}", a[0], a[1], a[2], a[3], win.options.posctl_port))
                .unwrap_or_default();
            let peer = peer_ip
                .map(|a| format!("{}.{}.{}.{}:{}", a[0], a[1], a[2], a[3], win.options.posctl_port))
                .unwrap_or_default();
            out.push(PosctlProbe {
                probe_id: id,
                direction: "b2a".into(),
                tuple: FiveTuple { proto: "ipv4".into(), src: peer, dst: own },
                expected_path: "tunnel",
                sent: true,
                sent_at_mono: win.clock.mono_ms(),
                observed: None,
                note: "peer-initiated: marker announced to this device by the operator relay of §13.10 (arm option peer_b2a_probe_id); observation = the to-os delivery side (§4.3/D5)".into(),
            });
        }
        None => {
            notes.push(
                "posctl b2a not sent: no peer marker was announced to this device (the arm \
                 options carried no peer_b2a_probe_id — §13.10's operator relay)"
                    .into(),
            );
            out.push(PosctlProbe {
                direction: "b2a".into(),
                expected_path: "tunnel",
                sent: false,
                note: "the peer's b2a marker was never announced to this device (the arm options carried no peer_b2a_probe_id)"
                    .into(),
                ..PosctlProbe::default()
            });
        }
    }
    out
}

/// Send one positive-control datagram from a fresh WILDCARD socket (§4.2's
/// socket discipline) with the host payload shape (§4.3).
fn send_posctl(dst: SocketAddr, id: &str) -> Result<(usize, String), String> {
    let sock = std::net::UdpSocket::bind(PROBE_BIND_WILDCARD)
        .map_err(|e| format!("posctl socket bind: {e}"))?;
    let src = sock.local_addr().map_err(|e| format!("posctl socket addr: {e}"))?;
    let payload = posctl_payload(id);
    let n = sock
        .send_to(payload.as_bytes(), dst)
        .map_err(|e| format!("posctl send: {e}"))?;
    Ok((n, src.to_string()))
}

/// The observation loop (host shape, `n2h.rs:1643-1751`): bounded 10 ms beats
/// until every leg is closed, then a settle tail; the deadline is the exit.
/// No long sleep is ever the mechanism.
fn observe(
    win: &Arc<Window>,
    probes: &[OuterProbe],
    receipts: &mut Vec<EndpointReceipt>,
    posctl: &mut [PosctlProbe],
    peer_baseline: Option<u64>,
    notes: &mut Vec<String>,
) {
    let deadline = win.clock.mono_ms() + OBSERVE_DEADLINE_MS;
    let mut settle = 0u32;
    let mut wg_confirmed = false;
    loop {
        if win.disarmed() {
            return;
        }
        // 1. late endpoint-side receipts (offline harness sink records; the
        //    production seam answers None — its endpoints are real services)
        for p in probes.iter().filter(|p| p.sent) {
            if receipts.iter().any(|r| r.probe_id == p.probe_id && r.observed) {
                continue;
            }
            let Some(marker) = p.marker.as_ref() else { continue };
            if let Some(fact) = win.host.probe_receipt(&p.probe_id, marker) {
                let kind = p.kind.map(|k| k.name()).unwrap_or("unknown").to_string();
                receipts.push(EndpointReceipt {
                    probe_id: p.probe_id.clone(),
                    kind,
                    observed: true,
                    physical_src: fact.physical_src,
                    physical_src_source: fact.physical_src_source,
                    at_unix: win.clock.unix_s(),
                    method: fact.method,
                    detail: fact.detail,
                });
            }
        }
        // 2. the wg_peer receipt: the counterparty's own off-path drop count
        if !wg_confirmed {
            if let (Some(base), Some(now)) = (peer_baseline, win.host.peer_report().unknown_peer_drops)
            {
                if now > base {
                    if let Some(r) = receipts.iter_mut().find(|r| r.kind == "wg_peer") {
                        r.observed = true;
                        r.detail = format!(
                            "counted at the counterparty's outer socket: unknown_peer_drops delta = {} (off-path demux drop — the outer path carried it, §4.0)",
                            now - base
                        );
                    }
                    wg_confirmed = true;
                }
            }
        }
        // 3. positive controls from the tee (D5's two observation points)
        for p in posctl.iter_mut().filter(|p| p.sent && p.observed.is_none()) {
            if p.direction == "a2b" {
                let entrance = win.tee.directed_hit(&p.probe_id, TunFrameDir::FromOs);
                let echo = win.tee.directed_hit(&p.probe_id, TunFrameDir::ToOs);
                match (entrance, echo) {
                    (Some(e), Some(r)) => {
                        p.note = format!(
                            "a2b closed end to end: from-os entrance frame observed (len={}, mono={}) AND the echo return observed in to-os (len={}, mono={}) (§4.3/D5)",
                            e.frame_len, e.observed_at_mono, r.frame_len, r.observed_at_mono
                        );
                        p.observed = Some(e);
                    }
                    (Some(e), None) => {
                        p.note = format!(
                            "a2b entered the tunnel (from-os, len={}, mono={}) but the echo return was NOT observed in to-os inside the window — the end-to-end closure is missing (§4.3/D5)",
                            e.frame_len, e.observed_at_mono
                        );
                    }
                    (None, Some(r)) => {
                        p.note = format!(
                            "a2b echo seen in to-os (len={}, mono={}) but the from-os tunnel-entrance frame was not — the entrance leg is unproven (§4.3/D5)",
                            r.frame_len, r.observed_at_mono
                        );
                    }
                    (None, None) => {
                        p.note = "a2b observed in neither direction inside the window (no tunnel entrance frame, no echo return)"
                            .into();
                    }
                }
            } else if let Some(hit) = win.tee.directed_hit(&p.probe_id, TunFrameDir::ToOs) {
                p.note = format!(
                    "b2a observed at the receiver's TUN delivery side (to-os, len={}, mono={}) (§4.3/D5)",
                    hit.frame_len, hit.observed_at_mono
                );
                p.observed = Some(hit);
            } else {
                p.note = "b2a not observed in the to-os direction inside the window (the peer-initiated payload never reached this TUN)"
                    .into();
            }
        }
        let sent_probes: usize = probes.iter().filter(|p| p.sent).count();
        let observed_receipts: usize = receipts.iter().filter(|r| r.observed).count();
        win.set_counts(sent_probes, observed_receipts);
        let all_receipts = probes.iter().filter(|p| p.sent).all(|p| {
            receipts.iter().any(|r| r.probe_id == p.probe_id && r.observed)
        });
        let all_posctl = posctl.iter().filter(|p| p.sent).all(|p| p.observed.is_some());
        if all_receipts && all_posctl {
            settle += 1;
            if settle >= OBSERVE_SETTLE_ROUNDS {
                return;
            }
        } else {
            settle = 0;
        }
        if win.clock.mono_ms() >= deadline {
            notes.push(format!(
                "observation deadline {OBSERVE_DEADLINE_MS} ms reached with legs still open; the document records the gaps"
            ));
            return;
        }
        crate::sys::sleep_ms(OBSERVE_BEAT_MS);
    }
}

/// Window close through the host's paired read, with the bounded resample of
/// design §4.4 (the device-side twin of I4's `close_window`, which takes a
/// [`LedgerReads`] rather than the host seam).
fn close_window_host(
    host: &Arc<dyn CollectorHost>,
    before: &PairedRead,
    at_mono: u64,
    at_unix: i64,
) -> Result<WindowSample, String> {
    let mut resamples = 0u32;
    loop {
        let read = host.paired_read(at_mono, at_unix)?;
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

/// One paired read for the OFFLINE `tee-fallback` caliber (§3.2): the I2 tee
/// totals stand in for `/proc/net/dev`, so a harness never invents a ledger —
/// and the caliber is declared in `notes[]` through
/// [`counters_source_note`].
pub fn tee_fallback_read(
    totals: &TunTeeTotals,
    wg: WgDeviceStats,
    at_mono: u64,
    at_unix: i64,
) -> PairedRead {
    PairedRead {
        at_mono,
        at_unix,
        tun: tee_fallback_row(totals),
        wg,
        source: COUNTERS_SOURCE_TEE_FALLBACK,
    }
}

// ---------------------------------------------------------------------------
// I7 — `collector{}`: the build self-identification block (design §3.3/A-1)
// ---------------------------------------------------------------------------

/// Embedded build digest (design §3.3 `build_sha256`), injected by
/// `client/core/build.rs` (I7). Non-empty and reproducible by construction:
/// build.rs refuses to emit anything but a 64-hex digest, and its default
/// caliber is the deterministic digest of the crate's own build inputs
/// (`client/core/build.sh` step 7 recomputes it independently and greps the
/// shipped `.so` for it).
pub const BUILD_SHA256: &str = env!("N2H_BUILD_SHA256");

/// `target_triple` (design §3.3), forwarded by build.rs: `env!("TARGET")` is a
/// BUILD SCRIPT variable and is refused for the crate compilation, so the
/// value is relayed as `N2H_BUILD_TARGET` (same string, same meaning).
pub const BUILD_TARGET_TRIPLE: &str = env!("N2H_BUILD_TARGET");

/// §3.3 `build_source`: the digest was injected at build time.
pub const BUILD_SOURCE_EMBEDDED: &str = "embedded";

/// §3.3 `build_source`: the digest came from the operator's arm options
/// (`build_sha256_reported`) — the design's fallback, labelled as such.
pub const BUILD_SOURCE_OPERATOR_REPORTED: &str = "operator-reported";

/// The trailing `collector{}` block of the device document (design §3.3, D10).
///
/// `build_sha256` precedence: a non-empty operator report from the arm options
/// (`build_sha256_reported`, §3.3's fallback) wins and is labelled
/// `operator-reported` — that value is what can bind the document to the ONE
/// artifact installed on the device (a HAP digest is not reproducible, design
/// §13.10⑦); otherwise the embedded [`BUILD_SHA256`] is used and labelled
/// `embedded` (the reproducible source caliber). Neither branch can be empty.
///
/// N2-H I3a (v3.1 §2.1 R-2①): `exclusion_generation` is APPENDED as the
/// block's last field — the evidence generation G the shell reported at arm
/// (the gen of its last ACCEPTED `create()`), the value the judge compares
/// verbatim against the `N2H_COLLECTOR_ARMED` marker and the last accepted
/// `VPN_EXCLUSION_GENERATION` line. `None` renders as `null` (the shell
/// reported no generation): the field is always present, its VALUE is never
/// invented.
pub fn collector_block_json(
    reported_sha256: &str,
    exclusion_generation: Option<u64>,
) -> String {
    let reported = reported_sha256.trim();
    let (sha256, source) = if reported.is_empty() {
        (BUILD_SHA256, BUILD_SOURCE_EMBEDDED)
    } else {
        (reported, BUILD_SOURCE_OPERATOR_REPORTED)
    };
    let generation = match exclusion_generation {
        Some(gen) => gen.to_string(),
        None => "null".to_string(),
    };
    format!(
        "\"collector\":{{{},{},{},{},\"exclusion_generation\":{}}}",
        jstr("crate_version", env!("CARGO_PKG_VERSION")),
        jstr("target_triple", BUILD_TARGET_TRIPLE),
        jstr("build_sha256", sha256),
        jstr("build_source", source),
        generation
    )
}

/// Tail-append the `collector{}` block to the frozen renderer's output (design
/// §3.1's tail-append discipline, precedent a1947b9's `wg_peers`): nothing is
/// inserted between the existing fields, `n2h::Evidence::to_json` stays the ONE
/// renderer and untouched, and the block is the document's last key.
pub fn with_collector_block(
    document: &str,
    reported_sha256: &str,
    exclusion_generation: Option<u64>,
) -> String {
    let body = document
        .strip_suffix('}')
        .expect("n2h::Evidence::to_json always renders one JSON object");
    format!(
        "{body},{}}}",
        collector_block_json(reported_sha256, exclusion_generation)
    )
}

// ---------------------------------------------------------------------------
// document assembly + the two checks the judge cannot make
// ---------------------------------------------------------------------------

/// Everything the device document is assembled from (design §3.2's 取数层,
/// one field per evidence leg). Pure input — the assembly performs no IO, so
/// the judge-reaching rules are directly unit-testable.
pub struct WindowMaterial {
    pub freeze: Vec<FrozenEndpoint>,
    pub probes: Vec<OuterProbe>,
    pub posctl: Vec<PosctlProbe>,
    pub receipts: Vec<EndpointReceipt>,
    pub scan: TeeSnapshot,
    pub sample: Option<WindowSample>,
    pub notes: Vec<String>,
    pub opened_mono: u64,
    pub closed_mono: u64,
    pub generated_unix: i64,
}

/// Assemble the device document and judge it — with the TWO rules the frozen
/// judge cannot see, both of which can only make the verdict STRICTER:
///
/// 1. **design §4.4's second identity.** `WindowCounters::reconciled()` (the
///    judge's step 6) checks the BYTE identity only (`n2h.rs:560-563`), so a
///    pairing whose packet identity broke would slip through. When
///    `WindowSample::consistent()` is false the collector appends the reason
///    and forces `n2h-inconclusive` — the resample bound already tried.
/// 2. **§6.3's zero-frame anomaly.** A window in which the tee saw NO frame at
///    all is a collector anomaly, not a clean scan: a negative scan over zero
///    frames proves nothing.
///
/// Falsification is never downgraded: an outer-marker hit stays `n2h-fail`.
pub fn assemble_device_evidence(m: WindowMaterial) -> Evidence {
    let mut notes = m.notes;
    let mut counters: Vec<WindowCounters> = Vec::new();
    let mut identity_reasons: Vec<String> = Vec::new();
    if let Some(sample) = &m.sample {
        counters.push(sample.counters.clone());
        notes.push(counters_source_note(sample.source));
        if sample.resamples > 0 {
            notes.push(format!("counters-pairing-resamples={}", sample.resamples));
        }
        if !sample.identities.bytes {
            notes.push("counters-identity=bytes-broken".to_string());
        }
        if !sample.identities.packets {
            notes.push("counters-identity=packets-broken".to_string());
        }
        if !sample.identities.hold() {
            let d = sample.counters.delta();
            identity_reasons.push(format!(
                "counter reconciliation mismatch (design §4.4 second identity, invisible to \
                 WindowCounters::reconciled): delta.wg.rx_packets {} != delta.tun.delivered_frames \
                 {} (bytes identity holds: {}); {} bounded resample(s) taken, none self-consistent \
                 — recorded, never repaired",
                d.wg_rx_packets,
                d.tun_delivered_frames,
                sample.identities.bytes,
                sample.resamples
            ));
        }
    }
    // D4's direction fact, recorded where the design allows additions: the
    // frozen renderer carries no per-hit direction field.
    for hit in &m.scan.outer_hits {
        if let Some(dir) = m.scan.directed.iter().find(|d| d.probe_id == hit.probe_id) {
            notes.push(format!(
                "outer-hit-direction probe_id={} direction={} side={} frame_len={}",
                hit.probe_id,
                direction_token(dir.direction),
                hit.side,
                hit.frame_len
            ));
        }
    }
    let mut evidence = Evidence {
        mode: MODE_DEVICE.to_string(),
        session: SESSION_DEVICE.to_string(),
        generated_unix: m.generated_unix,
        started_mono: m.opened_mono,
        finished_mono: m.closed_mono,
        frozen_endpoints: m.freeze,
        probes: m.probes,
        tunnel_positive_control: PosctlEvidence { probes: m.posctl },
        tun_negative: TunScanResult {
            hit: !m.scan.outer_hits.is_empty(),
            frames_scanned: m.scan.frames_scanned,
            hits: m.scan.outer_hits.clone(),
            sides: m.scan.sides.clone(),
        },
        endpoint_side: m.receipts,
        counters,
        notes,
        ..Evidence::default()
    };
    let (mut verdict, mut reasons) = evidence.judge();
    reasons.extend(identity_reasons);
    let mut strict_override = m.sample.as_ref().is_some_and(|s| !s.identities.hold());
    if m.scan.frames_scanned == 0 {
        reasons.push(
            "tun_negative: zero frames scanned in the window — the tee observed nothing (the \
             window was too short or the tee was never fed); a clean negative scan over zero \
             frames is not interpretable"
                .to_string(),
        );
        strict_override = true;
    }
    // falsification is never downgraded: a `n2h-fail` stays a fail
    if strict_override && verdict != crate::n2h::VERDICT_FAIL {
        verdict = crate::n2h::VERDICT_INCONCLUSIVE.to_string();
    }
    evidence.verdict = verdict;
    evidence.reasons = reasons;
    evidence
}

// ---------------------------------------------------------------------------
// production host: the connector's live WG feed (design U9)
// ---------------------------------------------------------------------------

/// The PRODUCTION [`CollectorHost`]: the connector's live WG data-plane feed
/// (`crate::wg_device::WgDeviceFeed`) plus the PUBLIC connector documents for
/// the freeze facts (`connector_status()` / `connector_network_config()` — the
/// same JSON the shell reads), PLUS the connector's INTERNAL session knowledge
/// (`crate::connector::core_freeze_inputs()`: `netbird_config.stuns[]` and the
/// management dial address, §7-I5's freeze-source extension — §5.2 keeps both
/// out of the public documents).
///
/// Design U9 asks the collector's dependencies to be an injectable seam with
/// the production side taking them from the connector singleton. This type IS
/// that production side; it is installed by the connector start entries
/// (`connector.rs`'s `install_collector_host`, next to the feed construction)
/// and removed by the stop entry.
///
/// Registration itself is inert: with the host installed but no arm, `arm`
/// answers `{ok:false,error:"device-not-up"|"tunnel-not-ready"}` and nothing
/// else happens — no thread, no tee, no packet (§6.1). Without any host seam
/// (a process that never started a connector) `arm` answers
/// `{ok:false,error:"no-connector"}`.
pub struct WgFeedHost {
    feed: Arc<crate::wg_device::WgDeviceFeed>,
    hdc_capture_path: Option<std::path::PathBuf>,
    /// §13.10's ops relay: the arm-time peer announcement. Replaced on every
    /// accepted arm (empty when the control file carried no marker), read by
    /// the window through [`CollectorHost::peer_report`].
    peer_announcement: Mutex<PeerReport>,
}

impl WgFeedHost {
    /// Host over a live feed, with the operator `/proc` capture path (§4.4's
    /// D-class fallback) wired to [`N2H_HDC_CAPTURE_PATH`].
    pub fn new(feed: Arc<crate::wg_device::WgDeviceFeed>) -> Self {
        WgFeedHost {
            feed,
            hdc_capture_path: Some(std::path::PathBuf::from(N2H_HDC_CAPTURE_PATH)),
            peer_announcement: Mutex::new(PeerReport::default()),
        }
    }

    /// Override the operator capture path (`None` = never consult it).
    pub fn with_hdc_capture_path(mut self, path: Option<std::path::PathBuf>) -> Self {
        self.hdc_capture_path = path;
        self
    }

    fn wg_stats(&self) -> Result<WgDeviceStats, String> {
        self.feed
            .with_device(|d| d.stats())
            .ok_or_else(|| "wg device is not up".to_string())
    }
}

impl CollectorHost for WgFeedHost {
    fn dataplane(&self) -> Result<HostDataplane, String> {
        Ok(HostDataplane {
            device_up: self.feed.device_up(),
            ready: crate::connector::WgPeerApplier::tunnel_ready(&*self.feed),
        })
    }

    /// The live counters of the fed device (`None` while no device is up —
    /// the I6 peer status line then says `unknown_peer_drops=na`).
    fn wg_stats(&self) -> Option<WgDeviceStats> {
        self.feed.with_device(|d| d.stats())
    }

    /// The device-side known peer announcement (§4.3/§13.10), set at arm from
    /// the operator-relayed arm option. BEFORE the first arm (and after an arm
    /// that carried no marker) this is empty: the b2a leg is then a REASONED
    /// gap with its own note, never an invented marker.
    fn peer_report(&self) -> PeerReport {
        self.peer_announcement.lock_poison().clone()
    }

    /// Wholesale replace (this host's only peer source is the arm relay):
    /// `PeerReport::default()` clears, so window N+1 cannot inherit window N's
    /// marker. `unknown_peer_drops` is deliberately dropped: the COUNTERPARTY's
    /// counter is not a value this device holds (see [`PeerReport`]).
    fn note_peer_announcement(&self, report: PeerReport) {
        *self.peer_announcement.lock_poison() = PeerReport {
            b2a_probe_id: report.b2a_probe_id,
            unknown_peer_drops: None,
        };
    }

    fn freeze_facts(&self) -> Result<HostFreezeFacts, String> {
        // public documents (§5.2, unchanged shape) + the connector's INTERNAL
        // session knowledge (`stuns[]`, management dial address) — read at
        // ARM time, so the last applied sync's stun set is what gets frozen
        process_freeze_facts_with_core(
            &crate::connector::connector_status_json(),
            &crate::connector::connector_network_config_json(),
            &crate::connector::core_freeze_inputs(),
        )
    }

    fn attach_tee(&self, tee: Arc<dyn TunFrameTee>) -> Result<(), String> {
        self.feed
            .with_device(|d| d.attach_tun_tee(tee))
            .map(|_replaced| ())
            .ok_or_else(|| "wg device is not up (no tee attached)".to_string())
    }

    fn detach_tee(&self) -> Result<(), String> {
        match self.feed.with_device(|d| d.detach_tun_tee()) {
            Some(_detached) => Ok(()),
            None => Ok(()), // no device ⇒ nothing attached: idempotent
        }
    }

    fn paired_read(&self, at_mono: u64, at_unix: i64) -> Result<PairedRead, String> {
        let mut reads = DeviceLedgerReads {
            hdc_capture_path: self.hdc_capture_path.clone(),
            wg_stats: || self.wg_stats(),
        };
        paired_read(&mut reads, at_mono, at_unix)
    }
}

// ---------------------------------------------------------------------------
// JSON readers used by the production host (pure, fixture-testable)
// ---------------------------------------------------------------------------

fn obj_get<'a>(fields: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn as_u64(v: &Json) -> Option<u64> {
    match v {
        Json::Num(n) => Some(*n),
        _ => None,
    }
}

fn as_str(v: &Json) -> Option<&str> {
    match v {
        Json::Str(s) => Some(s),
        _ => None,
    }
}

fn as_obj(v: &Json) -> Option<&[(String, Json)]> {
    match v {
        Json::Obj(f) => Some(f),
        _ => None,
    }
}

fn as_arr(v: &Json) -> &[Json] {
    match v {
        Json::Arr(items) => items,
        _ => &[],
    }
}

fn parse_addr_literal(s: &str) -> Option<([u8; 4], u16)> {
    let addr: SocketAddr = s.parse().ok()?;
    match addr {
        SocketAddr::V4(v4) => Some((v4.ip().octets(), v4.port())),
        SocketAddr::V6(_) => None, // IPv4-only (design §10-4)
    }
}

fn injected_from_addr(addr: &str) -> Option<InjectedEndpoint> {
    let (ip, port) = parse_addr_literal(addr)?;
    let host = format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
    Some(InjectedEndpoint::with_resolved(&host, port, addr))
}

/// One IPv4 range of the OVERLAY's own coverage (v3.1 C1 dns caliber): the
/// `address/address_prefix_len` of this device plus every `routes[].network`
/// of the applied network map.
///
/// The dns freeze set is exactly the resolvers OUTSIDE these ranges: an
/// overlay-internal resolver (the tunnel's own DNS) must never be frozen —
/// and, even more, never be route-excluded, because a host route for it would
/// push the tunnel's DNS out of the tunnel and open a physical clear-net
/// resolver path (preregistration :68).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OverlayRange {
    /// Network address, host bits masked off.
    net: u32,
    mask: u32,
}

impl OverlayRange {
    /// `a.b.c.d/p`; a bare `a.b.c.d` means /32. The value is MASKED, not
    /// validated for host bits: `address` is a HOST address carrying its
    /// prefix (connector `ShellNetworkConfig::address`), so masking is the
    /// documented reading, not a repair of bad input.
    fn parse(s: &str) -> Option<OverlayRange> {
        let (addr_part, prefix) = match s.split_once('/') {
            None => (s, 32u8),
            Some((a, p)) => (a, p.trim().parse::<u8>().ok()?),
        };
        if prefix > 32 {
            return None;
        }
        let ip = crate::config::parse_ipv4(addr_part.trim())?;
        let mask: u32 = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix as u32) };
        Some(OverlayRange { net: u32::from_be_bytes(ip) & mask, mask })
    }

    fn contains(&self, ip: [u8; 4]) -> bool {
        u32::from_be_bytes(ip) & self.mask == self.net
    }
}

/// The overlay coverage of one `connector_network_config` document: the
/// device's own `address` (+ `address_prefix_len`, default /32 as the core
/// renders it) and every `routes[].network`. An absent/unparsable entry is
/// simply not a range — it can never silently EXCLUDE a resolver from the
/// freeze (the fail-open direction is "nothing is overlay-internal").
fn overlay_ranges(fields: &[(String, Json)]) -> Vec<OverlayRange> {
    let mut out: Vec<OverlayRange> = Vec::new();
    if let Some(addr) = obj_get(fields, "address").and_then(as_str) {
        let prefix = obj_get(fields, "address_prefix_len").and_then(as_u64).unwrap_or(32);
        if let Some(range) = OverlayRange::parse(&format!("{addr}/{prefix}")) {
            out.push(range);
        }
    }
    for route in obj_get(fields, "routes").map(as_arr).unwrap_or(&[]) {
        if let Some(network) =
            as_obj(route).and_then(|r| obj_get(r, "network")).and_then(as_str)
        {
            if let Some(range) = OverlayRange::parse(network) {
                out.push(range);
            }
        }
    }
    out
}

/// Why a `dns.servers[]` entry could not become a freeze (v3.1's strict IPv4
/// `SocketAddr` rule, preregistration :112). Every reason is recorded in
/// `notes[]` — an unusable entry is NEVER silently skipped.
fn dns_entry_unusable_reason(ip: &str, port: Option<u64>) -> &'static str {
    if ip.trim().is_empty() {
        "empty-ip"
    } else if ip.contains(':') {
        "ipv6" // strictly IPv4 freeze set: an IPv6 resolver is a dns gap
    } else if port.is_none() || port == Some(0) {
        "no-port"
    } else {
        "not-strict-ipv4-socketaddr"
    }
}

/// Freeze facts of the production host from the two connector documents
/// (design §4.0's core-session sources). Pure: the documents are arguments, so
/// the mapping is unit-testable without a connector.
///
/// This is the DOCUMENT-ONLY half (kept for callers that have the two public
/// documents and nothing else); the production host uses
/// [`process_freeze_facts_with_core`], which adds the session's INTERNAL
/// sources. Facts the public JSON surface does NOT carry are left empty here
/// and become reasoned absences in the document (never invented):
/// `management_dial` and `stun_uris` — see [`apply_core_freeze_inputs`].
pub fn process_freeze_facts(status_json: &str, network_json: &str) -> Result<HostFreezeFacts, String> {
    process_freeze_facts_with_core(
        status_json,
        network_json,
        &crate::connector::CoreFreezeInputs::default(),
    )
}

/// [`process_freeze_facts`] plus the connector's INTERNAL session knowledge
/// (design §4.0's `stuns[]` and management dial rows, which §5.2 keeps OUT of
/// the public documents). This is exactly what
/// [`WgFeedHost::freeze_facts`] calls, at arm time.
pub fn process_freeze_facts_with_core(
    status_json: &str,
    network_json: &str,
    core: &crate::connector::CoreFreezeInputs,
) -> Result<HostFreezeFacts, String> {
    let status = crate::config::parse_document(status_json)
        .map_err(|e| format!("connector_status document: {e}"))?;
    let network = crate::config::parse_document(network_json)
        .map_err(|e| format!("connector_network_config document: {e}"))?;
    // both documents are produced by this crate's own renderers, so a
    // non-object shape is a bug: the caller records it as an unattributable
    // freeze-facts gap instead of silently reading nothing
    let status_fields = as_obj(&status)
        .ok_or_else(|| "connector_status document is not a JSON object".to_string())?;
    let network_fields = as_obj(&network)
        .ok_or_else(|| "connector_network_config document is not a JSON object".to_string())?;
    let mut facts = HostFreezeFacts::default();

    // -- status: the session's outer WG endpoints + the relay url sets --
    {
        let fields = status_fields;
        if let Some(wg) = obj_get(fields, "wg").and_then(as_obj) {
            for peer in obj_get(wg, "wg_peers").map(as_arr).unwrap_or(&[]) {
                if let Some(endpoint) = as_obj(peer).and_then(|p| obj_get(p, "endpoint")).and_then(as_str)
                {
                    if let Some(ep) = injected_from_addr(endpoint) {
                        facts.wg_peer_endpoints.push(ep);
                    }
                }
            }
        }
        let mut urls: Vec<String> = Vec::new();
        // §4.0: the ADVERTISED set is the relay freeze source; the runtime set
        // only ever repeats it, and is appended so a live url never vanishes
        if let Some(adv) = obj_get(fields, "relay_advertised").and_then(as_obj) {
            for url in obj_get(adv, "advertised_urls").map(as_arr).unwrap_or(&[]) {
                if let Some(u) = as_str(url) {
                    urls.push(u.to_string());
                }
            }
        }
        if let Some(relay) = obj_get(fields, "relay").and_then(as_obj) {
            for url in obj_get(relay, "urls").map(as_arr).unwrap_or(&[]) {
                if let Some(u) = as_str(url) {
                    urls.push(u.to_string());
                }
            }
        }
        for url in urls {
            match crate::relay_client::RelayUrl::parse(&url) {
                Ok(parsed) => facts
                    .relay_advertised
                    .push(InjectedEndpoint::new(parsed.host(), parsed.port())),
                Err(_) => continue, // an unparsable advertisement is not a freeze
            }
        }
    }

    // -- network config: signal, DNS, own + peer overlay addresses --
    {
        let fields = network_fields;
        if let Some(signal) = obj_get(fields, "signal").and_then(as_str) {
            if let Ok((host, port)) = crate::host_sockets::parse_signal_uri(signal) {
                facts.signal_config = Some(InjectedEndpoint::new(&host, port));
            }
        }
        // -- dns (v3.1 C1 caliber, preregistration :68/:112) -----------------
        // The freeze set is the TUNNEL-OUTSIDE resolver set: `interface_dns`
        // (the platform's tunnel interface resolver) and every `dns.servers[]`
        // entry that falls inside the overlay's own coverage are kept OUT of
        // it, and the exclusion is accounted for in `notes[]`. Everything else
        // must become a strict IPv4 `SocketAddr` freeze (`host:port`, the host
        // is the /32 the exclusion installs); an entry that cannot — no port,
        // IPv6, empty, not a dotted quad — is a RECORDED dns gap, never a
        // silent skip.
        let ranges = overlay_ranges(fields);
        let mut overlay_internal_excluded: usize = 0;
        let mut unusable_servers: usize = 0;
        if let Some(dns) = obj_get(fields, "dns").and_then(as_obj) {
            for server in obj_get(dns, "servers").map(as_arr).unwrap_or(&[]) {
                let Some(entry) = as_obj(server) else {
                    unusable_servers += 1;
                    facts.notes.push(
                        "dns-server-unusable=ip=<not-an-object>|port=<absent>|reason=malformed-entry"
                            .to_string(),
                    );
                    continue;
                };
                let ip = obj_get(entry, "ip").and_then(as_str).unwrap_or("");
                let port = obj_get(entry, "port").and_then(as_u64);
                if let Some(octets) = crate::config::parse_ipv4(ip.trim()) {
                    if ranges.iter().any(|r| r.contains(octets)) {
                        // overlay-internal resolver: never frozen, never
                        // excluded (a host route for it would kick the
                        // tunnel's own DNS out of the tunnel)
                        overlay_internal_excluded += 1;
                        continue;
                    }
                }
                let frozen = port
                    .and_then(|p| u16::try_from(p).ok())
                    .filter(|p| *p != 0)
                    .and_then(|p| injected_from_addr(&format!("{ip}:{p}")));
                match frozen {
                    Some(ep) => facts.dns_servers.push(ep),
                    None => {
                        unusable_servers += 1;
                        facts.notes.push(format!(
                            "dns-server-unusable=ip={}|port={}|reason={}",
                            if ip.is_empty() { "<empty>" } else { ip },
                            port.map(|p| p.to_string())
                                .unwrap_or_else(|| "<absent>".to_string()),
                            dns_entry_unusable_reason(ip, port)
                        ));
                    }
                }
            }
        }
        facts.notes.push(format!(
            "dns-overlay-internal-excluded-from-freeze={overlay_internal_excluded}"
        ));
        if let Some(iface_dns) = obj_get(fields, "interface_dns").and_then(as_str) {
            if !iface_dns.trim().is_empty() {
                // the tunnel interface's own resolver: kept out of the freeze
                // set, accounted for (the v3.1 rule names it explicitly; the
                // count above is the `dns.servers[]` half of the account)
                facts.notes.push("dns-interface-dns-excluded-from-freeze=1".to_string());
            }
        }
        if unusable_servers > 0 {
            facts.notes.push(format!("dns-freeze-gap=unusable-servers={unusable_servers}"));
        }
        if let Some(addr) = obj_get(fields, "address").and_then(as_str) {
            facts.own_overlay = parse_addr_literal(&format!("{addr}:0")).map(|(ip, _)| ip);
        }
        for peer in obj_get(fields, "peers").map(as_arr).unwrap_or(&[]) {
            let Some(entry) = as_obj(peer) else { continue };
            for vpn in obj_get(entry, "vpn_addresses").map(as_arr).unwrap_or(&[]) {
                if let Some(a) = as_str(vpn).and_then(|s| parse_addr_literal(&format!("{s}:0"))) {
                    facts.peer_overlay.push(a.0);
                }
            }
        }
    }
    apply_core_freeze_inputs(&mut facts, core);
    Ok(facts)
}

/// Merge the connector's INTERNAL session knowledge (design §4.0) into the
/// facts read from the public documents — the §7-I5 freeze-source extension.
///
/// - **stun**: `netbird_config.stuns[]` as the ICE orchestrator parsed it
///   ([`crate::connector::CoreFreezeInputs::stuns`]) — the set core gathers
///   against, and one of the seven kinds the judge requires. `turn:` URIs are
///   rejected by that parser (`peer_conn.rs` `set_stuns`) and have no internal
///   store, so the optional `turn` kind keeps its generic reasoned absence.
/// - **management**: the shell-resolved address every protected dial connects
///   to ([`crate::connector::CoreFreezeInputs::management_dial`]).
///
/// A value that is unusable (empty host / port 0 / IPv6 — this build is
/// IPv4-only, §10-4) is DROPPED here: `assemble_freezes` then records the
/// kind's reasoned absence. Nothing is ever invented for a missing source.
pub fn apply_core_freeze_inputs(
    facts: &mut HostFreezeFacts,
    core: &crate::connector::CoreFreezeInputs,
) {
    for stun in &core.stuns {
        if stun.host.trim().is_empty() || stun.port == 0 {
            continue;
        }
        facts.stun_uris.push(InjectedEndpoint::new(&stun.host, stun.port));
    }
    if let Some(SocketAddr::V4(v4)) = core.management_dial {
        let resolved = v4.to_string();
        facts.management_dial = Some(InjectedEndpoint::with_resolved(
            &v4.ip().to_string(),
            v4.port(),
            &resolved,
        ));
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

    /// N2-H I2 (design §13.13): the shell now feeds its stun class as
    /// `shell_exclusions[kind=stun]` (the arm source). This pins the CORE half
    /// of that cross-boundary link: the entry freezes with the shell token and
    /// the shell-resolved `ip:port`, keeps the slot against an identical
    /// core-network-map candidate, and a DIFFERENT stun endpoint from the core
    /// source still freezes alongside it (no kind is dropped by the merge).
    #[test]
    fn freeze_assembly_takes_the_shell_stun_exclusion_and_keeps_core_stuns_too() {
        let sources = FreezeSources {
            shell_exclusions: vec![ShellExclusion::with_resolved(
                EndpointKind::Stun,
                "stun.shell.example",
                3478,
                "198.51.100.7:3478",
            )],
            // the same resolved endpoint as the shell entry (must not duplicate)
            // plus a different one (must still freeze from the core source)
            stun_uris: vec![
                InjectedEndpoint::new("198.51.100.7", 3478),
                InjectedEndpoint::new("stun.core.example", 19302),
            ],
            ..FreezeSources::default()
        };
        let frozen = assemble_freezes(&sources);
        let stuns: Vec<_> =
            frozen.iter().filter(|f| f.kind == Some(EndpointKind::Stun)).collect();
        assert_eq!(stuns.len(), 2, "shell + distinct core stun endpoints: {stuns:?}");
        assert_eq!(stuns[0].host, "stun.shell.example");
        assert_eq!(stuns[0].source, SOURCE_SHELL_EXCLUSION, "the C2 cross-check token");
        assert_eq!(stuns[0].resolved, "198.51.100.7:3478");
        assert_eq!(stuns[0].proto, "udp", "transport follows the kind");
        assert_eq!(stuns[0].present, true);
        assert_eq!(stuns[0].absent_reason, "");
        assert_eq!(stuns[1].host, "stun.core.example");
        assert_eq!(stuns[1].source, SOURCE_CORE_NETWORK_MAP);
        assert_eq!(stuns[1].resolved, "stun.core.example:19302");
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

    // ---- I5 ----

    /// A frozen clock: the state machine's timestamps are injected (design
    /// U9), so no unit test depends on wall time.
    struct FixedClock {
        mono: u64,
        unix: i64,
    }

    impl CollectorClock for FixedClock {
        fn mono_ms(&self) -> u64 {
            self.mono
        }
        fn unix_s(&self) -> i64 {
            self.unix
        }
    }

    fn fixed_clock(mono: u64) -> Arc<dyn CollectorClock> {
        Arc::new(FixedClock { mono, unix: 1_756_000_000 })
    }

    /// One honest IPv4/UDP frame (`parse_ipv4_frame` reads it back).
    fn udp_frame(src: [u8; 4], dst: [u8; 4], sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
        let mut p = Vec::with_capacity(28 + payload.len());
        p.extend_from_slice(&[0x45, 0x00]);
        p.extend_from_slice(&((20 + 8 + payload.len()) as u16).to_be_bytes());
        p.extend_from_slice(&[0, 1, 0, 0, 64, 17, 0, 0]);
        p.extend_from_slice(&src);
        p.extend_from_slice(&dst);
        p.extend_from_slice(&sport.to_be_bytes());
        p.extend_from_slice(&dport.to_be_bytes());
        p.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
        p.extend_from_slice(&[0, 0]);
        p.extend_from_slice(payload);
        p
    }

    #[test]
    fn arm_options_parse_the_documented_shape_and_refuse_malformed_input() {
        // §5.1's documented document
        let opts = ArmOptions::parse_json(
            "{\"shell_exclusions\":[{\"kind\":\"management\",\"host\":\"mgmt.example\",\
             \"port\":443,\"resolved\":\"203.0.113.7:443\"},{\"kind\":\"signal\",\
             \"host\":\"signal.example\",\"port\":10000}],\"posctl_port\":40002,\
             \"probe_wait_ms\":900,\"build_sha256_reported\":\"deadbeef\"}",
        )
        .expect("documented opts parse");
        assert_eq!(opts.posctl_port, 40_002);
        assert_eq!(opts.probe_wait_ms, 900);
        assert_eq!(opts.build_sha256_reported, "deadbeef");
        assert_eq!(opts.shell_exclusions.len(), 2);
        assert_eq!(opts.shell_exclusions[0].kind, EndpointKind::Management);
        assert_eq!(opts.shell_exclusions[0].resolved, "203.0.113.7:443");
        assert_eq!(opts.shell_exclusions[1].resolved, "", "resolved is optional");
        // the shell set feeds the freeze with the D7 token
        let frozen = assemble_freezes(&FreezeSources {
            shell_exclusions: opts.shell_exclusions.clone(),
            ..FreezeSources::default()
        });
        let mgmt = frozen.iter().find(|f| f.kind == Some(EndpointKind::Management)).unwrap();
        assert!(mgmt.present);
        assert_eq!(mgmt.source, SOURCE_SHELL_EXCLUSION);
        assert_eq!(mgmt.resolved, "203.0.113.7:443");

        // empty = all defaults; malformed = refused (never half-understood)
        let defaults = ArmOptions::parse_json("").expect("empty opts");
        assert_eq!(defaults.posctl_port, DEFAULT_POSCTL_PORT);
        assert_eq!(defaults.probe_wait_ms, PROBE_WAIT_MS_DEFAULT);
        assert!(defaults.shell_exclusions.is_empty());
        for bad in [
            "not json",
            "[]",
            "{\"posctl_port\":0}",
            "{\"posctl_port\":70000}",
            "{\"probe_wait_ms\":0}",
            "{\"shell_exclusions\":{}}",
            "{\"shell_exclusions\":[{\"kind\":\"nope\",\"host\":\"h\",\"port\":1}]}",
            "{\"shell_exclusions\":[{\"kind\":\"relay\",\"host\":\"\",\"port\":1}]}",
            "{\"shell_exclusions\":[{\"kind\":\"relay\",\"host\":\"h\",\"port\":0}]}",
            "{\"shell_exclusions\":[{\"kind\":\"relay\",\"host\":\"h\"}]}",
        ] {
            assert!(ArmOptions::parse_json(bad).is_err(), "'{bad}' must be refused");
        }
        println!(
            "[I5] arm opts: posctl_port={} probe_wait_ms={} shell_exclusions={}",
            opts.posctl_port,
            opts.probe_wait_ms,
            opts.shell_exclusions.len()
        );
    }

    /// N2-H I3a (v3.1 §2.1 R-2①): the shell's ACCEPTED generation rides the
    /// arm options as a non-negative integer. An absent key stays `None` — the
    /// document then carries a value-less field instead of a generation core
    /// invented — and a wrongly-typed value refuses the arm (never a
    /// half-understood generation).
    #[test]
    fn arm_options_carry_the_shell_reported_exclusion_generation() {
        let opts = ArmOptions::parse_json("{\"exclusion_generation\":0}")
            .expect("generation 0 parses");
        assert_eq!(opts.exclusion_generation, Some(0), "0 is a real generation, not 'absent'");
        let opts = ArmOptions::parse_json("{\"probe_wait_ms\":900,\"exclusion_generation\":7}")
            .expect("documented shape + generation parses");
        assert_eq!(opts.exclusion_generation, Some(7));
        assert_eq!(opts.probe_wait_ms, 900, "the neighbouring options are untouched");

        let defaults = ArmOptions::parse_json("").expect("empty opts");
        assert_eq!(defaults.exclusion_generation, None);
        let without = ArmOptions::parse_json("{\"posctl_port\":40002}").expect("opts without key");
        assert_eq!(without.exclusion_generation, None, "absent key is not a generation");

        for bad in [
            "{\"exclusion_generation\":\"3\"}",
            "{\"exclusion_generation\":true}",
            "{\"exclusion_generation\":null}",
            "{\"exclusion_generation\":[3]}",
            "{\"exclusion_generation\":{}}",
            "{\"exclusion_generation\":-1}",
        ] {
            assert!(ArmOptions::parse_json(bad).is_err(), "'{bad}' must be refused");
        }
        println!("[I3a] arm opts exclusion_generation: parsed verbatim, absent stays None, malformed refused");
    }

    #[test]
    fn collector_tee_scans_both_directions_and_keeps_hits_per_direction() {
        let tee = CollectorTee::new(fixed_clock(77));
        let outer_id = "N2H-management-00112233445566778899aabb";
        let posctl_id = "N2H-posctl-ffeeddccbbaa998877665544";
        tee.register(outer_id, WireMarker::Ascii(outer_id.into()), false);
        tee.register(posctl_id, WireMarker::Ascii(posctl_id.into()), true);

        // a benign frame in each direction: counted + scanned, no hit
        let benign = udp_frame([10, 77, 0, 1], [10, 77, 0, 2], 1111, 2222, b"benign");
        tee.on_tun_frame(TunFrameDir::FromOs, &benign);
        tee.on_tun_frame(TunFrameDir::ToOs, &benign);

        // the outer probe's payload appears in the tunnel: a violation
        let leak = udp_frame([10, 77, 0, 1], [10, 77, 0, 2], 40000, 40001, outer_id.as_bytes());
        tee.on_tun_frame(TunFrameDir::FromOs, &leak);
        // the positive control is seen entering AND returning
        let entrance =
            udp_frame([10, 77, 0, 1], [10, 77, 0, 2], 40000, 40001, posctl_id.as_bytes());
        tee.on_tun_frame(TunFrameDir::FromOs, &entrance);
        tee.on_tun_frame(TunFrameDir::ToOs, &entrance);

        let snap = tee.snapshot();
        assert_eq!(snap.frames_scanned, 5, "both directions are scanned");
        assert_eq!(snap.totals.from_os_frames, 3);
        assert_eq!(snap.totals.to_os_frames, 2);
        assert_eq!(snap.totals.from_os_bytes, (benign.len() + leak.len() + entrance.len()) as u64);
        assert_eq!(snap.totals.to_os_bytes, (benign.len() + entrance.len()) as u64);
        // D3: ONE interface name for both directions
        assert_eq!(snap.sides, vec![TUN_IFACE_NAME.to_string()]);
        // the violation list is the outer probe only
        assert_eq!(snap.outer_hits.len(), 1);
        assert_eq!(snap.outer_hits[0].probe_id, outer_id);
        assert_eq!(snap.outer_hits[0].side, TUN_IFACE_NAME);
        assert_eq!(snap.outer_hits[0].frame_len, leak.len());
        assert_eq!(snap.outer_hits[0].observed_at_mono, 77);
        // D5: the positive control's two observation points stay distinct
        let entrance_hit = tee.directed_hit(posctl_id, TunFrameDir::FromOs).expect("entrance");
        let echo_hit = tee.directed_hit(posctl_id, TunFrameDir::ToOs).expect("echo");
        assert_eq!(entrance_hit.dport, 40001);
        assert_eq!(echo_hit.dport, 40001);
        assert!(tee.directed_hit(outer_id, TunFrameDir::ToOs).is_none(), "no return of the leak");
        assert_eq!(tee.outer_hit_count(), 1);
        assert_eq!(tee.frames_scanned(), 5);
        assert_eq!(direction_token(TunFrameDir::FromOs), "from-os");
        assert_eq!(direction_token(TunFrameDir::ToOs), "to-os");
        println!(
            "[I5] tee: frames={} from-os={}/{}B to-os={}/{}B hits={} sides={:?}",
            snap.frames_scanned,
            snap.totals.from_os_frames,
            snap.totals.from_os_bytes,
            snap.totals.to_os_frames,
            snap.totals.to_os_bytes,
            snap.outer_hits.len(),
            snap.sides
        );
    }

    /// A complete, clean window's material: every kind frozen (turn absent
    /// WITH reason), a sent probe + an observed receipt per present kind, both
    /// positive-control directions observed, frames scanned and a
    /// self-consistent counters pairing.
    fn clean_material() -> WindowMaterial {
        let freeze = assemble_freezes(&all_seven_sources());
        let probes: Vec<OuterProbe> = freeze
            .iter()
            .filter(|f| f.present)
            .map(|f| {
                let kind = f.kind.expect("kind");
                let id = id_of(kind, 0x40);
                OuterProbe {
                    probe_id: id.clone(),
                    kind: Some(kind),
                    tuple: FiveTuple {
                        proto: kind.proto().into(),
                        src: "0.0.0.0:5000".into(),
                        dst: f.resolved.clone(),
                    },
                    expected_path: "outer",
                    sent: true,
                    sent_at_unix: 1_756_000_000,
                    sent_at_mono: 1,
                    marker: Some(WireMarker::Ascii(id)),
                    payload_len: 30,
                }
            })
            .collect();
        let receipts: Vec<EndpointReceipt> = probes
            .iter()
            .map(|p| EndpointReceipt {
                probe_id: p.probe_id.clone(),
                kind: p.kind.map(|k| k.name()).unwrap_or("unknown").into(),
                observed: true,
                physical_src: "203.0.113.7:443".into(),
                physical_src_source: "endpoint-reported".into(),
                at_unix: 1_756_000_000,
                method: "sink-record".into(),
                detail: "harness sink".into(),
            })
            .collect();
        let hit = |id: &str, dir: TunFrameDir| DirectedHit {
            probe_id: id.to_string(),
            direction: dir,
            hit: TunHit {
                probe_id: id.to_string(),
                side: TUN_IFACE_NAME.to_string(),
                src: [10, 77, 0, 1],
                dst: [10, 77, 0, 2],
                sport: 40000,
                dport: 40001,
                frame_len: 64,
                observed_at_mono: 5,
            },
        };
        let posctl = vec![
            PosctlProbe {
                probe_id: "N2H-posctl-a2b".into(),
                direction: "a2b".into(),
                tuple: FiveTuple {
                    proto: "ipv4".into(),
                    src: "0.0.0.0:40000".into(),
                    dst: "10.77.0.2:40001".into(),
                },
                expected_path: "tunnel",
                sent: true,
                sent_at_mono: 4,
                observed: Some(hit("N2H-posctl-a2b", TunFrameDir::FromOs).hit),
                note: "closed end to end".into(),
            },
            PosctlProbe {
                probe_id: "N2H-posctl-b2a".into(),
                direction: "b2a".into(),
                tuple: FiveTuple {
                    proto: "ipv4".into(),
                    src: "10.77.0.2:40001".into(),
                    dst: "10.77.0.1:40001".into(),
                },
                expected_path: "tunnel",
                sent: true,
                sent_at_mono: 4,
                observed: Some(hit("N2H-posctl-b2a", TunFrameDir::ToOs).hit),
                note: "peer-initiated".into(),
            },
        ];
        let before = IfaceSnapshot {
            at_mono: 1,
            at_unix: 1_756_000_000,
            wg_rx_packets: 10,
            wg_rx_bytes_to_tun: 1000,
            tun_delivered_frames: 10,
            tun_delivered_bytes: 1000,
            ..IfaceSnapshot::default()
        };
        let after = IfaceSnapshot {
            at_mono: 9,
            at_unix: 1_756_000_009,
            wg_rx_packets: 14,
            wg_rx_bytes_to_tun: 1400,
            tun_delivered_frames: 14,
            tun_delivered_bytes: 1400,
            ..IfaceSnapshot::default()
        };
        let counters = WindowCounters { side: TUN_IFACE_NAME.to_string(), before, after };
        let identities = counter_identities(&counters.delta());
        assert!(identities.hold(), "the clean fixture must be self-consistent");
        WindowMaterial {
            freeze,
            probes,
            posctl,
            receipts,
            scan: TeeSnapshot {
                totals: TunTeeTotals {
                    from_os_frames: 3,
                    from_os_bytes: 300,
                    to_os_frames: 4,
                    to_os_bytes: 400,
                },
                frames_scanned: 7,
                sides: vec![TUN_IFACE_NAME.to_string()],
                outer_hits: Vec::new(),
                directed: Vec::new(),
            },
            sample: Some(WindowSample {
                counters,
                identities,
                resamples: 0,
                source: COUNTERS_SOURCE_TEE_FALLBACK,
            }),
            notes: Vec::new(),
            opened_mono: 1,
            closed_mono: 9,
            generated_unix: 1_756_000_009,
        }
    }

    #[test]
    fn clean_device_document_parses_with_the_strict_reader_and_device_rules() {
        let evidence = assemble_device_evidence(clean_material());
        assert_eq!(evidence.verdict, crate::n2h::VERDICT_PASS, "{:?}", evidence.reasons);
        assert!(evidence.reasons.is_empty(), "{:?}", evidence.reasons);
        assert_eq!(evidence.mode, MODE_DEVICE);
        assert_eq!(evidence.session, SESSION_DEVICE);
        assert_eq!(evidence.counters.len(), 1, "D8: one platform TUN");
        assert_eq!(evidence.counters[0].side, TUN_IFACE_NAME);
        assert!(evidence.counters[0].reconciled());
        assert_eq!(evidence.tun_negative.sides, vec![TUN_IFACE_NAME.to_string()]);
        assert!(!evidence.tun_negative.hit);
        // the source caliber is DECLARED, never implied (§3.2)
        assert!(
            evidence.notes.iter().any(|n| n == "counters-source=tee-fallback"),
            "{:?}",
            evidence.notes
        );
        let json = evidence.to_json();
        // §8-1: the document parses under the crate's OWN strict reader
        crate::config::parse_document(&json).expect("the device document is strict JSON");
        assert!(json.contains("\"mode\":\"device\""));
        assert!(json.contains("\"scanned_sides\":[\"vpn-tun\"]"));
        assert!(json.contains("\"unsat_note\":"));
        for forbidden in ["protect pass", "N2 pass", "等同逐 socket protect", "waived", "N/A"] {
            assert!(!json.contains(forbidden), "forbidden token '{forbidden}' in evidence");
        }
        println!(
            "[I5] clean device document: {} bytes, verdict={}, counters side={}, frames_scanned={}",
            json.len(),
            evidence.verdict,
            evidence.counters[0].side,
            evidence.tun_negative.frames_scanned
        );
    }

    #[test]
    fn the_packet_identity_and_a_zero_frame_scan_force_inconclusive() {
        // 1. design §4.4's SECOND identity: the judge's step 6 only reads
        //    WindowCounters::reconciled() (bytes). A pairing the byte identity
        //    ACCEPTS but the packet identity refuses must NOT pass — the
        //    collector appends the reason and downgrades the verdict itself.
        let mut m = clean_material();
        {
            let sample = m.sample.as_mut().expect("sample");
            sample.counters.after.wg_rx_packets += 1; // wg saw one extra frame
            sample.counters.after.tun_delivered_frames -= 1;
            sample.identities = counter_identities(&sample.counters.delta());
            sample.resamples = MAX_PAIRING_RESAMPLES;
            assert!(sample.identities.bytes, "the frozen byte identity still holds");
            assert!(!sample.identities.hold(), "…and the packet identity does not");
            assert!(
                sample.counters.reconciled(),
                "WindowCounters::reconciled is bytes-only — the judge cannot see this break"
            );
        }
        let evidence = assemble_device_evidence(m);
        assert_eq!(evidence.verdict, crate::n2h::VERDICT_INCONCLUSIVE);
        assert!(
            evidence.reasons.iter().any(|r| r.contains("design §4.4 second identity")),
            "{:?}",
            evidence.reasons
        );
        assert!(
            evidence.notes.iter().any(|n| n == "counters-identity=packets-broken"),
            "{:?}",
            evidence.notes
        );

        // 2. §6.3: a window that scanned ZERO frames proves nothing
        let mut m = clean_material();
        m.scan.frames_scanned = 0;
        m.scan.totals = TunTeeTotals::default();
        m.sample = None;
        let evidence = assemble_device_evidence(m);
        assert_eq!(evidence.verdict, crate::n2h::VERDICT_INCONCLUSIVE);
        assert!(
            evidence.reasons.iter().any(|r| r.contains("zero frames scanned")),
            "{:?}",
            evidence.reasons
        );

        // 3. falsification is never downgraded: an outer hit stays n2h-fail
        //    even with the packet identity broken in the same window
        let mut m = clean_material();
        let leaked = m.probes[0].probe_id.clone();
        {
            let sample = m.sample.as_mut().expect("sample");
            sample.counters.after.wg_rx_packets += 1;
            sample.identities = counter_identities(&sample.counters.delta());
        }
        m.scan.outer_hits.push(TunHit {
            probe_id: leaked.clone(),
            side: TUN_IFACE_NAME.to_string(),
            src: [10, 77, 0, 1],
            dst: [10, 77, 0, 2],
            sport: 40000,
            dport: 40001,
            frame_len: 64,
            observed_at_mono: 5,
        });
        m.scan.directed.push(DirectedHit {
            probe_id: leaked.clone(),
            direction: TunFrameDir::FromOs,
            hit: m.scan.outer_hits[0].clone(),
        });
        let evidence = assemble_device_evidence(m);
        assert_eq!(evidence.verdict, crate::n2h::VERDICT_FAIL, "{:?}", evidence.reasons);
        assert!(evidence.tun_negative.hit);
        assert!(
            evidence.reasons.iter().any(|r| r.contains(&leaked)),
            "the fail reason names the leaked probe: {:?}",
            evidence.reasons
        );
        assert!(
            evidence.notes.iter().any(|n| n
                == &format!(
                    "outer-hit-direction probe_id={leaked} direction=from-os side=vpn-tun frame_len=64"
                )),
            "D4's direction fact is recorded in notes: {:?}",
            evidence.notes
        );
        println!(
            "[I5] forced rules: packet-identity break → {}, zero frames → {}, leak → {}",
            crate::n2h::VERDICT_INCONCLUSIVE,
            crate::n2h::VERDICT_INCONCLUSIVE,
            crate::n2h::VERDICT_FAIL
        );
    }

    #[test]
    fn process_freeze_facts_map_both_connector_documents() {
        let status = "{\"running\":true,\"state\":\"connected\",\"wg\":{\"device_up\":true,\
            \"ready\":true,\"wg_peers\":[{\"pub_key_b64\":\"k\",\"endpoint\":\"198.51.100.4:51820\",\
            \"allowed_ips\":[\"10.77.0.2/32\"]},{\"pub_key_b64\":\"j\",\"endpoint\":null}]},\
            \"relay_advertised\":{\"advertised\":true,\"advertised_urls\":[\"rels://home.example:28443\",\
            \"garbage\"]},\"relay\":{\"enabled\":true,\"urls\":[\"rel://relay.example:8080\"]}}";
        let network = "{\"available\":true,\"serial\":7,\"address\":\"10.77.0.1\",\
            \"address_prefix_len\":32,\"interface_dns\":\"10.0.0.53\",\"signal\":\"signal.example:10000\",\
            \"routes\":[],\"dns\":{\"service_enable\":true,\"servers\":[{\"ip\":\"10.0.0.54\",\"port\":53}]},\
            \"peer_count\":1,\"peers\":[{\"pub_key\":\"k\",\"allowed_ips\":1,\
            \"vpn_addresses\":[\"10.77.0.2\"]}],\"default_route\":{\"allowed\":false,\"reason\":\"held\"}}";
        let facts = process_freeze_facts(status, network).expect("both documents parse");
        assert_eq!(facts.wg_peer_endpoints.len(), 1, "a null endpoint is not a freeze");
        assert_eq!(facts.wg_peer_endpoints[0].resolved, "198.51.100.4:51820");
        // advertised urls first, then the runtime urls, unparsable ones dropped
        let relays: Vec<String> = facts
            .relay_advertised
            .iter()
            .map(|e| format!("{}:{}", e.host, e.port))
            .collect();
        assert_eq!(relays, vec!["home.example:28443", "relay.example:8080"]);
        assert_eq!(facts.signal_config.as_ref().expect("signal").port, 10_000);
        // v3.1 C1 dns caliber: `interface_dns` is NOT a freeze target any more
        // (it is the tunnel interface's own resolver) — it is accounted for in
        // notes, and the server outside the overlay's coverage is frozen
        assert_eq!(facts.dns_servers.len(), 1, "dns.servers[] only (interface_dns is out)");
        assert_eq!(facts.dns_servers[0].resolved, "10.0.0.54:53");
        assert!(
            facts.notes.contains(&"dns-overlay-internal-excluded-from-freeze=0".to_string()),
            "the exclusion count is always recorded: {:?}",
            facts.notes
        );
        assert!(
            facts.notes.contains(&"dns-interface-dns-excluded-from-freeze=1".to_string()),
            "the interface_dns exclusion is recorded: {:?}",
            facts.notes
        );
        assert!(
            !facts.notes.iter().any(|n| n.starts_with("dns-freeze-gap=")),
            "no unusable entry in this document: {:?}",
            facts.notes
        );
        assert_eq!(facts.own_overlay, Some([10, 77, 0, 1]));
        assert_eq!(facts.peer_overlay, vec![[10, 77, 0, 2]]);
        // the assembly turns these into the D7 freeze + the a2b destination
        let frozen = assemble_freezes(&FreezeSources {
            signal_config: facts.signal_config.clone(),
            relay_advertised: facts.relay_advertised.clone(),
            wg_peer_endpoints: facts.wg_peer_endpoints.clone(),
            dns_servers: facts.dns_servers.clone(),
            ..FreezeSources::default()
        });
        let signal = frozen.iter().find(|f| f.kind == Some(EndpointKind::Signal)).unwrap();
        assert!(signal.present);
        assert_eq!(signal.source, SOURCE_CORE_SIGNAL_CONFIG);
        // malformed documents are refused, never guessed
        assert!(process_freeze_facts("{", network).is_err());
        assert!(process_freeze_facts(status, "[]").is_err());
        println!(
            "[I5] process freeze facts: wg_peers={} relays={:?} dns={} own={:?} peer={:?}",
            facts.wg_peer_endpoints.len(),
            relays,
            facts.dns_servers.len(),
            facts.own_overlay,
            facts.peer_overlay
        );
    }

    /// v3.1's dns freeze caliber (preregistration :68/:112), the whole rule in
    /// one document: `interface_dns` and every `dns.servers[]` entry inside the
    /// overlay's own coverage stay OUT of the freeze set (they are the
    /// tunnel's own resolvers — excluding them would push DNS out of the
    /// tunnel), everything else freezes as a strict IPv4 `host:port`, and any
    /// entry that cannot (IPv6 / no port / empty / not a dotted quad /
    /// malformed element) is a RECORDED dns gap, never a silent skip.
    #[test]
    fn dns_freeze_caliber_v31_filters_overlay_internal_and_records_unusable_entries() {
        let status = "{\"running\":true,\"state\":\"connected\"}";
        // coverage: the device's own /20 address AND two route networks
        let network = "{\"available\":true,\"serial\":7,\"address\":\"10.77.0.1\",\
            \"address_prefix_len\":20,\"interface_dns\":\"10.0.0.53\",\"signal\":\"signal.example:10000\",\
            \"routes\":[{\"network\":\"10.9.0.0/16\",\"is_default\":false},\
            {\"network\":\"172.16.0.0/12\",\"is_default\":false}],\
            \"dns\":{\"service_enable\":true,\"servers\":[\
            {\"ip\":\"10.77.0.53\",\"port\":53},\
            {\"ip\":\"10.9.1.53\",\"port\":53},\
            {\"ip\":\"172.16.5.53\",\"port\":53},\
            {\"ip\":\"198.51.100.53\",\"port\":53},\
            {\"ip\":\"203.0.113.53\",\"port\":53},\
            {\"ip\":\"2001:db8::53\",\"port\":53},\
            {\"ip\":\"198.51.100.54\"},\
            {\"ip\":\"\",\"port\":53},\
            {\"ip\":\"not-an-ip\",\"port\":53},\
            \"garbage\"]},\"peer_count\":0,\"peers\":[],\
            \"default_route\":{\"allowed\":false,\"reason\":\"held\"}}";
        let facts = process_freeze_facts(status, network).expect("both documents parse");

        let dns: Vec<&str> = facts.dns_servers.iter().map(|e| e.resolved.as_str()).collect();
        assert_eq!(
            dns,
            vec!["198.51.100.53:53", "203.0.113.53:53"],
            "only the resolvers OUTSIDE the overlay coverage are frozen"
        );
        for internal in ["10.77.0.53", "10.9.1.53", "172.16.5.53", "10.0.0.53"] {
            assert!(
                !dns.iter().any(|r| r.starts_with(&format!("{internal}:"))),
                "an overlay-internal resolver must never be frozen: {internal} in {dns:?}"
            );
        }
        assert!(
            facts.notes.contains(&"dns-overlay-internal-excluded-from-freeze=3".to_string()),
            "the count covers the in-coverage dns.servers[] entries: {:?}",
            facts.notes
        );
        assert!(
            facts.notes.contains(&"dns-interface-dns-excluded-from-freeze=1".to_string()),
            "interface_dns is accounted for too: {:?}",
            facts.notes
        );
        for reason in ["ipv6", "no-port", "empty-ip", "not-strict-ipv4-socketaddr"] {
            assert!(
                facts.notes.iter().any(|n| n.starts_with("dns-server-unusable=")
                    && n.ends_with(&format!("|reason={reason}"))),
                "an unusable entry with reason {reason} is recorded, never skipped: {:?}",
                facts.notes
            );
        }
        assert!(
            facts.notes.contains(
                &"dns-server-unusable=ip=<not-an-object>|port=<absent>|reason=malformed-entry"
                    .to_string()
            ),
            "a malformed element is recorded as well: {:?}",
            facts.notes
        );
        assert!(
            facts.notes.contains(&"dns-freeze-gap=unusable-servers=5".to_string()),
            "the unusable entries are counted as a dns required-class gap: {:?}",
            facts.notes
        );

        // the assembly turns exactly the filtered set into the dns freeze
        let frozen = assemble_freezes(&FreezeSources {
            dns_servers: facts.dns_servers.clone(),
            ..FreezeSources::default()
        });
        let entries: Vec<&FrozenEndpoint> =
            frozen.iter().filter(|f| f.kind == Some(EndpointKind::Dns)).collect();
        assert_eq!(entries.len(), 2);
        for entry in &entries {
            assert!(entry.present);
            assert_eq!(entry.source, SOURCE_CORE_NETWORK_MAP);
            assert_eq!(entry.proto, "udp");
        }

        // everything overlay-internal ⇒ a REASONED absence, never an invented
        // freeze (and never a reclassification of dns into an optional kind)
        let all_internal = "{\"available\":true,\"serial\":8,\"address\":\"10.77.0.1\",\
            \"address_prefix_len\":16,\"interface_dns\":\"10.0.0.53\",\
            \"routes\":[],\"dns\":{\"service_enable\":true,\"servers\":[{\"ip\":\"10.77.9.9\",\"port\":53}]},\
            \"peer_count\":0,\"peers\":[]}";
        let bare = process_freeze_facts(status, all_internal).expect("document parses");
        assert!(bare.dns_servers.is_empty());
        assert!(
            bare.notes.contains(&"dns-overlay-internal-excluded-from-freeze=1".to_string()),
            "{:?}",
            bare.notes
        );
        let bare_frozen = assemble_freezes(&FreezeSources {
            dns_servers: bare.dns_servers,
            ..FreezeSources::default()
        });
        let absent = bare_frozen
            .iter()
            .find(|f| f.kind == Some(EndpointKind::Dns))
            .expect("the dns kind is always represented");
        assert!(!absent.present, "no outer resolver ⇒ the dns kind is a reasoned absence");
        assert!(!absent.absent_reason.is_empty(), "{absent:?}");
        assert!(EndpointKind::Dns.required(), "dns must stay a REQUIRED kind (C1)");
        println!(
            "[v3.1 dns] frozen={dns:?} overlay-internal-excluded=3 unusable=5 notes={:?}",
            facts.notes
        );
    }

    /// The shell's v3.1 dns branch record (R-4): the ONE branch token is an
    /// allow-list, the two text fields are strictly shaped (a value carrying
    /// `|` or whitespace could forge a second `notes[]` field), and the notes
    /// render verbatim — a missing record stays missing (the judge's dns gap).
    #[test]
    fn arm_opts_carry_the_dns_branch_record_into_the_evidence_notes() {
        let good = "{\"dns\":{\"branch\":\"all-nets-filtered\",\
            \"physical_evidence\":\"handles=101,102;networkCap-has-NOT_VPN=15;\
            bearerTypes-no-BEARER_VPN=4\",\"gap_reason\":\"dnses-empty\"}}";
        let opts = ArmOptions::parse_json(good).expect("the branch record parses");
        let dns = opts.dns.as_ref().expect("dns record");
        assert_eq!(dns.branch, DNS_BRANCH_ALL_NETS_FILTERED);
        assert_eq!(
            dns.notes(),
            vec![
                "dns-branch=all-nets-filtered".to_string(),
                "dns-physical-evidence=handles=101,102;networkCap-has-NOT_VPN=15;\
                 bearerTypes-no-BEARER_VPN=4"
                    .to_string(),
                "dns-gap=dnses-empty".to_string(),
            ]
        );
        // the success shape: no gap_reason ⇒ no dns-gap line
        let clean = ArmOptions::parse_json(
            "{\"dns\":{\"branch\":\"default\",\
             \"physical_evidence\":\"handle=101;networkCap-has-NOT_VPN=15;\
             bearerTypes-no-BEARER_VPN=4\"}}",
        )
        .expect("the default branch record parses");
        let clean_notes = clean.dns.as_ref().expect("dns record").notes();
        assert_eq!(clean_notes.len(), 2);
        assert_eq!(clean_notes[0], "dns-branch=default");
        // absent section ⇒ NO dns notes at all (never a fabricated branch)
        assert!(ArmOptions::parse_json("{}").expect("no opts").dns.is_none());
        // malformed records refuse the arm (half-understood ⇒ no window)
        for bad in [
            "{\"dns\":[]}",
            "{\"dns\":{\"physical_evidence\":\"e\"}}",
            "{\"dns\":{\"branch\":\"best-effort\",\"physical_evidence\":\"e\"}}",
            "{\"dns\":{\"branch\":\"default\"}}",
            "{\"dns\":{\"branch\":\"default\",\"physical_evidence\":\"\"}}",
            "{\"dns\":{\"branch\":\"default\",\"physical_evidence\":\"handle=1|dns-gap=x\"}}",
            "{\"dns\":{\"branch\":\"default\",\"physical_evidence\":\"two words\"}}",
            "{\"dns\":{\"branch\":\"default\",\"physical_evidence\":\"e\",\"gap_reason\":7}}",
        ] {
            assert!(ArmOptions::parse_json(bad).is_err(), "must refuse {bad}");
        }
        println!(
            "[v3.1 dns] branch record: {:?} (missing record ⇒ no dns-branch line at all)",
            dns.notes()
        );
    }

    /// §7-I5's freeze-source extension: the connector's INTERNAL session
    /// knowledge (`stuns[]` parsed by the ICE orchestrator + the management
    /// dial address) closes the two gaps the public documents leave — the
    /// `stun` kind is REQUIRED by the judge (`n2h.rs:151-153`), so freezing it
    /// as an unreasoned absence would make every device round inconclusive.
    ///
    /// `process_freeze_facts_with_core` is exactly what
    /// [`WgFeedHost::freeze_facts`] calls; the connector side of the read
    /// (orchestrator + factory) is unit-tested in `crate::connector` and
    /// end-to-end in `tests/n2h_device_collector.rs`.
    #[test]
    fn core_freeze_inputs_close_the_stun_and_management_gap() {
        let status = "{\"running\":true,\"state\":\"connected\",\"wg\":{\"device_up\":true,\
            \"ready\":true,\"wg_peers\":[{\"pub_key_b64\":\"k\",\"endpoint\":\"198.51.100.4:51820\"}]},\
            \"relay_advertised\":{\"advertised\":true,\"advertised_urls\":[\"rels://home.example:28443\"]}}";
        let network = "{\"available\":true,\"serial\":7,\"address\":\"10.77.0.1\",\
            \"address_prefix_len\":32,\"interface_dns\":\"10.0.0.53\",\"signal\":\"signal.example:10000\",\
            \"routes\":[],\"dns\":{\"service_enable\":true,\"servers\":[{\"ip\":\"10.0.0.54\",\"port\":53}]},\
            \"peer_count\":1,\"peers\":[{\"pub_key\":\"k\",\"allowed_ips\":1,\
            \"vpn_addresses\":[\"10.77.0.2\"]}],\"default_route\":{\"allowed\":false,\"reason\":\"held\"}}";

        // the connector-internal inputs, built by the REAL parser the ICE
        // orchestrator uses (`set_stuns` → `ice::parse_stun_uri`)
        let core = crate::connector::CoreFreezeInputs {
            stuns: vec![
                crate::ice::parse_stun_uri("stun:stun.netbird.io:3478").expect("stun uri"),
                crate::ice::parse_stun_uri("stun:203.0.113.9").expect("default port 3478"),
            ],
            management_dial: Some("198.51.100.7:443".parse().expect("dial address")),
        };
        let facts = process_freeze_facts_with_core(status, network, &core)
            .expect("both documents parse");
        let frozen = assemble_freezes(&FreezeSources {
            management_dial: facts.management_dial.clone(),
            signal_config: facts.signal_config.clone(),
            relay_advertised: facts.relay_advertised.clone(),
            stun_uris: facts.stun_uris.clone(),
            wg_peer_endpoints: facts.wg_peer_endpoints.clone(),
            dns_servers: facts.dns_servers.clone(),
            frozen_at_unix: 1_756_000_000,
            frozen_at_mono: 42,
            ..FreezeSources::default()
        });
        for kind in EndpointKind::ALL {
            assert!(
                frozen.iter().any(|f| f.kind == Some(kind)),
                "{kind:?} must be represented (present or reasoned absence)"
            );
        }
        assert_eq!(
            frozen.len(),
            8,
            "7 kinds; stun×2 carries several endpoints and dns ONE (v3.1: interface_dns is not \
             a freeze target)"
        );

        // stun: no longer an absence — one entry per parsed URI, core-network-map
        let stuns: Vec<&FrozenEndpoint> =
            frozen.iter().filter(|f| f.kind == Some(EndpointKind::Stun)).collect();
        assert_eq!(stuns.len(), 2, "one freeze per stun URI: {stuns:?}");
        for entry in &stuns {
            assert!(entry.present, "stun must freeze from the internal source: {entry:?}");
            assert_eq!(entry.source, SOURCE_CORE_NETWORK_MAP);
            assert_eq!(entry.proto, "udp");
            assert!(entry.absent_reason.is_empty(), "a present entry carries no reason");
        }
        assert_eq!(stuns[0].resolved, "stun.netbird.io:3478");
        assert_eq!(stuns[1].resolved, "203.0.113.9:3478", "the URI default port");
        assert!(EndpointKind::Stun.required(), "the judge REQUIRES this kind (n2h.rs:151)");

        // management: the dial address, with its own source token
        let management = frozen
            .iter()
            .find(|f| f.kind == Some(EndpointKind::Management))
            .expect("management entry");
        assert!(management.present && management.source == SOURCE_CORE_DIAL_ADDRESS);
        assert_eq!(management.resolved, "198.51.100.7:443");

        // fail-closed without the internal inputs: both kinds are REASONED
        // absences (no connector ⇒ nothing is invented)
        let bare = process_freeze_facts_with_core(
            status,
            network,
            &crate::connector::CoreFreezeInputs::default(),
        )
        .expect("both documents parse");
        let bare_frozen = assemble_freezes(&FreezeSources {
            management_dial: bare.management_dial.clone(),
            stun_uris: bare.stun_uris.clone(),
            frozen_at_unix: 1_756_000_000,
            frozen_at_mono: 42,
            ..FreezeSources::default()
        });
        for kind in [EndpointKind::Stun, EndpointKind::Management] {
            let entry = bare_frozen.iter().find(|f| f.kind == Some(kind)).expect("entry");
            assert!(!entry.present, "{kind:?} must not appear without a source");
            assert!(!entry.absent_reason.is_empty(), "{kind:?} absence must be reasoned");
        }
        // unusable internal values are dropped, never invented either
        let junk = crate::connector::CoreFreezeInputs {
            stuns: vec![crate::ice::StunServer { host: "  ".into(), port: 3478 }],
            management_dial: None,
        };
        let junk_facts = process_freeze_facts_with_core(status, network, &junk).expect("parses");
        assert!(junk_facts.stun_uris.is_empty(), "an unusable stun source is dropped");
        println!(
            "[I5-wire] stun freezes={:?} source={} management={} source={} bare-stun='{}'",
            stuns.iter().map(|e| e.resolved.clone()).collect::<Vec<_>>(),
            SOURCE_CORE_NETWORK_MAP,
            management.resolved,
            management.source,
            bare_frozen
                .iter()
                .find(|f| f.kind == Some(EndpointKind::Stun))
                .map(|f| f.absent_reason.clone())
                .unwrap_or_default()
        );
    }

    #[test]
    fn the_unarmed_napi_surface_is_inert_and_idempotent() {
        // §6.1: with no window at all the four entries answer the documented
        // not-armed shapes and touch nothing.
        uninstall_host();
        *window_slot() = None;
        assert_eq!(status_json(), "{\"ok\":false,\"error\":\"not-armed\"}");
        assert_eq!(evidence_json(-1), "{\"error\":\"no-evidence\"}");
        assert_eq!(evidence_json(0), "{\"error\":\"no-evidence\"}");
        assert_eq!(disarm_json(), "{\"ok\":true}");
        // no host seam installed ⇒ the arm is refused fail-closed (§6.1)
        assert_eq!(
            arm_json(""),
            "{\"ok\":false,\"error\":\"no-connector\"}",
            "an uninstalled host must refuse the arm"
        );
        assert_eq!(arm_json("{"), "{\"ok\":false,\"error\":\"invalid-opts\"}");
        assert!(installed_host().is_none());
        assert!(window_slot().is_none(), "a refused arm leaves no window behind");
        println!("[I5] unarmed surface: status={} evidence={}", status_json(), evidence_json(-1));
    }

    // ---- I6 ----

    /// The posctl wire form is ONE contract shared by the device sender (I3/I5)
    /// and the host peer endpoint (I6): the peer echoes exactly what
    /// [`posctl_marker`] accepts, so this test is the receipt test's guard
    /// against drift AND against the endpoint being used as a reflector.
    #[test]
    fn posctl_payload_and_marker_round_trip_on_the_frozen_wire_shape() {
        let id = probe_id(POSCTL_KIND_TOKEN, &"ab".repeat(12));
        let payload = posctl_payload(&id);
        assert_eq!(payload, format!("N2H-posctl-{}|positive-control", "ab".repeat(12)));
        assert_eq!(posctl_marker(payload.as_bytes()), Some(id.as_str()));
        // …and the device's OWN sender produces exactly that payload (the
        // single-source property: `send_posctl` calls `posctl_payload`).
        let mut sink = crate::n2h::UdpSink::bind(false).expect("udp sink");
        let dst = sink.local_addr().expect("sink addr");
        let (n, _src) = send_posctl(dst, &id).expect("posctl send");
        assert_eq!(n, payload.len());
        let mut recorded: Option<Vec<u8>> = None;
        for _ in 0..100 {
            sink.drain();
            if let Some(rec) = sink.observed(&WireMarker::Ascii(id.clone())) {
                recorded = Some(rec.payload.clone());
                break;
            }
            crate::sys::sleep_ms(10);
        }
        assert_eq!(recorded.as_deref(), Some(payload.as_bytes()), "the sink recorded the probe");

        let hex24 = "ab".repeat(12);
        for bad in [
            String::new(),
            "N2H-posctl-|positive-control".to_string(),
            format!("N2H-posctl-{}|positive-control", "ab".repeat(11)), // 22 hex
            format!("N2H-posctl-{}|positive-control", "ab".repeat(13)), // 26 hex
            format!("N2H-posctl-{}|positive-control", "zz".repeat(12)), // not hex
            format!("N2H-posctl-{hex24}"),                              // no suffix
            format!("N2H-wg_peer-{hex24}|positive-control"),            // wrong kind
            format!("N2H-posctl-{hex24}|positive-control!"),            // trailing byte
            format!("N2H-posctl-{hex24}|positive-control|positive-control"),
            format!("N2H-posctl-{hex24}|positive-control extra"),
            "nbinterop-probe".to_string(),
        ] {
            assert_eq!(posctl_marker(bad.as_bytes()), None, "must not echo {bad:?}");
        }
        assert_eq!(posctl_marker(&[0xff, 0xfe]), None, "non-UTF8 is not a probe");
        println!("[I6] posctl wire form: payload={payload} marker={}", posctl_marker(payload.as_bytes()).unwrap_or("?"));
    }

    /// §13.10's ops relay contract: the arm option accepts ONLY the frozen
    /// posctl marker shape (the id form the I6 announcement line prints), and
    /// anything else refuses the whole arm. External input therefore never
    /// widens the tee's marker set, and the raw value is never echoed into a
    /// document (only the sanitized hilog detail carries the reason).
    #[test]
    fn arm_opts_accept_only_a_frozen_shape_peer_announcement() {
        let hex24 = "cd".repeat(12);
        let id = probe_id(POSCTL_KIND_TOKEN, &hex24);

        // the announced id form round-trips through the SAME validator the
        // peer's datagram must pass (`posctl_marker`)
        assert_eq!(announced_b2a_marker(&id), Ok(id.clone()));
        let opts = ArmOptions::parse_json(&format!(
            "{{\"posctl_port\":40001,\"peer_b2a_probe_id\":\"{id}\"}}"
        ))
        .expect("the announced marker is accepted");
        assert_eq!(opts.peer_b2a_probe_id.as_deref(), Some(id.as_str()));

        // an ABSENT key is no announcement at all (the reasoned-gap semantics)
        assert_eq!(ArmOptions::parse_json("{}").expect("no options").peer_b2a_probe_id, None);

        // present but not the frozen shape ⇒ the arm is REFUSED (never
        // silently ignored, never trimmed into shape)
        let payload_form = posctl_payload(&id);
        for bad in [
            payload_form.as_str(),                        // payload, not the id
            "N2H-posctl-cdcdcdcdcdcdcdcdcdcdcd",          // 22 hex
            "N2H-posctl-zzzzzzzzzzzzzzzzzzzzzzzz",        // not hex
            "N2H-wg_peer-cdcdcdcdcdcdcdcdcdcdcdcd",       // wrong kind, right nonce
            "nbinterop-probe",                            // unrelated marker
            "",                                           // present but empty
        ] {
            assert!(announced_b2a_marker(bad).is_err(), "must refuse {bad:?}");
            let json = format!("{{{}}}", jstr("peer_b2a_probe_id", bad));
            assert!(
                ArmOptions::parse_json(&json).is_err(),
                "a present-but-invalid announcement must refuse the arm: {json}"
            );
        }
        // trailing byte / trailing space: the validator is exact
        assert!(announced_b2a_marker(&format!("{id} ")).is_err());
        assert!(announced_b2a_marker(&format!("{id}!")).is_err());
        // a JSON-injection shaped value stays a plain (refused) string: it can
        // never smuggle another option field into the arm document
        let injected = format!("{hex24}\",\"posctl_port\":0");
        assert!(announced_b2a_marker(&injected).is_err());
        assert!(ArmOptions::parse_json(&format!(
            "{{{}}}",
            jstr("peer_b2a_probe_id", &injected)
        ))
        .is_err());
        // a non-string value is refused by the parser, before any validation
        assert!(ArmOptions::parse_json("{\"peer_b2a_probe_id\":40001}").is_err());
        println!(
            "[13.10] arm announcement: accepted={id} (frozen shape); payload/22-hex/non-hex/\
             wrong-kind/empty/trailing-byte/injected shapes refused; absent key ⇒ no announcement"
        );
    }

    /// The defaulted [`CollectorHost::wg_stats`]: an injected host that does not
    /// expose counters answers `None` (the I6 status line then prints `na`)
    /// instead of inventing a value.
    #[test]
    fn the_default_host_exposes_no_wg_counters() {
        struct Bare;
        impl CollectorHost for Bare {
            fn dataplane(&self) -> Result<HostDataplane, String> {
                Err("no device".into())
            }
            fn freeze_facts(&self) -> Result<HostFreezeFacts, String> {
                Ok(HostFreezeFacts::default())
            }
            fn attach_tee(&self, _tee: Arc<dyn TunFrameTee>) -> Result<(), String> {
                Err("no device".into())
            }
            fn detach_tee(&self) -> Result<(), String> {
                Ok(())
            }
            fn paired_read(&self, _at_mono: u64, _at_unix: i64) -> Result<PairedRead, String> {
                Err("no device".into())
            }
        }
        let bare: Arc<dyn CollectorHost> = Arc::new(Bare);
        assert_eq!(bare.wg_stats(), None);
        println!("[I6] default host wg_stats: None (the status line prints na)");
    }

    // ---- I7 ----

    /// The embedded self-identification is well-formed by construction (design
    /// §3.3/A-1 requires a NON-EMPTY digest): 64 hex characters, and the
    /// `collector{}` block carries it verbatim with the frozen four fields.
    #[test]
    fn the_collector_block_identifies_the_build_with_the_frozen_shape() {
        assert_eq!(BUILD_SHA256.len(), 64, "embedded build digest: {BUILD_SHA256}");
        assert!(
            BUILD_SHA256.bytes().all(|b| b.is_ascii_hexdigit()),
            "embedded build digest is not hex: {BUILD_SHA256}"
        );
        assert!(!BUILD_TARGET_TRIPLE.is_empty(), "target triple must be non-empty");

        let block = collector_block_json("", None);
        let document = format!("{{{block}}}");
        // §8-1 discipline: even the block alone is strict JSON
        let parsed = crate::config::parse_document(&document).expect("block is strict JSON");
        let _ = parsed;
        assert!(block.contains(&format!("\"build_sha256\":\"{BUILD_SHA256}\"")));
        assert!(block.contains(&format!(
            "\"crate_version\":\"{}\"",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(block.contains(&format!("\"target_triple\":\"{BUILD_TARGET_TRIPLE}\"")));
        assert!(block.contains(&format!("\"build_source\":\"{BUILD_SOURCE_EMBEDDED}\"")));
        println!("[I7] embedded collector{{}} block: {document}");
    }

    /// §3.3's fallback: an operator-reported digest wins and is LABELLED as
    /// reported (never passed off as the embedded value), and the embedded
    /// digest is what an absent/blank report falls back to.
    #[test]
    fn a_reported_digest_wins_and_is_labelled_operator_reported() {
        let reported = "a".repeat(64);
        let block = collector_block_json(&format!("  {reported}  "), None);
        assert!(block.contains(&format!("\"build_sha256\":\"{reported}\"")));
        assert!(block.contains(&format!(
            "\"build_source\":\"{BUILD_SOURCE_OPERATOR_REPORTED}\""
        )));
        assert!(!block.contains(BUILD_SHA256), "the embedded digest must not be claimed");
        for blank in ["", "   "] {
            let block = collector_block_json(blank, None);
            assert!(block.contains(&format!("\"build_sha256\":\"{BUILD_SHA256}\"")));
            assert!(block.contains(&format!("\"build_source\":\"{BUILD_SOURCE_EMBEDDED}\"")));
        }
        println!("[I7] reported digest precedence: operator-reported wins, embedded is the default");
    }

    /// N2-H I3a (v3.1 §2.1 R-2①): `exclusion_generation` is the block's LAST
    /// field (the four §3.3 fields keep their order and position), carries the
    /// shell-reported generation verbatim, and renders as `null` — present but
    /// value-less — when the shell reported none. The block stays strict JSON
    /// in both cases, so the judge can read the value mechanically.
    #[test]
    fn the_collector_block_carries_the_shell_reported_exclusion_generation() {
        let four = collector_block_json("", None);
        assert!(
            four.ends_with(",\"exclusion_generation\":null}"),
            "null generation is not the block's last field: {four}"
        );
        for gen in [0_u64, 1, 7, u32::MAX as u64] {
            let block = collector_block_json("", Some(gen));
            assert!(
                block.ends_with(&format!(",\"exclusion_generation\":{gen}}}")),
                "generation {gen} is not the block's appended last field: {block}"
            );
            let document = format!("{{{block}}}");
            let Json::Obj(outer) =
                crate::config::parse_document(&document).expect("block is strict JSON")
            else {
                panic!("the block wrapper is not a JSON object: {document}");
            };
            assert_eq!(outer.len(), 1, "one collector key: {block}");
            assert_eq!(outer[0].0, "collector");
            let Json::Obj(fields) = &outer[0].1 else {
                panic!("collector is not a JSON object: {block}");
            };
            assert_eq!(fields.len(), 5, "the block carries exactly five fields: {block}");
            assert_eq!(fields[4].0, "exclusion_generation", "appended last: {block}");
            assert_eq!(fields[4].1, Json::Num(gen), "verbatim value: {block}");
            // the four §3.3 fields keep their frozen order and meaning
            assert_eq!(
                fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
                vec![
                    "crate_version",
                    "target_triple",
                    "build_sha256",
                    "build_source",
                    "exclusion_generation"
                ]
            );
        }
        println!("[I3a] collector{{}} exclusion_generation: appended last, verbatim, null when absent");
    }

    /// The tail-append discipline (design §3.1/D10): the block is added AFTER
    /// every frozen renderer field, the untouched prefix is byte-identical to
    /// `n2h::Evidence::to_json()`, and the composed document still parses.
    #[test]
    fn the_collector_block_is_tail_appended_to_the_frozen_renderer_output() {
        let evidence = assemble_device_evidence(clean_material());
        let plain = evidence.to_json();
        assert!(plain.ends_with('}'), "the renderer emits one JSON object");
        for (reported, generation) in
            [("", None), ("b".repeat(64).as_str(), Some(2_u64)), ("", Some(0_u64))]
        {
            let document = with_collector_block(&plain, reported, generation);
            assert!(
                document.starts_with(&plain[..plain.len() - 1]),
                "the renderer's fields must be untouched"
            );
            assert!(document.ends_with(&format!(
                ",{}}}",
                collector_block_json(reported, generation)
            )));
            crate::config::parse_document(&document).expect("composed document is strict JSON");
            assert_eq!(
                document.matches("\"collector\":").count(),
                1,
                "exactly one collector block"
            );
        }
        println!(
            "[I7] tail append: {} -> {} bytes, one trailing collector{{}} block",
            plain.len(),
            with_collector_block(&plain, "", None).len()
        );
    }
}
