// Host-side checks against the byte-identical production status policy.
// Run: node client/entry/selfcheck/run-status-freshness.mts
//
// 0.0.34 peer-status lag fix (device report `peer-status-lag/REPORT.md` §5
// F1/F3/F4): the same pure module now also owns the WRITER's publish debounce
// and the READER's change skip. Those two functions ARE the shell half of the
// fix, so they are exercised here byte-identically; the WIRING (the edge wake
// subscription, the retained heartbeat, the 1 s reader tick) is pinned against
// the real sources below.
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, '..', 'src', 'main', 'ets', 'vpnextensionability', 'NetBirdStatusPolicy.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-status-freshness-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
}
try {
  const fixture = join(dir, 'NetBirdStatusPolicy.ts');
  writeFileSync(fixture, readFileSync(source));
  const policy = await import(pathToFileURL(fixture).href);
  const now = 1800000000000;
  const connected = { updated_at_ms: now - 1000, request_id: 'ui-1', state: 'connected',
    peer_count: 1, route_count: 1, last_error_class: 'none', signal_registered: true,
    signal_reconnects: 0, wg_ready: true, wg_sessions: 1 };
  check('fresh connected stays connected', policy.currentPublishedStatus(connected, now)?.state === 'connected');
  check('boundary at two poll cycles stays connected',
    policy.currentPublishedStatus({ ...connected, updated_at_ms: now - 10000 }, now)?.state === 'connected');
  check('expired connected is not running',
    policy.currentPublishedStatus({ ...connected, updated_at_ms: now - 10001 }, now) === null);
  check('three ticks old (the pre-2026-10-05 boundary) is not running',
    policy.currentPublishedStatus({ ...connected, updated_at_ms: now - 15000 }, now) === null);
  check('stopped stays stopped', policy.currentPublishedStatus({ ...connected, state: 'stopped' }, now)?.state === 'stopped');
  check('missing file means not running', policy.currentPublishedStatus(null, now) === null);
  check('legacy snapshot without timestamp is not running',
    policy.currentPublishedStatus({ ...connected, updated_at_ms: undefined }, now) === null);
  check('future snapshot is not running',
    policy.currentPublishedStatus({ ...connected, updated_at_ms: now + 1 }, now) === null);

  // -------------------------------------------------------------------------
  // 0.0.34 F4 — the WRITER's debounce (the extension's heartbeat tick)
  // -------------------------------------------------------------------------
  const cadence = 5000;                       // NetBirdConnector.STATUS_POLL_MS
  const maxAge = policy.statusHeartbeatMaxAgeMs(cadence);
  check('heartbeat: the mandatory-write age keeps the freshness gate armed',
    maxAge === policy.STATUS_MAX_AGE_MS - cadence - policy.STATUS_PUBLISH_FRESHNESS_SLACK_MS &&
    maxAge + cadence <= policy.STATUS_MAX_AGE_MS &&
    maxAge > 0);
  check('heartbeat: a cadence longer than the gate clamps to "write every tick"',
    policy.statusHeartbeatMaxAgeMs(policy.STATUS_MAX_AGE_MS + 1) === 0);
  const pub = (changeKey: string, lastChangeKey: string, ageMs: number): boolean =>
    policy.shouldPublishStatus({ changeKey: changeKey, lastChangeKey: lastChangeKey,
      nowMs: now, lastPublishMs: now - ageMs, heartbeatMaxAgeMs: maxAge });
  // (a) wake -> publish: the core's revision moved (an edge the shell's wake
  //     just announced) => publish on the spot, however recent the last write.
  check('wake -> publish: a new core revision publishes immediately',
    pub('41|present|7', '40|present|7', 1));
  check('wake -> publish: a new vpn-tun verdict publishes immediately',
    pub('40|absent|7', '40|present|7', 1));
  check('wake -> publish: a new network-map serial publishes immediately',
    pub('40|present|8', '40|present|7', 1));
  // (b) nothing changed and the document is young => NO write (the redundant
  //     heartbeat right after an edge publication is exactly this case).
  check('no publish when nothing changed (young document)',
    pub('40|present|7', '40|present|7', 0) === false &&
    pub('40|present|7', '40|present|7', maxAge - 1) === false);
  // (c) nothing changed but the heartbeat is due => a write (the freshness gate
  //     the reader applies must never lapse).
  check('heartbeat still published when nothing changed',
    pub('40|present|7', '40|present|7', maxAge) &&
    pub('40|present|7', '40|present|7', cadence + 1));
  // (d) the first publication of a process/session is never debounced away.
  check('the first publication always happens',
    policy.shouldPublishStatus({ changeKey: '9|present|1', lastChangeKey: '',
      nowMs: now, lastPublishMs: 0, heartbeatMaxAgeMs: maxAge }) &&
    policy.shouldPublishStatus({ changeKey: '9|present|1', lastChangeKey: '9|present|1',
      nowMs: now, lastPublishMs: 0, heartbeatMaxAgeMs: maxAge }));
  check('a broken clock publishes nothing (never a write with an unreadable stamp)',
    policy.shouldPublishStatus({ changeKey: 'a', lastChangeKey: 'b',
      nowMs: NaN, lastPublishMs: 1, heartbeatMaxAgeMs: maxAge }) === false);
  // (e) the change key: revision + iface verdict + map serial, absent = stable.
  check('change key covers revision/iface/serial and folds "absent" to -1',
    policy.statusPublishChangeKey(7, 'present', 3) === '7|present|3' &&
    policy.statusPublishChangeKey(undefined, 'unknown', undefined) === '-1|unknown|-1' &&
    policy.statusPublishChangeKey(7, 'present', 3) ===
      policy.statusPublishChangeKey(7, 'present', 3));

  // -------------------------------------------------------------------------
  // 0.0.34 F3 — the READER's change skip (the page's 1 s tick)
  // -------------------------------------------------------------------------
  check('reader stamp: document identity = write stamp + edge revision',
    policy.statusDocumentStamp({ updated_at_ms: 111, status_revision: 5 }) === '111|5' &&
    policy.statusDocumentStamp({ updated_at_ms: 111 }) === '111|-1' &&
    policy.statusDocumentStamp(null) === '' &&
    policy.statusDocumentStamp({ updated_at_ms: undefined }) === '' &&
    policy.statusDocumentStamp({ updated_at_ms: 0 }) === '');
  check('reader skip: the SAME document (idle screen) is not re-applied',
    policy.shouldReapplyPublishedStatus('111|5', '111|5') === false);
  check('reader re-apply: a new write stamp is applied at once',
    policy.shouldReapplyPublishedStatus('111|5', '112|5'));
  check('reader re-apply: two writes in the same millisecond are still seen',
    policy.shouldReapplyPublishedStatus('111|5', '111|6'));
  check('reader skip: an unreadable document never re-applies',
    policy.shouldReapplyPublishedStatus('111|5', '') === false);
  check('reader re-apply: the first document of a session is always applied',
    policy.shouldReapplyPublishedStatus('', '111|5'));

  // -------------------------------------------------------------------------
  // the WIRING: pin the real sources (byte-identical production files)
  // -------------------------------------------------------------------------
  const ets = join(here, '..', 'src', 'main', 'ets');
  const ext = readFileSync(join(ets, 'vpnextensionability', 'NetBirdVpnExtensionAbility.ets'), 'utf8');
  const page = readFileSync(join(ets, 'pages', 'Index.ets'), 'utf8');
  const connector = readFileSync(
    join(ets, 'vpnextensionability', 'NetBirdConnector.ets'), 'utf8');
  check('wiring: the extension subscribes the core edge wake and publishes on it',
    ext.includes('connectorStatusSubscribe((): void => {') &&
    ext.includes('this.statusEdgeWake();') &&
    ext.includes('private statusEdgeWake(): void {') &&
    ext.includes('this.publishStatusForUi(status, this.lastIfacePresence);'));
  check('wiring: the wake is subscribed with the watcher and detached with it',
    ext.includes('this.subscribeStatusWake();') &&
    ext.includes('this.stopStatusWake();') &&
    ext.indexOf('this.subscribeStatusWake();') < ext.indexOf('this.watcherTimer = setInterval'));
  check('wiring: the 5 s heartbeat is retained (the reader gate rides it)',
    ext.includes('}, STATUS_POLL_MS);') &&
    ext.includes('connectorStatusSubscribe, connectorStatusUnsubscribe,'));
  check('wiring: the heartbeat write goes through the debounce policy, never around it',
    ext.includes('shouldPublishStatus({') &&
    ext.includes('heartbeatMaxAgeMs: statusHeartbeatMaxAgeMs(STATUS_POLL_MS)') &&
    ext.includes('if (!force && !shouldPublishStatus({') &&
    ext.includes("this.publishStatusForUi(status, 'unknown', true, force);"));
  check('wiring: the debounce bookkeeping is updated only by a real write',
    ext.indexOf('this.lastStatusChangeKey = changeKey;') >
      ext.indexOf('fs.writeSync(file.fd, JSON.stringify(doc));'));
  check('wiring: the reader ticks at 1 s and skips an unchanged document',
    page.includes('const STATUS_POLL_MS: number = 1000;') &&
    page.includes('shouldReapplyPublishedStatus') &&
    page.includes('statusDocumentStamp(live)') &&
    page.indexOf('shouldReapplyPublishedStatus(this.appliedStatusStamp, nextStamp)') <
      page.indexOf('this.published = live;'));
  check('wiring: the connector exports the three core bindings',
    connector.includes("connector_status_subscribe") &&
    connector.includes("connector_status_unsubscribe") &&
    connector.includes("connector_status_revision") &&
    connector.includes('export function connectorStatusRevision(): number'));
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
