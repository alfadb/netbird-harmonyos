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
  const gate = {
    networkDirty: true, networkRecoveryReady: false, networkRecoveryActive: false,
    isDns: false, destroyRequested: false, destroyInFlight: false,
    recreateInFlight: false, hasConnection: true, connectionUnchanged: true,
    protectDeviation: false, productionGateBlocked: false
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
  // N2 first batch C/D columns (2026-09-30): the formal-switch production gate.
  // It refuses a production socket for EVERY kind of supply — including the
  // protected-DNS one and including while the recovery is active, because the
  // pre control the gate waits for is sent by the shell's own opener.
  check('a closed formal-switch gate refuses a management supply',
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: true }) ===
      'production-gate');
  check('a closed formal-switch gate refuses the protected-DNS supply too',
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: true,
      isDns: true, networkRecoveryReady: true, networkRecoveryActive: true }) ===
      'production-gate');
  check('the gate wins over the network-dirty rule (it is checked first)',
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: true,
      networkRecoveryReady: true }) === 'production-gate');
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
      networkRecoveryReady: true }) === 'network-dirty' &&
    policy.socketSupplyRejectionReason({ ...gate, productionGateBlocked: false,
      networkRecoveryReady: true, networkRecoveryActive: true, isDns: true }) === '');
  const firstConnection = {};
  const nextConnection = {};
  const supplyFor = (current: object | undefined, selected: object,
    recreateInFlight: boolean = false, destroyRequested: boolean = false): string =>
    policy.socketSupplyRejectionReason({ ...gate, networkDirty: false,
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
    ability.includes('this.fulfillSocketRequest(request)') &&
    ability.includes('this.startIceSocketTopUp();') &&
    ability.includes('this.topUpIceBatch()'));
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
