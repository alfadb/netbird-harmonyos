// N2 第一批 C/D 列「应用内证据环形文件」shell-side pure logic (2026-09-29).
// ZERO imports, the same discipline as the other N2 pure modules.
//
// The RING itself is written by the core (`client/core/src/n2_ring.rs`): every
// core `N2_*` / `N7_WG_FEED` / `N2_TUN_*` line is appended verbatim by the
// `hilog::emit` tee, and the extension forwards its own lines of those families
// through `n2_ring_append`. One persisted record is
//
//   N2R|sess=<arm token>|seq=<durable monotonic>|mono_ms=|wall_ms=|line=<原始行>
//
// so a page can be addressed by its DURABLE `seq` (across appends, rotation and
// restarts) instead of by a line index over concatenated segments. This module
// owns the shell half of that retrieval:
//
//   * the ring file's identity (basename / cap / segment count), and
//   * the `n2_ring_dump` private channel: the FALLBACK fetch path for the case
//     where the vendor-documented bundle transfer
//     (`hdc file recv -b <bundle> ./data/storage/el2/base/files/<file>`) is not
//     available on the device ROM (E005004) or the HAP is not debug-signed
//     (E003001). The operator then asks the App to re-emit a bounded window of
//     the ring on HiLog, which the host reads live after returning to Wi-Fi.
//
// One launch payload — the CORE's own seq-paged document:
//
//   {"from_seq":0,"to_seq":0,"count":64,"session":"<arm token>"}
//
// `from_seq` is the lowest durable `seq` to re-emit (0 = the whole ring),
// `to_seq` the frozen tail taken from a `n2_ring_manifest` line (0 / absent =
// the tail at serve time), `count` the page size (1..N2_RING_MAX_DUMP_LINES)
// and `session` an optional arm-token filter. The shell validates the document
// and forwards it verbatim to the core; the core answers the
// `N2_RING_CHUNK|seq=<durable>|of=<count>|line=<原件>` records plus one
// `N2_RING_DUMP` summary, and the shell mirrors the outcome in its own
// `N2_RING_DUMP_SHELL` receipt. The summary is NEVER written back into the ring
// (it is not ring content) and neither are the chunk records.
//
// The LEGACY line-index document (`{"start":<line index>}`) is refused with
// `legacy-start-index-unsupported`: a line index over concatenated segments has
// no safe mapping onto durable seqs, and pretending to serve it would silently
// duplicate or skip records.
//
// A dump is READ-ONLY with respect to the retrieved watermark: it reports the
// core's `acked_seq` (as the receipt's `ack_watermark=`) and NEVER advances it.
// Advancing it is the ONE job of the explicit host ACK channel — a SEPARATE
// launch payload
//
//   {"session":"<the arm token the records carry>","up_to_seq":<durable seq>}
//
// which the host may only send after it has PERSISTED the fetched bytes and
// verified the window is complete. BOTH keys are required; the shell parses the
// document (answering the same named refusals the core answers for the same
// document), forwards it VERBATIM to `n2_ring_ack`, and mirrors the outcome in
// `N2_RING_ACK_SHELL|…|advanced=…`. The CORE binds the request to the CURRENT
// arm under its own lock, so a stale session, a seq beyond the tail and an
// unarmed ring are all refused, and a repeat is idempotent — the shell never
// reads the session and then acks unconditionally.

export const N2_RING_FILE_NAME: string = 'netbird-n2-evidence-ring.log';
export const N2_RING_DUMP_FILE_NAME: string = 'netbird-n2-ring-dump';
export const N2_RING_ACK_FILE_NAME: string = 'netbird-n2-ring-ack';
// Must match `client/core/src/n2_ring.rs` defaults (the shell passes them to
// `n2_ring_configure`; the core validates and can only tighten).
export const N2_RING_MAX_BYTES: number = 4 * 1024 * 1024;
export const N2_RING_SEGMENTS: number = 3;
// The smallest segment cap the core accepts (`n2_ring::MIN_MAX_BYTES`). The
// diagnostic `ring_segment_bytes` arm-ticket key may SHRINK the ring between
// this bound and `N2_RING_MAX_BYTES`; the core re-validates the same bound.
export const N2_RING_MIN_BYTES: number = 64 * 1024;
// One launch re-emits at most this many records (bounded hilog traffic); the
// core's own `MAX_DUMP_LINES` is the same bound.
export const N2_RING_MAX_DUMP_LINES: number = 256;
// The largest accepted request document; the core's `MAX_DUMP_REQUEST_BYTES`
// is the same bound, so a larger document is refused by the same name before
// any core call.
export const N2_RING_MAX_DUMP_REQUEST: number = 512;
// The ACK channel's own bound; the core's `MAX_ACK_REQUEST_BYTES` is the same.
export const N2_RING_MAX_ACK_REQUEST: number = 512;
// The core's own default page size (`DEFAULT_DUMP_COUNT`).
export const N2_RING_DEFAULT_DUMP_COUNT: number = 64;

export interface N2RingDumpRequest {
  fromSeq: number;
  toSeq: number;
  // A `to_seq` of 0 is a real (empty) window, so "absent" is its own bit: the
  // core reads an absent `to_seq` as "the tail at serve time".
  hasToSeq: boolean;
  count: number;
  session: string;
}

export interface N2RingDumpDecision {
  request?: N2RingDumpRequest;
  error: string;
}

// -2 = the key is absent; -1 = present but not a non-negative integer.
const N2_RING_SEQ_ABSENT: number = -2;
const N2_RING_SEQ_MALFORMED: number = -1;

export function cleanN2RingToken(text: string): string {
  let out: string = '';
  for (let i = 0; i < text.length && out.length < 64; i++) {
    const c: string = text.charAt(i);
    out += /[A-Za-z0-9._:-]/.test(c) ? c : '_';
  }
  return out;
}

function ringSeqField(value: Object | undefined): number {
  if (value === undefined) {
    return N2_RING_SEQ_ABSENT;
  }
  if (typeof value !== 'number' || !Number.isInteger(value as number) || (value as number) < 0) {
    return N2_RING_SEQ_MALFORMED;
  }
  return value as number;
}

// Parse one `n2_ring_dump` request document. Every refusal is the SAME named
// token the core answers for the same document, so a refusal reads identically
// whether the shell or the core produced it.
export function parseN2RingDump(text: string): N2RingDumpDecision {
  const trimmed: string = text.trim();
  if (trimmed.length === 0) {
    return { error: 'empty-request' };
  }
  if (trimmed.length > N2_RING_MAX_DUMP_REQUEST) {
    return { error: 'request-too-long' };
  }
  let doc: Record<string, Object> | null = null;
  try {
    doc = JSON.parse(trimmed) as Record<string, Object>;
  } catch (error) {
    return { error: 'invalid-json' };
  }
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) {
    return { error: 'invalid-request' };
  }
  if (doc['start'] !== undefined) {
    return { error: 'legacy-start-index-unsupported' };
  }
  const rawFrom: number = ringSeqField(doc['from_seq']);
  if (rawFrom === N2_RING_SEQ_MALFORMED) {
    return { error: 'invalid-request' };
  }
  const rawTo: number = ringSeqField(doc['to_seq']);
  if (rawTo === N2_RING_SEQ_MALFORMED) {
    return { error: 'invalid-request' };
  }
  let count: number = N2_RING_DEFAULT_DUMP_COUNT;
  const rawCount: Object | undefined = doc['count'];
  if (rawCount !== undefined) {
    if (typeof rawCount !== 'number' || !Number.isInteger(rawCount as number) ||
      (rawCount as number) < 1 || (rawCount as number) > N2_RING_MAX_DUMP_LINES) {
      return { error: 'invalid-count' };
    }
    count = rawCount as number;
  }
  let session: string = '';
  const rawSession: Object | undefined = doc['session'];
  if (rawSession !== undefined) {
    if (typeof rawSession !== 'string') {
      return { error: 'invalid-request' };
    }
    session = rawSession as string;
    if (session.length === 0) {
      return { error: 'invalid-session' };
    }
  }
  return { error: '', request: {
    fromSeq: rawFrom === N2_RING_SEQ_ABSENT ? 0 : rawFrom,
    toSeq: rawTo === N2_RING_SEQ_ABSENT ? 0 : rawTo,
    hasToSeq: rawTo !== N2_RING_SEQ_ABSENT,
    count: count,
    session: session
  } };
}

// The shell's own receipt of one `n2_ring_dump` launch: the CORRELATION half
// (requestId + the window the shell asked for + the retrieved watermark the
// core reported at serve time). The core's `N2_RING_CHUNK` / `N2_RING_DUMP`
// lines stay authoritative for the record content and the counters.
//
// `ackWatermark` is the core's `acked_seq` AS IT STANDS — a dump advances
// nothing, so the receipt names it `ack_watermark` and never claims a
// confirmation. `-1` = the core did not answer one (a refusal / a throw).
export function formatN2RingDumpShellLine(requestId: string, fromSeq: number, toSeq: number,
  count: number, emitted: number, ackWatermark: number, result: string, reason: string): string {
  return `N2_RING_DUMP_SHELL|requestId=${cleanN2RingToken(requestId)}` +
    `|from_seq=${String(fromSeq)}|to_seq=${String(toSeq)}|count=${String(count)}` +
    `|emitted=${String(emitted)}|ack_watermark=${String(ackWatermark)}` +
    `|result=${cleanN2RingToken(result)}|reason=${cleanN2RingToken(reason)}` +
    `|diag=true|is_evidence=false`;
}

// The shell receipt for a document that never became a request (empty / torn /
// oversized / legacy / malformed private-channel payload): named refusal, no
// fabricated window and no core call.
export function formatN2RingDumpShellReject(requestId: string, reason: string): string {
  return `N2_RING_DUMP_SHELL|requestId=${cleanN2RingToken(requestId)}` +
    `|from_seq=0|to_seq=0|count=0|emitted=0|ack_watermark=-1` +
    `|result=rejected|reason=${cleanN2RingToken(reason)}|diag=true|is_evidence=false`;
}

// One explicit host ACK request: the arm the records belong to plus the durable
// seq the host has persisted and verified. BOTH are required — an ACK without a
// session could release another arm's segments, and one without `up_to_seq`
// confirms nothing.
export interface N2RingAckRequest {
  session: string;
  upToSeq: number;
}

export interface N2RingAckDecision {
  request?: N2RingAckRequest;
  error: string;
}

// Parse one `n2_ring_ack` request document. Every refusal is the SAME named
// token the core answers for the same document, so a refusal reads identically
// whether the shell or the core produced it.
export function parseN2RingAck(text: string): N2RingAckDecision {
  const trimmed: string = text.trim();
  if (trimmed.length === 0) {
    return { error: 'empty-request' };
  }
  if (trimmed.length > N2_RING_MAX_ACK_REQUEST) {
    return { error: 'request-too-long' };
  }
  let doc: Record<string, Object> | null = null;
  try {
    doc = JSON.parse(trimmed) as Record<string, Object>;
  } catch (error) {
    return { error: 'invalid-json' };
  }
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) {
    return { error: 'invalid-request' };
  }
  const rawSession: Object | undefined = doc['session'];
  if (rawSession !== undefined && typeof rawSession !== 'string') {
    return { error: 'invalid-request' };
  }
  // Absent and empty are the same refusal: neither names an arm.
  if (rawSession === undefined || (rawSession as string).length === 0) {
    return { error: 'invalid-session' };
  }
  const rawUpTo: number = ringSeqField(doc['up_to_seq']);
  if (rawUpTo === N2_RING_SEQ_ABSENT || rawUpTo === N2_RING_SEQ_MALFORMED) {
    return { error: 'invalid-request' };
  }
  return { error: '', request: { session: rawSession as string, upToSeq: rawUpTo } };
}

// The shell's own receipt of one `n2_ring_ack` launch: the request it forwarded
// (requestId + the arm + the watermark it named) and the core's answer. This is
// the ONLY line that reports a watermark ADVANCEMENT (`advanced=true`); a
// refused request reports `advanced=false` and the untouched watermark.
export function formatN2RingAckShellLine(requestId: string, session: string, upToSeq: number,
  ackedSeq: number, advanced: boolean, result: string, reason: string): string {
  return `N2_RING_ACK_SHELL|requestId=${cleanN2RingToken(requestId)}` +
    `|sess=${cleanN2RingToken(session)}|up_to_seq=${String(upToSeq)}|acked_seq=${String(ackedSeq)}` +
    `|advanced=${advanced ? 'true' : 'false'}|result=${cleanN2RingToken(result)}` +
    `|reason=${cleanN2RingToken(reason)}|diag=true|is_evidence=false`;
}

// The shell receipt for an ACK document that never became a request (empty /
// torn / oversized / malformed): named refusal, no fabricated arm or watermark,
// and no core call — so no watermark can move.
export function formatN2RingAckShellReject(requestId: string, reason: string): string {
  return `N2_RING_ACK_SHELL|requestId=${cleanN2RingToken(requestId)}` +
    `|sess=-|up_to_seq=-1|acked_seq=-1|advanced=false|result=rejected` +
    `|reason=${cleanN2RingToken(reason)}|diag=true|is_evidence=false`;
}
