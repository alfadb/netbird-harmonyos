// N2 T5a「B 断流刺激」shell-side pure logic (preregistration v4 §4 B column /
// §7 T5a). ZERO imports on purpose: this module is the parse/format half of the
// armed-only private-channel entry, so the host selfcheck loads it
// byte-for-byte with no stubs at all.
//
// Two halves:
//   * `parseN2Stimulus` validates one launch payload
//     (`{"kind":"M","id":"b-M-1"}`) — kind ∈ the seven frozen outer classes,
//     id = the stimulus marker (same character set as the positive-control
//     ids). A malformed request is REFUSED here (never handed to the core);
//     the core refuses it again, independently.
//   * the formatters render the shell's own receipt line. The CORE line
//     (`N2_STIMULUS|…`) is the authoritative one (it carries the real fd and
//     endpoint); the shell line only proves the delivery path and is
//     `diag=true|is_evidence=false` like every other diagnostic marker.

export const N2_STIMULUS_FILE_NAME: string = 'netbird-n2-stimulus';

// The seven outer classes of the frozen preregistration. The list is closed:
// an eighth class needs a new pre-registration, not a longer array.
export const N2_STIMULUS_KINDS: string[] = ['M', 'S', 'R', 'I', 'Wb', 'Ws', 'D'];

export interface N2StimulusRequest {
  kind: string;
  id: string;
}

export interface N2StimulusDecision {
  request?: N2StimulusRequest;
  error: string;
}

export interface N2StimulusReply {
  ok: boolean;
  result?: string;
  reason?: string;
  target_fd?: number;
  remote?: string;
  line?: string;
}

// Fixed markers; nothing here echoes arbitrary text into a log line.
export function cleanN2StimulusToken(text: string): string {
  let out: string = '';
  for (let i = 0; i < text.length && out.length < 64; i++) {
    const c: string = text.charAt(i);
    out += /[A-Za-z0-9._:-]/.test(c) ? c : '_';
  }
  return out;
}

// One launch payload. The trim makes a trailing newline from the private
// channel harmless; an empty or oversized payload is refused by name.
export function parseN2Stimulus(text: string): N2StimulusDecision {
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
  const kind: Object | undefined = doc['kind'];
  if (typeof kind !== 'string' || N2_STIMULUS_KINDS.indexOf(kind) < 0) {
    return { error: 'invalid-kind' };
  }
  const id: Object | undefined = doc['id'];
  if (typeof id !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/.test(id)) {
    return { error: 'invalid-id' };
  }
  return { error: '', request: { kind: kind, id: id } };
}

// The core's answer. A malformed/absent body is a REJECTED reply, never a
// silently accepted one.
export function parseN2StimulusReply(raw: string): N2StimulusReply {
  const rejected: N2StimulusReply = { ok: false, result: 'rejected', reason: 'invalid-reply' };
  if (raw.length === 0) {
    return rejected;
  }
  let parsed: N2StimulusReply | null = null;
  try {
    parsed = JSON.parse(raw) as N2StimulusReply;
  } catch (error) {
    return rejected;
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return rejected;
  }
  const result: string = typeof parsed.result === 'string' ? parsed.result : 'rejected';
  return {
    ok: parsed.ok === true,
    result: result,
    reason: typeof parsed.reason === 'string' ? parsed.reason : 'none',
    target_fd: typeof parsed.target_fd === 'number' ? parsed.target_fd : -1,
    remote: typeof parsed.remote === 'string' ? parsed.remote : 'unknown',
    line: typeof parsed.line === 'string' ? parsed.line : ''
  };
}

// The shell receipt for one DELIVERED request (the core answered). The core's
// own `N2_STIMULUS|…` line stays the authoritative record; this line carries
// the request id so the delivery is attributable across the private channel.
export function formatN2StimulusShellLine(requestId: string, kind: string, id: string,
  reply: N2StimulusReply): string {
  return `N2_STIMULUS_SHELL|requestId=${cleanN2StimulusToken(requestId)}` +
    `|id=${cleanN2StimulusToken(id)}|kind=${cleanN2StimulusToken(kind)}` +
    `|core_ok=${reply.ok === true}|core_result=${cleanN2StimulusToken(reply.result ?? 'rejected')}` +
    `|reason=${cleanN2StimulusToken(reply.reason ?? 'none')}` +
    `|target_fd=${typeof reply.target_fd === 'number' ? reply.target_fd : -1}` +
    `|diag=true|is_evidence=false`;
}

// The shell receipt for a request that never reached the core (malformed
// payload, unreadable file): named refusal, no fabricated result.
export function formatN2StimulusShellReject(requestId: string, reason: string): string {
  return `N2_STIMULUS_SHELL|requestId=${cleanN2StimulusToken(requestId)}` +
    `|id=none|kind=none|core_ok=false|core_result=rejected` +
    `|reason=${cleanN2StimulusToken(reason)}|target_fd=-1|diag=true|is_evidence=false`;
}
