// Host-side checks against the byte-identical recreate lifecycle policy
// (roam-1005-0014): settle wait, post-create interface verification and the
// interface-loss watchdog decision — the exact decisions the extension's
// async plumbing consumes.
// Run: node client/entry/selfcheck/run-recreate-policy.mts
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, '..', 'src', 'main', 'ets', 'vpnextensionability', 'NetBirdRecreatePolicy.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-recreate-policy-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
}
try {
  const fixture = join(dir, 'NetBirdRecreatePolicy.ts');
  writeFileSync(fixture, readFileSync(source));
  const p = await import(pathToFileURL(fixture).href);

  // ---- settleWaitAction -----------------------------------------------------
  // floor: nothing proceeds before the settle floor, not even a gone registry
  check('settle: waits before the floor even when the iface is gone',
    p.settleWaitAction(0, 800, 3000, 'absent') === 'wait' &&
    p.settleWaitAction(799, 800, 3000, 'absent') === 'wait');
  // the floor is elapsed-time based, not poll-count based
  check('settle: proceeds at the floor when the old iface is gone',
    p.settleWaitAction(800, 800, 3000, 'absent') === 'proceed' &&
    p.settleWaitAction(2500, 800, 3000, 'absent') === 'proceed');
  // an unreadable registry does not extend the wait past the floor
  check('settle: unknown registry proceeds at the floor (lenient)',
    p.settleWaitAction(800, 800, 3000, 'unknown') === 'proceed');
  // the OLD interface still listed keeps waiting — until the cap fuse
  check('settle: still-present keeps waiting under the cap',
    p.settleWaitAction(800, 800, 3000, 'present') === 'wait' &&
    p.settleWaitAction(3000, 800, 3000, 'present') === 'wait' &&
    p.settleWaitAction(3799, 800, 3000, 'present') === 'wait');
  check('settle: the cap is a fuse — proceeds even still-present',
    p.settleWaitAction(3800, 800, 3000, 'present') === 'proceed');

  // ---- ifaceVerifyOutcome ---------------------------------------------------
  check('verify: present accepts',
    p.ifaceVerifyOutcome('present', 1, 2) === 'ok' &&
    p.ifaceVerifyOutcome('present', 2, 2) === 'ok');
  // an unreadable registry is a LENIENT pass: it must not tear down a tunnel
  // it cannot see (it only cannot save a broken one)
  check('verify: unknown registry accepts (lenient)',
    p.ifaceVerifyOutcome('unknown', 1, 2) === 'ok' &&
    p.ifaceVerifyOutcome('unknown', 2, 2) === 'ok');
  check('verify: absent on attempt 1 retries the full recreate',
    p.ifaceVerifyOutcome('absent', 1, 2) === 'retry');
  check('verify: absent on the LAST attempt fails closed',
    p.ifaceVerifyOutcome('absent', 2, 2) === 'fail' &&
    p.ifaceVerifyOutcome('absent', 3, 3) === 'fail');

  // ---- ifaceLostDecision ----------------------------------------------------
  check('lost: present or unknown is ignored',
    p.ifaceLostDecision('present', false, true) === 'ignore' &&
    p.ifaceLostDecision('unknown', false, true) === 'ignore');
  check('lost: absent without a live generation is ignored (rebuild in flight)',
    p.ifaceLostDecision('absent', false, false) === 'ignore');
  check('lost: first loss of a live generation rebuilds (budget available)',
    p.ifaceLostDecision('absent', true, true) === 'rebuild');
  check('lost: a spent budget fails closed',
    p.ifaceLostDecision('absent', false, true) === 'fail-closed');

  // ---- ifaceLostRetryAllowed (roam-1005-0124: per-EVENT budget + storm cap) -
  // an empty log always allows: every recreate event re-arms the rebuild
  check('budget: empty log allows the rebuild',
    p.ifaceLostRetryAllowed([], 100000, 60000, 3) === true);
  // a rebuild started inside the window counts against the cap
  check('budget: under the cap allows (2 of 3 spent)',
    p.ifaceLostRetryAllowed([95000, 98000], 100000, 60000, 3) === true);
  check('budget: at the cap denies (3 of 3 spent)',
    p.ifaceLostRetryAllowed([95000, 98000, 99999], 100000, 60000, 3) === false);
  // a rebuild started OUTSIDE the window no longer counts — the window slides
  check('budget: an aged-out rebuild frees budget again',
    p.ifaceLostRetryAllowed([10000, 20000, 30000], 100000, 60000, 3) === true);
  // boundary: exactly windowMs old is OUT of the window (strictly-greater
  // comparison) — one ms newer is IN
  check('budget: the window edge is exclusive',
    p.ifaceLostRetryAllowed([40000, 40000, 40000], 100000, 60000, 3) === true &&
    p.ifaceLostRetryAllowed([40001, 40001, 40001], 100000, 60000, 3) === false);
  // the SECOND network switch gets its retry: one earlier rebuild (from the
  // first switch) does not exhaust the per-event budget
  check('budget: one prior rebuild still allows the next event\'s retry',
    p.ifaceLostRetryAllowed([70000], 130000, 60000, 3) === true);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
