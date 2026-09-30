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
