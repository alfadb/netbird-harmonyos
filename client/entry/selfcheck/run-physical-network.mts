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
  const policySource = readFileSync(source, 'utf8');
  writeFileSync(fixture, policySource);
  const policy = await import(pathToFileURL(fixture).href);
  check('registration callback for startup netId is not a switch',
    !policy.isPhysicalNetworkSwitch(101, 101));
  check('capability update for same netId is not a switch',
    !policy.isPhysicalNetworkSwitch(101, 101));
  check('different physical netId is a switch', policy.isPhysicalNetworkSwitch(101, 202));
  check('no network to physical network is a switch', policy.isPhysicalNetworkSwitch(0, 101));
  check('physical network lost is a switch', policy.isPhysicalNetworkSwitch(101, 0));
  const ability = readFileSync(join(here, '..', 'src', 'main', 'ets', 'vpnextensionability',
    'NetBirdVpnExtensionAbility.ets'), 'utf8');
  const notifyCalls = [...ability.matchAll(/connectorPhysicalNetworkChanged\(/g)].length;
  check('physical recovery has exactly one core network-change call site', notifyCalls === 1 &&
    ability.includes('connectorManagementAddressUpdate(addr)'));
  let netId = 101;
  let notifications = 0;
  for (const next of [101, 101, 202, 202, 0, 0, 303]) {
    if (policy.isPhysicalNetworkSwitch(netId, next)) { notifications++; }
    netId = next;
  }
  check('one notification per distinct physical netId transition', notifications === 3);
  // F1 (v0.0.27 outage REPORT.md §5): the supply gate is INSTANTANEOUS. Its only
  // network input is `networkUnusableNow` — "the platform reports no usable
  // physical default network AT THIS REQUEST" — and the remembered switch latch
  // (`networkDirty`) is not an input at all any more. The outage shape is the
  // pair (usable now, recovery-ready, latch previously set): it must be SERVED.
  const gate = {
    networkUnusableNow: true, networkRecoveryReady: false,
    destroyRequested: false, destroyInFlight: false,
    recreateInFlight: false, hasConnection: true, connectionUnchanged: true,
    protectDeviation: false, productionGateBlocked: false
  };
  check('startup physical switch permits protected management resupply',
    policy.socketSupplyRejectionReason(gate) === '');
  check('startup physical switch permits protected DNS resupply (no isDns carve-out left)',
    policy.socketSupplyRejectionReason(gate) ===
      policy.socketSupplyRejectionReason({ ...gate, isDns: true }));
  // The 5 h 29 m latch (REPORT.md §0.1/§2.4): the network is USABLE now, the
  // create leg has settled, and a recovery is NOT in flight — a switch happened
  // earlier and its recovery aborted. Pre-F1 this answered `network-dirty` for
  // management AND dns; there is no input left that can express "a switch
  // happened once" (the policy has no networkDirty field).
  const outageGate = { ...gate, networkUnusableNow: false, networkRecoveryReady: true };
  check('a usable physical network is never refused: management (the latch is gone)',
    policy.socketSupplyRejectionReason(outageGate) === '');
  check('a usable physical network serves the protected-DNS socket too',
    policy.socketSupplyRejectionReason(outageGate) ===
      policy.socketSupplyRejectionReason({ ...outageGate, isDns: true }));
  check('the outage shape is exactly the old latch: no field can say "a switch happened"',
    !policySource.includes('networkDirty: boolean') &&
    !policySource.includes('isDns: boolean') &&
    !policySource.includes('networkRecoveryActive: boolean') &&
    policySource.indexOf('gate.networkDirty') < 0 &&
    policySource.indexOf('gate.isDns') < 0 &&
    policySource.indexOf('gate.networkRecoveryActive') < 0);
  check('no usable physical network at THIS request refuses management',
    policy.socketSupplyRejectionReason({ ...outageGate, networkUnusableNow: true }) ===
      'network-unusable');
  check('no usable physical network at THIS request refuses the protected-DNS socket as well',
    policy.socketSupplyRejectionReason({ ...outageGate, networkUnusableNow: true }) ===
      'network-unusable');
  check('before the first create an unusable network does not gate (login fails on its own terms)',
    policy.socketSupplyRejectionReason({ ...outageGate, networkUnusableNow: true,
      networkRecoveryReady: false }) === '');
  check('recreate rejects even before create',
    policy.socketSupplyRejectionReason({ ...gate, recreateInFlight: true }) === 'recreate');
  check('missing connection never supplies a socket',
    policy.socketSupplyRejectionReason({ ...gate, hasConnection: false }) === 'no-connection');
  check('teardown never supplies a socket',
    policy.socketSupplyRejectionReason({ ...gate, destroyRequested: true }) === 'destroy');
  check('protect deviation never supplies a socket',
    policy.socketSupplyRejectionReason({ ...gate, protectDeviation: true }) === 'deviation');
  // N2 first batch C/D columns (2026-09-30): the formal-switch production gate.
  // It refuses a production socket for EVERY kind of supply — including the
  // protected-DNS one and including while the recovery is active, because the
  // pre control the gate waits for is sent by the shell's own opener.
  check('a closed formal-switch gate refuses a management supply',
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: true }) ===
      'production-gate');
  check('a closed formal-switch gate refuses the protected-DNS supply too',
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: true,
      networkRecoveryReady: true }) === 'production-gate');
  check('the gate wins over the network rule (it is checked first)',
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: true,
      networkRecoveryReady: true, networkUnusableNow: true }) === 'production-gate');
  check('the gate still loses to the lifecycle reasons (destroy/recreate/connection)',
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: true,
      destroyRequested: true }) === 'destroy' &&
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: true,
      recreateInFlight: true }) === 'recreate' &&
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: true,
      hasConnection: false }) === 'no-connection' &&
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: true,
      protectDeviation: true }) === 'deviation');
  check('an OPEN gate changes nothing (the pre-existing rules still decide)',
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: false,
      networkRecoveryReady: true, networkUnusableNow: true }) === 'network-unusable' &&
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: false,
      networkRecoveryReady: true, networkUnusableNow: false }) === '');
  const firstConnection = {};
  const nextConnection = {};
  const supplyFor = (current: object | undefined, selected: object,
    recreateInFlight: boolean = false, destroyRequested: boolean = false): string =>
    policy.socketSupplyRejectionReason({ ...gate, networkUnusableNow: false,
      hasConnection: current !== undefined, connectionUnchanged: current === selected,
      recreateInFlight, destroyRequested });
  check('first connection accepts supply', supplyFor(firstConnection, firstConnection) === '');
  check('recreate in progress rejects supply',
    supplyFor(firstConnection, firstConnection, true) === 'recreate');
  check('recreate complete accepts supply on new connection',
    supplyFor(nextConnection, nextConnection) === '');
  check('destroy rejects supply', supplyFor(nextConnection, nextConnection, false, true) === 'destroy');
  check('replacement during async protect rejects delivery',
    supplyFor(nextConnection, firstConnection) === 'connection-changed');
  check('ICE top-up uses the same lifecycle gate',
    policy.protectedConnectionRejectionReason({ ...gate, hasConnection: true,
      connectionUnchanged: false }) === 'connection-changed');
  check('socket supply timer does not capture a connection',
    ability.includes('this.startSocketSupply();') &&
    ability.includes('this.fulfillSocketRequest(request)'));
  // STAGE 4 (official-alignment audit §2.7): the ICE socket POOL is gone with
  // its sizing module — the whole ICE plane rides the ONE shared mux, so no
  // periodic ICE top-up may exist in the extension any more.
  check('the retired ICE socket pool has no driver left in the extension',
    !ability.includes('this.startIceSocketTopUp();') &&
    !ability.includes('this.topUpIceBatch()') &&
    !ability.includes('iceSocketTopUpCount(') &&
    !ability.includes('protectAndFeedIceSocket('));
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
