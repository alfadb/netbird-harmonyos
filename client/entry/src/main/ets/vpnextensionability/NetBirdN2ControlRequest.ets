// N2 T3 §3 / §4 D positive-control private channel — shell-side pure logic.
// ZERO imports on purpose (the same discipline as NetBirdN2Stimulus.ets): this
// module is the parse/format half of the armed-only `n2_control` entry, so the
// host selfcheck loads it byte-for-byte with no stubs at all.
//
// One launch payload (the private channel carries it verbatim):
//
//   {"target":"119.29.29.119:53","proto":"udp","id":"evt1-pre"}
//
// * `target` is one frozen outer destination `IP:port`. This module validates
//   the SHAPE only; "the IP must be inside the armed ticket's `capture_32`" is
//   the arm/core half's own check (`validateN2Control` in NetBirdN2Evidence /
//   `ControlState::register` in the core) and answers `target-not-captured`
//   there — never silently here.
// * `proto` is `tcp` or `udp`, the two production openers the control reuses.
// * `id` is the positive-control marker id (same `[A-Za-z0-9_-]{1,64}` rule as
//   the stimulus ids and the core marker). The core refuses a REUSED id
//   (`duplicate-control`), so each sandwich leg needs a fresh id.
//
// Batching is NOT implemented (preregistration-compatible minimum): a
// `{"controls":[...]}` payload is REFUSED BY NAME here (`batch-not-implemented`)
// instead of being half-applied or silently ignored.

export const N2_CONTROL_FILE_NAME: string = 'netbird-n2-control';

// The two production openers a control may reuse (`mgmtSocketOpen` for tcp,
// `iceSocketOpen` for udp). Closed list: a third protocol needs the frozen
// preregistration updated first.
export type N2ControlProto = 'tcp' | 'udp';

export interface N2ControlRequest {
  target: string;
  proto: N2ControlProto;
  id: string;
}

export interface N2ControlDecision {
  request?: N2ControlRequest;
  error: string;
}

// Fixed markers; nothing here echoes arbitrary text into a log line.
export function cleanN2ControlToken(text: string): string {
  let out: string = '';
  for (let i = 0; i < text.length && out.length < 64; i++) {
    const c: string = text.charAt(i);
    out += /[A-Za-z0-9._:-]/.test(c) ? c : '_';
  }
  return out;
}

// Dotted quad + port 1..65535 — the same target shape the shell's
// `validateN2Control` and the core's `Ipv4Addr`/`u16` parse accept.
const N2_CONTROL_TARGET_SHAPE: RegExp =
  /^((?:0|[1-9][0-9]{0,2})(?:\.(?:0|[1-9][0-9]{0,2})){3}):([1-9][0-9]{0,4})$/;

// One launch payload. The trim makes a trailing newline from the private
// channel harmless; an empty or oversized payload is refused by name.
export function parseN2ControlRequest(text: string): N2ControlDecision {
  const trimmed: string = text.trim();
  if (trimmed.length === 0) {
    return { error: 'empty-request' };
  }
  if (trimmed.length > 256) {
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
  if (doc['controls'] !== undefined) {
    return { error: 'batch-not-implemented' };
  }
  const target: Object | undefined = doc['target'];
  if (typeof target !== 'string') {
    return { error: 'invalid-target' };
  }
  const match: RegExpExecArray | null = N2_CONTROL_TARGET_SHAPE.exec(target);
  if (match === null ||
    match[1].split('.').some((part: string): boolean => Number(part) > 255) ||
    Number(match[2]) > 65535) {
    return { error: 'invalid-target' };
  }
  const rawProto: Object | undefined = doc['proto'];
  const protoText: string = typeof rawProto === 'string' ? rawProto : '';
  // ArkTS does not narrow an `Object` to the literal union, so the two
  // accepted protocols are assigned from their literals explicitly; anything
  // else (`icmp`, `UDP`, a non-string, a missing key) is refused by name.
  let proto: N2ControlProto;
  if (protoText === 'tcp') {
    proto = 'tcp';
  } else if (protoText === 'udp') {
    proto = 'udp';
  } else {
    return { error: 'invalid-protocol' };
  }
  const id: Object | undefined = doc['id'];
  if (typeof id !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/.test(id)) {
    return { error: 'invalid-id' };
  }
  return { error: '', request: { target: target, proto: proto, id: id } };
}

// The shell's delivery receipt for one request that reached the send call.
// `result` is `sent` (the send call returned without throwing) or `rejected`
// (the armed-but-refused shapes), `reason` names the exact branch. The CORE's
// own `N2_CONTROL_SENT` / `N2_CONTROL_HIT` / `N2_CONTROL_MISS` lines stay
// authoritative for what the marker actually did on the wire/TUN.
export function formatN2ControlShellLine(requestId: string, request: N2ControlRequest,
  result: string, reason: string): string {
  return `N2_CONTROL_SHELL|requestId=${cleanN2ControlToken(requestId)}` +
    `|id=${cleanN2ControlToken(request.id)}|target=${cleanN2ControlToken(request.target)}` +
    `|proto=${cleanN2ControlToken(request.proto)}|result=${cleanN2ControlToken(result)}` +
    `|reason=${cleanN2ControlToken(reason)}|diag=true|is_evidence=false`;
}

// The shell receipt for a payload that never became a request (malformed
// payload, unreadable file): named refusal, no fabricated target/result.
export function formatN2ControlShellReject(requestId: string, reason: string): string {
  return `N2_CONTROL_SHELL|requestId=${cleanN2ControlToken(requestId)}` +
    `|id=none|target=none|proto=none|result=rejected` +
    `|reason=${cleanN2ControlToken(reason)}|diag=true|is_evidence=false`;
}
