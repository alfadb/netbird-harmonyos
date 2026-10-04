// 场景B roaming self-check (LAN-ROUTE-ROAMING-ANALYSIS.md §4 step 2) —
// host-side, no device.
//
// Covers the completion criterion's PURE logic: "network change -> exclusion
// comparison -> rebuild or not" plus the debounce arithmetic. The production
// policy module client/entry/src/main/ets/vpnextensionability/
// NetBirdPhysicalNetworkPolicy.ets and the derivation module
// NetBirdEndpointExclusion.ets are zero-import pure modules; like
// run-lan-exclusion.mts, this harness copies the REAL files byte-for-byte
// into an OS-temp fixture (.ts) and drives the shipped lanExclusionSignature /
// lanRoamDecision / lanRoamCheckDue through:
//   - the exclusion signature of every derived shape (home Wi-Fi / cellular /
//     nothing usable),
//   - the full decision matrix (change => recreate; unchanged/lifecycle/N2
//     gates => skip with a stable token, including the documented precedence),
//   - the stability window (boundary, mid-storm, flap-restart, clock skew),
// plus wiring pins on the shipped extension source (the switch handler arms
// the window, the watcher tick runs the check, both create() submit points
// record the live signature, the N8 path is reused, the failure reason is
// surfaced).
//
// Run: node client/entry/selfcheck/run-lan-roam.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL_DIR = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability');
const STABLE_MS = 2500; // mirrors the shipped LAN_ROAM_STABLE_MS

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-lan-roam-'));
writeFileSync(join(fixtureDir, 'NetBirdPhysicalNetworkPolicy.ts'),
  readFileSync(join(REAL_DIR, 'NetBirdPhysicalNetworkPolicy.ets')));
writeFileSync(join(fixtureDir, 'NetBirdEndpointExclusion.ts'),
  readFileSync(join(REAL_DIR, 'NetBirdEndpointExclusion.ets')));

let passed: number = 0;
let failedCount: number = 0;
function check(name: string, condition: boolean, detail?: string): void {
  if (condition) {
    passed++;
    console.log(`ok   ${name}`);
  } else {
    failedCount++;
    console.log(`FAIL ${name}${detail !== undefined ? ` — ${detail}` : ''}`);
  }
}

try {
  const policy = await import(pathToFileURL(
    join(fixtureDir, 'NetBirdPhysicalNetworkPolicy.ts')).href);
  const exclusion = await import(pathToFileURL(
    join(fixtureDir, 'NetBirdEndpointExclusion.ts')).href);

  // ---- the signature of every derived shape --------------------------------
  const wifiEntry = exclusion.deriveLanExclusion({ bearerTypes: [1],
    linkAddresses: [{ address: '192.168.50.199', prefixLength: 24 }] }, '10.99.0.1').entry;
  const cellularResult = exclusion.deriveLanExclusion({ bearerTypes: [0],
    linkAddresses: [{ address: '10.41.63.59', prefixLength: 29 }] }, '10.99.0.1');
  check('a derived home Wi-Fi exclusion signs as its network/prefix',
    exclusion.lanExclusionSignature(wifiEntry) === '192.168.50.0/24',
    exclusion.lanExclusionSignature(wifiEntry));
  check('a cellular derivation (no entry) signs as none',
    exclusion.lanExclusionSignature(cellularResult.entry) === 'none');
  check('an unusable probe signs as none too',
    exclusion.lanExclusionSignature(exclusion.deriveLanExclusion(
      exclusion.emptyLanExclusionProbe(), '10.99.0.1').entry) === 'none');
  check('null/undefined entries sign as none and never throw',
    exclusion.lanExclusionSignature(null) === 'none' &&
    exclusion.lanExclusionSignature(undefined) === 'none');
  check('the prefix rides the signature (a /29 cellular-style prefix)',
    exclusion.lanExclusionSignature({ destination: '10.41.63.56', prefixLength: 29 }) ===
    '10.41.63.56/29');

  // ---- the decision matrix --------------------------------------------------
  const gate = {
    evidenceMode: false, destroyRequested: false, recreateInFlight: false,
    hasSettledConnection: true, networkLost: false, productionGateBlocked: false,
    appliedSignature: '192.168.50.0/24', currentSignature: 'none'
  };
  const decide = (overrides: Record<string, unknown>): { action: string; reason: string } =>
    policy.lanRoamDecision({ ...gate, ...overrides });
  const skip = (overrides: Record<string, unknown>, reason: string): boolean => {
    const d = decide(overrides);
    return d.action === 'skip' && d.reason === reason;
  };

  check('home Wi-Fi -> cellular (exclusion dropped) => recreate',
    decide({}).action === 'recreate' &&
    decide({}).reason === 'local-lan-exclusion-changed');
  check('cellular -> home Wi-Fi (exclusion added) => recreate',
    decide({ appliedSignature: 'none', currentSignature: '192.168.50.0/24' })
      .action === 'recreate');
  check('home Wi-Fi -> another Wi-Fi subnet => recreate',
    decide({ currentSignature: '192.168.1.0/24' }).action === 'recreate');
  check('Wi-Fi -> same Wi-Fi (unchanged) => skip exclusion-unchanged',
    skip({ currentSignature: '192.168.50.0/24' }, 'exclusion-unchanged'));
  check('cellular -> cellular (none == none) => skip exclusion-unchanged',
    skip({ appliedSignature: 'none', currentSignature: 'none' }, 'exclusion-unchanged'));
  check('N2 evidence mode never autonomously rebuilds (original behavior)',
    skip({ evidenceMode: true }, 'n2-evidence-mode'));
  check('teardown latched => skip destroy-requested',
    skip({ destroyRequested: true }, 'destroy-requested'));
  check('a recreate already in flight => skip (that leg re-derives anyway)',
    skip({ recreateInFlight: true }, 'recreate-in-flight'));
  check('no settled connection => skip',
    skip({ hasSettledConnection: false }, 'no-settled-connection'));
  check('physical network gone at check time => skip network-lost',
    skip({ networkLost: true }, 'network-lost'));
  check('an armed N2 C/D production gate => skip (plan undisturbed)',
    skip({ productionGateBlocked: true }, 'production-gate-blocked'));
  check('precedence: destroy beats evidence mode',
    skip({ destroyRequested: true, evidenceMode: true }, 'destroy-requested'));
  check('precedence: evidence mode beats recreate-in-flight',
    skip({ evidenceMode: true, recreateInFlight: true }, 'n2-evidence-mode'));
  check('precedence: network-lost beats the production gate',
    skip({ networkLost: true, productionGateBlocked: true }, 'network-lost'));
  check('gates beat the comparison: an in-flight recreate skips even on change',
    skip({ recreateInFlight: true }, 'recreate-in-flight'));

  // ---- the stability window (debounce arithmetic) ---------------------------
  check('due exactly at the boundary (now - last == stable)',
    policy.lanRoamCheckDue(STABLE_MS, 0, STABLE_MS));
  check('not due one ms before the boundary',
    !policy.lanRoamCheckDue(STABLE_MS - 1, 0, STABLE_MS));
  check('a flap restarts the window: t=2000 re-arm is not due at t=4499',
    !policy.lanRoamCheckDue(4499, 2000, STABLE_MS));
  check('the restarted window is due at t=4500 (2500 after the LAST switch)',
    policy.lanRoamCheckDue(4500, 2000, STABLE_MS));
  check('clock skew (now before the switch) is never due',
    !policy.lanRoamCheckDue(-1, 0, STABLE_MS));
  // deterministic timeline: one storm of three switches, one decision
  let lastSwitchAtMs = 0;
  let decisions = 0;
  let recreates = 0;
  for (const [tMs, kind] of [[0, 'switch'], [1200, 'switch'], [2100, 'switch'],
    [2000, 'tick'], [3000, 'tick'], [4400, 'tick'], [4600, 'tick'],
    [9000, 'tick']] as Array<[number, string]>) {
    if (kind === 'switch') {
      lastSwitchAtMs = tMs; // every measured switch re-arms
      continue;
    }
    if (policy.lanRoamCheckDue(tMs, lastSwitchAtMs, STABLE_MS)) {
      decisions++;
      if (policy.lanRoamDecision({ ...gate }).action === 'recreate') { recreates++; }
      break; // the armed check is one-shot per switch
    }
  }
  check('timeline: mid-storm ticks stay silent, exactly ONE settled decision',
    decisions === 1);
  check('timeline: the one settled decision rebuilt (exclusion had changed)',
    recreates === 1);

  // ---- wiring pins on the shipped sources -----------------------------------
  const extSource: string =
    readFileSync(join(REAL_DIR, 'NetBirdVpnExtensionAbility.ets'), 'utf8');
  const policySource: string =
    readFileSync(join(REAL_DIR, 'NetBirdPhysicalNetworkPolicy.ets'), 'utf8');
  check('the shipped stability window is 2-3 s (LAN_ROAM_STABLE_MS = 2500)',
    extSource.includes('const LAN_ROAM_STABLE_MS: number = 2500'));
  check('every measured switch arms the window exactly once (switch handler)',
    extSource.split('this.lanRoamArmed = true').length === 2 &&
    extSource.includes('this.lanRoamLastSwitchAtMs = Date.now();'));
  check('the watcher tick runs the check exactly once (no standalone timer)',
    extSource.split('this.checkLanRoamRecreate();').length === 2 &&
    !extSource.includes('lanRoamTimer'));
  check('the tick places the check BEFORE the core-driven N8 recreate block',
    extSource.indexOf('this.checkLanRoamRecreate();') <
    extSource.indexOf('status.recreate.required === true'));
  check('both create() submit points record the live exclusion signature',
    extSource.split('this.appliedLanExclusion = lanExclusionSignature(').length === 3);
  check('the roam trigger reuses the existing N8 recreate path with the plan reason',
    extSource.includes('this.performControlledRecreate(decision.reason)') &&
    policySource.includes("'local-lan-exclusion-changed'"));
  check('the roam marker logs from/to/action/result (VPN_LAN_ROAM)',
    extSource.includes(
      'VPN_LAN_ROAM|requestId=%{public}s|from=%{public}s|to=%{public}s|') &&
    extSource.includes('action=%{public}s|result=%{public}s'));
  check('the evidence gate is wired to the N2 ticket state',
    extSource.includes('evidenceMode: this.n2Evidence !== undefined'));
  check('a failed rebuild surfaces its stage on the roam trail (fail-closed kept)',
    extSource.includes('this.lastRecreateOutcome = `fail:${stage}`') &&
    extSource.includes('action=teardown-vpn-fail-closed'));
  check('the settled-connection gate mirrors the recreate precondition',
    extSource.includes('this.connection !== undefined && this.createSettled && this.tunFd >= 0'));
} catch (error) {
  failedCount++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log('');
console.log(`${passed} passed, ${failedCount} failed`);
process.exit(failedCount === 0 ? 0 : 1);
