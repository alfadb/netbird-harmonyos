// N2 T5a「B 断流刺激」shell self-check (host-only, no device, no network, no
// build).
//
// Two layers, same principle as run-n2-evidence.mts / run-n2-socket-trace.mts:
//   * the REAL pure module (client/entry/src/main/ets/vpnextensionability/
//     NetBirdN2Stimulus.ets, copied byte-for-byte into a temp fixture) is
//     exercised as CODE — the parser and the two receipt formatters;
//   * the integration is pinned on the byte-identical shipped sources, because
//     the armed-only private channel spans three files (EntryAbility writes the
//     request file, the extension polls + calls the core, the core emits the
//     authoritative `N2_STIMULUS|…` line) and none of those needs a device.
//
// Run: node client/entry/selfcheck/run-n2-stimulus.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ETS = join(HERE, '..', 'src', 'main', 'ets');
const REAL = join(ETS, 'vpnextensionability', 'NetBirdN2Stimulus.ets');
const ABILITY = join(ETS, 'vpnextensionability', 'NetBirdVpnExtensionAbility.ets');
const ENTRY = join(ETS, 'entryability', 'EntryAbility.ets');
const CORE_SRC = join(HERE, '..', '..', 'core', 'src');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-n2-stimulus-'));
const fixture: string = join(fixtureDir, 'NetBirdN2Stimulus.ts');
writeFileSync(fixture, readFileSync(REAL)); // byte-identical copy of the real module

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
  const mod = await import(pathToFileURL(fixture).href) as Record<string, unknown>;
  const parse = mod['parseN2Stimulus'] as (text: string) => { request?: { kind: string; id: string }; error: string };
  const parseReply = mod['parseN2StimulusReply'] as (raw: string) => {
    ok: boolean; result?: string; reason?: string; target_fd?: number; remote?: string; line?: string;
  };
  const shellLine = mod['formatN2StimulusShellLine'] as
    (requestId: string, kind: string, id: string, reply: object) => string;
  const shellReject = mod['formatN2StimulusShellReject'] as (requestId: string, reason: string) => string;
  const kinds = mod['N2_STIMULUS_KINDS'] as string[];
  const fileName = mod['N2_STIMULUS_FILE_NAME'] as string;

  const ability = readText(ABILITY);
  const entry = readText(ENTRY);
  const rust = (name: string): string => readText(join(CORE_SRC, name));

  // ---------------- P: the pure module ------------------------------------
  check('P1 the class list is the frozen seven, in order',
    JSON.stringify(kinds) === JSON.stringify(['M', 'S', 'R', 'I', 'Wb', 'Ws', 'D']),
    JSON.stringify(kinds));
  check('P2 the request basename is the documented literal',
    fileName === 'netbird-n2-stimulus', fileName);

  for (const kind of ['M', 'S', 'R', 'I', 'Wb', 'Ws', 'D']) {
    const decision = parse(`{"kind":"${kind}","id":"b-${kind}-1"}`);
    check(`P3 ${kind} is accepted with its request`,
      decision.error === '' && decision.request?.kind === kind && decision.request?.id === `b-${kind}-1`,
      JSON.stringify(decision));
  }
  check('P4 a trailing newline (the private-channel write shape) is harmless',
    parse('{"kind":"M","id":"b-M-1"}\n').error === '');
  check('P5 surrounding whitespace is trimmed',
    parse('  {"kind":"Ws","id":"x"}  ').request?.kind === 'Ws');

  for (const [label, text, error] of [
    ['empty', '', 'empty-request'],
    ['whitespace only', '   \n ', 'empty-request'],
    ['not JSON', '{kind:M}', 'invalid-json'],
    ['JSON null', 'null', 'invalid-request'],
    ['JSON array', '[{"kind":"M","id":"x"}]', 'invalid-request'],
    ['JSON string', '"M"', 'invalid-request'],
    ['missing kind', '{"id":"x"}', 'invalid-kind'],
    ['non-string kind', '{"kind":7,"id":"x"}', 'invalid-kind'],
    ['unknown kind W', '{"kind":"W","id":"x"}', 'invalid-kind'],
    ['lowercase kind', '{"kind":"m","id":"x"}', 'invalid-kind'],
    ['missing id', '{"kind":"M"}', 'invalid-id'],
    ['non-string id', '{"kind":"M","id":7}', 'invalid-id'],
    ['empty id', '{"kind":"M","id":""}', 'invalid-id'],
    ['id with a separator', '{"kind":"M","id":"a|b"}', 'invalid-id'],
    ['id with a space', '{"kind":"M","id":"a b"}', 'invalid-id'],
    ['id too long', `{"kind":"M","id":"${'x'.repeat(65)}"}`, 'invalid-id'],
    ['oversized request', `{"kind":"M","id":"x","pad":"${'y'.repeat(300)}"}`, 'request-too-long']
  ] as const) {
    const decision = parse(text);
    check(`P6 refuses ${label} (${error})`,
      decision.error === error && decision.request === undefined,
      `${decision.error} ${JSON.stringify(decision.request)}`);
  }

  check('P7 a valid request carries no error',
    parse('{"kind":"R","id":"b-R-3"}').error === '');

  // The core's reply shape.
  const goodReply = parseReply('{"ok":true,"result":"applied","reason":"shutdown-socket","target_fd":31,"remote":"203.0.113.7:443","line":"N2_STIMULUS|id=x"}');
  check('P8 the core reply is parsed field by field',
    goodReply.ok === true && goodReply.result === 'applied' &&
    goodReply.reason === 'shutdown-socket' && goodReply.target_fd === 31 &&
    goodReply.remote === '203.0.113.7:443', JSON.stringify(goodReply));
  for (const [label, raw] of [['empty', ''], ['garbage', 'not json'], ['JSON null', 'null'],
    ['JSON array', '[1]']] as const) {
    const reply = parseReply(raw);
    check(`P9 a ${label} reply is a rejection, never an acceptance`,
      reply.ok === false && reply.result === 'rejected' && reply.reason === 'invalid-reply',
      JSON.stringify(reply));
  }
  const bare = parseReply('{"ok":false}');
  check('P10 a bare reply gets honest defaults',
    bare.ok === false && bare.result === 'rejected' && bare.reason === 'none' &&
    bare.target_fd === -1 && bare.remote === 'unknown', JSON.stringify(bare));

  // The shell receipts.
  const line = shellLine('req-1', 'M', 'b-M-1', goodReply);
  check('P11 the shell line is fixed-shape and diagnostic',
    line === 'N2_STIMULUS_SHELL|requestId=req-1|id=b-M-1|kind=M|core_ok=true|' +
      'core_result=applied|reason=shutdown-socket|target_fd=31|diag=true|is_evidence=false',
    line);
  check('P12 the shell line never lets pipeline characters through',
    !shellLine('req|1', 'M', 'b|M', goodReply).includes('|1') &&
    shellLine('req|1', 'M', 'b|M', goodReply).includes('requestId=req_1') &&
    shellLine('req|1', 'M', 'b|M', goodReply).includes('|id=b_M|'),
    shellLine('req|1', 'M', 'b|M', goodReply));
  check('P13 the rejection line names the reason and stays diagnostic',
    shellReject('req-2', 'invalid-kind') ===
      'N2_STIMULUS_SHELL|requestId=req-2|id=none|kind=none|core_ok=false|' +
      'core_result=rejected|reason=invalid-kind|target_fd=-1|diag=true|is_evidence=false',
    shellReject('req-2', 'invalid-kind'));

  // ---------------- S: the shipped wiring --------------------------------
  check('S1 the extension imports the pure module and its erased shapes',
    ability.includes("} from './NetBirdN2Stimulus';") &&
    ability.includes('N2StimulusDecision') && ability.includes('N2StimulusReply') &&
    ability.includes('parseN2Stimulus,') && ability.includes('parseN2StimulusReply') &&
    ability.includes('formatN2StimulusShellLine') && ability.includes('formatN2StimulusShellReject'),
    'import block');
  check('S2 the armed guard is the FIRST line of the entry (unarmed never probes the file)',
    ability.includes('private pollN2Stimulus(): void {\n' +
      '    if (this.n2Evidence === undefined || this.destroyRequested) {\n' +
      '      return;\n' +
      '    }'),
    'guard');
  check('S3 the request path is the shared filesDir basename',
    ability.includes('const path: string = `${this.context.filesDir}/${N2_STIMULUS_FILE_NAME}`;'));
  check('S4 the ONE core call per request, with the parsed kind and id',
    (ability.match(/core\.n2_stimulus\(/g) ?? []).length === 1 &&
    ability.includes('reply = parseN2StimulusReply(core.n2_stimulus(kind, id));'),
    String((ability.match(/core\.n2_stimulus\(/g) ?? []).length));
  check('S5 a thrown core call is a rejection, never an acceptance',
    ability.includes("reply = { ok: false, result: 'rejected', reason: 'core-throw' };"));
  check('S6 the request is consumed exactly once, in a finally',
    ability.includes('    } finally {\n' +
      '      // consume exactly once, whatever happened above\n' +
      '      try {\n' +
      '        fs.unlinkSync(path);'),
    'finally');
  check('S7 the stimulus poll rides the watcher tick after the window close edge and AFTER the control poll',
    ability.indexOf('      this.maybeCloseInterfaceStatsWindow();') > 0 &&
    ability.indexOf('      this.pollN2Control();') >
      ability.indexOf('      this.maybeCloseInterfaceStatsWindow();') &&
    ability.indexOf('      this.pollN2Stimulus();') > ability.indexOf('      this.pollN2Control();') &&
    ability.includes('      this.pollN2Stimulus();\n      const status: ConnectorStatusResult | null = pollStatusOrNull();'),
    'tick');
  check('S8 the operator trigger is documented at the call site (aa start --ps n2_stimulus)',
    ability.includes('--ps n2_stimulus'), 'trigger doc');

  check('S9 EntryAbility carries the n2_stimulus parameter and the shared basename',
    entry.includes("const N2_STIMULUS_PARAM: string = 'n2_stimulus';") &&
    entry.includes("const N2_STIMULUS_FILE: string = 'netbird-n2-stimulus';"),
    'entry constants');
  check('S10 an explicit parameter writes the request; every other launch clears a leftover',
    entry.includes("params[N2_STIMULUS_PARAM];") &&
    entry.includes('this.writeStimulusFile(stimulusText);') &&
    entry.includes("this.removeStimulusFile('default-no-stimulus');") &&
    entry.includes("this.removeControlFile('default-no-control');"));
  check('S11 the write is honest about a failed/short write (no usable request left behind)',
    entry.includes('N2_STIMULUS_FILE_WRITE_FAILED') &&
    entry.includes('short-write:${written}of${expectedBytes}') &&
    entry.includes("this.removeStimulusFile('failed-write-cleanup');") &&
    entry.includes('N2_STIMULUS_FILE_CLOSE_FAILED'));
  check('S12 the payload is never logged (file name + UTF-8 byte count only)',
    entry.includes('N2_STIMULUS_FILE, expectedBytes);') &&
    entry.includes('N2_STIMULUS_FILE, failureCode);') &&
    !entry.includes('hilog.warn(DOMAIN, TAG, text') &&
    !entry.includes('hilog.info(DOMAIN, TAG, text'));
  check('S13 the arm-ticket channel is untouched by the stimulus channel',
    entry.includes("const N2H_ARM_OPTIONS_PARAM: string = 'n2h_arm_options';") &&
    entry.includes("this.removeArmTicket('default-no-arm');") &&
    entry.includes("this.removeStimulusFile('default-no-stimulus');"));

  // ---------------- R: the core surface ----------------------------------
  const napi = rust('napi.rs');
  const trace = rust('n2_socket_trace.rs');
  const stimulus = rust('n2_stimulus.rs');
  const connector = rust('connector.rs');
  const wgDevice = rust('wg_device.rs');
  const peerConn = rust('peer_conn.rs');
  const sysRs = rust('sys.rs');

  check('R1 the NAPI export exists exactly once and is registered',
    napi.includes('reg!("n2_stimulus", napi_n2_stimulus);') &&
    napi.includes('crate::connector::n2_stimulus_json(&a.string_at(0), &a.string_at(1))'));
  check('R2 the registry registers from the SAME first_io that emits N2_SOCK',
    trace.includes('crate::n2_stimulus::observe(&meta.kind, fd, remote);') &&
    trace.includes('if status != "failed" {'),
    'observe call site');
  check('R3 arming/disarming clears the registry (default-off leaves nothing)',
    trace.includes('crate::n2_stimulus::reset();'));
  check('R4 the seven classes each have an action in the core',
    stimulus.includes('pub const KINDS: [&str; 7] = ["M", "S", "R", "I", "Wb", "Ws", "D"];') &&
    stimulus.includes('"M" | "S" | "R" => Some(Action::ShutdownSocket),') &&
    stimulus.includes('"I" | "Wb" | "Ws" => Some(Action::DropEndpoint),') &&
    stimulus.includes('"D" => Some(Action::RequeryDns),'));
  check('R5 the marker is fixed-shape with the diagnostic tail',
    stimulus.includes('"N2_STIMULUS|id={}|kind={}|target_fd={}|remote={}|mono_ms={}|result={}|reason={}|diag=true|is_evidence=false"'));
  check('R6 unarmed is refused before any syscall and one stimulus consumes one socket',
    stimulus.includes('return record(kind, id, -1, "unknown".into(), "rejected", "not-armed");') &&
    stimulus.includes('sys::shutdown(hold.as_raw_fd(), sys::SHUT_RDWR)') &&
    stimulus.includes('let hold = state().targets.remove(key).and_then(|t| t.hold);'));
  check('R7 the connector owns the sink lifecycle (one per session)',
    connector.includes('crate::n2_stimulus::install_sink(Arc::new(ConnectorStimulusSink));') &&
    connector.includes('crate::n2_stimulus::clear_sink();') &&
    connector.includes('impl crate::n2_stimulus::Sink for ConnectorStimulusSink'));
  check('R8 I/Wb/Ws act on their class only, D re-queries the last protected host',
    connector.includes('"I" => handle.ice.lock_poison().break_ice(crate::sys::mono_ms()),') &&
    connector.includes('"Wb" | "Ws" => handle.wg.reset_peer_paths(),') &&
    connector.includes('"D" => force_protected_dns_requery(&handle),'));
  check('R9 the WG seam drops every selected path without closing the base device socket',
    wgDevice.includes('pub fn reset_peer_paths(&mut self) -> usize') &&
    wgDevice.includes('fn recycle_endpoint_at(&mut self, idx: usize) -> bool') &&
    connector.includes('fn reset_peer_paths(&self) -> usize {'));
  check('R10 the ICE class drops its session and rearms negotiation',
    peerConn.includes('pub fn break_ice(&mut self, now_ms: u64) -> usize') &&
    peerConn.includes('self.drop_session(idx, PeerIceState::Failed, "n2-stimulus-I");') &&
    peerConn.includes('self.guard_reset(idx, now_ms);'));
  check('R11 the only new syscall is the documented shutdown deviation',
    sysRs.includes('pub fn shutdown(fd: c_int, how: c_int) -> c_int;') &&
    sysRs.includes('pub const SHUT_RDWR: c_int = 2;'));
  check('R12 the trace mirror is not polluted by the stimulus-owned dup',
    rust('mgmtsock.rs').includes('pub fn dup_socket_fd_untraced(raw: i32) -> Result<i32, SocketSeamError>') &&
    stimulus.includes('crate::mgmtsock::dup_socket_fd_untraced(fd)'));
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
