// Host-side checks against the byte-identical production status policy.
// Run: node client/entry/selfcheck/run-status-freshness.mts
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
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
