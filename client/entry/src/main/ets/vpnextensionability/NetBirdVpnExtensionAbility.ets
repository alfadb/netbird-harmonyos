import VpnExtensionAbility from '@ohos.app.ability.VpnExtensionAbility';
import Want from '@ohos.app.ability.Want';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import vpnExtension from '@ohos.net.vpnExtension';
import connection from '@ohos.net.connection';
import core from 'libnetbird_core.so';
import { VpnRouteConfig, VpnTunnelConfig, buildPlatformVpnConfig, gateDefaultRoutes,
  loadVpnTunnelConfig, applyNetworkConfig, withEndpointExclusions,
  CaptureRouteOutcome, TunnelSubnetRouteOutcome, withCaptureRoute,
  withTunnelSubnetRoute } from './NetBirdVpnConfig';
import { N2EvidenceDecision, N2EvidenceOptions, parseN2Evidence, withN2EvidenceRoutes,
  withoutN2ExcludedRoutes } from './NetBirdN2Evidence';
import { N2ControlProtocol, sendN2Control } from './NetBirdN2Control';
import { N2_CONTROL_FILE_NAME, N2ControlDecision,
  formatN2ControlShellLine, formatN2ControlShellReject,
  parseN2ControlRequest } from './NetBirdN2ControlRequest';
import { N2_STIMULUS_FILE_NAME, N2StimulusDecision, N2StimulusReply,
  formatN2StimulusShellLine, formatN2StimulusShellReject, parseN2Stimulus,
  parseN2StimulusReply } from './NetBirdN2Stimulus';
import { RouteCheckResult, RouteViolation, EXCLUSION_GENERATION_INITIAL,
  EXCLUSION_GENERATION_RECREATE, formatExclusionGeneration, formatRouteTable,
  formatRouteViolation, validateRouteEntries } from './NetBirdRouteValidation';
import { runInterfaceStatsProbe, runNetworkContextProbe, runInterfaceStatsWindowClose,
  readInterfaceStatsBurst } from
  './NetBirdInterfaceStatsProbe';
import { resolveVpnTunNetId, VpnNetIdResolution } from './NetBirdInterfaceStatsProbe';
import { ExcludedRouteEntry, EndpointResolution, ExclusionBuildResult, ExclusionEndpoint,
  DnsDerivationResult,
  StunDerivationResult, StunUriEntry,
  RelayEndpoint, buildExcludedRouteEntries, deriveStunEndpointExclusions,
  isExclusionFailClosed, parseRelayUrl,
  stripDialPort
} from './NetBirdEndpointExclusion';
import {
  ConnectorStatusResult, CoreNetworkConfig, CoreRouteEntry, ConnectorStartResult,
  DeviceConfigFile, ManagementEndpoint, NativeTcpSocketOpen, RelayAdvertisedView,
  RouteSetAppliedResult, SocketFeedResult, WgFeedResult,
  DEFAULT_DEVICE_CONFIG_FILE, DEFAULT_SETUP_KEY_FILE, NETWORK_MAP_POLL_MS, NETWORK_MAP_WAIT_MS,
  PACKAGED_CA_ROOTS_RAWFILE, STATUS_POLL_MS, buildConnectorConfigJson, connectorIceSocketFeed,
  connectorNetworkConfig, connectorRouteSetApplied, connectorSignalFeed, connectorStartWithSocket,
  connectorSocketFeed, connectorPhysicalNetworkChanged, connectorManagementAddressUpdate,
  connectorStatus, connectorStop, connectorTunFdFeed, connectorWgSocketFeed,
  socketSupplyPoll, socketSupplyReply, socketSupplyCloseFd, socketSupplyShutdown,
  socketSupplyDnsServers, physicalDefaultDnsServers, physicalDefaultNetId, forgetPhysicalDefaultDns,
  resolveProtectedIp,
  SocketSupplyRequest,
  countPemCertificates, derivePlatformDnsEndpoints,
  formatRelayStatusLine,
  iceSocketOpen, mgmtSocketOpen, parseDeviceConfig, parseManagementEndpoint, parseRelayAdvertised,
  parseSignalEndpoint,
  pollStatusOrNull, readCredentialFileText, readPackagedCaRootsPem, resolveExclusionEndpointIp,
  resolveManagementIp, sleep
} from './NetBirdConnector';

import { isPhysicalNetworkSwitch, protectedConnectionRejectionReason,
  socketSupplyRejectionReason } from './NetBirdPhysicalNetworkPolicy';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const MAX_NATIVE_FD: number = 2147483647;
// Bound for the VpnConnection.protect() wait (house rule: every ArkTS-side
// wait is bounded). Same 5 s box as the verified spike helper
// protectSocketWithBox (spikes/n1b-disc-phys-hap e8e2cf9). Unlike the spike's
// best-effort treatment, the shell treats box expiry as protect failure
// (governance docs/native-nx-governance.md item 4: fail-closed).
const PROTECT_BOX_MS: number = 5000;
// ICE socket pool: the ICE gather consumes one FRESH unbound UDP socket per
// IPv4 interface round (dups share the socket object, so a bound copy cannot
// be re-bound), and a device has several interfaces — one socket starves the
// gather after the first round (device-verified 2026-09-14).
const ICE_SOCKET_POOL_SIZE: number = 8;
// A failed ICE session is re-negotiated after a cooldown, and every retry
// consumes fresh sockets again (the pool is one-shot). Refill when status
// reports a drained queue, with a periodic retry while the VPN is armed; a
// core that is not gathering must not make the shell pile up fds (device run 3
// reached queue=26 while nothing consumed them).
const ICE_SOCKET_TOPUP_MS: number = 4000;
// Keep a full pool queued, not a trickle: ONE gather round binds one socket
// per IPv4 interface, so a queue of 2-3 drains mid-round and the core reports
// "protected-udp: no-protected-socket" (device run 5).
const ICE_TOPUP_BATCH: number = 4;
// N3-7: bound for the shell-side DNS resolution of the management host
// (DNS must stay OUT of native so the protected dial never pulls nameserver
// traffic through our own tunnel).
const DNS_RESOLVE_BOX_MS: number = 5000;
// N3-7 gap 3: consecutive connector_status() native-call failures before the
// watcher concludes the connector is unreachable and tears the VPN down.
const CONNECTOR_POLL_FAILURE_LIMIT: number = 3;
// N8: bounded boxes for the controlled-recreate destroy/create legs (house
// rule: every ArkTS-side wait is bounded). Box expiry = recreate failure =
// fail-closed teardown (never a half-configured tunnel).
const RECREATE_DESTROY_BOX_MS: number = 5000;
const RECREATE_CREATE_BOX_MS: number = 5000;
// N2-H I5 (§5.3-1): the development-channel arm trigger of the device-side
// evidence collector. The file only carries NON-SENSITIVE window options; its
// EXISTENCE is the arm request. No arm file ⇒ the collector stays inert.
// N2-H arm private channel (2026-09-18): the ticket is NO LONGER under the
// shared /data/local/tmp location — that path is invisible to the app sandbox
// (device 19:49-19:50: `fs.accessSync` answered boolean false →
// accessError=none, NOT a proven ENOENT — while the shell listed the same
// file; only "this process cannot see the path" is proven, never SELinux or a
// mount verdict). EntryAbility now writes the ticket from the non-secret
// `n2h_arm_options` want parameter into the app's OWN filesDir — the SAME
// basename and the SAME directory mechanism as the credential files — and
// deletes a leftover ticket on every launch WITHOUT the parameter (default:
// no arm). The name below is resolved under this.context.filesDir at read
// time (n2hArmFilePath), never a fixed absolute path.
const N2H_ARM_FILE_NAME: string = 'netbird-n2h-arm';
// N2-H I5a (§13.15): the two outcomes the arm-file probe can PROVE, and the
// documented "No such file or directory" code that separates them. SDK
// 26.0.0.821 `@ohos.file.fs.d.ts:309-336` (`accessSync(path, mode?): boolean`)
// documents BOTH shapes for a file that is not there — the boolean `false`
// return and the BusinessError `13900002` — and other codes for a probe that
// failed WITHOUT proving absence (`13900012` permission denied / `13900005`
// I/O error / …). "proved absent" (false OR 13900002) and "could not be read"
// (any other code) are therefore distinguishable and must NEVER be one reason
// (device dry-run 2026-09-18: 84 zero-row collector reads with no marker
// explaining that the arm had never even been attempted).
const N2H_ARM_FILE_ABSENT: string = 'arm-file-absent';
const N2H_ARM_FILE_UNREADABLE: string = 'arm-file-unreadable';
const N2H_ACCESS_ENOENT: string = '13900002';
// §5.1 arm-option defaults (the file may override them; core re-validates).
const N2H_POSCTL_PORT_DEFAULT: number = 40001;
const N2H_PROBE_WAIT_MS_DEFAULT: number = 800;

// S2-S4 diagnostic switches (2026-09-23, REAL-DEVICE diagnostic rounds only):
// three ADDITIONAL keys of the SAME `netbird-n2h-arm` ticket EntryAbility
// writes from the non-secret `n2h_arm_options` want parameter (and deletes on
// every launch without it — the structural default-OFF guarantee). They are
// read SEPARATELY and EARLIER than the arm options (startConnectorThenCreate:
// the blocked-app list must be final before create()), they are NEVER
// forwarded to the core (the arm opts document below is built field by field,
// so an extra ticket key has no path into it), and each one is enabled ONLY by
// a literal JSON boolean true. An ABSENT key is off and prints nothing; a key
// that is present with a wrong type is off and prints exactly one
// N2H_DIAG_OPTION_IGNORED|key=…|reason=… line. No switch ever changes a gate:
// they only add one blocked-app entry (S2), fire one diagnostic platform call
// (S3) or withhold one diagnostic probe option (S4) — every result is
// `diag=true|is_evidence=false`.
//
// The former S1 `diag_tunnel_route` key is RETIRED (2026-09-26): the tunnel
// subnet is declared in VpnConfig.routes UNCONDITIONALLY (injectTunnelSubnetRoute
// below / withTunnelSubnetRoute in NetBirdVpnConfig). The key is no longer read
// at all, so a ticket that still carries it is handled like any other unknown
// key: silently ignored, with no diagnostic line.
const N2H_DIAG_BLOCKED_SELF_KEY: string = 'diag_blocked_self';
const N2H_DIAG_PROTECT_PROCESS_NET_KEY: string = 'diag_protect_process_net';
const N2H_DIAG_POSCTL_UNBOUND_KEY: string = 'diag_posctl_unbound';
// S5 (2026-09-26, default OFF): `diag_iface_bytes_burst`. The SAME arm-ticket
// mechanism as S2-S4 (readDiagSwitch: literal JSON boolean true only, an
// absent key is off and silent, a wrong-typed value is off plus the one
// N2H_DIAG_OPTION_IGNORED line). It changes no gate and adds no evidence: when
// it is true, the arm SUCCESS branch (the N2H_COLLECTOR_ARMED line, where the
// iface-bytes availability probe already runs) additionally starts a BOUNDED
// 500 ms series of vpn-tun byte reads on the SAME official per-NIC API — the
// survey half of the C7 same-source calibration plan
// (n2h-kit/c7-same-source-plan-20260926.md §3 mechanism a / §6.1), whose
// criterion needs the two window edges to be pairable against the host's
// /proc time series within <1 s (v4 §4 (ii)). The new
// `N2H_IFACE_BYTES_BURST|` prefix is deliberately distinct from
// `N2H_IFACE_BYTES_WINDOW|` (plan §6.4: the window prefix is pinned at exactly
// one line per window by the selfcheck).
const N2H_DIAG_IFACE_BYTES_BURST_KEY: string = 'diag_iface_bytes_burst';
// S6 (2026-09-26, default OFF): `diag_capture_route`. The SAME arm-ticket
// mechanism as S2-S5 (readDiagSwitch: literal JSON boolean true only, an absent
// key is off and silent, a wrong-typed value is off plus the ONE
// N2H_DIAG_OPTION_IGNORED line). When it is true AND the ticket carries a
// usable `protect_probe.target`, the shell appends ONE explicit NON-excluded
// `<target>/32` route to VpnConfig.routes on BOTH legs (withCaptureRoute,
// same placement as the tunnel-subnet route) and logs one
// N2H_DIAG_CAPTURE_ROUTE line per leg. This is the protect-bypass experiment's
// capture face (plan §3.1 + §6 item 1②): without it the target would be
// unreachable through the tunnel and the P0/P1/P2 arms would be meaningless.
// The target is read from the SAME ticket object the arm forwards to the core
// verbatim (`protect_probe`) — never from the want, never a second key. Off:
// nothing is read into the config (byte-identical VpnConfig, no line).
const N2H_DIAG_CAPTURE_ROUTE_KEY: string = 'diag_capture_route';
const N2H_ENDPOINT_EXCLUSIONS_KEY: string = 'endpoint_exclusions_enabled';
// S7 (2026-09-26, opt-in): the protect-probe fd hand-off's trigger. The ticket
// key is the SAME `protect_probe` object core parses (`{target, count,
// mode:"protect"|"none"}`); ONLY `mode:"protect"` adds any shell behaviour
// (open -> protect -> commit BEFORE the arm). `mode:"none"` and an absent key
// are byte-identical to the pre-increment arm path.
const N2H_PROTECT_PROBE_KEY: string = 'protect_probe';
const N2H_PROTECT_PROBE_MODE_PROTECT: string = 'protect';
// The burst cadence (plan §0/§6.1: 500 ms) and its hard cap. The app-side
// window close edge lands about one watcher period after ARMED
// (STATUS_POLL_MS = 5000) and must NOT cut the series short: the C7 same-source
// calibration pairs two sample points ≥10 s apart and depends only on the
// peer's steady one-frame-per-second traffic, not on the app window being open.
// So 40 ticks at 500 ms span 20 s — comfortably past that ≥10 s pairing gap —
// while staying far below the plan's ≤120-read ceiling. The cap is the ordinary
// stop; the other one is teardown (stopStatusAndNetcfgWatcher).
const N2H_IFACE_BURST_INTERVAL_MS: number = 500;
const N2H_IFACE_BURST_MAX_TICKS: number = 40;
// The ONLY reason token of the S2-S4 ignored-key line: the key was present in
// the ticket document but its value was not a JSON boolean (string / number /
// null / object). Never printed for an absent key, never for a boolean false.
const N2H_DIAG_OPTION_IGNORED_REASON: string = 'not-boolean';
// This app's own bundle name, needed by the S2 diagnostic switch
// (blockedApplications). Deliberately a LOCAL constant: the same literal
// exists in the UI half (EntryAbility.ets BUNDLE_NAME, Index.ets) and the
// extension must not import a UI page/ability module for a name.
const SELF_BUNDLE_NAME: string = 'cn.alfadb.netbird';

interface BusinessFailure extends Error {
  code?: number;
}

interface CreateOutcome {
  destroyRequired: boolean;
  accepted: boolean;
  reason: string;
}

// N2-H: outcome of the signal provisioning leg, extended to carry the parsed
// endpoint + the IP the protected signal socket ACTUALLY dialed (first-fed
// address wins core-side) — both feed the exclusion derivation.
interface SignalProvisionResult {
  endpoint: ManagementEndpoint;
  dialIp: string;
}

// N2-H: outcome of the endpoint-exclusion derivation. `fatal=true` is the
// fail-closed verdict (relay required but some endpoint could not be frozen):
// the caller must NOT create the VPN.
interface EndpointExclusionOutcome {
  fatal: boolean;
  routes: ExcludedRouteEntry[];
}

// N2-H I5: one shell-exclusion entry of the arm options (§5.1
// `{"shell_exclusions":[{kind,host,port,resolved}]}`, the D7
// `shell-exclusion` freeze source).
interface N2hShellExclusion {
  kind: string;
  host: string;
  port: number;
  resolved: string;
}

// N2-H I5: the non-sensitive options the arm control file may carry (0 =
// absent → the §5.1 default).
interface N2hArmFileOptions {
  posctl_port: number;
  probe_wait_ms: number;
  // N2-H §13.10 (ops relay): the b2a marker the counterparty announced, as its
  // `nbinterop n2h-peer` line prints it (`b2a_probe_id=N2H-posctl-<24 hex>`).
  // ABSENT key ⇒ nothing is passed ⇒ the b2a leg stays a reasoned gap; a
  // present value is forwarded verbatim and the core refuses anything that is
  // not the frozen posctl shape (the arm then fails with `invalid-opts`).
  peer_b2a_probe_id?: string;
  // 2026-09-18 (parallel posctl-target fix): the ACTUAL n2h-peer overlay
  // address, so the posctl legs stop assuming the FIRST peer in the map.
  // Same verbatim discipline as peer_b2a_probe_id: the shell never validates,
  // trims, rewrites, type-coerces, or defaults it — a present value (whatever
  // its runtime type) is forwarded as-is and the CORE strictly refuses
  // anything that is not a bare IPv4 present in the peer map.
  peer_overlay_ip?: string;
  // v4 effective preregistration §3 path① (2026-09-20) / §6.2 item 2: the
  // explicit enable of the `dns-nxdomain-echo` receipt method on the device
  // collector's dns probe. OPTIONAL and default OFF (the CORE's `ArmOptions`
  // default `false` keeps the pre-existing ASCII/no-echo probe byte-identical).
  // The key is forwarded ONLY when the control file carried it: an absent key
  // stays absent — the shell never injects a `false`, because "key missing" and
  // "explicit false" are a distinct, core-defined document semantics even
  // though both mean the core default. Same verbatim discipline as
  // peer_overlay_ip: the shell never validates, trims, rewrites or type-coerces
  // it; whatever value the parsed JSON carries rides the opts verbatim and the
  // CORE's strict boolean check refuses a non-boolean (fail-closed).
  dns_nxdomain_echo?: boolean;
  // 2026-09-26 (protect-bypass plan §5): the multi-probe positive-control count
  // (`posctl_probes`, core default 1, cap 10). Forwarded ONLY when the control
  // file carried the key — an absent key stays absent so the core's documented
  // default keeps the single-probe window byte-identical. Same verbatim
  // discipline as peer_overlay_ip: whatever value the parsed JSON carries rides
  // the opts as-is (a wrong-typed value is forwarded AS THAT VALUE) and the
  // core's strict u64 + range check refuses it.
  posctl_probes?: number;
  // 2026-09-26 (protect-bypass plan §4/§5): the opt-in protect probe arm
  // (`{target:"IPv4:port", count, mode:"protect"|"none"}`). Forwarded ONLY when
  // the file carried the key (absent stays absent: the core then opens no
  // socket, sends nothing and adds no evidence byte). Never validated, trimmed
  // or type-coerced here — the core's strict parse (bare IPv4:port, count
  // 1..=10, frozen mode tokens) is the authority. The shell reads the SAME
  // object once more for its two local triggers: the `diag_capture_route`
  // target (readDiagSwitches) and the `mode:"protect"` fd hand-off
  // (maybeArmN2hCollector).
  protect_probe?: N2hProtectProbeTicket;
}

// 2026-09-26 (protect-bypass plan §4/§5): the ticket's `protect_probe` object
// as the CORE defines it. The shell only ever reads `target` (string check) and
// `mode` (strict literal compare); every other shape decision is core-side.
interface N2hProtectProbeTicket {
  target: string;
  count: number;
  mode: string;
}

// {ok,fd,handoff_id,bind_rc,bind_errno} JSON of the synchronous
// core.n2h_probe_socket_open() NAPI export (the ice_socket_open hand-off shape):
// ONE unbound, core-owned UDP fd the shell runs its protect wrapper on.
// {ok:false,error,errno} on failure carries NO fd (a failed open answers
// nothing usable rather than a -1 the caller might pass on).
interface ProbeSocketOpenNative {
  ok: boolean;
  fd?: number;
  handoff_id?: string;
  bind_rc?: number;
  bind_errno?: number;
  error?: string;
  errno?: number;
}

// {ok,fd,handoff_id,protect_result} / {ok:false,error} JSON of the synchronous
// core.n2h_probe_socket_commit(fd, result) NAPI export: the shell reports ITS
// protect outcome for the very fd it was handed. `error` is one of the core's
// refusal tokens (`bad-result` / `no-pending-fd` / `fd-mismatch` /
// `fd-invalid`).
interface ProbeSocketCommitNative {
  ok: boolean;
  fd?: number;
  handoff_id?: string;
  protect_result?: string;
  error?: string;
}

// The dedicated protect wrapper's four-way answer (plan §4): the `outcome`
// token is the log's `outcome=` field and `result` is the commit token the core
// records (`protected` / `unprotected` / `error`).
interface ProtectProbeOutcome {
  outcome: string;
  result: string;
  elapsedMs: number;
}

// N2-H I5: the `n2h_collector_arm` options document (§5.1).
// N2-H v3.1 C1 dns (R-4): the branch record the arm hands to the core, which
// renders it into the evidence document's `notes[]` verbatim. `branch` is one
// of `default` / `all-nets-filtered` / `none`; `gap_reason` is present only
// when the shell recorded a dns required-class gap.
interface N2hDnsArmReport {
  branch: string;
  physical_evidence: string;
  gap_reason?: string;
}

interface N2hArmOptions {
  shell_exclusions: N2hShellExclusion[];
  posctl_port: number;
  probe_wait_ms: number;
  // §13.10: only set when the control file carried the key (an absent key must
  // stay absent — the two cases have different, documented semantics).
  peer_b2a_probe_id?: string;
  // 2026-09-18: the actual n2h-peer overlay address, only set when the control
  // file carried the key (absent stays absent; forwarded verbatim, validated
  // core-side — see N2hArmFileOptions).
  peer_overlay_ip?: string;
  // v4 §3 path① / §6.2 item 2: the dns-nxdomain-echo enable, only set when the
  // control file carried the key (absent stays absent — no `false` injected;
  // forwarded verbatim, validated core-side — see N2hArmFileOptions).
  dns_nxdomain_echo?: boolean;
  // v3.1 C1 dns: the branch record of this session's derivation (always set —
  // a missing record is a dns gap for the judge).
  dns?: N2hDnsArmReport;
  // N2-H I3a (v3.1 §2.1 R-2①): the evidence generation G — the gen of the
  // last ACCEPTED create() — which the core echoes into the evidence
  // document's `collector.exclusion_generation`. Only set when an accepted
  // generation exists (see n2hAcceptedExclusionGeneration); an absent key
  // makes the core record a value-less field, never a fabricated generation.
  exclusion_generation?: number;
  // 2026-09-20 (diagnostic-only): the VPN-network netId the shell resolved
  // for the core's own TUN interface (resolveVpnTunNetId, official read-only
  // getters only). When present, the core DIAGNOSTICALLY binds the ONE
  // posctl probe socket to that network before its send (OH_NetConn_BindSocket,
  // runtime-resolved; every failure is recorded and never changes the send).
  // Only set when exactly one vpn-tun handle was found; an absent key keeps
  // the probe byte-identical to the pre-binding behavior.
  probe_net_id?: number;
  // 2026-09-26 (protect-bypass plan §5/§4): the multi-probe count and the
  // opt-in protect probe, forwarded with the SAME present-only + verbatim
  // discipline as peer_overlay_ip / dns_nxdomain_echo (see N2hArmFileOptions).
  // An absent key must stay absent: the core's defaults (1 probe, no protect
  // probe = no fd, no socket, no line, no evidence byte) are what the omission
  // means.
  posctl_probes?: number;
  protect_probe?: N2hProtectProbeTicket;
  n2_capture_32?: string[];
}

// N2-H I5: the `n2h_collector_arm` answer (§5.1) — `{ok:true,armed:true,
// opened_mono}` or `{ok:false,error:"<token>"}`.
interface N2TunDestConfigureResult {
  ok: boolean;
  error?: string;
}

interface N2hCollectorArmResult {
  ok: boolean;
  armed?: boolean;
  opened_mono?: number;
  error?: string;
}

// N2-H I5a (§13.15): the arm control-file probe's outcome — the arm decision
// plus the observable explanation of a skip. `skipReason` is '' while the file
// is present, otherwise exactly one of N2H_ARM_FILE_ABSENT /
// N2H_ARM_FILE_UNREADABLE; `accessError` is 'none' when the API answered with a
// boolean, the BusinessError CODE as a decimal token when it threw, 'no-code'
// when it threw without a code (codes only — never path content, never message
// text, never the file's contents).
interface N2hArmProbeOutcome {
  present: boolean;
  skipReason: string;
  accessError: string;
}

// {open,errno} JSON returned by the synchronous core.fd_status(fd) NAPI export.
interface FdStatusNative {
  open: boolean;
  errno: number;
}

interface FdSnapshot {
  fd: number;
  integer: boolean;
  nonnegative: boolean;
  probeEligible: boolean;
  probed: boolean;
  open: boolean;
  errno: number;
}

// {fd,bind_rc,bind_errno} JSON returned by the synchronous core.wg_fwd_open()
// NAPI export: native opens + binds the tunnel-outer UDP socket alone so the
// shell can protect it BEFORE any datagram flows (verified WG-FWD-PROTECT
// split, spikes/n1b-disc-phys-hap e8e2cf9).
interface NativeSocketOpen {
  fd: number;
  bind_rc: number;
  bind_errno: number;
}

function clean(value: string): string {
  return value.replace(/\|/g, '/').replace(/\r/g, ' ').replace(/\n/g, ' ').slice(0, 160);
}

function safeError(error: Error): string {
  const code: string = String((error as BusinessFailure).code ?? 'none');
  const name: string = clean(error.name || 'Error');
  const message: string = clean(error.message || 'no-message');
  return `code=${code},name=${name},message=${message}`;
}

// VPN extension shell, config-driven (promoted from the verified e3 physical
// preflight create/destroy + fd contract and the verified n1b MR4 config /
// protect split):
// - Start flow (N3-6 + N3-7 + N5d + N7): credentials read from app-private
//   filesDir files (DEVELOPMENT-TIME practice, NOT secure storage) ->
//   shell-side DNS of the management host -> core.mgmt_socket_open (TCP,
//   unconnected) -> fail-closed VpnConnection.protect(fd) gate ->
//   core.connector_start_with_socket -> bounded wait for
//   core.connector_network_config -> N5d: provision the SIGNAL socket
//   (parse netbird_config.signal → shell-side DNS → open+protect →
//   core.connector_signal_socket_feed; any failure = connector stopped,
//   NO VPN created) -> N2-H endpoint freeze: resolve the real outer
//   endpoints (management / signal / relay urls from connector_status) and
//   derive the host /32|/128 isExcludedRoute set BEFORE create()
//   (relay-enabled + unresolved endpoint = fail-closed, no VPN) — N2-H I3a:
//   every create() is GENERATION-STAMPED (one VPN_EXCLUSION_GENERATION line
//   before it, `gen=` on every VPN_ROUTE_ENTRY row, and an acceptance anchor
//   after it: VPN_CREATE_RESOLVED for the initial leg, VPN_RECREATE_RESOLVED
//   for a recreate leg; the arm reports the last accepted generation) -> merge
//   address/routes/DNS into VpnConfig under the
//   default-route safety gate (0.0.0.0/0 held unless the core gate allows /
//   forceDefaultRoute opt-in) -> connection.create() -> fd contract +
//   fail-closed protect gate for the WG outer socket -> N7: FEED THE WG
//   DATA PLANE (protected outer UDP socket via core.connector_wg_socket_feed
//   + TUN fd via core.connector_tun_fd_feed; any failure = connector
//   stopped + VPN destroyed — no data plane, no default route) -> N8: ACK
//   the applied route set (core.connector_route_set_applied,
//   isRecreate=false). The map snapshot is applied at create() time
//   (platform VpnConfig is fixed then); a LATER desired-route-set change
//   (e.g. the gate releasing 0.0.0.0/0 once the WG handshake completes —
//   necessarily AFTER create()) is healed by the N8 CONTROLLED RECREATE:
//   the core raises status.recreate.required (bounded: budget 3 + 30 s
//   cooldown), the shell destroys the connection, create()s again with the
//   desired route set, feeds the NEW TUN fd (core-side replace-while-up
//   keeps the WG sessions and the protected outer socket) and ACKs with
//   isRecreate=true. Any recreate failure is fail-closed: connector
//   stopped + VPN destroyed — never a half-configured tunnel.
// - Protect: the management socket is protected BEFORE the connector's first
//   dial (reconnects re-protect via core-fed sockets), and after create()
//   yields a valid TUN fd, the native outer UDP socket (core.wg_fwd_open) is
//   protected via VpnConnection.protect() BEFORE any datagram flows; protect
//   is fail-closed (governance docs/native-nx-governance.md item 4): any
//   open/bind/protect failure or the 5 s box expiring aborts the flow /
//   destroys the VPN and never starts traffic.
// - Stop flow: onDestroy -> core.connector_stop (idempotent) -> exactly-
//   once destroy + fd close re-check (fd contract unchanged). N3-7: a
//   SELF-terminated connector (status.terminal) or repeated status-poll
//   failures trigger the same teardown from the runtime watcher.
export default class NetBirdVpnExtensionAbility extends VpnExtensionAbility {
  private requestId: string = 'missing';
  private connectionCreationAttempted: boolean = false;
  private connection?: vpnExtension.VpnConnection;
  private createPromise?: Promise<CreateOutcome>;
  private createSettled: boolean = false;
  private destroyPromise?: Promise<void>;
  private destroyTrigger: string = 'none';
  // Raw TUN fd. Invariant: the ONLY owner of its close is
  // connection.destroy(); native consumes it ONLY as a dup copy via the N7
  // core.connector_tun_fd_feed (TunFd fd contract — the raw number itself is
  // never stored nor closed by native).
  private tunFd: number = -1;
  private fdConfirmedOpenAfterCreate: boolean = false;
  // Shell-owned current WG UDP raw fd; the device owns a separate duplicate.
  private outboundSocketFd: number = -1;
  private outboundProtected: boolean = false;
  // ICE UDP sockets are protected before the native queue duplicates them.
  private iceProtected: boolean = false;
  private iceTopUpTimer: number = -1;
  private socketSupplyTimer: number = -1;
  private networkObserver: connection.NetConnection | undefined = undefined;
  private networkDebounceTimer: number = -1;
  private networkEpoch: number = 0;
  private networkNetId: number = 0;
  private networkDirty: boolean = false;
  private networkRecoveryActive: boolean = false;
  private networkRecoveryReady: boolean = false;
  private managementEndpoint: ManagementEndpoint | undefined = undefined;
  private signalUri: string = '';
  // Depth reported by the last accepted feed — the top-up uses it to tell a
  // draining core (keep feeding) from an idle one (stop piling up).
  private iceQueueDepth: number = 0;
  private iceTopUpActive: boolean = false;
  // Process-level protect (protectProcessNet, API 22+) — diagnostic attempt
  // once per session; its result does not establish per-socket bypass semantics.
  private processProtectAttempted: boolean = false;
  private processProtected: boolean = false;
  // DEV-ONLY: when armed, skip per-socket protect and record a route-exclusion
  // deviation. The normal path protects each outer socket before core handoff.
  private protectDeviationRouteExclusion: boolean = false;
  // N3-6 control-plane wiring state. destroyRequested latches onDestroy so
  // the (async) start flow never creates a VPN after teardown began.
  private destroyRequested: boolean = false;
  private watcherTimer: number = -1;
  // Serial of the network-config snapshot merged into the platform
  // VpnConfig at create() time (-1 = none applied).
  private appliedNetSerial: number = -1;
  private lastUnappliedNetSerial: number = -1;
  private lastUnappliedNetLogAtMs: number = 0;
  private lastStatusLogSignature: string = '';
  private lastStatusLogAtMs: number = 0;
  // N3-7 gap 3: consecutive connector_status() native-call failures and the
  // one-shot latch for the connector-death teardown (idempotent on top of
  // the destroyOnce latch).
  private statusPollFailures: number = 0;
  private connectorDeathHandled: boolean = false;
  // N8: route networks destined for the initial-create ACK (the managed
  // subset actually merged into the platform VpnConfig; core-visible form =
  // canonical 'a.b.c.d/p' strings exactly as connector_network_config
  // renders them). Stashed before create(), consumed in settleCreate.
  private pendingAppliedRoutes: string[] | null = null;
  // N8: one controlled recreate at a time (the watcher ticks faster than a
  // recreate completes; this is the shell-side serialization on top of the
  // core-side budget/cooldown bounds).
  private recreateInFlight: boolean = false;
  private networkRecoveryPending: boolean = false;
  // N2-H: the endpoint-derived exclusion set frozen at THIS session's start
  // (before the first create()). The controlled recreate grafts the SAME
  // frozen set onto its rebuilt config — a recreate that dropped the
  // exclusions would suck the outer endpoints into the fresh (often
  // default-routed) tunnel. A fresh freeze (re-resolution) happens on the
  // next full start, never from a cached cross-session set.
  private frozenEndpointExclusions: ExcludedRouteEntry[] = [];
  private endpointExclusionsEnabled: boolean = true;
  // N2-H I5 (§5.3-2): the SHELL exclusion set retention for the device-side
  // evidence collector — the same derivation's endpoints (kind/host/port) and
  // per-endpoint resolutions, kept on this instance so `n2h_collector_arm` can
  // freeze the shell-side half of C1/C2 in the SAME document. Development
  // channel only: nothing is read from them unless the arm control file
  // exists (§6.1), and the arm action itself is never gate evidence.
  private n2hExclusionEndpoints: ExclusionEndpoint[] = [];
  private n2hExclusionResolutions: EndpointResolution[] = [];
  // N2-H v3.1 C1 dns (R-4): the branch record of THIS session's dns
  // derivation (branch token + verbatim physical-attribution grounds + the
  // gap the shell recorded, if any). It is the shell half of the `notes[]`
  // lines the evidence document must carry, so the arm hands it to the core
  // verbatim; nothing is read from it unless the arm control file exists.
  private n2hDnsReport: DnsDerivationResult | null = null;
  // N2-H I5 (§5.3-1): the one-shot latch of the control-file check, so the
  // watcher attempts the arm at most once per ability instance.
  private n2hArmAttempted: boolean = false;
  // 2026-09-18 timing fix: log-dedup ONLY — the ticket-present-but-session-
  // not-ready WAITING line prints once per instance instead of every watcher
  // tick. It never gates anything: the arm attempt stays owned by
  // n2hArmAttempted alone.
  private n2hArmWaitLogged: boolean = false;
  // N2-H iface-bytes WINDOW (v4 preregistration §4 (ii) C7 same-source
  // calibration prerequisite, 2026-09-20): the two-edge latch pair.
  // n2hIfaceWindowOpenSeen flips ONLY from the probe's async open-edge hook —
  // after the phase=open line is emitted, i.e. after this tick's synchronous
  // body has fully unwound — so the close edge can never fire on the arming
  // tick itself. n2hIfaceWindowCloseDone consumes the one-shot close read on
  // the first later watcher trigger, before any early return (the very top of
  // the watcher body). Diagnostic-only (C7 counting reconciliation): never
  // Evidence.counters/notes, never the judge, never path/isolation evidence;
  // neither latch touches the arm/WAITING semantics above.
  private n2hIfaceWindowOpenSeen: boolean = false;
  private n2hIfaceWindowCloseDone: boolean = false;
  // S5 iface-bytes BURST (2026-09-26, default OFF): the handle of the bounded
  // 500 ms survey interval and its monotone tick index. -1 = no interval
  // running; the tick body refuses to emit anything once the handle is back to
  // -1, so a cleared timer cannot print even if a stray callback survives the
  // cancellation. Both are written ONLY by startInterfaceStatsBurst /
  // interfaceStatsBurstTick / stopInterfaceStatsBurst — nothing else, and
  // nothing about them reaches a gate or Evidence.
  private n2hIfaceBurstTimer: number = -1;
  private n2hIfaceBurstSeq: number = 0;
  // N2-H I3a (v3.1 §2.1 C2 (1)/(2)): the generation counter of THIS ability
  // instance. requestId is fixed for the instance's whole lifetime (onCreate
  // runs before any create()), so a per-instance counter IS the frozen
  // per-requestId sequence: the initial create() is generation 0, every
  // recreate leg +1, never reused, never decremented. A leg that aborts
  // before its create() has already announced its generation (the
  // announcement precedes the route-row dump so the rows always have their
  // header); the number the ARM reports is a different, stricter value — the
  // last ACCEPTED one below.
  private n2hExclusionGeneration: number = -1;
  // N2-H I3a (v3.1 §2.1 C2 (4) / R-2③): the ONE source of "the evidence
  // generation G" — the gen whose create() was ACCEPTED (initial: the
  // VPN_CREATE_RESOLVED|accepted=true anchor; recreate: the
  // VPN_RECREATE_RESOLVED|accepted=true anchor). The arm reads this single
  // field twice (the `exclusion_generation` arm option the core echoes into
  // `collector.exclusion_generation` and the `gen=` field of the ARMED
  // marker), so those two and the generation line cannot disagree. -1 = no
  // accepted create() yet — unreachable on the arm path (device_up/ready are
  // only reached after an accepted create); it is forwarded as "no
  // generation", never fabricated into a value.
  private n2hAcceptedExclusionGeneration: number = -1;
  // S2-S5 diagnostic switches (see the constants above). Read ONCE per start
  // flow from the arm ticket (readDiagSwitches) and defaulted to false here,
  // so an instance that never read a ticket — or read an absent/unreadable/
  // malformed one — is inert in every path below. Nothing else writes them.
  // The tunnel-subnet route is NOT one of them: it is unconditional.
  private diagBlockedSelf: boolean = false;
  private diagProtectProcessNet: boolean = false;
  private diagPosctlUnbound: boolean = false;
  private diagIfaceBytesBurst: boolean = false;
  // S6 capture route (2026-09-26, default OFF): the switch itself plus the
  // target it was given, both read ONCE per start flow from the SAME arm ticket
  // (readDiagSwitches) — the target from the ticket's own `protect_probe.target`
  // string, never from the want. '' means "no usable target was carried": with
  // the switch on that is a logged skip, never a guessed destination. Nothing
  // else writes them, and with the switch off the config path never reads them.
  private diagCaptureRoute: boolean = false;
  private diagCaptureTarget: string = '';
  private n2Evidence?: N2EvidenceOptions;
  private n2EvidenceError: string = '';
  private n2ForceDefaultRoute: boolean = false;

  sendN2PositiveControl(target: string, protocol: N2ControlProtocol, id: string): void {
    sendN2Control(this.n2Evidence, target, protocol, id);
  }

  onCreate(want: Want): void {
    const wantRequestId: string = String(want.parameters?.['requestId'] ?? '');
    if (wantRequestId.length > 0 && wantRequestId.length <= 96) {
      this.requestId = clean(wantRequestId);
    } else {
      this.requestId = 'missing';
    }
    hilog.info(DOMAIN, TAG, 'VPN_ONCREATE|bundle=%{public}s|requestId=%{public}s',
      want.bundleName ?? 'unknown', this.requestId);
    // N2-H I5b (§13.16): the extension's own "this process reached the
    // earliest reachable lifecycle point" beacon, printed AFTER requestId is
    // resolved (so the id is real) and BEFORE every early return below. It is
    // the only thing that separates "the extension process never started" from
    // "it started but the watcher never ticked" — both were zero-row states in
    // the 2026-09-18 device dry-run. One line per onCreate call: no latch, no
    // counter, no gate touched, nothing but the fixed development-channel path
    // marker.
    hilog.info(DOMAIN, TAG,
      'N2H_COLLECTOR_PATH|requestId=%{public}s|stage=onCreate-reached|diag=true|is_evidence=false',
      this.requestId);

    if (this.connectionCreationAttempted) {
      hilog.error(DOMAIN, TAG,
        'VPN_CREATE_REJECTED|requestId=%{public}s|phase=duplicate-onCreate|summary=connection-already-attempted',
        this.requestId);
      return;
    }
    this.connectionCreationAttempted = true;

    let connection: vpnExtension.VpnConnection;
    try {
      connection = vpnExtension.createVpnConnection(this.context);
      this.connection = connection;
      hilog.info(DOMAIN, TAG, 'VPN_CONNECTION_CREATED|requestId=%{public}s', this.requestId);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_CREATE_REJECTED|requestId=%{public}s|phase=create-connection|summary=%{public}s',
        this.requestId, safeError(error as Error));
      return;
    }

    // N3-6 start flow (async, bounded waits only): read credentials from
    // the app-private dir -> connector_start -> bounded wait for the
    // network map -> merge address/routes/DNS into the platform VpnConfig
    // -> connection.create() -> fd contract + fail-closed protect gate.
    // The map is applied at create() time because the platform VpnConfig is
    // fixed then; later map changes are logged as NOT applied.
    this.networkNetId = physicalDefaultNetId();
    hilog.info(DOMAIN, TAG, 'VPN_NETWORK_BASELINE|requestId=%{public}s|netId=%{public}d',
      this.requestId, this.networkNetId);
    this.watchPhysicalNetwork();
    this.startConnectorThenCreate(want, connection).catch((error: Error) => {
      hilog.error(DOMAIN, TAG,
        'VPN_START_FLOW_REJECTED|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error));
    });
  }

  private watchPhysicalNetwork(): void {
    try {
      const observer: connection.NetConnection = connection.createNetConnection();
      observer.register((error: Error | undefined): void => {
        if (error !== undefined) {
          hilog.warn(DOMAIN, TAG, 'VPN_NETWORK_WATCH_FAIL|summary=%{public}s', safeError(error));
        }
      });
      observer.on('netLost', (net: connection.NetHandle): void => {
        this.schedulePhysicalNetworkCheck('netLost', net.netId);
      });
      observer.on('netAvailable', (net: connection.NetHandle): void => {
        this.schedulePhysicalNetworkCheck('netAvailable', net.netId);
      });
      observer.on('netCapabilitiesChange', (info: connection.NetCapabilityInfo): void => {
        this.schedulePhysicalNetworkCheck('netCapabilitiesChange', info.netHandle.netId);
      });
      this.networkObserver = observer;
    } catch (error) {
      hilog.warn(DOMAIN, TAG, 'VPN_NETWORK_WATCH_FAIL|summary=%{public}s', safeError(error as Error));
    }
  }

  private schedulePhysicalNetworkCheck(event: string, eventNetId: number): void {
    const physicalNetId: number = physicalDefaultNetId();
    hilog.info(DOMAIN, TAG,
      'VPN_NETWORK_EVENT|event=%{public}s|netId=%{public}d|physicalDefault=%{public}s|' +
      'physicalNetId=%{public}d|switched=pending|action=debounce',
      event, eventNetId, String(eventNetId === physicalNetId && physicalNetId !== 0), physicalNetId);
    if (this.networkDebounceTimer !== -1) { clearTimeout(this.networkDebounceTimer); }
    this.networkDebounceTimer = setTimeout((): void => {
      this.networkDebounceTimer = -1;
      this.physicalNetworkAvailable(event, eventNetId, physicalDefaultNetId());
    }, 250);
  }

  private physicalNetworkAvailable(event: string, eventNetId: number, netId: number): void {
    const switched: boolean = !this.destroyRequested &&
      isPhysicalNetworkSwitch(this.networkNetId, netId);
    const action: string = this.destroyRequested ? 'destroy' : switched ?
      (netId === 0 ? 'network-lost' : 'redial') : 'unchanged';
    hilog.info(DOMAIN, TAG,
      'VPN_NETWORK_EVENT|event=%{public}s|netId=%{public}d|physicalDefault=%{public}s|' +
      'physicalNetId=%{public}d|switched=%{public}s|action=%{public}s',
      event, eventNetId, String(eventNetId === netId && netId !== 0), netId,
      String(switched), action);
    if (!switched) { return; }
    this.networkNetId = netId;
    this.networkDirty = true;
    this.networkEpoch++;
    connectorPhysicalNetworkChanged('');
    if (netId !== 0) {
      this.recoverPhysicalNetwork().catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG, 'VPN_NETWORK_RECOVERY_FAIL|summary=%{public}s', safeError(error));
      });
    }
  }

  private async recoverPhysicalNetwork(): Promise<void> {
    if (!this.networkRecoveryReady || this.destroyRequested) { return; }
    if (this.networkRecoveryActive) {
      this.networkRecoveryPending = true;
      return;
    }
    this.networkRecoveryActive = true;
    let attemptEpoch: number = 0;
    try {
      do {
        attemptEpoch = this.networkEpoch;
        const epoch: number = attemptEpoch;
        const netId: number = this.networkNetId;
        const vpn: vpnExtension.VpnConnection | undefined = this.connection;
        if (vpn === undefined || this.recreateInFlight || netId === 0 || this.managementEndpoint === undefined) { break; }
        forgetPhysicalDefaultDns();
        const dns: string[] = physicalDefaultDnsServers(netId);
        if (dns.length === 0 || !socketSupplyDnsServers(dns).ok) { break; }
        const managementIp: string = await resolveProtectedIp(this.managementEndpoint.host);
        if (managementIp.length === 0) { break; }
        const addr: string = `${managementIp}:${this.managementEndpoint.port}`;
        if (epoch !== this.networkEpoch || this.destroyRequested || this.recreateInFlight ||
          this.connection !== vpn) { continue; }
        const opened: NativeSocketOpen = JSON.parse(core.wg_fwd_open()) as NativeSocketOpen;
        if (opened.fd < 0 || opened.bind_rc !== 0) {
          if (opened.fd >= 0) { socketSupplyCloseFd(opened.fd); }
          break;
        }
        let adopted: boolean = false;
        try {
          if (!await this.protectSocketWithBox(vpn, opened.fd, 'Wb')) { break; }
          if (epoch !== this.networkEpoch || this.destroyRequested || this.recreateInFlight ||
            this.connection !== vpn) { continue; }
          const fed: WgFeedResult = connectorWgSocketFeed(opened.fd);
          if (!fed.ok || fed.device_up !== true) { break; }
          adopted = true;
          const oldFd: number = this.outboundSocketFd;
          this.outboundSocketFd = -1;
          this.outboundProtected = true;
          socketSupplyCloseFd(opened.fd);
          if (oldFd >= 0 && oldFd !== opened.fd) { socketSupplyCloseFd(oldFd); }
        } finally {
          if (!adopted) { socketSupplyCloseFd(opened.fd); }
        }
        const restarted: SocketFeedResult = connectorManagementAddressUpdate(addr);
        if (!restarted.ok) { break; }
        const updated: SocketFeedResult = socketSupplyDnsServers(dns);
        if (!updated.ok) { break; }
        this.iceQueueDepth = 0;
        if (this.iceTopUpTimer !== -1) {
          clearInterval(this.iceTopUpTimer);
          this.iceTopUpTimer = -1;
        }
        if (!await this.protectAndFeedIceSocket()) { break; }
        if (epoch !== this.networkEpoch || this.destroyRequested || this.recreateInFlight ||
          this.connection !== vpn) { continue; }
        if (this.signalUri.length > 0) {
          const endpoint: ManagementEndpoint = parseSignalEndpoint(this.signalUri, 443);
          const signalIp: string = await resolveProtectedIp(endpoint.host);
          if (signalIp.length === 0 || epoch !== this.networkEpoch) { break; }
          const signalAddr: string = `${signalIp}:${endpoint.port}`;
          const sock: NativeTcpSocketOpen = mgmtSocketOpen();
          if (sock.fd < 0 || sock.bind_rc !== 0) {
            if (sock.fd >= 0) { socketSupplyCloseFd(sock.fd); }
            break;
          }
          let signalFed: boolean = false;
          try {
            if (await this.protectSocketWithBox(vpn, sock.fd, 'S') &&
              epoch === this.networkEpoch && !this.destroyRequested && !this.recreateInFlight &&
              this.connection === vpn) {
              signalFed = connectorSignalFeed(sock.fd, signalAddr).ok;
            }
          } finally { socketSupplyCloseFd(sock.fd); }
          if (!signalFed) { break; }
        }
        if (!await this.feedSpareProtectedSocket(vpn, epoch)) { break; }
        if (epoch !== this.networkEpoch || this.destroyRequested || this.recreateInFlight ||
          this.connection !== vpn) { continue; }
        this.networkDirty = false;
        hilog.info(DOMAIN, TAG, 'VPN_NETWORK_RECOVERED|requestId=%{public}s|netId=%{public}d',
          this.requestId, netId);
      } while (attemptEpoch !== this.networkEpoch && !this.destroyRequested && this.networkNetId !== 0);
    } finally {
      this.networkRecoveryActive = false;
      const retry: boolean = this.networkRecoveryPending;
      this.networkRecoveryPending = false;
      if (retry && this.networkDirty && !this.recreateInFlight && !this.destroyRequested) {
        this.recoverPhysicalNetwork().catch((error: Error): void => {
          hilog.warn(DOMAIN, TAG, 'VPN_NETWORK_RECOVERY_FAIL|summary=%{public}s', safeError(error));
        });
      }
    }
  }

  // N3-6/N3-7 start flow. Abort paths leave the VPN un-created and report a
  // stable error class. N3-7 pre-flight (fail-closed, governance §二.4 —
  // per-socket protect BEFORE the first connect AND before every reconnect;
  // upstream ControlProtectSocket semantics, protectsocket_android.go:22-46):
  //   resolve management host shell-side (DNS) → open a TCP socket
  //   (core.mgmt_socket_open, unconnected) → VpnConnection.protect(fd) →
  //   ONLY THEN core.connector_start_with_socket(fd, …, ip:port).
  // DNS failure / open failure / protect failure / start refusal ⇒ the
  // connector never starts and no VPN is created.
  private async startConnectorThenCreate(want: Want, connection: vpnExtension.VpnConnection):
    Promise<void> {
    const filesDir: string = this.context.filesDir;
    const setupKeyFile: string =
      String(want.parameters?.['setupKeyFile'] ?? DEFAULT_SETUP_KEY_FILE);
    const deviceFile: string =
      String(want.parameters?.['deviceConfigFile'] ?? DEFAULT_DEVICE_CONFIG_FILE);
    // N3-7 DEVELOPMENT opt-in (default false): force the managed default
    // route into VpnConfig even while the data plane is not ready. Risk:
    // traffic black hole until a peer tunnel actually works.
    const forceDefaultRoute: boolean =
      String(want.parameters?.['forceDefaultRoute'] ?? 'false') === 'true';
    // DEV-ONLY DEVIATION (user-approved 2026-09-14): when armed, skip
    // protect(fd) and rely on endpoint route exclusion. Device observations
    // since then show protect(fd) resolves; MANAGE_VPN log lines do not prove
    // it is blocked. This deviation is not a gate pass; the default path
    // remains fail-closed.
    // Handed over through filesDir by EntryAbility (see its
    // PROTECT_DEVIATION_FILE): Index.buildWant() builds the extension want
    // itself and does NOT forward the launcher's --ps parameters, so reading
    // the want here silently never armed the flag (device-verified
    // 2026-09-14). The file is the same development-time channel the
    // credentials already use.
    this.protectDeviationRouteExclusion = false;
    try {
      const deviation: string =
        readCredentialFileText(this.context.filesDir, 'netbird-protect-deviation');
      this.protectDeviationRouteExclusion = deviation.trim() === 'route-exclusion';
    } catch (error) {
      this.protectDeviationRouteExclusion = false;
    }
    if (this.protectDeviationRouteExclusion) {
      hilog.warn(DOMAIN, TAG,
        'VPN_PROTECT_DEVIATION_ARMED|requestId=%{public}s|mode=route-exclusion|' +
        'reason=operator-opt-in|is_evidence=false|not_a_gate_pass=true',
        this.requestId);
    }

    // S2-S4 (default off): the diagnostic switches of the SAME arm ticket must
    // be final BEFORE any VpnConfig is built in this flow, so they are read
    // here — early, on the SAME file the arm hook reads later — and never from
    // the want (Index.buildWant() does not forward launcher parameters). The
    // arm hook's own options read, the keys it forwards to the core and every
    // pre-existing path stay untouched; these three keys never leave this
    // process. A missing ticket is the normal case: everything stays off and
    // NOTHING is logged.
    this.n2ForceDefaultRoute = forceDefaultRoute;
    this.readDiagSwitches();
    if ((this.n2EvidenceError ?? '').length > 0) {
      hilog.error(DOMAIN, TAG,
        'N2_EVIDENCE_REFUSED|requestId=%{public}s|reason=%{public}s|diag=true|is_evidence=false',
        this.requestId, this.n2EvidenceError);
      return;
    }
    if (this.n2Evidence !== undefined) {
      this.probeN2ProcNet();
      hilog.warn(DOMAIN, TAG,
        'N2_EVIDENCE_ARMED|requestId=%{public}s|trusted=%{public}s|capture32=%{public}s|' +
        'endpointExclusions=false|hdcControl=%{public}s|allowHdcOverlap=%{public}s|' +
        'diag=true|is_evidence=false',
        this.requestId, SELF_BUNDLE_NAME, this.n2Evidence.capture32.join(','),
        this.n2Evidence.hdcControlIp || 'not-supplied', String(this.n2Evidence.allowHdcOverlap));
    }
    if (!this.endpointExclusionsEnabled &&
      (this.protectDeviationRouteExclusion || this.diagCaptureRoute)) {
      hilog.error(DOMAIN, TAG, 'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=dev-switch-conflict',
        this.requestId);
      return;
    }

    let setupKey: string;
    let device: DeviceConfigFile;
    try {
      // DEVELOPMENT-TIME credential source, NOT secure storage (module doc
      // + docs/n3-shell-integration-notes.md): filesDir files, with the
      // development-only /data/local/tmp same-name override (device
      // validation aid — hdc cannot write into the app sandbox). Missing
      // in BOTH locations still aborts here: no credentials, no VPN.
      setupKey = readCredentialFileText(filesDir, setupKeyFile);
      device = parseDeviceConfig(readCredentialFileText(filesDir, deviceFile));
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=credentials|' +
        'errorClass=credentials-missing|summary=%{public}s',
        this.requestId, safeError(error as Error));
      return;
    }

    if (this.destroyRequested) {
      return;
    }

    // Packaged public CA roots: the core reads NO system trust store — for
    // https:// it REQUIRES caller-injected ca_pem (grpc.rs:63-66;
    // connector.rs:683 missing-ca-for-https = config error). An https
    // management deployment WITHOUT an explicit device ca_pem therefore gets
    // the Let's Encrypt anchors shipped in the HAP rawfile; an explicit
    // device.ca_pem (private CA / self-hosted) keeps priority, and http://
    // deployments are completely untouched (no new logic runs for them).
    // Fail-closed: a missing/unreadable/empty rawfile aborts the start like
    // missing credentials — never an https start with an empty root set.
    if (device.management_url.startsWith('https://')) {
      if (device.ca_pem !== undefined && device.ca_pem.length > 0) {
        hilog.info(DOMAIN, TAG,
          'VPN_CA_ROOTS_SOURCE|requestId=%{public}s|source=device-config|note=explicit-ca-pem-wins',
          this.requestId);
      } else {
        try {
          device.ca_pem = readPackagedCaRootsPem(this.context.resourceManager);
          hilog.info(DOMAIN, TAG,
            'VPN_CA_ROOTS_SOURCE|requestId=%{public}s|source=rawfile|file=%{public}s|' +
            'bytes=%{public}d|certs=%{public}d',
            this.requestId, PACKAGED_CA_ROOTS_RAWFILE, device.ca_pem.length,
            countPemCertificates(device.ca_pem));
        } catch (error) {
          hilog.error(DOMAIN, TAG,
            'VPN_CA_ROOTS_FAIL|requestId=%{public}s|stage=rawfile|file=%{public}s|' +
            'errorClass=https-ca-roots-missing|summary=%{public}s',
            this.requestId, PACKAGED_CA_ROOTS_RAWFILE, safeError(error as Error));
          return;
        }
      }
    }

    // 1. shell-side endpoint parse + DNS resolution (never native).
    // N2-H: the parsed endpoint + resolved address survive this block — they
    // are the management half of the exclusion derivation (the IP the
    // protected management socket actually dials).
    let mgmtEndpoint: ManagementEndpoint = { host: '', port: 0, ip: '' };
    let connectAddr: string = '';
    try {
      mgmtEndpoint = parseManagementEndpoint(device.management_url);
      connectAddr = await resolveManagementIp(mgmtEndpoint, DNS_RESOLVE_BOX_MS);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=dns|' +
        'errorClass=%{public}s|summary=%{public}s',
        this.requestId, (error as Error).message ?? 'dns-failed',
        safeError(error as Error));
      return;
    }

    // 2. open the management TCP socket (unconnected — no packet flows yet)
    let mgmtFd: number = -1;
    try {
      const opened = mgmtSocketOpen();
      mgmtFd = opened.fd;
      if (mgmtFd < 0 || opened.bind_rc !== 0) {
        if (mgmtFd >= 0) { socketSupplyCloseFd(mgmtFd); }
        hilog.error(DOMAIN, TAG,
          'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=mgmt-socket-open|' +
          'errorClass=mgmt-socket-open-failed|fd=%{public}d|bindRc=%{public}d|bindErrno=%{public}d',
          this.requestId, mgmtFd, opened.bind_rc, opened.bind_errno);
        return;
      }
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=mgmt-socket-open-throw|' +
        'errorClass=mgmt-socket-open-failed|summary=%{public}s',
        this.requestId, safeError(error as Error));
      return;
    }

    // 3. protect the management socket BEFORE any packet can flow (the
    // connect itself happens later, on a native dup, only after this gate).
    // TODO(未验证): protect() on a VpnConnection before create() is the
    // required shape here; the on-device behavior is physical-gate work
    // (N2b) — this box treats any failure/timeout as fail-closed abort.
    const mgmtProtected: boolean = await this.protectSocketWithBox(connection, mgmtFd, 'M');
    if (!mgmtProtected) {
      socketSupplyCloseFd(mgmtFd);
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=mgmt-protect|' +
        'errorClass=protect-fail-closed|fd=%{public}d',
        this.requestId, mgmtFd);
      return;
    }

    // 4. start the connector over the protected socket (Rust dup-consumes
    // the fd per dial; the original number stays shell-owned — fd contract)
    const creds: Record<string, Object> = { 'setup_key': setupKey };
    let start: ConnectorStartResult;
    try {
      start = connectorStartWithSocket(mgmtFd, buildConnectorConfigJson(device, forceDefaultRoute),
        JSON.stringify(creds), connectAddr, physicalDefaultDnsServers());
    } finally {
      socketSupplyCloseFd(mgmtFd);
    }
    if (!start.started) {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=start|errorClass=%{public}s',
        this.requestId, start.error ?? 'start-failed');
      return;
    }
    if (this.n2Evidence !== undefined) {
      const result: N2TunDestConfigureResult = JSON.parse(core.n2_tun_dest_configure(
        JSON.stringify({ n2_capture_32: this.n2Evidence.capture32,
          n2_outer_destinations: this.n2Evidence.outerDestinations }))) as N2TunDestConfigureResult;
      if (!result.ok) {
        hilog.error(DOMAIN, TAG,
          'N2_TUN_DEST_CONFIG_FAILED|requestId=%{public}s|error=%{public}s|diag=true|is_evidence=false',
          this.requestId, result.error ?? 'unknown');
        connectorStop();
        return;
      }
    }
    if (forceDefaultRoute) {
      hilog.warn(DOMAIN, TAG,
        'VPN_DEFAULT_ROUTE_FORCED|requestId=%{public}s|' +
        'warning=data-plane-not-ready-black-hole-risk|optIn=forceDefaultRoute',
        this.requestId);
    }
    this.managementEndpoint = mgmtEndpoint;
    // seed ONE spare protected socket so a single reconnect dial can
    // re-protect; further resupply policy is an N4+ increment (a starved
    // queue fails dials CLOSED — never unprotected)
    this.startSocketSupply();
    this.feedSpareProtectedSocket(connection);
    hilog.info(DOMAIN, TAG, 'VPN_CONNECTOR_STARTED|requestId=%{public}s|state=%{public}s|' +
      'mgmtSocket=protected|connectAddr=%{public}s',
      this.requestId, start.state ?? 'unknown', connectAddr);

    const net: CoreNetworkConfig | null = await this.waitNetworkConfigBounded();
    if (this.destroyRequested) {
      connectorStop();
      hilog.info(DOMAIN, TAG,
        'VPN_CONNECTOR_STOPPED|requestId=%{public}s|alreadyStopped=false|state=closed|' +
        'note=destroy-during-network-map-wait', this.requestId);
      return;
    }

    // N5d signal provisioning (fail-closed, governance §二.4 — signal is an
    // enumerated protect socket): the core's real signal link dials ONLY
    // over shell-protected sockets. The signal URI rides the network-config
    // snapshot; resolve it shell-side, open + protect a TCP socket, feed it
    // (with the resolved address) to the core. ANY failure is fail-closed:
    // the connector is stopped and the VPN is NOT created — without signal
    // there is no offer/answer/candidate path, hence no peer connectivity.
    // N2-H: on success the parsed endpoint + dialed IP are kept for the
    // exclusion derivation below.
    let signalProvisioned: SignalProvisionResult | null = null;
    if (net !== null && net.signal !== undefined && net.signal !== null && net.signal.length > 0) {
      signalProvisioned = await this.provisionSignalSocket(connection, net.signal);
      if (signalProvisioned !== null) { this.signalUri = net.signal; }
      if (signalProvisioned === null) {
        hilog.error(DOMAIN, TAG,
          'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=signal-provision|' +
          'errorClass=signal-provision-fail-closed|action=connector-stopped-no-vpn',
          this.requestId);
        connectorStop();
        return;
      }
    } else {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=signal-uri|' +
        'errorClass=%{public}s|action=connector-stopped-no-vpn',
        this.requestId, net === null ? 'no-network-map' : 'no-signal-uri-in-config');
      connectorStop();
      return;
    }

    // N2-H outer-endpoint freeze (docs/native-nx-governance.md 追加登记;
    // docs/n13-relay-increment-plan-20260914.md §3.4/§5): resolve the REAL
    // outer endpoints (management / signal / relay) and derive the host
    // /32|/128 exclusion routes BEFORE create() — the platform VpnConfig
    // routes are fixed at create time, and this deployment leans on route
    // exclusion instead of the (unavailable) protect path. Relay-enabled
    // deployments fail CLOSED here if any endpoint cannot be frozen. The
    // relay hosts come from the management ADVERTISEMENT (core 95f6c02:
    // relay_advertised, exposed also while relay is disabled) — the N2-H
    // "exclude BEFORE enable" precondition: an advertised relay endpoint is
    // excluded even when relay_enabled=false (harmless and required), and
    // only an enabled relay turns its resolution failures fatal.
    const baseTemplate: VpnTunnelConfig = this.n2Evidence !== undefined
      ? withoutN2ExcludedRoutes(loadVpnTunnelConfig()) : loadVpnTunnelConfig();
    // The exclusion `address` field must be a BARE IP — NEVER 'IP:port'.
    // resolveManagementIp returns the 'ip:port' DIAL address (needed
    // verbatim by connectorStartWithSocket above); passing it through
    // unstripped made classifyAddress read it as an IPv6 literal (/128,
    // family=2) and the platform rejected the whole VpnConfig at
    // VpnConnection.create (create-401, 2026-09-15). 对照 signal 分支:
    // provisionSignalSocket freezes its dialIp with the same port-strip
    // (addr.slice(0, addr.lastIndexOf(':'))) — stripDialPort is the shared,
    // IPv6-safe implementation, and buildExcludedRouteEntries re-applies it
    // as the builder-side defense.
    const exclusion: EndpointExclusionOutcome = this.endpointExclusionsEnabled
      ? await this.deriveEndpointExclusions(mgmtEndpoint, stripDialPort(connectAddr),
        signalProvisioned, baseTemplate.tunnelAddress, net !== null ? net.stuns : null)
      : { fatal: false, routes: [] };
    if (!this.endpointExclusionsEnabled) {
      hilog.warn(DOMAIN, TAG,
        'VPN_ENDPOINT_EXCLUSIONS_DISABLED|requestId=%{public}s|mode=diagnostic|routes=0',
        this.requestId);
      this.n2hExclusionEndpoints = [];
      this.n2hExclusionResolutions = [];
    }
    if (exclusion.fatal) {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=endpoint-exclusion|' +
        'errorClass=endpoint-exclusion-fail-closed|action=connector-stopped-no-vpn|' +
        'reason=relay-enabled-unresolved-endpoint',
        this.requestId);
      connectorStop();
      return;
    }
    this.frozenEndpointExclusions = exclusion.routes;
    const base: VpnTunnelConfig = withEndpointExclusions(baseTemplate, exclusion.routes);
    let merged: VpnTunnelConfig = net !== null ? applyNetworkConfig(base, net) : base;
    // N3-7 default-route safety gate (shell half): hold 0.0.0.0/0 unless the
    // core gate allowed it (usable peers + WG tunnel ready) or the explicit
    // forceDefaultRoute opt-in was honored. The core already excludes a held
    // route from the snapshot `routes`; this pass also guards the base
    // fallback path (no network map) and the empty-routes fallback.
    let gateAllowed: boolean = forceDefaultRoute;
    let gateReason: string = 'default-route-held:shell-fallback-no-network-map';
    if (net !== null && net.default_route !== undefined && net.default_route !== null) {
      gateAllowed = net.default_route.allowed || forceDefaultRoute;
      gateReason = forceDefaultRoute && !net.default_route.allowed
        ? 'default-route-forced:debug-opt-in-black-hole-risk'
        : net.default_route.reason;
    }
    if (this.n2Evidence !== undefined) {
      gateAllowed = false;
      gateReason = 'n2-evidence-no-default-route';
    }
    merged = gateDefaultRoutes(merged, gateAllowed);
    // The tunnel subnet route is grafted AFTER the default-route gate (it is
    // never an isDefaultRoute entry, so the gate can never swallow it) and
    // BEFORE the generation marker / route rows / the fail-closed validation
    // gate below, so all three describe THIS list. UNCONDITIONAL since
    // 2026-09-26 (AUTH 0031 R0/R1): without this entry a packet sent to the
    // tunnel subnet from an unbound socket leaves over the physical network.
    merged = this.injectTunnelSubnetRoute(merged, 'initial');
    // S6 (2026-09-26, default OFF): the diagnostic capture /32, in the SAME
    // position as the tunnel-subnet entry (after the default-route gate,
    // before the generation marker / route rows / validation gate) and on
    // BOTH legs. Off: not called at all, so the submitted VpnConfig stays
    // byte-identical.
    if (this.diagCaptureRoute) {
      merged = this.injectCaptureRoute(merged, 'initial');
    }
    if (this.n2Evidence !== undefined) {
      try {
        merged = withN2EvidenceRoutes(merged, this.n2Evidence.capture32);
        if (merged.routes.some((route: VpnRouteConfig): boolean => route.isDefaultRoute)) {
          throw new Error('default-route-present');
        }
      } catch (error) {
        hilog.error(DOMAIN, TAG,
          'N2_EVIDENCE_REFUSED|requestId=%{public}s|reason=%{public}s|diag=true|is_evidence=false',
          this.requestId, safeError(error as Error));
        connectorStop();
        return;
      }
      hilog.info(DOMAIN, TAG,
        'N2_EVIDENCE_ROUTES|requestId=%{public}s|leg=initial|capture32=%{public}s|' +
          'defaultRoutes=0|excludedRoutes=0|diag=true|is_evidence=false',
        this.requestId, this.n2Evidence.capture32.join(','));
    }
    hilog.info(DOMAIN, TAG,
      'VPN_DEFAULT_ROUTE_GATE|requestId=%{public}s|allowed=%{public}s|reason=%{public}s|' +
      'defaultRoutesInConfig=%{public}d',
      this.requestId, String(gateAllowed), gateReason,
      merged.routes.filter((r: VpnRouteConfig): boolean => r.isDefaultRoute).length);
    if (net !== null) {
      this.appliedNetSerial = net.serial ?? -1;
      hilog.info(DOMAIN, TAG,
        'VPN_NETCFG_APPLIED|requestId=%{public}s|serial=%{public}s|address=%{public}s|' +
        'routes=%{public}d|dns=%{public}d|appliedAt=create',
        this.requestId, String(this.appliedNetSerial), merged.tunnelAddress,
        merged.routes.length, merged.dnsServers.length);
    } else {
      // map never arrived within the bounded box: fallback config, snapshot
      // explicitly NOT applied
      hilog.warn(DOMAIN, TAG,
        'VPN_NETCFG_NOT_APPLIED|requestId=%{public}s|reason=no-network-map-in-box|' +
        'boxMs=%{public}d|fallback=local-config', this.requestId, NETWORK_MAP_WAIT_MS);
    }
    this.logConfigApplied(merged);

    // H1 diagnosis (docs/vpn-create-401-diagnosis-20260915.md §3.2/§4/§5):
    // per-entry SDK-contract validation + the pre-create per-field truth
    // dump. The core snapshot reaches these entries through a bare
    // JSON.parse; a required field that arrived undefined/NaN must abort
    // BEFORE connection.create() — fail-closed, no repair, no silent drop.
    // N2-H I3a (v3.1 §2.1 C2 (1)/(2)/(4)): the generation announcement of
    // THIS create() comes first, then the rows it announces. `merged` is
    // final here (exclusions grafted, map applied, default route gated), so
    // the announced exclusions= / routes= are the counts actually submitted.
    // wgPeerEndpoints=0: the wg_peer outer-endpoint caliber is not derived
    // yet (its own increment) — the field is mandatory, the value is the
    // honest "none known" (and §2.1 (6) makes that a C2 failure while the
    // frozen set carries wg_peer, which is exactly the registered state).
    const gen: number =
      this.logExclusionGeneration(EXCLUSION_GENERATION_INITIAL, merged.routes, 0);
    this.logRouteTable(merged, gen);
    const routeCheck: RouteCheckResult = validateRouteEntries(merged.routes, merged.interfaceName);
    if (!routeCheck.ok) {
      this.logRouteViolations(routeCheck.violations);
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=route-validation|' +
        'errorClass=route-entry-invalid-fail-closed|action=connector-stopped-no-vpn|' +
        'violations=%{public}d',
        this.requestId, routeCheck.violations.length);
      connectorStop();
      return;
    }

    const config: vpnExtension.VpnConfig =
      buildPlatformVpnConfig(merged, this.diagBlockedApplicationList('initial'),
        this.n2Evidence !== undefined ? [SELF_BUNDLE_NAME] : undefined);
    // N8: stash the managed route set actually merged into the platform
    // VpnConfig, for the initial-create ACK once create() + the WG feed
    // succeed (settleCreate). Only a real core snapshot qualifies — the
    // base fallback shape is not the core's desired rendering.
    this.pendingAppliedRoutes =
      net !== null && net.routes !== undefined && net.routes !== null && net.routes.length > 0
        ? net.routes.map((r: CoreRouteEntry): string => r.network)
        : null;
    hilog.info(DOMAIN, TAG,
      'VPN_CREATE_BEGIN|requestId=%{public}s|address=%{public}s|prefixLength=%{public}d',
      this.requestId, merged.tunnelAddress, merged.tunnelPrefixLength);
    const rawCreate: Promise<number> = connection.create(config);
    this.createPromise = rawCreate.then((fd: number): Promise<CreateOutcome> => {
      return this.settleCreate(fd, gen);
    }, (error: Error): CreateOutcome => {
      this.createSettled = true;
      hilog.error(DOMAIN, TAG,
        'VPN_CREATE_REJECTED|requestId=%{public}s|phase=create|summary=%{public}s',
        this.requestId, safeError(error));
      return {
        destroyRequired: false,
        accepted: false,
        reason: 'create-rejected'
      };
    });
  }

  // Bounded wait for the first network map (NETWORK_MAP_WAIT_MS box,
  // NETWORK_MAP_POLL_MS steps). Returns null on box expiry — never throws.
  private async waitNetworkConfigBounded(): Promise<CoreNetworkConfig | null> {
    const deadline: number = Date.now() + NETWORK_MAP_WAIT_MS;
    while (Date.now() < deadline) {
      if (this.destroyRequested) {
        return null;
      }
      try {
        const net: CoreNetworkConfig = connectorNetworkConfig();
        if (net.available) {
          return net;
        }
      } catch (error) {
        hilog.warn(DOMAIN, TAG,
          'VPN_NETCFG_POLL_REJECTED|requestId=%{public}s|summary=%{public}s',
          this.requestId, safeError(error as Error));
      }
      await sleep(NETWORK_MAP_POLL_MS);
    }
    return null;
  }

  // N5d signal provisioning (parse → DNS → open → protect → feed). Every
  // step is fail-closed with a stable stage marker: parse/dns/open/protect/
  // feed failure or the protect box expiring returns null (the caller
  // stops the connector and never creates the VPN). On success the parsed
  // endpoint and the dialed IP are returned so the N2-H exclusion derivation
  // freezes exactly what this link dials (first-fed address wins core-side).
  // DNS stays OUT of native (same rule as the management dial); the FIRST
  // fed address wins for the link's lifetime (core-side contract), later
  // feeds only refill the fd queue — reconnect re-protect needs fresh feeds,
  // which the runtime watcher may add as an N5+ refinement.
  private async provisionSignalSocket(connection: vpnExtension.VpnConnection,
    signalUri: string): Promise<SignalProvisionResult | null> {
    // 1. parse (port default 443 — production signal is TLS; a bare uri
    //    inherits the management transport security level core-side)
    let endpoint: ManagementEndpoint;
    try {
      endpoint = parseSignalEndpoint(signalUri, 443);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_SIGNAL_PROVISION_FAIL|requestId=%{public}s|stage=parse|uri=%{public}s|' +
        'summary=%{public}s', this.requestId, signalUri, safeError(error as Error));
      return null;
    }
    // 2. DNS over the default network, bounded
    let addr: string;
    try {
      addr = await resolveManagementIp(endpoint, DNS_RESOLVE_BOX_MS);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_SIGNAL_PROVISION_FAIL|requestId=%{public}s|stage=dns|host=%{public}s|' +
        'summary=%{public}s', this.requestId, endpoint.host, safeError(error as Error));
      return null;
    }
    // 3. open (unconnected TCP) + protect BEFORE any packet can flow
    let fd: number = -1;
    try {
      const opened = mgmtSocketOpen();
      fd = opened.fd;
      if (fd < 0 || opened.bind_rc !== 0) {
        if (fd >= 0) { socketSupplyCloseFd(fd); }
        hilog.error(DOMAIN, TAG,
          'VPN_SIGNAL_PROVISION_FAIL|requestId=%{public}s|stage=open|fd=%{public}d|' +
          'bindRc=%{public}d', this.requestId, fd, opened.bind_rc);
        return null;
      }
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_SIGNAL_PROVISION_FAIL|requestId=%{public}s|stage=open-throw|summary=%{public}s',
        this.requestId, safeError(error as Error));
      return null;
    }
    try {
      const protectedOk: boolean = await this.protectSocketWithBox(connection, fd, 'S');
      if (!protectedOk || this.connection !== connection || this.recreateInFlight) {
        hilog.error(DOMAIN, TAG,
          'VPN_SIGNAL_PROVISION_FAIL|requestId=%{public}s|stage=protect|fd=%{public}d|' +
          'errorClass=protect-fail-closed', this.requestId, fd);
        return null;
      }
      const fed = connectorSignalFeed(fd, addr);
      if (!fed.ok) {
        hilog.error(DOMAIN, TAG,
          'VPN_SIGNAL_PROVISION_FAIL|requestId=%{public}s|stage=feed|fd=%{public}d|' +
          'error=%{public}s', this.requestId, fd, fed.error ?? 'no-message');
        return null;
      }
      hilog.info(DOMAIN, TAG,
        'VPN_SIGNAL_FED|requestId=%{public}s|fd=%{public}d|addr=%{public}s|queued=%{public}s',
        this.requestId, fd, addr, String(fed.queued ?? -1));
      return { endpoint: endpoint, dialIp: addr.slice(0, addr.lastIndexOf(':')) };
    } finally {
      socketSupplyCloseFd(fd);
    }
  }

  // N2-H endpoint-freeze derivation (docs/native-nx-governance.md 追加登记
  // 2026-09-14; docs/n13-relay-increment-plan-20260914.md §3.4/§5). Runs
  // BEFORE connection.create() — the platform VpnConfig routes are fixed at
  // create time, so the exclusion set must be final here.
  //
  // Endpoint sources (all EXISTING mechanisms — no new state channel):
  //   - management: parseManagementEndpoint(device.management_url) + the IP
  //     this flow's protected socket actually dials (resolved moments ago in
  //     the same start flow — never a cross-session cache);
  //   - signal: parseSignalEndpoint(net.signal) + the fed address (first-fed
  //     wins for the link lifetime core-side);
  //   - relay: the management ADVERTISEMENT
  //     connector_status().relay_advertised.advertised_urls (core 95f6c02 —
  //     exposed ALWAYS, also while relay_enabled=false: the N2-H "exclude
  //     BEFORE enable" source) UNION the runtime status.relay.urls
  //     (non-empty only while enabled; identical strings collapse),
  //     `rel://`/`rels://` parsed per the core's own RelayUrl::parse
  //     contract, then resolved FRESH at config-build time — DDNS (relay
  //     TTL 600) must never freeze a stale IP.
  // Additionally every endpoint host gets a FRESH build-time resolution so a
  // DDNS flip inside this start flow is covered too (dial IP + fresh IP both
  // frozen; pure dedup collapses matches).
  //
  // Severity (isExclusionFailClosed), kept strictly two-branch by the
  // unchanged relayRequired verdict ("relay is being ENABLED this session"):
  //   (a) relayRequired=true + any failed parse/resolution = fatal
  //       (fail-closed, no VPN);
  //   (b) relay NOT enabled (relayRequired=false, advertisement only) + a
  //       failed relay endpoint = SKIP that endpoint with the
  //       VPN_ENDPOINT_EXCLUSION_SKIPPED marker — never fatal (an unused
  //       relay must not abort the start; the skip stays grep-able).
  // Management/signal dial IPs are always frozen regardless (they are
  // known-good: the flow aborts earlier when those resolutions fail).
  //
  // Log discipline: hosts/ports/counts only — no tokens, no resolved IPs.
  // N2-H I2: `stuns` = the announced `netbird_config.stuns[].uri` list read
  // from the SAME `connector_network_config()` snapshot the caller already
  // holds (`net.stuns`; null/absent when no map arrived) — the stun class of
  // the seven frozen kinds, parsed by the pure module with the core's own
  // `stun:` rules (`turn:` refused by name).
  private async deriveEndpointExclusions(mgmt: ManagementEndpoint, mgmtDialIp: string,
    signal: SignalProvisionResult | null, gateway: string,
    stuns: StunUriEntry[] | null | undefined): Promise<EndpointExclusionOutcome> {
    // relay state: the connector status is the only source (relay_enabled
    // rides the core config; the ADVERTISED url set arrives with the
    // `relay_advertised` sibling section). Lenient: a missing / type-
    // anomalous section is treated as NOT advertised (grep-able hint) and
    // must never throw the existing flow off.
    let relayUrls: string[] = [];
    let advertisedView: RelayAdvertisedView =
      { advertised: false, urls: [], malformed: false };
    try {
      const status: ConnectorStatusResult = connectorStatus();
      if (status.relay !== undefined && status.relay !== null &&
        status.relay.urls !== undefined && status.relay.urls !== null) {
        relayUrls = status.relay.urls;
      }
      advertisedView = parseRelayAdvertised(status);
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'VPN_ENDPOINT_EXCLUSION_RELAY_STATE_UNKNOWN|requestId=%{public}s|' +
        'summary=%{public}s|treatment=relay-not-required|advertised=unknown',
        this.requestId, safeError(error as Error));
    }
    if (advertisedView.malformed) {
      hilog.warn(DOMAIN, TAG,
        'VPN_RELAY_ADVERTISED_MALFORMED|requestId=%{public}s|' +
        'treatment=not-advertised|note=lenient-parse-no-throw',
        this.requestId);
    } else if (!advertisedView.advertised) {
      hilog.info(DOMAIN, TAG,
        'VPN_RELAY_ADVERTISED_NONE|requestId=%{public}s|' +
        'treatment=not-advertised|urls=0',
        this.requestId);
    }
    const relayAdvertised: boolean = advertisedView.advertised;
    // Exclusion endpoint source = ADVERTISEMENT ∪ runtime urls (identical
    // strings collapse; with relay enabled the runtime set only ever
    // repeats the advertisement). relayRequired itself stays computed from
    // the RUNTIME set only — its meaning is unchanged.
    const combinedRelayUrls: string[] = [];
    for (let i = 0; i < advertisedView.urls.length; i++) {
      if (combinedRelayUrls.indexOf(advertisedView.urls[i]) < 0) {
        combinedRelayUrls.push(advertisedView.urls[i]);
      }
    }
    for (let i = 0; i < relayUrls.length; i++) {
      if (combinedRelayUrls.indexOf(relayUrls[i]) < 0) {
        combinedRelayUrls.push(relayUrls[i]);
      }
    }
    const relayRequired: boolean = relayUrls.length > 0;
    hilog.info(DOMAIN, TAG,
      'VPN_ENDPOINT_EXCLUSIONS_BEGIN|requestId=%{public}s|relayRequired=%{public}s|' +
      'relayUrls=%{public}d',
      this.requestId, String(relayRequired), relayUrls.length);

    // Endpoint descriptors + the resolutions that bypass DNS (the exact IPs
    // this flow dials).
    const endpoints: ExclusionEndpoint[] = [];
    const resolutions: EndpointResolution[] = [];
    endpoints.push({ kind: 'management', host: mgmt.host, port: mgmt.port });
    if (mgmtDialIp.length > 0) {
      resolutions.push({ kind: 'management', host: mgmt.host, ok: true, address: mgmtDialIp });
    }
    if (signal !== null) {
      endpoints.push({ kind: 'signal', host: signal.endpoint.host, port: signal.endpoint.port });
      if (signal.dialIp.length > 0) {
        resolutions.push({ kind: 'signal', host: signal.endpoint.host, ok: true,
          address: signal.dialIp });
      }
    }
    // Distinct relay HOSTS frozen from the advertised/union set (log truth:
    // two urls may share one host, and unparsable urls count only as
    // failures, never as hosts).
    const relayHosts: string[] = [];
    for (let i = 0; i < combinedRelayUrls.length; i++) {
      const parsed: RelayEndpoint | null = parseRelayUrl(combinedRelayUrls[i]);
      if (parsed === null) {
        // The core would refuse this url typed as well — an unparsable relay
        // url is an unfreezable endpoint: record as failed resolution.
        // Severity stays with relayRequired: fatal only when relay is being
        // ENABLED; a disabled relay's bad advertisement is skipped+logged.
        resolutions.push({ kind: 'relay', host: 'unparsable-relay-url', ok: false, address: '' });
        hilog.error(DOMAIN, TAG,
          'VPN_ENDPOINT_EXCLUSION_URL_MALFORMED|requestId=%{public}s|index=%{public}d|' +
          'schemes=rel|rels',
          this.requestId, i);
        continue;
      }
      if (relayHosts.indexOf(parsed.host) < 0) {
        relayHosts.push(parsed.host);
      }
      endpoints.push({ kind: 'relay', host: parsed.host, port: parsed.port });
    }

    // N2-H v3.1 C1 dns (§2-C1 dns caliber / R-4): the 7th frozen kind's SHELL
    // half is the PLATFORM default (physical) network's resolver set — never
    // the tunnel's own DNS. The branch order is pinned by the preregistration
    // and implemented in the pure module (`deriveDnsEndpointExclusions`); this
    // call site only (a) collects the platform facts and (b) records the
    // verbatim branch record. The resolvers are ordinary exclusion endpoints
    // (IPv4 literals — the fresh-resolution pass below short-circuits a
    // literal, so no DNS leg is ever pulled for them).
    //
    // A dns GAP (API error / empty / unprovable physical attribution) is
    // recorded here and is NOT fatal in this increment: widening fail-closed
    // to all five required kinds is a separate, later increment.
    const dns: DnsDerivationResult = derivePlatformDnsEndpoints();
    this.n2hDnsReport = dns;
    for (let i = 0; i < dns.endpoints.length; i++) {
      endpoints.push(dns.endpoints[i]);
    }
    // the marker carries the SAME verbatim tokens as the evidence `notes[]`
    // (a reviewer compares log and document line by line)
    hilog.info(DOMAIN, TAG,
      'N2H_DNS_BRANCH|requestId=%{public}s|dns-branch=%{public}s|' +
      'dns-physical-evidence=%{public}s|dnsResolvers=%{public}d|dnsGap=%{public}s|' +
      'diag=true|is_evidence=false',
      this.requestId, dns.branch, dns.physicalEvidence, dns.endpoints.length,
      dns.gap ? dns.gapReason : 'none');

    // N2-H I2 (design §13.13): the REQUIRED stun class, from the announced
    // `netbird_config.stuns[].uri` list that core tail-appends to
    // `connector_network_config()` (the shell had no channel to it before this
    // increment). The pure module applies core's OWN `stun:` parse (default
    // port 3478) so the shell freezes the very endpoints core gathers against;
    // `turn:` is refused with the NAMED reason and NO turn route is ever
    // generated (this build has no turn path). A missing/empty announcement or
    // a refused URI is recorded here as an honest gap — no invented endpoint,
    // no silent drop. The gap is diagnostic in this increment (widening
    // fail-closed to the five required kinds and the arm-opts contract are
    // separate increments).
    const stun: StunDerivationResult = deriveStunEndpointExclusions(stuns);
    for (let i = 0; i < stun.endpoints.length; i++) {
      endpoints.push(stun.endpoints[i]);
    }
    for (let i = 0; i < stun.rejected.length; i++) {
      hilog.warn(DOMAIN, TAG,
        'N2H_STUN_URI_REJECTED|requestId=%{public}s|index=%{public}d|reason=%{public}s|' +
        'diag=true|is_evidence=false',
        this.requestId, i, stun.rejected[i]);
    }
    hilog.info(DOMAIN, TAG,
      'N2H_STUN_ENDPOINTS|requestId=%{public}s|stuns=%{public}d|usable=%{public}d|' +
      'rejected=%{public}d|turnRejected=%{public}d|stunGap=%{public}s|' +
      'diag=true|is_evidence=false',
      this.requestId, stun.announced,
      stun.endpoints.length, stun.rejected.length, stun.turnRejected,
      stun.gap ? stun.gapReason : 'none');

    // FRESH build-time resolutions (literals short-circuit DNS; IPv4
    // preferred, IPv6 accepted as a /128 freeze). Failures map to '' here —
    // severity is decided ONCE on the merged resolution list.
    const freshAddresses: string[] = await Promise.all<string>(endpoints.map(
      (endpoint: ExclusionEndpoint): Promise<string> =>
        resolveExclusionEndpointIp(endpoint.host, DNS_RESOLVE_BOX_MS).catch((): string => '')));
    for (let i = 0; i < endpoints.length; i++) {
      resolutions.push({ kind: endpoints[i].kind, host: endpoints[i].host,
        ok: freshAddresses[i].length > 0, address: freshAddresses[i] });
    }

    const build: ExclusionBuildResult = buildExcludedRouteEntries(resolutions, gateway);
    for (let i = 0; i < build.failed.length; i++) {
      hilog.warn(DOMAIN, TAG,
        'VPN_ENDPOINT_EXCLUSION_SKIPPED|requestId=%{public}s|kind=%{public}s|host=%{public}s|' +
        'reason=unresolved|relayRequired=%{public}s|action=%{public}s',
        this.requestId, build.failed[i].kind, build.failed[i].host, String(relayRequired),
        relayRequired ? 'fail-closed' : 'skip-with-log');
    }
    hilog.info(DOMAIN, TAG,
      'VPN_ENDPOINT_EXCLUSIONS|requestId=%{public}s|hosts=%{public}d|resolved=%{public}d|' +
      'duplicates=%{public}d|failed=%{public}d|routes=%{public}d|relayRequired=%{public}s|' +
      'relayAdvertised=%{public}s|relayHosts=%{public}d',
      this.requestId, endpoints.length, build.resolvedCount, build.duplicatesRemoved,
      build.failed.length, build.routes.length, String(relayRequired),
      String(relayAdvertised), relayHosts.length);

    const fatal: boolean = isExclusionFailClosed(build, relayRequired);
    if (fatal) {
      hilog.error(DOMAIN, TAG,
        'VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED|requestId=%{public}s|' +
        'reason=relay-enabled-unresolved-endpoint|failedHosts=%{public}d|' +
        'action=no-vpn-created|is_evidence=false|not_a_gate_pass=true',
        this.requestId, build.failed.length);
    }
    // N2-H I5 (§5.3-2): retain THIS derivation's endpoints + resolutions on the
    // instance (counts/addresses only, no secret material) so the collector's
    // arm can freeze the shell-side exclusion half. Nothing is read from them
    // unless the arm control file appears (§6.1); the product paths above are
    // untouched.
    this.n2hExclusionEndpoints = endpoints;
    this.n2hExclusionResolutions = resolutions;
    return { fatal: fatal, routes: build.routes };
  }

  // N2-H I5 (§5.3-1): the device-side evidence collector's arm hook. Runs at
  // most ONCE per ability instance, only when the development control file
  // exists AND the live session is ready (fail-closed: no ready session ⇒ no
  // window, the reason is logged). The options carry the shell exclusion set
  // retained above (D7 `shell-exclusion`) plus the non-sensitive window knobs
  // from the control file — and, when that file carries it, the operator's
  // §13.10 peer announcement (`peer_b2a_probe_id`) that lets the device
  // attribute the counterparty's b2a positive control. Everything is
  // marker-logged with `diag=true|is_evidence=false`: arming is a diagnostic
  // action, never gate evidence — only the reviewed document is.
  // N2-H I5a (§13.15): every outcome of this hook is now observable — ARMED /
  // ARM_FAILED|REJECTED, the one N2H_COLLECTOR_ARM_SKIPPED line for the file
  // probe itself (see probeN2hArmFile), and — since the 2026-09-18 timing fix
  // — one deduped N2H_COLLECTOR_WAITING line while the ticket exists but the
  // session gate (wg.device_up && wg.ready) is still closed.
  private maybeArmN2hCollector(status: ConnectorStatusResult): void {
    if (this.n2hArmAttempted || this.destroyRequested) {
      return;
    }
    const armPath: string = this.n2hArmFilePath();
    const probe: N2hArmProbeOutcome = this.probeN2hArmFile(armPath);
    if (!probe.present) {
      // N2-H I5a (§13.15): for a missing/unreadable file the ONE-SHOT latch is
      // still consumed — no ticket IS the normal startup default, that single
      // probe is the whole attempt, and the skip below is printed on the FIRST
      // tick that reached this hook and never again (`attempted=true` on the
      // line is the visible form of the same single field, no second counter).
      this.n2hArmAttempted = true;
      // The silent branch the 2026-09-18 device dry-run could not explain: 84
      // zero-row collector reads, a2b_seen=0, and no marker saying the arm had
      // never been attempted. The reason distinguishes "proved absent" from
      // "probe failed without proving absence"; `path=` is the resolved
      // filesDir join of the fixed development-channel name (never file
      // content), `accessError=` is an error CODE token only.
      hilog.warn(DOMAIN, TAG,
        'N2H_COLLECTOR_ARM_SKIPPED|requestId=%{public}s|reason=%{public}s|path=%{public}s|' +
        'accessError=%{public}s|attempted=true|diag=true|is_evidence=false',
        this.requestId, probe.skipReason, armPath, probe.accessError);
      return;
    }
    const deviceUp: boolean =
      status.wg !== undefined && status.wg !== null && status.wg.device_up === true;
    const ready: boolean = status.wg !== undefined && status.wg !== null && status.wg.ready === true;
    if (!deviceUp || !ready) {
      // 2026-09-18 real-device fix (0016 run, 21:42): the ticket was written
      // while WG was still ramping (deviceUp=true, ready=false) and the FIRST
      // watcher tick consumed the only attempt here — even if the session had
      // become ready later, that instance could no longer arm (0016 ended
      // before any later state was observed). While the gate is closed
      // the attempt is therefore NOT consumed and the native arm is NOT
      // called: one deduped WAITING line (the boolean above is log-dedup
      // only, never a gate) and the EXISTING watcher's later ticks re-check.
      // No timer, no retry loop, the gate itself unchanged.
      if (!this.n2hArmWaitLogged) {
        this.n2hArmWaitLogged = true;
        hilog.warn(DOMAIN, TAG,
          'N2H_COLLECTOR_WAITING|requestId=%{public}s|reason=session-not-ready|' +
          'deviceUp=%{public}s|ready=%{public}s|attempted=false|diag=true|is_evidence=false',
          this.requestId, String(deviceUp), String(ready));
      }
      return;
    }
    // Gate open: THIS tick is the one attempt. Latched BEFORE the file read /
    // native call so a native failure is reported once (ARM_FAILED|REJECTED)
    // and never retried on later ticks.
    this.n2hArmAttempted = true;
    // S7 (2026-09-26, opt-in; protect-bypass plan §4/§6 item 5): the arm body is
    // ONE async step because the protect-probe hand-off must run
    // open -> platform protect -> commit BEFORE the native arm call. Everything
    // up to the first `await` still runs synchronously on THIS tick, so a ticket
    // without `protect_probe.mode:"protect"` reaches `n2h_collector_arm` exactly
    // as before (no await at all on that path). The last-resort catch keeps an
    // unhandled rejection impossible and reports it in the pre-existing
    // ARM_REJECTED shape.
    this.runN2hArm(armPath).catch((error: Error): void => {
      hilog.error(DOMAIN, TAG,
        'N2H_COLLECTOR_ARM_REJECTED|requestId=%{public}s|summary=%{public}s|' +
        'diag=true|is_evidence=false',
        this.requestId, safeError(error));
    });
  }

  // The gate-open half of the arm hook (the latch / gate / waiting semantics
  // stay in maybeArmN2hCollector above): read the control file's options, build
  // the options document, run the protect-probe fd hand-off when — and only
  // when — the ticket asked for the protected arm, then call the ONE native
  // arm entry.
  private async runN2hArm(armPath: string): Promise<void> {
    // The control file carries only non-sensitive window options; a malformed
    // or absent body falls back to the §5.1 defaults (the core re-validates
    // and refuses anything it cannot understand).
    let fileOpts: N2hArmFileOptions = { posctl_port: 0, probe_wait_ms: 0 };
    try {
      const text: string = fs.readTextSync(armPath).trim();
      if (text.length > 0) {
        fileOpts = JSON.parse(text) as N2hArmFileOptions;
      }
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'N2H_COLLECTOR_ARM_OPTS_IGNORED|requestId=%{public}s|summary=%{public}s|' +
        'action=defaults|diag=true|is_evidence=false',
        this.requestId, safeError(error as Error));
      fileOpts = { posctl_port: 0, probe_wait_ms: 0 };
    }
    const posctlPort: number = fileOpts.posctl_port > 0
      ? fileOpts.posctl_port
      : N2H_POSCTL_PORT_DEFAULT;
    const probeWaitMs: number = fileOpts.probe_wait_ms > 0
      ? fileOpts.probe_wait_ms
      : N2H_PROBE_WAIT_MS_DEFAULT;
    // Shell exclusion set: join the retained endpoints (kind/host/port) with
    // the retained resolutions (which carry the dialed address). Only a
    // resolved endpoint can be frozen; an unresolved one stays a core-side
    // freeze gap in the document (never guessed here).
    const exclusions: N2hShellExclusion[] = [];
    for (let i = 0; i < this.n2hExclusionEndpoints.length; i++) {
      const endpoint: ExclusionEndpoint = this.n2hExclusionEndpoints[i];
      for (let j = 0; j < this.n2hExclusionResolutions.length; j++) {
        const resolution: EndpointResolution = this.n2hExclusionResolutions[j];
        if (resolution.kind !== endpoint.kind || resolution.host !== endpoint.host) {
          continue;
        }
        if (resolution.ok !== true || resolution.address.length === 0) {
          break;
        }
        exclusions.push({
          kind: endpoint.kind,
          host: endpoint.host,
          port: endpoint.port,
          resolved: `${stripDialPort(resolution.address)}:${endpoint.port}`
        });
        break;
      }
    }
    const opts: N2hArmOptions = {
      shell_exclusions: exclusions,
      posctl_port: posctlPort,
      probe_wait_ms: probeWaitMs
    };
    // N2-H I3a (v3.1 §2.1 R-2①②③): the evidence generation G — the gen of
    // the last ACCEPTED create() (n2hAcceptedExclusionGeneration, recorded at
    // the acceptance anchor itself). This ONE read feeds both the arm option
    // the core echoes into `collector.exclusion_generation` AND the `gen=`
    // field of the ARMED marker below, which is what makes the three-way
    // "逐字相同" requirement structural instead of coincidental.
    // -1 (no accepted create yet) is unreachable here — status.wg.device_up /
    // ready are only reached after an accepted create — and is handled
    // honestly rather than fabricated: the option is NOT sent (the core then
    // records a value-less field) and the marker prints the sentinel.
    const exclusionGeneration: number = this.n2hAcceptedExclusionGeneration;
    if (exclusionGeneration >= 0) {
      opts.exclusion_generation = exclusionGeneration;
    }
    // v3.1 C1 dns (R-4): the branch record of this session's derivation rides
    // the arm verbatim (the core validates the frozen token shapes and renders
    // the three `notes[]` lines). It is ALWAYS present when a derivation ran —
    // an absent record would be a dns gap, so the shell never omits it.
    if (this.n2hDnsReport !== null) {
      const report: N2hDnsArmReport = {
        branch: this.n2hDnsReport.branch,
        physical_evidence: this.n2hDnsReport.physicalEvidence
      };
      if (this.n2hDnsReport.gap) {
        report.gap_reason = this.n2hDnsReport.gapReason;
      }
      opts.dns = report;
    }
    // §13.10: the operator's peer announcement rides the same control file. The
    // key is forwarded ONLY when the file carried it — an absent key must stay
    // absent so the core keeps the documented reasoned-gap semantics; a present
    // value goes verbatim (no trimming, no rewriting, no type coercion) and the
    // core's strict `posctl_marker` check accepts or refuses it.
    const announcedPeer: string | undefined = fileOpts.peer_b2a_probe_id;
    if (announcedPeer !== undefined) {
      opts.peer_b2a_probe_id = announcedPeer;
    }
    // 2026-09-18 (parallel posctl-target fix): the actual n2h-peer overlay
    // address rides the same control file with the SAME verbatim discipline —
    // forwarded ONLY when the file carried the key (absent stays absent, so
    // the core's "which peer" default remains the documented one), never
    // validated/trimmed/rewritten/type-coerced here. A wrong-type value (e.g.
    // a JSON number) is forwarded as that value and the core's strict bare
    // IPv4 + peer-map check refuses it — the refusal is core-side evidence,
    // not something the shell silently repairs. No marker field and no log
    // literal changes: this key is not echoed into N2H_COLLECTOR_ARMED.
    const announcedOverlay: string | undefined = fileOpts.peer_overlay_ip;
    if (announcedOverlay !== undefined) {
      opts.peer_overlay_ip = announcedOverlay;
    }
    // v4 §3 path① / §6.2 item 2: the dns-nxdomain-echo enable rides the same
    // control file with the SAME present-only + verbatim discipline — the key
    // is set ONLY when the file carried it (an absent key stays absent so the
    // core's documented default OFF is what the omission means; the shell
    // never injects `false`), never defaulted, never type-coerced. A non-boolean
    // value (e.g. a JSON string/number) is forwarded AS THAT VALUE and the
    // core's strict boolean check refuses the arm — the refusal is core-side
    // evidence, not something the shell silently repairs.
    const nxdomainEcho: boolean | undefined = fileOpts.dns_nxdomain_echo;
    if (nxdomainEcho !== undefined) {
      opts.dns_nxdomain_echo = nxdomainEcho;
    }
    // 2026-09-26 (protect-bypass plan §5/§4): the multi-probe count and the
    // opt-in protect probe ride the same control file with the SAME
    // present-only + verbatim discipline as the three keys above. The shell does
    // NO semantic validation of either: whatever value the parsed JSON carries
    // (a wrong type included) is forwarded AS THAT VALUE and the core's strict
    // checks (u64 + 1..=10; bare `IPv4:port` target, count 1..=10, frozen mode
    // tokens) decide. An absent key stays ABSENT — no default is injected, so
    // the core's documented defaults (1 probe, no protect probe at all) remain
    // exactly what an omission means. These two keys are the ONLY reason this
    // shell ever hands the core a probe socket.
    const posctlProbes: number | undefined = fileOpts.posctl_probes;
    if (posctlProbes !== undefined) {
      opts.posctl_probes = posctlProbes;
    }
    const protectProbe: N2hProtectProbeTicket | undefined = fileOpts.protect_probe;
    if (protectProbe !== undefined) {
      opts.protect_probe = protectProbe;
    }
    if (this.n2Evidence !== undefined) {
      opts.n2_capture_32 = this.n2Evidence.capture32;
    }
    // S7 (2026-09-26): the hand-off trigger, read off the SAME object that was
    // just forwarded verbatim. ONLY the exact string literal `protect` turns it
    // on: `mode:"none"` (the control arm), an absent key, a wrong-typed value
    // and a malformed object all leave the arm path byte-identical to the
    // pre-increment behavior — no open call, no protect call, no commit, no
    // line. The core still validates the object's real shape; this test only
    // decides whether the shell runs its half of the hand-off.
    let protectProbeMode: string = '';
    if (protectProbe !== undefined && protectProbe !== null &&
      typeof protectProbe === 'object') {
      protectProbeMode = protectProbe.mode === N2H_PROTECT_PROBE_MODE_PROTECT
        ? N2H_PROTECT_PROBE_MODE_PROTECT
        : '';
    }
    // 2026-09-20 (diagnostic-only): resolve the VPN network's netId via the
    // official read-only getters BEFORE the native arm, so the core can pin
    // the posctl probe's egress to the VPN network for this one diagnostic
    // socket. Found ⇒ the non-secret integer rides the opts JSON; anything
    // else (no hit, a name collision, a rejected API) keeps the key ABSENT
    // (the probe stays byte-identical) and one marker carries the reason and
    // the error CODE token — never an object, never an exception message.
    //
    // S4 (diag, default off, 2026-09-23): `diag_posctl_unbound` withholds that
    // option DELIBERATELY — the probe socket then stays on the process's
    // default network, which is the control arm for the huawei claim 「不进
    // 隧道只由 routes/应用列表决定」. One marker replaces nothing: the
    // UNAVAILABLE line below is NOT printed on this branch (the resolution
    // result is unused, and printing "unavailable" would be a false reason),
    // and the core records its existing BIND_NET result=not-requested path.
    // The found/not-found branches are otherwise untouched.
    const vpnResolution: VpnNetIdResolution = resolveVpnTunNetId();
    if (this.diagPosctlUnbound) {
      hilog.info(DOMAIN, TAG,
        'N2H_DIAG_POSCTL_UNBOUND|requestId=%{public}s|diag=true|is_evidence=false',
        this.requestId);
    } else if (vpnResolution.found) {
      opts.probe_net_id = vpnResolution.netId;
    } else {
      hilog.warn(DOMAIN, TAG,
        'N2H_POSCTL_PROBE_NET_ID_UNAVAILABLE|requestId=%{public}s|reason=%{public}s|' +
        'code=%{public}s|diag=true|is_evidence=false',
        this.requestId, vpnResolution.reason, vpnResolution.code);
    }
    const optsJson: string = JSON.stringify(opts);
    const announcedLog: string = announcedPeer === undefined ? 'none' : clean(String(announcedPeer));
    // S7 (2026-09-26, opt-in; plan §4): the protect-probe fd hand-off runs
    // HERE — after the options document is final and BEFORE the native arm, so
    // `n2h_collector_arm` consumes the fd this shell just had protected and
    // committed. Awaited: the core's arm resolves the hand-off synchronously at
    // its own entry, so "committed before the arm" is a real ordering, not a
    // race. When the hand-off itself fails (open refused / malformed answer)
    // the arm still runs — the core then records the never-committed hand-off
    // in its own notes[].
    if (protectProbeMode === N2H_PROTECT_PROBE_MODE_PROTECT) {
      await this.handOffProtectProbe();
    }
    try {
      const answer: N2hCollectorArmResult = JSON.parse(core.n2h_collector_arm(optsJson)) as
        N2hCollectorArmResult;
      if (answer.ok === true) {
        hilog.info(DOMAIN, TAG,
          'N2H_COLLECTOR_ARMED|requestId=%{public}s|gen=%{public}d|openedMono=%{public}d|' +
          'exclusions=%{public}d|posctlPort=%{public}d|probeWaitMs=%{public}d|' +
          'peerB2aProbeId=%{public}s|diag=true|is_evidence=false',
          this.requestId, exclusionGeneration, answer.opened_mono ?? -1, exclusions.length,
          posctlPort, probeWaitMs, announcedLog);
        // 2026-09-18 iface-bytes availability probe (diag-only): run 0017
        // could not read /proc/net/dev (Permission denied, counters=[]) while
        // the OFFICIAL per-NIC byte API (@ohos.net.statistics, since 10)
        // documents NO permission for the iface methods. This ONE
        // fire-and-forget sample runs ONLY on the diagnostic arm SUCCESS
        // branch — never on a plain VPN start, never inside the native window
        // (no await, no timer, no loop); every rejection is caught (the probe
        // catches each call, this catch is the last resort), so the arm path
        // cannot gain an unhandled rejection. Bytes only, diag-only: the
        // sample never joins Evidence.counters/notes, never becomes a packet
        // count, and no pre/post-ARM reconciliation happens.
        // The latch callback is the WINDOW open edge (v4 §4 (ii)): it flips
        // n2hIfaceWindowOpenSeen only AFTER the probe emitted its phase=open
        // line — strictly after this synchronous arm branch has returned —
        // which is what pins the close edge to a LATER watcher tick (see
        // maybeCloseInterfaceStatsWindow); it writes one boolean, nothing
        // else.
        runInterfaceStatsProbe(this.requestId, (): void => {
          this.n2hIfaceWindowOpenSeen = true;
        }).catch((error: Error): void => {
          // Codes/names only — the probe's discipline (never an exception
          // message, never a payload).
          hilog.warn(DOMAIN, TAG,
            'N2H_IFACE_BYTES_PROBE_ABORTED|requestId=%{public}s|code=%{public}s|' +
            'name=%{public}s|diag=true|is_evidence=false',
            this.requestId, String((error as BusinessFailure).code ?? 'no-code'),
            clean(error.name || 'Error'));
        });
        // 2026-09-20 network-context snapshot (diag-only): the same ONE-SHOT
        // arm SUCCESS branch, right after the iface-bytes probe — the PUBLIC
        // connection getters only (default/app/all NetHandle, per-handle
        // interface/bearerTypes/routes metadata), NO bindSocket/setAppNet, no
        // route change, no UID statistics. It records the SYSTEM's network
        // view at arm time as circumstantial context for the parallel native
        // SO_MARK observation; it is NOT a routing verdict (no public SO_MARK
        // layout is decoded here). Same discipline: fire-and-forget (all-Sync
        // body, no await added to the arm path), every per-call rejection is
        // caught inside the probe, this catch is the last resort.
        runNetworkContextProbe(this.requestId).catch((error: Error): void => {
          hilog.warn(DOMAIN, TAG,
            'N2H_NET_CONTEXT_ABORTED|requestId=%{public}s|code=%{public}s|' +
            'name=%{public}s|diag=true|is_evidence=false',
            this.requestId, String((error as BusinessFailure).code ?? 'no-code'),
            clean(error.name || 'Error'));
        });
        // S5 iface-bytes BURST (2026-09-26, default OFF): same arm SUCCESS
        // branch, same official API, same `vpn-tun` name — the 500 ms survey
        // series the C7 same-source calibration needs (n2h-kit plan §3 a).
        // Gated by the ticket switch; when it is off this is one boolean test
        // and NOTHING else happens (no timer, no line, byte-identical
        // behaviour). Fire-and-forget like the two probes above: the arm path
        // gains no await and no unbounded wait, and every rejection inside a
        // tick is caught by the tick's own last-resort catch.
        if (this.diagIfaceBytesBurst) {
          this.startInterfaceStatsBurst();
        }
      } else {
        hilog.error(DOMAIN, TAG,
          'N2H_COLLECTOR_ARM_FAILED|requestId=%{public}s|error=%{public}s|' +
          'peerB2aProbeId=%{public}s|diag=true|is_evidence=false',
          this.requestId, answer.error ?? 'unknown', announcedLog);
      }
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'N2H_COLLECTOR_ARM_REJECTED|requestId=%{public}s|summary=%{public}s|' +
        'diag=true|is_evidence=false',
        this.requestId, safeError(error as Error));
    }
  }

  // S7 (2026-09-26, opt-in; protect-bypass plan §4 + §6 item 5): the shell's
  // half of the protect-probe fd hand-off, run BEFORE the native arm:
  //   1. core.n2h_probe_socket_open()      — ONE unbound, core-owned UDP fd;
  //   2. this.protectProbeSocketWithBox()  — the shell's protect call on that fd;
  //   3. core.n2h_probe_socket_commit(fd, result) — the outcome for that fd;
  //   4. (the caller then arms, and the arm consumes the committed fd.)
  // fd ownership is the core's, unchanged: this method never closes the fd,
  // never duplicates it and never touches its flags (an fd closed here would
  // make the core's commit answer `fd-invalid` and the protected arm would
  // silently become a no-op). Every outcome is logged on the ONE
  // `VPN_PROTECT_PROBE|` prefix; an open failure prints its own line and
  // returns, so the arm still runs and the core records the never-committed
  // hand-off in its own notes.
  private async handOffProtectProbe(): Promise<void> {
    let opened: ProbeSocketOpenNative;
    try {
      opened = JSON.parse(core.n2h_probe_socket_open()) as ProbeSocketOpenNative;
    } catch (error) {
      // Codes/names only — the SAME discipline as the two diagnostic probes: a
      // parse failure's message can quote the body it choked on, and this line
      // must never echo a payload.
      hilog.error(DOMAIN, TAG,
        'VPN_PROTECT_PROBE|requestId=%{public}s|fd=-1|handoff_id=|outcome=open-failed|' +
        'elapsedMs=0|commit=none|error=open-rejected|summary=code=%{public}s,name=%{public}s|' +
        'diag=true|is_evidence=false',
        this.requestId, String((error as BusinessFailure).code ?? 'no-code'),
        clean((error as Error).name || 'Error'));
      return;
    }
    if (opened.ok !== true || opened.fd === undefined || opened.fd < 0) {
      hilog.error(DOMAIN, TAG,
        'VPN_PROTECT_PROBE|requestId=%{public}s|fd=-1|handoff_id=|outcome=open-failed|' +
        'elapsedMs=0|commit=none|error=%{public}s|diag=true|is_evidence=false',
        this.requestId, clean(String(opened.error ?? 'no-fd')));
      return;
    }
    const fd: number = opened.fd as number;
    // The hand-off id is the core's own `N2H-protect-<24 hex>` token: echoed
    // verbatim into the line so the shell's outcome can be joined with the
    // core's phase=open/commit lines (which carry the SAME id). A missing token
    // is logged as `none`, never invented.
    const handoffId: string = clean(String(opened.handoff_id ?? 'none'));
    const connection: vpnExtension.VpnConnection | undefined = this.connection;
    if (connection === undefined) {
      // Unreachable on the real path (onCreate assigns this.connection before
      // any watcher tick can reach the arm hook); honest, still-arming failure
      // instead of a thrown-away rejected promise.
      hilog.error(DOMAIN, TAG,
        'VPN_PROTECT_PROBE|requestId=%{public}s|fd=%{public}d|handoff_id=%{public}s|' +
        'outcome=no-connection|elapsedMs=0|commit=none|diag=true|is_evidence=false',
        this.requestId, fd, handoffId);
      return;
    }
    const protectedOutcome: ProtectProbeOutcome =
      await this.protectProbeSocketWithBox(connection, fd);
    const commit: string = this.commitProtectProbe(fd, protectedOutcome.result);
    hilog.info(DOMAIN, TAG,
      'VPN_PROTECT_PROBE|requestId=%{public}s|fd=%{public}d|handoff_id=%{public}s|' +
      'outcome=%{public}s|elapsedMs=%{public}d|commit=%{public}s|diag=true|is_evidence=false',
      this.requestId, fd, handoffId, protectedOutcome.outcome, protectedOutcome.elapsedMs, commit);
  }

  // The commit half of the hand-off: report THIS shell's protect outcome for the
  // fd the core handed over. Returns the log line's `commit=` value: 'ok' on the
  // core's acceptance, the core's own refusal token otherwise
  // (`bad-result` / `no-pending-fd` / `fd-mismatch` / `fd-invalid`), and
  // 'rejected' when the native call itself could not be parsed. Never throws and
  // never retries: a refused commit is exactly what the experiment must see.
  private commitProtectProbe(fd: number, result: string): string {
    let commit: ProbeSocketCommitNative;
    try {
      commit = JSON.parse(core.n2h_probe_socket_commit(fd, result)) as ProbeSocketCommitNative;
    } catch (error) {
      return 'rejected';
    }
    if (commit.ok === true) {
      return 'ok';
    }
    return clean(String(commit.error ?? 'unknown'));
  }

  // S7 (2026-09-26, plan §4): the DEDICATED protect wrapper for the probe fd.
  // It ALWAYS calls the platform `connection.protect(fd)` — deliberately NOT
  // routed through protectSocketWithBox, whose `protectDeviation=route-exclusion`
  // short-circuit returns true WITHOUT calling the platform and would silently
  // void this whole experiment (the deviation exists precisely to model
  // "protect unavailable"). The 5 s wait is the house-standard bounded box and
  // the handle is cleared as soon as the platform call settles (the 2026-09-25
  // fix), so a resolved/rejected protect leaves no stale timeout behind.
  // Four-way answer, never throws:
  //   resolve    -> {outcome:'settled',  result:'protected'}
  //   reject     -> {outcome:'rejected', result:'unprotected'}
  //   box expiry -> {outcome:'timeout',  result:'error'}
  //   sync throw -> {outcome:'throw',    result:'error'}
  // No gate reads this value: it is recorded (via commit) and nothing else.
  private async protectProbeSocketWithBox(connection: vpnExtension.VpnConnection,
    fd: number): Promise<ProtectProbeOutcome> {
    const t0: number = Date.now();
    let boxTimer: number = -1;
    const settled: Promise<ProtectProbeOutcome> = new Promise<ProtectProbeOutcome>(
      (resolve: (value: ProtectProbeOutcome) => void): void => {
        try {
          connection.protect(fd).then((): void => {
            clearTimeout(boxTimer);
            resolve({ outcome: 'settled', result: 'protected', elapsedMs: Date.now() - t0 });
          }).catch((): void => {
            clearTimeout(boxTimer);
            resolve({ outcome: 'rejected', result: 'unprotected', elapsedMs: Date.now() - t0 });
          });
        } catch (error) {
          clearTimeout(boxTimer);
          resolve({ outcome: 'throw', result: 'error', elapsedMs: Date.now() - t0 });
        }
      });
    const box: Promise<ProtectProbeOutcome> = new Promise<ProtectProbeOutcome>(
      (resolve: (value: ProtectProbeOutcome) => void): void => {
        boxTimer = setTimeout((): void => {
          resolve({ outcome: 'timeout', result: 'error', elapsedMs: Date.now() - t0 });
        }, PROTECT_BOX_MS);
      });
    return await Promise.race([settled, box]);
  }

  // The iface-bytes WINDOW close edge: at most ONE vpn-tun re-read per
  // instance, issued only after the open edge was recorded (the probe's
  // phase=open line) and never during teardown. One-shot by
  // n2hIfaceWindowCloseDone, consumed at ISSUE time — later ticks repeat
  // nothing even if the read itself rejected (the close line then carries
  // unavailable + the error CODE token; the diagnostic flow continues either
  // way). Fire-and-forget: no await in the tick, the last-resort catch below
  // keeps an unhandled rejection impossible (codes/names only, same
  // discipline as the probe's guard). C7 counting input only — never
  // Evidence.counters/notes, never the judge (v4 §4 影响与边界).
  private maybeCloseInterfaceStatsWindow(): void {
    if (!this.n2hIfaceWindowOpenSeen || this.n2hIfaceWindowCloseDone ||
      this.destroyRequested) {
      return;
    }
    this.n2hIfaceWindowCloseDone = true;
    // S5: the burst survey series is deliberately NOT stopped here. This edge
    // is the app-side boundary of the collector WINDOW (the native
    // N2H_COLLECTOR_CLOSE line is emitted by the core and is not observable in
    // this shell) and it lands only ~5 s after ARMED, while the C7 same-source
    // calibration needs two sample points ≥10 s apart from a steady peer
    // stream: the series runs on to its own tick cap instead.
    runInterfaceStatsWindowClose(this.requestId).catch((error: Error): void => {
      hilog.warn(DOMAIN, TAG,
        'N2H_IFACE_BYTES_WINDOW_CLOSE_ABORTED|requestId=%{public}s|code=%{public}s|' +
        'name=%{public}s|diag=true|is_evidence=false',
        this.requestId, String((error as BusinessFailure).code ?? 'no-code'),
        clean(error.name || 'Error'));
    });
  }

  // S5 iface-bytes BURST (2026-09-26, default OFF) — the start/stop pair. ONE
  // bounded interval behind the `diag_iface_bytes_burst` ticket switch: while
  // it runs, every N2H_IFACE_BURST_INTERVAL_MS one vpn-tun sample is emitted
  // as a `N2H_IFACE_BYTES_BURST|` line by the probe module (the read stays
  // there — this shell owns only the cadence, because the cadence must be
  // cancellable from the teardown paths). Hard cap: N2H_IFACE_BURST_MAX_TICKS,
  // reached by the series itself (the window close edge does not stop it).
  // Starting twice is impossible: the handle guard below plus the one-shot arm
  // attempt (n2hArmAttempted) both refuse.
  private startInterfaceStatsBurst(): void {
    if (this.n2hIfaceBurstTimer >= 0 || this.destroyRequested) {
      return;
    }
    this.n2hIfaceBurstSeq = 0;
    this.n2hIfaceBurstTimer = setInterval((): void => {
      this.interfaceStatsBurstTick();
    }, N2H_IFACE_BURST_INTERVAL_MS);
  }

  // ONE burst tick. Every exit that is not an emission goes through
  // stopInterfaceStatsBurst, so a tick that arrives after the cap or after
  // destroyRequested can neither print a line nor leave
  // a live handle behind (the -1 handle is checked first, so even a callback
  // that somehow survived cancellation is inert). Fire-and-forget: no await
  // added anywhere, and the last-resort catch keeps an unhandled rejection
  // impossible (code/name tokens only — the line itself already carries the
  // per-direction CODE tokens).
  private interfaceStatsBurstTick(): void {
    if (this.n2hIfaceBurstTimer < 0 || this.destroyRequested ||
      this.n2hIfaceBurstSeq >= N2H_IFACE_BURST_MAX_TICKS) {
      this.stopInterfaceStatsBurst();
      return;
    }
    const seq: number = this.n2hIfaceBurstSeq;
    this.n2hIfaceBurstSeq++;
    readInterfaceStatsBurst(this.requestId, seq).catch((error: Error): void => {
      hilog.warn(DOMAIN, TAG,
        'N2H_IFACE_BYTES_BURST_ABORTED|requestId=%{public}s|seq=%{public}d|code=%{public}s|' +
        'name=%{public}s|diag=true|is_evidence=false',
        this.requestId, seq, String((error as BusinessFailure).code ?? 'no-code'),
        clean(error.name || 'Error'));
    });
  }

  // Stop the burst survey series and CLEAR the interval handle. Called by
  // (a) stopStatusAndNetcfgWatcher — the stop/teardown paths: connector death,
  // fail-closed recreate teardown, onDestroy — and (b) the tick itself on the
  // cap or on destroyRequested. Deliberately NOT called by the window close
  // edge (maybeCloseInterfaceStatsWindow): the app window closing ~5 s after
  // ARMED must not truncate a series whose calibration needs ≥10 s of steady
  // peer traffic.
  // Idempotent: -1 is a no-op, so repeat calls print nothing and clear nothing
  // twice.
  private stopInterfaceStatsBurst(): void {
    if (this.n2hIfaceBurstTimer >= 0) {
      clearInterval(this.n2hIfaceBurstTimer);
      this.n2hIfaceBurstTimer = -1;
    }
  }

  // The arm ticket's path under THIS process's own filesDir: the same
  // name-only resolution the credential files use (`<filesDir>/<basename>`,
  // N2H_ARM_FILE_NAME for the why). Never a fixed absolute path again.
  private n2hArmFilePath(): string {
    return `${this.context.filesDir}/${N2H_ARM_FILE_NAME}`;
  }

  // S2-S4: read the three diagnostic switches out of the arm ticket. Called
  // ONCE per start flow, BEFORE any VpnConfig is built. Never throws: a
  // missing / unreadable / empty / malformed ticket leaves every switch OFF
  // and prints nothing — the ticket's ABSENCE is the documented default (not
  // an anomaly: EntryAbility deletes a leftover ticket on every launch without
  // the parameter). Only a key PRESENT with a non-boolean value is logged, by
  // readDiagSwitch.
  private readDiagSwitches(): void {
    core.n2_socket_trace_arm(false);
    this.n2Evidence = undefined;
    this.n2EvidenceError = '';
    this.diagBlockedSelf = false;
    this.diagProtectProcessNet = false;
    this.diagPosctlUnbound = false;
    this.diagIfaceBytesBurst = false;
    this.diagCaptureRoute = false;
    this.diagCaptureTarget = '';
    this.endpointExclusionsEnabled = true;
    let text: string = '';
    try {
      text = fs.readTextSync(this.n2hArmFilePath()).trim();
    } catch (error) {
      return; // no ticket / unreadable: structural default, no log
    }
    if (text.length === 0) {
      return; // empty ticket: carries no key at all
    }
    let doc: Record<string, Object> | null = null;
    try {
      doc = JSON.parse(text) as Record<string, Object>;
    } catch (error) {
      return; // malformed document: every key unreadable → off, no log
    }
    if (doc === null) {
      return; // JSON 'null' parses but has no keys
    }
    this.diagBlockedSelf = this.readDiagSwitch(doc, N2H_DIAG_BLOCKED_SELF_KEY);
    this.diagProtectProcessNet = this.readDiagSwitch(doc, N2H_DIAG_PROTECT_PROCESS_NET_KEY);
    this.diagPosctlUnbound = this.readDiagSwitch(doc, N2H_DIAG_POSCTL_UNBOUND_KEY);
    this.diagIfaceBytesBurst = this.readDiagSwitch(doc, N2H_DIAG_IFACE_BYTES_BURST_KEY);
    // S6 (2026-09-26): the capture-route switch and its target. The target is
    // the ticket's own `protect_probe.target` — the SAME object the arm
    // forwards to the core verbatim — read here (EARLY) because the route must
    // be in the FIRST VpnConfig, long before the arm hook runs.
    this.diagCaptureRoute = this.readDiagSwitch(doc, N2H_DIAG_CAPTURE_ROUTE_KEY);
    const exclusionSetting: Object | undefined = doc[N2H_ENDPOINT_EXCLUSIONS_KEY];
    if (exclusionSetting !== undefined) {
      if (typeof exclusionSetting === 'boolean') {
        this.endpointExclusionsEnabled = exclusionSetting;
      } else {
        hilog.warn(DOMAIN, TAG,
          'N2H_DIAG_OPTION_IGNORED|key=%{public}s|reason=%{public}s|diag=true|is_evidence=false',
          N2H_ENDPOINT_EXCLUSIONS_KEY, N2H_DIAG_OPTION_IGNORED_REASON);
      }
    }
    this.diagCaptureTarget = this.readProtectProbeTarget(doc);
    const decision: N2EvidenceDecision = parseN2Evidence(doc['n2_evidence'],
      this.diagBlockedSelf, this.diagCaptureRoute, this.n2ForceDefaultRoute === true, exclusionSetting);
    this.n2EvidenceError = decision.error;
    this.n2Evidence = decision.options;
    if (this.n2Evidence !== undefined) {
      core.n2_socket_trace_arm(true);
      this.endpointExclusionsEnabled = false;
    }
  }

  private probeN2ProcNet(): void {
    let uid: string = 'unknown';
    try {
      const status: string = fs.readTextSync('/proc/self/status');
      const row: string | undefined = status.split('\n').find((line: string): boolean =>
        line.startsWith('Uid:'));
      if (row !== undefined) {
        uid = row.trim().split(/\s+/)[1] || 'unknown';
      }
    } catch (error) { /* probe below records each file separately */ }
    const names: string[] = ['tcp', 'udp', 'tcp6', 'udp6'];
    const roots: string[] = ['/proc/net/', '/proc/self/net/'];
    for (let i = 0; i < roots.length; i++) {
      for (let j = 0; j < names.length; j++) {
        const path: string = roots[i] + names[j];
        try {
          const text: string = fs.readTextSync(path);
          const lines: number = text.length === 0 ? 0 : text.trimEnd().split('\n').length;
          hilog.info(DOMAIN, TAG,
            'N2_PROC_NET|path=%{public}s|readable=true|lines=%{public}d|uid=%{public}s|' +
            'diag=true|is_evidence=false', path, lines, uid);
        } catch (error) {
          hilog.warn(DOMAIN, TAG,
            'N2_PROC_NET|path=%{public}s|readable=false|lines=0|uid=%{public}s|' +
            'code=%{public}s|diag=true|is_evidence=false',
            path, uid, String((error as BusinessFailure).code ?? 'no-code'));
        }
      }
    }
  }

  // S6: the capture route's destination, taken from the ticket's `protect_probe`
  // object. ONLY a JSON string `target` is ever used; an absent key, a
  // non-object `protect_probe`, a non-string `target` or an empty string all
  // answer '' — the switch then logs `reason=no-target` instead of inventing a
  // destination. Nothing is validated here beyond "it is a string": the strict
  // `IPv4:port` acceptance lives in the pure route helper (withCaptureRoute),
  // which mirrors the core's own parse so a target the core would refuse never
  // gets a route. Silent by construction (no log): the capture line is printed
  // by injectCaptureRoute, only when the switch is on.
  private readProtectProbeTarget(doc: Record<string, Object>): string {
    const raw: Object | undefined = doc[N2H_PROTECT_PROBE_KEY];
    if (raw === undefined || raw === null || typeof raw !== 'object') {
      return '';
    }
    const target: Object | undefined = (raw as Record<string, Object>)['target'];
    return typeof target === 'string' ? target : '';
  }

  // ONE switch key of the ticket document: literal boolean true = ON. An
  // absent key is OFF and SILENT (JSON has no `undefined`, so an absent key
  // reads as undefined; a JSON `null` is NOT absent and takes the ignored
  // branch below). A present non-boolean value is OFF and prints the one
  // ignored-key line — the key NAME and the fixed reason token only, never the
  // value, which may be arbitrary operator text. An explicit boolean `false`
  // is a valid, correctly typed OFF and stays silent: the line exists to
  // expose a WRONG TYPE, not to report an intentional disable.
  private readDiagSwitch(doc: Record<string, Object>, key: string): boolean {
    const raw: Object | undefined = doc[key];
    if (raw === undefined) {
      return false;
    }
    if (typeof raw === 'boolean') {
      return raw === true;
    }
    hilog.warn(DOMAIN, TAG,
      'N2H_DIAG_OPTION_IGNORED|key=%{public}s|reason=%{public}s|diag=true|is_evidence=false',
      key, N2H_DIAG_OPTION_IGNORED_REASON);
    return false;
  }

  // Merge the tunnel's own subnet into `config.routes` BEFORE the
  // generation/route logs and the fail-closed validation gate (both legs), so
  // the announced route count and the validation gate cover exactly the list
  // that is submitted. UNCONDITIONAL since 2026-09-26 (AUTH 0031 R0/R1):
  // without the entry a packet to a tunnel address sent from an unbound socket
  // never enters the VPN. The pure helper decides (withTunnelSubnetRoute); this
  // method only logs the outcome — one line per leg, result=added|skipped plus
  // the reason token, network='' when no network could be derived. Never a
  // gate: it can only append a non-default, non-excluded route.
  private injectTunnelSubnetRoute(config: VpnTunnelConfig, leg: string): VpnTunnelConfig {
    const outcome: TunnelSubnetRouteOutcome = withTunnelSubnetRoute(config);
    hilog.info(DOMAIN, TAG,
      'VPN_TUNNEL_SUBNET_ROUTE|requestId=%{public}s|addr=%{public}s|prefix=%{public}d|' +
      'network=%{public}s|leg=%{public}s|result=%{public}s|reason=%{public}s',
      this.requestId, config.tunnelAddress, config.tunnelPrefixLength, outcome.network, leg,
      outcome.added ? 'added' : 'skipped', outcome.reason);
    return outcome.config;
  }

  // S6 (diag, default off; protect-bypass plan §3.1/§6 ②): merge ONE explicit
  // NON-excluded `<target>/32` capture route into `config.routes`, in the SAME
  // position as the tunnel-subnet entry (after the default-route gate, before
  // the generation marker / route rows / validation gate, on BOTH legs). Only
  // called when the switch is on, so an off switch leaves the submitted
  // VpnConfig byte-identical. The pure helper decides (withCaptureRoute); this
  // method only logs the outcome — one line per leg, the derived /32 in
  // `target=` ('' when none could be derived) and result=added|skipped plus the
  // reason token. Never a gate: it can only append a non-default, non-excluded
  // host route, and an excluded target is refused by the helper instead of
  // re-creating the endpoint masking this experiment exists to remove.
  private injectCaptureRoute(config: VpnTunnelConfig, leg: string): VpnTunnelConfig {
    const outcome: CaptureRouteOutcome = withCaptureRoute(config, this.diagCaptureTarget);
    hilog.info(DOMAIN, TAG,
      'N2H_DIAG_CAPTURE_ROUTE|requestId=%{public}s|target=%{public}s|leg=%{public}s|' +
      'result=%{public}s|reason=%{public}s|diag=true|is_evidence=false',
      this.requestId, outcome.target, leg, outcome.added ? 'added' : 'skipped', outcome.reason);
    return outcome.config;
  }

  // S2 (diag, default off): the ONE place the blocked-application list is
  // decided — this app's own bundle name, exactly one entry, ONLY when the
  // switch is on; otherwise undefined, which keeps VpnConfig.blockedApplications
  // ABSENT (never an empty array). trustedApplications is never set (the two
  // lists are mutually exclusive per the SDK); the platform VpnConfig builder
  // writes the field only for a non-empty list.
  private diagBlockedApplicationList(leg: string): string[] | undefined {
    if (!this.diagBlockedSelf) {
      return undefined;
    }
    hilog.info(DOMAIN, TAG,
      'N2H_DIAG_BLOCKED_APPS|requestId=%{public}s|bundle=%{public}s|count=1|leg=%{public}s|' +
      'diag=true|is_evidence=false',
      this.requestId, SELF_BUNDLE_NAME, leg);
    return [SELF_BUNDLE_NAME];
  }

  // N2-H I5a (§13.15): probe the arm control file WITHOUT hiding why the arm
  // did not happen. `fs.accessSync(path)` (@ohos.file.fs) can report "not
  // there" in TWO documented shapes — the boolean `false` return and the
  // BusinessError `13900002` — and can also throw codes that do NOT prove
  // absence (`13900012` permission denied, `13900005` I/O error, `13900011` OOM,
  // `13900013` bad address, `13900042` unknown, …). The two cases are kept
  // apart: `arm-file-absent` is a PROOF (false or 13900002), every other error
  // token is `arm-file-unreadable` (the file may exist but this process could
  // not get to it). The raw code rides the skip line as `accessError=`: a
  // decimal code, 'none' for the boolean answer, 'no-code' for an error thrown
  // without a code token — never path content, never a message.
  private probeN2hArmFile(path: string): N2hArmProbeOutcome {
    let present: boolean = false;
    try {
      present = fs.accessSync(path);
    } catch (error) {
      const code: string = String((error as BusinessFailure).code ?? 'no-code');
      return {
        present: false,
        skipReason: code === N2H_ACCESS_ENOENT ? N2H_ARM_FILE_ABSENT : N2H_ARM_FILE_UNREADABLE,
        accessError: code
      };
    }
    return present
      ? { present: true, skipReason: '', accessError: 'none' }
      : { present: false, skipReason: N2H_ARM_FILE_ABSENT, accessError: 'none' };
  }

  // N2 T3 §3「正控」private channel (2026-09-28): the shell's armed-only entry,
  // the missing caller of `sendN2PositiveControl` / NetBirdN2Control's
  // `sendN2Control`. ONE file = ONE control: the request file is deleted in
  // `finally`, so the 5 s watcher tick can never replay it; the core's own
  // `N2_CONTROL_SENT` / `N2_CONTROL_HIT` / `N2_CONTROL_MISS` lines stay the
  // authoritative record of what the marker did.
  //
  // Trigger (documented in docs/n2-evidence-switch-20260928.md): the operator
  // writes ONE request through the existing non-secret want-parameter private
  // channel, which needs no new permission —
  //   aa start -b cn.alfadb.netbird -a EntryAbility \
  //     --ps n2h_arm_options '<the same arm ticket>' \
  //     --ps n2_control '{"target":"119.29.29.119:53","proto":"udp","id":"evt1-pre"}'
  // EntryAbility writes it to <filesDir>/netbird-n2-control; this poll picks it
  // up on the next watcher tick and consumes it. The sandwich is three separate
  // launches (pre-control -> production event -> post-control) with a fresh
  // `id` per leg, because a reused marker id is refused by the core
  // (`duplicate-control`). `target` must be one frozen outer destination inside
  // the armed ticket's `capture_32`; a target outside it is refused by name
  // (`target-not-captured`) and never sent.
  //
  // UNARMED: the request is still consumed (one stat per tick, no line unless
  // the file exists) and answered `result=rejected|reason=not-armed` WITHOUT
  // any send — the core is never called, and the leftover cannot survive to
  // fire after a later arming.
  private pollN2Control(): void {
    if (this.destroyRequested) {
      return;
    }
    const path: string = `${this.context.filesDir}/${N2_CONTROL_FILE_NAME}`;
    let text: string = '';
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      text = fs.readTextSync(path);
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'N2_CONTROL_READ_FAILED|requestId=%{public}s|code=%{public}s|diag=true|is_evidence=false',
        this.requestId, String((error as BusinessFailure).code ?? 'none'));
      return;
    }
    try {
      const decision: N2ControlDecision = parseN2ControlRequest(text);
      if (decision.request === undefined) {
        hilog.warn(DOMAIN, TAG, '%{public}s',
          formatN2ControlShellReject(this.requestId, decision.error));
        return;
      }
      const request = decision.request;
      if (this.n2Evidence === undefined) {
        hilog.warn(DOMAIN, TAG, '%{public}s',
          formatN2ControlShellLine(this.requestId, request, 'rejected', 'not-armed'));
        return;
      }
      let result: string = 'rejected';
      let reason: string = 'none';
      try {
        this.sendN2PositiveControl(request.target, request.proto, request.id);
        result = 'sent';
        reason = 'ok';
      } catch (error) {
        // the shell's own armed/refusal branches (target-not-captured,
        // invalid-marker, n2-control-open-failed, …) or a core refusal — never
        // reported as sent
        result = 'rejected';
        reason = (error as Error).message !== undefined && (error as Error).message.length > 0
          ? (error as Error).message
          : 'send-throw';
      }
      hilog.info(DOMAIN, TAG, '%{public}s',
        formatN2ControlShellLine(this.requestId, request, result, reason));
    } finally {
      // consume exactly once, whatever happened above
      try {
        fs.unlinkSync(path);
      } catch (error) {
        hilog.warn(DOMAIN, TAG,
          'N2_CONTROL_CONSUME_FAILED|requestId=%{public}s|code=%{public}s|diag=true|is_evidence=false',
          this.requestId, String((error as BusinessFailure).code ?? 'none'));
      }
    }
  }

  // N2 T5a「B 断流刺激」(preregistration v4 §4 B / §7 T5a): the shell's
  // armed-only private-channel entry — the analogue of NetBirdN2Control's
  // `sendN2Control`. ONE file = ONE stimulus: the request file is deleted in
  // `finally`, so the 5 s watcher tick can never replay it, and the core's own
  // `N2_STIMULUS|…` line stays the authoritative record (fd/endpoint/result).
  // Unarmed sessions never read the file and never call the core.
  //
  // Trigger (documented in docs/n2-evidence-switch-20260928.md): the operator
  // writes ONE request through the existing non-secret want-parameter private
  // channel, which needs no new permission —
  //   aa start -b cn.alfadb.netbird -a EntryAbility \
  //     --ps n2h_arm_options '<the same arm ticket>' \
  //     --ps n2_stimulus '{"kind":"M","id":"b-M-1"}'
  // EntryAbility writes it to <filesDir>/netbird-n2-stimulus; this poll picks
  // it up on the next watcher tick and consumes it. Repeating the command three
  // times (fresh id each time) is the frozen three-repetition minimum.
  private pollN2Stimulus(): void {
    if (this.n2Evidence === undefined || this.destroyRequested) {
      return;
    }
    const path: string = `${this.context.filesDir}/${N2_STIMULUS_FILE_NAME}`;
    let text: string = '';
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      text = fs.readTextSync(path);
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'N2_STIMULUS_READ_FAILED|requestId=%{public}s|code=%{public}s|diag=true|is_evidence=false',
        this.requestId, String((error as BusinessFailure).code ?? 'none'));
      return;
    }
    try {
      const decision: N2StimulusDecision = parseN2Stimulus(text);
      if (decision.request === undefined) {
        hilog.warn(DOMAIN, TAG, '%{public}s',
          formatN2StimulusShellReject(this.requestId, decision.error));
        return;
      }
      const kind: string = decision.request.kind;
      const id: string = decision.request.id;
      let reply: N2StimulusReply = { ok: false, result: 'rejected', reason: 'core-throw' };
      try {
        reply = parseN2StimulusReply(core.n2_stimulus(kind, id));
      } catch (error) {
        // the core refuses a non-armed call; a thrown call is reported as its
        // own reason, never as applied
        reply = { ok: false, result: 'rejected', reason: 'core-throw' };
      }
      hilog.info(DOMAIN, TAG, '%{public}s',
        formatN2StimulusShellLine(this.requestId, kind, id, reply));
    } finally {
      // consume exactly once, whatever happened above
      try {
        fs.unlinkSync(path);
      } catch (error) {
        hilog.warn(DOMAIN, TAG,
          'N2_STIMULUS_CONSUME_FAILED|requestId=%{public}s|code=%{public}s|diag=true|is_evidence=false',
          this.requestId, String((error as BusinessFailure).code ?? 'none'));
      }
    }
  }

  // Runtime watch (while the VPN is up): poll connector_status (state /
  // counts / last error class) and connector_network_config. Platform
  // conclusion baked in here: VpnConnection has NO runtime route/DNS update
  // API — a map serial newer than the applied one is logged as explicitly
  // NOT applied, never silently dropped, never pretended applied.
  //
  // N3-7 gap 3 (connector-death teardown): the watcher ACTIVELY dismantles
  // the VPN when
  //   (a) status.terminal === true — the worker ended by itself (fatal auth
  //       error / retry budget exhausted: running=false, state=failed), or
  //   (b) connector_status() failed CONNECTOR_POLL_FAILURE_LIMIT times in a
  //       row (native library unreachable).
  // Teardown = connectorStop() (idempotent) + exactly-once
  // connection.destroy() via destroyOnce (latched), each with the logged
  // reason. A USER stop (state=disconnected) is NOT terminal and never
  // triggers this path.
  private startStatusAndNetcfgWatcher(): void {
    if (this.watcherTimer >= 0 || this.destroyRequested) {
      return;
    }
    this.watcherTimer = setInterval((): void => {
      // N2-H iface-bytes WINDOW close edge (v4 preregistration §4 (ii), C7
      // same-source calibration prerequisite): rides EVERY watcher trigger,
      // at the VERY TOP of the tick — BEFORE the status read and before the
      // terminal / recreate / poll-failure early returns — so the two-edge
      // invariant「open edge seen ⇒ the NEXT watcher trigger issues the close
      // edge exactly once」is unconditional for every non-teardown watcher
      // trigger, in every tick form (healthy, terminal, recreate pending,
      // status unreachable). The sole exception is teardown: destroyRequested
      // is latched by onDestroy (which also stops the watcher), and the close
      // edge is skipped while it is set. A
      // teardown-straddling tick no longer drops the close edge: the close
      // read is a diag-only paired read that does not depend on connector
      // status. The call is a no-op until the probe's async open-edge latch
      // fires (which cannot happen inside the arming tick's synchronous
      // body, so the close can never land on the arming tick itself); the
      // one-shot latch consumes the issue, so later ticks repeat nothing.
      // No new timer, no loop, no await; ARM/latch/WAITING semantics
      // untouched; C7 counting input only, never Evidence/judge.
      this.maybeCloseInterfaceStatsWindow();
      // N2 T3/T5a private channels: the COMBINATION RULE of the two pending
      // requests is fixed HERE — within one tick the positive-control poll runs
      // BEFORE the stimulus poll, so if both files are somehow pending (two
      // separate launches before a tick) the control is always submitted first.
      // A SINGLE launch carrying both parameters writes neither (EntryAbility's
      // N2_CONTROL_COMBO_REJECTED), so this order only covers the two-launch
      // shape. Both polls are one-shot + self-consuming, and the control poll
      // is a bare stat (no line, no core call) when nothing is pending.
      this.pollN2Control();
      // N2 T5a: the private-channel stimulus poll rides every watcher trigger
      // at the same place as the window close edge — BEFORE the status read
      // and the terminal/recreate early returns, so an armed session picks up
      // a stimulus within one 5 s tick in every tick form. No-op (not even a
      // file probe) unless an accepted n2_evidence ticket armed the session.
      this.pollN2Stimulus();
      const status: ConnectorStatusResult | null = pollStatusOrNull();
      if (status !== null) {
        this.statusPollFailures = 0;
        const errorClass: string =
          status.last_error !== null && status.last_error !== undefined
            ? status.last_error.class
            : 'none';
        // N5d: signal link state rides the same poll (registered /
        // reconnects / class-only last error)
        const signalRegistered: string =
          status.signal !== undefined && status.signal !== null
            ? String(status.signal.registered)
            : 'unknown';
        const signalReconnects: number =
          status.signal !== undefined && status.signal !== null ? status.signal.reconnects : -1;
        const signalError: string =
          status.signal !== undefined && status.signal !== null &&
          status.signal.last_error !== null && status.signal.last_error !== undefined
            ? status.signal.last_error.class
            : 'none';
        // N7: WG data-plane state (device-backed seam) rides the same poll
        const wgReady: string =
          status.wg !== undefined && status.wg !== null ? String(status.wg.ready) : 'unknown';
        const wgSessions: number =
          status.wg !== undefined && status.wg !== null ? status.wg.peers_with_session : -1;
        const wgDeviceUp: string =
          status.wg !== undefined && status.wg !== null ? String(status.wg.device_up) : 'unknown';
        // Byte counters make the tunnel's traffic observable from the device
        // side (probe validation: rx_bytes_to_tun growing = the peer's probe
        // was decrypted and handed to the platform TUN).
        const wgRxToTun: number =
          status.wg !== undefined && status.wg !== null ? (status.wg.rx_bytes_to_tun ?? -1) : -1;
        const wgTxBytes: number =
          status.wg !== undefined && status.wg !== null ? (status.wg.tx_bytes ?? -1) : -1;
        const statusSignature: string = `${status.state}|${status.peer_count}|${status.route_count}|` +
          `${errorClass}|${signalRegistered}|${signalReconnects}|${signalError}|` +
          `${wgDeviceUp}|${wgReady}|${wgSessions}|${status.relay?.state ?? 'none'}`;
        const now: number = Date.now();
        if (statusSignature !== this.lastStatusLogSignature || now - this.lastStatusLogAtMs >= 60000) {
          this.lastStatusLogSignature = statusSignature;
          this.lastStatusLogAtMs = now;
          hilog.info(DOMAIN, TAG,
            'VPN_CONNECTOR_STATUS|requestId=%{public}s|running=%{public}s|state=%{public}s|' +
            'peers=%{public}d|routes=%{public}d|reconnects=%{public}d|lastError=%{public}s|' +
            'terminal=%{public}s|signalRegistered=%{public}s|signalReconnects=%{public}d|' +
            'signalLastError=%{public}s|wgDeviceUp=%{public}s|wgReady=%{public}s|wgSessions=%{public}d|' +
            'wgRxToTun=%{public}d|wgTxBytes=%{public}d',
            this.requestId, String(status.running), status.state, status.peer_count,
            status.route_count, status.reconnects, errorClass, String(status.terminal),
            signalRegistered, signalReconnects, signalError, wgDeviceUp, wgReady, wgSessions,
            wgRxToTun, wgTxBytes);
          hilog.info(DOMAIN, TAG, '%{public}s',
            formatRelayStatusLine(this.requestId, status));
        }
        // Cross-process status bridge (device-verified defect 2026-09-14): the
        // UI ability lives in ITS OWN process, so its NAPI status reads hit an
        // empty connector slot and the UI showed "disconnected / wg sessions: 0"
        // while THIS process had a live WG session. Publish the same fields to
        // filesDir, which both processes share.
        this.publishStatusForUi(status);
        // Refresh the ICE queue depth from the LIVE status: a depth learned
        // only from our own feed calls goes stale and stops resupply exactly
        // when the queue has drained (the host CLI starved that way, and the
        // shell gate had the same flaw).
        if (status.ice_sockets !== undefined && status.ice_sockets !== null) {
          this.iceQueueDepth = status.ice_sockets.queued;
          if (this.iceQueueDepth < ICE_SOCKET_POOL_SIZE && !this.networkDirty && this.connection !== undefined) {
            this.topUpIceBatch().catch((error: Error): void => {
              hilog.warn(DOMAIN, TAG, 'VPN_ICE_TOPUP|rejected|summary=%{public}s', safeError(error));
            });
          }
        }
        if (status.terminal === true) {
          this.handleConnectorDeath(`terminal-state:${status.state}`);
          return;
        }
        // N8 controlled recreate (the platform-VpnConfig-is-fixed gap): when
        // the core's desired route set diverges from the set the shell last
        // ACKed as applied, it raises recreate.required (bounded core-side:
        // budget RECREATE_MAX=3 + 30 s cooldown; level-based until ACKed).
        // The shell then rebuilds the connection ONCE per request —
        // serialized by recreateInFlight, fail-closed on any error.
        if (status.recreate !== undefined && status.recreate !== null &&
          status.recreate.required === true) {
          this.performControlledRecreate(status.recreate.reason).catch((error: Error): void => {
            hilog.error(DOMAIN, TAG,
              'VPN_RECREATE_REJECTED|requestId=%{public}s|summary=%{public}s',
              this.requestId, safeError(error));
            this.failRecreate('unexpected-throw', error);
          });
          return;
        }
        // N2-H I5 (§5.3-1): one-shot development-channel arm check. It never
        // changes a product path — the collector stays inert per §6.1, and an
        // absent/unreadable arm file now costs exactly ONE grep-able
        // N2H_COLLECTOR_ARM_SKIPPED line (I5a §13.15) instead of silence; a
        // terminal connector or a pending recreate has already returned above,
        // so a window never straddles a teardown or a rebuild.
        this.maybeArmN2hCollector(status);
      } else {
        this.statusPollFailures++;
        hilog.warn(DOMAIN, TAG,
          'VPN_CONNECTOR_STATUS_UNREACHABLE|requestId=%{public}s|consecutive=%{public}d|' +
          'limit=%{public}d',
          this.requestId, this.statusPollFailures, CONNECTOR_POLL_FAILURE_LIMIT);
        if (this.statusPollFailures >= CONNECTOR_POLL_FAILURE_LIMIT) {
          this.handleConnectorDeath('connector-status-unreachable');
          return;
        }
      }
      try {
        const net: CoreNetworkConfig = connectorNetworkConfig();
        const serial: number = net.available && net.serial !== undefined ? net.serial : -1;
        if (serial >= 0 && serial !== this.appliedNetSerial &&
          serial !== this.lastUnappliedNetSerial &&
          Date.now() - this.lastUnappliedNetLogAtMs >= 60000) {
          this.lastUnappliedNetSerial = serial;
          this.lastUnappliedNetLogAtMs = Date.now();
          hilog.warn(DOMAIN, TAG,
            'VPN_NETCFG_CHANGED_NOT_APPLIED|requestId=%{public}s|appliedSerial=%{public}d|' +
            'newSerial=%{public}d|reason=platform-vpnconfig-fixed-at-create',
            this.requestId, this.appliedNetSerial, serial);
        }
      } catch (error) {
        hilog.warn(DOMAIN, TAG,
          'VPN_NETCFG_WATCH_REJECTED|requestId=%{public}s|summary=%{public}s',
          this.requestId, safeError(error as Error));
      }
    }, STATUS_POLL_MS);
    // N2-H I5b (§13.16): the watcher's own start beacon, printed here — after
    // the once-guard above (which is the ONLY thing that makes this the first
    // start: `watcherTimer >= 0` from now on) and after the interval exists,
    // but OUTSIDE the tick callback, so no tick can ever re-print it. Together
    // with the onCreate beacon it yields the three-state reading: an arm line
    // ⇒ extension + watcher both live; only beacons ⇒ the watcher never ticked
    // (or the hook was never reached); no beacon at all ⇒ the extension process
    // never started. No latch, no counter, no gate touched.
    hilog.info(DOMAIN, TAG,
      'N2H_COLLECTOR_PATH|requestId=%{public}s|stage=watcher-started|diag=true|is_evidence=false',
      this.requestId);
  }

  // N3-7 gap 3: connector died by itself → tear the VPN down instead of
  // holding a tunnel that no control plane governs (default-route scenarios
  // turn this into a traffic black hole). Idempotent: the
  // connectorDeathHandled latch short-circuits repeat watcher ticks, and the
  // underlying connectorStop()/destroyOnce() are independently idempotent.
  private handleConnectorDeath(reason: string): void {
    if (this.connectorDeathHandled || this.destroyRequested) {
      return;
    }
    this.connectorDeathHandled = true;
    hilog.error(DOMAIN, TAG,
      'VPN_CONNECTOR_DEAD|requestId=%{public}s|reason=%{public}s|action=teardown-vpn',
      this.requestId, reason);
    this.stopStatusAndNetcfgWatcher();
    try {
      const stop = connectorStop();
      hilog.info(DOMAIN, TAG,
        'VPN_CONNECTOR_STOPPED|requestId=%{public}s|alreadyStopped=%{public}s|state=%{public}s|' +
        'cause=connector-death',
        this.requestId, String(stop.already_stopped), stop.state);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_STOP_REJECTED|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error as Error));
    }
    this.destroyOnce(`connector-dead:${reason}`).catch((error: Error) => {
      hilog.error(DOMAIN, TAG,
        'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=connector-death|summary=%{public}s',
        this.requestId, safeError(error));
    });
  }

  // ---------------------------------------------------------------------------
  // N8: controlled recreate — the platform VpnConfig is fixed at create()
  // time, so a desired-route-set change (typically 0.0.0.0/0 released once
  // the WG handshake completes, necessarily AFTER create()) can only reach
  // the tunnel by rebuilding the connection. The core decides WHEN
  // (status.recreate.required, bounded by its own budget + cooldown); the
  // shell executes: snapshot desired config -> destroy old connection (the
  // raw TUN fd dies here — fd contract) -> fresh create() with the desired
  // route set -> fd contract -> feed the NEW TUN fd (core-side
  // replace-while-up keeps the WG sessions and the protected outer socket)
  // -> ACK the new applied set (isRecreate=true). Any failed leg is
  // fail-closed: connector stopped + current generation destroyed — never a
  // half-configured tunnel, never an unprotected fallback.
  // ---------------------------------------------------------------------------

  // N8: initial-create ACK (best-effort — a failed ACK only leaves the
  // recreate signaling silent, i.e. exact N7 behavior; it never fails the
  // start flow).
  private ackAppliedRoutesInitial(): void {
    const routes: string[] | null = this.pendingAppliedRoutes;
    this.pendingAppliedRoutes = null;
    if (routes === null || routes.length === 0) {
      hilog.warn(DOMAIN, TAG,
        'VPN_ROUTE_SET_ACK_SKIPPED|requestId=%{public}s|reason=no-core-snapshot-routes',
        this.requestId);
      return;
    }
    try {
      const ack: RouteSetAppliedResult = connectorRouteSetApplied(routes, false);
      if (!ack.ok) {
        hilog.warn(DOMAIN, TAG,
          'VPN_ROUTE_SET_ACK_FAILED|requestId=%{public}s|error=%{public}s',
          this.requestId, ack.error ?? 'no-message');
        return;
      }
      hilog.info(DOMAIN, TAG,
        'VPN_ROUTE_SET_ACKED|requestId=%{public}s|routes=%{public}d|isRecreate=false',
        this.requestId, routes.length);
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'VPN_ROUTE_SET_ACK_FAILED|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error as Error));
    }
  }

  private async performControlledRecreate(reason: string): Promise<void> {
    if (this.recreateInFlight) {
      hilog.info(DOMAIN, TAG,
        'VPN_RECREATE_SKIP|requestId=%{public}s|reason=already-in-flight', this.requestId);
      return;
    }
    if (this.destroyRequested || this.connectorDeathHandled) {
      return; // teardown already latched — nothing to rebuild
    }
    if (this.connection === undefined || !this.createSettled || this.tunFd < 0) {
      hilog.warn(DOMAIN, TAG,
        'VPN_RECREATE_SKIP|requestId=%{public}s|reason=initial-generation-not-settled',
        this.requestId);
      return;
    }
    this.recreateInFlight = true;
    hilog.info(DOMAIN, TAG,
      'VPN_RECREATE_BEGIN|requestId=%{public}s|reason=%{public}s', this.requestId, reason);
    let stage: string = 'snapshot';
    try {
      if (this.n2Evidence !== undefined) {
        hilog.warn(DOMAIN, TAG,
          'N2_EVIDENCE_RECREATE_DETECTED|requestId=%{public}s|reason=%{public}s|' +
          'diag=true|is_evidence=false', this.requestId, clean(reason));
      }
      // 1. the DESIRED config, fresh from the core (live gate state)
      const net: CoreNetworkConfig = connectorNetworkConfig();
      if (!net.available || net.routes === undefined || net.routes === null ||
        net.routes.length === 0) {
        throw new Error('recreate-no-usable-snapshot');
      }
      // N2-H: graft the endpoint exclusions frozen at THIS session's start
      // onto the rebuilt config — a recreate that dropped them would suck
      // the outer endpoints into the fresh (often default-routed) tunnel.
      // The frozen set is reused verbatim (no DNS inside the bounded
      // recreate leg); the next full start re-freezes with fresh
      // resolutions.
      const recreateTemplate: VpnTunnelConfig = this.n2Evidence !== undefined
        ? withoutN2ExcludedRoutes(loadVpnTunnelConfig()) : loadVpnTunnelConfig();
      const base: VpnTunnelConfig = withEndpointExclusions(recreateTemplate,
        this.frozenEndpointExclusions);
      let merged: VpnTunnelConfig = applyNetworkConfig(base, net);
      const gateAllowed: boolean = net.default_route !== undefined && net.default_route !== null
        ? net.default_route.allowed
        : false;
      merged = gateDefaultRoutes(merged, this.n2Evidence !== undefined ? false : gateAllowed);
      // The RECREATE leg needs the tunnel subnet route too: the platform
      // VpnConfig is fully REPLACED, so a rebuild without it would silently
      // DROP the entry the first generation submitted. Same placement as the
      // initial leg: after the gate, before the generation marker / rows /
      // validation gate below.
      merged = this.injectTunnelSubnetRoute(merged, 'recreate');
      // S6 (2026-09-26, default OFF): the capture /32 must be re-declared on
      // the RECREATE leg too (the platform VpnConfig is fully REPLACED), in
      // the same placement as the tunnel-subnet entry above.
      if (this.diagCaptureRoute) {
        merged = this.injectCaptureRoute(merged, 'recreate');
      }
      if (this.n2Evidence !== undefined) {
        merged = withN2EvidenceRoutes(merged, this.n2Evidence.capture32);
        if (merged.routes.some((route: VpnRouteConfig): boolean => route.isDefaultRoute)) {
          throw new Error('n2-evidence-default-route-present');
        }
        hilog.info(DOMAIN, TAG,
          'N2_EVIDENCE_ROUTES|requestId=%{public}s|leg=recreate|capture32=%{public}s|' +
            'defaultRoutes=0|excludedRoutes=0|diag=true|is_evidence=false',
          this.requestId, this.n2Evidence.capture32.join(','));
      }
      // H1 fail-closed gate on the RECREATE leg too: a snapshot entry that
      // violates the SDK RouteInfo contract must never reach create().
      // Failing BEFORE the destroy leg keeps the current generation intact
      // (same early-throw discipline as the snapshot preconditions above).
      // N2-H I3a (v3.1 §2.1 C2 (1)/(2)): the RECREATE generation is
      // announced BEFORE this leg's create() and before the route rows it
      // announces — `merged` is the config the create below submits verbatim
      // (the destroy leg in between does not touch it). wgPeerEndpoints=0 for
      // the same reason as the initial leg.
      const gen: number =
        this.logExclusionGeneration(EXCLUSION_GENERATION_RECREATE, merged.routes, 0);
      this.logRouteTable(merged, gen);
      const routeCheck: RouteCheckResult = validateRouteEntries(merged.routes, merged.interfaceName);
      if (!routeCheck.ok) {
        this.logRouteViolations(routeCheck.violations);
        stage = 'route-validation';
        throw new Error('recreate-route-entry-invalid');
      }
      // the ACK set is the core-visible managed subset — exactly the
      // networks the core renders as its desired set (NOT the shell-local
      // base exclusions, which the core does not track)
      const routeNetworks: string[] = net.routes.map((r: CoreRouteEntry): string => r.network);
      const defaultRoutes: number = merged.routes.filter(
        (r: VpnRouteConfig): boolean => r.isDefaultRoute).length;
      hilog.info(DOMAIN, TAG,
        'VPN_RECREATE_SNAPSHOT|requestId=%{public}s|serial=%{public}s|routes=%{public}d|' +
        'defaultRoutes=%{public}d|gateAllowed=%{public}s|frozenEndpointExclusions=%{public}d',
        this.requestId, String(net.serial ?? -1), merged.routes.length, defaultRoutes,
        String(gateAllowed), this.frozenEndpointExclusions.length);

      // 2. destroy the OLD connection — the raw TUN fd dies here (fd
      // contract: its close belongs exclusively to VpnConnection.destroy());
      // the data plane keeps running on its own dups until the replacement
      // lands (no route-less window worse than the old config).
      stage = 'destroy';
      const oldConnection: vpnExtension.VpnConnection = this.connection;
      const destroyed: boolean = await this.destroyConnectionBounded(oldConnection);
      if (!destroyed) {
        throw new Error('recreate-destroy-failed');
      }
      if (this.destroyRequested || this.connectorDeathHandled) {
        throw new Error('recreate-interrupted-by-teardown');
      }
      this.connection = undefined;
      this.tunFd = -1;
      this.fdConfirmedOpenAfterCreate = false;

      // 3. a FRESH VpnConnection + create() with the desired route set
      stage = 'create';
      const conn: vpnExtension.VpnConnection = vpnExtension.createVpnConnection(this.context);
      this.connection = conn;
      const config: vpnExtension.VpnConfig =
        buildPlatformVpnConfig(merged, this.diagBlockedApplicationList('recreate'),
          this.n2Evidence !== undefined ? [SELF_BUNDLE_NAME] : undefined);
      const fd: number = await this.createConnectionBounded(conn, config);

      // 4. the same fd contract as the initial create
      stage = 'fd-contract';
      const snapshot: FdSnapshot = this.captureFdSnapshot(fd);
      if (!(snapshot.integer && snapshot.nonnegative && snapshot.probed && snapshot.open)) {
        throw new Error('recreate-invalid-fd');
      }
      this.tunFd = fd;
      this.fdConfirmedOpenAfterCreate = true;
      this.logFdSnapshot('recreate-post-create', snapshot, 'RECREATE_FD_ACCEPTED');
      // N2-H I3a (v3.1 §2.1 C2 (4)): the recreate leg's ACCEPTANCE ANCHOR —
      // this generation's create() was accepted, with the SAME fd-contract
      // acceptance semantics as the initial leg's
      // `VPN_CREATE_RESOLVED|accepted=true`. It is logged after the
      // generation line and every gen==G route row (rule (4)'s ordering) and
      // before the wg-feed/ACK legs (a failure there is the same fail-closed
      // teardown as the initial leg's protect gate). The record of the
      // accepted generation is made at this anchor, so the anchor and the
      // value the arm reports cannot drift (R-2③).
      this.n2hAcceptedExclusionGeneration = gen;
      hilog.info(DOMAIN, TAG,
        'VPN_RECREATE_RESOLVED|requestId=%{public}s|gen=%{public}d|accepted=true',
        this.requestId, gen);

      // 5. replace only the TUN dup. The live core device already owns the
      // protected WG socket dup and retains its sessions across TUN replacement.
      stage = 'tun-feed';
      const tunFeed: WgFeedResult = connectorTunFdFeed(fd);
      if (!tunFeed.ok || tunFeed.device_up !== true) {
        throw new Error(`recreate-tun-feed-failed:${tunFeed.error ?? 'device-not-up'}`);
      }

      // 6. ACK the newly applied route set — completes the recreate (counts
      // against the core-side budget, arms the cooldown, clears `required`)
      stage = 'ack';
      const ack: RouteSetAppliedResult = connectorRouteSetApplied(routeNetworks, true);
      if (!ack.ok) {
        throw new Error(`recreate-ack-failed:${ack.error ?? 'no-message'}`);
      }
      this.appliedNetSerial = net.serial !== undefined && net.serial !== null
        ? net.serial
        : this.appliedNetSerial;
      // final-teardown bookkeeping now points at the NEW generation
      this.createSettled = true;
      this.createPromise = Promise.resolve<CreateOutcome>({
        destroyRequired: true,
        accepted: true,
        reason: 'recreate-created'
      });
      hilog.info(DOMAIN, TAG,
        'VPN_RECREATE_DONE|requestId=%{public}s|tunFd=%{public}d|routes=%{public}d|' +
        'defaultRoutes=%{public}d|recreateCount=%{public}s',
        this.requestId, fd, merged.routes.length, defaultRoutes,
        String(ack.recreate !== undefined && ack.recreate !== null ? ack.recreate.count : -1));
    } catch (error) {
      // fail-closed: ANY failed leg leaves no half-configured device — the
      // connector is stopped and whatever generation is current is destroyed
      this.failRecreate(stage, error as Error);
    } finally {
      this.recreateInFlight = false;
      if (this.networkDirty && !this.destroyRequested && this.networkNetId !== 0) {
        this.recoverPhysicalNetwork().catch((error: Error): void => {
          hilog.warn(DOMAIN, TAG, 'VPN_NETWORK_RECOVERY_FAIL|summary=%{public}s', safeError(error));
        });
      }
    }
  }

  // Bounded destroy of ONE connection generation (recreate leg only).
  // Unlike destroyOnce (the latched FINAL teardown), this is
  // generation-scoped; box expiry reports failure (fail-closed).
  private destroyConnectionBounded(connection: vpnExtension.VpnConnection): Promise<boolean> {
    const settled: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      connection.destroy().then((): void => {
        resolve(true);
      }).catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG,
          'VPN_RECREATE_DESTROY_REJECTED|requestId=%{public}s|summary=%{public}s',
          this.requestId, safeError(error));
        resolve(false);
      });
    });
    const box: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      setTimeout((): void => resolve(false), RECREATE_DESTROY_BOX_MS);
    });
    return Promise.race([settled, box]);
  }

  // Bounded create of the NEW generation (recreate leg). Box expiry rejects
  // (fail-closed — the caller tears everything down).
  private createConnectionBounded(connection: vpnExtension.VpnConnection,
    config: vpnExtension.VpnConfig): Promise<number> {
    const settled: Promise<number> = connection.create(config);
    const box: Promise<number> = new Promise<number>((_resolve: (value: number) => void,
      reject: (e: Error) => void) => {
      setTimeout((): void => {
        reject(new Error('recreate-create-timeout'));
      }, RECREATE_CREATE_BOX_MS);
    });
    return Promise.race([settled, box]);
  }

  // N8 recreate failure = the N3-7 fail-closed teardown: stop the watcher,
  // stop the connector (idempotent), destroy whatever generation is current
  // (latched destroyOnce — the final teardown latch).
  private failRecreate(stage: string, error: Error): void {
    hilog.error(DOMAIN, TAG,
      'VPN_RECREATE_FAIL|requestId=%{public}s|stage=%{public}s|' +
      'action=teardown-vpn-fail-closed|summary=%{public}s',
      this.requestId, stage, safeError(error));
    this.stopStatusAndNetcfgWatcher();
    try {
      const stop = connectorStop();
      hilog.info(DOMAIN, TAG,
        'VPN_CONNECTOR_STOPPED|requestId=%{public}s|alreadyStopped=%{public}s|state=%{public}s|' +
        'cause=recreate-fail-closed',
        this.requestId, String(stop.already_stopped), stop.state);
    } catch (err) {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_STOP_REJECTED|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(err as Error));
    }
    this.destroyOnce(`recreate-fail-closed:${stage}`).catch((err: Error): void => {
      hilog.error(DOMAIN, TAG,
        'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=recreate-fail-closed|summary=%{public}s',
        this.requestId, safeError(err));
    });
  }

  private startSocketSupply(): void {
    if (this.socketSupplyTimer >= 0) {
      return;
    }
    this.socketSupplyTimer = setInterval((): void => {
      if (this.destroyRequested || this.destroyPromise !== undefined) {
        return;
      }
      try {
        const request: SocketSupplyRequest = socketSupplyPoll();
        if (request.id.length > 0) {
          this.fulfillSocketRequest(request).catch((error: Error): void => {
            hilog.error(DOMAIN, TAG, 'VPN_SOCKET_SUPPLY_ERROR|id=%{public}s|summary=%{public}s',
              request.id, safeError(error));
          });
        }
      } catch (error) {
        hilog.error(DOMAIN, TAG, 'VPN_SOCKET_SUPPLY_POLL_FAILED|summary=%{public}s',
          safeError(error as Error));
      }
    }, 100);
  }

  private async fulfillSocketRequest(request: SocketSupplyRequest): Promise<void> {
    const connection: vpnExtension.VpnConnection | undefined = this.connection;
    let fd: number = -1;
    let result: string = 'rejected';
    let reason: string = 'none';
    const t0: number = Date.now();
    const epoch: number = this.networkEpoch;
    try {
      const isDns: boolean = request.kind === 'udp' && request.purpose === 'dns';
      reason = socketSupplyRejectionReason({
        networkDirty: this.networkDirty, networkRecoveryReady: this.networkRecoveryReady,
        networkRecoveryActive: this.networkRecoveryActive, isDns: isDns,
        destroyRequested: this.destroyRequested, destroyInFlight: this.destroyPromise !== undefined,
        recreateInFlight: this.recreateInFlight, hasConnection: this.connection !== undefined,
        connectionUnchanged: this.connection === connection,
        protectDeviation: this.protectDeviationRouteExclusion
      });
      if (reason.length > 0 || connection === undefined) { return; }
      if (!isDns && (request.kind !== 'tcp' || (request.purpose !== 'management' &&
        request.purpose !== 'signal' && request.purpose !== 'relay'))) {
        reason = 'unsupported-request';
        return;
      }
      if (isDns) {
        const servers: string[] = physicalDefaultDnsServers(this.networkNetId || undefined);
        const updated: SocketFeedResult = socketSupplyDnsServers(servers);
        if (!updated.ok || servers.length === 0) {
          reason = 'no-physical-dns';
          return;
        }
      }
      const opened: NativeTcpSocketOpen = isDns ? iceSocketOpen() : mgmtSocketOpen();
      fd = opened.fd;
      if (fd < 0 || opened.bind_rc !== 0) {
        reason = 'socket-open';
        return;
      }
      const protectedOk: boolean = await this.protectSocketWithBox(connection, fd,
        isDns ? 'D' : request.purpose === 'relay' ? 'R' : request.purpose === 'signal' ? 'S' : 'M', request.id);
      const postReason: string = socketSupplyRejectionReason({
        networkDirty: this.networkDirty, networkRecoveryReady: this.networkRecoveryReady,
        networkRecoveryActive: this.networkRecoveryActive, isDns: isDns,
        destroyRequested: this.destroyRequested, destroyInFlight: this.destroyPromise !== undefined,
        recreateInFlight: this.recreateInFlight, hasConnection: this.connection !== undefined,
        connectionUnchanged: this.connection === connection,
        protectDeviation: this.protectDeviationRouteExclusion
      });
      if (protectedOk && epoch === this.networkEpoch && postReason.length === 0) {
        result = 'protected';
      } else {
        result = 'protect-failed';
        reason = epoch !== this.networkEpoch ? 'network-epoch' :
          postReason.length > 0 ? postReason : 'protect-failed';
      }
    } catch (error) {
      reason = 'exception';
      hilog.warn(DOMAIN, TAG, 'VPN_SOCKET_SUPPLY_FAILED|id=%{public}s|summary=%{public}s',
        request.id, safeError(error as Error));
    } finally {
      try {
        const reply: SocketFeedResult = socketSupplyReply(request.id, result === 'protected' ? fd : -1,
          result);
        hilog.info(DOMAIN, TAG,
          'VPN_SOCKET_SUPPLY|id=%{public}s|purpose=%{public}s|kind=%{public}s|fd=%{public}d|' +
          'protect=%{public}s|reason=%{public}s|elapsedMs=%{public}d|accepted=%{public}s',
          request.id, request.purpose ?? 'unknown', request.kind ?? 'unknown', fd,
          result, reason, Date.now() - t0, String(reply.ok));
      } finally {
        if (fd >= 0) {
          socketSupplyCloseFd(fd);
        }
      }
    }
  }

  private stopSocketSupply(): void {
    if (this.socketSupplyTimer >= 0) {
      clearInterval(this.socketSupplyTimer);
      this.socketSupplyTimer = -1;
    }
    socketSupplyShutdown();
    forgetPhysicalDefaultDns();
  }

  // Startup seed; subsequent transport dials also use on-demand protected supply.
  private async feedSpareProtectedSocket(connection: vpnExtension.VpnConnection,
    epoch: number = this.networkEpoch): Promise<boolean> {
    let fd: number = -1;
    try {
      const opened = mgmtSocketOpen();
      fd = opened.fd;
      if (fd < 0 || opened.bind_rc !== 0) {
        hilog.warn(DOMAIN, TAG,
          'VPN_MGMT_SOCKET_FEED_SKIPPED|requestId=%{public}s|stage=open|fd=%{public}d',
          this.requestId, fd);
        return false;
      }
      if (!await this.protectSocketWithBox(connection, fd, 'M') || epoch !== this.networkEpoch ||
        protectedConnectionRejectionReason({
          destroyRequested: this.destroyRequested, destroyInFlight: this.destroyPromise !== undefined,
          recreateInFlight: this.recreateInFlight, hasConnection: this.connection !== undefined,
          connectionUnchanged: this.connection === connection
        }).length > 0 ||
        this.networkDirty && this.networkRecoveryReady &&
        !this.networkRecoveryActive) {
        hilog.warn(DOMAIN, TAG,
          'VPN_MGMT_SOCKET_FEED_SKIPPED|requestId=%{public}s|stage=protect|fd=%{public}d',
          this.requestId, fd);
        return false;
      }
      const fed = connectorSocketFeed(fd);
      hilog.info(DOMAIN, TAG,
        'VPN_MGMT_SOCKET_FED|requestId=%{public}s|fd=%{public}d|ok=%{public}s|queued=%{public}s|' +
        'error=%{public}s',
        this.requestId, fd, String(fed.ok), String(fed.queued ?? -1), fed.error ?? 'none');
      return fed.ok;
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'VPN_MGMT_SOCKET_FEED_SKIPPED|requestId=%{public}s|stage=throw|summary=%{public}s',
        this.requestId, safeError(error as Error));
      return false;
    } finally {
      if (fd >= 0) { socketSupplyCloseFd(fd); }
    }
  }

  private stopStatusAndNetcfgWatcher(): void {
    this.stopSocketSupply();
    if (this.watcherTimer >= 0) {
      clearInterval(this.watcherTimer);
      this.watcherTimer = -1;
    }
    // S5: this is teardown (connector death, fail-closed recreate teardown,
    // onDestroy), so the optional burst survey series is cancelled and its
    // handle cleared here — independently of the app window close edge, which
    // no longer stops it.
    this.stopInterfaceStatsBurst();
  }

  private logConfigApplied(config: VpnTunnelConfig): void {
    let defaultRoutes: number = 0;
    let excludedRoutes: number = 0;
    for (let i = 0; i < config.routes.length; i++) {
      if (config.routes[i].isDefaultRoute) {
        defaultRoutes++;
      }
      if (config.routes[i].isExcludedRoute) {
        excludedRoutes++;
      }
    }
    hilog.info(DOMAIN, TAG,
      'VPN_CONFIG_APPLIED|requestId=%{public}s|address=%{public}s/%{public}d|interface=%{public}s|' +
      'routes=%{public}d|defaultRoutes=%{public}d|excludedRoutes=%{public}d|dns=%{public}s|' +
      'mtu=%{public}d|endpoint=%{public}s',
      this.requestId, config.tunnelAddress, config.tunnelPrefixLength, config.interfaceName,
      config.routes.length, defaultRoutes, excludedRoutes, config.dnsServers.join(','), config.mtu,
      config.peerEndpoint);
    // The peer endpoint feeds the native WireGuard data plane, not the
    // platform VpnConfig. TODO: native connect through the protected socket
    // is not wired yet (Rust core data-plane export pending) — 未验证.
  }

  // N2-H I3a (v3.1 §2.1 C2 (1)/(2), R-1): exactly ONE
  // VPN_EXCLUSION_GENERATION line per create(), logged BEFORE that create()
  // and BEFORE the route rows it announces. `routes` MUST be the very list
  // handed to buildPlatformVpnConfig/create() — the announced counts are
  // computed from it, never estimated. Returns the generation, so the caller
  // stamps the acceptance anchor with the same number.
  private logExclusionGeneration(kind: string, routes: VpnRouteConfig[],
    wgPeerEndpoints: number): number {
    this.n2hExclusionGeneration++;
    const gen: number = this.n2hExclusionGeneration;
    hilog.info(DOMAIN, TAG, '%{public}s',
      formatExclusionGeneration(this.requestId, gen, kind, routes, wgPeerEndpoints));
    return gen;
  }

  // H1 pre-create truth dump (diagnosis §5 T0 / §6-4): every route entry
  // with its per-field runtime truth, BEFORE connection.create() — one
  // hilog line each (header + one per entry). Field names and route values
  // only; never tokens or credentials.
  // N2-H I3a: every entry line carries `gen=<n>`, the generation of the
  // create() it is about to be submitted to (v3.1 §2.1 C2 (2)) — always the
  // value returned by the logExclusionGeneration call immediately above.
  private logRouteTable(config: VpnTunnelConfig, gen: number): void {
    const lines: string[] = formatRouteTable(this.requestId, config.interfaceName,
      config.routes, gen);
    for (let i = 0; i < lines.length; i++) {
      hilog.info(DOMAIN, TAG, '%{public}s', lines[i]);
    }
  }

  private logRouteViolations(violations: RouteViolation[]): void {
    for (let i = 0; i < violations.length; i++) {
      hilog.error(DOMAIN, TAG, '%{public}s', formatRouteViolation(this.requestId, violations[i]));
    }
  }

  // Post-create settle: TUN fd contract check, then the fail-closed protect
  // gate. Any protect-path failure destroys the VPN and never starts traffic
  // (governance item 4). `gen` is the generation logged for THIS create()
  // (logExclusionGeneration) — the acceptance anchor below is what makes it
  // the evidence generation G of §2.1 C2 (4).
  private async settleCreate(fd: number, gen: number): Promise<CreateOutcome> {
    this.createSettled = true;
    this.tunFd = fd;
    const snapshot: FdSnapshot = this.captureFdSnapshot(fd);
    const accepted: boolean = snapshot.integer && snapshot.nonnegative && snapshot.probed && snapshot.open;
    const marker: string = accepted ? 'CREATE_ACCEPTED' : 'CREATE_INVALID_DESTROY_REQUIRED';
    this.fdConfirmedOpenAfterCreate = accepted;
    this.logFdSnapshot('post-create', snapshot, marker);

    hilog.info(DOMAIN, TAG,
      'VPN_CREATE_RESOLVED|requestId=%{public}s|fd=%{public}s|accepted=%{public}s|marker=%{public}s',
      this.requestId, String(fd), String(accepted), marker);
    if (accepted) {
      // N2-H I3a (v3.1 §2.1 C2 (4) / R-2③): the initial leg's ACCEPTANCE
      // ANCHOR (`VPN_CREATE_RESOLVED|accepted=true`) is where this generation
      // becomes the accepted one — the same value the arm later reports as G.
      // Recorded here, at the anchor itself, so nothing can drift between the
      // marker, the evidence document field and the generation line.
      this.n2hAcceptedExclusionGeneration = gen;
    } else {
      hilog.error(DOMAIN, TAG,
        'VPN_CREATE_INVALID_FD|requestId=%{public}s|fd=%{public}s|marker=CREATE_INVALID_DESTROY_REQUIRED',
        this.requestId, String(fd));
      this.destroyOnce('invalid-create-resolution').catch((error: Error) => {
        hilog.error(DOMAIN, TAG,
          'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=invalid-create-unexpected|summary=%{public}s',
          this.requestId, safeError(error));
      });
      return {
        destroyRequired: true,
        accepted: false,
        reason: 'create-resolved-invalid-or-not-open'
      };
    }

    // S3 (diag, default off): ONE fire-and-forget protectProcessNet() call per
    // session, issued here — AFTER the acceptance anchor above (so a create()
    // that was not accepted never calls it) and BEFORE the fail-closed
    // per-socket protect gate below (so the platform's whole-process
    // exclusion, if it works, is in place for the outer socket gate that
    // follows). The latch (processProtectAttempted) makes it once-only even
    // across a recreate that settles again; the result is logged by
    // protectProcessNetWithBox and changes NO gate — a rejection, a throw or
    // the box expiring is not a failure of this flow (the per-socket gate
    // below is still the deciding one).
    //
    // On the tested device, protectProcessNet() resolved in the diagnostic
    // round. This switch remains diagnostic; the per-socket gate decides.
    if (this.diagProtectProcessNet && !this.processProtectAttempted) {
      const processConnection: vpnExtension.VpnConnection | undefined = this.connection;
      if (processConnection !== undefined) {
        this.processProtectAttempted = true;
        this.protectProcessNetWithBox(processConnection).catch((error: Error): void => {
          // Last resort only: the box inside never rejects (every outcome is
          // caught there). An unexpected rejection must not surface as an
          // unhandled rejection in the extension process.
          hilog.warn(DOMAIN, TAG,
            'VPN_PROTECT_PROCESS_NET|requestId=%{public}s|result=unexpected-rejection|' +
            'summary=%{public}s|switch=s3|diag=true|is_evidence=false',
            this.requestId, safeError(error));
        });
      }
    }

    // Fail-closed protect gate: the tunnel-outer socket must be protected
    // before any datagram can flow; failure destroys the VPN.
    const protectedOk: boolean = await this.protectOutboundSocketFailClosed();
    if (!protectedOk) {
      this.destroyOnce('protect-fail-closed').catch((error: Error) => {
        hilog.error(DOMAIN, TAG,
          'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=protect-fail-closed|summary=%{public}s',
          this.requestId, safeError(error));
      });
      return {
        destroyRequired: true,
        accepted: true,
        reason: 'protect-fail-closed'
      };
    }

    // N7 fail-closed WG data-plane feed: hand the PROTECTED outer UDP socket
    // and the TUN fd to the core (dup copies only) so the real WireGuard
    // device comes up. ANY feed failure ⇒ no data plane ⇒ default route
    // stays held ⇒ connector stopped + VPN destroyed (never a half-wired
    // tunnel, never an unprotected fallback).
    const fedOk: boolean = this.feedWgDataplane();
    if (!fedOk) {
      try {
        const stop = connectorStop();
        hilog.info(DOMAIN, TAG,
          'VPN_CONNECTOR_STOPPED|requestId=%{public}s|alreadyStopped=%{public}s|state=%{public}s|' +
          'cause=wg-dataplane-feed-fail-closed',
          this.requestId, String(stop.already_stopped), stop.state);
      } catch (error) {
        hilog.error(DOMAIN, TAG,
          'VPN_CONNECTOR_STOP_REJECTED|requestId=%{public}s|summary=%{public}s',
          this.requestId, safeError(error as Error));
      }
      this.destroyOnce('wg-dataplane-feed-fail-closed').catch((error: Error) => {
        hilog.error(DOMAIN, TAG,
          'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=wg-feed-fail-closed|summary=%{public}s',
          this.requestId, safeError(error));
      });
      return {
        destroyRequired: true,
        accepted: true,
        reason: 'wg-dataplane-feed-fail-closed'
      };
    }

    // N5c/N7 device gap: provision the ICE UDP socket (core-opened unbound
    // UDP → shell protect gate → connectorIceSocketFeed). The protected-fd
    // rule of §二.4 covers ICE/STUN as explicitly as it covers management:
    // without a fed socket the core's candidate gathering fails CLOSED and
    // ICE stays idle (device run 1: 61x "peer-conn: pump error (network)").
    // Failure here is the same fail-closed teardown as the WG feed.
    const iceOk: boolean = await this.protectAndFeedIceSocket();
    if (!iceOk) {
      try {
        const stop = connectorStop();
        hilog.info(DOMAIN, TAG,
          'VPN_CONNECTOR_STOPPED|requestId=%{public}s|alreadyStopped=%{public}s|state=%{public}s|' +
          'cause=ice-socket-feed-fail-closed',
          this.requestId, String(stop.already_stopped), stop.state);
      } catch (error) {
        hilog.error(DOMAIN, TAG,
          'VPN_CONNECTOR_STOP_REJECTED|requestId=%{public}s|summary=%{public}s',
          this.requestId, safeError(error as Error));
      }
      this.destroyOnce('ice-socket-feed-fail-closed').catch((error: Error) => {
        hilog.error(DOMAIN, TAG,
          'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=ice-feed-fail-closed|summary=%{public}s',
          this.requestId, safeError(error));
      });
      return {
        destroyRequired: true,
        accepted: true,
        reason: 'ice-socket-feed-fail-closed'
      };
    }

    // VPN is up (create accepted + protect passed + WG data plane fed):
    // N8 — ACK the route set applied to the platform VpnConfig (initial
    // create; arms the core's desired-vs-applied recreate comparison), then
    // start the status / network-config watch. Runtime map changes are
    // logged as NOT applied (platform VpnConfig fixed at create — see the
    // watcher); a desired-route-set CHANGE goes through the controlled
    // recreate below.
    this.networkRecoveryReady = true;
    if (this.networkEpoch > 0) {
      this.recoverPhysicalNetwork().catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG, 'VPN_NETWORK_RECOVERY_FAIL|summary=%{public}s', safeError(error));
      });
    }
    this.ackAppliedRoutesInitial();
    this.startStatusAndNetcfgWatcher();
    return {
      destroyRequired: true,
      accepted: true,
      reason: 'create-open-fd-confirmed-protect-ok-wg-fed'
    };
  }

  // N7 WG data-plane feed (protected socket → TUN fd → device up). Order is
  // mandatory: the outer UDP socket must ALREADY be protected here (§二.4 —
  // protect before any datagram flows; the WG device sends the handshake the
  // moment the endpoint lands), and the TUN fd is the second of the two
  // feeds. Native duplicates and owns the WG socket at feed time; the shell
  // closes its original immediately. The platform owns the original TUN fd,
  // which native duplicates when constructing or replacing the device.
  // Returns true only when the core reports device_up.
  private feedWgDataplane(): boolean {
    if (this.outboundSocketFd < 0 || !this.outboundProtected || this.tunFd < 0) {
      hilog.error(DOMAIN, TAG,
        'VPN_WG_FEED_FAIL|requestId=%{public}s|stage=precondition|wgFd=%{public}d|' +
        'protected=%{public}s|tunFd=%{public}d',
        this.requestId, this.outboundSocketFd, String(this.outboundProtected), this.tunFd);
      if (this.outboundSocketFd >= 0) {
        socketSupplyCloseFd(this.outboundSocketFd);
        this.outboundSocketFd = -1;
      }
      return false;
    }
    // 1. the protected WG outer socket (data plane feed #1)
    const wgFd: number = this.outboundSocketFd;
    let sock: WgFeedResult;
    try {
      sock = connectorWgSocketFeed(wgFd);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_WG_FEED_FAIL|requestId=%{public}s|stage=wg-socket-throw|fd=%{public}d|summary=%{public}s',
        this.requestId, wgFd, safeError(error as Error));
      return false;
    } finally {
      socketSupplyCloseFd(wgFd);
      this.outboundSocketFd = -1;
    }
    if (!sock.ok) {
      hilog.error(DOMAIN, TAG,
        'VPN_WG_FEED_FAIL|requestId=%{public}s|stage=wg-socket|fd=%{public}d|error=%{public}s',
        this.requestId, wgFd, sock.error ?? 'no-message');
      return false;
    }
    // 2. the platform TUN fd (data plane feed #2) — device comes up here
    let tun: WgFeedResult;
    try {
      tun = connectorTunFdFeed(this.tunFd);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_WG_FEED_FAIL|requestId=%{public}s|stage=tun-throw|fd=%{public}d|summary=%{public}s',
        this.requestId, this.tunFd, safeError(error as Error));
      return false;
    }
    if (!tun.ok) {
      hilog.error(DOMAIN, TAG,
        'VPN_WG_FEED_FAIL|requestId=%{public}s|stage=tun|fd=%{public}d|error=%{public}s',
        this.requestId, this.tunFd, tun.error ?? 'no-message');
      return false;
    }
    if (tun.device_up !== true) {
      // a feed can report ok while the pair is still incomplete (e.g. the
      // socket feed was accepted but this tun feed somehow did not complete
      // the pair) — treat anything but an explicit device_up as failure
      hilog.error(DOMAIN, TAG,
        'VPN_WG_FEED_FAIL|requestId=%{public}s|stage=device-not-up|wgFd=%{public}d|tunFd=%{public}d',
        this.requestId, wgFd, this.tunFd);
      return false;
    }
    hilog.info(DOMAIN, TAG,
      'VPN_WG_DATAPLANE_FED|requestId=%{public}s|wgFd=%{public}d|tunFd=%{public}d|deviceUp=true',
      this.requestId, wgFd, this.tunFd);
    return true;
  }

  // ControlProtectSocket equivalent on the shell side: protect ONE socket,
  // before it is used, fail-closed. Verified shape per spike e8e2cf9:
  // native opens+binds the UDP socket alone (core.wg_fwd_open), the shell
  // protects that fd via VpnConnection.protect (bounded 5 s box), then the
  // native data plane adopts the SAME fd. Every future outbound socket
  // (management/signal/relay/ICE/DNS outer) must go through this gate before
  // its first connect and after every reconnect.
  private async protectOutboundSocketFailClosed(): Promise<boolean> {
    const connection: vpnExtension.VpnConnection | undefined = this.connection;
    if (connection === undefined) {
      hilog.error(DOMAIN, TAG,
        'VPN_PROTECT_FAIL_CLOSED|requestId=%{public}s|stage=connection-missing', this.requestId);
      return false;
    }

    let fd: number = -1;
    let bindRc: number = -1;
    let bindErrno: number = -1;
    try {
      const opened: NativeSocketOpen = JSON.parse(core.wg_fwd_open()) as NativeSocketOpen;
      fd = opened.fd;
      bindRc = opened.bind_rc;
      bindErrno = opened.bind_errno;
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_PROTECT_FAIL_CLOSED|requestId=%{public}s|stage=outbound-open-throw|summary=%{public}s',
        this.requestId, safeError(error as Error));
      return false;
    }
    if (fd < 0 || bindRc !== 0) {
      if (fd >= 0) { socketSupplyCloseFd(fd); }
      hilog.error(DOMAIN, TAG,
        'VPN_PROTECT_FAIL_CLOSED|requestId=%{public}s|stage=outbound-open|fd=%{public}d|' +
        'bindRc=%{public}d|bindErrno=%{public}d',
        this.requestId, fd, bindRc, bindErrno);
      return false;
    }
    // Keep this original local across await: destroy audits only published fds.
    // A late protect result after teardown must never publish an orphan fd.
    hilog.info(DOMAIN, TAG,
      'VPN_PROTECT_BEGIN|requestId=%{public}s|fd=%{public}d', this.requestId, fd);

    const protectedOk: boolean = await this.protectSocketWithBox(connection, fd, 'Wb');
    if (!protectedOk || this.destroyRequested || this.destroyPromise !== undefined ||
      this.connection !== connection) {
      socketSupplyCloseFd(fd);
      hilog.error(DOMAIN, TAG,
        'VPN_PROTECT_FAIL_CLOSED|requestId=%{public}s|stage=protect|fd=%{public}d', this.requestId, fd);
      return false;
    }
    this.outboundSocketFd = fd;
    this.outboundProtected = true;
    hilog.info(DOMAIN, TAG,
      'VPN_PROTECT_RESOLVED|requestId=%{public}s|fd=%{public}d|datagramsFlown=none-yet', this.requestId, fd);
    return true;
  }

  // N5c/N7 device gap: ICE UDP socket provisioning (core-open → protect →
  // feed). Mirrors protectOutboundSocketFailClosed for the ICE seam: each fd
  // is core-opened UNBOUND (ice_socket_open — binding here would pick the port
  // before the interface is known), protected BEFORE the ICE consumer binds a
  // dup copy, then handed over. A POOL is fed because every interface round
  // consumes one socket (see feedOneIceSocket). Failure of the whole pool is
  // fail-closed; a partial pool is logged and the gather simply has fewer
  // interfaces to work with.
  private async protectAndFeedIceSocket(): Promise<boolean> {
    const connection: vpnExtension.VpnConnection | undefined = this.connection;
    if (connection === undefined) {
      hilog.error(DOMAIN, TAG,
        'VPN_ICE_FEED_FAIL|requestId=%{public}s|stage=connection-missing', this.requestId);
      return false;
    }
    let fedCount: number = 0;
    for (let i = 0; i < ICE_SOCKET_POOL_SIZE; i++) {
      const ok: boolean = await this.feedOneIceSocket(connection);
      if (ok) {
        fedCount++;
      }
    }
    if (fedCount === 0) {
      hilog.error(DOMAIN, TAG,
        'VPN_ICE_FEED_FAIL|requestId=%{public}s|stage=pool|fed=0|size=%{public}d',
        this.requestId, ICE_SOCKET_POOL_SIZE);
      return false;
    }
    hilog.warn(DOMAIN, TAG,
      'VPN_ICE_POOL|fed=%{public}d|size=%{public}d', fedCount, ICE_SOCKET_POOL_SIZE);
    this.startIceSocketTopUp();
    return true;
  }

  // Bounded top-up of the ICE socket pool: a failed session is renegotiated
  // after a cooldown and needs fresh sockets again (device run 3: the retry
  // hit "no-protected-socket" forever because the pool was one-shot).
  private startIceSocketTopUp(): void {
    if (this.iceTopUpTimer !== -1) {
      return;
    }
    let given: number = 0;
    this.iceTopUpTimer = setInterval((): void => {
      if (this.destroyRequested) {
        clearInterval(this.iceTopUpTimer);
        this.iceTopUpTimer = -1;
        return;
      }
      if (this.iceQueueDepth >= ICE_SOCKET_POOL_SIZE || this.iceTopUpActive || this.networkDirty) {
        return;
      }
      this.topUpIceBatch().then((fed: number): void => {
        given += fed;
        if (fed === 0) {
          hilog.warn(DOMAIN, TAG, 'VPN_ICE_TOPUP|feed-failed|given=%{public}d', given);
        }
      }).catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG, 'VPN_ICE_TOPUP|rejected|summary=%{public}s', safeError(error));
      });
    }, ICE_SOCKET_TOPUP_MS);
  }

  // Feed up to ICE_TOPUP_BATCH sockets while the live queue is below target.
  // One gather round needs one socket per IPv4 interface.
  private async topUpIceBatch(): Promise<number> {
    const connection: vpnExtension.VpnConnection | undefined = this.connection;
    if (this.iceTopUpActive || this.networkDirty ||
      protectedConnectionRejectionReason({
        destroyRequested: this.destroyRequested, destroyInFlight: this.destroyPromise !== undefined,
        recreateInFlight: this.recreateInFlight, hasConnection: connection !== undefined,
        connectionUnchanged: this.connection === connection
      }).length > 0 || connection === undefined) { return 0; }
    this.iceTopUpActive = true;
    let fed: number = 0;
    try {
      for (let n = 0; n < ICE_TOPUP_BATCH && this.iceQueueDepth < ICE_SOCKET_POOL_SIZE; n++) {
        const ok: boolean = await this.feedOneIceSocket(connection);
        if (!ok) { break; }
        fed++;
      }
    } finally {
      this.iceTopUpActive = false;
    }
    return fed;
  }

  // Publish the live connector status for the UI process (see the call site).
  // Best-effort: a failure here must never disturb the VPN.
  private publishStatusForUi(status: ConnectorStatusResult): void {
    const path: string = `${this.context.filesDir}/netbird-status.json`;
    let file: fs.File | undefined = undefined;
    try {
      const errorClass: string =
        status.last_error !== undefined && status.last_error !== null
          ? status.last_error.class
          : 'none';
      const signalRegistered: boolean =
        status.signal !== undefined && status.signal !== null ? status.signal.registered : false;
      const signalReconnects: number =
        status.signal !== undefined && status.signal !== null ? status.signal.reconnects : 0;
      const wgReady: boolean =
        status.wg !== undefined && status.wg !== null ? status.wg.ready : false;
      const wgSessions: number =
        status.wg !== undefined && status.wg !== null ? status.wg.peers_with_session : 0;
      const doc: Record<string, Object> = {
        'updated_at_ms': Date.now(),
        'request_id': this.requestId,
        'state': status.state,
        'peer_count': status.peer_count,
        'route_count': status.route_count,
        'last_error_class': errorClass,
        'signal_registered': signalRegistered,
        'signal_reconnects': signalReconnects,
        'wg_ready': wgReady,
        'wg_sessions': wgSessions
      };
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      fs.writeSync(file.fd, JSON.stringify(doc));
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'VPN_STATUS_PUBLISH_FAIL|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error as Error));
    } finally {
      if (file !== undefined) {
        try {
          fs.closeSync(file);
        } catch (closeError) {
          // best-effort document: nothing to recover
        }
      }
    }
  }

  // Feed ONE ICE socket unless the pool is already full. Each ICE interface  // round needs a FRESH unbound UDP socket (dups share one socket object, so a
  // bound copy cannot be re-bound for the next interface), and the device has
  // several IPv4 interfaces — a single socket starves the gather right after
  // the first round (device-verified 2026-09-14: "protected-udp:
  // no-protected-socket" every 2 s, ICE never left idle).
  private async feedOneIceSocket(connection: vpnExtension.VpnConnection): Promise<boolean> {
    if (protectedConnectionRejectionReason({
      destroyRequested: this.destroyRequested, destroyInFlight: this.destroyPromise !== undefined,
      recreateInFlight: this.recreateInFlight, hasConnection: this.connection !== undefined,
      connectionUnchanged: this.connection === connection
    }).length > 0) { return false; }
    let fd: number = -1;
    try {
      const opened: NativeTcpSocketOpen = iceSocketOpen();
      fd = opened.fd;
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_ICE_FEED_FAIL|requestId=%{public}s|stage=open-throw|summary=%{public}s',
        this.requestId, safeError(error as Error));
      return false;
    }
    if (fd < 0) {
      hilog.error(DOMAIN, TAG,
        'VPN_ICE_FEED_FAIL|requestId=%{public}s|stage=open|fd=%{public}d', this.requestId, fd);
      return false;
    }
    const epoch: number = this.networkEpoch;
    try {
      if (!await this.protectSocketWithBox(connection, fd, 'I') || epoch !== this.networkEpoch ||
        protectedConnectionRejectionReason({
          destroyRequested: this.destroyRequested, destroyInFlight: this.destroyPromise !== undefined,
          recreateInFlight: this.recreateInFlight, hasConnection: this.connection !== undefined,
          connectionUnchanged: this.connection === connection
        }).length > 0 ||
        this.networkDirty && this.networkRecoveryReady &&
        !this.networkRecoveryActive) {
        hilog.error(DOMAIN, TAG,
          'VPN_ICE_FEED_FAIL|requestId=%{public}s|stage=protect-or-stale|fd=%{public}d',
          this.requestId, fd);
        return false;
      }
      this.iceProtected = true;
      let fed: SocketFeedResult;
      try {
        fed = connectorIceSocketFeed(fd);
      } catch (error) {
        hilog.error(DOMAIN, TAG,
          'VPN_ICE_FEED_FAIL|requestId=%{public}s|stage=feed-throw|fd=%{public}d|summary=%{public}s',
          this.requestId, fd, safeError(error as Error));
        return false;
      }
      if (!fed.ok) {
        hilog.error(DOMAIN, TAG,
          'VPN_ICE_FEED_FAIL|requestId=%{public}s|stage=feed|fd=%{public}d|error=%{public}s',
          this.requestId, fd, fed.error ?? 'no-message');
        return false;
      }
      hilog.warn(DOMAIN, TAG,
        'VPN_ICE_FEED|fed|fd=%{public}d|queue=%{public}d', fd, fed.queued ?? -1);
      this.iceQueueDepth = fed.queued ?? this.iceQueueDepth;
      return true;
    } finally {
      socketSupplyCloseFd(fd);
    }
  }

  // Bounded VpnConnection.protect() box (same shape as the verified spike
  // helper protectSocketWithBox, e8e2cf9). Never throws. Box expiry is
  // reported as failure (fail-closed) — stricter than the spike's
  // best-effort treatment, per governance item 4.
  private async protectSocketWithBox(connection: vpnExtension.VpnConnection, fd: number,
    traceKind: string = '', traceId: string = this.requestId): Promise<boolean> {
    if (this.connection !== connection || this.recreateInFlight || this.destroyRequested ||
      this.destroyPromise !== undefined) {
      return false;
    }
    const t0: number = Date.now();
    if (this.n2Evidence !== undefined && traceKind.length > 0) {
      hilog.info(DOMAIN, TAG,
        'N2_SOCK|phase=open|kind=%{public}s|request_id=%{public}s|fd=%{public}d|' +
        'generation=%{public}d|net_id=%{public}d|wall_ms=%{public}d|diag=true|is_evidence=false',
        traceKind, traceId, fd, this.networkEpoch, this.networkNetId, t0);
      hilog.info(DOMAIN, TAG,
        'N2_SOCK|phase=protect_begin|kind=%{public}s|request_id=%{public}s|fd=%{public}d|' +
        'generation=%{public}d|net_id=%{public}d|wall_ms=%{public}d|diag=true|is_evidence=false',
        traceKind, traceId, fd, this.networkEpoch, this.networkNetId, t0);
    }
    // On the tested device, per-fd protect() resolved despite MANAGE_VPN
    // check-failure log lines. The dev-only deviation skips this call when
    // armed; the normal path protects each outer socket before core handoff.
    if (this.protectDeviationRouteExclusion) {
      hilog.warn(DOMAIN, TAG,
        'VPN_PROTECT_DEVIATION|requestId=%{public}s|fd=%{public}d|mode=route-exclusion|' +
        'reason=operator-opt-in|platform_call=not-attempted|' +
        'is_evidence=false|not_a_gate_pass=true',
        this.requestId, fd);
      return this.connection === connection && !this.recreateInFlight && !this.destroyRequested &&
        this.destroyPromise === undefined;
    }
    // The box timer is a bounded WAIT, not a scheduled event: once the platform
    // call settles (resolve or reject) the handle is cleared, so a successful
    // protect() no longer prints a second, false VPN_PROTECT_TIMEOUT line 5 s
    // later. The box itself is unchanged — it still resolves false on expiry,
    // and an expiry that decided the race is still reported as failure.
    let boxTimer: number = -1;
    const settled: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      connection.protect(fd).then((): void => {
        clearTimeout(boxTimer);
        hilog.info(DOMAIN, TAG,
          'VPN_PROTECT_SETTLED|requestId=%{public}s|fd=%{public}d|elapsedMs=%{public}d',
          this.requestId, fd, Date.now() - t0);
        resolve(true);
      }).catch((error: Error): void => {
        clearTimeout(boxTimer);
        hilog.warn(DOMAIN, TAG,
          'VPN_PROTECT_REJECTED|requestId=%{public}s|fd=%{public}d|elapsedMs=%{public}d|summary=%{public}s',
          this.requestId, fd, Date.now() - t0, safeError(error));
        resolve(false);
      });
    });
    const box: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      boxTimer = setTimeout((): void => {
        hilog.warn(DOMAIN, TAG,
          'VPN_PROTECT_TIMEOUT|requestId=%{public}s|fd=%{public}d|boxMs=%{public}d|stillPending=true',
          this.requestId, fd, PROTECT_BOX_MS);
        resolve(false);
      }, PROTECT_BOX_MS);
    });
    const outcome: boolean = await Promise.race([settled, box]);
    if (this.n2Evidence !== undefined && traceKind.length > 0) {
      const elapsed: number = Date.now() - t0;
      hilog.info(DOMAIN, TAG,
        'N2_SOCK|phase=protect_settle|kind=%{public}s|request_id=%{public}s|fd=%{public}d|' +
        'result=%{public}s|elapsed_ms=%{public}d|generation=%{public}d|net_id=%{public}d|' +
        'wall_ms=%{public}d|diag=true|is_evidence=false',
        traceKind, traceId, fd, outcome ? 'success' : 'failed', elapsed,
        this.networkEpoch, this.networkNetId, Date.now());
      core.n2_socket_trace_protect(fd, traceKind,
        `${traceId},${this.networkEpoch},${this.networkNetId},${elapsed}`, outcome);
    }
    if (!outcome && this.protectDeviationRouteExclusion) {
      hilog.warn(DOMAIN, TAG,
        'VPN_PROTECT_DEVIATION|requestId=%{public}s|fd=%{public}d|mode=route-exclusion|' +
        'reason=operator-opt-in|is_evidence=false|not_a_gate_pass=true',
        this.requestId, fd);
      return this.connection === connection && !this.recreateInFlight && !this.destroyRequested &&
        this.destroyPromise === undefined;
    }
    return outcome && this.connection === connection && !this.recreateInFlight && !this.destroyRequested &&
        this.destroyPromise === undefined;
  }

  // Bounded process-level protect (protectProcessNet, API 22+ / SDK
  // @ohos.net.vpnExtension.d.ts:275 `protectProcessNet(): Promise<void>`).
  // Same house rule: every ArkTS wait is bounded; expiry = the platform call
  // did not answer and the per-fd path takes over. Since 2026-09-23 this
  // wrapper has exactly ONE call site: the S3 diagnostic switch
  // (settleCreate), so every line below carries the fixed
  // `switch=s3|diag=true|is_evidence=false` tail and the result NEVER changes
  // a gate — it is recorded and nothing else.
  private async protectProcessNetWithBox(connection: vpnExtension.VpnConnection): Promise<boolean> {
    const t0: number = Date.now();
    // The box timer is cancelled as soon as the platform call settles —
    // resolve, reject or the synchronous throw below — so a settled call
    // leaves no pending 5 s callback behind. `raceSettled` stays as the
    // independent second guard: it makes the timeout line mean "the box
    // decided this race" instead of "a timer went off", so even a callback
    // that somehow survived cancellation can never print a second, false
    // `result=timeout` line — indistinguishable from a real wedge in the
    // device evidence. The race outcome itself is unchanged (the box still
    // resolves false; a settled race simply ignores it).
    let raceSettled: boolean = false;
    let boxTimer: number = -1;
    const settled: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      try {
        connection.protectProcessNet().then((): void => {
          raceSettled = true;
          clearTimeout(boxTimer);
          hilog.info(DOMAIN, TAG,
            'VPN_PROTECT_PROCESS_NET|requestId=%{public}s|result=resolved|elapsedMs=%{public}d|' +
            'switch=s3|diag=true|is_evidence=false',
            this.requestId, Date.now() - t0);
          resolve(true);
        }).catch((error: Error): void => {
          raceSettled = true;
          clearTimeout(boxTimer);
          hilog.warn(DOMAIN, TAG,
            'VPN_PROTECT_PROCESS_NET|requestId=%{public}s|result=rejected|elapsedMs=%{public}d|' +
            'summary=%{public}s|switch=s3|diag=true|is_evidence=false',
            this.requestId, Date.now() - t0, safeError(error));
          resolve(false);
        });
      } catch (error) {
        raceSettled = true;
        clearTimeout(boxTimer);
        hilog.warn(DOMAIN, TAG,
          'VPN_PROTECT_PROCESS_NET|requestId=%{public}s|result=throw|summary=%{public}s|' +
          'switch=s3|diag=true|is_evidence=false',
          this.requestId, safeError(error as Error));
        resolve(false);
      }
    });
    const box: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      boxTimer = setTimeout((): void => {
        if (!raceSettled) {
          hilog.warn(DOMAIN, TAG,
            'VPN_PROTECT_PROCESS_NET|requestId=%{public}s|result=timeout|boxMs=%{public}d|' +
            'switch=s3|diag=true|is_evidence=false',
            this.requestId, PROTECT_BOX_MS);
        }
        resolve(false);
      }, PROTECT_BOX_MS);
    });
    return await Promise.race([settled, box]);
  }

  onDestroy(): void {
    hilog.info(DOMAIN, TAG, 'VPN_ONDESTROY|requestId=%{public}s', this.requestId);
    // Stop flow (N3-6): latch first (the async start flow must not create
    // past this point), stop the watch, stop the connector (idempotent,
    // safe with no connector), then the exactly-once VPN destroy — the
    // destroy semantics and fd contract are unchanged.
    this.destroyRequested = true;
    this.stopStatusAndNetcfgWatcher();
    try {
      const stop = connectorStop();
      hilog.info(DOMAIN, TAG,
        'VPN_CONNECTOR_STOPPED|requestId=%{public}s|alreadyStopped=%{public}s|state=%{public}s',
        this.requestId, String(stop.already_stopped), stop.state);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_STOP_REJECTED|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error as Error));
    }
    this.destroyOnce('onDestroy').catch((error: Error) => {
      hilog.error(DOMAIN, TAG,
        'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=unexpected|summary=%{public}s',
        this.requestId, safeError(error));
    });
  }

  private captureFdStatus(): FdSnapshot {
    return this.captureFdSnapshot(this.tunFd);
  }

  // Read-only F_GETFD probe through the native core. Never closes or dups
  // the fd — safe on the raw TUN fd (close ownership stays with
  // connection.destroy()) and on native-owned sockets alike.
  private captureFdSnapshot(fd: number): FdSnapshot {
    const integer: boolean = Number.isInteger(fd);
    const nonnegative: boolean = integer && fd >= 0;
    const probeEligible: boolean = nonnegative && fd <= MAX_NATIVE_FD;
    if (!probeEligible) {
      return {
        fd: fd,
        integer: integer,
        nonnegative: nonnegative,
        probeEligible: false,
        probed: false,
        open: false,
        errno: -1
      };
    }

    try {
      const status: FdStatusNative = JSON.parse(core.fd_status(fd)) as FdStatusNative;
      return {
        fd: fd,
        integer: integer,
        nonnegative: nonnegative,
        probeEligible: true,
        probed: true,
        open: status.open,
        errno: status.errno
      };
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_FD_PROBE_REJECTED|requestId=%{public}s|fd=%{public}s|summary=%{public}s',
        this.requestId, String(fd), safeError(error as Error));
      return {
        fd: fd,
        integer: integer,
        nonnegative: nonnegative,
        probeEligible: true,
        probed: false,
        open: false,
        errno: -1
      };
    }
  }

  private logFdSnapshot(phase: string, snapshot: FdSnapshot, marker: string): void {
    hilog.info(DOMAIN, TAG,
      'VPN_FD_SNAPSHOT|requestId=%{public}s|phase=%{public}s|fd=%{public}s|integer=%{public}s|' +
      'nonnegative=%{public}s|probeEligible=%{public}s|probed=%{public}s|open=%{public}s|' +
      'errno=%{public}d|marker=%{public}s',
      this.requestId, phase, String(snapshot.fd), String(snapshot.integer), String(snapshot.nonnegative),
      String(snapshot.probeEligible), String(snapshot.probed), String(snapshot.open), snapshot.errno, marker);
  }

  private postDestroyMarker(snapshot: FdSnapshot): string {
    if (!snapshot.probed) {
      return 'FD_STATE_UNCONFIRMED';
    }
    if (snapshot.open) {
      return 'FD_STILL_OPEN';
    }
    if (this.fdConfirmedOpenAfterCreate) {
      return 'FD_CLOSED_CONFIRMED';
    }
    return 'FD_NOT_OPEN_AFTER_DESTROY';
  }

  private removeStatusForUi(): void {
    const path: string = `${this.context.filesDir}/netbird-status.json`;
    try {
      if (fs.accessSync(path)) {
        fs.unlinkSync(path);
      }
    } catch (error) {
      hilog.warn(DOMAIN, TAG, 'VPN_STATUS_REMOVE_FAIL|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error as Error));
    }
  }

  private destroyOnce(trigger: string): Promise<void> {
    this.removeStatusForUi();
    this.networkRecoveryReady = false;
    if (this.networkObserver !== undefined) {
      try {
        this.networkObserver.unregister((error: Error | undefined): void => {
          if (error !== undefined) {
            hilog.warn(DOMAIN, TAG, 'VPN_NETWORK_UNWATCH_FAIL|summary=%{public}s', safeError(error));
          }
        });
      } catch (error) {
        hilog.warn(DOMAIN, TAG, 'VPN_NETWORK_UNWATCH_FAIL|summary=%{public}s', safeError(error as Error));
      }
      this.networkObserver = undefined;
    }
    this.stopSocketSupply();
    if (this.destroyPromise !== undefined) {
      hilog.info(DOMAIN, TAG,
        'VPN_DESTROY_ALREADY|requestId=%{public}s|firstTrigger=%{public}s|requestedTrigger=%{public}s',
        this.requestId, this.destroyTrigger, trigger);
      return this.destroyPromise;
    }
    this.destroyTrigger = trigger;
    this.destroyPromise = this.runDestroyOnce();
    return this.destroyPromise;
  }

  private async runDestroyOnce(): Promise<void> {
    const createPromise: Promise<CreateOutcome> | undefined = this.createPromise;
    if (createPromise === undefined) {
      hilog.info(DOMAIN, TAG,
        'VPN_DESTROY_SKIPPED|requestId=%{public}s|reason=create-not-started-or-sync-rejected', this.requestId);
      return;
    }

    if (!this.createSettled) {
      hilog.info(DOMAIN, TAG, 'VPN_DESTROY_WAIT_CREATE|requestId=%{public}s', this.requestId);
    }
    const outcome: CreateOutcome = await createPromise;
    if (!outcome.destroyRequired) {
      hilog.info(DOMAIN, TAG,
        'VPN_DESTROY_SKIPPED|requestId=%{public}s|reason=%{public}s', this.requestId, outcome.reason);
      return;
    }

    const connection: vpnExtension.VpnConnection | undefined = this.connection;
    if (connection === undefined) {
      hilog.info(DOMAIN, TAG,
        'VPN_DESTROY_SKIPPED|requestId=%{public}s|reason=connection-invariant-missing', this.requestId);
      return;
    }

    const preDestroy: FdSnapshot = this.captureFdStatus();
    const preMarker: string = preDestroy.probed ?
      (preDestroy.open ? 'PRE_DESTROY_OPEN' : 'PRE_DESTROY_NOT_OPEN') : 'FD_STATE_UNCONFIRMED';
    this.logFdSnapshot('pre-destroy', preDestroy, preMarker);
    hilog.info(DOMAIN, TAG,
      'VPN_DESTROY_BEGIN|requestId=%{public}s|trigger=%{public}s|createAccepted=%{public}s|reason=%{public}s',
      this.requestId, this.destroyTrigger, String(outcome.accepted), outcome.reason);

    try {
      await connection.destroy();
      const postDestroy: FdSnapshot = this.captureFdStatus();
      const marker: string = this.postDestroyMarker(postDestroy);
      this.logFdSnapshot('post-destroy-resolved', postDestroy, marker);
      hilog.info(DOMAIN, TAG,
        'VPN_DESTROY_RESOLVED|requestId=%{public}s|fdOpen=%{public}s|fdMarker=%{public}s',
        this.requestId, String(postDestroy.open), marker);
    } catch (error) {
      const postDestroy: FdSnapshot = this.captureFdStatus();
      const marker: string = this.postDestroyMarker(postDestroy);
      this.logFdSnapshot('post-destroy-rejected', postDestroy, marker);
      hilog.error(DOMAIN, TAG,
        'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=destroy|fdOpen=%{public}s|' +
        'fdMarker=%{public}s|summary=%{public}s',
        this.requestId, String(postDestroy.open), marker, safeError(error as Error));
    }

    this.auditOutboundSocketAfterDestroy();
  }

  // Normally feed closes the shell's WG original immediately. An abort before
  // feed may leave it pending; retire that socket here, never the TUN fd.
  private auditOutboundSocketAfterDestroy(): void {
    const fd: number = this.outboundSocketFd;
    if (fd < 0) {
      hilog.info(DOMAIN, TAG,
        'VPN_OUTBOUND_AUDIT|requestId=%{public}s|socket=already-closed', this.requestId);
      return;
    }
    this.outboundSocketFd = -1;
    socketSupplyCloseFd(fd);
    hilog.info(DOMAIN, TAG,
      'VPN_OUTBOUND_AUDIT|requestId=%{public}s|fd=%{public}d|owner=shell|closed=true',
      this.requestId, fd);
  }
}
