// N2 T3 §3 positive-control private channel — shell self-check (host-only, no
// device, no network, no build).
//
// Two layers, the same principle as run-n2-stimulus.mts:
//   * the REAL pure module (client/entry/src/main/ets/vpnextensionability/
//     NetBirdN2ControlRequest.ets, copied byte-for-byte into a temp fixture) is
//     exercised as CODE — the parser and the two receipt formatters;
//   * the integration is pinned on the byte-identical shipped sources, because
//     the armed-only private channel spans three files (EntryAbility writes the
//     request file, the extension polls + calls `sendN2PositiveControl`, the
//     core emits the authoritative `N2_CONTROL_SENT`/`_HIT`/`_MISS` lines) and
//     none of those needs a device.
//
// The behavioural drive of the REAL poll (armed sent / armed refusal / unarmed
// refusal / control-before-stimulus ordering) lives in
// run-exclusion-generation.mts (N group), which stages the REAL extension and
// the REAL EntryAbility against the path-aware fake fs.
//
// Run: node client/entry/selfcheck/run-n2-control.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ETS = join(HERE, '..', 'src', 'main', 'ets');
const REAL = join(ETS, 'vpnextensionability', 'NetBirdN2ControlRequest.ets');
const ABILITY = join(ETS, 'vpnextensionability', 'NetBirdVpnExtensionAbility.ets');
const ENTRY = join(ETS, 'entryability', 'EntryAbility.ets');
const CORE_SRC = join(HERE, '..', '..', 'core', 'src');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-n2-control-'));
const fixture: string = join(fixtureDir, 'NetBirdN2ControlRequest.ts');
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
  const parse = mod['parseN2ControlRequest'] as (text: string) =>
    { request?: { target: string; proto: string; id: string }; error: string };
  const shellLine = mod['formatN2ControlShellLine'] as
    (requestId: string, request: object, result: string, reason: string) => string;
  const shellReject = mod['formatN2ControlShellReject'] as (requestId: string, reason: string) => string;
  const fileName = mod['N2_CONTROL_FILE_NAME'] as string;

  const ability = readText(ABILITY);
  const entry = readText(ENTRY);
  const rust = (name: string): string => readText(join(CORE_SRC, name));

  // ---------------- P: the pure module ------------------------------------
  check('P1 the request basename is the documented literal',
    fileName === 'netbird-n2-control', fileName);

  for (const proto of ['udp', 'tcp']) {
    const decision = parse(`{"target":"119.29.29.119:53","proto":"${proto}","id":"evt1-pre"}`);
    check(`P2 ${proto} is accepted with its request`,
      decision.error === '' && decision.request?.target === '119.29.29.119:53' &&
      decision.request?.proto === proto && decision.request?.id === 'evt1-pre',
      JSON.stringify(decision));
  }
  check('P3 a trailing newline (the private-channel write shape) is harmless',
    parse('{"target":"119.29.29.119:53","proto":"udp","id":"x"}\n').error === '');
  check('P4 surrounding whitespace is trimmed',
    parse('  {"target":"119.29.29.119:443","proto":"tcp","id":"x"}  ').request?.proto === 'tcp');
  check('P5 a port above 1024 and a multi-octet IP are kept verbatim',
    parse('{"target":"203.0.113.7:28443","proto":"tcp","id":"post-1"}').request?.target ===
      '203.0.113.7:28443');

  for (const [label, text, error] of [
    ['empty', '', 'empty-request'],
    ['whitespace only', '   \n ', 'empty-request'],
    ['not JSON', '{target:1}', 'invalid-json'],
    ['JSON null', 'null', 'invalid-request'],
    ['JSON array', '[{"target":"119.29.29.119:53","proto":"udp","id":"x"}]', 'invalid-request'],
    ['JSON string', '"udp"', 'invalid-request'],
    ['a batch payload (batching is not implemented)', '{"controls":[{"target":"119.29.29.119:53","proto":"udp","id":"x"}]}', 'batch-not-implemented'],
    ['missing target', '{"proto":"udp","id":"x"}', 'invalid-target'],
    ['non-string target', '{"target":7,"proto":"udp","id":"x"}', 'invalid-target'],
    ['target without a port', '{"target":"119.29.29.119","proto":"udp","id":"x"}', 'invalid-target'],
    ['target with a hostname', '{"target":"home.alfadb.cn:53","proto":"udp","id":"x"}', 'invalid-target'],
    ['target with an octet over 255', '{"target":"119.29.29.300:53","proto":"udp","id":"x"}', 'invalid-target'],
    ['target with port 0', '{"target":"119.29.29.119:0","proto":"udp","id":"x"}', 'invalid-target'],
    ['target with port over 65535', '{"target":"119.29.29.119:65536","proto":"udp","id":"x"}', 'invalid-target'],
    ['target with a leading zero octet', '{"target":"119.29.29.0119:53","proto":"udp","id":"x"}', 'invalid-target'],
    ['target with a protocol suffix', '{"target":"udp/119.29.29.119:53","proto":"udp","id":"x"}', 'invalid-target'],
    ['missing proto', '{"target":"119.29.29.119:53","id":"x"}', 'invalid-protocol'],
    ['non-string proto', '{"target":"119.29.29.119:53","proto":17,"id":"x"}', 'invalid-protocol'],
    ['unknown proto', '{"target":"119.29.29.119:53","proto":"icmp","id":"x"}', 'invalid-protocol'],
    ['uppercase proto', '{"target":"119.29.29.119:53","proto":"UDP","id":"x"}', 'invalid-protocol'],
    ['missing id', '{"target":"119.29.29.119:53","proto":"udp"}', 'invalid-id'],
    ['non-string id', '{"target":"119.29.29.119:53","proto":"udp","id":7}', 'invalid-id'],
    ['empty id', '{"target":"119.29.29.119:53","proto":"udp","id":""}', 'invalid-id'],
    ['id with a separator', '{"target":"119.29.29.119:53","proto":"udp","id":"a|b"}', 'invalid-id'],
    ['id with a space', '{"target":"119.29.29.119:53","proto":"udp","id":"a b"}', 'invalid-id'],
    ['id too long', `{"target":"119.29.29.119:53","proto":"udp","id":"${'x'.repeat(65)}"}`, 'invalid-id'],
    ['oversized request', `{"target":"119.29.29.119:53","proto":"udp","id":"x","pad":"${'y'.repeat(300)}"}`, 'request-too-long']
  ] as const) {
    const decision = parse(text);
    check(`P6 refuses ${label} (${error})`,
      decision.error === error && decision.request === undefined,
      `${decision.error} ${JSON.stringify(decision.request)}`);
  }

  // The shell receipts.
  const request = { target: '119.29.29.119:53', proto: 'udp', id: 'evt1-pre' };
  const sent = shellLine('req-1', request, 'sent', 'ok');
  check('P7 the shell line is fixed-shape, carries the request and stays diagnostic',
    sent === 'N2_CONTROL_SHELL|requestId=req-1|id=evt1-pre|target=119.29.29.119:53|' +
      'proto=udp|result=sent|reason=ok|diag=true|is_evidence=false',
    sent);
  check('P8 a rejected send names its reason and is never reported as sent',
    shellLine('req-2', request, 'rejected', 'target-not-captured') ===
      'N2_CONTROL_SHELL|requestId=req-2|id=evt1-pre|target=119.29.29.119:53|' +
      'proto=udp|result=rejected|reason=target-not-captured|diag=true|is_evidence=false');
  check('P9 the shell line never lets pipeline characters through',
    !shellLine('req|1', { target: '119.29.29.119:53', proto: 'udp', id: 'a|b' }, 'sent', 'ok')
      .includes('|1|') &&
    shellLine('req|1', { target: '119.29.29.119:53', proto: 'udp', id: 'a|b' }, 'sent', 'ok')
      .includes('requestId=req_1') &&
    shellLine('req|1', { target: '119.29.29.119:53', proto: 'udp', id: 'a|b' }, 'sent', 'ok')
      .includes('|id=a_b|'),
    shellLine('req|1', { target: '119.29.29.119:53', proto: 'udp', id: 'a|b' }, 'sent', 'ok'));
  check('P10 the rejection line names the reason, invents no target, and stays diagnostic',
    shellReject('req-3', 'invalid-target') ===
      'N2_CONTROL_SHELL|requestId=req-3|id=none|target=none|proto=none|' +
      'result=rejected|reason=invalid-target|diag=true|is_evidence=false',
    shellReject('req-3', 'invalid-target'));

  // ---------------- S: the shipped wiring --------------------------------
  check('S1 the extension imports the pure module and its erased shapes',
    ability.includes("} from './NetBirdN2ControlRequest';") &&
    ability.includes('N2_CONTROL_FILE_NAME') && ability.includes('N2ControlDecision') &&
    ability.includes('parseN2ControlRequest }') &&
    ability.includes('formatN2ControlShellLine') && ability.includes('formatN2ControlShellReject'),
    'import block');
  check('S2 the armed guard is consulted BEFORE the send and refused by name',
    ability.includes('      if (this.n2Evidence === undefined) {\n' +
      '        this.n2WarnAndRing(\n' +
      '          formatN2ControlShellLine(this.requestId, request, \'rejected\', \'not-armed\'));\n' +
      '        return;\n' +
      '      }'),
    'unarmed guard');
  check('S3 the request path is the shared filesDir basename',
    ability.includes('const path: string = `${this.context.filesDir}/${N2_CONTROL_FILE_NAME}`;'));
  check('S4 the ONE send call per request, through the positive-control entry',
    (ability.match(/this\.sendN2PositiveControl\(/g) ?? []).length === 1 &&
    ability.includes('this.sendN2PositiveControl(request.target, request.proto, request.id);'),
    String((ability.match(/this\.sendN2PositiveControl\(/g) ?? []).length));
  check('S5 a thrown send is a rejection with its own reason, never a send',
    ability.includes("let result: string = 'rejected';") &&
    ability.includes("result = 'sent';") &&
    ability.includes('reason = (error as Error).message'),
    'send error branch');
  check('S6 the request is consumed exactly once, in a finally',
    ability.includes('    } finally {\n' +
      '      // consume exactly once, whatever happened above\n' +
      '      try {\n' +
      '        fs.unlinkSync(path);'),
    'finally');
  check('S7 the control poll rides the watcher tick BEFORE the stimulus poll',
    ability.indexOf('      this.pollN2Control();') > ability.indexOf('this.maybeCloseInterfaceStatsWindow();') &&
    ability.indexOf('      this.pollN2Stimulus();') > ability.indexOf('      this.pollN2Control();') &&
    ability.includes('      this.pollN2Control();\n      // N2 T5a: the private-channel stimulus poll'),
    'tick order');
  check('S8 the operator trigger is documented at the call site (aa start --ps n2_control)',
    ability.includes('--ps n2_control'), 'trigger doc');
  check('S9 the control entry is the ONLY caller of sendN2PositiveControl (no product path uses it)',
    (ability.match(/this\.sendN2PositiveControl\(/g) ?? []).length === 1 &&
    ability.includes('  sendN2PositiveControl(target: string, protocol: N2ControlProtocol, id: string): void {'),
    String((ability.match(/this\.sendN2PositiveControl\(/g) ?? []).length));

  check('S10 EntryAbility carries the n2_control parameter and the shared basename',
    entry.includes("const N2_CONTROL_PARAM: string = 'n2_control';") &&
    entry.includes("const N2_CONTROL_FILE: string = 'netbird-n2-control';"),
    'entry constants');
  check('S11 an explicit parameter writes the request; every other launch clears a leftover',
    entry.includes('params[N2_CONTROL_PARAM];') &&
    entry.includes('this.writeControlFile(controlText);') &&
    entry.includes("this.removeControlFile('default-no-control');") &&
    entry.includes("this.removeControlFile('combo-rejected');") &&
    entry.includes("this.removeControlFile('failed-write-cleanup');"));
  check('S12 a single launch carrying BOTH requests writes NEITHER (combination refused)',
    entry.includes('if (stimulusText.length > 0 && controlText.length > 0) {') &&
    entry.includes('N2_CONTROL_COMBO_REJECTED|stimulus=%{public}s|control=%{public}s|') &&
    entry.includes("this.removeStimulusFile('combo-rejected');") &&
    entry.indexOf('this.removeControlFile(\'combo-rejected\');') <
      entry.indexOf('this.writeStimulusFile(stimulusText);'),
    'combo rule');
  check('S13 the write is honest about a failed/short write (no usable request left behind)',
    entry.includes('N2_CONTROL_FILE_WRITE_FAILED') &&
    entry.includes('short-write:${written}of${expectedBytes}') &&
    entry.includes('N2_CONTROL_FILE_CLOSE_FAILED'));
  check('S14 the payload is never logged (file name + UTF-8 byte count only)',
    entry.includes('N2_CONTROL_FILE, expectedBytes);') &&
    entry.includes('N2_CONTROL_FILE, failureCode);') &&
    !entry.includes('hilog.warn(DOMAIN, TAG, text') &&
    !entry.includes('hilog.info(DOMAIN, TAG, text'));
  check('S15 the arm-ticket and stimulus channels are untouched by the control channel',
    entry.includes("const N2H_ARM_OPTIONS_PARAM: string = 'n2h_arm_options';") &&
    entry.includes("const N2_STIMULUS_PARAM: string = 'n2_stimulus';") &&
    entry.includes("this.removeArmTicket('default-no-arm');") &&
    entry.includes("this.removeStimulusFile('default-no-stimulus');"));

  // ---------------- H: the loader mocks stay in sync ---------------------
  for (const hookName of ['exclusion-generation-hooks.mjs', 'relay-exclusion-hooks.mjs']) {
    const hook = readText(join(HERE, hookName));
    check(`H1 ${hookName} stubs the control sender as a recording seam`,
      hook.includes("if (specifier === './NetBirdN2Control')") &&
      hook.includes('g.__NB_CONTROL_LAST = { armed: armed, target: target, protocol: protocol, id: id };') &&
      hook.includes('if (armed === undefined) { throw new Error("n2-not-armed"); }'),
      hookName);
    check(`H2 ${hookName} resolves the control request module to its staged shim`,
      hook.includes("'./NetBirdN2ControlRequest': 'NetBirdN2ControlRequest.shim.ts'"),
      hookName);
  }
  check('H3 both staging harnesses stage the control request module and its erased types',
    readText(join(HERE, 'run-exclusion-generation.mts')).includes("'NetBirdN2ControlRequest',") &&
    readText(join(HERE, 'run-relay-exclusion.mts')).includes("'NetBirdN2ControlRequest',") &&
    readText(join(HERE, 'run-exclusion-generation.mts')).includes(
      "NetBirdN2ControlRequest: ['N2ControlDecision', 'N2ControlRequest'],") &&
    readText(join(HERE, 'run-relay-exclusion.mts')).includes(
      "NetBirdN2ControlRequest: ['N2ControlDecision', 'N2ControlRequest'],"));

  // ---------------- R: the core surface ----------------------------------
  const napi = rust('napi.rs');
  const control = rust('n2_control.rs');
  const trace = rust('n2_socket_trace.rs');
  const dns = rust('protected_dns.rs');

  check('R1 the n2_control NAPI exports stay registered and reach the control state',
    napi.includes('reg!("n2_control_send", napi_n2_control_send);') &&
    napi.includes('reg!("n2_control_status", napi_n2_control_status);') &&
    napi.includes('crate::connector::n2_control_send_json(a.i32_at(0, -1),'));
  check('R2 the core emits the authoritative SENT and HIT/MISS lines, each with id= and marker=',
    // N2 T7a item 2: every HIT/MISS/SENT line carries the REQUEST `id` and the
    // matched `marker` (identical by construction) so a judge can look the
    // request up by id without knowing the marker convention.
    control.includes('pub fn sent_line(id: &str, destination: &str, proto: &str, source_port: u16, now: u64) -> String {') &&
    control.includes('"N2_CONTROL_SENT|id={id}|marker={id}|dst={destination}|proto={proto}|source_port={source_port}|mono_ms={now}|diag=true|is_evidence=false"') &&
    control.includes('"N2_CONTROL_HIT|id={id}|marker={id}|dst={}:{port}|proto={}|mono_ms={now}|diag=true|is_evidence=false"') &&
    control.includes('"N2_CONTROL_MISS|id={id}|marker={id}|dst={}:{port}|proto={}|mono_ms={now}|diag=true|is_evidence=false"'));
  check('R3 a reused marker id / uncaptured target is refused in the core',
    control.includes('return Err("target-not-captured");') &&
    control.includes('return Err("duplicate-control");'));

  // N2_DNS (preregistration §4 D row) — armed-only, every documented field.
  check('R4 the N2_DNS receipt is armed-only and carries every documented field',
    trace.includes('pub fn dns_receipt(') && trace.includes('if !armed() { return None; }') &&
    trace.includes('"N2_DNS|query_id={}|qname={}|server={}|fd={}|dup_fd={}|sent_mono_ms={}|' +
      'response={}|rcode={}|answer_count={}|elapsed_ms={}|diag=true|is_evidence=false"'),
    'dns receipt');
  check('R5 the production DNS path emits it per query, from the shared descriptor identity',
    dns.includes('crate::n2_socket_trace::dns_receipt(&receipt)') &&
    dns.includes('crate::n2_socket_trace::original_fd(request_fd).unwrap_or(request_fd)') &&
    dns.includes('dns_response_class(outcome.is_ok(), facts, query_id, &error_token)'));
  check('R6 the D-row verdicts are the frozen four (matched/mismatched/timeout/error)',
    dns.includes('if ok { return "matched"; }') &&
    dns.includes('if error == "dns:timeout" { return "timeout"; }') &&
    dns.includes('if facts.has_response && facts.response_id >= 0 && facts.response_id != query_id as i32 {') &&
    dns.includes('    "error"\n'));
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
