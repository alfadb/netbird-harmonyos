// N2 第一批 C/D 列「切网夹心」shell-side pure logic (2026-09-29).
// ZERO imports on purpose (the same discipline as the other N2 pure modules):
// the host selfcheck loads this file byte-for-byte with no stubs.
//
// One launch payload (the private channel carries it verbatim):
//
//   {"column":"C","kind":"M","rep":1,"id":"cd-C-M-1"}
//
// * `column` is `C` (Wi-Fi -> 蜂窝) or `D` (蜂窝 -> Wi-Fi) — the two lifecycle
//   columns whose cellular phase has no live host channel.
// * `kind` is one of the seven frozen outer classes.
// * `rep` is the repetition index (the frozen §3 minimum is three independent
//   events, so 1..3 for a clean plan; 1..999 is accepted).
// * `id` is the plan id; the two control markers are DERIVED from it
//   (`<id>-pre` / `<id>-post`), so the id is capped at 59 characters to keep
//   both derivations inside the core control registry's `[A-Za-z0-9_-]{1,64}`
//   marker rule. Use a FRESH id per event (the control registry refuses a
//   reused id).
//
// The module also owns the two PURE decisions the operator's network facts
// feed: which frozen outer destination a kind's control marker goes to (with
// the kind's own protocol), and whether the measured bearer transition matches
// the column's direction. Both answer a NAMED refusal token instead of guessing.

export const N2_CD_FILE_NAME: string = 'netbird-n2-cd-plan';
export const N2_CD_COLUMNS: string[] = ['C', 'D'];
export const N2_CD_KINDS: string[] = ['M', 'S', 'R', 'D', 'I', 'Wb', 'Ws'];
// The longest plan id whose `-pre`/`-post` markers still satisfy the core's
// `[A-Za-z0-9_-]{1,64}` control marker rule.
export const N2_CD_MAX_ID: number = 59;
// @ohos.net.connection `NetBearType` literals (SDK 26.0.0.821: cellular=0,
// wifi=1, bluetooth=2, ethernet=3, vpn=4).
export const N2_BEARER_CELLULAR: number = 0;
export const N2_BEARER_WIFI: number = 1;
export const N2_BEARER_BLUETOOTH: number = 2;
export const N2_BEARER_ETHERNET: number = 3;
export const N2_BEARER_VPN: number = 4;

export interface N2CdPlanRequest {
  column: string;
  kind: string;
  rep: number;
  id: string;
}

export interface N2CdPlanDecision {
  request?: N2CdPlanRequest;
  error: string;
}

// The measured physical network at one instant: its classified bearer and its
// own local address (`unknown` when the platform could not tell — never an
// invented address).
export interface N2BearerFacts {
  bearer: string;
  local: string;
}

// Fixed markers; nothing here echoes arbitrary text into a log line.
export function cleanN2CdToken(text: string): string {
  let out: string = '';
  for (let i = 0; i < text.length && out.length < 64; i++) {
    const c: string = text.charAt(i);
    out += /[A-Za-z0-9._:-]/.test(c) ? c : '_';
  }
  return out;
}

export function parseN2CdPlan(text: string): N2CdPlanDecision {
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
  const rawColumn: Object | undefined = doc['column'];
  if (typeof rawColumn !== 'string' || N2_CD_COLUMNS.indexOf(rawColumn) < 0) {
    return { error: 'invalid-column' };
  }
  const rawKind: Object | undefined = doc['kind'];
  if (typeof rawKind !== 'string' || N2_CD_KINDS.indexOf(rawKind) < 0) {
    return { error: 'invalid-kind' };
  }
  const rawRep: Object | undefined = doc['rep'];
  if (typeof rawRep !== 'number' || !Number.isInteger(rawRep) ||
    (rawRep as number) < 1 || (rawRep as number) > 999) {
    return { error: 'invalid-rep' };
  }
  const rawId: Object | undefined = doc['id'];
  if (typeof rawId !== 'string' ||
    !new RegExp('^[A-Za-z0-9_-]{1,' + String(N2_CD_MAX_ID) + '}$').test(rawId)) {
    return { error: 'invalid-id' };
  }
  return { error: '', request: { column: rawColumn, kind: rawKind, rep: rawRep as number,
    id: rawId } };
}

// The kind's own protocol: M/S/R are the TCP rows, D/I/Wb/Ws the UDP rows. A
// control marker must be sent through the production opener of ITS kind, so the
// protocol is never a free choice.
export function n2CdProtocol(kind: string): string {
  return kind === 'M' || kind === 'S' || kind === 'R' ? 'tcp' : 'udp';
}

// The frozen destination a kind's marker goes to, chosen from the armed
// ticket's `outer_destinations` (`<proto>/<ip>:<port>`): the kind's protocol
// only, preferring the frozen I destination (`udp/…:3478`, STUN) for I and the
// protected-DNS port (`udp/…:53`) for D, else the first entry of that protocol.
// '' = there is none, and the caller refuses with `no-target` instead of
// inventing one.
export function selectN2CdTarget(outerDestinations: string[], kind: string): string {
  const wanted: string = n2CdProtocol(kind);
  const preferredPort: string = kind === 'I' ? '3478' : (kind === 'D' ? '53' : '');
  let fallback: string = '';
  for (let i = 0; i < outerDestinations.length; i++) {
    const entry: string = outerDestinations[i];
    const slash: number = entry.indexOf('/');
    if (slash <= 0 || entry.substring(0, slash) !== wanted) {
      continue;
    }
    const target: string = entry.substring(slash + 1);
    if (target.length === 0) {
      continue;
    }
    if (fallback.length === 0) {
      fallback = target;
    }
    if (preferredPort.length === 0) {
      return target;
    }
    const colon: number = target.lastIndexOf(':');
    if (colon > 0 && target.substring(colon + 1) === preferredPort) {
      return target;
    }
  }
  return fallback;
}

// The measured physical network's bearer, from the platform's `bearerTypes`
// list plus its link addresses. Precedence is Wi-Fi > cellular > ethernet >
// bluetooth; a VPN bearer is NOT a physical bearer and answers `unknown` on its
// own. `unknown` is the honest answer that makes the caller UNSAT — the frozen
// direction judgement requires a measured bearer.
export function classifyN2Bearer(bearerTypes: number[],
  linkAddresses: string[]): N2BearerFacts {
  let bearer: string = 'unknown';
  if (bearerTypes.indexOf(N2_BEARER_WIFI) >= 0) {
    bearer = 'wifi';
  } else if (bearerTypes.indexOf(N2_BEARER_CELLULAR) >= 0) {
    bearer = 'cellular';
  } else if (bearerTypes.indexOf(N2_BEARER_ETHERNET) >= 0) {
    bearer = 'ethernet';
  } else if (bearerTypes.indexOf(N2_BEARER_BLUETOOTH) >= 0) {
    bearer = 'bluetooth';
  }
  let local: string = 'unknown';
  for (let i = 0; i < linkAddresses.length; i++) {
    const address: string = linkAddresses[i];
    if (typeof address === 'string' && address.length > 0) {
      local = address;
      break;
    }
  }
  return { bearer: bearer, local: local };
}

// The column's expected STARTING bearer OF ITS MEASURED EVENT: C measures
// Wi-Fi → 蜂窝, D measures 蜂窝 → Wi-Fi. This is NOT the plan-ACCEPTANCE bearer
// — both columns are issued over the Wi-Fi hdc link (see n2CdStartVerdict).
export function n2CdStartBearer(column: string): string {
  return column === 'C' ? 'wifi' : 'cellular';
}

// The column's expected TARGET bearer after its measured switch.
export function n2CdTargetBearer(column: string): string {
  return column === 'C' ? 'cellular' : 'wifi';
}

// The plan-ACCEPTANCE verdict. The private channel IS the Wi-Fi hdc link, so
// both columns are accepted while the device is on Wi-Fi: a C plan is then in
// position for its Wi-Fi → 蜂窝 switch, and a D plan is PRE-SET here, reaching
// its cellular start through its own preparation hop (n2CdTransitionStep).
// `unknown` is the honest UNSAT — never a guess.
export function n2CdStartVerdict(column: string, before: string): string {
  if (before === 'unknown') {
    return 'bearer-unknown';
  }
  return before === 'wifi' ? '' : 'start-network-mismatch';
}

// The direction verdict at switch time: '' = the measured transition matches
// the column, otherwise a named refusal.
export function n2CdDirectionVerdict(column: string, before: string, after: string): string {
  if (before === 'unknown' || after === 'unknown') {
    return 'bearer-unknown';
  }
  if (before !== n2CdStartBearer(column)) {
    return 'start-network-mismatch';
  }
  return after === n2CdTargetBearer(column) ? '' : 'direction-mismatch';
}

// WHICH plan step a measured bearer transition is:
//   * `prepare` — the D column's Wi-Fi → 非-Wi-Fi PREPARATION hop: recorded,
//     never the D measurement, no control sent for it and the production
//     recovery stays ungated. The cellular-side recovery afterwards is what
//     puts the plan in position.
//   * `switch` — the FORMAL switch the column measures.
//   * `none` — the transition is not this plan's at all: a D plan still on its
//     acceptance bearer that moved between Wi-Fi networks WITHOUT leaving
//     Wi-Fi. The plan keeps waiting instead of being closed by a direction
//     refusal that has nothing to do with its measurement.
// Only a plan still on its acceptance bearer can see a preparation hop, so the
// shell's own `before` reading decides it — never a second platform read.
export function n2CdTransitionStep(column: string, before: string, after: string): string {
  if (column !== 'C' && column !== 'D') {
    return 'none';
  }
  if (column === 'D' && before === 'wifi') {
    return after === 'wifi' ? 'none' : 'prepare';
  }
  return 'switch';
}

// The preparation hop's own verdict: '' only when it landed on the D column's
// cellular start bearer; anything else is a named refusal the core closes the
// plan with (an unmeasurable bearer is UNSAT, never a guess).
export function n2CdPrepareVerdict(after: string): string {
  if (after === 'unknown') {
    return 'bearer-unknown';
  }
  return after === n2CdStartBearer('D') ? '' : 'direction-mismatch';
}

// The shell's own delivery/phase receipt for one plan step. The core's
// `N2_CD_SANDWICH|…` lines stay authoritative for the ordering verdict.
export function formatN2CdPlanShellLine(requestId: string, request: N2CdPlanRequest,
  phase: string, marker: string, result: string, reason: string): string {
  return `N2_CD_SANDWICH_SHELL|requestId=${cleanN2CdToken(requestId)}` +
    `|id=${cleanN2CdToken(request.id)}|column=${cleanN2CdToken(request.column)}` +
    `|kind=${cleanN2CdToken(request.kind)}|rep=${String(request.rep)}` +
    `|phase=${cleanN2CdToken(phase)}|marker=${cleanN2CdToken(marker)}` +
    `|result=${cleanN2CdToken(result)}|reason=${cleanN2CdToken(reason)}` +
    `|diag=true|is_evidence=false`;
}

// The shell receipt for a payload that never became a plan (malformed payload,
// unreadable file): named refusal, no fabricated target/result.
export function formatN2CdPlanShellReject(requestId: string, reason: string): string {
  return `N2_CD_SANDWICH_SHELL|requestId=${cleanN2CdToken(requestId)}` +
    `|id=none|column=none|kind=none|rep=0|phase=plan|marker=none|result=rejected` +
    `|reason=${cleanN2CdToken(reason)}|diag=true|is_evidence=false`;
}
