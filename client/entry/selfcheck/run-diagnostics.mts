/* SPDX-License-Identifier: AGPL-3.0-or-later */
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
// Host-side checks for the pure diagnostics core (byte-identical import of
// client/entry/src/main/ets/vpnextensionability/NetBirdDiagnosticsCore.ets).
// Run: node client/entry/selfcheck/run-diagnostics.mts
//
// Pinned here: the fixed basenames/order and the 4 MiB total, the exact record
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
  check('four files, 1 MiB each, exactly 4 MiB total',
    m.UI_LOG_FILES_BY_AGE.length === 4 && m.PER_FILE_MAX_BYTES === 1048576 &&
    m.TOTAL_MAX_BYTES === 4194304 &&
    m.PER_FILE_MAX_BYTES * m.UI_LOG_FILES_BY_AGE.length === m.TOTAL_MAX_BYTES);
  check('line/value/name bounds',
    m.MAX_LINE_CHARS === 512 && m.MAX_VALUE_CHARS === 48 && m.MAX_EVENT_NAME_CHARS === 40);
  check('queue capacity and the per-role sample intervals',
    m.QUEUE_CAPACITY === 256 && m.SAMPLE_INTERVAL_MS === 1000 &&
    m.UI_SAMPLE_INTERVAL_MS === 10000);
  check('sampleIntervalFor picks the interval by role (ui 10 s, vpn 1 s)',
    m.sampleIntervalFor('vpn') === 1000 && m.sampleIntervalFor('ui') === 10000 &&
    m.sampleIntervalFor('vpn') === m.SAMPLE_INTERVAL_MS &&
    m.sampleIntervalFor('ui') === m.UI_SAMPLE_INTERVAL_MS);
  check('the detail cadence stays counted in SAMPLES, so ui details every 100 s',
    m.SAMPLE_INTERVAL_MS * m.MEM_DETAIL_INTERVAL === 10000 &&
    m.UI_SAMPLE_INTERVAL_MS * m.MEM_DETAIL_INTERVAL === 100000);
  check('the detail cadence is fixed at every 10th sample',
    m.MEM_DETAIL_INTERVAL === 10 && m.isDetailSample(10) === true &&
    m.isDetailSample(20) === true && m.isDetailSample(1) === false &&
    m.isDetailSample(9) === false && m.isDetailSample(11) === false &&
    m.isDetailSample(0) === false && m.isDetailSample(-10) === false);
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

  // ---- 0.0.26 gap 2: the VPN_* lifecycle markers reach netbird-vpn.log -------
  // The shell mirrors each VPN_* HiLog lifecycle marker through the SAME
  // diagnostics tee as event=<marker lowercased> + detail=<verbatim marker>.
  // Pinned here: (a) the marker token survives the credential guard (it must
  // NOT be redacted as an opaque secret) and (b) every required call site is
  // really present in the extension source (a wiring pin, since the byte-
  // identical .ets source is the artifact that ships).
  const markerRecord = m.buildRecord(ts, 42, 'vpn', 'vpn_protect_begin',
    { detail: 'VPN_PROTECT_BEGIN:stage=protect' }, false);
  check('a mirrored lifecycle marker survives redaction and keeps its values',
    markerRecord ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=vpn event=vpn_protect_begin ' +
    'detail=VPN_PROTECT_BEGIN:stage=protect\n',
    markerRecord);
  check('every mirrored marker name is a legal public event name',
    ['vpn_protect_begin', 'vpn_protect_resolved', 'vpn_protect_fail_closed',
      'vpn_ice_mux_armed', 'vpn_ice_mux_fail', 'vpn_create_begin',
      'vpn_create_resolved', 'vpn_create_rejected', 'vpn_create_invalid_fd',
      'vpn_connection_created', 'vpn_start_flow_rejected', 'vpn_destroy_begin',
      'vpn_destroy_resolved', 'vpn_destroy_rejected', 'vpn_destroy_skipped']
      .every((n: string): boolean => m.isPublicEventName(n)));
  {
    const ext = readFileSync(join(here, '..', 'src', 'main', 'ets',
      'vpnextensionability', 'NetBirdVpnExtensionAbility.ets'), 'utf8');
    check('the extension defines the shared lifecycle-marker mirror helper',
      ext.includes('private diagLifecycleMarker(marker: string, detail: string): void'));
    for (const marker of ['VPN_PROTECT_BEGIN', 'VPN_PROTECT_RESOLVED',
      'VPN_PROTECT_FAIL_CLOSED', 'VPN_ICE_MUX_ARMED', 'VPN_ICE_MUX_FAIL',
      'VPN_CREATE_BEGIN', 'VPN_CREATE_RESOLVED', 'VPN_CREATE_REJECTED',
      'VPN_CONNECTION_CREATED', 'VPN_START_FLOW_REJECTED', 'VPN_DESTROY_BEGIN',
      'VPN_DESTROY_RESOLVED', 'VPN_DESTROY_REJECTED', 'VPN_DESTROY_SKIPPED']) {
      check(`the ${marker} marker is mirrored into the app log`,
        ext.includes(`this.diagLifecycleMarker('${marker}'`));
    }
    // the HiLog line itself is untouched (only a mirror call was ADDED next to
    // each one) — the marker text is still passed to hilog verbatim.
    check('the HiLog markers themselves are unchanged (mirror is additive)',
      ext.includes("'VPN_PROTECT_BEGIN|requestId=%{public}s|fd=%{public}d'") &&
      ext.includes("'VPN_PROTECT_RESOLVED|requestId=%{public}s|fd=%{public}d|datagramsFlown=none-yet'") &&
      ext.includes('VPN_ICE_MUX_ARMED|requestId='));
  }

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
    m.INTERNAL_EVENT_NAMES.indexOf('diag_dropped') >= 0 &&
    m.INTERNAL_EVENT_NAMES.indexOf('mem_sample') >= 0 &&
    m.INTERNAL_EVENT_NAMES.indexOf('mem_detail') >= 0 &&
    m.INTERNAL_EVENT_NAMES.indexOf('mem_vm') >= 0 &&
    m.isPublicEventName('mem_detail') === false &&
    m.isPublicEventName('mem_vm') === false &&
    m.isPublicEventName('mem_sample') === false);

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
  // 2026-10-09 (0.0.17) onDestroy-entry stop marker: the SAME line shape, the
  // token fits the 8-char state bound exactly, and it is NOT a prior unclean
  // exit (isPriorUnclean claims state=open only) — that is what stops an
  // orderly user stop whose async `clean` transition was lost from being
  // reported as PRIOR_EXIT_UNCLEAN by the next session.
  const stoppingLine = m.buildMarker(ts, 4242, 'vpn', m.MARKER_STATE_STOPPING);
  const stoppingParsed = m.parseMarker(stoppingLine);
  check('the stopping marker round-trips exactly (state fits the 8-char bound)',
    m.MARKER_STATE_STOPPING === 'stopping' && stoppingLine ===
      'v=1 state=stopping ts=2026-10-06T12:00:00.000Z pid=4242 role=vpn\n' &&
    stoppingParsed !== null && stoppingParsed.state === 'stopping' &&
    stoppingParsed.pid === 4242 && stoppingParsed.role === 'vpn', stoppingLine);
  check('a stopping marker from another pid is NOT a prior unclean exit',
    m.isPriorUnclean(stoppingParsed, 2) === false &&
    m.isPriorUnclean(m.parseMarker(m.buildMarker(ts, 1, 'ui', 'stopping')), 2) === false);
  check('only an open marker is a prior unclean exit (open and stopping stay distinct)',
    m.buildMarker(ts, 1, 'ui', m.MARKER_STATE_OPEN) !==
      m.buildMarker(ts, 1, 'ui', m.MARKER_STATE_STOPPING) &&
    m.isPriorUnclean(m.parseMarker(m.buildMarker(ts, 1, 'ui', 'open')), 2) === true &&
    m.isPriorUnclean(m.parseMarker(m.buildMarker(ts, 1, 'ui', 'stopping')), 2) === false);

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
    m.PROC_STATUS_KEYS.join(',') ===
      'Threads,VmSize,RssAnon,RssFile,VmData,VmStk,RssShmem' &&
    m.PROC_MEMINFO_KEYS.join(',') === 'SwapTotal,SwapFree');

  const statusFixture = 'Name:\tnetbird\nUid:\t20020001\nThreads:\t42\nVmSize:\t 4194304 kB\n' +
    'VmRSS:\t 153000 kB\nRssAnon:\t 120000 kB\nRssFile:\t 33000 kB\nVmSwap:\t 0 kB\n' +
    'VmData:\t 1234567 kB\nVmStk:\t 4096 kB\nRssShmem:\t 8192 kB\n';
  const statusParsed = m.parseProcNumbers(statusFixture, m.PROC_STATUS_KEYS, m.PROC_MAX_LINES);
  check('proc parse: exactly the requested numeric keys are read (nothing else)',
    statusParsed.size === 7 && statusParsed.get('Threads') === 42 &&
    statusParsed.get('VmSize') === 4194304 && statusParsed.get('RssAnon') === 120000 &&
    statusParsed.get('RssFile') === 33000 && statusParsed.get('VmData') === 1234567 &&
    statusParsed.get('VmStk') === 4096 && statusParsed.get('RssShmem') === 8192 &&
    statusParsed.has('VmRSS') === false &&
    statusParsed.has('Name') === false && statusParsed.has('Uid') === false,
    JSON.stringify([...statusParsed.entries()]));
  check('the three added /proc keys ride the SAME one status read (no extra node)',
    m.PROC_STATUS_KEYS.length === 7 &&
    m.PROC_STATUS_KEYS.filter((k: string): boolean =>
      ['VmData', 'VmStk', 'RssShmem'].indexOf(k) >= 0).length === 3);
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

  const naResource = m.buildResourceFields(null, null, null, null, null, null, null, null, null,
    null, null, null);
  check('unavailable resource readings are explicit na, never 0',
    naResource['cpu_ms'] === 'na' && naResource['threads'] === 'na' &&
    naResource['vmsize_kb'] === 'na' && naResource['rss_anon_kb'] === 'na' &&
    naResource['rss_file_kb'] === 'na' && naResource['vmdata_kb'] === 'na' &&
    naResource['vmstk_kb'] === 'na' && naResource['rss_shmem_kb'] === 'na' &&
    naResource['sys_swap_total_kb'] === 'na' &&
    naResource['sys_swap_free_kb'] === 'na' && naResource['fd_count'] === 'na' &&
    naResource['fd_capped'] === 'na' && Object.keys(naResource).length === 12);
  const resFields = m.buildResourceFields('1234', '42', '4194304', '120000', '33000',
    '1234567', '4096', '8192', '0', '0', '7', false);
  check('resource fields carry exact units: cpu_ms / threads / *_kb / fd_count / fd_capped',
    resFields['cpu_ms'] === '1234' && resFields['threads'] === '42' &&
    resFields['vmsize_kb'] === '4194304' && resFields['rss_anon_kb'] === '120000' &&
    resFields['rss_file_kb'] === '33000' && resFields['vmdata_kb'] === '1234567' &&
    resFields['vmstk_kb'] === '4096' && resFields['rss_shmem_kb'] === '8192' &&
    resFields['sys_swap_total_kb'] === '0' &&
    resFields['sys_swap_free_kb'] === '0' && resFields['fd_count'] === '7' &&
    resFields['fd_capped'] === 'false');
  check('a full fd list is flagged as a lower bound (fd_capped=true)',
    m.buildResourceFields(null, null, null, null, null, null, null, null, null, null, '1024', true)['fd_capped'] === 'true' &&
    m.buildResourceFields(null, null, null, null, null, null, null, null, null, null, '7', false)['fd_capped'] === 'false');
  check('every resource key is internal-only (a public event cannot carry it)',
    Object.keys(naResource).every((k: string) => m.INTERNAL_FIELD_KEYS.indexOf(k) >= 0 &&
      m.PUBLIC_FIELD_KEYS.indexOf(k) < 0));
  check('a public event cannot smuggle a resource key through',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { cpu_ms: '1', fd_count: '2', threads: '3' }, false) ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=ui_ok\n');
  const mergedSample = m.mergeFields({ rss_kb: '1' },
    m.buildResourceFields('2', null, null, null, null, null, null, null, null, null, null, null));
  check('mergeFields puts both halves in ONE record',
    mergedSample['rss_kb'] === '1' && mergedSample['cpu_ms'] === '2' &&
    mergedSample['threads'] === 'na' && Object.keys(mergedSample).length === 13);
  const fullSample = m.buildRecord(ts, 42, 'vpn', 'mem_sample',
    m.mergeFields(m.buildSampleFields('45120', '3', '8192', '1234567'),
      m.buildResourceFields('1234', '42', '4194304', '120000', '33000', '1234567',
        '4096', '8192', '2097152', '1048576', '7', false)), true);
  check('exact combined mem_sample record: memory + resource fields, one bounded line',
    fullSample === 'ts=2026-10-06T12:00:00.000Z pid=42 role=vpn event=mem_sample ' +
      'cpu_ms=1234 fd_capped=false fd_count=7 heap_kb=8192 rss_anon_kb=120000 ' +
      'rss_file_kb=33000 rss_kb=45120 rss_shmem_kb=8192 swap_kb=3 sys_avail_kb=1234567 ' +
      'sys_swap_free_kb=1048576 sys_swap_total_kb=2097152 threads=42 vmdata_kb=1234567 ' +
      'vmsize_kb=4194304 vmstk_kb=4096\n',
    fullSample);
  check('the fully populated sample stays well inside the 512-char line bound',
    fullSample.length < m.MAX_LINE_CHARS && fullSample.indexOf('~') < 0,
    `len=${fullSample.length}`);
  check('the per-tick mem_sample carries the three added /proc keys',
    fullSample.indexOf(' vmdata_kb=1234567 ') > 0 &&
    fullSample.indexOf(' vmstk_kb=4096\n') > 0 &&
    fullSample.indexOf(' rss_shmem_kb=8192 ') > 0);
  // Why the detail readings became a SEPARATE record: even a mem_sample whose
  // every field sits at its own type/parser bound (proc <= MAX_PROC_DIGITS,
  // hidebug bigints at u64, fd_count <= FD_LIST_MAX, cpu_ms at the longest
  // decimal a finite number can print) stays under the line bound, while the
  // same record with PSS + the Rust heap added would not.
  const procMaxText = '9'.repeat(m.MAX_PROC_DIGITS);
  const bigMaxText = '18446744073709551615';
  const widestSample = m.buildRecord(ts, 4242, 'vpn', 'mem_sample',
    m.mergeFields(m.buildSampleFields(bigMaxText, bigMaxText, bigMaxText, bigMaxText),
      m.buildResourceFields(String(Number.MAX_VALUE), procMaxText, procMaxText, procMaxText,
        procMaxText, procMaxText, procMaxText, procMaxText, procMaxText, procMaxText,
        String(m.FD_LIST_MAX), true)), true);
  check('even the widest reachable mem_sample (every field at its own bound) fits 512',
    widestSample.length < m.MAX_LINE_CHARS && widestSample.indexOf('~') < 0,
    `len=${widestSample.length}`);

  // ---- the every-10th mem_detail record (separate line) --------------------
  const naNative = m.buildNativeMemFields(null, null, null, null, null);
  check('unavailable smaps readings are explicit na, never 0',
    naNative['pss_kb'] === 'na' && naNative['private_dirty_kb'] === 'na' &&
    naNative['shared_dirty_kb'] === 'na' && naNative['private_clean_kb'] === 'na' &&
    naNative['shared_clean_kb'] === 'na' && Object.keys(naNative).length === 5);
  const nativeFields = m.buildNativeMemFields('204800', '120000', '8000', '64000', '4096');
  check('the smaps readings keep their own KB text (pss = Pss+SwapPss, no arithmetic here)',
    nativeFields['pss_kb'] === '204800' && nativeFields['private_dirty_kb'] === '120000' &&
    nativeFields['shared_dirty_kb'] === '8000' && nativeFields['private_clean_kb'] === '64000' &&
    nativeFields['shared_clean_kb'] === '4096');
  const naRust = m.buildRustAllocFields(null);
  check('no Rust reader / no document renders all four rust keys as na, never 0',
    naRust['rust_heap_kb'] === 'na' && naRust['rust_heap_peak_kb'] === 'na' &&
    naRust['rust_allocs'] === 'na' && naRust['rust_frees'] === 'na' &&
    Object.keys(naRust).length === 4);
  const rustParsed = m.buildRustAllocFields('{"bytes":1048576,"peak":2097152,"allocs":77,"frees":12}');
  check('the alloc_stats document converts bytes to KiB by integer division',
    rustParsed['rust_heap_kb'] === '1024' && rustParsed['rust_heap_peak_kb'] === '2048' &&
    rustParsed['rust_allocs'] === '77' && rustParsed['rust_frees'] === '12');
  check('a sub-KiB Rust heap floors to an honest 0 (a real reading, not na)',
    m.buildRustAllocFields('{"bytes":1023,"peak":1023,"allocs":1,"frees":0}')['rust_heap_kb'] === '0');
  check('malformed / anomalous alloc documents degrade to na, never a fabricated number',
    m.buildRustAllocFields('not-json')['rust_heap_kb'] === 'na' &&
    m.buildRustAllocFields('[]')['rust_heap_kb'] === 'na' &&
    m.buildRustAllocFields('7')['rust_heap_kb'] === 'na' &&
    m.buildRustAllocFields('null')['rust_heap_kb'] === 'na' &&
    m.buildRustAllocFields('{"bytes":"1","peak":-1,"allocs":null,"frees":2}')['rust_heap_kb'] === 'na' &&
    m.buildRustAllocFields('{"bytes":"1","peak":-1,"allocs":null,"frees":2}')['rust_heap_peak_kb'] === 'na' &&
    m.buildRustAllocFields('{"bytes":"1","peak":-1,"allocs":null,"frees":2}')['rust_allocs'] === 'na' &&
    m.buildRustAllocFields('{"bytes":"1","peak":-1,"allocs":null,"frees":2}')['rust_frees'] === '2');
  const detailRecord = m.buildRecord(ts, 42, 'vpn', 'mem_detail',
    m.mergeFields(m.buildNativeMemFields('204800', '120000', '8000', '64000', '4096'),
      m.buildRustAllocFields('{"bytes":1048576,"peak":2097152,"allocs":77,"frees":12}')), true);
  check('exact mem_detail record: smaps + rust fields, one bounded line',
    detailRecord === 'ts=2026-10-06T12:00:00.000Z pid=42 role=vpn event=mem_detail ' +
      'private_clean_kb=64000 private_dirty_kb=120000 pss_kb=204800 rust_allocs=77 ' +
      'rust_frees=12 rust_heap_kb=1024 rust_heap_peak_kb=2048 shared_clean_kb=4096 ' +
      'shared_dirty_kb=8000\n',
    detailRecord);
  check('the widest possible mem_detail stays inside the 512-char line bound',
    detailRecord.length < m.MAX_LINE_CHARS && detailRecord.indexOf('~') < 0,
    `len=${detailRecord.length}`);
  const detailWide = m.buildRecord(ts, 42, 'vpn', 'mem_detail',
    m.mergeFields(
      m.buildNativeMemFields('18446744073709551615', '18446744073709551615',
        '18446744073709551615', '18446744073709551615', '18446744073709551615'),
      m.buildRustAllocFields('{"bytes":18446744073709551615,"peak":18446744073709551615,' +
        '"allocs":18446744073709551615,"frees":18446744073709551615}')), true);
  check('even u64-max readings fit the mem_detail line (that is why it is a separate record)',
    detailWide.length < m.MAX_LINE_CHARS && detailWide.indexOf('~') < 0,
    `len=${detailWide.length}`);
  check('the detail keys are internal-only (a public event cannot carry them)',
    Object.keys(naNative).concat(Object.keys(naRust)).every((k: string) =>
      m.INTERNAL_FIELD_KEYS.indexOf(k) >= 0 && m.PUBLIC_FIELD_KEYS.indexOf(k) < 0));
  check('a public event cannot smuggle a detail key through',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { pss_kb: '1', rust_heap_kb: '2' }, false) ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=ui_ok\n');

  // ---- the SAME tick's third record: event=mem_vm --------------------------
  // The ArkTS-VM / native-malloc / ArkTS-GC attribution readings. It is a third
  // line (not a wider mem_detail) for the same reason mem_detail is a second
  // line: at every field's own bound the joined sets overflow MAX_LINE_CHARS,
  // and a cut line silently loses its tail.
  check('the GC item names are the API 26 declaration\'s own keys',
    m.GC_STAT_GC_COUNT === 'ark.gc.gc-count' && m.GC_STAT_GC_TIME === 'ark.gc.gc-time' &&
    m.GC_STAT_BYTES_ALLOCATED === 'ark.gc.gc-bytes-allocated' &&
    m.GC_STAT_BYTES_FREED === 'ark.gc.gc-bytes-freed');
  const naVm = m.buildVmMemFields(null, null, null, null, null, null, null, null, null);
  check('unavailable VM / native-heap / GC readings are explicit na, never 0',
    naVm['vm_total_heap_kb'] === 'na' && naVm['vm_array_kb'] === 'na' &&
    naVm['native_heap_kb'] === 'na' && naVm['native_heap_alloc_kb'] === 'na' &&
    naVm['native_heap_free_kb'] === 'na' && naVm['gc_count'] === 'na' &&
    naVm['gc_time_ms'] === 'na' && naVm['gc_alloc_kb'] === 'na' &&
    naVm['gc_freed_kb'] === 'na' && Object.keys(naVm).length === 9);
  // The platform's own memory limit (getAppMemoryLimit, API >= 12): three keys,
  // KILOBYTES as the platform reports them — the pure core validates the decimal
  // text and NOTHING else (no division, no scaling).
  const naLimit = m.buildMemLimitFields(null, null, null);
  check('unavailable app-memory-limit readings are explicit na, never 0',
    naLimit['rss_limit_kb'] === 'na' && naLimit['vss_limit_kb'] === 'na' &&
    naLimit['vm_heap_limit_kb'] === 'na' && Object.keys(naLimit).length === 3);
  const limitFields = m.buildMemLimitFields('262144', '4194304', '524288');
  check('the app-memory-limit fields keep the platform KiB text unchanged',
    limitFields['rss_limit_kb'] === '262144' && limitFields['vss_limit_kb'] === '4194304' &&
    limitFields['vm_heap_limit_kb'] === '524288');
  check('a limit is already KiB: the text is validated, never divided',
    m.MAX_LIMIT_DIGITS === 20 && m.limitKiBText('0') === '0' &&
    m.limitKiBText('262144') === '262144' && m.limitKiBText('1'.repeat(20)) === '1'.repeat(20) &&
    m.limitKiBText(null) === null && m.limitKiBText('') === null &&
    m.limitKiBText('-1') === null && m.limitKiBText(' 1024') === null &&
    m.limitKiBText('1.5e+6') === null && m.limitKiBText('1024k') === null &&
    m.limitKiBText('1'.repeat(21)) === null);
  const vmFields = m.buildVmMemFields('16384', '4096', '20480', '12288', '8191',
    '77', '512', '102400', '51199');
  check('the mem_vm fields keep their own units (KB / ms / counts), no arithmetic here',
    vmFields['vm_total_heap_kb'] === '16384' && vmFields['vm_array_kb'] === '4096' &&
    vmFields['native_heap_kb'] === '20480' && vmFields['native_heap_alloc_kb'] === '12288' &&
    vmFields['native_heap_free_kb'] === '8191' && vmFields['gc_count'] === '77' &&
    vmFields['gc_time_ms'] === '512' && vmFields['gc_alloc_kb'] === '102400' &&
    vmFields['gc_freed_kb'] === '51199');
  check('a byte counter is divided to KiB by exact floor division',
    m.BYTES_PER_KIB === 1024 && m.bytesToKiBText('0') === '0' &&
    m.bytesToKiBText('1023') === '0' && m.bytesToKiBText('1024') === '1' &&
    m.bytesToKiBText('1048575') === '1023' && m.bytesToKiBText('1048576') === '1024' &&
    m.bytesToKiBText('104858623') === '102400' && m.bytesToKiBText('52428799') === '51199');
  check('the exact division holds at the u64 maximum (no bigint -> number step)',
    m.bytesToKiBText('18446744073709551615') === '18014398509481983' &&
    m.bytesToKiBText(String(BigInt(Number.MAX_SAFE_INTEGER) * 1024n)) ===
      String(Number.MAX_SAFE_INTEGER));
  check('an empty / non-decimal / over-long byte text is na, never a fabricated value',
    m.bytesToKiBText(null) === null && m.bytesToKiBText('') === null &&
    m.bytesToKiBText('1.5e+21') === null && m.bytesToKiBText('12345x') === null &&
    m.bytesToKiBText('-1024') === null && m.bytesToKiBText(' 1024') === null &&
    m.bytesToKiBText('1'.repeat(m.MAX_BYTE_DIGITS + 1)) === null &&
    m.bytesToKiBText('1'.repeat(m.MAX_BYTE_DIGITS)) !== null);
  check('a GC statistic must be a finite, non-negative number',
    m.statNumberText(0) === '0' && m.statNumberText(77) === '77' &&
    m.statNumberText(512.5) === '512.5' && m.statNumberText(NaN) === null &&
    m.statNumberText(Infinity) === null && m.statNumberText(-Infinity) === null &&
    m.statNumberText(-1) === null);
  const vmRecord = m.buildRecord(ts, 42, 'vpn', 'mem_vm',
    m.mergeFields(
      m.buildVmMemFields('16384', '4096', '20480', '12288', '8191', '77', '512',
        '102400', '51199'),
      m.buildMemLimitFields('262144', '4194304', '524288')),
    true);
  check('exact mem_vm record: VM heap + native heap + GC + the app memory limit',
    vmRecord === 'ts=2026-10-06T12:00:00.000Z pid=42 role=vpn event=mem_vm ' +
      'gc_alloc_kb=102400 gc_count=77 gc_freed_kb=51199 gc_time_ms=512 ' +
      'native_heap_alloc_kb=12288 native_heap_free_kb=8191 native_heap_kb=20480 ' +
      'rss_limit_kb=262144 vm_array_kb=4096 vm_heap_limit_kb=524288 ' +
      'vm_total_heap_kb=16384 vss_limit_kb=4194304\n',
    vmRecord);
  check('the fully populated mem_vm stays inside the 512-char line bound',
    vmRecord.length < m.MAX_LINE_CHARS && vmRecord.indexOf('~') < 0,
    `len=${vmRecord.length}`);
  const vmWide = m.buildRecord(ts, 42, 'vpn', 'mem_vm',
    m.mergeFields(
      m.buildVmMemFields(bigMaxText, bigMaxText, bigMaxText, bigMaxText, bigMaxText,
        String(Number.MAX_VALUE), String(Number.MAX_VALUE), bigMaxText, bigMaxText),
      m.buildMemLimitFields(bigMaxText, bigMaxText, bigMaxText)), true);
  check('even the widest reachable mem_vm (every field at its own bound) fits 512',
    vmWide.length < m.MAX_LINE_CHARS && vmWide.indexOf('~') < 0, `len=${vmWide.length}`);
  const joinedWide = m.buildRecord(ts, 42, 'vpn', 'mem_detail',
    m.mergeFields(m.mergeFields(
      m.buildNativeMemFields(bigMaxText, bigMaxText, bigMaxText, bigMaxText, bigMaxText),
      m.buildRustAllocFields('{"bytes":18446744073709551615,"peak":18446744073709551615,' +
        '"allocs":18446744073709551615,"frees":18446744073709551615}')),
      m.mergeFields(
        m.buildVmMemFields(bigMaxText, bigMaxText, bigMaxText, bigMaxText, bigMaxText,
          String(Number.MAX_VALUE), String(Number.MAX_VALUE), bigMaxText, bigMaxText),
        m.buildMemLimitFields(bigMaxText, bigMaxText, bigMaxText))), true);
  check('the mem_detail keys plus the mem_vm keys exceed 512 (hence the third line)',
    joinedWide.length === m.MAX_LINE_CHARS && joinedWide.endsWith('~\n') &&
    joinedWide.indexOf('~') === joinedWide.length - 2,
    `len=${joinedWide.length}`);
  check('the mem_vm keys are internal-only (a public event cannot carry them)',
    Object.keys(naVm).concat(Object.keys(naLimit)).every((k: string) =>
      m.INTERNAL_FIELD_KEYS.indexOf(k) >= 0 && m.PUBLIC_FIELD_KEYS.indexOf(k) < 0));
  check('a public event cannot smuggle a mem_vm key through',
    m.buildRecord(ts, 42, 'ui', 'ui_ok', { gc_count: '1', vm_array_kb: '2' }, false) ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=ui_ok\n');
  check('a public event cannot smuggle an app-memory-limit key through',
    m.buildRecord(ts, 42, 'ui', 'ui_ok',
      { rss_limit_kb: '1', vss_limit_kb: '2', vm_heap_limit_kb: '3' }, false) ===
    'ts=2026-10-06T12:00:00.000Z pid=42 role=ui event=ui_ok\n');

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
  check('the only smaps_rollup reader is the ASYNC one (blocking variants absent)',
    runtimeCode.includes('await hidebug.getAppNativeMemInfoAsync()') &&
    !/getPss|getAppNativeMemInfo\(|getAppNativeMemInfoWithCache|getSharedDirty|getPrivateDirty/.test(runtimeCode));
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
  check('runtime reads the three added status keys from the SAME one status read',
    runtimeCode.includes("procNumberText(values.get('VmData'))") &&
    runtimeCode.includes("procNumberText(values.get('VmStk'))") &&
    runtimeCode.includes("procNumberText(values.get('RssShmem'))") &&
    runtimeCode.split('readProcText(PROC_SELF_STATUS').length === 2);
  check('runtime emits mem_detail only from the detail branch of the ONE runSample',
    runtimeCode.includes('isDetailSample(this.sampleIndex)') &&
    runtimeCode.includes('await this.collectDetail()') &&
    runtimeCode.includes("emitInternal('mem_sample', fields)") &&
    runtimeCode.includes("emitInternal('mem_detail', detailFields)") &&
    runtimeCode.indexOf("emitInternal('mem_detail', detailFields)") >
      runtimeCode.indexOf("emitInternal('mem_sample', fields)") &&
    runtimeCode.split('emitInternal(\'mem_detail\'').length === 2);
  check('runtime keeps the single-flight guard around the awaited detail reading',
    runtimeCode.includes('private async runSample(generation: number, detail: boolean)') &&
    runtimeCode.includes('this.sampling = true;') &&
    runtimeCode.includes('if (generation === this.generation) {') &&
    runtimeCode.indexOf('this.sampling = true;') < runtimeCode.indexOf('await this.collectDetail()'));
  check('runtime parses the alloc document through the pure core (no ad-hoc JSON here)',
    runtimeCode.includes('buildRustAllocFields(allocText)') &&
    runtimeCode.includes('buildNativeMemFields(pssKb, privateDirtyKb, sharedDirtyKb, privateCleanKb, sharedCleanKb)') &&
    runtimeCode.split('JSON.parse').length === 1);
  check('runtime emits mem_vm from the SAME detail branch, right after mem_detail',
    runtimeCode.includes('vmFields = this.collectVmMem();') &&
    runtimeCode.includes("emitInternal('mem_vm', vmFields)") &&
    runtimeCode.split("emitInternal('mem_vm'").length === 2 &&
    runtimeCode.indexOf("emitInternal('mem_vm', vmFields)") >
      runtimeCode.indexOf("emitInternal('mem_detail', detailFields)") &&
    runtimeCode.indexOf('await this.collectDetail()') <
      runtimeCode.indexOf('vmFields = this.collectVmMem();'));
  check('the mem_vm legs are synchronous and add no timer inside the single-flight window',
    runtimeCode.includes('private collectVmMem(): DiagFields {') &&
    !runtimeCode.includes('private async collectVmMem(') &&
    runtimeCode.split('setInterval(').length === 2 &&
    runtimeCode.indexOf('this.sampling = true;') < runtimeCode.indexOf('vmFields = this.collectVmMem();'));
  check('the mem_vm legs read exactly the verified synchronous APIs',
    runtimeCode.includes('hidebug.getAppVMMemoryInfo()') &&
    runtimeCode.includes('hidebug.getNativeHeapSize()') &&
    runtimeCode.includes('hidebug.getNativeHeapAllocatedSize()') &&
    runtimeCode.includes('hidebug.getNativeHeapFreeSize()') &&
    runtimeCode.includes('hidebug.getVMRuntimeStat(GC_STAT_GC_COUNT)') &&
    runtimeCode.includes('hidebug.getVMRuntimeStat(GC_STAT_GC_TIME)') &&
    runtimeCode.includes('hidebug.getVMRuntimeStat(GC_STAT_BYTES_ALLOCATED)') &&
    runtimeCode.includes('hidebug.getVMRuntimeStat(GC_STAT_BYTES_FREED)') &&
    runtimeCode.includes('hidebug.getAppMemoryLimit()'));
  check('runtime spells no GC item name and converts bytes through the pure core',
    runtimeCode.indexOf('ark.gc.') < 0 &&
    runtimeCode.includes('bytesToKiBText(hidebug.getNativeHeapSize().toString())') &&
    runtimeCode.includes('bytesToKiBText(hidebug.getVMRuntimeStat(GC_STAT_BYTES_ALLOCATED).toString())') &&
    runtimeCode.includes('statNumberText(hidebug.getVMRuntimeStat(GC_STAT_GC_COUNT))') &&
    runtimeCode.includes('buildVmMemFields(vmTotalHeapKb, vmArrayKb, nativeHeapKb, nativeHeapAllocKb,'));
  // The app memory limit (analysis report extra measurement (a)): ONE reader for
  // both records, the platform's KiB bigints validated (never scaled) by the
  // pure core, and the once-per-process-start log ON the diag_start record.
  check('the app memory limit is read through the core and logged once per process start',
    runtimeCode.includes('limitKiBText(limit.rssLimit.toString())') &&
    runtimeCode.includes('limitKiBText(limit.vssLimit.toString())') &&
    runtimeCode.includes('limitKiBText(limit.vmHeapLimit.toString())') &&
    runtimeCode.includes('buildMemLimitFields(rssLimitKb, vssLimitKb, vmHeapLimitKb)') &&
    runtimeCode.includes("emitInternal('diag_start', this.readMemLimit(false))") &&
    runtimeCode.includes('this.readMemLimit(true)') &&
    !/rssLimit\s*[/+*-]/.test(runtimeCode) && !/[/+*-]\s*limit\.rssLimit/.test(runtimeCode));
  check('the mem-limit absence is reported by the detail leg only (the start leg is silent)',
    runtimeCode.split('this.readMemLimit(false)').length === 2 &&
    runtimeCode.split('this.readMemLimit(true)').length === 2 &&
    runtimeCode.indexOf("this.noteDetailUnavailable('mem-limit')") >
      runtimeCode.indexOf('private readMemLimit(noteFailure: boolean): DiagFields {'));

  // ---- decodeAscii: block decoding is identical to the per-byte loop -------
  // The runtime decodes its fixed 16 KiB buffer itself (no @ohos.util — pinned
  // above), so this pins the rewrite against the ORIGINAL algorithm, reproduced
  // here as the reference: byte i -> charCode i. That includes 0x80..0xff, which
  // must stay single U+0080..U+00FF chars and never become a UTF-8 replacement
  // char. The samples cover an empty read, both sides of the block boundary, the
  // exact 16 KiB cap and a partial read inside a larger buffer.
  const decodeMatch =
    /function decodeAscii\(buffer: ArrayBuffer, length: number\): string \{[\s\S]*?\n\}/
      .exec(runtimeText);
  const decodeBody = decodeMatch === null
    ? 'function decodeAscii(): string { return \'<decodeAscii not found>\'; }'
    : decodeMatch[0];
  const decodeFixture = join(dir, 'decodeAscii.ts');
  writeFileSync(decodeFixture, decodeBody + '\nexport { decodeAscii };\n');
  const decodeModule = await import(pathToFileURL(decodeFixture).href);
  const decodeRef = (buffer: ArrayBuffer, length: number): string => {
    const bytes: Uint8Array = new Uint8Array(buffer, 0, length);
    let text: string = '';
    for (let i = 0; i < length; i++) { text += String.fromCharCode(bytes[i]); }
    return text;
  };
  const everyByte = new Uint8Array(256);
  for (let i = 0; i < 256; i++) { everyByte[i] = i; }
  const capSized = new Uint8Array(m.PROC_READ_MAX_BYTES);
  for (let i = 0; i < capSized.length; i++) { capSized[i] = (i * 7 + 0x80) & 0xff; }
  const procDocText = 'Name:\tnetbird\nThreads:\t42\nVmData:\t 1234567 kB\nRssShmem:\t 8192 kB\n';
  const procDocBytes = new Uint8Array(procDocText.length);
  for (let i = 0; i < procDocBytes.length; i++) { procDocBytes[i] = procDocText.charCodeAt(i); }
  const decodeSamples: Array<{ name: string; buffer: ArrayBuffer; length: number }> = [
    { name: 'empty read', buffer: new ArrayBuffer(8), length: 0 },
    { name: 'one byte below a block', buffer: capSized.buffer, length: 15 },
    { name: 'exactly one block', buffer: capSized.buffer, length: 16 },
    { name: 'one byte above a block', buffer: capSized.buffer, length: 17 },
    { name: 'all 256 byte values', buffer: everyByte.buffer, length: 256 },
    { name: 'a partial read inside a larger buffer', buffer: everyByte.buffer, length: 17 },
    { name: 'a short proc document', buffer: procDocBytes.buffer, length: procDocBytes.length },
    { name: 'one byte below the 16 KiB cap', buffer: capSized.buffer,
      length: m.PROC_READ_MAX_BYTES - 1 },
    { name: 'the exact 16 KiB cap', buffer: capSized.buffer, length: m.PROC_READ_MAX_BYTES }
  ];
  const decodeBad: string[] = [];
  for (const s of decodeSamples) {
    if (decodeModule.decodeAscii(s.buffer, s.length) !== decodeRef(s.buffer, s.length)) {
      decodeBad.push(s.name);
    }
  }
  check('decodeAscii: the block rewrite is byte-identical to the per-byte loop on every sample',
    decodeBad.length === 0, decodeBad.join(', '));
  const everyByteText = decodeModule.decodeAscii(everyByte.buffer, 256);
  check('decodeAscii: 0x80..0xff stay one char each, never a UTF-8 replacement char',
    everyByteText.length === 256 && everyByteText.charCodeAt(0x00) === 0x00 &&
    everyByteText.charCodeAt(0x7f) === 0x7f && everyByteText.charCodeAt(0x80) === 0x80 &&
    everyByteText.charCodeAt(0xff) === 0xff && everyByteText.indexOf('\uFFFD') < 0);
  const capText = decodeModule.decodeAscii(capSized.buffer, m.PROC_READ_MAX_BYTES);
  check('decodeAscii: the empty read is empty and the 16 KiB read loses nothing',
    decodeModule.decodeAscii(new ArrayBuffer(8), 0) === '' &&
    capText.length === m.PROC_READ_MAX_BYTES &&
    capText === decodeRef(capSized.buffer, m.PROC_READ_MAX_BYTES));
  check('decodeAscii: a single-byte and an unterminated proc document still decode',
    decodeModule.decodeAscii(everyByte.buffer, 1) === '\u0000' &&
    decodeModule.decodeAscii(procDocBytes.buffer, procDocBytes.length) === procDocText);

  // ---- fake platform -------------------------------------------------------
  const PROC_STATUS = '/proc/self/status';
  const PROC_MEMINFO = '/proc/meminfo';
  const PROC_FD = '/proc/self/fd';
  const STATUS_FIXTURE = 'Name:\tnetbird\nUid:\t20020001\nThreads:\t42\nVmSize:\t 4194304 kB\n' +
    'VmRSS:\t 153000 kB\nRssAnon:\t 120000 kB\nRssFile:\t 33000 kB\nVmSwap:\t 0 kB\n' +
    'VmData:\t 1234567 kB\nVmStk:\t 4096 kB\nRssShmem:\t 8192 kB\n';
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
  // The synchronous mem_vm legs that can fail on their own: the native
  // (mallinfo) heap, the ArkTS GC statistics and the platform memory limit
  // (getAppMemoryLimit, API >= 12).
  let nativeHeapThrows = false;
  let gcStatThrows = false;
  let memLimitThrows = false;
  let procStatusText = STATUS_FIXTURE;
  let procMeminfoText = MEMINFO_FIXTURE;
  let denyProcOpen = ''; // exact proc path whose open throws EACCES
  let fdDenied = false;
  let fdNames: string[] = ['0', '1', '2', '3', '4', '5', '6'];
  let hangRead: Promise<void> | null = null; // when set, every read awaits it
  let procReadCalls = 0;
  let procListCalls = 0;
  let procReadCap = 0;
  // Read-buffer identity per proc node. The runtime REUSES one 16 KiB buffer per
  // node (NetBirdDiagnostics.ets readProcText), so every read of a node must hand
  // the fake the SAME ArrayBuffer object, and the two nodes must never share one.
  // `changes` counts how many distinct buffer objects a node has been read into.
  const procReadBuffers = new Map<string, ArrayBuffer>();
  const procReadBufferChanges = new Map<string, number>();
  // Per-node read gates: an ARMED node makes every read of that node wait, so a
  // test can hold one node's read while the other node's read is issued — the
  // cross-node overlap a stop()/start() pair can produce.
  const procReadGates = new Map<string, { promise: Promise<void>; release: () => void }>();
  const armProcReadGate = (p: string): void => {
    let release: () => void = (): void => undefined;
    const promise = new Promise<void>((resolve: () => void): void => { release = resolve; });
    procReadGates.set(p, { promise, release });
  };
  const releaseProcReadGate = (p: string): void => {
    const gate = procReadGates.get(p);
    if (gate !== undefined) { gate.release(); procReadGates.delete(p); }
  };
  // The every-10th detail reading (hidebug.getAppNativeMemInfoAsync): its own
  // throw latch, its own pending promise and its own call counter, so a hung
  // PSS read can be told apart from a hung /proc read.
  let nativeMemCalls = 0;
  let nativeMemThrows = false;
  let hangNativeMem: Promise<void> | null = null;
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
      if (procReadBuffers.get(p) !== buffer) {
        procReadBuffers.set(p, buffer);
        procReadBufferChanges.set(p, (procReadBufferChanges.get(p) ?? 0) + 1);
      }
      const gate = procReadGates.get(p);
      if (gate !== undefined) { await gate.promise; }
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
    getAppVMMemoryInfo: (): { totalHeap: bigint; heapUsed: bigint; allArraySize: bigint } => {
      if (memThrow) { throw new Error('unsupported'); }
      return { totalHeap: 16384n, heapUsed: 8192n, allArraySize: 4096n };
    },
    getSystemMemInfo: (): { availableMem: bigint } => {
      if (memThrow) { throw new Error('unsupported'); }
      return { availableMem: 1234567n };
    },
    // mallinfo, in BYTES. The byte counts deliberately do not sit on a KiB
    // boundary: 20480 KiB + 1023 B, 12288 KiB + 1023 B, 8192 KiB - 1 B — so the
    // emitted KiB values can only come from an honest floor division.
    getNativeHeapSize: (): bigint => {
      if (nativeHeapThrows) { throw new Error('unsupported'); }
      return 20972543n; // 20480 KiB + 1023 B
    },
    getNativeHeapAllocatedSize: (): bigint => {
      if (nativeHeapThrows) { throw new Error('unsupported'); }
      return 12583935n; // 12288 KiB + 1023 B
    },
    getNativeHeapFreeSize: (): bigint => {
      if (nativeHeapThrows) { throw new Error('unsupported'); }
      return 8388607n; // 8192 KiB - 1 B
    },
    // ArkTS GC statistics (API >= 12). An unknown item is the BusinessError the
    // declaration documents (:401), so the fake throws it like the platform.
    getVMRuntimeStat: (item: string): number => {
      if (gcStatThrows) { throw new Error('unsupported'); }
      if (item === m.GC_STAT_GC_COUNT) { return 77; }
      if (item === m.GC_STAT_GC_TIME) { return 512; }
      if (item === m.GC_STAT_BYTES_ALLOCATED) { return 104858623; } // 102400 KiB + 1023 B
      if (item === m.GC_STAT_BYTES_FREED) { return 52428799; } // 51200 KiB - 1 B
      throw new Error('unknown property');
    },
    // API >= 20 (the only smaps_rollup reader this module may use).
    getAppNativeMemInfoAsync: async (): Promise<Record<string, bigint>> => {
      nativeMemCalls++;
      if (hangNativeMem !== null) { await hangNativeMem; }
      if (nativeMemThrows) { throw new Error('unsupported'); }
      return { pss: 204800n, vss: 4194304n, rss: 200000n, sharedDirty: 8000n,
        privateDirty: 120000n, sharedClean: 4096n, privateClean: 64000n };
    },
    // API >= 12: the platform's own per-process memory limit, in KILOBYTES
    // (rssLimit 256 MiB / vssLimit 4 GiB / vmHeapLimit 512 MiB). The values are
    // already KiB, so any scaling in the module would be visible as a wrong
    // record. `memLimitThrows` is its OWN latch so the graceful-absence path can
    // be driven without taking the other mem_vm legs down with it.
    getAppMemoryLimit: (): Record<string, bigint> => {
      if (memLimitThrows) { throw new Error('unsupported'); }
      return { rssLimit: 262144n, vssLimit: 4194304n, vmHeapLimit: 524288n,
        vmTotalHeapSize: 1048576n };
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
  // 2026-10-12 (analysis report extra measurement (a)): the platform's own
  // per-process memory limit is logged ONCE PER PROCESS START, on the run's
  // first record — hidebug.getAppMemoryLimit()'s three KiB fields, no scaling.
  const startFields = fieldsOf(a1[0]);
  check('ui run: diag_start carries the app memory limit once per process start',
    startFields.get('rss_limit_kb') === '262144' &&
    startFields.get('vss_limit_kb') === '4194304' &&
    startFields.get('vm_heap_limit_kb') === '524288' &&
    a1[0].length < m.MAX_LINE_CHARS,
    a1[0]);
  check('ui run: the limit keys never ride the per-tick mem_sample line',
    fieldsOf(a1[1]).get('rss_limit_kb') === undefined &&
    fieldsOf(a1[1]).get('vss_limit_kb') === undefined &&
    fieldsOf(a1[1]).get('vm_heap_limit_kb') === undefined, a1[1]);
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
  check('ui run: VmData/VmStk/RssShmem ride the SAME status read (three added keys)',
    sampleFields.get('vmdata_kb') === '1234567' && sampleFields.get('vmstk_kb') === '4096' &&
    sampleFields.get('rss_shmem_kb') === '8192', a1[1]);
  check('ui run: system swap comes from /proc/meminfo (MemAvailable is not duplicated)',
    sampleFields.get('sys_swap_total_kb') === '2097152' &&
    sampleFields.get('sys_swap_free_kb') === '1048576' &&
    sampleFields.get('sys_avail_kb') === '1234567', a1[1]);
  check('ui run: the fd count is a count only and is not flagged as capped',
    sampleFields.get('fd_count') === '7' && sampleFields.get('fd_capped') === 'false', a1[1]);
  check('ui run: the mem_sample record carries no proc path or fd name',
    a1[1].indexOf('/proc/') < 0 && a1[1].indexOf('fdinfo') < 0);
  check('ui run: the ONE sampling interval is registered at 10 s',
    timers.length === 1 && timers[0].ms === 10000 && timers[0].live === true);
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
  check('rotation: every file is under the per-file cap and the total is under 4 MiB',
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
  check('vpn role: the ONE sampling interval keeps the 1 s period',
    timers.length === beforeD + 1 && timers[beforeD].ms === 1000 &&
    timers[beforeD].live === true);
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
  // The dropped tick must not leave the in-flight flag stuck: the NEXT tick,
  // after the previous sample has settled, runs its own read and lands again.
  const readsAfterSettle = procReadCalls;
  timers[timers.length - 1].cb();
  await settle();
  const concTick = linesOf(join(dirConc, 'netbird-ui.log'));
  check('single-flight: after the sample settles the next tick samples again',
    procReadCalls > readsAfterSettle &&
    concTick.filter((l: string): boolean => l.includes('event=mem_sample')).length === 2,
    `${procReadCalls - readsAfterSettle} read(s), ${concTick.join(' ~ ')}`);
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
    concStop.filter((l: string): boolean => l.includes('event=mem_sample')).length === 2,
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

  // ---- C5: the read buffer is reused, one per node and never one shared ----
  // A tick reads /proc/self/status and /proc/meminfo into the SAME two 16 KiB
  // buffers forever: no per-tick allocation. One buffer per node — not one shared
  // — is what the overlap check below pins down.
  const dirBuf = join(dir, 'buf');
  const bufChangesBefore: number[] = [procReadBufferChanges.get(PROC_STATUS) ?? 0,
    procReadBufferChanges.get(PROC_MEMINFO) ?? 0];
  const runBuf = await newRuntime('buf');
  runBuf.start(dirBuf, 'ui');
  await settle();
  const readsAfterFirst = procReadCalls;
  for (let i = 0; i < 3; i++) {
    timers[timers.length - 1].cb();
    await settle();
  }
  const bufLines = linesOf(join(dirBuf, 'netbird-ui.log'));
  check('buffer reuse: three more samples read both nodes twice each (6 reads)',
    procReadCalls === readsAfterFirst + 6 &&
    bufLines.filter((l: string): boolean => l.includes('event=mem_sample')).length === 4,
    `${procReadCalls - readsAfterFirst} read(s), ${bufLines.length} line(s)`);
  check('buffer reuse: each node is read into ONE and the same ArrayBuffer object',
    procReadBufferChanges.get(PROC_STATUS) === bufChangesBefore[0] + 1 &&
    procReadBufferChanges.get(PROC_MEMINFO) === bufChangesBefore[1] + 1,
    `status=${procReadBufferChanges.get(PROC_STATUS)} meminfo=${procReadBufferChanges.get(PROC_MEMINFO)}`);
  check('buffer reuse: the two nodes never share one buffer',
    procReadBuffers.get(PROC_STATUS) !== undefined &&
    procReadBuffers.get(PROC_MEMINFO) !== undefined &&
    procReadBuffers.get(PROC_STATUS) !== procReadBuffers.get(PROC_MEMINFO));
  const bufFields = fieldsOf(bufLines[bufLines.length - 1]);
  check('buffer reuse: the reused buffers still deliver every field of the last sample',
    bufFields.get('threads') === '42' && bufFields.get('vmsize_kb') === '4194304' &&
    bufFields.get('vmdata_kb') === '1234567' && bufFields.get('rss_shmem_kb') === '8192' &&
    bufFields.get('sys_swap_total_kb') === '2097152' &&
    bufFields.get('sys_swap_free_kb') === '1048576',
    bufLines[bufLines.length - 1]);
  runBuf.stop();
  await settle();

  // The overlap those per-node buffers exist for: a STALE /proc/meminfo read is
  // still in flight (stop() returned with it pending) when the NEXT run reads
  // /proc/self/status. Both reads then settle in the same tick, the fresh status
  // read writing FIRST; with one shared buffer the fresh decode would read the
  // stale meminfo bytes and report `na` for keys that were plainly present.
  const dirOv = join(dir, 'ov');
  const runOv = await newRuntime('ov');
  armProcReadGate(PROC_MEMINFO); // hold every meminfo read
  runOv.start(dirOv, 'ui'); // its status read completes, its meminfo read blocks
  await settle();
  const ovBlocked = linesOf(join(dirOv, 'netbird-ui.log'));
  armProcReadGate(PROC_STATUS); // hold status reads too
  runOv.stop(); // the stopped run's meminfo read is still in flight
  await settle();
  runOv.start(dirOv, 'ui'); // the new run's status read blocks on its gate
  await settle();
  releaseProcReadGate(PROC_STATUS); // the fresher status read writes first
  releaseProcReadGate(PROC_MEMINFO); // the stale meminfo read writes second
  await settle();
  const ovLines = linesOf(join(dirOv, 'netbird-ui.log'));
  const ovSample = fieldsOf(ovLines[ovLines.length - 1]);
  check('overlap: a stale read of the OTHER node cannot clobber the fresh status decode',
    ovBlocked.filter((l: string): boolean => l.includes('event=mem_sample')).length === 0 &&
    ovLines.filter((l: string): boolean => l.includes('event=mem_sample')).length === 1 &&
    ovSample.get('threads') === '42' && ovSample.get('vmsize_kb') === '4194304' &&
    ovSample.get('vmdata_kb') === '1234567' &&
    ovSample.get('sys_swap_total_kb') === '2097152' &&
    ovSample.get('sys_swap_free_kb') === '1048576',
    `${ovLines.filter((l: string): boolean => l.includes('event=mem_sample')).length} sample(s): ` +
    ovLines.slice(-3).join(' ~ '));
  runOv.stop();
  await settle();

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

  // ---- MD: the every-10th mem_detail + mem_vm (one tick, two extra records) --
  // UI role, NO injected alloc reader: the detail records must still appear on
  // the 10th sample — i.e. every 10 ticks, which for the 10 s ui cadence is
  // every 100 s (0.0.21), and NOT once per tick even though the tick slowed
  // down — mem_detail with `na` for the four rust keys, then mem_vm
  // with the real synchronous readings — and must be ABSENT on every other tick.
  const dirMD = join(dir, 'md');
  const runMD = await newRuntime('md');
  const mdNativeBefore = nativeMemCalls;
  runMD.start(dirMD, 'ui');
  await settle();
  const mdLog = (): string[] => linesOf(join(dirMD, 'netbird-ui.log'));
  const mdSamples = (): string[] => mdLog().filter((l: string): boolean =>
    l.includes('event=mem_sample'));
  const mdDetails = (): string[] => mdLog().filter((l: string): boolean =>
    l.includes('event=mem_detail'));
  const mdVms = (): string[] => mdLog().filter((l: string): boolean =>
    l.includes('event=mem_vm'));
  check('detail: the first sample carries no mem_detail, no mem_vm and no detail key',
    mdSamples().length === 1 && mdDetails().length === 0 && mdVms().length === 0 &&
    fieldsOf(mdSamples()[0]).get('pss_kb') === undefined &&
    fieldsOf(mdSamples()[0]).get('rust_heap_kb') === undefined &&
    fieldsOf(mdSamples()[0]).get('gc_count') === undefined &&
    fieldsOf(mdSamples()[0]).get('native_heap_kb') === undefined &&
    nativeMemCalls === mdNativeBefore, mdLog().join(' ~ '));
  for (let i = 0; i < 8; i++) {
    timers[timers.length - 1].cb();
    await settle();
  }
  check('detail: samples 2..9 emit neither mem_detail nor mem_vm nor their keys',
    mdSamples().length === 9 && mdDetails().length === 0 && mdVms().length === 0 &&
    nativeMemCalls === mdNativeBefore &&
    mdSamples().every((l: string): boolean =>
      fieldsOf(l).get('pss_kb') === undefined && fieldsOf(l).get('rust_allocs') === undefined &&
      fieldsOf(l).get('gc_count') === undefined &&
      fieldsOf(l).get('native_heap_kb') === undefined),
    `${mdSamples().length} sample(s), ${mdDetails().length} detail(s)`);
  timers[timers.length - 1].cb(); // the 10th sample: the detail tick
  await settle();
  check('detail: the 10th sample adds ONE mem_detail and ONE mem_vm, in that order',
    mdSamples().length === 10 && mdDetails().length === 1 && mdVms().length === 1 &&
    nativeMemCalls === mdNativeBefore + 1 &&
    mdLog()[mdLog().length - 3].includes('event=mem_sample') &&
    mdLog()[mdLog().length - 2].includes('event=mem_detail') &&
    mdLog()[mdLog().length - 1].includes('event=mem_vm'),
    mdLog().slice(-4).join(' ~ '));
  const mdDetail = fieldsOf(mdDetails()[0]);
  check('detail: the smaps readings come from the async API as their own KB text',
    mdDetail.get('pss_kb') === '204800' && mdDetail.get('private_dirty_kb') === '120000' &&
    mdDetail.get('shared_dirty_kb') === '8000' &&
    mdDetail.get('private_clean_kb') === '64000' &&
    mdDetail.get('shared_clean_kb') === '4096', mdDetails()[0]);
  check('detail: a UI process with no alloc reader writes na for all four rust keys',
    mdDetail.get('rust_heap_kb') === 'na' && mdDetail.get('rust_heap_peak_kb') === 'na' &&
    mdDetail.get('rust_allocs') === 'na' && mdDetail.get('rust_frees') === 'na',
    mdDetails()[0]);
  check('detail: the per-tick record keeps the three added /proc keys',
    fieldsOf(mdSamples()[9]).get('vmdata_kb') === '1234567' &&
    fieldsOf(mdSamples()[9]).get('vmstk_kb') === '4096' &&
    fieldsOf(mdSamples()[9]).get('rss_shmem_kb') === '8192' &&
    fieldsOf(mdSamples()[0]).get('vmdata_kb') === '1234567', mdSamples()[9]);
  const mdVm = fieldsOf(mdVms()[0]);
  check('mem_vm: the ArkTS VM heap shape comes from getAppVMMemoryInfo (KB, as answered)',
    mdVm.get('vm_total_heap_kb') === '16384' && mdVm.get('vm_array_kb') === '4096',
    mdVms()[0]);
  check('mem_vm: the mallinfo byte counts are floored down to KiB (1023 B never rounds up)',
    mdVm.get('native_heap_kb') === '20480' && mdVm.get('native_heap_alloc_kb') === '12288' &&
    mdVm.get('native_heap_free_kb') === '8191', mdVms()[0]);
  check('mem_vm: the GC counters are counts/ms and the two byte counters are floored KiB',
    mdVm.get('gc_count') === '77' && mdVm.get('gc_time_ms') === '512' &&
    mdVm.get('gc_alloc_kb') === '102400' && mdVm.get('gc_freed_kb') === '51199', mdVms()[0]);
  check('mem_vm: the app memory limit rides every mem_vm record, in KiB as answered',
    mdVm.get('rss_limit_kb') === '262144' && mdVm.get('vss_limit_kb') === '4194304' &&
    mdVm.get('vm_heap_limit_kb') === '524288' &&
    fieldsOf(mdLog()[0]).get('rss_limit_kb') === '262144', mdVms()[0]);
  check('mem_vm: the whole mem_vm record stays inside the 512-char bound',
    mdVms()[0].length < m.MAX_LINE_CHARS && mdVms()[0].indexOf('~') < 0 &&
    /^ts=\d{4}-\d{2}-\d{2}T[\d:.]+Z pid=\d+ role=ui event=mem_vm/.test(mdVms()[0]),
    mdVms()[0]);
  check('detail: the detail keys never ride the mem_sample line (separate record)',
    mdSamples().every((l: string): boolean => fieldsOf(l).get('pss_kb') === undefined &&
      fieldsOf(l).get('rust_heap_kb') === undefined &&
      fieldsOf(l).get('gc_count') === undefined &&
      fieldsOf(l).get('native_heap_kb') === undefined),
    mdSamples()[9]);
  check('detail: the three records of a detail tick are disjoint (no key on the wrong line)',
    mdDetail.get('gc_count') === undefined && mdDetail.get('native_heap_kb') === undefined &&
    mdDetail.get('vm_array_kb') === undefined && mdDetail.get('rss_limit_kb') === undefined &&
    mdVm.get('pss_kb') === undefined &&
    mdVm.get('rust_heap_kb') === undefined, mdVms()[0]);
  check('detail: every emitted line stays inside the 512-char bound, unmarked',
    mdLog().every((l: string): boolean => l.length < m.MAX_LINE_CHARS && l.indexOf('~') < 0),
    `max=${Math.max(...mdLog().map((l: string): number => l.length))}`);
  check('detail: the mem_sample and its mem_detail share the record envelope',
    /^ts=\d{4}-\d{2}-\d{2}T[\d:.]+Z pid=\d+ role=ui event=mem_sample/.test(mdSamples()[9]) &&
    /^ts=\d{4}-\d{2}-\d{2}T[\d:.]+Z pid=\d+ role=ui event=mem_detail/.test(mdDetails()[0]),
    mdDetails()[0]);
  runMD.stop();
  await settle();

  // An injected reader (the VPN extension passes nativeAllocStats) must reach
  // the same mem_detail record through the pure core's parser. Driven through a
  // ui run because the vpn role writes only to the caller's sink.
  const dirMD3 = join(dir, 'md3');
  const runMD3 = await newRuntime('md3');
  runMD3.start(dirMD3, 'ui', undefined, (): string | undefined =>
    '{"bytes":1048576,"peak":2097152,"allocs":77,"frees":12}');
  await settle();
  for (let i = 0; i < 9; i++) {
    timers[timers.length - 1].cb();
    await settle();
  }
  const md3Details = linesOf(join(dirMD3, 'netbird-ui.log')).filter((l: string): boolean =>
    l.includes('event=mem_detail'));
  check('detail: an injected reader is parsed into KiB + counters by the pure core',
    md3Details.length === 1 && fieldsOf(md3Details[0]).get('rust_heap_kb') === '1024' &&
    fieldsOf(md3Details[0]).get('rust_heap_peak_kb') === '2048' &&
    fieldsOf(md3Details[0]).get('rust_allocs') === '77' &&
    fieldsOf(md3Details[0]).get('rust_frees') === '12', md3Details.join(' ~ '));
  runMD3.stop();
  await settle();

  // ---- MD-HANG: a pending PSS promise keeps single-flight, never misfires --
  const dirMDH = join(dir, 'mdh');
  const runMDH = await newRuntime('mdh');
  runMDH.start(dirMDH, 'ui');
  await settle();
  for (let i = 0; i < 8; i++) {
    timers[timers.length - 1].cb();
    await settle();
  }
  let releaseNative: (() => void) | null = null;
  hangNativeMem = new Promise<void>((resolve: () => void): void => { releaseNative = resolve; });
  const nativeBeforeHang = nativeMemCalls;
  timers[timers.length - 1].cb(); // the 10th sample: its PSS read hangs
  await settle();
  const mdhSamplesHung = linesOf(join(dirMDH, 'netbird-ui.log')).filter((l: string): boolean =>
    l.includes('event=mem_sample')).length;
  check('detail hang: the detail tick issues exactly ONE async smaps read',
    nativeMemCalls === nativeBeforeHang + 1, `calls=${nativeMemCalls - nativeBeforeHang}`);
  check('detail hang: no sample, no detail and no mem_vm is emitted while the PSS read is pending',
    mdhSamplesHung === 9 &&
    linesOf(join(dirMDH, 'netbird-ui.log')).filter((l: string): boolean =>
      l.includes('event=mem_detail') || l.includes('event=mem_vm')).length === 0,
    `${mdhSamplesHung} sample(s)`);
  timers[timers.length - 1].cb();
  timers[timers.length - 1].cb();
  await settle();
  check('detail hang: ticks during the pending PSS read start no second read (single-flight)',
    nativeMemCalls === nativeBeforeHang + 1, `calls=${nativeMemCalls - nativeBeforeHang}`);
  const release: (() => void) | null = releaseNative;
  if (release !== null) { release(); }
  await settle();
  const mdhFinal = linesOf(join(dirMDH, 'netbird-ui.log'));
  check('detail hang: releasing the read lands ONE sample + its ONE detail + ONE mem_vm (tick 10)',
    mdhFinal.filter((l: string): boolean => l.includes('event=mem_sample')).length === 10 &&
    mdhFinal.filter((l: string): boolean => l.includes('event=mem_detail')).length === 1 &&
    mdhFinal.filter((l: string): boolean => l.includes('event=mem_vm')).length === 1 &&
    mdhFinal[mdhFinal.length - 1].includes('event=mem_vm') &&
    mdhFinal[mdhFinal.length - 2].includes('event=mem_detail') &&
    mdhFinal[mdhFinal.length - 3].includes('event=mem_sample'),
    mdhFinal.slice(-4).join(' ~ '));
  check('detail hang: the tick that landed is the 10th (its detail and mem_vm follow it)',
    fieldsOf(mdhFinal[mdhFinal.length - 2]).get('pss_kb') === '204800' &&
    fieldsOf(mdhFinal[mdhFinal.length - 2]).get('rust_heap_kb') === 'na' &&
    fieldsOf(mdhFinal[mdhFinal.length - 1]).get('gc_count') === '77');
  hangNativeMem = null;
  runMDH.stop();
  await settle();

  // ---- MD-FAIL: an unsupported async smaps API -> na, said once per run ----
  const dirMDF = join(dir, 'mdf');
  const mdWarnBefore = hilogLines.filter((l: string): boolean =>
    l.includes('DIAG_DETAIL_UNAVAILABLE')).length;
  nativeMemThrows = true;
  const runMDF = await newRuntime('mdf');
  runMDF.start(dirMDF, 'ui');
  await settle();
  for (let i = 0; i < 9; i++) {
    timers[timers.length - 1].cb();
    await settle();
  }
  const mdfDetails = linesOf(join(dirMDF, 'netbird-ui.log')).filter((l: string): boolean =>
    l.includes('event=mem_detail'));
  check('detail failure: an unsupported async smaps API renders every smaps key as na',
    mdfDetails.length === 1 && fieldsOf(mdfDetails[0]).get('pss_kb') === 'na' &&
    fieldsOf(mdfDetails[0]).get('private_dirty_kb') === 'na' &&
    fieldsOf(mdfDetails[0]).get('shared_clean_kb') === 'na' &&
    fieldsOf(mdfDetails[0]).get('rust_heap_kb') === 'na', mdfDetails.join(' ~ '));
  const mdfVms = linesOf(join(dirMDF, 'netbird-ui.log')).filter((l: string): boolean =>
    l.includes('event=mem_vm'));
  check('detail failure: the PSS failure does not take the mem_vm record down with it',
    mdfVms.length === 1 && fieldsOf(mdfVms[0]).get('native_heap_kb') === '20480' &&
    fieldsOf(mdfVms[0]).get('gc_count') === '77' &&
    fieldsOf(mdfVms[0]).get('vm_total_heap_kb') === '16384', mdfVms.join(' ~ '));
  check('detail failure: the failure is said ONCE per run, not once per detail tick',
    hilogLines.filter((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE')).length === mdWarnBefore + 1 &&
    hilogLines.filter((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE')).every((l: string): boolean =>
      l.indexOf('/proc/') < 0 && l.includes('status=na')),
    String(hilogLines.filter((l: string): boolean => l.includes('DIAG_DETAIL_UNAVAILABLE')).length));
  runMDF.stop();
  await settle();
  nativeMemThrows = false;
  // a throwing/undefined alloc reader is reported the same way, once
  const dirMDR = join(dir, 'mdr');
  const runMDR = await newRuntime('mdr');
  runMDR.start(dirMDR, 'ui', undefined, (): string | undefined => { throw new Error('boom'); });
  await settle();
  for (let i = 0; i < 9; i++) {
    timers[timers.length - 1].cb();
    await settle();
  }
  const mdrDetails = linesOf(join(dirMDR, 'netbird-ui.log')).filter((l: string): boolean =>
    l.includes('event=mem_detail'));
  check('detail failure: a throwing reader is contained and lands as na, never into the run',
    mdrDetails.length === 1 && fieldsOf(mdrDetails[0]).get('rust_heap_kb') === 'na' &&
    hilogLines.some((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE') && l.includes('rust-alloc')),
    mdrDetails.join(' ~ '));
  runMDR.stop();
  await settle();
  check('detail failure: the run still closes clean after both detail failures',
    /state=clean/.test(fileText(join(dirMDR, 'netbird-diag-ui.marker'))));

  // ---- MV-FAIL: the synchronous mem_vm legs fail on their own -------------
  // The mallinfo and GC APIs throw while the VM-memory API still answers: only
  // the failing legs' keys may become `na`, and both failures are still said
  // once per run (one line, its stage label), never once per tick.
  const dirMVF = join(dir, 'mvf');
  const mvWarnBefore = hilogLines.filter((l: string): boolean =>
    l.includes('DIAG_DETAIL_UNAVAILABLE')).length;
  nativeHeapThrows = true;
  gcStatThrows = true;
  const runMVF = await newRuntime('mvf');
  runMVF.start(dirMVF, 'ui');
  await settle();
  for (let i = 0; i < 9; i++) {
    timers[timers.length - 1].cb();
    await settle();
  }
  const mvfVms = linesOf(join(dirMVF, 'netbird-ui.log')).filter((l: string): boolean =>
    l.includes('event=mem_vm'));
  check('mem_vm failure: throwing mallinfo/GC APIs are na while the VM keys stay real',
    mvfVms.length === 1 && fieldsOf(mvfVms[0]).get('native_heap_kb') === 'na' &&
    fieldsOf(mvfVms[0]).get('native_heap_alloc_kb') === 'na' &&
    fieldsOf(mvfVms[0]).get('native_heap_free_kb') === 'na' &&
    fieldsOf(mvfVms[0]).get('gc_count') === 'na' &&
    fieldsOf(mvfVms[0]).get('gc_time_ms') === 'na' &&
    fieldsOf(mvfVms[0]).get('gc_alloc_kb') === 'na' &&
    fieldsOf(mvfVms[0]).get('gc_freed_kb') === 'na' &&
    fieldsOf(mvfVms[0]).get('vm_total_heap_kb') === '16384' &&
    fieldsOf(mvfVms[0]).get('vm_array_kb') === '4096', mvfVms.join(' ~ '));
  check('mem_vm failure: two failing legs are still said ONCE per run, with a stage label',
    hilogLines.filter((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE')).length === mvWarnBefore + 1 &&
    hilogLines.some((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE') && l.includes('native-heap')),
    String(hilogLines.filter((l: string): boolean => l.includes('DIAG_DETAIL_UNAVAILABLE')).length));
  runMVF.stop();
  await settle();
  nativeHeapThrows = false;
  gcStatThrows = false;

  // A throwing VM-memory API is its own leg with its own stage label.
  const dirMVV = join(dir, 'mvv');
  const mvvWarnBefore = hilogLines.filter((l: string): boolean =>
    l.includes('DIAG_DETAIL_UNAVAILABLE')).length;
  memThrow = true;
  const runMVV = await newRuntime('mvv');
  runMVV.start(dirMVV, 'ui');
  await settle();
  for (let i = 0; i < 9; i++) {
    timers[timers.length - 1].cb();
    await settle();
  }
  const mvvVms = linesOf(join(dirMVV, 'netbird-ui.log')).filter((l: string): boolean =>
    l.includes('event=mem_vm'));
  check('mem_vm failure: an unsupported getAppVMMemoryInfo is na for both VM keys',
    mvvVms.length === 1 && fieldsOf(mvvVms[0]).get('vm_total_heap_kb') === 'na' &&
    fieldsOf(mvvVms[0]).get('vm_array_kb') === 'na' &&
    fieldsOf(mvvVms[0]).get('gc_count') === '77' &&
    fieldsOf(mvvVms[0]).get('native_heap_kb') === '20480', mvvVms.join(' ~ '));
  check('mem_vm failure: the VM-leg failure is said ONCE, with its own stage label',
    hilogLines.filter((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE')).length === mvvWarnBefore + 1 &&
    hilogLines.some((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE') && l.includes('vm-heap')),
    String(hilogLines.filter((l: string): boolean => l.includes('DIAG_DETAIL_UNAVAILABLE')).length));
  runMVV.stop();
  await settle();
  memThrow = false;
  check('mem_vm failure: the run still closes clean after the mem_vm failures',
    /state=clean/.test(fileText(join(dirMVV, 'netbird-diag-ui.marker'))));

  // ---- ML: an absent getAppMemoryLimit is `na` in BOTH places, no crash -----
  // API 12 may be missing on an older build. The start-time read must render
  // `na` on diag_start WITHOUT consuming the run's once-per-run detail report
  // (it runs before any detail tick), and the detail-tick leg then reports the
  // failure once with its own stage label — while every other mem_vm leg keeps
  // answering.
  const dirML = join(dir, 'ml');
  const mlWarnBefore = hilogLines.filter((l: string): boolean =>
    l.includes('DIAG_DETAIL_UNAVAILABLE')).length;
  memLimitThrows = true;
  const runML = await newRuntime('ml');
  runML.start(dirML, 'ui');
  await settle();
  const mlLog = (): string[] => linesOf(join(dirML, 'netbird-ui.log'));
  check('mem-limit absence: diag_start renders the three limit keys as na, never 0',
    fieldsOf(mlLog()[0]).get('rss_limit_kb') === 'na' &&
    fieldsOf(mlLog()[0]).get('vss_limit_kb') === 'na' &&
    fieldsOf(mlLog()[0]).get('vm_heap_limit_kb') === 'na', mlLog()[0]);
  check('mem-limit absence: the start-time read is silent (no failure report yet)',
    hilogLines.filter((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE')).length === mlWarnBefore,
    String(hilogLines.filter((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE')).length));
  for (let i = 0; i < 9; i++) {
    timers[timers.length - 1].cb();
    await settle();
  }
  const mlVms = mlLog().filter((l: string): boolean => l.includes('event=mem_vm'));
  check('mem-limit absence: the mem_vm record is still emitted, limits na, other legs live',
    mlVms.length === 1 && fieldsOf(mlVms[0]).get('rss_limit_kb') === 'na' &&
    fieldsOf(mlVms[0]).get('vss_limit_kb') === 'na' &&
    fieldsOf(mlVms[0]).get('vm_heap_limit_kb') === 'na' &&
    fieldsOf(mlVms[0]).get('gc_count') === '77' &&
    fieldsOf(mlVms[0]).get('native_heap_kb') === '20480' &&
    fieldsOf(mlVms[0]).get('vm_total_heap_kb') === '16384', mlVms.join(' ~ '));
  check('mem-limit absence: it is said ONCE, with its own stage label',
    hilogLines.filter((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE')).length === mlWarnBefore + 1 &&
    hilogLines.some((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE') && l.includes('mem-limit')),
    String(hilogLines.filter((l: string): boolean =>
      l.includes('DIAG_DETAIL_UNAVAILABLE')).length));
  runML.stop();
  await settle();
  memLimitThrows = false;
  check('mem-limit absence: the run still closes clean',
    /state=clean/.test(fileText(join(dirML, 'netbird-diag-ui.marker'))));

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
  check('ui wiring: the bounded memory-level enum is recorded and nothing is released',
    entryText.includes('onMemoryLevel(level: AbilityConstant.MemoryLevel): void') &&
    entryText.includes("diagnosticsEvent('ui_memory_level', fields)") &&
    entryText.includes("fields['state'] = String(level);") &&
    !/onMemoryLevel[\s\S]{0,400}?(free|release|trim|evict|clear|unload)\(/.test(
      entryText.slice(entryText.indexOf('onMemoryLevel(level: AbilityConstant.MemoryLevel)'),
        entryText.indexOf('onConfigurationUpdate(newConfig: Configuration)'))));

  // ---- W2: the shipped VPN extension's onDestroy-entry stop marker ---------
  // 2026-10-09 (0.0.17) PRIOR_EXIT_UNCLEAN over-reporting fix. The device fact
  // behind the ordering: the JS context is destroyed ~3 ms after onDestroy
  // returns and the process is gone within ~21 ms, so NOTHING after the entry's
  // synchronous statements is guaranteed to run — the async destroyOnce
  // continuation and the late stopDiagnostics() (the only writer of `clean`)
  // never executed (V016: netbird-vpn.log:1528-1547 ends at
  // `vpn_destroy_phase phase=requested`). The marker therefore has to be moved
  // off `open` synchronously, as the FIRST thing the entry does, before the
  // native connector stop, the destroy leg and the late diagnostics stop.
  const extText = readFileSync(join(here, '..', 'src', 'main', 'ets',
    'vpnextensionability', 'NetBirdVpnExtensionAbility.ets'), 'utf8');
  const extOnDestroyAt = extText.indexOf('onDestroy(): void {');
  const extOnDestroyEndAt = extText.indexOf('\n  }\n', extOnDestroyAt);
  const extStopMarkerAt = extText.indexOf('this.writeStopMarkerSync();');
  const extStopMarkerDefAt = extText.indexOf('private writeStopMarkerSync(): void {');
  const extConnectorStopAt = extText.indexOf('connectorStop()', extOnDestroyAt);
  const extDestroyOnceAt = extText.indexOf("this.destroyOnce('onDestroy')", extOnDestroyAt);
  const extDiagStopAt = extText.indexOf('stopDiagnostics();', extOnDestroyAt);
  check('vpn wiring: onDestroy writes the stop marker FIRST, inside the entry',
    extOnDestroyAt > 0 && extStopMarkerAt > extOnDestroyAt &&
    extStopMarkerAt < extOnDestroyEndAt &&
    extStopMarkerAt < extConnectorStopAt && extStopMarkerAt < extDestroyOnceAt &&
    extStopMarkerAt < extDiagStopAt && extStopMarkerDefAt > 0 &&
    extStopMarkerDefAt < extOnDestroyAt,
    JSON.stringify({ extOnDestroyAt, extStopMarkerAt, extOnDestroyEndAt, extStopMarkerDefAt,
      extConnectorStopAt, extDestroyOnceAt, extDiagStopAt }));
  check('vpn wiring: the marker write is a synchronous, guarded, one-shot fail-soft helper',
    extText.indexOf('fs.openSync(path,', extStopMarkerDefAt) > 0 &&
    extText.indexOf('fs.writeSync(file.fd, buildMarker(', extStopMarkerDefAt) > 0 &&
    extText.indexOf('if (!this.logTeeConfigured) {', extStopMarkerDefAt) > 0 &&
    extText.indexOf('if (this.stopMarkerWritten) {', extStopMarkerDefAt) > 0 &&
    extText.indexOf('catch (error) {', extStopMarkerDefAt) > 0 &&
    extText.indexOf('VPN_STOP_MARKER_WRITE_FAILED', extStopMarkerDefAt) > 0 &&
    extText.includes('private stopMarkerWritten: boolean = false;') &&
    extText.includes('private stopMarkerWarned: boolean = false;'));
  check('vpn wiring: the state token and the path/format rules come from the pure core',
    extText.includes("from './NetBirdDiagnosticsCore'") &&
    extText.indexOf('MARKER_STATE_STOPPING', extStopMarkerDefAt) > 0 &&
    extText.indexOf('recordPath(this.context.filesDir, markerBasename(', extStopMarkerDefAt) > 0 &&
    !/(state=stopping|'stopping')/.test(extText));
  // The synchronous write is allowed in the extension ONLY: the diagnostics
  // RUNTIME module must stay free of synchronous file APIs (pinned at the top
  // of this suite), so the helper above may never move into it.
  check('the extension owns the only synchronous marker write (runtime stays async)',
    extText.indexOf('openSync') > 0 &&
    !/(openSync|writeSync|readTextSync|accessSync|unlinkSync|renameSync|mkdirSync|listFileSync)/
      .test(runtimeCode));

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
