// Host-side checks against the byte-identical production network policy.
// Run: node client/entry/selfcheck/run-physical-network.mts
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, '..', 'src', 'main', 'ets', 'vpnextensionability',
  'NetBirdPhysicalNetworkPolicy.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-physical-network-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
}
try {
  const fixture = join(dir, 'NetBirdPhysicalNetworkPolicy.ts');
  writeFileSync(fixture, readFileSync(source));
  const policy = await import(pathToFileURL(fixture).href);
  check('registration callback for startup netId is not a switch',
    !policy.isPhysicalNetworkSwitch(101, 101));
  check('capability update for same netId is not a switch',
    !policy.isPhysicalNetworkSwitch(101, 101));
  check('different physical netId is a switch', policy.isPhysicalNetworkSwitch(101, 202));
  check('no network to physical network is a switch', policy.isPhysicalNetworkSwitch(0, 101));
  check('physical network lost is a switch', policy.isPhysicalNetworkSwitch(101, 0));
  const gate = {
    networkDirty: true, networkRecoveryReady: false, networkRecoveryActive: false,
    isDns: false, destroyRequested: false, destroyInFlight: false,
    recreateInFlight: false, hasConnection: true, protectDeviation: false
  };
  check('startup physical switch permits protected management resupply',
    policy.socketSupplyRejectionReason(gate) === '');
  check('startup physical switch permits protected DNS resupply',
    policy.socketSupplyRejectionReason({ ...gate, isDns: true }) === '');
  check('post-create dirty management supply is held',
    policy.socketSupplyRejectionReason({ ...gate, networkRecoveryReady: true }) === 'network-dirty');
  check('active recovery permits protected DNS supply',
    policy.socketSupplyRejectionReason({ ...gate, networkRecoveryReady: true,
      networkRecoveryActive: true, isDns: true }) === '');
  check('recreate rejects even before create',
    policy.socketSupplyRejectionReason({ ...gate, recreateInFlight: true }) === 'recreate');
  check('missing connection never supplies a socket',
    policy.socketSupplyRejectionReason({ ...gate, hasConnection: false }) === 'no-connection');
  check('teardown never supplies a socket',
    policy.socketSupplyRejectionReason({ ...gate, destroyRequested: true }) === 'destroy');
  check('protect deviation never supplies a socket',
    policy.socketSupplyRejectionReason({ ...gate, protectDeviation: true }) === 'deviation');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
