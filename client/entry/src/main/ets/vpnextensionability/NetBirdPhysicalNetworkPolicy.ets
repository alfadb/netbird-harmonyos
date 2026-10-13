// Pure policy shared by network observation and protected socket supply.
export function isPhysicalNetworkSwitch(previousNetId: number, currentNetId: number): boolean {
  return previousNetId !== currentNetId;
}

// F4 (v0.0.27 outage REPORT.md §5): the dedupe key of a network handle's
// capability set. Upstream dedupes the callback on the network HANDLE
// (ConcreteNetworkAvailabilityListener.java:70-84: a same-type AP handover
// rebinds every connection and must count as a transition); HarmonyOS exposes
// no handle identity beyond the netId, so the capability set is the only
// additional distinguishable fact — a capability change for the handle the
// platform currently reports as the default one is a re-evaluation trigger even
// when the netId did not move. Order-insensitive (the platform does not promise
// one), and the empty set signs as '' so "no observation" can never equal a real
// set.
export function capabilitySignature(caps: number[]): string {
  if (caps.length === 0) { return ''; }
  const sorted: number[] = caps.slice();
  sorted.sort((a: number, b: number): number => a - b);
  return sorted.join(',');
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
  // F1 (v0.0.27 outage REPORT.md §5 + §3 R1): the physical default network is
  // unusable **at THIS request** — the platform currently reports no usable
  // physical default network (`physicalDefaultNetId() === 0`). It is an
  // INSTANTANEOUS predicate, never a remembered switch. It replaced the old
  // `networkDirty` edge latch, which was set by a switch and cleared only by a
  // FULLY successful host-side recovery: one failed recovery (a single EPERM on
  // the protected-DNS resolve) then refused every management / signal / relay /
  // DNS socket until a NEW netId edge arrived — with no new edge for 5 h 29 m on
  // the device (REPORT.md §0.1, §2.4). Upstream has no such host-side latch at
  // all: its host layer reports availability now and protects one fd per dial
  // (`client/net/protectsocket_android.go:16-47`), and the core's own guard
  // gates on the CURRENT online verdict (`peer/guard/guard.go:123`). This field
  // is that verdict's host-side analogue; the epoch/stale-fd fail-closed check
  // (`fulfillSocketRequest`) and `protectedConnectionRejectionReason` keep what
  // the latch was actually there for.
  networkUnusableNow: boolean;
  networkRecoveryReady: boolean;
  protectDeviation: boolean;
  // N2 first batch C/D columns: the formal-switch production-recovery gate.
  // `true` = a C/D plan's formal switch is reported but the core has NOT
  // released it yet (or the plan ended UNSAT): NO production socket may be
  // handed to the core for this switch. It is checked BEFORE the network rule
  // and, unlike that rule, has NO carve-out of any kind — the pre control the
  // gate waits for is sent by the shell's own opener, never through here, so
  // refusing even the DNS socket cannot deadlock the release.
  productionGateBlocked: boolean;
}

export function socketSupplyRejectionReason(gate: SocketSupplyGate): string {
  const connectionReason: string = protectedConnectionRejectionReason(gate);
  if (connectionReason.length > 0) { return connectionReason; }
  if (gate.protectDeviation) { return 'deviation'; }
  if (gate.productionGateBlocked) { return 'production-gate'; }
  // Before the first create(), the protected startup/retry sockets must be
  // available even if the physical network is momentarily unusable (login is
  // allowed to fail on its own terms; the shell must not invent a gate).
  if (gate.networkUnusableNow && gate.networkRecoveryReady) { return 'network-unusable'; }
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
