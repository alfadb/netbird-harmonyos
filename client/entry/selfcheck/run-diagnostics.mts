/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host-side checks for the pure diagnostics core (byte-identical import of
// client/entry/src/main/ets/vpnextensionability/NetBirdDiagnosticsCore.ets).
// Run: node client/entry/selfcheck/run-diagnostics.mts
//
// Pinned here: the fixed basenames/order and the 2 MiB total, the exact record
// and marker line format, the field allowlist + secret redaction + injection
// guard + line bound, the prior-exit rule, the rotation plan, the sample
// fields, and the bounded queue semantics.
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { registerHooks } from 'node:module';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const rejections: unknown[] = [];
process.on('unhandledRejection', (reason: unknown): void => { rejections.push(reason); });

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, '..', 'src', 'main', 'ets', 'vpnextensionability', 'NetBirdDiagnosticsCore.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-diagnostics-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) {
    console.log(`     ${detail}`);
  }
}
try {
  const text = readFileSync(source, 'utf8');
  check('core module is pure (never imports a platform module)',
    !/from\s+['"]@ohos/.test(text) && !/\brequire\s*\(/.test(text));
  check('core module has no import statement', !/^\s*import\s/m.test(text));
  const fixture = join(dir, 'NetBirdDiagnosticsCore.ts');
  writeFileSync(fixture, text);
  const m = await import(pathToFileURL(fixture).href);

  // ---- fixed basenames, order, bounds --------------------------------------
  check('active log basename', m.UI_LOG_ACTIVE === 'netbird-ui.log');
  check('rotated basenames newest -> oldest',
    m.UI_LOG_ROTATED.length === 3 && m.UI_LOG_ROTATED[0] === 'netbird-ui.log.1' &&
    m.UI_LOG_ROTATED[1] === 'netbird-ui.log.2' && m.UI_LOG_ROTATED[2] === 'netbird-ui.log.3');
  check('file order export is active + rotated',
    m.UI_LOG_FILES_BY_AGE.join(',') ===
    'netbird-ui.log,netbird-ui.log.1,netbird-ui.log.2,netbird-ui.log.3');
  check('four files, 512 KiB each, exactly 2 MiB total',
    m.UI_LOG_FILES_BY_AGE.length === 4 && m.PER_FILE_MAX_BYTES === 524288 &&
    m.TOTAL_MAX_BYTES === 2097152 &&
    m.PER_FILE_MAX_BYTES * m.UI_LOG_FILES_BY_AGE.length === m.TOTAL_MAX_BYTES);
  check('line/value/name bounds',
    m.MAX_LINE_CHARS === 512 && m.MAX_VALUE_CHARS === 48 && m.MAX_EVENT_NAME_CHARS === 40);
  check('queue capacity and sample interval',
    m.QUEUE_CAPACITY === 256 && m.SAMPLE_INTERVAL_MS === 60000);
  check('unavailable token is not a number', m.UNAVAILABLE === 'na');
  check('marker basenames are per role and separate from the log basenames',
    m.markerBasename('ui') === 'netbird-diag-ui.marker' &&
    m.markerBasename('vpn') === 'netbird-diag-vpn.marker' &&
    m.markerBasename('vpn') !== m.markerBasename('ui') &&
    m.UI_LOG_FILES_BY_AGE.indexOf(m.markerBasename('ui')) < 0 &&
    m.UI_LOG_FILES_BY_AGE.indexOf(m.markerBasename('vpn')) < 0);

  // ---- path allowlist ------------------------------------------------------
  check('allowlist accepts the four logs and two markers',
    m.isAllowlistedBasename('netbird-ui.log') && m.isAllowlistedBasename('netbird-ui.log.1') &&
    m.isAllowlistedBasename('netbird-ui.log.2') && m.isAllowlistedBasename('netbird-ui.log.3') &&
    m.isAllowlistedBasename('netbird-diag-ui.marker') &&
    m.isAllowlistedBasename('netbird-diag-vpn.marker'));
  check('allowlist rejects foreign paths',
    !m.isAllowlistedBasename('netbird-status.json') &&
    !m.isAllowlistedBasename('netbird-setup-key') &&
    !m.isAllowlistedBasename('netbird-ui.log.4') &&
    !m.isAllowlistedBasename('../netbird-ui.log') &&
    !m.isAllowlistedBasename(''));
  check('recordPath joins an allowlisted basename',
    m.recordPath('/data/files', 'netbird-ui.log') === '/data/files/netbird-ui.log' &&
    m.recordPath('/data/files', 'netbird-diag-vpn.marker') === '/data/files/netbird-diag-vpn.marker');
  check('recordPath refuses a foreign basename or an empty filesDir',
    m.recordPath('/data/files', 'netbird-status.json') === '' &&
    m.recordPath('/data/files', '../../etc/passwd') === '' &&
    m.recordPath('', 'netbird-ui.log') === '');

  // ---- rotation plan -------------------------------------------------------
  const steps = m.rotationSteps();
  check('rotation plan: drop oldest, shift .2->.3, .1->.2, active->.1',
    JSON.stringify(steps) === JSON.stringify([
      { op: 'unlink', from: 'netbird-ui.log.3', to: '' },
      { op: 'rename', from: 'netbird-ui.log.2', to: 'netbird-ui.log.3' },
      { op: 'rename', from: 'netbird-ui.log.1', to: 'netbird-ui.log.2' },
      { op: 'rename', from: 'netbird-ui.log', to: 'netbird-ui.log.1' }
    ]), JSON.stringify(steps));
  check('every rotation source and target is allowlisted',
    steps.every((s: { from: string; to: string }) =>
      m.isAllowlistedBasename(s.from) && (s.to === '' || m.isAllowlistedBasename(s.to))));
  check('rotation boundary: empty active file never rotates',
    m.needsRotation(0, 100000) === false);
  check('rotation boundary: exactly full appends',
    m.needsRotation(m.PER_FILE_MAX_BYTES - 1, 1) === false &&
    m.needsRotation(m.PER_FILE_MAX_BYTES - 1, 2) === true &&
    m.needsRotation(m.PER_FILE_MAX_BYTES, 1) === true);

  // ---- record line format --------------------------------------------------
  const ts = '2026-10-06T12:00:00.000Z';
  check('exact record line for a fieldless internal event',
    m.buildRecord(ts, 42, 'ui', 'diag_start', {}, true) ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=diag_start\n',
    m.buildRecord(ts, 42, 'ui', 'diag_start', {}, true));
  check('exact record line for a public event with fields (lexicographic order)',
    m.buildRecord(ts, 42, 'ui', 'ui_connect_ok', { stage: 'dial', code: 7, ok: true }, false) ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=ui_connect_ok code=7 ok=true stage=dial\n',
    m.buildRecord(ts, 42, 'ui', 'ui_connect_ok', { stage: 'dial', code: 7, ok: true }, false));
  check('role is recorded per process',
    m.buildRecord(ts, 7, 'vpn', 'mem_sample', {}, true).indexOf('role=vpn') > 0);
  check('undefined fields is the same as no fields',
    m.buildRecord(ts, 42, 'ui', 'ui_ping', undefined, false) ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=ui_ping\n');
  check('unknown public field keys are dropped (no credential passthrough)',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { setup_key: 'abc', stage: 'x' }, false).indexOf('setup_key') < 0 &&
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { setup_key: 'abc', stage: 'x' }, false).indexOf('stage=x') > 0);
  check('internal memory keys are not accepted on public events',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { rss_kb: '1' }, false).indexOf('rss_kb') < 0 &&
    m.buildRecord(ts, 42, 'ui', 'mem_sample', { rss_kb: '1' }, true).indexOf('rss_kb=1') > 0);
  check('a space or newline in a value cannot inject a field or a record',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { reason: 'a b\nc' }, false) ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=ui_ok reason=a_b_c\n',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { reason: 'a b\nc' }, false));
  check('a value keeps exactly one trailing newline',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { reason: 'x\ny\n' }, false).endsWith('_\n') &&
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { reason: 'x\ny\n' }, false).split('\n').length === 2);
  check('empty value is the dash token',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { reason: '' }, false).indexOf('reason=-') > 0);
  check('non-finite numbers become the unavailable token',
    m.normalizeValue('ms', NaN) === 'na' && m.normalizeValue('ms', Infinity) === 'na' &&
    m.normalizeValue('ms', 12.5) === '12.5' && m.normalizeValue('ms', 0) === '0');
  check('booleans are literal true/false',
    m.normalizeValue('ok', true) === 'true' && m.normalizeValue('ok', false) === 'false');

  // ---- redaction -----------------------------------------------------------
  const jwt = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIn0.abcdefghijklmnop';
  const wgKey = 'YJk3mZ1S6rPq0vQ4xT8uL2nF5hD9cB7aE1gK4wR6tY0=';
  check('a JWT-looking value is redacted', m.normalizeValue('detail', jwt) === 'redacted');
  check('an opaque base64 key is redacted', m.normalizeValue('detail', wgKey) === 'redacted');
  check('a Bearer header is redacted', m.normalizeValue('detail', 'Bearer abc.def') === 'redacted');
  check('setup-key style prefixes are redacted',
    m.normalizeValue('detail', 'sb-1234567890') === 'redacted' &&
    m.normalizeValue('detail', 'nbp_abcdef') === 'redacted' &&
    m.normalizeValue('detail', 'sk-live-abcdef') === 'redacted');
  check('secret-labelled key names are redacted regardless of value',
    m.looksSecret('setup_key', 'plain') === true && m.looksSecret('auth_token', 'x') === true &&
    m.looksSecret('credential', 'x') === true && m.looksSecret('password', 'x') === true);
  check('a readable value survives sanitising',
    m.normalizeValue('detail', 'vpn-tun up') === 'vpn-tun_up' &&
    m.normalizeValue('peer', 'peer-a.netbird.cloud') === 'peer-a.netbird.cloud');
  check('a long readable value is truncated to the value bound',
    m.normalizeValue('detail', 'netbird link ' + 'x'.repeat(60)).length === m.MAX_VALUE_CHARS);
  check('a long dotted hostname is not mistaken for a secret',
    m.normalizeValue('peer', 'peer-a.devices.netbird.cloud.example.internal') ===
    'peer-a.devices.netbird.cloud.example.internal');

  // ---- event name validation ----------------------------------------------
  check('public name pattern', m.isPublicEventName('ui_connect_ok') === true &&
    m.isPublicEventName('vpn_stage_2') === true);
  check('public names reject uppercase, separators, spaces and empties',
    m.isPublicEventName('UI_OK') === false && m.isPublicEventName('ui ok') === false &&
    m.isPublicEventName('ui-ok') === false && m.isPublicEventName('') === false &&
    m.isPublicEventName('9ui') === false && m.isPublicEventName('_ui') === false);
  check('public names reject over-length and internal names',
    m.isPublicEventName('u'.repeat(41)) === false &&
    m.isPublicEventName('diag_start') === false && m.isPublicEventName('mem_sample') === false &&
    m.isPublicEventName('PRIOR_EXIT_UNCLEAN') === false);
  check('internal names are fixed and reserved',
    m.INTERNAL_EVENT_NAMES.indexOf('PRIOR_EXIT_UNCLEAN') >= 0 &&
    m.INTERNAL_EVENT_NAMES.indexOf('diag_dropped') >= 0);

  // ---- system-event ingress (own SystemEvents format only) ----------------
  // The exact line the pure projector builds for an APP_KILLED with a reason.
  const sysKill = 'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=sys_app_killed' +
    ' ev_kill_reason=LowMemoryKill ev_pid=52036 ev_process_name=com.example.netbird' +
    ' ev_reason=LowMemoryKill ev_time=1760000000000\n';
  check('sys ingress accepts the projector\'s own line (kill reason intact)',
    m.isSystemEventLine(sysKill) === true, sysKill);
  check('sys ingress accepts the fieldless watcher lifecycle line',
    m.isSystemEventLine('ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=sys_watcher_started' +
      ' ev_code=na ev_state=started\n') === true);
  const longSysBody = 'ts=2026-10-06T12:00:00.000Z pid=42 role=vpn event=sys_app_killed ' +
    'ev_exit_msg=' + 'A.'.repeat(300) + ' ev_pid=52036';
  const truncatedSys = longSysBody.substring(0, m.MAX_LINE_CHARS - 2) + '~\n';
  check('sys ingress accepts the projector\'s marked truncation (cut inside a field)',
    longSysBody.length > m.MAX_LINE_CHARS && truncatedSys.length === m.MAX_LINE_CHARS &&
    m.isSystemEventLine(truncatedSys) === true, truncatedSys.slice(-24));
  check('sys ingress refuses a public/diagnostic record',
    m.isSystemEventLine('ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=ui_ok stage=x\n') === false &&
    m.isSystemEventLine(m.buildRecord(ts, 42, 'ui', 'ui_ok', { stage: 'x' }, false)) === false);
  check('sys ingress refuses a non-ev_ payload key (no raw params passthrough)',
    m.isSystemEventLine('ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=sys_app_killed' +
      ' reason=LowMemoryKill\n') === false &&
    m.isSystemEventLine('ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=sys_app_killed' +
      ' ev_detail={"a":1}\n') === false);
  check('sys ingress refuses a foreign role, a non-sys name and a non-numeric pid',
    m.isSystemEventLine('ts=2026-10-06T12:00:00.000Z pid=42 role=root event=sys_app_killed\n') === false &&
    m.isSystemEventLine('ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=sys_APP_KILLED\n') === false &&
    m.isSystemEventLine('ts=2026-10-06T12:00:00.000Z pid=abc role=ui event=sys_app_killed\n') === false);
  check('sys ingress refuses a second line, a missing newline and an unmarked over-long line',
    m.isSystemEventLine('ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=sys_app_killed\n' +
      'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=sys_app_killed\n') === false &&
    m.isSystemEventLine(sysKill.substring(0, sysKill.length - 1)) === false &&
    m.isSystemEventLine(longSysBody + '\n') === false);
  check('sys ingress refuses a payload value over the value bound and a stray ~',
    m.isSystemEventLine('ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=sys_app_killed' +
      ' ev_reason=' + 'B'.repeat(m.MAX_VALUE_CHARS + 1) + '\n') === false &&
    m.isSystemEventLine(sysKill.substring(0, sysKill.length - 1) + '~\n') === false);

  // ---- line bound ----------------------------------------------------------
  // Every public key with a maximum readable value: far past MAX_LINE_CHARS.
  const wide: Record<string, string> = {};
  const wideValue = 'stage value ' + 'x'.repeat(40);
  for (const key of m.PUBLIC_FIELD_KEYS) {
    wide[key] = wideValue;
  }
  const long = m.buildRecord(ts, 42, 'ui', 'ui_ok', wide, false);
  check('an over-long record is cut to the line bound and marked',
    long.length === m.MAX_LINE_CHARS && long.endsWith('~\n') &&
    long.split('\n').length === 2,
    `len=${long.length}`);
  check('a short record is never marked as truncated',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { stage: 'a' }, false).indexOf('~') < 0);

  // ---- markers -------------------------------------------------------------
  const markerLine = m.buildMarker(ts, 4242, 'vpn', 'open');
  check('exact marker line',
    markerLine === 'v=1 state=open ts=2026-10-06T12:00:00.000Z pid=4242 role=vpn\n', markerLine);
  const parsed = m.parseMarker(markerLine);
  check('marker round-trip',
    parsed !== null && parsed.state === 'open' && parsed.ts === ts && parsed.pid === 4242 &&
    parsed.role === 'vpn');
  const clean = m.parseMarker(m.buildMarker(ts, 9, 'ui', 'clean'));
  check('clean marker parses', clean !== null && clean.state === 'clean' && clean.pid === 9);
  check('garbled / empty / pid-less markers are null',
    m.parseMarker('') === null && m.parseMarker('garbage') === null &&
    m.parseMarker('state=open') === null && m.parseMarker('state=open pid=abc') === null &&
    m.parseMarker('pid=7') === null);
  check('an open marker from another pid is a prior unclean exit',
    m.isPriorUnclean(m.parseMarker(m.buildMarker(ts, 1, 'ui', 'open')), 2) === true);
  check('an open marker from this pid is not a prior exit',
    m.isPriorUnclean(m.parseMarker(m.buildMarker(ts, 2, 'ui', 'open')), 2) === false);
  check('a clean marker and a missing marker are not prior unclean exits',
    m.isPriorUnclean(m.parseMarker(m.buildMarker(ts, 1, 'ui', 'clean')), 2) === false &&
    m.isPriorUnclean(null, 2) === false);
  check('an unknown marker state is not reported as unclean (observation only)',
    m.isPriorUnclean(m.parseMarker('v=1 state=weird ts=x pid=1 role=ui'), 2) === false);

  // ---- memory sample fields ------------------------------------------------
  const naFields = m.buildSampleFields(null, null, null, null);
  check('unavailable memory readings are explicit na, never 0',
    naFields['rss_kb'] === 'na' && naFields['swap_kb'] === 'na' &&
    naFields['heap_kb'] === 'na' && naFields['sys_avail_kb'] === 'na');
  const sample = m.buildRecord(ts, 42, 'vpn', 'mem_sample',
    m.buildSampleFields('45120', '0', '8192', '1234567'), true);
  check('exact memory sample record with explicit KB units and scope',
    sample === 'ts=2026-10-06T12:00:00.000Z pid=42 role=vpn event=mem_sample ' +
      'heap_kb=8192 rss_kb=45120 swap_kb=0 sys_avail_kb=1234567\n', sample);
  check('every memory key carries its unit suffix',
    Object.keys(naFields).every((k: string) => k.endsWith('_kb')) &&
    Object.keys(naFields).length === 4);
  check('the memory half stays exactly the four existing KB keys (consumers pinned)',
    Object.keys(naFields).join(',') === 'rss_kb,swap_kb,heap_kb,sys_avail_kb');

  // ---- /proc sampling: bounds, parser, resource fields ---------------------
  check('proc bounds are fixed and small',
    m.PROC_READ_MAX_BYTES === 16384 && m.PROC_MAX_LINES === 512 &&
    m.MAX_PROC_KEY_CHARS === 32 && m.MAX_PROC_DIGITS === 15 && m.FD_LIST_MAX === 1024);
  check('the proc nodes are self-only or pid-free (no other process addressed)',
    m.PROC_SELF_STATUS === '/proc/self/status' && m.PROC_MEMINFO === '/proc/meminfo' &&
    m.PROC_SELF_FD === '/proc/self/fd' &&
    m.PROC_SELF_STATUS.charAt(0) === '/' && m.PROC_SELF_FD.indexOf('/proc/self/') === 0 &&
    m.PROC_MEMINFO === '/proc/meminfo');
  check('only the documented status/meminfo keys are requested',
    m.PROC_STATUS_KEYS.join(',') === 'Threads,VmSize,RssAnon,RssFile' &&
    m.PROC_MEMINFO_KEYS.join(',') === 'SwapTotal,SwapFree');

  const statusFixture = 'Name:\tnetbird\nUid:\t20020001\nThreads:\t42\nVmSize:\t 4194304 kB\n' +
    'VmRSS:\t 153000 kB\nRssAnon:\t 120000 kB\nRssFile:\t 33000 kB\nVmSwap:\t 0 kB\n';
  const statusParsed = m.parseProcNumbers(statusFixture, m.PROC_STATUS_KEYS, m.PROC_MAX_LINES);
  check('proc parse: exactly the requested numeric keys are read (nothing else)',
    statusParsed.size === 4 && statusParsed.get('Threads') === 42 &&
    statusParsed.get('VmSize') === 4194304 && statusParsed.get('RssAnon') === 120000 &&
    statusParsed.get('RssFile') === 33000 && statusParsed.has('VmRSS') === false &&
    statusParsed.has('Name') === false && statusParsed.has('Uid') === false,
    JSON.stringify([...statusParsed.entries()]));
  check('proc parse: tabs/spaces and trailing units are tolerated',
    m.parseProcNumbers('SwapFree:\t\t1048576 kB\n', ['SwapFree'], 8).get('SwapFree') === 1048576 &&
    m.parseProcNumbers('Threads:  7  \n', ['Threads'], 8).get('Threads') === 7);
  check('proc parse: a real 0 is an honest reading, not unavailable',
    m.procNumberText(m.parseProcNumbers('SwapTotal: 0 kB\n', ['SwapTotal'], 8).get('SwapTotal')) === '0');
  check('proc parse: missing/garbled/non-numeric readings are simply absent',
    m.parseProcNumbers('', m.PROC_STATUS_KEYS, 8).size === 0 &&
    m.parseProcNumbers('no colon here\n', ['Threads'], 8).size === 0 &&
    m.parseProcNumbers('Threads:\n', ['Threads'], 8).size === 0 &&
    m.parseProcNumbers('Threads: abc\n', ['Threads'], 8).size === 0 &&
    m.parseProcNumbers('Threads: -1\n', ['Threads'], 8).size === 0 &&
    m.parseProcNumbers('Threads: x5\n', ['Threads'], 8).size === 0);
  check('proc parse: the first occurrence wins and duplicates are ignored',
    m.parseProcNumbers('Threads: 1\nThreads: 2\n', ['Threads'], 8).get('Threads') === 1);
  check('proc parse: the line bound is enforced',
    m.parseProcNumbers('x\nThreads: 5\n', ['Threads'], 1).size === 0 &&
    m.parseProcNumbers('x\nThreads: 5\n', ['Threads'], 2).get('Threads') === 5);
  check('proc parse: key length and digit count are bounded',
    m.parseProcNumbers('A'.repeat(m.MAX_PROC_KEY_CHARS + 1) + ': 1\n',
      ['A'.repeat(m.MAX_PROC_KEY_CHARS + 1)], 8).size === 0 &&
    m.parseProcNumbers('Threads: ' + '9'.repeat(40) + '\n', ['Threads'], 8).get('Threads') ===
      Number('9'.repeat(m.MAX_PROC_DIGITS)));

  check('cpu text is the raw cumulative counter, never a rate',
    m.cpuMsText(0) === '0' && m.cpuMsText(1234) === '1234' && m.cpuMsText(9876543) === '9876543');
  check('cpu text: non-finite or negative is unavailable, never a fake number',
    m.cpuMsText(NaN) === null && m.cpuMsText(Infinity) === null && m.cpuMsText(-1) === null);
  check('proc value text: absent is unavailable, a present 0 is honest',
    m.procNumberText(undefined) === null && m.procNumberText(0) === '0' &&
    m.procNumberText(42) === '42');

  const naResource = m.buildResourceFields(null, null, null, null, null, null, null, null, null);
  check('unavailable resource readings are explicit na, never 0',
    naResource['cpu_ms'] === 'na' && naResource['threads'] === 'na' &&
    naResource['vmsize_kb'] === 'na' && naResource['rss_anon_kb'] === 'na' &&
    naResource['rss_file_kb'] === 'na' && naResource['sys_swap_total_kb'] === 'na' &&
    naResource['sys_swap_free_kb'] === 'na' && naResource['fd_count'] === 'na' &&
    naResource['fd_capped'] === 'na' && Object.keys(naResource).length === 9);
  const resFields = m.buildResourceFields('1234', '42', '4194304', '120000', '33000',
    '0', '0', '7', false);
  check('resource fields carry exact units: cpu_ms / threads / *_kb / fd_count / fd_capped',
    resFields['cpu_ms'] === '1234' && resFields['threads'] === '42' &&
    resFields['vmsize_kb'] === '4194304' && resFields['rss_anon_kb'] === '120000' &&
    resFields['rss_file_kb'] === '33000' && resFields['sys_swap_total_kb'] === '0' &&
    resFields['sys_swap_free_kb'] === '0' && resFields['fd_count'] === '7' &&
    resFields['fd_capped'] === 'false');
  check('a full fd list is flagged as a lower bound (fd_capped=true)',
    m.buildResourceFields(null, null, null, null, null, null, null, '1024', true)['fd_capped'] === 'true' &&
    m.buildResourceFields(null, null, null, null, null, null, null, '7', false)['fd_capped'] === 'false');
  check('every resource key is internal-only (a public event cannot carry it)',
    Object.keys(naResource).every((k: string) => m.INTERNAL_FIELD_KEYS.indexOf(k) >= 0 &&
      m.PUBLIC_FIELD_KEYS.indexOf(k) < 0));
  check('a public event cannot smuggle a resource key through',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { cpu_ms: '1', fd_count: '2', threads: '3' }, false) ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=ui_ok\n');
  const mergedSample = m.mergeFields({ rss_kb: '1' },
    m.buildResourceFields('2', null, null, null, null, null, null, null, null));
  check('mergeFields puts both halves in ONE record',
    mergedSample['rss_kb'] === '1' && mergedSample['cpu_ms'] === '2' &&
    mergedSample['threads'] === 'na' && Object.keys(mergedSample).length === 10);
  const fullSample = m.buildRecord(ts, 42, 'vpn', 'mem_sample',
    m.mergeFields(m.buildSampleFields('45120', '3', '8192', '1234567'),
      m.buildResourceFields('1234', '42', '4194304', '120000', '33000', '2097152',
        '1048576', '7', false)), true);
  check('exact combined mem_sample record: memory + resource fields, one bounded line',
    fullSample === 'ts=2026-10-06T12:00:00.000Z pid=42 role=vpn event=mem_sample ' +
      'cpu_ms=1234 fd_capped=false fd_count=7 heap_kb=8192 rss_anon_kb=120000 ' +
      'rss_file_kb=33000 rss_kb=45120 swap_kb=3 sys_avail_kb=1234567 ' +
      'sys_swap_free_kb=1048576 sys_swap_total_kb=2097152 threads=42 vmsize_kb=4194304\n',
    fullSample);
  check('the fully populated sample stays well inside the 512-char line bound',
    fullSample.length < m.MAX_LINE_CHARS && fullSample.indexOf('~') < 0,
    `len=${fullSample.length}`);

  // ---- bounded queue -------------------------------------------------------
  const q = new m.BoundedQueue(3);
  check('queue accepts up to capacity in order',
    q.push('a') === true && q.push('b') === true && q.push('c') === true && q.size() === 3);
  check('queue drops (and counts) beyond capacity',
    q.push('d') === false && q.push('e') === false && q.dropped() === 2 && q.size() === 3);
  check('queue is FIFO', q.shift() === 'a' && q.shift() === 'b' && q.shift() === 'c');
  check('queue shift on empty is undefined and size is 0',
    q.shift() === undefined && q.size() === 0);
  check('takeDropped hands the count over exactly once',
    q.takeDropped() === 2 && q.takeDropped() === 0 && q.dropped() === 0);
  check('dropped count survives until taken (records are never silently lost)',
    q.push('f') === true && q.push('g') === true && q.push('h') === true && q.push('i') === false &&
    q.dropped() === 1 && q.takeDropped() === 1);
  const clamped = new m.BoundedQueue(0);
  check('a non-positive capacity clamps to 1',
    clamped.push('a') === true && clamped.push('b') === false && clamped.size() === 1);
  const full = new m.BoundedQueue(m.QUEUE_CAPACITY);
  let acceptedAll = true;
  for (let i = 0; i < m.QUEUE_CAPACITY; i++) {
    if (!full.push('l' + i)) { acceptedAll = false; }
  }
  check('queue capacity is the documented record bound',
    acceptedAll && full.size() === m.QUEUE_CAPACITY && full.push('overflow') === false);
  // =========================================================================
  // runtime half: the REAL NetBirdDiagnostics.ets driven against a fake
  // platform (in-memory fs, stubbed hilog/hidebug/process, captured timers).
  // This covers what the pure checks cannot: the async queue + rotation
  // execution, marker detection and clean stop, sink fan-out and error
  // swallowing.
  // =========================================================================
  const runtimeSource = join(here, '..', 'src', 'main', 'ets', 'vpnextensionability',
    'NetBirdDiagnostics.ets');
  const runtimeText = readFileSync(runtimeSource, 'utf8');
  const runtimeCode = runtimeText.replace(/\/\/[^\n]*/g, ''); // comments removed
  const importSpecs = [...runtimeText.matchAll(/^import[^;]*from\s+'([^']+)'/gm)]
    .map((mm) => mm[1]);
  check('runtime imports only @ohos modules and the pure core',
    importSpecs.length >= 5 &&
    importSpecs.every((s: string) => s.startsWith('@ohos.') || s === './NetBirdDiagnosticsCore'),
    importSpecs.join(','));
  check('runtime never uses a synchronous file API',
    !/(openSync|writeSync|readTextSync|statSync|accessSync|unlinkSync|renameSync|mkdirSync|listFileSync)/.test(runtimeCode));
  check('runtime never names a credential or config document',
    !/netbird-setup-key|netbird-device-config|netbird-status\.json|netbird-n2/.test(runtimeText));
  check('runtime samples only the verified cheap memory APIs',
    runtimeCode.includes('hidebug.getRssInfo()') &&
    runtimeCode.includes('hidebug.getAppVMMemoryInfo()') &&
    runtimeCode.includes('hidebug.getSystemMemInfo()'));
  check('runtime never touches the blocking smaps_rollup APIs (no getPss)',
    !/getPss|getAppNativeMemInfo|getAppNativeMemInfoAsync|getAppNativeMemInfoWithCache|getSharedDirty|getPrivateDirty/.test(runtimeCode));
  check('runtime reads the one cheap cumulative CPU API and no expensive CPU API',
    runtimeCode.includes('process.getPastCpuTime()') &&
    !/getCpuUsage|startProfiling|getAppMemoryInfo|getVssInfo/.test(runtimeCode));
  check('runtime takes the CPU counter raw (no rate arithmetic on the reading)',
    runtimeCode.includes('cpuMs = cpuMsText(process.getPastCpuTime())') &&
    !/getPastCpuTime\(\)\s*[-+*/]/.test(runtimeCode) &&
    !/[-+*/]\s*getPastCpuTime\(\)/.test(runtimeCode));
  check('runtime reads the proc nodes through the core constants, bounded and async',
    runtimeCode.includes('fs.open(path, fs.OpenMode.READ_ONLY)') &&
    runtimeCode.includes('new ArrayBuffer(PROC_READ_MAX_BYTES)') &&
    runtimeCode.includes('fs.listFile(PROC_SELF_FD, options)') &&
    runtimeCode.includes('parseProcNumbers(statusText, PROC_STATUS_KEYS, PROC_MAX_LINES)') &&
    runtimeCode.includes('parseProcNumbers(meminfoText, PROC_MEMINFO_KEYS, PROC_MAX_LINES)') &&
    runtimeCode.includes('await fs.close(file)'));
  check('runtime hardcodes no proc path and adds no new framework module',
    runtimeCode.indexOf("'/proc/") < 0 && !/TextDecoder|@ohos\.util/.test(runtimeCode) &&
    runtimeCode.includes('function decodeAscii') &&
    !/smaps|smaps_rollup|fdinfo|readlink/.test(runtimeCode));
  check('runtime decodes only a fixed bounded buffer (never a whole-file slurp)',
    runtimeCode.includes('const take: number = length < PROC_READ_MAX_BYTES ? length : PROC_READ_MAX_BYTES') &&
    runtimeCode.includes('return decodeAscii(buffer, take)') &&
    runtimeCode.includes('await fs.close(file)'));

  // ---- fake platform -------------------------------------------------------
  const PROC_STATUS = '/proc/self/status';
  const PROC_MEMINFO = '/proc/meminfo';
  const PROC_FD = '/proc/self/fd';
  const STATUS_FIXTURE = 'Name:\tnetbird\nUid:\t20020001\nThreads:\t42\nVmSize:\t 4194304 kB\n' +
    'VmRSS:\t 153000 kB\nRssAnon:\t 120000 kB\nRssFile:\t 33000 kB\nVmSwap:\t 0 kB\n';
  const MEMINFO_FIXTURE = 'MemTotal:       16384000 kB\nMemFree:         1234567 kB\n' +
    'MemAvailable:    1234567 kB\nSwapTotal:       2097152 kB\nSwapFree:        1048576 kB\n';
  const files = new Map<string, { content: string }>();
  const touched = new Set<string>();
  const openFds = new Map<number, string>();
  const hilogLines: string[] = [];
  const timers: { cb: () => void; ms: number; live: boolean }[] = [];
  let cpuTime = 1234;
  let cpuThrows = false;
  const fakeProcess = {
    pid: 111,
    getPastCpuTime: (): number => {
      if (cpuThrows) { throw new Error('unsupported'); }
      return cpuTime;
    }
  };
  let nextFd = 1;
  let failOpen = false;
  let memThrow = false;
  let procStatusText = STATUS_FIXTURE;
  let procMeminfoText = MEMINFO_FIXTURE;
  let denyProcOpen = ''; // exact proc path whose open throws EACCES
  let fdDenied = false;
  let fdNames: string[] = ['0', '1', '2', '3', '4', '5', '6'];
  let hangRead: Promise<void> | null = null; // when set, every read awaits it
  let procReadCalls = 0;
  let procListCalls = 0;
  let procReadCap = 0;
  const OpenMode = { READ_ONLY: 0o0, READ_WRITE: 0o2, CREATE: 0o100, TRUNC: 0o1000, APPEND: 0o2000 };
  const fakeFs = {
    OpenMode,
    access: async (p: string): Promise<boolean> => { touched.add(p); return files.has(p); },
    readText: async (p: string): Promise<string> => {
      touched.add(p);
      const node = files.get(p);
      if (node === undefined) { throw new Error('ENOENT'); }
      return node.content;
    },
    stat: async (p: string): Promise<{ size: number }> => {
      touched.add(p);
      const node = files.get(p);
      if (node === undefined) { throw new Error('ENOENT'); }
      return { size: node.content.length };
    },
    open: async (p: string, mode: number): Promise<{ fd: number }> => {
      touched.add(p);
      if (failOpen) { throw new Error('EACCES'); }
      if (p.startsWith('/proc/')) {
        if (p === denyProcOpen) { throw new Error('EACCES'); }
        if (p !== PROC_STATUS && p !== PROC_MEMINFO) { throw new Error('ENOENT'); }
        const fd = nextFd++;
        openFds.set(fd, p);
        return { fd };
      }
      const create = (mode & OpenMode.CREATE) === OpenMode.CREATE;
      const trunc = (mode & OpenMode.TRUNC) === OpenMode.TRUNC;
      if (!files.has(p)) {
        if (!create) { throw new Error('ENOENT'); }
        files.set(p, { content: '' });
      }
      if (trunc) { files.set(p, { content: '' }); }
      const fd = nextFd++;
      openFds.set(fd, p);
      return { fd };
    },
    read: async (fd: number, buffer: ArrayBuffer): Promise<number> => {
      procReadCalls++;
      const p = openFds.get(fd);
      if (p === undefined) { throw new Error('EBADF'); }
      if (hangRead !== null) { await hangRead; }
      let text: string;
      if (p === PROC_STATUS) { text = procStatusText; }
      else if (p === PROC_MEMINFO) { text = procMeminfoText; }
      else {
        const node = files.get(p);
        if (node === undefined) { throw new Error('ENOENT'); }
        text = node.content;
      }
      if (buffer.byteLength > procReadCap) { procReadCap = buffer.byteLength; }
      const slice = text.slice(0, buffer.byteLength); // the buffer IS the cap
      const view = new Uint8Array(buffer);
      for (let i = 0; i < slice.length; i++) { view[i] = slice.charCodeAt(i) & 0xff; }
      return slice.length;
    },
    listFile: async (p: string, options?: { listNum?: number }): Promise<string[]> => {
      touched.add(p);
      procListCalls++;
      if (p !== PROC_FD) { throw new Error('ENOENT'); }
      if (fdDenied) { throw new Error('EACCES'); }
      const cap = options !== undefined && options.listNum !== undefined && options.listNum > 0
        ? options.listNum : fdNames.length;
      return fdNames.slice(0, cap);
    },
    write: async (fd: number, text: string): Promise<number> => {
      const p = openFds.get(fd);
      if (p === undefined) { throw new Error('EBADF'); }
      const node = files.get(p);
      if (node === undefined) { throw new Error('ENOENT'); }
      node.content += text;
      return text.length;
    },
    close: async (file: number | { fd: number }): Promise<void> => {
      openFds.delete(typeof file === 'number' ? file : file.fd);
    },
    fsync: async (_fd: number): Promise<void> => undefined,
    unlink: async (p: string): Promise<void> => {
      touched.add(p);
      if (!files.delete(p)) { throw new Error('ENOENT'); }
    },
    rename: async (from: string, to: string): Promise<void> => {
      touched.add(from);
      touched.add(to);
      const node = files.get(from);
      if (node === undefined) { throw new Error('ENOENT'); }
      files.delete(from);
      files.set(to, node);
    }
  };
  const hilogStub = {
    info: (...a: unknown[]): void => { hilogLines.push('I|' + a.join('|')); },
    warn: (...a: unknown[]): void => { hilogLines.push('W|' + a.join('|')); },
    error: (...a: unknown[]): void => { hilogLines.push('E|' + a.join('|')); },
    debug: (..._a: unknown[]): void => undefined,
    isLoggable: (): boolean => false
  };
  const hidebugStub = {
    getRssInfo: (): { rss: bigint; swapRss: bigint } => {
      if (memThrow) { throw new Error('unsupported'); }
      return { rss: 45120n, swapRss: 3n };
    },
    getAppVMMemoryInfo: (): { heapUsed: bigint } => {
      if (memThrow) { throw new Error('unsupported'); }
      return { heapUsed: 8192n };
    },
    getSystemMemInfo: (): { availableMem: bigint } => {
      if (memThrow) { throw new Error('unsupported'); }
      return { availableMem: 1234567n };
    }
  };
  (globalThis as unknown as { __diagFake: unknown }).__diagFake =
    { fs: fakeFs, hilog: hilogStub, hidebug: hidebugStub, process: fakeProcess };
  const realSetInterval = globalThis.setInterval;
  const realClearInterval = globalThis.clearInterval;
  (globalThis as unknown as { setInterval: unknown }).setInterval =
    (cb: () => void, ms: number): number => { timers.push({ cb, ms, live: true }); return timers.length; };
  (globalThis as unknown as { clearInterval: unknown }).clearInterval = (id: number): void => {
    const timer = timers[id - 1];
    if (timer !== undefined) { timer.live = false; }
  };
  registerHooks({
    resolve(spec: string, ctx: unknown, next: (s: string, c: unknown) => unknown): unknown {
      if (spec.startsWith('@ohos.')) { return { url: 'diagstub:' + spec, shortCircuit: true }; }
      if (spec === './NetBirdDiagnosticsCore') { return next('./NetBirdDiagnosticsCore.ts', ctx); }
      return next(spec, ctx);
    },
    load(url: string, ctx: unknown, next: (u: string, c: unknown) => unknown): unknown {
      if (url.startsWith('diagstub:@ohos.file.fs')) {
        return { format: 'module', source: 'export default globalThis.__diagFake.fs;\n', shortCircuit: true };
      }
      if (url.startsWith('diagstub:@ohos.hilog')) {
        return { format: 'module', source: 'export default globalThis.__diagFake.hilog;\n', shortCircuit: true };
      }
      if (url.startsWith('diagstub:@ohos.hidebug')) {
        return { format: 'module', source: 'export default globalThis.__diagFake.hidebug;\n', shortCircuit: true };
      }
      if (url.startsWith('diagstub:@ohos.process')) {
        return { format: 'module', source: 'export default globalThis.__diagFake.process;\n', shortCircuit: true };
      }
      return next(url, ctx);
    }
  });
  const newRuntime = async (tag: string): Promise<Record<string, any>> => {
    const fixture = join(dir, 'NetBirdDiagnostics.' + tag + '.ts');
    writeFileSync(fixture, runtimeText);
    return await import(pathToFileURL(fixture).href);
  };
  const settle = async (turns: number = 8): Promise<void> => {
    for (let i = 0; i < turns; i++) { await new Promise<void>((r) => setImmediate(r)); }
  };
  const fileText = (p: string): string => {
    const node = files.get(p);
    return node === undefined ? '' : node.content;
  };
  const linesOf = (p: string): string[] =>
    fileText(p).split('\n').filter((l: string) => l.length > 0);
  const fieldsOf = (line: string): Map<string, string> => {
    const out = new Map<string, string>();
    for (const part of line.split(' ')) {
      const eq = part.indexOf('=');
      if (eq > 0) { out.set(part.slice(0, eq), part.slice(eq + 1)); }
    }
    return out;
  };
  const RECORD_RE = /^ts=\d{4}-\d{2}-\d{2}T[\d:.]+Z pid=\d+ role=(ui|vpn) event=[A-Za-z0-9_]+/;

  // ---- A: first ui run, format, interval, event policy ---------------------
  const dirA = join(dir, 'a');
  const runA = await newRuntime('a');
  runA.start(dirA, 'ui');
  await settle();
  check('ui run: marker opened with state/pid/role and its own basename',
    /^v=1 state=open ts=\d{4}-\d{2}-\d{2}T[\d:.]+Z pid=111 role=ui\n$/
      .test(fileText(join(dirA, 'netbird-diag-ui.marker'))),
    fileText(join(dirA, 'netbird-diag-ui.marker')));
  const a1 = linesOf(join(dirA, 'netbird-ui.log'));
  check('ui run: diag_start then the initial mem_sample',
    a1.length === 2 && a1[0].includes('event=diag_start') && a1[1].includes('event=mem_sample'),
    a1.join(' ~ '));
  check('ui run: every record carries ts/pid/role/event',
    a1.every((l: string) => RECORD_RE.test(l)));
  const sampleFields = fieldsOf(a1[1]);
  check('ui run: memory units and scope are explicit KB fields',
    sampleFields.get('rss_kb') === '45120' && sampleFields.get('swap_kb') === '3' &&
    sampleFields.get('heap_kb') === '8192' && sampleFields.get('sys_avail_kb') === '1234567',
    a1[1]);
  check('ui run: the raw cumulative CPU counter is emitted as cpu_ms',
    sampleFields.get('cpu_ms') === '1234', a1[1]);
  check('ui run: threads and the anon/file split come from /proc/self/status',
    sampleFields.get('threads') === '42' && sampleFields.get('vmsize_kb') === '4194304' &&
    sampleFields.get('rss_anon_kb') === '120000' && sampleFields.get('rss_file_kb') === '33000',
    a1[1]);
  check('ui run: system swap comes from /proc/meminfo (MemAvailable is not duplicated)',
    sampleFields.get('sys_swap_total_kb') === '2097152' &&
    sampleFields.get('sys_swap_free_kb') === '1048576' &&
    sampleFields.get('sys_avail_kb') === '1234567', a1[1]);
  check('ui run: the fd count is a count only and is not flagged as capped',
    sampleFields.get('fd_count') === '7' && sampleFields.get('fd_capped') === 'false', a1[1]);
  check('ui run: the mem_sample record carries no proc path or fd name',
    a1[1].indexOf('/proc/') < 0 && a1[1].indexOf('fdinfo') < 0);
  check('ui run: one 60 s sampling interval is registered',
    timers.length === 1 && timers[0].ms === 60000 && timers[0].live === true);
  check('ui run: a public event is accepted and sanitized',
    runA.event('ui_connect_ok', { stage: 'dial', code: 7, ok: true }) === true &&
    runA.event('ui_note', { detail: 'hello world' }) === true);
  await settle();
  const a2 = linesOf(join(dirA, 'netbird-ui.log'));
  check('ui run: the event line keeps the whitelisted fields in order',
    a2.length === 4 && a2[2].includes('event=ui_connect_ok code=7 ok=true stage=dial') &&
    a2[3].includes('event=ui_note detail=hello_world'), a2.join(' ~ '));
  check('ui run: a reserved internal name is rejected',
    runA.event('diag_start', {}) === false && runA.event('PRIOR_EXIT_UNCLEAN', {}) === false);
  await settle();
  check('ui run: a rejected name adds no record',
    linesOf(join(dirA, 'netbird-ui.log')).length === 4);
  check('ui run: a non-whitelisted field (and its value) never reaches the file',
    runA.event('ui_secret', { setup_key: 'SBSECRET123', stage: 'x' }) === true);
  await settle();
  const a3 = linesOf(join(dirA, 'netbird-ui.log'));
  check('ui run: the dropped field leaves no trace',
    a3[a3.length - 1].includes('event=ui_secret stage=x') &&
    !a3[a3.length - 1].includes('setup_key') && !fileText(join(dirA, 'netbird-ui.log')).includes('SBSECRET123'),
    a3[a3.length - 1]);
  check('ui run: records accumulate in call order',
    linesOf(join(dirA, 'netbird-ui.log')).length === 5);

  // later sample ticks use the same interval callback
  timers[0].cb();
  await settle();
  const a4 = linesOf(join(dirA, 'netbird-ui.log'));
  check('ui run: the tick sample uses the same record shape',
    a4.length === 6 && RECORD_RE.test(a4[5]) && a4[5].includes('event=mem_sample'), a4[5]);

  // ---- B: prior unfinished run detection -----------------------------------
  const dirB = join(dir, 'b');
  fakeProcess.pid = 111;
  const runB1 = await newRuntime('b1');
  runB1.start(dirB, 'ui');
  await settle();
  // no stop(): the marker stays open for pid 111
  fakeProcess.pid = 222;
  const runB2 = await newRuntime('b2');
  runB2.start(dirB, 'ui');
  await settle();
  const b1 = linesOf(join(dirB, 'netbird-ui.log'));
  const unclean = b1.filter((l: string) => l.includes('event=PRIOR_EXIT_UNCLEAN'));
  check('prior exit: the previous open marker of another pid is reported once',
    unclean.length === 1 && unclean[0].includes('pid=222') &&
    unclean[0].includes('prev_pid=111') && unclean[0].includes('prev_state=open'),
    unclean.join(' ~ '));
  check('prior exit: the report carries the previous marker timestamp',
    /prev_ts=\d{4}-\d{2}-\d{2}T[\d:.]+Z/.test(unclean[0]), unclean[0]);
  check('prior exit: it is the first record of the new run (pid 222)',
    (() => {
      const mine = b1.filter((l: string) => l.includes('pid=222'));
      return mine.length >= 2 && mine[0].includes('event=PRIOR_EXIT_UNCLEAN') &&
        mine[1].includes('event=diag_start');
    })(), b1.filter((l: string) => l.includes('pid=222')).join(' ~ '));
  runB2.stop();
  await settle();
  check('prior exit: stop() marks the run clean on the same basename',
    /^v=1 state=clean ts=\d{4}-\d{2}-\d{2}T[\d:.]+Z pid=222 role=ui\n$/
      .test(fileText(join(dirB, 'netbird-diag-ui.marker'))),
    fileText(join(dirB, 'netbird-diag-ui.marker')));
  fakeProcess.pid = 333;
  const runB3 = await newRuntime('b3');
  runB3.start(dirB, 'ui');
  await settle();
  const b2 = linesOf(join(dirB, 'netbird-ui.log'));
  check('prior exit: a clean marker produces no new report',
    b2.filter((l: string) => l.includes('event=PRIOR_EXIT_UNCLEAN')).length === 1);
  // same-pid re-init: the open marker is this process, not a prior exit
  const runB4 = await newRuntime('b4');
  runB4.start(dirB, 'ui');
  await settle();
  const b3 = linesOf(join(dirB, 'netbird-ui.log'));
  check('prior exit: an open marker of THIS pid is not a prior exit',
    b3.filter((l: string) => l.includes('event=PRIOR_EXIT_UNCLEAN')).length === 1);
  check('idempotent start: a second start() on the running instance is ignored',
    runB4.start(dirB, 'ui') === undefined &&
    linesOf(join(dirB, 'netbird-ui.log')).length === b3.length);

  // ---- C: rotation, allowlist, total bound --------------------------------
  const dirC = join(dir, 'c');
  const cap = m.PER_FILE_MAX_BYTES;
  const seedActive = 'd'.repeat(cap - 5);
  files.set(join(dirC, 'netbird-ui.log'), { content: seedActive });
  files.set(join(dirC, 'netbird-ui.log.1'), { content: 'a'.repeat(cap - 1) });
  files.set(join(dirC, 'netbird-ui.log.2'), { content: 'b'.repeat(cap - 1) });
  files.set(join(dirC, 'netbird-ui.log.3'), { content: 'c'.repeat(cap - 1) });
  files.set(join(dirC, 'netbird-status.json'), { content: '{"setup_key":"SECRET"}' });
  const runC = await newRuntime('c');
  runC.start(dirC, 'ui');
  await settle();
  check('rotation: oldest dropped, chain shifted, fresh active file written',
    fileText(join(dirC, 'netbird-ui.log.3')) === 'b'.repeat(cap - 1) &&
    fileText(join(dirC, 'netbird-ui.log.2')) === 'a'.repeat(cap - 1) &&
    fileText(join(dirC, 'netbird-ui.log.1')) === seedActive &&
    fileText(join(dirC, 'netbird-ui.log')) !== seedActive &&
    RECORD_RE.test(linesOf(join(dirC, 'netbird-ui.log'))[0]),
    `active=${fileText(join(dirC, 'netbird-ui.log')).length}`);
  const logSizes = [0, 1, 2, 3].map((i: number) =>
    fileText(join(dirC, i === 0 ? 'netbird-ui.log' : 'netbird-ui.log.' + i)).length);
  check('rotation: every file is under the per-file cap and the total is under 2 MiB',
    logSizes.every((s: number) => s <= m.PER_FILE_MAX_BYTES) &&
    logSizes.reduce((a: number, b: number) => a + b, 0) <= m.TOTAL_MAX_BYTES,
    logSizes.join(','));
  check('allowlist: a foreign file in filesDir is never read, renamed or unlinked',
    fileText(join(dirC, 'netbird-status.json')) === '{"setup_key":"SECRET"}' &&
    !touched.has(join(dirC, 'netbird-status.json')));
  const foreign = [...touched].filter((p: string) =>
    p.startsWith(dir + '/') && !m.isAllowlistedBasename(p.slice(p.lastIndexOf('/') + 1)));
  check('allowlist: every path the helper ever touched is allowlisted',
    foreign.length === 0, foreign.join(','));

  // ---- D: vpn role -> caller sink, no file writer -------------------------
  const dirD = join(dir, 'd');
  fakeProcess.pid = 444;
  const sinkLines: string[] = [];
  const runD = await newRuntime('d');
  const beforeD = timers.length;
  runD.start(dirD, 'vpn', (line: string): void => { sinkLines.push(line); });
  await settle();
  check('vpn role: records go to the supplied sink',
    sinkLines.length === 2 && sinkLines[0].includes('role=vpn event=diag_start') &&
    sinkLines[1].includes('event=mem_sample'), sinkLines.join(' ~ '));
  check('vpn role: no log file is written for the vpn process',
    !files.has(join(dirD, 'netbird-ui.log')) && !files.has(join(dirD, 'netbird-vpn.log')));
  check('vpn role: the vpn marker basename is used, ui marker stays untouched',
    fileText(join(dirD, 'netbird-diag-vpn.marker')).indexOf('role=vpn') > 0 &&
    !files.has(join(dirD, 'netbird-diag-ui.marker')));
  check('vpn role: a public event reaches the sink in order',
    runD.event('vpn_stage', { stage: 'dial' }) === true);
  await settle();
  check('vpn role: the sink line is a complete record',
    sinkLines.length === 3 && RECORD_RE.test(sinkLines[2]) &&
    sinkLines[2].includes('event=vpn_stage stage=dial'), sinkLines[2]);
  runD.stop();
  await settle();
  check('vpn role: stop() flushes the queue and marks the run clean',
    sinkLines[sinkLines.length - 1].includes('event=diag_stop') &&
    /state=clean/.test(fileText(join(dirD, 'netbird-diag-vpn.marker'))),
    sinkLines.join(' ~ '));
  check('vpn role: event() after stop() is refused',
    runD.event('vpn_after_stop', {}) === false);
  check('stop() clears the sampling interval',
    timers.length === beforeD + 1 && timers[beforeD].live === false);

  // a vpn run without a sink: no destination for records, said out loud once
  const dirI = join(dir, 'i');
  const runI = await newRuntime('i');
  runI.start(dirI, 'vpn');
  await settle();
  check('vpn without a sink: the missing destination is reported, not silent',
    hilogLines.some((l: string) => l.includes('DIAG_SINK_MISSING')) &&
    /state=open/.test(fileText(join(dirI, 'netbird-diag-vpn.marker'))));
  runI.stop();
  await settle();

  // ---- E: no I/O failure, and no throwing sink, can break the helper ------
  const dirE = join(dir, 'e');
  failOpen = true;
  let threw = false;
  const runE = await newRuntime('e');
  try {
    runE.start(dirE, 'ui');
    await settle();
    const accepted = runE.event('ui_ok', { stage: 'x' });
    await settle();
    runE.stop();
    await settle();
    check('I/O failure: event() still answers and stop() still returns',
      accepted === true && runE.event('ui_ok', {}) === false);
  } catch (error) {
    threw = true;
  }
  check('I/O failure: nothing is thrown into the application', threw === false);
  check('I/O failure: the failure is counted and reported through hilog',
    hilogLines.some((l: string) => l.includes('DIAG_IO_ERROR')));
  check('I/O failure: the hilog flood stays capped at 8 per run',
    hilogLines.filter((l: string) => l.includes('DIAG_IO_ERROR')).length > 0 &&
    hilogLines.filter((l: string) => l.includes('DIAG_IO_ERROR')).every((l: string) => {
      const mm = /(\d+)$/.exec(l);
      return mm !== null && Number(mm[1]) <= 8;
    }),
    String(hilogLines.filter((l: string) => l.includes('DIAG_IO_ERROR')).length));
  failOpen = false;
  const dirF = join(dir, 'f');
  let sinkThrew = false;
  const runF = await newRuntime('f');
  try {
    runF.start(dirF, 'vpn', (): void => { throw new Error('sink boom'); });
    await settle();
    runF.event('vpn_ok', { stage: 'x' });
    await settle();
    runF.stop();
    await settle();
  } catch (error) {
    sinkThrew = true;
  }
  check('throwing sink: the exception is swallowed and the run still closes clean',
    sinkThrew === false &&
    hilogLines.some((l: string) => l.includes('DIAG_IO_ERROR') && l.includes('|sink|none|')) &&
    /state=clean/.test(fileText(join(dirF, 'netbird-diag-vpn.marker'))),
    fileText(join(dirF, 'netbird-diag-vpn.marker')));

  // ---- memory API unavailable (throwing hidebug / process) ----------------
  const dirG = join(dir, 'g');
  memThrow = true;
  cpuThrows = true;
  const runG = await newRuntime('g');
  runG.start(dirG, 'ui');
  await settle();
  const gLines = linesOf(join(dirG, 'netbird-ui.log'));
  const gFields = fieldsOf(gLines[1]);
  check('unsupported hidebug: every memory reading is the explicit na token',
    gFields.get('rss_kb') === 'na' && gFields.get('swap_kb') === 'na' &&
    gFields.get('heap_kb') === 'na' && gFields.get('sys_avail_kb') === 'na', gLines[1]);
  check('unsupported CPU API: cpu_ms is na, never 0',
    gFields.get('cpu_ms') === 'na', gLines[1]);
  check('an unavailable hidebug does not take the /proc readings down with it',
    gFields.get('threads') === '42' && gFields.get('vmsize_kb') === '4194304' &&
    gFields.get('sys_swap_total_kb') === '2097152' && gFields.get('fd_count') === '7',
    gLines[1]);
  memThrow = false;
  cpuThrows = false;

  // ---- N: a denied / malformed / oversized proc node -> na, said once ------
  const dirN = join(dir, 'n');
  denyProcOpen = PROC_MEMINFO;
  procStatusText = 'not a status file at all\nno colon here\n';
  const procWarnBefore = hilogLines.filter((l: string): boolean =>
    l.includes('DIAG_PROC_UNAVAILABLE')).length;
  const runN = await newRuntime('n');
  runN.start(dirN, 'ui');
  await settle();
  const nLines = linesOf(join(dirN, 'netbird-ui.log'));
  const nFields = fieldsOf(nLines[1]);
  check('denied /proc/meminfo: both swap fields are na, nothing is invented',
    nFields.get('sys_swap_total_kb') === 'na' && nFields.get('sys_swap_free_kb') === 'na', nLines[1]);
  check('malformed /proc/self/status: every status field is na, the record still lands',
    nFields.get('threads') === 'na' && nFields.get('vmsize_kb') === 'na' &&
    nFields.get('rss_anon_kb') === 'na' && nFields.get('rss_file_kb') === 'na' &&
    nFields.get('rss_kb') === '45120' && nFields.get('cpu_ms') === '1234', nLines[1]);
  timers[timers.length - 1].cb();
  await settle();
  timers[timers.length - 1].cb();
  await settle();
  check('a denied proc node is said ONCE per run, not once per sample',
    hilogLines.filter((l: string): boolean => l.includes('DIAG_PROC_UNAVAILABLE')).length ===
      procWarnBefore + 1,
    String(hilogLines.filter((l: string): boolean => l.includes('DIAG_PROC_UNAVAILABLE')).length));
  check('the later samples still carry na (the ongoing signal is the record)',
    fieldsOf(linesOf(join(dirN, 'netbird-ui.log'))[3]).get('sys_swap_free_kb') === 'na');
  runN.stop();
  await settle();
  denyProcOpen = '';
  procStatusText = STATUS_FIXTURE;

  // ---- N2: denied or capped fd enumeration --------------------------------
  const dirN2 = join(dir, 'n2');
  fdDenied = true;
  const runN2 = await newRuntime('n2');
  runN2.start(dirN2, 'ui');
  await settle();
  const n2Fields = fieldsOf(linesOf(join(dirN2, 'netbird-ui.log'))[1]);
  check('denied /proc/self/fd: count and capped flag are na, never 0',
    n2Fields.get('fd_count') === 'na' && n2Fields.get('fd_capped') === 'na',
    linesOf(join(dirN2, 'netbird-ui.log'))[1]);
  runN2.stop();
  await settle();
  fdDenied = false;
  const dirN3 = join(dir, 'n3');
  const savedFdNames = fdNames;
  fdNames = Array.from({ length: m.FD_LIST_MAX }, (): string => '0');
  const runN3 = await newRuntime('n3');
  runN3.start(dirN3, 'ui');
  await settle();
  const n3Fields = fieldsOf(linesOf(join(dirN3, 'netbird-ui.log'))[1]);
  check('a full fd list is flagged as a lower bound, not an exact count',
    n3Fields.get('fd_count') === String(m.FD_LIST_MAX) && n3Fields.get('fd_capped') === 'true',
    linesOf(join(dirN3, 'netbird-ui.log'))[1]);
  runN3.stop();
  await settle();
  fdNames = savedFdNames;

  // ---- N4: the 16 KiB read cap bounds what can be parsed ------------------
  const dirN4 = join(dir, 'n4');
  const filler = 'Filler: ' + '1'.repeat(40) + '\n';
  let oversized = '';
  while (oversized.length < m.PROC_READ_MAX_BYTES + 200) { oversized += filler; }
  oversized += 'Threads: 42\n'; // beyond the cap: must never be seen
  procStatusText = oversized;
  const readsBefore = procReadCalls;
  const runN4 = await newRuntime('n4');
  runN4.start(dirN4, 'ui');
  await settle();
  const n4Fields = fieldsOf(linesOf(join(dirN4, 'netbird-ui.log'))[1]);
  check('the status read uses exactly the 16 KiB fixed buffer',
    procReadCalls > readsBefore && procReadCap === m.PROC_READ_MAX_BYTES,
    `cap=${procReadCap} calls=${procReadCalls - readsBefore}`);
  check('a key beyond the byte cap is not parsed (na), never invented',
    n4Fields.get('threads') === 'na', linesOf(join(dirN4, 'netbird-ui.log'))[1]);
  runN4.stop();
  await settle();
  procStatusText = STATUS_FIXTURE;

  // ---- C2: one sample in flight; a hung read never stacks -----------------
  const dirConc = join(dir, 'conc');
  let releaseRead: (() => void) | null = null;
  const releaseHang = (): void => {
    const release: (() => void) | null = releaseRead;
    if (release !== null) { release(); }
  };
  hangRead = new Promise<void>((resolve: () => void): void => { releaseRead = resolve; });
  const runConc = await newRuntime('conc');
  runConc.start(dirConc, 'ui'); // its initial sample hangs on the status read
  await settle();
  const readsHung = procReadCalls;
  const listHung = procListCalls;
  timers[timers.length - 1].cb();
  timers[timers.length - 1].cb();
  await settle();
  check('single-flight: two ticks during a hung read start no second read/list',
    procReadCalls === readsHung && procListCalls === listHung,
    `${procReadCalls - readsHung} read(s), ${procListCalls - listHung} list(s)`);
  check('single-flight: no sample record is emitted while the read is hung',
    linesOf(join(dirConc, 'netbird-ui.log')).filter((l: string): boolean =>
      l.includes('event=mem_sample')).length === 0,
    linesOf(join(dirConc, 'netbird-ui.log')).join(' ~ '));
  releaseHang(); // the hung read settles
  await settle();
  const concAfter = linesOf(join(dirConc, 'netbird-ui.log'));
  check('single-flight: exactly ONE sample lands when the hung read is released',
    concAfter.filter((l: string): boolean => l.includes('event=mem_sample')).length === 1,
    concAfter.join(' ~ '));
  // post-stop: a reading that settles after stop() must emit nothing
  hangRead = new Promise<void>((resolve: () => void): void => { releaseRead = resolve; });
  timers[timers.length - 1].cb();
  await settle();
  runConc.stop();
  await settle();
  const beforeRelease = linesOf(join(dirConc, 'netbird-ui.log')).length;
  releaseHang();
  await settle();
  const concStop = linesOf(join(dirConc, 'netbird-ui.log'));
  check('post-stop: a reading that settles after stop() emits nothing',
    concStop.length === beforeRelease &&
    concStop[concStop.length - 1].includes('event=diag_stop') &&
    concStop.filter((l: string): boolean => l.includes('event=mem_sample')).length === 1,
    concStop.join(' ~ '));
  check('post-stop: the run still closes clean',
    /state=clean/.test(fileText(join(dirConc, 'netbird-diag-ui.marker'))));
  hangRead = null;

  // ---- C3: a delayed reading is not reused by a later run -----------------
  const dirGen = join(dir, 'gen');
  const runGen = await newRuntime('gen');
  runGen.start(dirGen, 'ui');
  await settle();
  const genBefore = linesOf(join(dirGen, 'netbird-ui.log')).length;
  hangRead = new Promise<void>((resolve: () => void): void => { releaseRead = resolve; });
  timers[timers.length - 1].cb(); // this tick's sample hangs
  await settle();
  runGen.stop(); // the run ends with that reading still in flight
  await settle();
  runGen.start(dirGen, 'ui'); // same instance, a NEW run
  await settle();
  const genAfterStart = linesOf(join(dirGen, 'netbird-ui.log')).length;
  releaseHang(); // the OLD reading settles now
  await settle();
  const genFinal = linesOf(join(dirGen, 'netbird-ui.log'));
  check('generation: a reading that settles after stop() is discarded, not emitted into the next run',
    genFinal.length === genAfterStart + 1 &&
    genFinal.filter((l: string): boolean => l.includes('event=mem_sample')).length === 2 &&
    genFinal[genFinal.length - 1].includes('event=mem_sample'),
    `before=${genBefore} afterStart=${genAfterStart} final=${genFinal.length}`);
  runGen.stop();
  await settle();
  hangRead = null;

  // ---- C4: the CPU counter stays raw across a new process ---------------
  const dirCpu1 = join(dir, 'cpu1');
  cpuTime = 900000; // a long-lived process: a large cumulative counter
  const runCpu1 = await newRuntime('cpu1');
  runCpu1.start(dirCpu1, 'ui');
  await settle();
  check('cpu: the emitted value is the platform raw cumulative counter',
    fieldsOf(linesOf(join(dirCpu1, 'netbird-ui.log'))[1]).get('cpu_ms') === '900000');
  runCpu1.stop();
  await settle();
  const dirCpu2 = join(dir, 'cpu2');
  fakeProcess.pid = 909;
  cpuTime = 5; // a NEW process: the counter restarts low
  const runCpu2 = await newRuntime('cpu2');
  runCpu2.start(dirCpu2, 'ui');
  await settle();
  const cpu2Line = linesOf(join(dirCpu2, 'netbird-ui.log'))[1];
  const cpu2Fields = fieldsOf(cpu2Line);
  check('cpu: a counter that restarts with a new process stays raw (no delta, no negative, no na)',
    cpu2Fields.get('cpu_ms') === '5' && cpu2Fields.get('pid') === '909', cpu2Line);
  runCpu2.stop();
  await settle();
  cpuTime = 1234;

  // ---- S: the system-event watcher's ingress into the same bounded writer --
  // The UI ability passes appendSystemEvent as NetBirdSystemEvents' sink, so a
  // projected line must reach the ui log verbatim — in particular its
  // ev_kill_reason must NOT be squeezed through event('x', {detail: line}),
  // whose 48-char value bound would cut the reason off.
  const dirS = join(dir, 's');
  const runS = await newRuntime('s');
  runS.start(dirS, 'ui');
  await settle();
  const killLineS = 'ts=2026-10-06T12:00:00.000Z pid=111 role=ui event=sys_app_killed' +
    ' ev_kill_reason=LowMemoryKill ev_pid=52036 ev_process_name=com.example.netbird' +
    ' ev_reason=LowMemoryKill ev_time=1760000000000\n';
  check('sys ingress: a projected kill line is accepted while the writer runs',
    runS.appendSystemEvent(killLineS) === true);
  await settle();
  const s1 = linesOf(join(dirS, 'netbird-ui.log'));
  check('sys ingress: the line lands on the SAME ui log, verbatim and in order',
    s1.length === 3 && s1[2] === killLineS.substring(0, killLineS.length - 1), s1.join(' ~ '));
  check('sys ingress: the kill reason survives in full (never cut to the value bound)',
    s1[2].indexOf('ev_kill_reason=LowMemoryKill') > 0 && s1[2].length > 100 &&
    s1[2].indexOf('~') < 0);
  check('sys ingress: a foreign record and a non-ev_ payload are refused',
    runS.appendSystemEvent(m.buildRecord(ts, 111, 'ui', 'ui_ok', { stage: 'x' }, false)) === false &&
    runS.appendSystemEvent('ts=2026-10-06T12:00:00.000Z pid=111 role=ui event=sys_app_killed' +
      ' reason=raw\n') === false &&
    runS.appendSystemEvent('ts=2026-10-06T12:00:00.000Z pid=111 role=ui event=sys_app_killed' +
      ' ev_exit_msg=' + 'A'.repeat(300) + '\n') === false);
  await settle();
  check('sys ingress: a refused line adds no record',
    linesOf(join(dirS, 'netbird-ui.log')).length === 3);
  // The watcher's stop line is appended by onDestroy BEFORE the writer's stop();
  // this ordering is exactly what makes it survive the flush.
  check('sys ingress: a line appended just before stop() is still flushed',
    runS.appendSystemEvent('ts=2026-10-06T12:00:00.000Z pid=111 role=ui' +
      ' event=sys_watcher_stopped ev_code=na ev_state=stopped\n') === true);
  runS.stop();
  await settle();
  const sFinal = linesOf(join(dirS, 'netbird-ui.log'));
  const sysStopAt = sFinal.findIndex((l: string): boolean =>
    l.indexOf('event=sys_watcher_stopped') > 0);
  const diagStopAt = sFinal.findIndex((l: string): boolean => l.indexOf('event=diag_stop') > 0);
  check('sys ingress: stop() flushes the watcher\'s last line before its own diag_stop',
    sysStopAt > 0 && diagStopAt === sFinal.length - 1 && sysStopAt < diagStopAt &&
    /state=clean/.test(fileText(join(dirS, 'netbird-diag-ui.marker'))),
    sFinal.join(' ~ '));
  check('sys ingress: after stop() the ingress is closed',
    runS.appendSystemEvent(killLineS) === false);

  // ---- W: the shipped UI ability's wiring order ---------------------------
  const entryText = readFileSync(join(here, '..', 'src', 'main', 'ets', 'entryability',
    'EntryAbility.ets'), 'utf8');
  const createAt = entryText.indexOf('onCreate(want: Want, launchParam');
  const diagStartAt = entryText.indexOf("startDiagnostics(this.context.filesDir, 'ui')");
  const sysStartAt = entryText.indexOf("startSystemEvents('ui'");
  const destroyAt = entryText.indexOf('onDestroy(): void {');
  const entrySysStopAt = entryText.indexOf('stopSystemEvents();');
  const entryDiagStopAt = entryText.indexOf('stopDiagnostics();');
  check('ui wiring: the diagnostics writer starts first, then the system-event watcher',
    createAt > 0 && diagStartAt > createAt && sysStartAt > diagStartAt,
    JSON.stringify({ createAt, diagStartAt, sysStartAt }));
  check('ui wiring: the watcher sink is the writer\'s validated ingress (one queue)',
    /startSystemEvents\('ui', \(line: string\): void => \{ appendSystemEvent\(line\); \}\)/
      .test(entryText));
  check('ui wiring: the projected line never rides event() with a detail field',
    !/diagnosticsEvent\(\s*'[^']*'\s*,\s*\{[^}]*detail/.test(entryText));
  check('ui wiring: onDestroy stops the watcher BEFORE the writer (its lines flush)',
    destroyAt > 0 && entrySysStopAt > destroyAt && entryDiagStopAt > entrySysStopAt,
    JSON.stringify({ destroyAt, entrySysStopAt, entryDiagStopAt }));

  // ---- H: bounded queue backpressure --------------------------------------
  const dirH = join(dir, 'h');
  const runH = await newRuntime('h');
  runH.start(dirH, 'ui');
  // Synchronous burst before the first drain: nothing can be written yet.
  let acceptedH = 0;
  let rejectedH = 0;
  for (let i = 0; i < 400; i++) {
    if (runH.event('ui_fill', { seq: i }) === true) { acceptedH++; } else { rejectedH++; }
  }
  await settle();
  const hLines = linesOf(join(dirH, 'netbird-ui.log'));
  const dropLines = hLines.filter((l: string) => l.includes('event=diag_dropped'));
  check('backpressure: a full queue rejects and counts instead of growing',
    acceptedH === m.QUEUE_CAPACITY && rejectedH === 400 - m.QUEUE_CAPACITY,
    `${acceptedH} accepted / ${rejectedH} rejected`);
  // The queue was full for the whole synchronous burst, so diag_start (queued
  // while ready=false) may still be dropped; the async mem_sample can land once
  // the drain has made room. Either way each internal record is EITHER in the
  // file OR counted in the one dropped report — never silently lost.
  const hStartLines = hLines.filter((l: string) => l.includes('event=diag_start')).length;
  const hSampleLines = hLines.filter((l: string) => l.includes('event=mem_sample')).length;
  check('backpressure: the dropped records are reported once, with the honest count',
    dropLines.length === 1 &&
    Number(fieldsOf(dropLines[0]).get('dropped')) ===
      144 + (1 - hStartLines) + (1 - hSampleLines) &&
    hLines.length === m.QUEUE_CAPACITY + 1 + hStartLines + hSampleLines,
    `${dropLines.length} report(s), ${hLines.length} lines, ${dropLines.join(' ~ ')}`);

  // ---- no leak: no record or log line ever carries a proc path / fd target --
  const allRecordLines: string[] = [];
  for (const node of files.values()) {
    for (const l of node.content.split('\n')) {
      if (RECORD_RE.test(l)) { allRecordLines.push(l); }
    }
  }
  check('no record carries a proc path, fd name/target or inode',
    allRecordLines.length > 0 && allRecordLines.every((l: string) =>
      l.indexOf('/proc/') < 0 && l.indexOf('fdinfo') < 0 && l.indexOf('inode') < 0));
  check('the availability warning is one concise line with no path/pid/fd target',
    hilogLines.filter((l: string) => l.includes('DIAG_PROC_UNAVAILABLE')).every((l: string) =>
      l.indexOf('/proc/') < 0 && l.indexOf('fdinfo') < 0 && l.includes('status=na')));

  (globalThis as unknown as { setInterval: unknown }).setInterval = realSetInterval;
  (globalThis as unknown as { clearInterval: unknown }).clearInterval = realClearInterval;
  check('no unhandled promise rejection escaped the helper', rejections.length === 0,
    JSON.stringify(rejections));
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
