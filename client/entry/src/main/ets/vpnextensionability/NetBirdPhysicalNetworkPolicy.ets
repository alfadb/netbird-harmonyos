// Pure policy shared by network observation and protected socket supply.
export function isPhysicalNetworkSwitch(previousNetId: number, currentNetId: number): boolean {
  return previousNetId !== currentNetId;
}

export interface SocketSupplyGate {
  networkDirty: boolean;
  networkRecoveryReady: boolean;
  networkRecoveryActive: boolean;
  isDns: boolean;
  destroyRequested: boolean;
  destroyInFlight: boolean;
  recreateInFlight: boolean;
  hasConnection: boolean;
  protectDeviation: boolean;
}

export function socketSupplyRejectionReason(gate: SocketSupplyGate): string {
  if (gate.destroyRequested || gate.destroyInFlight) { return 'destroy'; }
  if (gate.recreateInFlight) { return 'recreate'; }
  if (!gate.hasConnection) { return 'no-connection'; }
  if (gate.protectDeviation) { return 'deviation'; }
  // Before the first create(), the protected startup/retry sockets must be
  // available even if the physical network actually changed during login.
  if (gate.networkDirty && gate.networkRecoveryReady &&
    !(gate.isDns && gate.networkRecoveryActive)) { return 'network-dirty'; }
  return '';
}
