// N2 first batch C/D columns — the app-internal evidence ring shell self-check
// (host-only, no device, no network, no build).
//
// The RING FILE itself is written by the core (`client/core/src/n2_ring.rs`,
// covered by its own Rust unit tests: durable `N2R|sess=…|seq=…|line=…`
// records, ACK-gated rotation, the lost/overflow spans and the manifest). This
// check covers the shell half:
//   * the REAL pure module (NetBirdN2Ring.ets, copied byte-for-byte) — the
//     SEQ-addressed `n2_ring_dump` request parser (the legacy line-index form
//     is refused), the explicit host ACK document parser (`session` +
//     `up_to_seq`, both required) and the `N2_RING_DUMP_SHELL` /
//     `N2_RING_ACK_SHELL` receipt shapes;
//   * the shipped wiring — the ring is configured by the accepted arm and
//     disabled on every arming change, its counters are reported every 60 s and
//     at teardown AFTER the connector/destroy settled, the shell's own line
//     families are forwarded verbatim, the dump channel brackets the page with
//     a manifest and re-emits only what the core answered WITHOUT advancing the
//     retrieved watermark, and the SEPARATE explicit ACK channel forwards the
//     host's `{"session":…,"up_to_seq":…}` document verbatim.
//
// Run: node client/entry/selfcheck/run-n2-ring.mts
// Exit 0 = all assertions pass.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ETS = join(HERE, '..', 'src', 'main', 'ets');
const REAL = join(ETS, 'vpnextensionability', 'NetBirdN2Ring.ets');
const ABILITY = join(ETS, 'vpnextensionability', 'NetBirdVpnExtensionAbility.ets');
const ENTRY = join(ETS, 'entryability', 'EntryAbility.ets');
const CORE_SRC = join(HERE, '..', '..', 'core', 'src');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-n2-ring-'));
const fixture: string = join(fixtureDir, 'NetBirdN2Ring.ts');
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
  const parse = mod['parseN2RingDump'] as (text: string) =>
    { request?: { fromSeq: number; toSeq: number; hasToSeq: boolean; count: number;
      session: string }; error: string };
  const parseAck = mod['parseN2RingAck'] as (text: string) =>
    { request?: { session: string; upToSeq: number }; error: string };
  const ackLine = mod['formatN2RingAckShellLine'] as (requestId: string, session: string,
    upToSeq: number, ackedSeq: number, advanced: boolean, result: string,
    reason: string) => string;
  const ackReject = mod['formatN2RingAckShellReject'] as (requestId: string,
    reason: string) => string;
  const shellLine = mod['formatN2RingDumpShellLine'] as (requestId: string, fromSeq: number,
    toSeq: number, count: number, emitted: number, ackWatermark: number, result: string,
    reason: string) => string;
  const shellReject = mod['formatN2RingDumpShellReject'] as (requestId: string,
    reason: string) => string;
  const fileName = mod['N2_RING_FILE_NAME'] as string;
  const dumpFileName = mod['N2_RING_DUMP_FILE_NAME'] as string;
  const ackFileName = mod['N2_RING_ACK_FILE_NAME'] as string;
  const maxBytes = mod['N2_RING_MAX_BYTES'] as number;
  const minBytes = mod['N2_RING_MIN_BYTES'] as number;
  const segments = mod['N2_RING_SEGMENTS'] as number;
  const maxLines = mod['N2_RING_MAX_DUMP_LINES'] as number;
  const maxRequest = mod['N2_RING_MAX_DUMP_REQUEST'] as number;
  const maxAckRequest = mod['N2_RING_MAX_ACK_REQUEST'] as number;
  const defaultCount = mod['N2_RING_DEFAULT_DUMP_COUNT'] as number;

  const ability = readText(ABILITY);
  const entry = readText(ENTRY);
  const rust = (name: string): string => readText(join(CORE_SRC, name));

  // ---------------- P: the pure module ------------------------------------
  check('P1 the ring identity and bounds are the documented literals',
    fileName === 'netbird-n2-evidence-ring.log' && dumpFileName === 'netbird-n2-ring-dump' &&
    ackFileName === 'netbird-n2-ring-ack' &&
    maxBytes === 4 * 1024 * 1024 && minBytes === 64 * 1024 && segments === 3 &&
    maxLines === 256 &&
    maxRequest === 512 && maxAckRequest === 512 && defaultCount === 64,
    JSON.stringify({ fileName, dumpFileName, ackFileName, maxBytes, minBytes, segments, maxLines,
      maxRequest, maxAckRequest, defaultCount }));

  // The request is SEQ-addressed: `from_seq` / `to_seq` / `count` / `session`.
  const explicit = parse('{"from_seq":12,"to_seq":40,"count":8,"session":"arm-1"}');
  check('P2 an explicit seq window is accepted verbatim (including its session filter)',
    explicit.error === '' && explicit.request?.fromSeq === 12 && explicit.request?.toSeq === 40 &&
    explicit.request?.hasToSeq === true && explicit.request?.count === 8 &&
    explicit.request?.session === 'arm-1',
    JSON.stringify(explicit));
  const bare = parse('{}');
  check('P3 every key is optional: the whole ring, the live tail and the default page size',
    bare.error === '' && bare.request?.fromSeq === 0 && bare.request?.toSeq === 0 &&
    bare.request?.hasToSeq === false && bare.request?.count === 64 &&
    bare.request?.session === '');
  check('P4 a `to_seq` of 0 is a REAL window while an absent one is its own bit',
    parse('{"to_seq":0}').request?.hasToSeq === true &&
    parse('{"to_seq":0}').request?.toSeq === 0 &&
    parse('{}').request?.hasToSeq === false);
  check('P5 a trailing newline / surrounding whitespace is harmless',
    parse('  {"from_seq":1}  \n').error === '');

  for (const [label, text, error] of [
    ['empty', '', 'empty-request'],
    ['whitespace only', '   ', 'empty-request'],
    ['not JSON', '{from_seq:1}', 'invalid-json'],
    ['JSON null', 'null', 'invalid-request'],
    ['JSON array', '[{"from_seq":1}]', 'invalid-request'],
    ['the LEGACY line-index form', '{"start":12,"count":8}', 'legacy-start-index-unsupported'],
    ['a bare legacy start key', '{"start":0}', 'legacy-start-index-unsupported'],
    ['negative from_seq', '{"from_seq":-1}', 'invalid-request'],
    ['fractional from_seq', '{"from_seq":1.5}', 'invalid-request'],
    ['non-numeric from_seq', '{"from_seq":"0"}', 'invalid-request'],
    ['negative to_seq', '{"to_seq":-1}', 'invalid-request'],
    ['non-numeric to_seq', '{"to_seq":"9"}', 'invalid-request'],
    ['zero count', '{"count":0}', 'invalid-count'],
    ['count over the bound', `{"count":${maxLines + 1}}`, 'invalid-count'],
    ['fractional count', '{"count":2.5}', 'invalid-count'],
    ['empty session', '{"session":""}', 'invalid-session'],
    ['non-string session', '{"session":3}', 'invalid-request'],
    ['oversized request', `{"from_seq":0,"pad":"${'y'.repeat(maxRequest)}"}`, 'request-too-long']
  ] as const) {
    const decision = parse(text);
    check(`P6 refuses ${label} (${error})`,
      decision.error === error && decision.request === undefined,
      `${decision.error} ${JSON.stringify(decision.request)}`);
  }

  // The explicit host ACK document: `session` AND `up_to_seq` are required —
  // an ACK without an arm could release another arm's segments, and one without
  // a watermark confirms nothing.
  const ackOk = parseAck('{"session":"auto-1-2-3","up_to_seq":42}');
  check('P7 an explicit ACK document is accepted with BOTH keys, verbatim',
    ackOk.error === '' && ackOk.request?.session === 'auto-1-2-3' &&
    ackOk.request?.upToSeq === 42,
    JSON.stringify(ackOk));
  check('P7 the dump module no longer carries the auto-ACK watermark reader',
    mod['n2RingChunkSeq'] === undefined && mod['formatN2RingAckShellLine'] !== undefined &&
    mod['parseN2RingAck'] !== undefined);

  for (const [label, text, error] of [
    ['empty', '', 'empty-request'],
    ['whitespace only', '   ', 'empty-request'],
    ['not JSON', '{session:1}', 'invalid-json'],
    ['JSON null', 'null', 'invalid-request'],
    ['JSON array', '[{"session":"a","up_to_seq":1}]', 'invalid-request'],
    ['a missing session', '{"up_to_seq":3}', 'invalid-session'],
    ['an empty session', '{"session":""}', 'invalid-session'],
    ['an empty session with a watermark', '{"session":"","up_to_seq":3}', 'invalid-session'],
    ['a non-string session', '{"session":3,"up_to_seq":1}', 'invalid-request'],
    ['a missing watermark', '{"session":"a"}', 'invalid-request'],
    ['a negative watermark', '{"session":"a","up_to_seq":-1}', 'invalid-request'],
    ['a fractional watermark', '{"session":"a","up_to_seq":1.5}', 'invalid-request'],
    ['a string watermark', '{"session":"a","up_to_seq":"1"}', 'invalid-request'],
    ['oversized request', `{"session":"a","up_to_seq":1,"pad":"${'y'.repeat(maxAckRequest)}"}`,
      'request-too-long']
  ] as const) {
    const decision = parseAck(text);
    check(`P7b refuses ${label} (${error})`,
      decision.error === error && decision.request === undefined,
      `${decision.error} ${JSON.stringify(decision.request)}`);
  }

  // The two dump receipt shapes: the watermark is REPORTED (as `ack_watermark`),
  // never claimed as confirmed.
  check('P8 the dump receipt names the window, the emitted count and the watermark',
    shellLine('req-1', 0, 40, 8, 3, 7, 'dumped', 'ok') ===
      'N2_RING_DUMP_SHELL|requestId=req-1|from_seq=0|to_seq=40|count=8|emitted=3|ack_watermark=7|' +
      'result=dumped|reason=ok|diag=true|is_evidence=false',
    shellLine('req-1', 0, 40, 8, 3, 7, 'dumped', 'ok'));
  check('P9 a refused dump names its reason, invents no window and reports no watermark',
    shellReject('req-2', 'legacy-start-index-unsupported') ===
      'N2_RING_DUMP_SHELL|requestId=req-2|from_seq=0|to_seq=0|count=0|emitted=0|ack_watermark=-1|' +
      'result=rejected|reason=legacy-start-index-unsupported|diag=true|is_evidence=false' &&
    shellLine('req|3', 0, 1, 1, 1, 0, 'dumped', 'ok').indexOf('requestId=req_3') > 0,
    shellReject('req-2', 'legacy-start-index-unsupported'));

  // The ACK receipt: `advanced=true` is the ONLY shape that reports a moved
  // watermark; a rejected one reports the untouched watermark and the refusal.
  check('P10 the ACK receipt names the arm, the request and whether it advanced',
    ackLine('req-4', 'auto-1-2-3', 42, 42, true, 'acked', 'ok') ===
      'N2_RING_ACK_SHELL|requestId=req-4|sess=auto-1-2-3|up_to_seq=42|acked_seq=42|advanced=true|' +
      'result=acked|reason=ok|diag=true|is_evidence=false' &&
    ackLine('req-4', 'auto-1-2-3', 42, 7, false, 'rejected', 'session-mismatch') ===
      'N2_RING_ACK_SHELL|requestId=req-4|sess=auto-1-2-3|up_to_seq=42|acked_seq=7|advanced=false|' +
      'result=rejected|reason=session-mismatch|diag=true|is_evidence=false',
    ackLine('req-4', 'auto-1-2-3', 42, 42, true, 'acked', 'ok'));
  check('P11 a refused ACK document names its reason and reports no arm or advancement',
    ackReject('req-5', 'invalid-session') ===
      'N2_RING_ACK_SHELL|requestId=req-5|sess=-|up_to_seq=-1|acked_seq=-1|advanced=false|' +
      'result=rejected|reason=invalid-session|diag=true|is_evidence=false' &&
    ackReject('req|6', 'empty-request').indexOf('requestId=req_6') > 0,
    ackReject('req-5', 'invalid-session'));

  // ---------------- S: the shipped wiring --------------------------------
  check('S1 the extension imports the ring pure module and its seq-paged shapes',
    ability.includes("} from './NetBirdN2Ring';") && ability.includes('N2_RING_FILE_NAME') &&
    ability.includes('N2_RING_DUMP_FILE_NAME') && ability.includes('N2_RING_ACK_FILE_NAME') &&
    ability.includes('N2RingDumpDecision') &&
    ability.includes('N2RingDumpRequest') && ability.includes('parseN2RingDump') &&
    ability.includes('formatN2RingDumpShellLine') && ability.includes('formatN2RingDumpShellReject') &&
    ability.includes('N2RingAckDecision') && ability.includes('N2RingAckRequest') &&
    ability.includes('parseN2RingAck') && ability.includes('formatN2RingAckShellLine') &&
    ability.includes('formatN2RingAckShellReject') &&
    !ability.includes('n2RingChunkSeq'),
    'import block');
  check('S2 the ring is configured with the documented path/segments by the armed arm, and its ' +
    'cap comes from the accepted ticket (absent ⇒ the documented 4 MiB default)',
    ability.includes('const path: string = `${this.context.filesDir}/${N2_RING_FILE_NAME}`;') &&
    ability.includes('const segmentBytes: number = this.n2Evidence !== undefined ?\n' +
      '      this.n2Evidence.ringSegmentBytes : N2_RING_MAX_BYTES;') &&
    ability.includes('core.n2_ring_configure(path, segmentBytes, N2_RING_SEGMENTS)') &&
    ability.includes('      this.n2RingConfigure();'),
    'configure');
  check('S2b the diagnostic shrink key is validated by the SAME fail-closed ticket parser',
    (() => {
      const evidence = readText(join(ETS, 'vpnextensionability', 'NetBirdN2Evidence.ets'));
      return evidence.includes("import { N2_RING_MAX_BYTES, N2_RING_MIN_BYTES } from './NetBirdN2Ring';") &&
        evidence.includes("const rawRing: Object | undefined = input['ring_segment_bytes'];") &&
        evidence.includes("return { error: 'invalid-ring-segment-bytes' };") &&
        evidence.includes('(rawRing as number) < N2_RING_MIN_BYTES || (rawRing as number) > N2_RING_MAX_BYTES') &&
        evidence.includes('ringSegmentBytes: ringSegmentBytes } };');
    })(),
    'the ticket key');
  check('S3 every arming change stops the ring AND its 60 s cadence (the FILE survives)',
    ability.includes('    core.n2_ring_disable();\n    this.n2RingStopStats();\n' +
      '    this.n2CdClearLatch();') &&
    ability.includes('    this.n2CdActive = \'\';'));
  check('S4 the shell forwards its own line families verbatim (one string, no drift)',
    ability.includes('  private n2RingAppend(line: string): void {') &&
    ability.includes("    if (this.n2Evidence === undefined) {\n      return;\n    }") &&
    ability.includes('      core.n2_ring_append(line);') &&
    ability.includes('  private n2WarnAndRing(line: string): void {') &&
    ability.includes('  private n2ErrorAndRing(line: string): void {'));
  const forwarded: string[] = ['VPN_NETWORK_BASELINE', 'VPN_NETWORK_EVENT',
    'VPN_NETWORK_RECOVERED', 'VPN_NETWORK_RECOVERY_FAIL', 'VPN_WG_DATAPLANE_FED',
    'N2_EVIDENCE_ARMED', 'N2_EVIDENCE_ROUTES', 'N2_EVIDENCE_RECREATE_DETECTED',
    'N2_SOCK|phase=open', 'N2_SOCK|phase=protect_begin', 'N2_SOCK|phase=protect_settle',
    'N2_PROC_NET', 'N2_RING_CONFIGURE_FAILED', 'N2_RING_DUMP_READ_FAILED',
    'N2_RING_DUMP_CONSUME_FAILED', 'N2_CD_PLAN_CONSUME_FAILED', 'N2_SANDWICH_POST_FAILED'];
  for (const family of forwarded) {
    check(`S5 the shell forwards a ${family} line to the ring`,
      ability.includes(family) && (ability.includes('this.n2RingAppend(') ||
        ability.includes('this.n2WarnAndRing(') || ability.includes('this.n2ErrorAndRing(')),
      family);
  }
  check('S6 the ring counters are reported AFTER the whole teardown settled',
    (() => {
      const destroy = ability.slice(ability.indexOf('  onDestroy(): void {'));
      const stopStats = destroy.indexOf('    this.n2RingStopStats();');
      const connectorStop = destroy.indexOf('    const stop = connectorStop();');
      const destroyOnce = destroy.indexOf("    this.destroyOnce('onDestroy')");
      const status = destroy.indexOf('        core.n2_ring_status();');
      const disable = destroy.indexOf('        core.n2_ring_disable();');
      return stopStats > 0 && connectorStop > stopStats && destroyOnce > connectorStop &&
        status > destroyOnce && disable > status;
    })(),
    'the onDestroy order');
  check('S7 the ring emits the core\'s own N2_RING_STATS line every 60 s while armed',
    ability.includes('const N2_RING_STATS_MS: number = 60000;') &&
    ability.includes('    this.n2RingStatsTimer = setInterval((): void => {') &&
    ability.includes('        core.n2_ring_status();') &&
    ability.includes('    }, N2_RING_STATS_MS);') &&
    ability.includes('    this.n2RingStartStats();') &&
    ability.includes('  private n2RingStopStats(): void {'));

  // The FETCH fallback: the CORE owns the file and the pagination.
  check('S8 the dump brackets the page with a manifest and forwards the document verbatim',
    ability.indexOf('        core.n2_ring_manifest();') > 0 &&
    ability.indexOf('        core.n2_ring_manifest();') <
      ability.indexOf('core.n2_ring_dump(text.trim())') &&
    ability.includes('page = JSON.parse(core.n2_ring_dump(text.trim())) as N2RingDumpPage;'));
  check('S9 the chunk records and the summary go to HiLog ONLY (never back into the ring)',
    ability.includes('      for (let i = 0; i < records.length; i++) {\n' +
      "        hilog.info(DOMAIN, TAG, '%{public}s', records[i]);\n      }") &&
    ability.includes("        hilog.info(DOMAIN, TAG, '%{public}s', page.summary);") &&
    ability.includes('      this.n2RingAppend(line);') &&
    ability.indexOf('this.n2RingAppend(page.summary)') < 0 &&
    ability.indexOf('this.n2RingAppend(records') < 0);
  check('S10 the dump NEVER advances the ACK: it mirrors the watermark and calls no ACK',
    ability.includes("      if (page.result === 'dumped' && typeof page.acked_seq === 'number') {") &&
    ability.includes('        ackWatermark = page.acked_seq;') &&
    ability.includes('request.count, records.length, ackWatermark,') &&
    (() => {
      const dump = ability.slice(ability.indexOf('  private pollN2RingDump(): void {'),
        ability.indexOf('  private pollN2RingAck(): void {'));
      return dump.length > 0 && dump.indexOf('core.n2_ring_ack') < 0;
    })(),
    'the dump path');
  check('S11 a torn / legacy / oversized document is refused by name and still consumed',
    ability.includes('const reject: string = formatN2RingDumpShellReject(this.requestId, decision.error);') &&
    ability.includes('        this.n2RingAppend(reject);') &&
    ability.includes("const failed: string = formatN2RingDumpShellLine(this.requestId, request.fromSeq,") &&
    ability.includes("          wantedToSeq, request.count, 0, -1, 'rejected', 'core-throw');"));
  check('S12 the ring channels ride the watcher tick BEFORE the plan polls',
    ability.indexOf('      this.pollN2RingDump();') > 0 &&
    ability.indexOf('      this.pollN2RingAck();') > ability.indexOf('      this.pollN2RingDump();') &&
    ability.indexOf('      this.pollN2CdPlan();') > ability.indexOf('      this.pollN2RingAck();'));
  check('S13 EntryAbility carries the n2_ring_dump parameter and the shared basename',
    entry.includes("const N2_RING_DUMP_PARAM: string = 'n2_ring_dump';") &&
    entry.includes("const N2_RING_DUMP_FILE: string = 'netbird-n2-ring-dump';") &&
    entry.includes('params[N2_RING_DUMP_PARAM];') &&
    entry.includes('this.writeRingDumpFile(ringDumpText);') &&
    entry.includes("this.removeRingDumpFile('default-no-ring-dump');") &&
    entry.includes("this.removeRingDumpFile('combo-rejected');") &&
    entry.includes("this.removeN2RequestFile(marker, name, 'failed-write-cleanup');") &&
    entry.includes("this.writeN2RequestFile('N2_RING_DUMP_FILE', N2_RING_DUMP_FILE, 'no-ring-dump', text);"),
    'entry constants');
  check('S13b EntryAbility carries the n2_ring_ack parameter and the shared basename',
    entry.includes("const N2_RING_ACK_PARAM: string = 'n2_ring_ack';") &&
    entry.includes("const N2_RING_ACK_FILE: string = 'netbird-n2-ring-ack';") &&
    entry.includes('params[N2_RING_ACK_PARAM];') &&
    entry.includes('this.writeRingAckFile(ringAckText);') &&
    entry.includes("this.removeRingAckFile('default-no-ring-ack');") &&
    entry.includes("this.removeRingAckFile('combo-rejected');") &&
    entry.includes("this.writeN2RequestFile('N2_RING_ACK_FILE', N2_RING_ACK_FILE, 'no-ring-ack', text);"),
    'entry ack constants');
  check('S14 the fetch path (bundle transfer, then this fallback) is documented at the call site',
    ability.includes('hdc file recv -b') && ability.includes('E005004') &&
    ability.includes('n2_ring_dump` private channel'));
  check('S15 the explicit ACK channel forwards the document verbatim and mirrors the core',
    ability.includes('  private pollN2RingAck(): void {') &&
    ability.includes('`${this.context.filesDir}/${N2_RING_ACK_FILE_NAME}`') &&
    ability.includes('const decision: N2RingAckDecision = parseN2RingAck(text);') &&
    ability.includes('const request: N2RingAckRequest = decision.request;') &&
    ability.includes('reply = JSON.parse(core.n2_ring_ack(text.trim())) as N2RingAckReply;') &&
    ability.includes('const accepted: boolean = reply.ok === true && reply.result === ') &&
    ability.includes('accepted && reply.advanced === true,') &&
    ability.includes("this.n2WarnAndRing(`N2_RING_ACK_CONSUME_FAILED|requestId=${this.requestId}|`") &&
    ability.includes("formatN2RingAckShellReject(this.requestId, decision.error)") &&
    ability.includes('      this.pollN2RingAck();'));
  check('S16 the ACK receipt is written back into the ring and the document consumed once',
    (() => {
      const ack = ability.slice(ability.indexOf('  private pollN2RingAck(): void {'),
        ability.indexOf('  // S6: the capture route'));
      return ack.length > 0 && ack.indexOf('this.n2RingAppend(line);') > 0 &&
        ack.indexOf('fs.unlinkSync(path);') > 0 && ack.indexOf('fs.accessSync(path)') > 0;
    })(),
    'the ack channel');

  // ---------------- H: the loader mocks stay in sync ---------------------
  for (const hookName of ['exclusion-generation-hooks.mjs', 'relay-exclusion-hooks.mjs']) {
    const hook = readText(join(HERE, hookName));
    check(`H1 ${hookName} stubs the seven ring seams and the current CD seams`,
      hook.includes('n2_ring_configure:') && hook.includes('n2_ring_append:') &&
      hook.includes('n2_ring_disable:') && hook.includes('n2_ring_status:') &&
      hook.includes('n2_ring_dump:') && hook.includes('n2_ring_manifest:') &&
      hook.includes('n2_ring_ack:') && hook.includes('n2_cd_prepare:') &&
      hook.includes('n2_cd_ready:') && hook.includes('n2_cd_identity:') &&
      hook.includes('n2_cd_switch_identified:') &&
      !hook.includes('n2_cd_switch:'), hookName);
    check(`H2 ${hookName} resolves the ring pure module to its staged shim`,
      hook.includes("'./NetBirdN2Ring': 'NetBirdN2Ring.shim.ts'"), hookName);
  }
  for (const runner of ['run-exclusion-generation.mts', 'run-relay-exclusion.mts',
    'run-exclusion-addr-shape.mts']) {
    const text = readText(join(HERE, runner));
    check(`H3 ${runner} stages the ring pure module and its erased types`,
      text.includes("'NetBirdN2Ring'];") &&
      text.includes("NetBirdN2Ring: ['N2RingDumpDecision', 'N2RingDumpRequest',") &&
      text.includes("'N2RingAckDecision', 'N2RingAckRequest']"), runner);
  }

  // ---------------- R: the core ring surface -----------------------------
  const ring = rust('n2_ring.rs');
  const hilog = rust('hilog.rs');
  check('R1 the tee lives in hilog::emit, so every core line is recorded',
    hilog.includes('    crate::n2_ring::append(line);'), 'hilog tee');
  check('R2 the ring is inert until configured (one atomic load on the hot path)',
    ring.includes('static ENABLED: AtomicBool = AtomicBool::new(false);') &&
    ring.includes('if line.is_empty() || !enabled() {\n        return;\n    }'));
  check('R3 the cap, the segments and the rotation are documented constants',
    ring.includes('pub const DEFAULT_MAX_BYTES: u64 = 4 * 1024 * 1024;') &&
    ring.includes('pub const DEFAULT_SEGMENTS: u32 = 3;') &&
    ring.includes('pub const DEFAULT_FILE_NAME: &str = "netbird-n2-evidence-ring.log";') &&
    ring.includes('fn rotate_current_segment(state: &mut State) -> Result<RotatedInfo, String> {'));
  check('R3b a shrunk cap is announced by its own line and the default arm emits nothing extra',
    ring.includes('const MIN_MAX_BYTES: u64 = 64 * 1024;') &&
    ring.includes('pub fn segment_line(segment_bytes: u64, mono_ms: u64) -> String {') &&
    ring.includes('N2_RING_SEGMENT_BYTES|segment_bytes={segment_bytes}|default_bytes={DEFAULT_MAX_BYTES}|mono_ms={mono_ms}|diag=true|is_evidence=false') &&
    ring.includes('    if max_bytes != DEFAULT_MAX_BYTES {\n' +
      '        hilog::emit(&segment_line(max_bytes, sys::mono_ms()));\n    }'));
  check('R4 write failures are counted, the first is announced and the recovery is reported',
    ring.includes('fn announce_failure(code: &str) {') &&
    ring.includes('fn announce_recovery() {') &&
    ring.includes('pub fn fail_line(failures: u64, code: &str, mono_ms: u64) -> String {') &&
    ring.includes('pub fn recovered_line(failures: u64, mono_ms: u64) -> String {'));
  check('R5 the failure/recovery paths cannot recurse into the writer',
    ring.includes('static IN_APPEND: Cell<bool> = const { Cell::new(false) };') &&
    ring.includes('if IN_APPEND.with(|flag| flag.get()) {\n        return;\n    }'));
  check('R6 the counters are reported as one N2_RING_STATS line',
    ring.includes('pub fn stats_line(file: &str, enabled: bool, bytes: u64, max_bytes: u64, segments: u32,') &&
    ring.includes('N2_RING_STATS|file={}|enabled={enabled}|bytes={bytes}|max_bytes={max_bytes}|segments={segments}|writes={writes}|failures={failures}|rotations={rotations}|last_error={}'));

  // B4: the durable record and the seq-addressed retrieval.
  check('R7 one persisted record carries the session and the durable monotonic seq',
    ring.includes('pub const RECORD_PREFIX: &str = "N2R|";') &&
    ring.includes('format!("{RECORD_PREFIX}sess={}|seq={seq}|mono_ms={mono_ms}|wall_ms={wall_ms}|line={}",') &&
    ring.includes('pub const DEFAULT_DUMP_COUNT: usize = 64;') &&
    ring.includes('pub const MAX_DUMP_LINES: usize = 256;'));
  check('R8 the legacy line-index document is refused by name (never pretended onto a seq)',
    ring.includes('if entries.iter().any(|(key, _)| key == "start") {') &&
    ring.includes('return Err("legacy-start-index-unsupported");') &&
    ring.includes('let from_seq = json_u64(&entries, "from_seq")?.unwrap_or(0);') &&
    ring.includes('let to_seq = json_u64(&entries, "to_seq")?;') &&
    ring.includes('return Err("invalid-session");'));
  check('R9 the page is served as chunk records plus one summary, with the malformed/lost facts',
    ring.includes('pub fn chunk_line(seq: u64, of: usize, line: &str) -> String {') &&
    ring.includes('format!("N2_RING_CHUNK|seq={seq}|of={of}|line={line}")') &&
    ring.includes('pub fn dump_summary_line(page: &DumpPage, emitted: usize) -> String {') &&
    ring.includes('pub fn dump_lines(request_json: &str) -> Vec<String> {'));
  check('R10 rotation reports the dropped SPAN (not a bare counter) and is ACK-gated',
    ring.includes('pub fn rotated_span_line(segments: u32, bytes: u64, dropped_from_seq: u64, dropped_to_seq: u64,') &&
    ring.includes('dropped_from_seq={dropped_from_seq}|dropped_to_seq={dropped_to_seq}|dropped_records={dropped_records}|acked_seq={acked_seq}') &&
    ring.includes('fn can_rotate(state: &State) -> bool {'));
  check('R10b the ACK is EXPLICIT, session-bound, armed-only, idempotent and range-checked',
    ring.includes("pub fn ack_seq(up_to_seq: u64, session: &str) -> Result<AckOutcome, &'static str> {") &&
    ring.includes('let mut state = state().ok_or("not-configured")?;') &&
    ring.includes('        return Err("not-armed");') &&
    ring.includes('        return Err("session-mismatch");') &&
    ring.includes('if up_to_seq > state.last_seq {\n        return Err("seq-beyond-tail");\n    }') &&
    ring.includes('state.acked_seq = state.acked_seq.max(up_to_seq);') &&
    ring.includes('pub fn parse_ack_request(text: &str) -> Result<AckRequest, &\'static str> {') &&
    ring.includes('let up_to_seq = match json_u64(&entries, "up_to_seq")? {') &&
    ring.includes('None => return Err("invalid-session"),'));
  check('R10c the dump/manifest/status surfaces cannot move the watermark',
    ring.includes('pub fn dump_page(request_json: &str) -> DumpPage {') &&
    ring.includes('pub fn manifest_line_now() -> String {') &&
    ring.includes('pub fn status_line_now() -> String {') &&
    (() => {
      const dump = ring.slice(ring.indexOf('pub fn dump_page(request_json: &str) -> DumpPage {'),
        ring.indexOf('/// The seq-paged dump as `N2_RING_CHUNK` records'));
      return dump.length > 0 && dump.indexOf('acked_seq =') < 0 && dump.indexOf('ack_seq(') < 0;
    })());
  check('R11 the manifest, the lost span and the overflow span are in-ring baseline lines',
    ring.includes('N2_RING_MANIFEST|file={}|sess={}|enabled={}|segments={}|max_bytes={}|bytes={}|records={}|first_seq={}|last_seq={}|frozen_tail_seq={}|next_seq={}|acked_seq={}') &&
    ring.includes('N2_RING_LOST|from_seq={from_seq}|to_seq={to_seq}|records={records}|reason={}|code={}|mono_ms={mono_ms}|') &&
    ring.includes('N2_RING_OVERFLOW|dropped_records={dropped_records}|from_seq={from_seq}|to_seq={to_seq}|acked_seq={acked_seq}|mono_ms={mono_ms}|'));
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
