// N2 P4/P5 — the A-column trigger private channel and the automatic I/Ws
// sandwich — shell self-check (host-only, no device, no network, no build).
//
// Two layers, the same principle as run-n2-control.mts / run-n2-stimulus.mts:
//   * the REAL pure modules (NetBirdN2AEvent.ets, NetBirdN2Sandwich.ets,
//     copied byte-for-byte into temp fixtures) are exercised as CODE — the
//     parsers, the target/marker rules and every receipt formatter;
//   * the integration is pinned on the byte-identical shipped sources, because
//     the armed-only channels span EntryAbility (writes the request file), the
//     extension (polls + drives the sandwich) and the Rust core (the
//     authoritative `N2_A_EVENT`/`N2_SANDWICH` lines) and none of those needs
//     a device.
//
// The behavioural drive of the REAL `pollN2AEvent` / `pollN2Sandwich` and of
// the EntryAbility three-way combination rule lives in
// run-exclusion-generation.mts (N group), which stages the REAL extension and
// the REAL EntryAbility against the path-aware fake fs.
//
// Run: node client/entry/selfcheck/run-n2-a-event.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ETS = join(HERE, '..', 'src', 'main', 'ets');
const REAL_A = join(ETS, 'vpnextensionability', 'NetBirdN2AEvent.ets');
const REAL_S = join(ETS, 'vpnextensionability', 'NetBirdN2Sandwich.ets');
const ABILITY = join(ETS, 'vpnextensionability', 'NetBirdVpnExtensionAbility.ets');
const CONTROL = join(ETS, 'vpnextensionability', 'NetBirdN2Control.ets');
const ENTRY = join(ETS, 'entryability', 'EntryAbility.ets');
const CORE_SRC = join(HERE, '..', '..', 'core', 'src');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-n2-a-event-'));
const fixtureA: string = join(fixtureDir, 'NetBirdN2AEvent.ts');
const fixtureS: string = join(fixtureDir, 'NetBirdN2Sandwich.ts');
writeFileSync(fixtureA, readFileSync(REAL_A)); // byte-identical copy
writeFileSync(fixtureS, readFileSync(REAL_S)); // byte-identical copy

let passed: number = 0;
let failed: number = 0;

function check(name: string, condition: boolean, detail?: string): void {
  if (condition) {
    passed++;
    console.log(`ok   ${name}`);
  } else {
    failed++;
    console.log(`FAIL ${name}${detail === undefined ? '' : ` — ${detail}`}`);
  }
}

const readText = (path: string): string => readFileSync(path, 'utf8');

try {
  const a = await import(pathToFileURL(fixtureA).href) as Record<string, unknown>;
  const s = await import(pathToFileURL(fixtureS).href) as Record<string, unknown>;

  const parseA = a['parseN2AEvent'] as (text: string) =>
    { request?: { kind: string; id: string }; error: string };
  const parseAReply = a['parseN2AEventReply'] as (raw: string) => Record<string, unknown>;
  const aShellLine = a['formatN2AEventShellLine'] as
    (requestId: string, kind: string, id: string, reply: Record<string, unknown>) => string;
  const aShellReject = a['formatN2AEventShellReject'] as (requestId: string, reason: string) => string;
  const aFileName = a['N2_A_EVENT_FILE_NAME'] as string;

  const selectTarget = s['selectN2SandwichTarget'] as (entries: string[]) => string;
  const legMarker = s['formatN2SandwichLegMarker'] as
    (kind: string, requestId: string, phase: string) => string;
  const parsePoll = s['parseN2SandwichPoll'] as (raw: string) =>
    { kind: string; marker: string }[];
  const sShellLine = s['formatN2SandwichShellLine'] as
    (kind: string, phase: string, marker: string, target: string, result: string,
      reason: string) => string;

  const ability = readText(ABILITY);
  const control = readText(CONTROL);
  const entry = readText(ENTRY);
  const rust = (name: string): string => readText(join(CORE_SRC, name));

  // ---------------- P: the A-event pure module ----------------------------
  check('P1 the A-event request basename is the documented literal',
    aFileName === 'netbird-n2-a-event', aFileName);
  check('P1 the accepted kinds are exactly the four frozen ones',
    JSON.stringify(a['N2_A_EVENT_KINDS']) === JSON.stringify(['M', 'R', 'D', 'S']),
    JSON.stringify(a['N2_A_EVENT_KINDS']));

  for (const kind of ['M', 'R', 'D', 'S']) {
    const decision = parseA(`{"kind":"${kind}","id":"a-${kind}-1"}`);
    check(`P2 ${kind} is accepted with its request`,
      decision.error === '' && decision.request?.kind === kind &&
      decision.request?.id === `a-${kind}-1`, JSON.stringify(decision));
  }
  check('P3 a trailing newline (the private-channel write shape) is harmless',
    parseA('{"kind":"M","id":"a-M-1"}\n').error === '');
  check('P4 surrounding whitespace is trimmed',
    parseA('  {"kind":"D","id":"a-D-1"}  ').request?.kind === 'D');

  for (const [label, text, error] of [
    ['empty', '', 'empty-request'],
    ['whitespace only', '  \n ', 'empty-request'],
    ['not JSON', '{kind:M}', 'invalid-json'],
    ['JSON null', 'null', 'invalid-request'],
    ['JSON array', '[{"kind":"M","id":"x"}]', 'invalid-request'],
    ['JSON string', '"M"', 'invalid-request'],
    ['missing kind', '{"id":"x"}', 'invalid-kind'],
    ['non-string kind', '{"kind":7,"id":"x"}', 'invalid-kind'],
    ['unknown kind', '{"kind":"W","id":"x"}', 'invalid-kind'],
    ['lowercase kind', '{"kind":"m","id":"x"}', 'invalid-kind'],
    ['missing id', '{"kind":"M"}', 'invalid-id'],
    ['non-string id', '{"kind":"M","id":7}', 'invalid-id'],
    ['empty id', '{"kind":"M","id":""}', 'invalid-id'],
    ['id with a separator', '{"kind":"M","id":"a|b"}', 'invalid-id'],
    ['id with a dot', '{"kind":"M","id":"a.b"}', 'invalid-id'],
    ['id too long', `{"kind":"M","id":"${'x'.repeat(65)}"}`, 'invalid-id'],
    ['oversized request', `{"kind":"M","id":"x","pad":"${'y'.repeat(300)}"}`, 'request-too-long']
  ] as const) {
    const decision = parseA(text);
    check(`P5 refuses ${label} (${error})`,
      decision.error === error && decision.request === undefined,
      `${decision.error} ${JSON.stringify(decision.request)}`);
  }

  // The core's answer is parsed honestly; a malformed body is a rejection.
  const goodReply = parseAReply(JSON.stringify({ ok: true, result: 'sent', reason: 'ok',
    kind: 'M', fd: 21, dup_fd: 22, line: 'N2_A_EVENT|x' }));
  check('P6 a well-formed core answer keeps every field',
    goodReply['ok'] === true && goodReply['result'] === 'sent' && goodReply['fd'] === 21 &&
    goodReply['dup_fd'] === 22);
  for (const bad of ['', 'not json', 'null', '[1]', '"x"']) {
    const reply = parseAReply(bad);
    check(`P6 a malformed core answer (${JSON.stringify(bad)}) is a rejection`,
      reply['ok'] === false && reply['result'] === 'rejected' && reply['reason'] === 'invalid-reply');
  }

  // The shell receipts.
  check('P7 the delivered-request line carries the parsed answer and stays diagnostic',
    aShellLine('req-1', 'M', 'a-M-1', goodReply) ===
      'N2_A_EVENT_SHELL|requestId=req-1|id=a-M-1|kind=M|core_ok=true|core_result=sent|' +
      'reason=ok|fd=21|dup_fd=22|diag=true|is_evidence=false',
    aShellLine('req-1', 'M', 'a-M-1', goodReply));
  check('P7 a rejected delivery is never reported as a dispatch',
    aShellLine('req-2', 'S', 'a-S-1',
      { ok: false, result: 'error', reason: 'no-live-signal-connection',
        fd: -1, dup_fd: -1 }) ===
      'N2_A_EVENT_SHELL|requestId=req-2|id=a-S-1|kind=S|core_ok=false|' +
      'core_result=error|reason=no-live-signal-connection|fd=-1|dup_fd=-1|' +
      'diag=true|is_evidence=false');
  check('P8 the rejection line names the reason and invents no result',
    aShellReject('req-3', 'invalid-kind') ===
      'N2_A_EVENT_SHELL|requestId=req-3|id=none|kind=none|core_ok=false|core_result=rejected|' +
      'reason=invalid-kind|fd=-1|dup_fd=-1|diag=true|is_evidence=false',
    aShellReject('req-3', 'invalid-kind'));
  check('P9 the shell line never lets pipeline characters through',
    !aShellLine('req|1', 'M', 'a|M', { ok: true, result: 'sent', reason: 'o|k' }).includes('|1|') &&
    aShellLine('req|1', 'M', 'a|M', { ok: true, result: 'sent', reason: 'o|k' })
      .includes('requestId=req_1'));

  // ---------------- Q: the sandwich pure module ---------------------------
  check('Q1 the frozen I destination (udp, port 3478) wins over an earlier udp entry',
    selectTarget(['udp/192.0.2.10:53', 'tcp/192.0.2.11:443', 'udp/192.0.2.9:3478']) ===
      '192.0.2.9:3478', selectTarget(['udp/192.0.2.10:53', 'udp/192.0.2.9:3478']));
  check('Q1 without a 3478 entry the first udp destination is the fallback',
    selectTarget(['tcp/192.0.2.11:443', 'udp/192.0.2.10:53', 'udp/192.0.2.12:51820']) ===
      '192.0.2.10:53');
  check('Q1 a tcp-only list has no I destination (never an invented target)',
    selectTarget(['tcp/192.0.2.11:443']) === '' && selectTarget([]) === '',
    selectTarget(['tcp/192.0.2.11:443']));

  const marker = legMarker('I', 'req-i3a', 'pre');
  check('Q2 the leg marker is the documented shape and satisfies the core marker rule',
    marker === 'n2s-I-req-i3a-pre' && /^[A-Za-z0-9_-]{1,64}$/.test(marker), marker);
  check('Q2 the pre and post markers of one event differ',
    legMarker('Ws', 'req-i3a', 'post') === 'n2s-Ws-req-i3a-post' &&
    legMarker('Ws', 'req-i3a', 'post') !== legMarker('Ws', 'req-i3a', 'pre'));
  const longMarker = legMarker('I', 'x'.repeat(200), 'pre');
  check('Q2 an oversized requestId is truncated INSIDE the marker rule',
    /^[A-Za-z0-9_-]{1,64}$/.test(longMarker) && longMarker.startsWith('n2s-I-') &&
    longMarker.endsWith('-pre') && longMarker.length <= 64, String(longMarker.length));
  check('Q2 characters outside the marker rule are replaced, never emitted raw',
    /^[A-Za-z0-9_-]{1,64}$/.test(legMarker('I', 'a|b c.d', 'pre')));

  const posts = parsePoll(JSON.stringify({ ok: true,
    posts: [{ kind: 'I', marker: 'n2s-I-pre' }, { kind: 'Ws', marker: 'n2s-Ws-post' }] }));
  check('Q3 a well-formed poll answer yields the ready legs',
    posts.length === 2 && posts[0].kind === 'I' && posts[1].marker === 'n2s-Ws-post',
    JSON.stringify(posts));
  check('Q3 malformed poll answers yield NO legs (never a guessed one)',
    parsePoll('').length === 0 && parsePoll('not json').length === 0 &&
    parsePoll('null').length === 0 && parsePoll('{"ok":true}').length === 0 &&
    parsePoll('{"posts":"I"}').length === 0);
  check('Q3 an unknown kind or an invalid marker is dropped',
    parsePoll(JSON.stringify({ posts: [{ kind: 'Wb', marker: 'x' },
      { kind: 'I', marker: 'bad marker' }, { kind: 'I', marker: 'good-1' }] })).length === 1);

  check('Q4 the sandwich shell receipt is fixed-shape and diagnostic',
    sShellLine('I', 'post', 'n2s-I-pre', '192.0.2.9:3478', 'observed', 'post-control-hit') ===
      'N2_SANDWICH_SHELL|kind=I|phase=post|marker=n2s-I-pre|target=192.0.2.9:3478|' +
      'result=observed|reason=post-control-hit|diag=true|is_evidence=false',
    sShellLine('I', 'post', 'n2s-I-pre', '192.0.2.9:3478', 'observed', 'post-control-hit'));

  // ---------------- S: the shipped wiring --------------------------------
  check('S1 the extension imports both pure modules and the control leg primitive',
    ability.includes("} from './NetBirdN2AEvent';") &&
    ability.includes("} from './NetBirdN2Sandwich';") &&
    ability.includes('sendAndWaitN2Control'),
    'import block');
  check('S2 the I pre control runs BEFORE the first ICE pool feed (and after attach)',
    ability.indexOf('await this.n2IceSandwichPre();') > 0 &&
    ability.indexOf('await this.n2IceSandwichPre();') <
      ability.indexOf('const iceOk: boolean = await this.protectAndFeedIceSocket();'),
    'I ordering');
  check('S2 the Ws pre control runs AFTER the ICE pool feed, before the watcher starts',
    ability.indexOf('await this.n2WsSandwichPre();') >
      ability.indexOf('const iceOk: boolean = await this.protectAndFeedIceSocket();') &&
    ability.indexOf('await this.n2WsSandwichPre();') <
      ability.indexOf('this.startStatusAndNetcfgWatcher();'),
    'Ws ordering');
  check('S3 both pre steps are gated on the accepted evidence arm',
    ability.includes('    if (this.n2Evidence !== undefined) {\n' +
      '      await this.n2IceSandwichPre();\n    }') &&
    ability.includes('    if (this.n2Evidence !== undefined) {\n' +
      '      await this.n2WsSandwichPre();\n    }'),
    'armed gates');
  check('S4 the tick polls the A-event channel and the sandwich, and keeps the control/stimulus order',
    ability.includes('      this.pollN2AEvent();') &&
    ability.includes('      this.pollN2Sandwich();') &&
    ability.indexOf('      this.pollN2AEvent();') >
      ability.indexOf('      this.maybeCloseInterfaceStatsWindow();') &&
    ability.indexOf('      this.pollN2Control();') >
      ability.indexOf('      this.pollN2AEvent();') &&
    ability.indexOf('      this.pollN2Stimulus();') >
      ability.indexOf('      this.pollN2Control();'),
    'tick');
  check('S5 the A-event channel has ONE core call and an armed guard before it',
    (ability.match(/core\.n2_a_event\(/g) ?? []).length === 1 &&
    ability.includes('core.n2_a_event(request.kind, request.id)') &&
    ability.includes('      if (this.n2Evidence === undefined) {'),
    String((ability.match(/core\.n2_a_event\(/g) ?? []).length));
  check('S5 a thrown core call is a rejection with its own reason, never a dispatch',
    ability.includes('reply = parseN2AEventReply(core.n2_a_event(request.kind, request.id));') &&
    ability.includes("reply = { ok: false, result: 'rejected', reason: 'core-throw' };"));
  check('S6 the request file is consumed exactly once, in a finally',
    ability.includes('    } finally {\n' +
      '      // consume exactly once, whatever happened above\n' +
      '      try {\n' +
      '        fs.unlinkSync(path);'),
    'finally');
  check('S7 the pre leg registers with the core BEFORE sending its control',
    ability.indexOf('core.n2_sandwich_pre(kind, marker)') > 0 &&
    ability.indexOf('core.n2_sandwich_pre(kind, marker)') <
      ability.indexOf("await sendAndWaitN2Control(armed, target, 'udp', marker)"),
    'pre order');
  check('S7 a pre miss closes the leg honestly so no send can be reported as sandwiched',
    ability.includes("core.n2_sandwich_post(kind, marker, 'unsat', 'pre-control-miss');"));
  check('S8 the Ws path never defers or reorders the egress/endpoint landing',
    !ability.includes('apply_endpoint') && !ability.includes('attach_egress_socket') &&
    rust('peer_conn.rs').includes('Some(fd) => self.wg.attach_egress_socket(&peer.key, fd),') &&
    rust('peer_conn.rs').includes('Ok(()) => self.wg.apply_endpoint(&peer.key, addr, remote.port),') &&
    rust('peer_conn.rs').indexOf('attach_egress_socket(&peer.key, fd)') <
      rust('peer_conn.rs').indexOf('apply_endpoint(&peer.key, addr, remote.port)'),
    'egress-then-endpoint');

  check('S9 the control module exports the bounded leg primitive the sandwich uses',
    control.includes('export async function sendAndWaitN2Control(') &&
    control.includes('await waitN2ControlHit(id);') &&
    control.includes('await sendAndWaitN2Control(armed, target, protocol, pre);') &&
    control.includes('await sendAndWaitN2Control(armed, target, protocol, post);'));

  check('S10 EntryAbility carries the parameter and the shared basename',
    entry.includes("const N2_A_EVENT_PARAM: string = 'n2_a_event';") &&
    entry.includes("const N2_A_EVENT_FILE: string = 'netbird-n2-a-event';"),
    'entry constants');
  check('S11 an explicit parameter writes the request; every other launch clears a leftover',
    entry.includes('params[N2_A_EVENT_PARAM];') &&
    entry.includes('this.writeAEventFile(aEventText);') &&
    entry.includes("this.removeAEventFile('default-no-a-event');") &&
    entry.includes("this.removeAEventFile('combo-rejected');") &&
    entry.includes("this.removeAEventFile('failed-write-cleanup');"));
  check('S12 the combination rule spans THREE channels and keeps the legacy two-way line',
    entry.includes('if (stimulusText.length > 0 && controlText.length > 0) {') &&
    entry.includes("} else if (stimulusText.length > 0 && aEventText.length > 0) {") &&
    entry.includes("} else if (controlText.length > 0 && aEventText.length > 0) {") &&
    entry.includes("'a_event=%{public}s|note=one-pending-request-per-launch|diag=true|is_evidence=false'") &&
    entry.includes("'note=one-pending-request-per-launch|diag=true|is_evidence=false',\n" +
      '          N2_STIMULUS_FILE, N2_CONTROL_FILE);'),
    'combo rule');
  check('S13 the write is honest about a failed/short write (no usable request left behind)',
    entry.includes('N2_A_EVENT_FILE_WRITE_FAILED') &&
    entry.includes('short-write:${written}of${expectedBytes}') &&
    entry.includes('N2_A_EVENT_FILE_CLOSE_FAILED'));
  check('S14 the payload is never logged (file name + UTF-8 byte count only)',
    entry.includes('N2_A_EVENT_FILE, expectedBytes);') &&
    entry.includes('N2_A_EVENT_FILE, failureCode);') &&
    !entry.includes('hilog.warn(DOMAIN, TAG, text') &&
    !entry.includes('hilog.info(DOMAIN, TAG, text'));
  check('S15 the arm-ticket / stimulus / control channels are untouched',
    entry.includes("const N2H_ARM_OPTIONS_PARAM: string = 'n2h_arm_options';") &&
    entry.includes("const N2_STIMULUS_PARAM: string = 'n2_stimulus';") &&
    entry.includes("const N2_CONTROL_PARAM: string = 'n2_control';") &&
    entry.includes("this.removeArmTicket('default-no-arm');") &&
    entry.includes("this.removeStimulusFile('default-no-stimulus');") &&
    entry.includes("this.removeControlFile('default-no-control');"));

  // ---------------- H: the loader mocks stay in sync ---------------------
  for (const hookName of ['exclusion-generation-hooks.mjs', 'relay-exclusion-hooks.mjs']) {
    const hook = readText(join(HERE, hookName));
    const recording = hookName === 'exclusion-generation-hooks.mjs';
    check(`H1 ${hookName} stubs the A-column trigger as a ${recording ? 'recording' : 'loud-refusal'} seam`,
      hook.includes('n2_a_event:') && hook.includes('"not-armed"') &&
      (!recording || (hook.includes('__NB_A_EVENT_LAST') && hook.includes('__NB_A_EVENT_CALLS'))),
      hookName);
    check(`H2 ${hookName} stubs every sandwich seam and the control leg primitive`,
      hook.includes('n2_sandwich_pre:') && hook.includes('n2_sandwich_post:') &&
      hook.includes('n2_sandwich_poll:') && hook.includes('sendAndWaitN2Control'),
      hookName);
    check(`H3 ${hookName} resolves both new pure modules to their staged shims`,
      hook.includes("'./NetBirdN2AEvent': 'NetBirdN2AEvent.shim.ts'") &&
      hook.includes("'./NetBirdN2Sandwich': 'NetBirdN2Sandwich.shim.ts'"),
      hookName);
  }
  check('H4 both staging harnesses stage the new modules and their erased types',
    readText(join(HERE, 'run-exclusion-generation.mts')).includes("'NetBirdN2AEvent',\n  'NetBirdN2Sandwich',") &&
    readText(join(HERE, 'run-relay-exclusion.mts')).includes("'NetBirdN2AEvent',\n  'NetBirdN2Sandwich',") &&
    readText(join(HERE, 'run-exclusion-generation.mts')).includes(
      "NetBirdN2AEvent: ['N2AEventDecision', 'N2AEventRequest', 'N2AEventReply']") &&
    readText(join(HERE, 'run-relay-exclusion.mts')).includes(
      "NetBirdN2Sandwich: ['N2SandwichPost']"));

  // ---------------- R: the core surface ----------------------------------
  const napi = rust('napi.rs');
  const aEvent = rust('n2_a_event.rs');
  const sandwich = rust('n2_sandwich.rs');
  const trace = rust('n2_socket_trace.rs');
  const wg = rust('wg_device.rs');
  const relay = rust('relay_client.rs');
  const connector = rust('connector.rs');
  const signal = rust('signal.rs');

  check('R1 the four new NAPI exports are registered and reach their modules',
    napi.includes('reg!("n2_a_event", napi_n2_a_event);') &&
    napi.includes('reg!("n2_sandwich_pre", napi_n2_sandwich_pre);') &&
    napi.includes('reg!("n2_sandwich_post", napi_n2_sandwich_post);') &&
    napi.includes('reg!("n2_sandwich_poll", napi_n2_sandwich_poll);') &&
    napi.includes('crate::connector::n2_a_event_json(&a.string_at(0), &a.string_at(1))') &&
    napi.includes('crate::connector::n2_sandwich_poll_json()'));
  check('R2 the A-event line carries the fd/dup pair and the frozen tail',
    aEvent.includes('"N2_A_EVENT|id={}|kind={}|fd={}|dup_fd={}|remote={}|result={}|reason={}|detail={}|' +
      'mono_ms={}|diag=true|is_evidence=false"'),
    'line');
  check('R3 S triggers the production watchdog probe and D refuses after a prior stimulus (Q1)',
    aEvent.includes('fn signal_probe(&self) -> Result<(), String>;') &&
    aEvent.includes('"self-addressed-heartbeat"') &&
    aEvent.includes('"no-live-signal-connection"') &&
    aEvent.includes('pub const S_PROBE_WAIT_MS: u64 = 5_000;') &&
    aEvent.includes('"prior-stimulus"') && aEvent.includes('n2_stimulus::applied() > 0'),
    'S trigger + Q1');
  check('R3b the connector S seam asks the registered signal watchdog for ONE heartbeat, bounded',
    connector.includes('crate::signal::request_receive_probe()') &&
    connector.includes('crate::n2_a_event::S_PROBE_WAIT_MS') &&
    connector.includes('"signal-heartbeat-timeout"') &&
    !connector.includes('signal.send_heartbeat') &&
    !connector.includes('SignalClient::connect_with_socket_source'),
    'S seam');
  check('R3c the receive watchdog is the product default (20 s probe / 30 s dead) on the SAME stream',
    signal.includes('pub const RECEIVE_PROBE_AFTER: core::time::Duration = core::time::Duration::from_secs(20);') &&
    signal.includes('pub const RECEIVE_DEAD_AFTER: core::time::Duration = core::time::Duration::from_secs(30);') &&
    signal.includes('probe_after: RECEIVE_PROBE_AFTER,') &&
    signal.includes('dead_after: RECEIVE_DEAD_AFTER,') &&
    signal.includes('self.client.send_heartbeat().await') &&
    signal.includes('pub async fn request_receive_probe() -> Result<(), String>'),
    'watchdog');
  check('R4 the R wait is the frozen bound and the trigger goes through the connector seam',
    aEvent.includes('pub const R_WAIT_MS: u64 = 90_000;') &&
    aEvent.includes('fn management_round_trip(&self) -> Result<(), String>;') &&
    aEvent.includes('fn arm_healthcheck_watch(&self, id: &str, wait_ms: u64, seq: u64) -> Result<(), String>;'));
  check('R4b the R timeout timer is keyed on (id, seq), so a stale timer cannot complete a later watch',
    aEvent.includes('pub fn r_watch_timeout(id: &str, seq: u64) -> Option<Outcome>') &&
    connector.includes('crate::n2_a_event::r_watch_timeout(&id, seq)'));
  check('R5 M uses the EXISTING logged-in client (no new dial, no second Sync stream)',
    connector.includes('handle.logout_slot.lock_poison().clone()') &&
    connector.includes('client.get_server_key()') &&
    (() => {
      const at = connector.indexOf('impl crate::n2_a_event::Trigger for ConnectorATrigger');
      if (at < 0) { return false; }
      const body = connector.slice(at, connector.indexOf('pub fn n2_a_event_json'));
      return !body.includes('ManagementGrpcClient::connect') &&
        !body.includes('SyncSession') && !body.includes('factory.connect');
    })(),
    'existing connection');
  check('R5 the M round trip is bounded and its error token is sanitized',
    connector.includes('crate::n2_a_event::M_RPC_WAIT_MS') &&
    connector.includes('"management-rpc-{}", ErrorClass::from_management(&error).as_str()') &&
    connector.includes('force_protected_dns_requery(&handle)'));
  check('R6 the relay reader reports the server-driven HealthCheck answer',
    relay.includes('let _ = crate::n2_a_event::healthcheck_answered();') &&
    aEvent.includes('pub fn healthcheck_answered()'));
  check('R7 the sandwich line is the documented literal',
    sandwich.includes('"N2_SANDWICH|kind={}|phase={}|marker={}|event_ref={}|result={}|reason={}|' +
      'fd={fd}|dup_fd={dup_fd}|remote={}|wait_ms={wait_ms}|mono_ms={}|diag=true|is_evidence=false"'),
    'line');
  check('R8 the frozen unsat token and the two documented wait bounds exist',
    sandwich.includes('"first-send-before-attach"') &&
    sandwich.includes('pub const PRE_WAIT_MS: u64 = 3_000;') &&
    sandwich.includes('pub const POST_WAIT_MS: u64 = 90_000;'));
  check('R9 the attach anchor is recorded at the real tee attach and the HIT at the tee scan',
    wg.includes('crate::n2_sandwich::attach_anchor(sys::mono_ms());') &&
    wg.includes('crate::n2_sandwich::control_hit(marker, now);') &&
    wg.indexOf('attach_n2_dest_tee(tee.clone())') <
      wg.indexOf('crate::n2_sandwich::attach_anchor(sys::mono_ms());'),
    'anchor');
  check('R10 the first I/Ws send is observed at the single N2_SOCK call site',
    trace.includes('crate::n2_sandwich::observe_io(&meta.kind, fd, meta.original_fd, remote, status);') &&
    trace.includes('pub fn live_of(kind: &str)') &&
    trace.includes('crate::n2_a_event::reset();') &&
    trace.includes('crate::n2_sandwich::reset();'),
    'hooks');
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
