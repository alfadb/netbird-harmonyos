/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host-side checks for the system-event watcher (byte-identical import of
// client/entry/src/main/ets/vpnextensionability/NetBirdSystemEventsCore.ets,
// plus text pins on NetBirdSystemEvents.ets).
// Run: node client/entry/selfcheck/run-system-events.mts
// Exit 0 = all assertions pass.
//
// Pinned here: the purity of the core, the SDK event names it watches, the
// params whitelist (only known fields, whole params never stringified, secret
// values redacted, ignored subtrees never leak), the explicit known-vs-missing
// kill reason, the input and line bounds, the dedup window semantics, and the
// runtime's subscription/bounds/no-timer/no-permission shape.
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const ETS = join(here, '..', 'src', 'main', 'ets', 'vpnextensionability');
const coreSource = join(ETS, 'NetBirdSystemEventsCore.ets');
const runtimeSource = join(ETS, 'NetBirdSystemEvents.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-sysevents-'));
const TS = '2026-10-06T12:00:00.000Z';
const PID = 42;
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) {
    console.log(`     ${detail}`);
  }
}
function fieldsOf(line: string): Map<string, string> {
  const out = new Map<string, string>();
  for (const part of line.trim().split(' ')) {
    const at = part.indexOf('=');
    if (at > 0) { out.set(part.slice(0, at), part.slice(at + 1)); }
  }
  return out;
}
// Code-shape checks must ignore comments (the files DOCUMENT the very things
// they must not DO), so the pins below run on comment-free source.
function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/^\s*\/\/.*$/gm, ' ');
}
try {
  const text = readFileSync(coreSource, 'utf8');
  const runtimeText = readFileSync(runtimeSource, 'utf8');
  const code = stripComments(text);
  const runtimeCode = stripComments(runtimeText);

  // ---- core purity ---------------------------------------------------------
  check('core module is pure (never imports a platform module)', !/@ohos/.test(code));
  check('core module has no import/require statement',
    !/^\s*import\s/m.test(code) && !/\brequire\s*\(/.test(code));
  check('core never stringifies anything (no JSON.stringify)', !/JSON\.stringify/.test(code));
  check('core uses no any/unknown (ArkTS-safe syntax)', !/:\s*any\b/.test(code) && !/\bunknown\b/.test(code));

  const fixture = join(dir, 'NetBirdSystemEventsCore.ts');
  writeFileSync(fixture, text);
  const m = await import(pathToFileURL(fixture).href);

  // ---- SDK names / bounds --------------------------------------------------
  check('bounds are the documented ones',
    m.MAX_LINE_CHARS === 512 && m.MAX_VALUE_CHARS === 48 && m.MAX_EVENT_NAME_CHARS === 40 &&
    m.MAX_DETAIL_INPUT_CHARS === 2048 && m.MAX_PARAMS_INPUT_CHARS === 4096 &&
    m.MAX_INFOS_PER_GROUP === 16 && m.MAX_GROUPS_PER_CALLBACK === 8 && m.DEDUP_WINDOW === 64,
    `${m.MAX_LINE_CHARS}/${m.MAX_VALUE_CHARS}/${m.MAX_DETAIL_INPUT_CHARS}/${m.DEDUP_WINDOW}`);
  check('explicit tokens: na / redacted',
    m.UNAVAILABLE === 'na' && m.REDACTED === 'redacted' && m.APP_KILLED_NAME === 'APP_KILLED');
  // The literal values the SDK uses for hiAppEvent.event.* (APP_CRASH 11,
  // APP_FREEZE 11, RESOURCE_OVERLIMIT 12, APP_KILLED 20).
  check('watched event names are the exact SDK string values',
    m.WATCHED_EVENT_NAMES.join(',') === 'APP_CRASH,APP_FREEZE,RESOURCE_OVERLIMIT,APP_KILLED',
    m.WATCHED_EVENT_NAMES.join(','));
  check('isWatchedEvent accepts exactly those four',
    m.WATCHED_EVENT_NAMES.every((n: string) => m.isWatchedEvent(n) === true) &&
    m.isWatchedEvent('USER_LOGIN') === false && m.isWatchedEvent('APP_LAUNCH') === false &&
    m.isWatchedEvent('') === false && m.isWatchedEvent('app_killed') === false);
  check('log names are the sys_ prefix + lowercase SDK name',
    m.logEventName('APP_KILLED') === 'sys_app_killed' &&
    m.logEventName('RESOURCE_OVERLIMIT') === 'sys_resource_overlimit');

  // ---- exact projection: top-level kill fields -----------------------------
  const killed = {
    reason: 'LowMemoryKill', time: 1760000000000, pid: '52036',
    process_name: 'com.example.netbird', foreground: true
  };
  check('exact APP_KILLED line (ev_ keys sorted, reason doubles as kill_reason, no foreground)',
    m.buildSystemEventLine(TS, PID, 'vpn', m.logEventName('APP_KILLED'),
      m.buildEventFields('APP_KILLED', killed)) ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=vpn event=sys_app_killed' +
    ' ev_kill_reason=LowMemoryKill ev_pid=52036 ev_process_name=com.example.netbird' +
    ' ev_reason=LowMemoryKill ev_time=1760000000000\n',
    m.buildSystemEventLine(TS, PID, 'vpn', m.logEventName('APP_KILLED'),
      m.buildEventFields('APP_KILLED', killed)));

  // ---- last_exit_detail_info (API 26 JSON string) --------------------------
  const detailDoc = JSON.stringify({
    exit_msg: 'THREAD_BLOCK_6S', kill_reason: 'ThreadBlock6S', pid: '52036',
    process_name: 'com.example.netbird', process_state: 'foreground',
    pss: '123456', rss: '234567', timestamp: '1760000000123', uid: '10086'
  });
  const detailLine = m.buildSystemEventLine(TS, PID, 'vpn', 'sys_app_killed',
    m.buildEventFields('APP_KILLED', { last_exit_detail_info: detailDoc }));
  const detail = fieldsOf(detailLine);
  check('detail document projects every whitelisted subfield',
    detail.get('ev_detail_ts') === '1760000000123' && detail.get('ev_exit_msg') === 'THREAD_BLOCK_6S' &&
    detail.get('ev_kill_reason') === 'ThreadBlock6S' && detail.get('ev_pid') === '52036' &&
    detail.get('ev_process_name') === 'com.example.netbird' &&
    detail.get('ev_process_state') === 'foreground' && detail.get('ev_pss_kb') === '123456' &&
    detail.get('ev_rss_kb') === '234567',
    detailLine);
  check('detail document keys outside the whitelist are never emitted (uid)',
    detail.get('uid') === undefined && detailLine.indexOf('10086') < 0 &&
    detailLine.indexOf('uid') < 0);
  check('detail document is never emitted raw', detailLine.indexOf('{') < 0 && detailLine.indexOf('\"') < 0);

  // ---- known vs missing kill reason ----------------------------------------
  const missing = fieldsOf(m.buildSystemEventLine(TS, PID, 'vpn', 'sys_app_killed',
    m.buildEventFields('APP_KILLED', { time: 5 })));
  check('APP_KILLED with no reason reports ev_kill_reason=na (never absent)',
    missing.get('ev_kill_reason') === 'na');
  const missingNoParams = fieldsOf(m.buildSystemEventLine(TS, PID, 'vpn', 'sys_app_killed',
    m.buildEventFields('APP_KILLED', null)));
  check('APP_KILLED with no params at all still reports ev_kill_reason=na',
    missingNoParams.get('ev_kill_reason') === 'na');
  check('a non-kill event does not invent a kill reason',
    fieldsOf(m.buildSystemEventLine(TS, PID, 'ui', 'sys_app_crash',
      m.buildEventFields('APP_CRASH', { time: 9, pid: 3 }))).get('ev_kill_reason') === undefined);

  // ---- APP_FREEZE: nested memory, arrays/objects ignored -------------------
  const freeze = {
    time: 7, pid: 99, process_name: 'com.example.netbird',
    memory: { rss: 111, pss: 222, vss: 333, sys_free_mem: 444 },
    hilog: ['SECRETLOG'], threads: [{ frames: ['SECRETFRAME'] }],
    exception: { name: 'APPFREEZE', message: 'SECRETMESSAGE' },
    external_log: ['/data/log/SECRETPATH'], page_switch_log: 'SECRETPAGE'
  };
  const freezeLine = m.buildSystemEventLine(TS, PID, 'vpn', 'sys_app_freeze', m.buildEventFields('APP_FREEZE', freeze));
  const freezeFields = fieldsOf(freezeLine);
  check('APP_FREEZE reads rss/pss from the nested memory object only',
    freezeFields.get('ev_rss_kb') === '111' && freezeFields.get('ev_pss_kb') === '222' &&
    freezeFields.get('ev_pid') === '99' && freezeFields.get('ev_time') === '7');
  const overlimit = fieldsOf(m.buildSystemEventLine(TS, PID, 'vpn', 'sys_resource_overlimit',
    m.buildEventFields('RESOURCE_OVERLIMIT', {
      time: 11, pid: 5, resource_type: 'pss_memory', memory: { rss: 12, pss: 34, limit_size: 99 },
      fd: { num: 3, top_fd_type: 'SECRETFD' }, external_log: ['/data/log/SECRETRO']
    })));
  check('RESOURCE_OVERLIMIT projects its resource_type and nested rss/pss, nothing else',
    overlimit.get('ev_resource_type') === 'pss_memory' && overlimit.get('ev_rss_kb') === '12' &&
    overlimit.get('ev_pss_kb') === '34' && overlimit.get('ev_pid') === '5' &&
    [...overlimit.keys()].filter((k) => k.startsWith('ev_')).sort().join(',') ===
      'ev_pid,ev_pss_kb,ev_resource_type,ev_rss_kb,ev_time' &&
    m.buildSystemEventLine(TS, PID, 'vpn', 'sys_resource_overlimit',
      m.buildEventFields('RESOURCE_OVERLIMIT', { resource_type: 'fd', fd: { num: 3 } }))
      .indexOf('SECRETFD') < 0 &&
    m.buildSystemEventLine(TS, PID, 'vpn', 'sys_resource_overlimit',
      m.buildEventFields('RESOURCE_OVERLIMIT', { external_log: ['/data/log/SECRETRO'] }))
      .indexOf('SECRETRO') < 0,
    m.buildSystemEventLine(TS, PID, 'vpn', 'sys_resource_overlimit',
      m.buildEventFields('RESOURCE_OVERLIMIT', { time: 11, pid: 5, resource_type: 'pss_memory', memory: { rss: 12, pss: 34 } })));
  check('ignored subtrees never leak (hilog/threads/exception/external_log/page_switch_log/vss)',
    freezeLine.indexOf('SECRET') < 0 && freezeLine.indexOf('SECRETPATH') < 0 &&
    freezeFields.get('vss') === undefined && freezeFields.get('sys_free_mem') === undefined &&
    freezeFields.get('external_log') === undefined && freezeFields.get('page_switch_log') === undefined);

  // ---- secret exclusion ----------------------------------------------------
  const opaque = fieldsOf(m.buildSystemEventLine(TS, PID, 'ui', 'sys_app_killed',
    m.buildEventFields('APP_KILLED', { reason: 'a1b2c3d4e5f60718293a4b5c6d7e8f90' })));
  check('a credential-shaped known value is redacted, not logged',
    opaque.get('ev_reason') === 'redacted' && opaque.get('ev_kill_reason') === 'redacted');
  const secretKeys = m.buildSystemEventLine(TS, PID, 'ui', 'sys_app_crash', m.buildEventFields('APP_CRASH', {
    time: 1, setup_key: 'sk-live-SECRETVALUE', access_token: 'eyJhbGciOiSECRETJWT',
    Authorization: 'Bearer SECRETBEARER', reason: 'ok'
  }));
  check('unknown secret-named keys are dropped with their values',
    secretKeys.indexOf('setup_key') < 0 && secretKeys.indexOf('access_token') < 0 &&
    secretKeys.indexOf('Authorization') < 0 && secretKeys.indexOf('SECRET') < 0 &&
    secretKeys.indexOf('ev_reason=ok') > 0);

  // ---- input and line bounds ----------------------------------------------
  // 'R.' is deliberately not an opaque credential run, so the length bound is
  // what cuts it (a pure alphanumeric run is covered by the redaction check).
  const longRaw = 'R.'.repeat(150);
  const longValue = fieldsOf(m.buildSystemEventLine(TS, PID, 'ui', 'sys_app_killed',
    m.buildEventFields('APP_KILLED', { reason: longRaw })));
  check('a single value is cut at MAX_VALUE_CHARS',
    longRaw.length > m.MAX_VALUE_CHARS && longValue.get('ev_reason') === 'R.'.repeat(m.MAX_VALUE_CHARS / 2) &&
    longValue.get('ev_reason')!.length === m.MAX_VALUE_CHARS,
    longValue.get('ev_reason'));
  const wideValue = 'V.'.repeat(24);
  const wide = m.buildSystemEventLine(TS, PID, 'vpn', 'sys_app_killed', m.buildEventFields('APP_KILLED', {
    time: 1760000000000, reason: wideValue, pid: wideValue, process_name: wideValue,
    process_state: wideValue, rss: wideValue, pss: wideValue
  }));
  check('a line longer than MAX_LINE_CHARS is cut and marked with ~',
    wide.length === m.MAX_LINE_CHARS && wide.endsWith('~\n'), `${wide.length}`);
  const oversized = fieldsOf(m.buildSystemEventLine(TS, PID, 'vpn', 'sys_app_killed',
    m.buildEventFields('APP_KILLED', { last_exit_detail_info: 'x'.repeat(m.MAX_DETAIL_INPUT_CHARS + 1) })));
  check('an oversized detail document is not parsed and is marked truncated',
    oversized.get('ev_detail_truncated') === 'true' && oversized.get('ev_kill_reason') === 'na' &&
    oversized.get('ev_exit_msg') === undefined);
  const brokenDetail = fieldsOf(m.buildSystemEventLine(TS, PID, 'vpn', 'sys_app_killed',
    m.buildEventFields('APP_KILLED', { last_exit_detail_info: '{not json' })));
  check('an unparsable detail document yields no fields and no truncation claim',
    brokenDetail.get('ev_kill_reason') === 'na' && brokenDetail.get('ev_exit_msg') === undefined &&
    brokenDetail.get('ev_detail_truncated') === undefined);
  const hugeParams = 'x'.repeat(m.MAX_PARAMS_INPUT_CHARS + 1);
  check('a params object arriving as an oversized JSON string is refused, not parsed',
    m.toFieldMap(hugeParams) === null &&
    fieldsOf(m.buildSystemEventLine(TS, PID, 'ui', 'sys_app_killed',
      m.buildEventFields('APP_KILLED', hugeParams))).get('ev_kill_reason') === 'na');
  check('injection guard: a value can never start a second line',
    m.sanitizeToken('a b\nc', 48) === 'a_b_c' &&
    m.buildSystemEventLine(TS, PID, 'ui', 'sys_app_killed', { ev_reason: 'fake\nline=1' })
      .split('\n').length === 2);

  // ---- param shapes: JSON string, Map, plain object ------------------------
  const asString = fieldsOf(m.buildSystemEventLine(TS, PID, 'ui', 'sys_app_killed',
    m.buildEventFields('APP_KILLED', '{"reason":"JsonReason","pid":"7"}')));
  check('params arriving as a JSON string are projected',
    asString.get('ev_reason') === 'JsonReason' && asString.get('ev_pid') === '7');
  const asMap = fieldsOf(m.buildSystemEventLine(TS, PID, 'ui', 'sys_app_killed',
    m.buildEventFields('APP_KILLED', new Map<string, Object>([['reason', 'MapReason'], ['pid', '8']]))));
  check('params arriving as a Map are projected',
    asMap.get('ev_reason') === 'MapReason' && asMap.get('ev_pid') === '8');
  check('a scalar or array params value is refused, never stringified',
    m.toFieldMap(5) === null && m.toFieldMap(true) === null && m.toFieldMap(['reason']) === null &&
    m.toFieldMap(null) === null && m.toFieldMap(undefined) === null);

  // ---- unknown events ------------------------------------------------------
  check('an unwatched event never projects and never produces a line',
    m.projectEvent('USER_LOGIN', { reason: 'x' }) === null &&
    m.projectInfos(TS, PID, 'ui', 'USER_LOGIN', [{ reason: 'x' }], 1, new m.EventDeduper(4)).length === 0);

  // ---- bounded dedup -------------------------------------------------------
  // Each case gets its own window so the cases stay independent.
  const twice = m.projectInfos(TS, PID, 'vpn', 'APP_KILLED', [killed, killed], 2, new m.EventDeduper(m.DEDUP_WINDOW));
  check('an exact re-delivery becomes ONE dup line, not a second record',
    twice.length === 2 && twice[0].indexOf('event=sys_app_killed') > 0 &&
    twice[1].indexOf('event=sys_event_dup') > 0 &&
    fieldsOf(twice[1]).get('ev_suppressed') === '1' &&
    twice.filter((l: string) => l.indexOf('event=sys_app_killed') > 0).length === 1,
    twice.join(' ~ '));
  const distinct = m.projectInfos(TS, PID, 'vpn', 'APP_KILLED',
    [killed, { reason: 'LowMemoryKill', time: 1760000000001, pid: '52036' }], 2, new m.EventDeduper(m.DEDUP_WINDOW));
  check('a new occurrence (new time) is never suppressed as a duplicate',
    distinct.length === 2 && distinct[0].indexOf('ev_time=1760000000000') > 0 &&
    distinct[1].indexOf('ev_time=1760000000001') > 0 && distinct[1].indexOf('sys_event_dup') < 0,
    distinct.join(' ~ '));
  const deduper = new m.EventDeduper(m.DEDUP_WINDOW);
  check('suppressed count is handed over exactly once',
    deduper.isDuplicate('fp-a') === false && deduper.isDuplicate('fp-a') === true &&
    deduper.isDuplicate('fp-a') === true && deduper.takeSuppressed() === 2 &&
    deduper.takeSuppressed() === 0);
  const window = new m.EventDeduper(4);
  for (const fp of ['w1', 'w2', 'w3', 'w4', 'w5']) { window.isDuplicate(fp); }
  check('the dedup window is bounded (the oldest fingerprint is evicted)',
    window.windowSize() === 4 && window.capacityValue() === 4 && window.isDuplicate('w1') === false);
  check('a non-positive capacity clamps to 1', new m.EventDeduper(0).capacityValue() === 1);
  const overflow = m.projectInfos(TS, PID, 'vpn', 'RESOURCE_OVERLIMIT',
    [{ time: 1 }, { time: 2 }], 20, new m.EventDeduper(8));
  check('infos beyond MAX_INFOS_PER_GROUP are projected bounded and reported dropped',
    overflow.length === 3 && overflow[2].indexOf('event=sys_event_dropped') > 0 &&
    fieldsOf(overflow[2]).get('ev_dropped') === '18' && fieldsOf(overflow[2]).get('ev_event') === 'RESOURCE_OVERLIMIT',
    overflow.join(' ~ '));
  check('every projected line respects the line bound',
    [twice, distinct, overflow].every((ls: string[]) => ls.every((l: string) => l.length <= m.MAX_LINE_CHARS && l.endsWith('\n'))));

  // ---- lifecycle lines -----------------------------------------------------
  const unavailable = fieldsOf(m.buildWatcherLine(TS, PID, 'vpn', 'unavailable', '401'));
  check('watcher unavailable is a line, not a throw',
    unavailable.get('ev_state') === 'unavailable' && unavailable.get('ev_code') === '401' &&
    m.buildWatcherLine(TS, PID, 'vpn', 'unavailable', '401').indexOf('event=sys_watcher_unavailable') > 0);
  check('watcher start/stop lines carry the role and the explicit boundary',
    m.buildWatcherLine(TS, PID, 'ui', 'started', 'na').indexOf('role=ui event=sys_watcher_started') > 0 &&
    m.buildWatcherLine(TS, PID, 'vpn', 'stopped', 'na').indexOf('event=sys_watcher_stopped') > 0);

  // ---- ingress compatibility (the diagnostics writer's validated door) -----
  // In the UI process every line projected here is handed to
  // NetBirdDiagnostics.appendSystemEvent, whose shape check is
  // NetBirdDiagnosticsCore.isSystemEventLine. The two grammars are written in
  // two files, so a drift would silently drop real kill lines: every line shape
  // this core can build is fed through that door here.
  const diagFixture = join(dir, 'NetBirdDiagnosticsCore.ts');
  writeFileSync(diagFixture, readFileSync(join(ETS, 'NetBirdDiagnosticsCore.ets'), 'utf8'));
  const diag = await import(pathToFileURL(diagFixture).href);
  const ingressLines: string[] = [
    m.buildSystemEventLine(TS, PID, 'ui', m.logEventName('APP_KILLED'),
      m.buildEventFields('APP_KILLED', killed)),
    detailLine,
    freezeLine,
    m.buildSystemEventLine(TS, PID, 'vpn', 'sys_resource_overlimit',
      m.buildEventFields('RESOURCE_OVERLIMIT', { resource_type: 'fd' })),
    m.buildWatcherLine(TS, PID, 'ui', 'started', m.UNAVAILABLE),
    m.buildWatcherLine(TS, PID, 'vpn', 'stopped', m.UNAVAILABLE),
    m.buildWatcherLine(TS, PID, 'ui', 'unavailable', '401'),
    m.buildDupLine(TS, PID, 'vpn', 3),
    m.buildDroppedLine(TS, PID, 'vpn', 'APP_EVENT_GROUPS', 2),
    wide // the marked-truncated shape
  ];
  check('every line this core can build is accepted by the diagnostics ingress',
    ingressLines.every((l: string): boolean => diag.isSystemEventLine(l) === true),
    ingressLines.filter((l: string): boolean => diag.isSystemEventLine(l) !== true).join(' ~ '));
  check('and that door still refuses a non-projected line',
    diag.isSystemEventLine('ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=sys_app_killed' +
      ' reason=raw\n') === false &&
    diag.isSystemEventLine(m.buildSystemEventLine(TS, PID, 'ui', 'USER_LOGIN', { ev_reason: 'x' }))
      === false,
    'ingress refusals');

  // ---- runtime text pins ---------------------------------------------------
  check('runtime imports the exact SDK module',
    /import hiAppEvent from '@ohos\.hiviewdfx\.hiAppEvent';/.test(runtimeCode));
  check('runtime subscribes the OS domain', runtimeCode.indexOf('hiAppEvent.domain.OS') > 0);
  const sdkRefs = [...runtimeCode.matchAll(/hiAppEvent\.event\.([A-Z_]+)/g)].map((hit) => hit[1]);
  check('runtime subscribes the four SDK event constants the core watches',
    sdkRefs.length === 4 && m.WATCHED_EVENT_NAMES.every((n: string) => sdkRefs.indexOf(n) >= 0),
    sdkRefs.join(','));
  check('runtime uses the exact SDK onReceive signature and group types',
    /onReceive: \(domain: string, appEventGroups: Array<hiAppEvent\.AppEventGroup>\): void =>/.test(runtimeCode) &&
    runtimeCode.indexOf('hiAppEvent.AppEventInfo') > 0);
  const addAt = runtimeCode.indexOf('hiAppEvent.addWatcher(');
  check('addWatcher is guarded (try before it, catch after it) and removeWatcher exists',
    addAt > 0 && runtimeCode.lastIndexOf('try {', addAt) > 0 &&
    runtimeCode.indexOf('catch', addAt) > addAt && runtimeCode.indexOf('hiAppEvent.removeWatcher(') > 0);
  check('runtime reports unavailable to the sink and still returns false',
    runtimeCode.indexOf("reportUnavailable('addWatcher'") > 0 &&
    /if \(holder === null \|\| holder === undefined\)/.test(runtimeCode));
  check('runtime bounds the group array per callback and reports the remainder',
    /MAX_GROUPS_PER_CALLBACK/.test(runtimeCode) &&
    runtimeCode.indexOf("buildDroppedLine(ts, pid, this.role, 'APP_EVENT_GROUPS', droppedGroups)") > 0);
  check('no timers anywhere (the SDK pushes; nothing polls)',
    !/(setInterval|setTimeout|clearInterval|clearTimeout)/.test(runtimeCode));
  check('no permissions / background flags / dump generation / clearData',
    !/(requestPermissionsFromUser|ohos\.permission|backgroundTaskManager|startBackgroundRunning|rawheap|dumpHeapData|dumpJsHeap|clearData)/.test(runtimeCode));
  check('runtime never stringifies params', !/JSON\.stringify/.test(runtimeCode));
  check('runtime delegates projection to the pure core',
    runtimeCode.indexOf('projectInfos(') > 0 && runtimeCode.indexOf("from './NetBirdSystemEventsCore'") > 0);
  check('runtime API is start(role, sink) / stop() / isRunning()',
    /export function start\(role: SysEventRole, safeSink: SafeSink\): boolean/.test(runtimeCode) &&
    /export function stop\(\): void/.test(runtimeCode) &&
    /export function isRunning\(\): boolean/.test(runtimeCode));
  const nameUi = /WATCHER_NAME_UI: string = '([^']+)'/.exec(runtimeCode);
  const nameVpn = /WATCHER_NAME_VPN: string = '([^']+)'/.exec(runtimeCode);
  const nameOk = (name: string | undefined): boolean =>
    name !== undefined && name.length <= 32 && /^[A-Za-z][A-Za-z0-9_]*[A-Za-z0-9]$/.test(name);
  check('watcher names satisfy the SDK constraint and differ per process role',
    nameOk(nameUi?.[1]) && nameOk(nameVpn?.[1]) && nameUi?.[1] !== nameVpn?.[1],
    `${nameUi?.[1]} / ${nameVpn?.[1]}`);
  check('the next-launch delivery limit is stated, not papered over',
    runtimeText.indexOf('NEXT LAUNCH') > 0 && runtimeText.indexOf('sys_watcher_started line') > 0);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
