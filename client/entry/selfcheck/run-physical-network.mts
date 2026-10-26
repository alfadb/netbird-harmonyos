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
  check('startup physical switch permits protected DNS resupply (the F1 network rule has no purpose carve-out)',
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
    !policySource.includes('networkRecoveryActive: boolean') &&
    policySource.indexOf('gate.networkDirty') < 0 &&
    policySource.indexOf('gate.networkRecoveryActive') < 0);
  // R2 (design `analysis-20261026-dns-structural` §4.2): "the physical default
  // network EXISTS" was the gate's only network input, and existence is not
  // usability — under the platform's EPERM the netId stayed valid while every
  // protected send was refused (`假 protected` 1612 times). The decision moves to
  // a LIVENESS verdict: presence AND no positive unusability evidence (the
  // core's EPERM fast-fail state, or the shell's own last protect failing).
  // A FLOWING data plane outranks a stale refusal — that is the guard against
  // re-introducing the 5 h 29 m latch of REPORT.md §0.1/§2.4.
  const liveFacts = { physicalNetId: 101, corePathUnusable: false,
    lastProtectFailed: false, dataPlaneFlowing: false };
  check('R2-1 a physical net id with no failure evidence is LIVE (startup / idle)',
    policy.protectedPathLive(liveFacts) === true);
  check('R2-2 the core\u2019s EPERM fast-fail state is NOT live',
    policy.protectedPathLive({ ...liveFacts, corePathUnusable: true }) === false);
  check('R2-3 the shell\u2019s own failed protect is NOT live',
    policy.protectedPathLive({ ...liveFacts, lastProtectFailed: true }) === false);
  check('R2-4 a flowing data plane clears a stale failed protect (no re-latch of the outage)',
    policy.protectedPathLive({ ...liveFacts, lastProtectFailed: true,
      dataPlaneFlowing: true }) === true);
  check('R2-4b but a flowing tunnel does NOT overrule the core\u2019s fresh EPERM verdict',
    policy.protectedPathLive({ ...liveFacts, corePathUnusable: true,
      lastProtectFailed: true, dataPlaneFlowing: true }) === false);
  check('R2-5 no physical default network is never live',
    policy.protectedPathLive({ ...liveFacts, physicalNetId: 0 }) === false);
  // The DNS-only rule: the conservative scope the design asks for (mgmt/signal/
  // relay keep being served), and it never fires before create() — the startup
  // carve-out of the network rule is preserved.
  check('R2-6 an unusable protected path refuses the DNS supply with its own reason',
    policy.socketSupplyRejectionReason({ ...gate, ...outageGate,
      protectedPathUnusable: true, isDnsRequest: true }) === 'protected-path-unusable');
  check('R2-7 the same unusable path still serves management/signal/relay',
    policy.socketSupplyRejectionReason({ ...outageGate,
      protectedPathUnusable: true, isDnsRequest: false }) === '');
  check('R2-8 the instantaneous network rule wins over the DNS-only rule',
    policy.socketSupplyRejectionReason({ ...outageGate, networkUnusableNow: true,
      protectedPathUnusable: true, isDnsRequest: true }) === 'network-unusable');
  check('R2-9 before the first create the DNS path is not gated by it (startup carve-out)',
    policy.socketSupplyRejectionReason({ ...outageGate, networkUnusableNow: true,
      networkRecoveryReady: false, protectedPathUnusable: true, isDnsRequest: true }) === '');
  check('R2-10 a live path changes nothing (the pre-existing rules still decide)',
    policy.socketSupplyRejectionReason({ ...outageGate,
      protectedPathUnusable: false, isDnsRequest: true }) === '');
  check('R2-11 the gate carries the R2 input (the ONE purpose-specific field) and no latch',
    policySource.includes('protectedPathUnusable: boolean') &&
    policySource.includes('isDnsRequest: boolean') &&
    !policySource.includes('networkDirty: boolean'));
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

  // -------------------------------------------------------------------------
  // R3 (REPORT §6.4; upstream `client/netevents/netevents.go:48-68` and
  // `client/internal/peer/guard/guard.go:123-125`): "is there a network at all"
  // is a SECOND signal next to "the network changed". The availability verdict
  // is pure policy (`networkEventAvailability`) so it is host-verifiable; the
  // observer forwards all three platform events to ONE core entry, and the
  // existing change seam keeps exactly one call site of its own.
  // -------------------------------------------------------------------------
  check('a lost default network is the offline verdict',
    policy.networkEventAvailability('netLost') === false);
  check('an available default network is the online verdict',
    policy.networkEventAvailability('netAvailable') === true);
  check('a capability change on the default handle means a network is present',
    policy.networkEventAvailability('netCapabilitiesChange') === true);
  check('an unknown observer event has no availability verdict (never invent offline)',
    policy.networkEventAvailability('netSpeedChange') === null &&
    policy.networkEventAvailability('') === null);
  check('the observer forwards each availability event to the ONE availability entry',
    ability.includes("this.noteNetworkAvailability('netLost');") &&
    ability.includes("this.noteNetworkAvailability('netAvailable');") &&
    ability.includes("this.noteNetworkAvailability('netCapabilitiesChange');") &&
    ability.includes('private noteNetworkAvailability(event: string): void {') &&
    ability.includes('connectorNetworkAvailable(available);'));
  check('availability is a SEPARATE entry from the physical-network change seam',
    [...ability.matchAll(/connectorNetworkAvailable\(/g)].length === 1 &&
    [...ability.matchAll(/connectorPhysicalNetworkChanged\(/g)].length === 1);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
