// N2 first batch C/D columns — the App-side switching sandwich shell self-check
// (host-only, no device, no network, no build).
//
// Two layers, the same principle as run-n2-control.mts:
//   * the REAL pure module (client/entry/src/main/ets/vpnextensionability/
//     NetBirdN2CdPlan.ets, copied byte-for-byte into a temp fixture) is
//     exercised as CODE — the plan parser, the target/protocol choice, the
//     bearer classification, the two direction verdicts and the receipts;
//   * the integration is pinned on the byte-identical shipped sources, because
//     the channel spans three files (EntryAbility writes the request file, the
//     extension polls + measures + drives the core, the core owns the ordering
//     verdict) and none of those needs a device.
//
// The behavioural drive of the REAL poll (armed accept + pre control, unarmed
// refusal, the switch hook, the receipt step, the ring forwarding and the
// 5-channel combination rule) lives in run-exclusion-generation.mts (N group).
//
// Run: node client/entry/selfcheck/run-n2-cd.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ETS = join(HERE, '..', 'src', 'main', 'ets');
const REAL = join(ETS, 'vpnextensionability', 'NetBirdN2CdPlan.ets');
const ABILITY = join(ETS, 'vpnextensionability', 'NetBirdVpnExtensionAbility.ets');
const ENTRY = join(ETS, 'entryability', 'EntryAbility.ets');
const CONNECTOR = join(ETS, 'vpnextensionability', 'NetBirdConnector.ets');
const CORE_SRC = join(HERE, '..', '..', 'core', 'src');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-n2-cd-'));
const fixture: string = join(fixtureDir, 'NetBirdN2CdPlan.ts');
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
  const parse = mod['parseN2CdPlan'] as (text: string) =>
    { request?: { column: string; kind: string; rep: number; id: string }; error: string };
  const protocol = mod['n2CdProtocol'] as (kind: string) => string;
  const select = mod['selectN2CdTarget'] as (destinations: string[], kind: string) => string;
  const classify = mod['classifyN2Bearer'] as (bearerTypes: number[], links: string[]) =>
    { bearer: string; local: string };
  const startVerdict = mod['n2CdStartVerdict'] as (column: string, before: string) => string;
  const directionVerdict = mod['n2CdDirectionVerdict'] as
    (column: string, before: string, after: string) => string;
  const transitionStep = mod['n2CdTransitionStep'] as
    (column: string, before: string, after: string) => string;
  const prepareVerdict = mod['n2CdPrepareVerdict'] as (after: string) => string;
  const shellLine = mod['formatN2CdPlanShellLine'] as (requestId: string,
    request: { column: string; kind: string; rep: number; id: string }, phase: string,
    marker: string, result: string, reason: string) => string;
  const shellReject = mod['formatN2CdPlanShellReject'] as (requestId: string, reason: string) => string;
  const fileName = mod['N2_CD_FILE_NAME'] as string;
  const columns = mod['N2_CD_COLUMNS'] as string[];
  const kinds = mod['N2_CD_KINDS'] as string[];
  const maxId = mod['N2_CD_MAX_ID'] as number;
  const bearerLiterals = [mod['N2_BEARER_CELLULAR'], mod['N2_BEARER_WIFI'],
    mod['N2_BEARER_BLUETOOTH'], mod['N2_BEARER_ETHERNET'], mod['N2_BEARER_VPN']];

  const ability = readText(ABILITY);
  const entry = readText(ENTRY);
  const connector = readText(CONNECTOR);
  const rust = (name: string): string => readText(join(CORE_SRC, name));

  // ---------------- P: the pure module ------------------------------------
  check('P1 the request basename and the frozen vocabularies are the documented literals',
    fileName === 'netbird-n2-cd-plan' && columns.length === 2 && columns[0] === 'C' &&
    columns[1] === 'D' && kinds.length === 7 && kinds[0] === 'M' && kinds[6] === 'Ws' &&
    maxId === 59 && bearerLiterals[0] === 0 && bearerLiterals[1] === 1 &&
    bearerLiterals[2] === 2 && bearerLiterals[3] === 3 && bearerLiterals[4] === 4,
    JSON.stringify({ fileName, columns, kinds, maxId, bearerLiterals }));

  const accepted = parse('{"column":"C","kind":"M","rep":1,"id":"cd-C-M-1"}');
  check('P2 a C plan is accepted with its request',
    accepted.error === '' && accepted.request?.column === 'C' && accepted.request?.kind === 'M' &&
    accepted.request?.rep === 1 && accepted.request?.id === 'cd-C-M-1',
    JSON.stringify(accepted));
  check('P3 a D plan with the last repetition and the longest id is accepted',
    parse('{"column":"D","kind":"Ws","rep":999,"id":"' + 'a'.repeat(maxId) + '"}').error === '');
  check('P4 a trailing newline (the private-channel write shape) is harmless',
    parse('{"column":"C","kind":"I","rep":2,"id":"x"}\n').error === '');
  check('P5 surrounding whitespace is trimmed',
    parse('  {"column":"D","kind":"R","rep":3,"id":"x"}  ').request?.kind === 'R');

  for (const [label, text, error] of [
    ['empty', '', 'empty-request'],
    ['whitespace only', '   \n ', 'empty-request'],
    ['not JSON', '{column:1}', 'invalid-json'],
    ['JSON null', 'null', 'invalid-request'],
    ['JSON array', '[{"column":"C","kind":"M","rep":1,"id":"x"}]', 'invalid-request'],
    ['missing column', '{"kind":"M","rep":1,"id":"x"}', 'invalid-column'],
    ['unknown column', '{"column":"E","kind":"M","rep":1,"id":"x"}', 'invalid-column'],
    ['non-string column', '{"column":3,"kind":"M","rep":1,"id":"x"}', 'invalid-column'],
    ['missing kind', '{"column":"C","rep":1,"id":"x"}', 'invalid-kind'],
    ['unknown kind', '{"column":"C","kind":"W","rep":1,"id":"x"}', 'invalid-kind'],
    ['lowercase kind', '{"column":"C","kind":"m","rep":1,"id":"x"}', 'invalid-kind'],
    ['missing rep', '{"column":"C","kind":"M","id":"x"}', 'invalid-rep'],
    ['rep zero', '{"column":"C","kind":"M","rep":0,"id":"x"}', 'invalid-rep'],
    ['rep fractional', '{"column":"C","kind":"M","rep":1.5,"id":"x"}', 'invalid-rep'],
    ['rep over the bound', '{"column":"C","kind":"M","rep":1000,"id":"x"}', 'invalid-rep'],
    ['missing id', '{"column":"C","kind":"M","rep":1}', 'invalid-id'],
    ['empty id', '{"column":"C","kind":"M","rep":1,"id":""}', 'invalid-id'],
    ['id with a separator', '{"column":"C","kind":"M","rep":1,"id":"a|b"}', 'invalid-id'],
    ['id with a space', '{"column":"C","kind":"M","rep":1,"id":"a b"}', 'invalid-id'],
    ['id one over the marker-derived bound',
      `{"column":"C","kind":"M","rep":1,"id":"${'a'.repeat(maxId + 1)}"}`, 'invalid-id'],
    ['oversized request',
      `{"column":"C","kind":"M","rep":1,"id":"x","pad":"${'y'.repeat(300)}"}`, 'request-too-long']
  ] as const) {
    const decision = parse(text);
    check(`P6 refuses ${label} (${error})`,
      decision.error === error && decision.request === undefined,
      `${decision.error} ${JSON.stringify(decision.request)}`);
  }

  // The kind's protocol is the production opener's, never a free choice.
  check('P7 the protocol follows the kind (M/S/R tcp, the rest udp)',
    protocol('M') === 'tcp' && protocol('S') === 'tcp' && protocol('R') === 'tcp' &&
    protocol('D') === 'udp' && protocol('I') === 'udp' && protocol('Wb') === 'udp' &&
    protocol('Ws') === 'udp');

  // The destination is one of the armed ticket's frozen outer destinations, of
  // the kind's OWN protocol, with the frozen I/D ports preferred.
  const destinations: string[] = ['tcp/203.0.113.7:443', 'udp/203.0.113.9:53',
    'udp/203.0.113.11:3478', 'udp/203.0.113.12:51820'];
  check('P8 the I marker targets the frozen STUN destination',
    select(destinations, 'I') === '203.0.113.11:3478', select(destinations, 'I'));
  check('P9 the D marker targets a udp destination (the resolved port-53 entry first)',
    select(destinations, 'D') === '203.0.113.9:53', select(destinations, 'D'));
  check('P10 an M/S/R marker targets a tcp destination',
    select(destinations, 'M') === '203.0.113.7:443' &&
    select(destinations, 'S') === '203.0.113.7:443');
  check('P11 Wb/Ws markers fall back to the first udp destination',
    select(destinations, 'Wb') === '203.0.113.9:53' &&
    select(destinations, 'Ws') === '203.0.113.9:53');
  check('P12 a missing protocol entry is an honest empty target (the caller refuses)',
    select(['udp/203.0.113.9:53'], 'M') === '' &&
    select([], 'I') === '' && select(['garbage'], 'D') === '');

  // The measured bearer facts.
  check('P13 wifi wins over cellular, cellular alone answers cellular',
    classify([1, 0], []).bearer === 'wifi' && classify([0], []).bearer === 'cellular' &&
    classify([3], []).bearer === 'ethernet' && classify([2], []).bearer === 'bluetooth');
  check('P14 a VPN bearer (or no bearer at all) is NOT a physical bearer',
    classify([4], []).bearer === 'unknown' && classify([], []).bearer === 'unknown');
  check('P15 the first link address is the recorded local name; none is unknown',
    classify([1], ['192.168.50.199', '10.0.0.2']).local === '192.168.50.199' &&
    classify([1], []).local === 'unknown' && classify([1], ['']).local === 'unknown');

  // The plan-ACCEPTANCE verdict. The private channel IS the Wi-Fi hdc link, so
  // BOTH columns are accepted while the device is on Wi-Fi: C is then in
  // position for its Wi-Fi -> cellular switch and D is PRE-SET, reaching its
  // cellular start through its own preparation hop. Every other measured
  // acceptance bearer is refused by name.
  check('P16 the start verdict accepts BOTH columns on the Wi-Fi acceptance link',
    startVerdict('C', 'wifi') === '' && startVerdict('D', 'wifi') === '');
  check('P17 the start verdict refuses every non-Wi-Fi acceptance bearer by name',
    startVerdict('C', 'cellular') === 'start-network-mismatch' &&
    startVerdict('D', 'cellular') === 'start-network-mismatch' &&
    startVerdict('C', 'ethernet') === 'start-network-mismatch' &&
    startVerdict('D', 'ethernet') === 'start-network-mismatch' &&
    startVerdict('C', 'unknown') === 'bearer-unknown' &&
    startVerdict('D', 'unknown') === 'bearer-unknown');
  check('P18 the direction verdict accepts exactly the column\'s transition',
    directionVerdict('C', 'wifi', 'cellular') === '' &&
    directionVerdict('D', 'cellular', 'wifi') === '');
  check('P19 the direction verdict refuses the wrong transition by name',
    directionVerdict('C', 'wifi', 'wifi') === 'direction-mismatch' &&
    directionVerdict('C', 'cellular', 'wifi') === 'start-network-mismatch' &&
    directionVerdict('D', 'cellular', 'cellular') === 'direction-mismatch');
  check('P20 an unmeasurable bearer on EITHER side is UNSAT, never a guess',
    directionVerdict('C', 'wifi', 'unknown') === 'bearer-unknown' &&
    directionVerdict('C', 'unknown', 'cellular') === 'bearer-unknown' &&
    directionVerdict('D', 'unknown', 'unknown') === 'bearer-unknown');

  // WHICH step a measured transition is: D's Wi-Fi -> non-Wi-Fi hop is the
  // PREPARATION (recorded, no control, no gate); everything else a plan on its
  // own start state can see is the FORMAL switch; a D plan still on Wi-Fi that
  // moved between Wi-Fi networks is NOT this plan's transition at all.
  check('P25 the D column\'s Wi-Fi -> non-Wi-Fi hop is the PREPARATION step',
    transitionStep('D', 'wifi', 'cellular') === 'prepare' &&
    transitionStep('D', 'wifi', 'ethernet') === 'prepare' &&
    transitionStep('D', 'wifi', 'unknown') === 'prepare' &&
    transitionStep('D', 'wifi', 'wifi') === 'none');
  check('P26 the C column and the D formal switch are the SWITCH step only',
    transitionStep('C', 'wifi', 'cellular') === 'switch' &&
    transitionStep('C', 'wifi', 'wifi') === 'switch' &&
    transitionStep('D', 'cellular', 'wifi') === 'switch' &&
    transitionStep('D', 'cellular', 'cellular') === 'switch' &&
    transitionStep('E', 'wifi', 'cellular') === 'none');
  check('P27 the preparation hop\'s verdict accepts only the D start bearer',
    prepareVerdict('cellular') === '' && prepareVerdict('wifi') === 'direction-mismatch' &&
    prepareVerdict('ethernet') === 'direction-mismatch' &&
    prepareVerdict('unknown') === 'bearer-unknown');

  // The shell receipts.
  const request = { column: 'C', kind: 'M', rep: 1, id: 'cd-C-M-1' };
  check('P21 the shell line is fixed-shape and stays diagnostic',
    shellLine('req-1', request, 'plan', 'none', 'planned', 'ok') ===
      'N2_CD_SANDWICH_SHELL|requestId=req-1|id=cd-C-M-1|column=C|kind=M|rep=1|phase=plan|' +
      'marker=none|result=planned|reason=ok|diag=true|is_evidence=false',
    shellLine('req-1', request, 'plan', 'none', 'planned', 'ok'));
  check('P22 a phase line carries its OWN marker and the named outcome',
    shellLine('req-2', request, 'post', 'cd-C-M-1-post', 'observed', 'ok') ===
      'N2_CD_SANDWICH_SHELL|requestId=req-2|id=cd-C-M-1|column=C|kind=M|rep=1|phase=post|' +
      'marker=cd-C-M-1-post|result=observed|reason=ok|diag=true|is_evidence=false' &&
    shellLine('req-3', request, 'pre', 'cd-C-M-1-pre', 'rejected', 'n2-control-miss') ===
      'N2_CD_SANDWICH_SHELL|requestId=req-3|id=cd-C-M-1|column=C|kind=M|rep=1|phase=pre|' +
      'marker=cd-C-M-1-pre|result=rejected|reason=n2-control-miss|diag=true|is_evidence=false');
  check('P23 the shell line never lets pipeline characters through',
    shellLine('req|1', { column: 'C', kind: 'M', rep: 1, id: 'a|b' }, 'plan', 'none', 'rejected',
      'invalid-id').indexOf('|1|') < 0 &&
    shellLine('req|1', { column: 'C', kind: 'M', rep: 1, id: 'a|b' }, 'plan', 'none', 'rejected',
      'invalid-id').indexOf('requestId=req_1') > 0);
  check('P24 the rejection line names the reason and invents no plan',
    shellReject('req-4', 'invalid-kind') ===
      'N2_CD_SANDWICH_SHELL|requestId=req-4|id=none|column=none|kind=none|rep=0|phase=plan|' +
      'marker=none|result=rejected|reason=invalid-kind|diag=true|is_evidence=false',
    shellReject('req-4', 'invalid-kind'));

  // ---------------- S: the shipped wiring --------------------------------
  check('S1 the extension imports the pure module and its erased shapes',
    ability.includes("} from './NetBirdN2CdPlan';") && ability.includes('N2_CD_FILE_NAME') &&
    ability.includes('N2CdPlanDecision') && ability.includes('N2CdPlanRequest') &&
    ability.includes('N2BearerFacts') && ability.includes('parseN2CdPlan,') &&
    ability.includes('n2CdDirectionVerdict') && ability.includes('n2CdStartVerdict') &&
    ability.includes('classifyN2Bearer') && ability.includes('selectN2CdTarget'),
    'import block');
  check('S2 the armed guard is consulted BEFORE the plan reaches the core',
    ability.includes('      const armed: N2EvidenceOptions | undefined = this.n2Evidence;\n' +
      '      if (armed === undefined) {\n' +
      '        const refused: string = formatN2CdPlanShellLine(this.requestId, request, \'plan\', \'none\',\n' +
      '          \'rejected\', \'not-armed\');\n' +
      '        hilog.warn(DOMAIN, TAG, \'%{public}s\', refused);\n' +
      '        return;\n' +
      '      }'),
    'unarmed guard');
  check('S3 the request path is the shared filesDir basename',
    ability.includes('const path: string = `${this.context.filesDir}/${N2_CD_FILE_NAME}`;'));
  // The acceptance path only REGISTERS the plan: no control is sent here (a
  // control sent at acceptance sits on the OLD network and the core refuses it
  // `pre-before-switch`). The pre leg is driven by the core's own `step=pre`.
  check('S4 the plan goes to the core with the measured bearer facts and the shell verdict',
    ability.includes('reply = JSON.parse(core.n2_cd_plan(request.id, request.column, request.kind,') &&
    ability.includes('request.rep, target, bearer.bearer, bearer.local, startVerdict)) as'));
  check('S5 acceptance sends NO control, and the pre leg is driven by the core\'s step=pre',
    ability.includes('if (step === \'pre\' && marker.length > 0) {') &&
    ability.includes('await sendAndWaitN2Control(armed, target, protocol, marker);') &&
    ability.includes('JSON.parse(core.n2_cd_pre(request.id, result, reason)) as') &&
    ability.includes(": 'control-send-throw';") &&
    ability.includes('      if (!accepted || target.length === 0) {\n        return;\n      }'));
  check('S6 the switch report precedes the re-feed, carries the identity baseline and owns the gate',
    ability.indexOf('    const gated: boolean = netId !== 0 && this.n2CdOnSwitch(netId);') > 0 &&
    ability.indexOf('    const gated: boolean = netId !== 0 && this.n2CdOnSwitch(netId);') <
      ability.indexOf("    connectorPhysicalNetworkChanged('');") &&
    ability.includes('const identity: Record<string, Object> = JSON.parse(core.n2_cd_identity(this.n2CdKind)) as') &&
    ability.includes("identityFd = Number(identity['fd'] ?? -1);") &&
    ability.includes('JSON.parse(core.n2_cd_switch_identified(id, direction,') &&
    ability.includes('after, bearer.local, netId, identityFd, identityDup, identityGeneration)) as') &&
    ability.includes('    if (netId !== 0 && !gated) {'));
  check('S7 the step poll sends the POST control only on `step=receipt` with the core\'s marker',
    ability.includes("if (step !== 'receipt' || marker.length === 0 || target.length === 0) {") &&
    ability.includes('JSON.parse(core.n2_cd_post(request.id, result, reason)) as') &&
    ability.includes("reason = 'post-control-miss';") &&
    ability.includes('this.n2CdSendPostControl(armed, request, target, protocol, marker, remote)'));
  check('S8 the step poll mirrors the core\'s phase in its own receipt',
    ability.includes('phase.length > 0 ? phase : \'result\''));
  check('S9 the watcher tick polls ring dump, ring ACK, plan, step and A-event in that order',
    ability.indexOf('      this.pollN2RingDump();\n      this.pollN2RingAck();\n' +
      '      this.pollN2CdPlan();\n' +
      '      this.pollN2CdStep();\n') > 0 &&
    ability.indexOf('      this.pollN2AEvent();') >
      ability.indexOf('      this.pollN2RingDump();\n      this.pollN2RingAck();\n' +
        '      this.pollN2CdPlan();\n      this.pollN2CdStep();\n'));
  check('S10 the operator trigger is documented at the call site (aa start --ps n2_cd_plan)',
    ability.includes('--ps n2_cd_plan'), 'trigger doc');
  check('S11 the network observer measures the bearer through the platform facts helper',
    ability.includes('const facts: PhysicalBearerFacts = physicalDefaultBearerFacts();') &&
    ability.includes('const bearer: N2BearerFacts = classifyN2Bearer(facts.bearerTypes, facts.linkAddresses);') &&
    connector.includes('export function physicalDefaultBearerFacts(): PhysicalBearerFacts {') &&
    connector.includes("connection.getConnectionPropertiesSync(handle);") &&
    connector.includes('linkAddresses.push(address.address);'),
    'bearer facts');
  check('S12 EntryAbility carries the n2_cd_plan parameter and the shared basename',
    entry.includes("const N2_CD_PLAN_PARAM: string = 'n2_cd_plan';") &&
    entry.includes("const N2_CD_PLAN_FILE: string = 'netbird-n2-cd-plan';") &&
    entry.includes('params[N2_CD_PLAN_PARAM];') &&
    entry.includes('this.writeCdPlanFile(cdPlanText);') &&
    entry.includes("this.removeCdPlanFile('default-no-cd-plan');") &&
    entry.includes("this.removeCdPlanFile('combo-rejected');") &&
    entry.includes("this.writeN2RequestFile('N2_CD_PLAN_FILE', N2_CD_PLAN_FILE, 'no-cd-plan', text);"),
    'entry constants');
  check('S13 a plan-only launch clears the other five channel leftovers',
    entry.includes("this.removeStimulusFile('default-no-stimulus');") &&
    entry.includes("this.removeControlFile('default-no-control');") &&
    entry.includes("this.removeAEventFile('default-no-a-event');") &&
    entry.includes("this.removeRingDumpFile('default-no-ring-dump');") &&
    entry.includes("this.removeRingAckFile('default-no-ring-ack');"));
  check('S14 the write is honest about a failed/short write (no usable plan left behind)',
    entry.includes("marker + '_WRITE_FAILED|file=%{public}s|code=%{public}s|note=' + note") &&
    entry.includes('short-write:${written}of${expectedBytes}') &&
    entry.includes("marker + '_CLOSE_FAILED|file=%{public}s|code=%{public}s'"));
  check('S15 the payload is never logged (file name + UTF-8 byte count only)',
    entry.includes('name, expectedBytes);') && entry.includes('name, failureCode);'));

  // ---------------- G: the formal-switch production-recovery gate ----------
  // One gate, one lifecycle: armed synchronously at the formal switch, CLOSED
  // to every supply entry until the core's own poll releases it for the very
  // plan and epoch the switch armed, HELD on a terminal UNSAT until the
  // evidence session is explicitly terminated, and absent when unarmed.
  const policy = readText(join(ETS, 'vpnextensionability', 'NetBirdPhysicalNetworkPolicy.ets'));
  check('G1 the gate state machine exists with its four documented states',
    ability.includes("private n2CdGateState: string = '';") &&
    ability.includes('private n2CdGateEpoch: number = 0;') &&
    ability.includes("private n2CdGatePlan: string = '';") &&
    ability.includes("this.n2CdGateState = 'blocked';") &&
    ability.includes("this.n2CdGateState = 'released';") &&
    ability.includes("this.n2CdGateState = 'held';") &&
    ability.includes('private n2CdProductionBlocked(epoch: number = this.networkEpoch): boolean {'),
    'gate fields/states');
  check('G2 the gate is armed SYNCHRONOUSLY before the identity capture and the switch report',
    ability.indexOf('    this.n2CdGateArm();') > 0 &&
    ability.indexOf('    this.n2CdGateArm();') <
      ability.indexOf('const identity: Record<string, Object> = JSON.parse(core.n2_cd_identity(') &&
    ability.indexOf('const identity: Record<string, Object> = JSON.parse(core.n2_cd_identity(') <
      ability.indexOf('JSON.parse(core.n2_cd_switch_identified(id, direction,'),
    'arm ordering');
  check('G3 the pre control is owed on the new network AFTER the core cleanup, never at acceptance ' +
    'and never from the switch report',
    ability.indexOf("    connectorPhysicalNetworkChanged('');") > 0 &&
    ability.indexOf("    connectorPhysicalNetworkChanged('');") <
      ability.indexOf('    if (gated) {\n      this.pollN2CdStep();\n    }') &&
    ability.indexOf('    this.n2CdBeforeBearer = after;\n' +
      '    // The gate stays \'blocked\' here; the pre control is owed on the NEW network') > 0 &&
    ability.includes('      // The gate is deliberately NOT touched here'),
    'pre ordering');
  check('G4 every supply entry consults the gate: recovery entry + chase rounds, on-demand supply, ' +
    'spare socket and the recreate-finally retry',
    ability.includes('if (this.n2CdProductionBlocked()) { return; }') &&
    ability.includes('if (this.n2CdProductionBlocked(attemptEpoch)) { break; }') &&
    ability.includes('this.networkNetId !== 0 && !this.n2CdProductionBlocked());') &&
    ability.includes('productionGateBlocked: this.n2CdProductionBlocked()') &&
    ability.includes('this.n2CdProductionBlocked(epoch)) {') &&
    // STAGE 4 (official-alignment audit §2.7): the ICE socket pool — and with it
    // the `paused` sizing chain that consumed the gate — is deleted. The ONE
    // shared-mux arm in the same leg still consults the gate explicitly (the
    // `this.n2CdProductionBlocked(epoch)` clause asserted above).
    ability.includes('if (this.networkDirty && !this.destroyRequested && this.networkNetId !== 0 &&\n' +
      '        !this.n2CdProductionBlocked()) {'),
    'supply entries');
  check('G5 the release is bound to the plan AND the switch epoch',
    ability.includes('private n2CdGateRelease(id: string): boolean {') &&
    ability.includes('if (this.n2CdGateState !== \'blocked\' || this.n2CdGatePlan !== id ||\n' +
      '      this.networkEpoch !== this.n2CdGateEpoch) {') &&
    ability.includes('      if (!this.n2CdGateRelease(id)) {\n        return;\n      }'),
    'release binding');
  check('G6 a terminal UNSAT HOLDS the gate, a terminal success releases it, and the explicit ' +
    'termination (readDiagSwitches) resets it',
    ability.includes("    } else if (result === 'unsat') {\n      this.n2CdGateHold();") &&
    ability.includes("      if (terminal === 'observed') {\n        this.n2CdGateReset();") &&
    ability.includes('    this.n2CdPreparePending = false;\n    this.n2CdBusy = false;\n  }') &&
    ability.includes('    this.n2CdGateReset();\n    this.n2Evidence = undefined;'),
    'terminal handling');
  check('G7 the pure policy carries the gate field and checks it BEFORE the network-dirty rule ' +
    '(outside that rule\'s DNS carve-out)',
    policy.includes('productionGateBlocked: boolean;') &&
    policy.includes("if (gate.productionGateBlocked) { return 'production-gate'; }") &&
    policy.indexOf("if (gate.productionGateBlocked) { return 'production-gate'; }") <
      policy.indexOf('if (gate.networkDirty && gate.networkRecoveryReady &&'),
    'policy');

  // ---------------- H: the loader mocks stay in sync ---------------------
  for (const hookName of ['exclusion-generation-hooks.mjs', 'relay-exclusion-hooks.mjs']) {
    const hook = readText(join(HERE, hookName));
    check(`H1 ${hookName} stubs the CD seams and the ring seams`,
      hook.includes('n2_cd_plan:') && hook.includes('n2_cd_pre:') &&
      hook.includes('n2_cd_prepare:') && hook.includes('n2_cd_ready:') &&
      hook.includes('n2_cd_identity:') && hook.includes('n2_cd_switch_identified:') &&
      hook.includes('n2_cd_post:') && hook.includes('n2_cd_poll:') &&
      hook.includes('n2_ring_configure:') && hook.includes('n2_ring_append:') &&
      hook.includes('n2_ring_disable:') && hook.includes('n2_ring_status:') &&
      hook.includes('n2_ring_dump:') && hook.includes('n2_ring_manifest:') &&
      hook.includes('n2_ring_ack:') && !hook.includes('n2_cd_switch:'),
      hookName);
    check(`H2 ${hookName} resolves the CD pure module to its staged shim`,
      hook.includes("'./NetBirdN2CdPlan': 'NetBirdN2CdPlan.shim.ts'"), hookName);
  }
  for (const runner of ['run-exclusion-generation.mts', 'run-relay-exclusion.mts',
    'run-exclusion-addr-shape.mts']) {
    const text = readText(join(HERE, runner));
    check(`H3 ${runner} stages the CD pure module and its erased types`,
      text.includes("'NetBirdN2CdPlan',") &&
      text.includes("NetBirdN2CdPlan: ['N2CdPlanDecision', 'N2CdPlanRequest', 'N2BearerFacts']"),
      runner);
  }

  // ---------------- R: the core surface ----------------------------------
  const napi = rust('napi.rs');
  const cd = rust('n2_cd.rs');
  // Every NON-comment line of the module: the module docs still NAME the
  // rejected criterion, so an "is it gone" pin must read the code only.
  const cdCode: string = cd.split('\n').filter((line: string): boolean =>
    !line.trimStart().startsWith('//')).join('\n');
  const wg = rust('wg_device.rs');
  check('R1 the eight n2_cd NAPI exports are registered and reach the module',
    napi.includes('reg!("n2_cd_plan", napi_n2_cd_plan);') &&
    napi.includes('reg!("n2_cd_prepare", napi_n2_cd_prepare);') &&
    napi.includes('reg!("n2_cd_ready", napi_n2_cd_ready);') &&
    napi.includes('reg!("n2_cd_pre", napi_n2_cd_pre);') &&
    napi.includes('reg!("n2_cd_switch_identified", napi_n2_cd_switch_identified);') &&
    napi.includes('reg!("n2_cd_identity", napi_n2_cd_identity);') &&
    napi.includes('reg!("n2_cd_poll", napi_n2_cd_poll);') &&
    napi.includes('reg!("n2_cd_post", napi_n2_cd_post);') &&
    !napi.includes('reg!("n2_cd_switch"') &&
    napi.includes('crate::connector::n2_cd_plan_json(') &&
    napi.includes('crate::connector::n2_cd_switch_identified_json(') &&
    napi.includes('crate::connector::n2_cd_identity_json('),
    'the CD export list');
  check('R2 the seven ring NAPI exports are registered and reach the ring',
    napi.includes('reg!("n2_ring_configure", napi_n2_ring_configure);') &&
    napi.includes('reg!("n2_ring_append", napi_n2_ring_append);') &&
    napi.includes('reg!("n2_ring_disable", napi_n2_ring_disable);') &&
    napi.includes('reg!("n2_ring_status", napi_n2_ring_status);') &&
    napi.includes('reg!("n2_ring_dump", napi_n2_ring_dump);') &&
    napi.includes('reg!("n2_ring_manifest", napi_n2_ring_manifest);') &&
    napi.includes('reg!("n2_ring_ack", napi_n2_ring_ack);') &&
    napi.includes('crate::connector::n2_ring_append_json(') &&
    napi.includes('crate::connector::n2_ring_dump_json('),
    'the ring export list');
  check('R3 the plan line is the documented literal and every phase names its own marker',
    cd.includes('"N2_CD_SANDWICH|id={}|column={}|kind={}|rep={rep}|phase={}|marker={}|result={}|reason={}|net_before={}|net_after={}|local_before={}|local_after={}|fd={fd}|dup_fd={dup_fd}|remote={}|wait_ms={wait_ms}|mono_ms={mono_ms}|diag=true|is_evidence=false"') &&
    cd.includes('fn pre_marker(id: &str) -> String { format!("{id}-pre") }') &&
    cd.includes('fn post_marker(id: &str) -> String { format!("{id}-post") }'),
    'the CD line');
  check('R4 the two wait bounds and the switch bound are written down',
    cd.includes('pub const RECEIPT_WAIT_MS: u64 = 90_000;') &&
    cd.includes('pub const POST_WAIT_MS: u64 = 90_000;') &&
    cd.includes('pub const SWITCH_WAIT_MS: u64 = 300_000;'));
  check('R5 the receipt is the new identity triple: generation AND fd/dup must change',
    cd.includes('pub struct Identity {') &&
    cd.includes('if receipt.identity.fd < 0 || receipt.identity.dup_fd < 0 { return refuse("invalid-fd"); }') &&
    cd.includes('if baseline.is_some_and(|base| base.generation == receipt.identity.generation) {') &&
    cd.includes('return refuse("generation-unchanged");') &&
    cd.includes('if baseline.is_some_and(|base| base.fd == receipt.identity.fd') &&
    cd.includes('&& base.dup_fd == receipt.identity.dup_fd)') &&
    cd.includes('return refuse("identity-unchanged");') &&
    cd.includes('measured.identity_at_switch = Some(identity);') &&
    // The REMOVED criterion stays removed: no CODE line may read the
    // process-global live registration counter, and no plan state may carry it
    // (the module docs may still NAME the rejected criterion).
    !cdCode.includes('live_seq') && !cdCode.includes('live_at_switch'),
    'the identity triple');
  check('R6 the pre/post HITs are the REAL tee\'s, not the shell\'s claim',
    cd.includes('pub fn control_hit(marker: &str, now: u64)') &&
    cd.includes('"unsat", "pre-hit-unobserved"') &&
    cd.includes('"unsat", "post-hit-unobserved"') &&
    wg.includes('crate::n2_cd::control_hit(marker, now);'));
  check('R7 the Wb row is no longer exempt: its receipt is the new generation\'s protect settle',
    cd.includes('if planned.kind == "Wb" && !receipt.protect_settle {') &&
    cd.includes('return refuse("wb-protect-settle-missing");') &&
    cd.includes('("exempted", "wb-protect-settle-success")') &&
    !cd.includes('wb-no-production-send') &&
    // The substitute receipt is fed at the CORE TAKEOVER (the validated dup),
    // bound to the adopted descriptor and the registration's OWN protect-settle
    // instant. A bare shell-stage protect feeds nothing: no core-owned
    // descriptor exists yet, so it could only hand out the shell's own number.
    rust('n2_socket_trace.rs').includes('pub fn adopted(from: RawFd, to: RawFd) {') &&
    rust('n2_socket_trace.rs').includes(
      'feed_cd_receipt(&meta, to, meta.protect_mono_ms, false, true, "protect_settle"') &&
    rust('wg_device.rs').includes('crate::n2_socket_trace::adopted(wg_socket_raw, fd);') &&
    !rust('n2_socket_trace.rs').includes('if kind == "Wb" && crate::n2_cd::awaiting_kind(kind) {'));

  // The core's own receipt criteria beyond the identity triple: the new
  // network, the real business write and the pre-HIT ordering.
  check('R9 the receipt refuses a stale network, a failed protect and an early I/O by name',
    cd.includes('if receipt.net_id != planned.net_id_after { return refuse("net-id-mismatch"); }') &&
    cd.includes('if !receipt.protect_ok { return refuse("protect-failed"); }') &&
    cd.includes('if receipt.io_mono_ms <= pre_hit_ms { return refuse("io-before-pre-hit"); }') &&
    cd.includes('if matches!(planned.kind.as_str(), "M" | "S" | "R") && !receipt.subsequent_io {') &&
    cd.includes('return refuse("connect-only");'));
  check('R10 the D column is pre-set on acceptance and only switches after prepare+ready',
    cd.includes('"D" => (Phase::AwaitStartBearer, None, "await-start-bearer", PREPARE_WAIT_MS),') &&
    cd.includes('return close(measured, "switch", "none", "unsat", "prepare-hop-required",') &&
    cd.includes('return close(measured, "switch", "none", "unsat", "dwell-min-not-reached",') &&
    cd.includes('pub const DWELL_MIN_MS: u64 = 3_000;') &&
    cd.includes('pub const PREPARE_WAIT_MS: u64 = 300_000;'));
  check('R8 unarmed is a silent no-op for every entry point',
    cd.includes('return silent("rejected", "not-armed", id);') &&
    cd.includes('pub fn reset() {'));
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
