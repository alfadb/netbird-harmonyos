// N2 P5「I/Ws 的 App 侧夹心编排」shell-side pure logic (preregistration v4.1
// §4 A 列 I/Ws 行 + §3 第 2 条的逐事件夹心). ZERO imports on purpose, the same
// discipline as the other N2 pure modules, so the host selfcheck loads it
// byte-for-byte with no stubs.
//
// Three pure decisions the extension makes around the REAL production path:
//   * `selectN2SandwichTarget` — which frozen outer destination the control
//     marker goes to. The frozen text names the I destination (`STUN
//     server IP:3478`, protocol udp); the preferred entry is therefore the
//     first `udp/<ip>:3478` of the armed ticket's `outer_destinations`, with
//     any other udp entry as the fallback. No entry = no target, and the
//     sandwich is reported `no-target` instead of inventing one.
//   * `formatN2SandwichLegMarker` — the leg's marker. It must satisfy the same
//     `[A-Za-z0-9_-]{1,64}` rule the core control registry and the sandwich
//     state enforce (a reused control id is refused), and the PRE and POST
//     controls of one event must be DIFFERENT markers, so the phase suffix is
//     part of the id.
//   * `parseN2SandwichPoll` — the core's `n2_sandwich_poll` answer: the legs
//     whose first production send has been observed and whose POST control the
//     shell must now send. Malformed entries are dropped, never guessed.
//
// The wait bounds live in the CORE (`n2_sandwich::PRE_WAIT_MS` = 3000 ms for
// the pre HIT before the first send, `POST_WAIT_MS` = 90000 ms for the first
// send before the post); this module only formats.

export interface N2SandwichPost {
  kind: string;
  marker: string;
}

export function cleanN2SandwichToken(text: string): string {
  let out: string = '';
  for (let i = 0; i < text.length && out.length < 64; i++) {
    const c: string = text.charAt(i);
    out += /[A-Za-z0-9._:@-]/.test(c) ? c : '_';
  }
  return out;
}

// The frozen I destination from the armed ticket's `outer_destinations`
// (`udp/<ip>:<port>` entries): port 3478 first (STUN), any other udp entry as
// the fallback, '' when there is none.
export function selectN2SandwichTarget(outerDestinations: string[]): string {
  let fallback: string = '';
  for (let i = 0; i < outerDestinations.length; i++) {
    const entry: string = outerDestinations[i];
    const slash: number = entry.indexOf('/');
    if (slash <= 0) {
      continue;
    }
    if (entry.substring(0, slash) !== 'udp') {
      continue;
    }
    const target: string = entry.substring(slash + 1);
    if (target.length === 0) {
      continue;
    }
    if (fallback.length === 0) {
      fallback = target;
    }
    const colon: number = target.lastIndexOf(':');
    if (colon > 0 && target.substring(colon + 1) === '3478') {
      return target;
    }
  }
  return fallback;
}

// One leg's control marker: `n2s-<kind>-<requestId>-<phase>`, sanitized to the
// marker rule and truncated so the whole id stays ≤ 64 characters. '' when the
// fixed parts alone cannot fit (the caller then reports a named refusal).
export function formatN2SandwichLegMarker(kind: string, requestId: string, phase: string): string {
  const head: string = `n2s-${kind}-`;
  const tail: string = `-${phase}`;
  const room: number = 64 - head.length - tail.length;
  if (room < 1) {
    return '';
  }
  let body: string = '';
  for (let i = 0; i < requestId.length && body.length < room; i++) {
    const c: string = requestId.charAt(i);
    body += /[A-Za-z0-9_-]/.test(c) ? c : '-';
  }
  return `${head}${body}${tail}`;
}

// The core's `n2_sandwich_poll` answer: `{"ok":true,"posts":[{"kind":"I",
// "marker":"…"}]}`. Only well-formed I/Ws legs with a valid marker are kept.
export function parseN2SandwichPoll(raw: string): N2SandwichPost[] {
  if (raw.length === 0) {
    return [];
  }
  let doc: Record<string, Object> | null = null;
  try {
    doc = JSON.parse(raw) as Record<string, Object>;
  } catch (error) {
    return [];
  }
  if (doc === null || typeof doc !== 'object' || Array.isArray(doc)) {
    return [];
  }
  const rawPosts: Object | undefined = doc['posts'];
  if (!Array.isArray(rawPosts)) {
    return [];
  }
  const posts: N2SandwichPost[] = [];
  for (const item of rawPosts as Object[]) {
    if (item === null || typeof item !== 'object' || Array.isArray(item)) {
      continue;
    }
    const record: Record<string, Object> = item as Record<string, Object>;
    const kind: Object | undefined = record['kind'];
    const marker: Object | undefined = record['marker'];
    if (typeof kind !== 'string' || (kind !== 'I' && kind !== 'Ws')) {
      continue;
    }
    if (typeof marker !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/.test(marker)) {
      continue;
    }
    posts.push({ kind: kind, marker: marker });
  }
  return posts;
}

// The shell's own delivery receipt for one orchestration step. The core's
// `N2_SANDWICH|…` line is authoritative for the ordering verdict.
export function formatN2SandwichShellLine(kind: string, phase: string, marker: string,
  target: string, result: string, reason: string): string {
  return `N2_SANDWICH_SHELL|kind=${cleanN2SandwichToken(kind)}` +
    `|phase=${cleanN2SandwichToken(phase)}|marker=${cleanN2SandwichToken(marker)}` +
    `|target=${cleanN2SandwichToken(target)}|result=${cleanN2SandwichToken(result)}` +
    `|reason=${cleanN2SandwichToken(reason)}|diag=true|is_evidence=false`;
}
