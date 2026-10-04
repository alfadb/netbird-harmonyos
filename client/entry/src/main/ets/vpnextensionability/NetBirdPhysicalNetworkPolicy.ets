// Pure policy shared by network observation and protected socket supply.
export function isPhysicalNetworkSwitch(previousNetId: number, currentNetId: number): boolean {
  return previousNetId !== currentNetId;
}

export interface ProtectedConnectionGate {
  destroyRequested: boolean;
  destroyInFlight: boolean;
  recreateInFlight: boolean;
  hasConnection: boolean;
  connectionUnchanged: boolean;
}

export function protectedConnectionRejectionReason(gate: ProtectedConnectionGate): string {
  if (gate.destroyRequested || gate.destroyInFlight) { return 'destroy'; }
  if (gate.recreateInFlight) { return 'recreate'; }
  if (!gate.hasConnection) { return 'no-connection'; }
  if (!gate.connectionUnchanged) { return 'connection-changed'; }
  return '';
}

export interface SocketSupplyGate extends ProtectedConnectionGate {
  networkDirty: boolean;
  networkRecoveryReady: boolean;
  networkRecoveryActive: boolean;
  isDns: boolean;
  protectDeviation: boolean;
  // N2 first batch C/D columns: the formal-switch production-recovery gate.
  // `true` = a C/D plan's formal switch is reported but the core has NOT
  // released it yet (or the plan ended UNSAT): NO production socket may be
  // handed to the core for this switch. It is checked BEFORE the networkDirty
  // rule and is deliberately NOT covered by that rule's
  // `isDns && networkRecoveryActive` carve-out — the pre control the gate waits
  // for is sent by the shell's own opener, never through here, so refusing even
  // the DNS socket cannot deadlock the release.
  productionGateBlocked: boolean;
}

export function socketSupplyRejectionReason(gate: SocketSupplyGate): string {
  const connectionReason: string = protectedConnectionRejectionReason(gate);
  if (connectionReason.length > 0) { return connectionReason; }
  if (gate.protectDeviation) { return 'deviation'; }
  if (gate.productionGateBlocked) { return 'production-gate'; }
  // Before the first create(), the protected startup/retry sockets must be
  // available even if the physical network actually changed during login.
  if (gate.networkDirty && gate.networkRecoveryReady &&
    !(gate.isDns && gate.networkRecoveryActive)) { return 'network-dirty'; }
  return '';
}

// ---------------------------------------------------------------------------
// 场景B roaming fix (LAN-ROUTE-ROAMING-ANALYSIS.md §4 step 2): after a
// measured physical default-network switch (Wi-Fi <-> cellular, or another
// Wi-Fi), the shell re-derives the base host-LAN exclusion from the NEW
// network (NetBirdConnector.physicalDefaultLanProbe +
// NetBirdEndpointExclusion.deriveLanExclusion, identity compared via
// NetBirdEndpointExclusion.lanExclusionSignature) and compares it with the
// one merged into the LIVE platform VpnConfig; only a CHANGE walks the
// existing N8 controlled-recreate path (the platform VpnConfig is fixed at
// create() time — an exclusion change can only reach the tunnel by
// rebuilding the connection). PURE decision only: the collector supplies the
// facts, this module never reads the platform and never owns WHEN (the
// debounce window lives in the ability).
// ---------------------------------------------------------------------------

export interface LanRoamGate {
  // An accepted N2 evidence ticket armed this session (n2Evidence !==
  // undefined): the forensic path keeps its ORIGINAL behavior — no
  // autonomous roam recreate may disturb it.
  evidenceMode: boolean;
  destroyRequested: boolean;
  recreateInFlight: boolean;
  // The live platform connection exists AND its generation settled — the
  // same triple performControlledRecreate requires before rebuilding.
  hasSettledConnection: boolean;
  // The physical default network is GONE right now (netId 0): nothing to
  // re-derive from; the next available switch re-arms the check.
  networkLost: boolean;
  // An N2 C/D plan's formal-switch production gate is armed/held: a rebuild
  // mid-plan would ruin the measurement the gate exists to protect.
  productionGateBlocked: boolean;
  // The LAN exclusion merged into the LIVE platform VpnConfig vs the one
  // re-derived from the CURRENT physical default network.
  appliedSignature: string;
  currentSignature: string;
}

export interface LanRoamDecision {
  // 'recreate' | 'skip'
  action: string;
  // 'local-lan-exclusion-changed' when recreating; a stable skip token
  // otherwise (greppable, logged as the roam line's result field).
  reason: string;
}

// PURE: lifecycle and N2 gates first (nothing may rebuild during teardown,
// evidence collection or an in-flight recreate), then the network state,
// then the exclusion comparison. Skip order is the documented precedence:
// destroy > evidence > recreate-in-flight > connection > network-lost >
// production gate > comparison.
export function lanRoamDecision(gate: LanRoamGate): LanRoamDecision {
  if (gate.destroyRequested) { return { action: 'skip', reason: 'destroy-requested' }; }
  if (gate.evidenceMode) { return { action: 'skip', reason: 'n2-evidence-mode' }; }
  if (gate.recreateInFlight) { return { action: 'skip', reason: 'recreate-in-flight' }; }
  if (!gate.hasSettledConnection) { return { action: 'skip', reason: 'no-settled-connection' }; }
  if (gate.networkLost) { return { action: 'skip', reason: 'network-lost' }; }
  if (gate.productionGateBlocked) { return { action: 'skip', reason: 'production-gate-blocked' }; }
  if (gate.appliedSignature === gate.currentSignature) {
    return { action: 'skip', reason: 'exclusion-unchanged' };
  }
  return { action: 'recreate', reason: 'local-lan-exclusion-changed' };
}

// Debounce arithmetic: the check is due only once the LAST switch is at
// least stableMs in the past — every switch refreshes lastSwitchAtMs, so a
// flapping Wi-Fi/cellular handover (or a lost->available pair) never acts
// mid-storm: the window restarts on each hop and the check fires once, on
// the network that actually stayed.
export function lanRoamCheckDue(nowMs: number, lastSwitchAtMs: number, stableMs: number): boolean {
  return nowMs - lastSwitchAtMs >= stableMs;
}
