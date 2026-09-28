// N2 P4「A 列生产事件触发原语」shell-side pure logic (preregistration v4.1 §4
// 「A 列生产事件的操作性定义（v4.1 澄清，不改判据）」). ZERO imports on purpose,
// the same discipline as NetBirdN2Stimulus.ets / NetBirdN2ControlRequest.ets:
// this module is the parse/format half of the armed-only private channel, so
// the host selfcheck loads it byte-for-byte with no stubs at all.
//
// Two halves:
//   * `parseN2AEvent` validates one launch payload
//     (`{"kind":"M|R|D|S","id":"a-M-1"}`) — kind ∈ the four kinds the core
//     accepts (all four are frozen A-column triggers: `M` a read-only
//     management RPC, `R` the server-driven HealthCheck echo, `D` the
//     protected DNS requery, `S` the signal receive watchdog's self-addressed
//     HEARTBEAT), id = the same `[A-Za-z0-9_-]{1,64}` marker rule as the
//     stimulus/control ids. A malformed request is REFUSED here (never handed
//     to the core); the core refuses it again, independently.
//   * the formatters render the shell's own receipt line. The CORE line
//     (`N2_A_EVENT|…`) is the authoritative one (it carries the real fd/dup of
//     the existing production socket); the shell line only proves the delivery
//     path and is `diag=true|is_evidence=false` like every other diagnostic.
//
// `S` is accepted by the SHAPE and dispatched like the other three: the core
// asks the registered signal receive watchdog for ONE self-addressed HEARTBEAT
// on the SAME stream (the P8 product watchdog sends it anyway; this only moves
// its send time) and answers `result=sent`, or an honest
// `result=error|reason=no-live-signal-connection|no-registered-signal-stream|
// signal-heartbeat-<class>|signal-heartbeat-timeout` — never a fabricated send.

export const N2_A_EVENT_FILE_NAME: string = 'netbird-n2-a-event';

// The four kinds this channel accepts. `M`/`R`/`D`/`S` are the frozen
// A-column triggers (see the header); the list is closed so an unknown kind is
// a parse-level refusal and a known one always reaches the core's own verdict.
export const N2_A_EVENT_KINDS: string[] = ['M', 'R', 'D', 'S'];

export interface N2AEventRequest {
  kind: string;
  id: string;
}

export interface N2AEventDecision {
  request?: N2AEventRequest;
  error: string;
}

export interface N2AEventReply {
  ok: boolean;
  result?: string;
  reason?: string;
  kind?: string;
  fd?: number;
  dup_fd?: number;
  line?: string;
}

// Fixed markers; nothing here echoes arbitrary text into a log line.
export function cleanN2AEventToken(text: string): string {
  let out: string = '';
  for (let i = 0; i < text.length && out.length < 64; i++) {
    const c: string = text.charAt(i);
    out += /[A-Za-z0-9._:-]/.test(c) ? c : '_';
  }
  return out;
}

// One launch payload. The trim makes a trailing newline from the private
// channel harmless; an empty or oversized payload is refused by name.
export function parseN2AEvent(text: string): N2AEventDecision {
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
  if (typeof kind !== 'string' || N2_A_EVENT_KINDS.indexOf(kind) < 0) {
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
export function parseN2AEventReply(raw: string): N2AEventReply {
  const rejected: N2AEventReply = { ok: false, result: 'rejected', reason: 'invalid-reply' };
  if (raw.length === 0) {
    return rejected;
  }
  let parsed: N2AEventReply | null = null;
  try {
    parsed = JSON.parse(raw) as N2AEventReply;
  } catch (error) {
    return rejected;
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    return rejected;
  }
  return {
    ok: parsed.ok === true,
    result: typeof parsed.result === 'string' ? parsed.result : 'rejected',
    reason: typeof parsed.reason === 'string' ? parsed.reason : 'none',
    kind: typeof parsed.kind === 'string' ? parsed.kind : 'none',
    fd: typeof parsed.fd === 'number' ? parsed.fd : -1,
    dup_fd: typeof parsed.dup_fd === 'number' ? parsed.dup_fd : -1,
    line: typeof parsed.line === 'string' ? parsed.line : ''
  };
}

// The shell receipt for one DELIVERED request (the core answered). The core's
// own `N2_A_EVENT|…` line stays authoritative; this line carries the request
// id so the delivery is attributable across the private channel.
export function formatN2AEventShellLine(requestId: string, kind: string, id: string,
  reply: N2AEventReply): string {
  return `N2_A_EVENT_SHELL|requestId=${cleanN2AEventToken(requestId)}` +
    `|id=${cleanN2AEventToken(id)}|kind=${cleanN2AEventToken(kind)}` +
    `|core_ok=${reply.ok === true}|core_result=${cleanN2AEventToken(reply.result ?? 'rejected')}` +
    `|reason=${cleanN2AEventToken(reply.reason ?? 'none')}` +
    `|fd=${typeof reply.fd === 'number' ? reply.fd : -1}` +
    `|dup_fd=${typeof reply.dup_fd === 'number' ? reply.dup_fd : -1}` +
    `|diag=true|is_evidence=false`;
}

// The shell receipt for a request that never reached the core (malformed
// payload, unreadable file): named refusal, no fabricated trigger result.
export function formatN2AEventShellReject(requestId: string, reason: string): string {
  return `N2_A_EVENT_SHELL|requestId=${cleanN2AEventToken(requestId)}` +
    `|id=none|kind=none|core_ok=false|core_result=rejected` +
    `|reason=${cleanN2AEventToken(reason)}|fd=-1|dup_fd=-1|diag=true|is_evidence=false`;
}
