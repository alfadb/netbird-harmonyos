import VpnExtensionAbility from '@ohos.app.ability.VpnExtensionAbility';
import AbilityConstant from '@ohos.app.ability.AbilityConstant';
import Want from '@ohos.app.ability.Want';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import process from '@ohos.process';
import vpnExtension from '@ohos.net.vpnExtension';
import connection from '@ohos.net.connection';
import statistics from '@ohos.net.statistics';
import core from 'libnetbird_core.so';
import { VpnRouteConfig, VpnTunnelConfig, buildPlatformVpnConfig, gateDefaultRoutes,
  loadVpnTunnelConfig, applyNetworkConfig, withEndpointExclusions,
  CaptureRouteOutcome, TunnelSubnetRouteOutcome, withCaptureRoute,
  withTunnelSubnetRoute } from './NetBirdVpnConfig';
import { N2EvidenceDecision, N2EvidenceOptions, parseN2Evidence, withN2EvidenceRoutes,
  withoutN2ExcludedRoutes } from './NetBirdN2Evidence';
import { N2ControlProtocol, sendAndWaitN2Control, sendN2Control } from './NetBirdN2Control';
import { N2_CONTROL_FILE_NAME, N2ControlDecision,
  formatN2ControlShellLine, formatN2ControlShellReject,
  parseN2ControlRequest } from './NetBirdN2ControlRequest';
import { N2_STIMULUS_FILE_NAME, N2StimulusDecision, N2StimulusReply,
  formatN2StimulusShellLine, formatN2StimulusShellReject, parseN2Stimulus,
  parseN2StimulusReply } from './NetBirdN2Stimulus';
import { N2_A_EVENT_FILE_NAME, N2AEventDecision, N2AEventReply,
  formatN2AEventShellLine, formatN2AEventShellReject, parseN2AEvent,
  parseN2AEventReply } from './NetBirdN2AEvent';
import { N2SandwichPost, formatN2SandwichLegMarker, formatN2SandwichShellLine,
  parseN2SandwichPoll, selectN2SandwichTarget } from './NetBirdN2Sandwich';
import { N2_CD_FILE_NAME, N2BearerFacts, N2CdPlanDecision, N2CdPlanRequest,
  classifyN2Bearer, formatN2CdPlanShellLine, formatN2CdPlanShellReject, n2CdDirectionVerdict,
  n2CdPrepareVerdict, n2CdProtocol, n2CdStartVerdict, n2CdTransitionStep, parseN2CdPlan,
  selectN2CdTarget } from './NetBirdN2CdPlan';
import { N2_RING_ACK_FILE_NAME, N2_RING_DUMP_FILE_NAME, N2_RING_FILE_NAME, N2_RING_MAX_BYTES,
  N2_RING_SEGMENTS, N2RingAckDecision, N2RingAckRequest, N2RingDumpDecision, N2RingDumpRequest,
  formatN2RingAckShellLine, formatN2RingAckShellReject, formatN2RingDumpShellLine,
  formatN2RingDumpShellReject, parseN2RingAck, parseN2RingDump } from './NetBirdN2Ring';
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
  RelayEndpoint, buildExcludedRouteEntries, deriveLanExclusion, deriveStunEndpointExclusions,
  isExclusionFailClosed, lanExclusionSignature, parseRelayUrl,
  stripDialPort
} from './NetBirdEndpointExclusion';
// type-only: erased at runtime, so the host selfcheck fixtures that stage
// this file byte-identically never resolve the sibling module
import type { LanExclusionProbe, LanExclusionResult } from './NetBirdEndpointExclusion';
import {
  ConnectorStatusResult, CoreNetworkConfig, CoreRouteEntry, ConnectorStartResult,
  ConnectorLogoutResult, DeviceConfigFile, ManagementEndpoint, NativeTcpSocketOpen, RelayAdvertisedView,
  RouteSetAppliedResult, SocketFeedResult, TunReleaseResult, WgFeedResult,
  DEFAULT_DEVICE_CONFIG_FILE, DEFAULT_SETUP_KEY_FILE, NETWORK_MAP_POLL_MS, NETWORK_MAP_WAIT_MS,
  PACKAGED_CA_ROOTS_RAWFILE, STATUS_POLL_MS, buildConnectorConfigJson, connectorIceSocketFeed,
  connectorNetworkConfig, connectorRouteSetApplied, connectorRouteSelectionSet,
  connectorRoutes, connectorSignalFeed,
  connectorStartWithSocket,
  connectorSocketFeed, connectorPhysicalNetworkChanged, connectorManagementAddressUpdate,
  connectorStatus, connectorStop, connectorLogout, connectorTunFdFeed, connectorTunRelease, connectorWgSocketFeed,
  socketSupplyPoll, socketSupplyReply, socketSupplyCloseFd, socketSupplyShutdown,
  socketSupplyDnsServers, physicalDefaultDnsServers, physicalDefaultNetId, forgetPhysicalDefaultDns,
  physicalDefaultLanProbe,
  resolveProtectedIp,
  SocketSupplyRequest,
  countPemCertificates, derivePlatformDnsEndpoints,
  formatRelayStatusLine,
  iceSocketOpen, mgmtSocketOpen, parseDeviceConfig, parseManagementEndpoint, parseRelayAdvertised,
  parseSignalEndpoint,
  PhysicalBearerFacts, physicalDefaultBearerFacts,
  pollStatusOrNull, readCredentialFileText, readPackagedCaRootsPem, readPrivateFileText,
  resolveExclusionEndpointIp,
  resolveManagementIp, sleep,
  phoneLogAppend, phoneLogConfigure, phoneLogStatus, phoneLogSummary,
  connectorResourceStatus, nativeResourceSample,
  nativeAllocStats
} from './NetBirdConnector';
// type-only: erased at runtime (same rule as the endpoint/policy shapes above),
// so the host selfcheck fixtures that stage this file byte-identically need no
// shim const for the phone_log result/summary interfaces — nor for the 006
// native-resource sample shape. (This declaration is kept byte-stable for the
// run-phone-log.mts wiring pin; the 2026-10-09 live-route setter's answer type is
// imported type-only on its OWN line right below, same erased-name rule.)
import type { PhoneLogConfigureResult, PhoneLogSummary, NativeResourceSample }
  from './NetBirdConnector';
// type-only: the live-route setter's answer (`connectorRouteSelectionSet`). Erased
// at runtime — the value import is the wrapper function the staged REAL
// connector already exports, so no shim const is needed anywhere.
import type { RouteSelectionSetResult } from './NetBirdConnector';

import { CONNECT_ENTRY_REJECT_RUNNING, UiStatusDocument, buildUiStatusDocument,
  connectEntryDecision } from './NetBirdUiModel';
import { ifaceLostDecision, ifaceLostRetryAllowed, ifaceVerifyOutcome, settleWaitAction }
  from './NetBirdRecreatePolicy';
import { ACCOUNT_MODE_ACCOUNT, ACCOUNT_MODE_FILE_NAME, ACCOUNT_MODE_SETUP_KEY,
  decideAccountCredentialMode, parseAuthMode }
  from '../login/NetBirdAccountMode';
import { loadAccountCredentials } from '../login/NetBirdAccountStore';

import { isPhysicalNetworkSwitch, lanRoamCheckDue,
  lanRoamDecision, protectedConnectionRejectionReason,
  socketSupplyRejectionReason } from './NetBirdPhysicalNetworkPolicy';
// type-only: erased at runtime (same rule as the endpoint shapes above), so
// the staging selfchecks never need a shim for the policy module.
import type { LanRoamDecision } from './NetBirdPhysicalNetworkPolicy';

import { ICE_SOCKET_POOL_MIN, iceSocketNeedPerPeer, iceSocketTargetDepth,
  iceSocketTopUpCount } from './NetBirdIceSocketPool';

// 2026-10-08 shell-side diagnostics wiring. The native core already tees its
// OWN events into filesDir/netbird-vpn.log; these two modules must NEVER
// re-forward a native line (that would double every record). They carry the
// facts the core cannot see — the shell's own lifecycle/network/recreate
// milestones (NetBirdDiagnostics, role 'vpn': marker + 1 s memory samples +
// the bounded milestone queue) and the platform's system events
// (NetBirdSystemEvents: APP_CRASH / APP_FREEZE / RESOURCE_OVERLIMIT /
// APP_KILLED) — into the SAME phone_log sink. Both are started with the SAME
// safe sink and both are fail-soft: a platform refusal or an IO error is
// logged and the VPN flow is untouched.
import { event as diagEvent, start as startDiagnostics, stop as stopDiagnostics }
  from './NetBirdDiagnostics';
// type-only: erased at runtime. `DiagFields` is the diagnostics writer's own
// field-bag type — every milestone below declares its bag with THIS name (an
// object literal against a Record-typed parameter is rejected by ArkTS:
// arkts-no-untyped-obj-literals), so the declaration form is the only one used.
import type { DiagFields } from './NetBirdDiagnostics';
// 2026-10-09 (0.0.17) onDestroy-entry stop marker. The ability writes the
// marker's `stopping` state with the SYNCHRONOUS fs API (see
// writeStopMarkerSync), so the file NAME, the path ALLOWLIST and the exact
// marker LINE come from the PURE core — never re-spelled here. Value-only
// import: the core imports nothing, and the host fixtures stage it
// byte-identically next to the extension.
import { buildMarker, markerBasename, recordPath, MARKER_STATE_STOPPING }
  from './NetBirdDiagnosticsCore';
import { start as startSystemEvents, stop as stopSystemEvents } from './NetBirdSystemEvents';
// 2026-10-07 per-NetID route selection (decision B). The VPE is the ONLY reader
// of the stored selection at start and the ONLY writer of the public catalog
// document. The FILE ADAPTER is imported by value (the host fixtures stage it
// byte-identically next to this file); the contract types are type-only, so the
// existing byte-identical staging selfchecks never have to resolve the pure
// routes module for erased names.
import { loadRouteSelection, writeRouteCatalog } from '../routes/NetBirdRouteStore';
import type { RouteCatalog, RoutePreferencesError, RouteSelection }
  from '../routes/NetBirdRoutePreferences';
// 2026-10-09 live route apply (instant apply: 路由选择页 → live VPE). The VPE is
// the CONSUMER of the page's apply request (filesDir/netbird-route-apply-request
// .json) and the ONLY writer of the apply result (…-result.json). Both the file
// adapter and the pure document module it uses are VALUE imports — the token
// vocabulary below is the contract's own, never a locally re-spelled literal —
// and the two contract interfaces are type-only so the existing byte-identical
// staging selfchecks only need the two resolver edges (see the selfcheck hooks).
import { readRouteApplyRequest, writeRouteApplyResult }
  from '../routes/NetBirdRouteRequestStore';
import { ROUTE_APPLY_MODE_NONE, ROUTE_APPLY_MODE_NO_OS_CHANGE,
  ROUTE_APPLY_MODE_RECREATED, ROUTE_APPLY_MODE_STORED_NO_MAP,
  ROUTE_APPLY_STATE_APPLIED, ROUTE_APPLY_STATE_FAILED }
  from '../routes/NetBirdRouteRequest';
import type { RouteApplyRequest, RouteApplyResult } from '../routes/NetBirdRouteRequest';

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';
const MAX_NATIVE_FD: number = 2147483647;
// The all-on route selection the live-apply consumer falls back to when the
// scoped preference document is ABSENT (`loadRouteSelection` answered null):
// the pure module's own `routeSelectionDefault()` shape — nothing explicit,
// everything default-on, future NetIDs included. The three fields are the
// selection's whole DECLARED surface, and `connectorRouteSelectionSet` builds a
// fresh payload out of exactly them, so this constant is never mutated.
const ALL_ON_ROUTE_SELECTION: RouteSelection = {
  selected_routes: [],
  deselected_routes: [],
  deselect_all: false
};
// Bound for the VpnConnection.protect() wait (house rule: every ArkTS-side
// wait is bounded). Same 5 s box as the verified spike helper
// protectSocketWithBox (spikes/n1b-disc-phys-hap e8e2cf9). Unlike the spike's
// best-effort treatment, the shell treats box expiry as protect failure
// (governance docs/native-nx-governance.md item 4: fail-closed).
const PROTECT_BOX_MS: number = 5000;
// ICE socket pool: the ICE gather consumes one FRESH unbound UDP socket per
// IPv4 interface round (dups share the socket object, so a bound copy cannot
// be re-bound), and a device has several interfaces — one socket starves the
// gather after the first round (device-verified 2026-09-14). The pool target is
// DEMAND-SIZED from the CORE's own per-peer need (peers × core need_per_peer,
// clamped 8..48, see NetBirdIceSocketPool.ets): smoke 0050 measured 6 peers ×
// need 4 = 24 against a fixed 8 (2 peers starved every round), and smoke 0051
// saw need=6 (3 allowed interfaces) → 36, which the old 32 cap did not cover.
// The core reports the need in connector_status().ice_sockets.need_per_peer;
// when that field is absent or -1 the shell falls back to its 4-per-peer
// constant. ICE_SOCKET_POOL_MIN is the target until a peer count is known.
// A failed ICE session is re-negotiated after a cooldown, and every retry
// consumes fresh sockets again (the pool is one-shot). Refill when status
// reports a drained queue, with a periodic retry while the VPN is armed; a
// core that is not gathering must not make the shell pile up fds (device run 3
// reached queue=26 while nothing consumed them).
const ICE_SOCKET_TOPUP_MS: number = 4000;
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
// roam-1005-0014 fix (RECREATE-LIFECYCLE-ANALYSIS.md §4): the destroy→create
// settle window. The device-proven race: system-side teardown (netdev removal
// + VPN unregister) landed ~168 ms AFTER the new create() and hit the SAME
// global VPN slot, killing the fresh vpn-tun while the fd stayed open
// in-process. SETTLE floor waits that window out before the new create; the
// CAP is a fuse (proceed and let the post-create verification decide, never
// block the recreate forever); POLL is the registry re-check cadence while
// waiting for the OLD vpn-tun to disappear.
const RECREATE_SETTLE_MIN_MS: number = 800;
const RECREATE_SETTLE_RETRY_MIN_MS: number = 1600;
const RECREATE_SETTLE_CAP_MS: number = 3000;
const RECREATE_SETTLE_POLL_MS: number = 250;
// Post-create interface verification: the new vpn-tun must REALLY be listed
// by the network registry (fd-open in-process proves nothing — the device
// defect kept an open fd over a removed netdev). Short bounded re-tries ride
// out registry propagation lag; the delayed RECHECK a few seconds after
// RECREATE_DONE catches a teardown that landed late and killed the fresh
// interface.
const RECREATE_IFACE_VERIFY_TRIES: number = 3;
const RECREATE_IFACE_VERIFY_GAP_MS: number = 500;
const RECREATE_IFACE_RECHECK_MS: number = 5000;
// roam-1005-0124: the iface-loss rebuild budget — per RECREATE EVENT (every
// loss of a live generation re-arms the automatic full rebuild; the old
// per-session latch starved the second network switch of its retry and
// fail-closed a healthy session), bounded by this anti-storm sliding
// window: at most 3 rebuild STARTS within any 60 s. Past the cap the honest
// fail-closed disconnect applies until the window slides.
const IFACE_LOST_RETRY_WINDOW_MS: number = 60000;
const IFACE_LOST_RETRY_WINDOW_MAX: number = 3;
// 场景B roaming fix (LAN-ROUTE-ROAMING-ANALYSIS.md §4 step 2): the stability
// window a switched physical default network must hold before the shell
// re-derives the LAN exclusion and compares it with the live config. The
// 250 ms networkDebounceTimer above already collapses one event storm into a
// single switch; this window (2–3 s per the fix plan) additionally rides out
// a flapping Wi-Fi <-> cellular handover, so a roam rebuild runs at most
// once, on the network that actually stayed. The check itself rides the
// status watcher tick (STATUS_POLL_MS cadence, no new timer): it fires on
// the first tick at least this many ms after the LAST switch.
const LAN_ROAM_STABLE_MS: number = 2500;
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
// N2 first batch C/D columns: the armed session's ring cadence. While the
// evidence ring is configured, one `N2_RING_STATS` line is emitted every
// minute — by the CORE (`n2_ring_status`), so the line name, every field and
// the ring membership are the core's own status formatter. A session that is
// killed mid-cellular therefore leaves a ring tail whose counters are at most
// one minute old. No timer exists while the ring is unconfigured.
const N2_RING_STATS_MS: number = 60000;

// Explicit user logout (stop-time Logout defect, 2026-10-01): the UI writes
// ONE `netbird-logout` ticket into the shared filesDir AFTER the user
// confirms 注销; THIS process — the only one holding the live connector —
// picks it up on the next watcher tick, sends ONE management Logout through
// core.connector_logout() (the server deregisters and DELETES the peer),
// writes the `netbird-logout-result` answer the UI is bounded-waiting for,
// then tears the session down (connectorStop + destroyOnce): a logged-out
// peer must not live on, and the dying Sync stream must not reconnect and
// re-register the peer that was just deleted server-side. The ticket is
// consumed exactly once (`finally` unlink) and freshness-bounded, so a
// leftover from a session that died before its tick can never tear a LATER
// session down.
const LOGOUT_FILE_NAME: string = 'netbird-logout';
const LOGOUT_RESULT_FILE_NAME: string = 'netbird-logout-result';
const LOGOUT_TICKET_MAX_AGE_MS: number = 30000;

// 2026-10-06 登录失败可见性: the persistent connect-failure notice file
// (same shared-filesDir channel as the logout ticket; reader + shape live
// in NetBirdConnector.ets — CONNECT_FAILURE_FILE / ConnectFailureNotice).
// Written when the connector canNOT reach a live tunnel (login fatal auth,
// retry budget exhausted, startup abort before create()) so the home card can
// show the engine's own MESSAGE (error line + one Toast) next to its official
// state instead of a stale 连接中…; a later attempt's own engine report
// supersedes it, never an up-front delete.
const CONNECT_FAILURE_FILE_NAME: string = 'netbird-connect-failure';

// 2026-10-08 shell-side diagnostics wiring:
//   VPN_LOG_FILE_NAME — the native phone_log tee target under filesDir. The
//     native module rotates it itself (netbird-vpn.log, .1 … .3, 24 MiB each)
//     and opens it in APPEND mode, so a re-entered extension keeps the older
//     records and marks the new run with its own `sess=` token.
//   PHONE_LOG_STATUS_SAMPLE_MS — the writer-health summary cadence. It rides
//     the EXISTING 5 s status watcher tick behind this gate: one native status
//     read + one enqueued record per minute (never a second timer, never a
//     per-tick write storm), and it is independent of every early return in
//     that tick so it survives an unreachable connector too.
const VPN_LOG_FILE_NAME: string = 'netbird-vpn.log';
const PHONE_LOG_STATUS_SAMPLE_MS: number = 60000;

interface BusinessFailure extends Error {
  code?: number;
}

interface CreateOutcome {
  destroyRequired: boolean;
  accepted: boolean;
  reason: string;
}

// 2026-10-09 reporting correctness: the EXACT route set the controlled
// recreate's own ACK accepted, handed back to the ONE operation that asked for
// the rebuild. The set comes from the recreate leg's OWN snapshot (its second
// `connector_network_config` call) — which can differ from a setter answer
// taken one native call earlier: the two are DISTINCT NAPI calls, and the Rust
// management worker (a separate thread) may advance the NetworkMap between them
// inside the SAME JS turn. "No JS await between the setter and the recreate"
// constrains only the shell's own watcher tick, never the native side.
// `null` = this operation never reached an accepted ACK; `[]` = a really
// accepted EMPTY set (every network off), kept distinguishable from `null`.
interface RecreateAckOut {
  routes: string[] | null;
}

// N2 first batch C/D columns: the core's own `n2_ring_dump` answer, parsed
// verbatim. `lines` are ready-made `N2_RING_CHUNK|seq=<durable>|of=<count>|
// line=<原件>` records (each `seq` is a durable ring sequence, not a line
// index) and `summary` is the single `N2_RING_DUMP` line the core formatted.
interface N2RingDumpPage {
  ok: boolean;
  result: string;
  reason: string;
  from_seq: number;
  to_seq: number;
  count: number;
  emitted: number;
  frozen_tail_seq: number;
  first_seq: number;
  last_seq: number;
  records: number;
  bytes: number;
  malformed: number;
  lost_records: number;
  overflow: boolean;
  acked_seq: number;
  lines: string[];
  summary: string;
}

// The inert page a failed `n2_ring_dump` call is treated as: nothing is
// re-emitted, no watermark is reported (the `catch` branch returns before
// either). A dump never advances the watermark anyway.
function emptyN2RingDumpPage(): N2RingDumpPage {
  return { ok: false, result: 'rejected', reason: 'core-throw', from_seq: 0, to_seq: 0, count: 0,
    emitted: 0, frozen_tail_seq: 0, first_seq: 0, last_seq: 0, records: 0, bytes: 0, malformed: 0,
    lost_records: 0, overflow: false, acked_seq: 0, lines: [], summary: '' };
}

// N2 first batch C/D columns: the core's own `n2_ring_ack` answer, parsed
// verbatim. `result=acked` with `ok=true` is the only accepted shape;
// `advanced` says whether THIS request moved the watermark (a repeat of an
// already-acknowledged watermark answers `advanced=false`).
interface N2RingAckReply {
  ok: boolean;
  result: string;
  reason: string;
  session: string;
  up_to_seq: number;
  acked_seq: number;
  previous_acked_seq: number;
  advanced: boolean;
}

// The inert reply a failed `n2_ring_ack` call is treated as: no watermark, no
// advancement (the `catch` branch returns before the receipt anyway).
function emptyN2RingAckReply(): N2RingAckReply {
  return { ok: false, result: 'rejected', reason: 'core-throw', session: '', up_to_seq: 0,
    acked_seq: -1, previous_acked_seq: -1, advanced: false };
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
// the caller must NOT create the VPN. `dnsRoutes` is the RESOLVER (/32)
// subset of `routes` — roam-1005-0124: the one piece a recreate must
// re-derive from the current physical network instead of reusing frozen.
interface EndpointExclusionOutcome {
  fatal: boolean;
  routes: ExcludedRouteEntry[];
  dnsRoutes: ExcludedRouteEntry[];
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

// The `netbird-logout` ticket body is `{"issued_at_ms":<ms>}` — the UI's
// wall-clock stamp used only for the freshness bound (a leftover ticket must
// never tear a LATER session down). Anything that does not parse to a
// positive number is refused by name.
function parseLogoutTicketIssuedAtMs(text: string): number {
  try {
    const parsed: object = JSON.parse(text) as object;
    const record: Record<string, Object> = parsed as Record<string, Object>;
    const value: Object | undefined = record['issued_at_ms'];
    if (typeof value === 'number' && value > 0) {
      return value;
    }
  } catch (error) {
    // refused below
  }
  return -1;
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
  // The network generation the sockets of the CURRENT network were protected
  // under; it is bumped by every measured switch. ONE-based on purpose: it is
  // the `protect_generation` the `N2_SOCK` receipts carry, and the core's C/D
  // receipt verdict treats generation 0 as "not measured" and REFUSES it — with
  // a zero-based counter the very first switch could never name a baseline.
  private networkEpoch: number = 1;
  private networkNetId: number = 0;
  private networkDirty: boolean = false;
  private networkRecoveryActive: boolean = false;
  private networkRecoveryReady: boolean = false;
  private managementEndpoint: ManagementEndpoint | undefined = undefined;
  // 2026-10-07 client-detail increment: the configured management_url string
  // (NOT published verbatim — buildUiStatusDocument publishes scheme+host+port
  // only, via managementEndpointView). Kept so the status document can name
  // the server the connector was started against; '' before the first start.
  private managementUrl: string = '';
  // 2026-10-07 route-catalog publication: the VPE mirrors the native
  // `connector_routes` getter into filesDir/netbird-routes.json on the status
  // tick for the offline UI. ONE write at a time — two overlapping atomic
  // temp+rename cycles would race the same .tmp name; the flag is released in
  // every completion path and is the ONLY backlog this path keeps.
  private routeCatalogPublishInFlight: boolean = false;
  // 2026-10-09 live route apply (filesDir/netbird-route-apply-request.json):
  // the last request id this instance CONSUMED (answered with a result, or
  // answered as a scope mismatch). The request document is a RETAINED mailbox
  // (the store never unlinks it) and is re-read on every 100 ms supply tick, so
  // this in-memory id is what makes a re-read of the same request a no-op
  // instead of a second apply. '' = nothing consumed yet.
  private lastRouteApplyId: string = '';
  // ONE route-apply handler at a time: the read → apply → answer chain is
  // async and the 100 ms tick re-enters far faster than one recreate completes,
  // so this latch (released on BOTH the resolve and the reject path) is the
  // overlap guard. A request that arrives while an apply is in flight is NOT
  // lost: the retained document is read again by a later tick, i.e. the newest
  // queued id is processed after the current recreate.
  private routeRequestBusy: boolean = false;
  // Log-dedup for an unreadable apply request (no id exists to answer): the
  // class is logged ONCE per class change instead of once per 100 ms tick.
  // '' = the last read succeeded.
  private routeApplyReadErrorClass: string = '';
  private signalUri: string = '';
  // Depth reported by the last accepted feed — the top-up uses it to tell a
  // draining core (keep feeding) from an idle one (stop piling up).
  private iceQueueDepth: number = 0;
  // Demand-sized pool target (peers × 4, clamped 8..32): refreshed from every
  // status sample and from the live status read taken at pool-feed time, so a
  // 6-peer map targets 24 sockets instead of the fixed 8 of smoke 0050.
  private iceTargetDepth: number = ICE_SOCKET_POOL_MIN;
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
  // 2026-10-12: at_ms of the last engine state report published from the START leg
  // (publishEngineStateReport; 0 = none yet). Throttles those early reports to the
  // watcher cadence — they keep the UI's report channel fresh while the run starts.
  private lastEngineReportAtMs: number = 0;
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
  // 2026-10-08 diagnostics wiring: the native tee is installed once per
  // PROCESS (the path is idempotent, see startDiagnosticsLogTee), and the
  // writer-health summary rides the status watcher behind a 60 s gate.
  private logTeeConfigured: boolean = false;
  // 2026-10-09 (0.0.17): the onDestroy ENTRY wrote the marker's `stopping`
  // state once in this process, and the one-shot latch of the single W line a
  // failed marker write may leave (see writeStopMarkerSync).
  private stopMarkerWritten: boolean = false;
  private stopMarkerWarned: boolean = false;
  private lastPhoneLogStatusAtMs: number = 0;
  // N8: route networks destined for the initial-create ACK (the managed
  // subset actually merged into the platform VpnConfig; core-visible form =
  // canonical 'a.b.c.d/p' strings exactly as connector_network_config
  // renders them). Stashed before create(), consumed in settleCreate.
  private pendingAppliedRoutes: string[] | null = null;
  // N8: one controlled recreate at a time (the watcher ticks faster than a
  // recreate completes; this is the shell-side serialization on top of the
  // core-side budget/cooldown bounds).
  private recreateInFlight: boolean = false;
  // roam-1005-0014: the delayed post-recreate interface recheck timer handle
  // (one shot per completed recreate; cleared on teardown), and the
  // interface-loss rebuild budget (roam-1005-0124): per-EVENT, not per
  // session — every recreate event re-arms the automatic full rebuild, and
  // the sliding-window cap (IFACE_LOST_RETRY_* below) is the anti-storm
  // bound. `ifaceLostRetryLogMs` holds the Date.now() of each rebuild
  // START.
  private ifaceRecheckTimer: number = -1;
  private ifaceLostRetryLogMs: number[] = [];
  // VPN_UI_STATUS diagnostic dedup (change-or-60 s, same discipline as the
  // VPN_CONNECTOR_STATUS signature gate).
  private lastUiStatusLogSignature: string = '';
  private lastUiStatusLogAtMs: number = 0;
  private networkRecoveryPending: boolean = false;
  // 场景B roaming (LAN-ROUTE-ROAMING-ANALYSIS.md §4 step 2): the
  // LAN-exclusion signature merged into the LIVE platform VpnConfig —
  // recorded at each create() submit point (initial AND recreate legs), so a
  // later network switch compares against what is REALLY live. '' = nothing
  // submitted yet; a roam decision needs a settled connection first, so the
  // value at decision time is always a real signature ('none' included =
  // "this config carries no LAN exclusion", e.g. a cellular create).
  private appliedLanExclusion: string = '';
  // The armed debounce window: set by every measured physical-network
  // switch, consumed once by the first watcher tick that finds the network
  // stable for LAN_ROAM_STABLE_MS (lanRoamCheckDue). A newer switch while
  // armed only refreshes the timestamp — the window restarts, it never
  // stacks. false = no roam check pending.
  private lanRoamArmed: boolean = false;
  private lanRoamLastSwitchAtMs: number = 0;
  // Outcome of the most recent performControlledRecreate ('ok' |
  // 'fail:<stage>' | 'skip:<reason>'), set by the recreate path itself and
  // read by the roam trigger's follow-up VPN_LAN_ROAM line so the roam trail
  // carries the rebuild's result (and failure reason) without duplicating
  // the recreate's own logging.
  private lastRecreateOutcome: string = 'none';
  // N2-H: the endpoint-derived exclusion set frozen at THIS session's start
  // (before the first create()). The controlled recreate grafts the SAME
  // frozen set onto its rebuilt config — a recreate that dropped the
  // exclusions would suck the outer endpoints into the fresh (often
  // default-routed) tunnel. A fresh freeze (re-resolution) happens on the
  // next full start, never from a cached cross-session set.
  private frozenEndpointExclusions: ExcludedRouteEntry[] = [];
  // roam-1005-0124: the RESOLVER (/32) subset of the frozen set — the one
  // piece that must NOT cross a network switch frozen (the home Wi-Fi DNS
  // /32 stayed excluded over cellular and broke 192.168.50.1 access). The
  // recreate leg re-derives it from the CURRENT physical network and grafts
  // the remainder (management/signal/relay/stun /32) frozen, as before.
  private frozenDnsEndpointExclusions: ExcludedRouteEntry[] = [];
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
  // N2 P5: one automatic I/Ws sandwich is in flight in the watcher tick at a
  // time (the post controls are async; a slow send must not stack up).
  private n2SandwichBusy: boolean = false;
  // N2 P5: the I leg's marker (registered before the ICE pool feed). The post
  // control is offered by the core once the first real I send is observed.
  private n2IceSandwichMarker: string = '';
  // N2 P5: the Ws leg's marker (registered before Ws selection).
  private n2WsSandwichMarker: string = '';
  // N2 first batch C/D columns (2026-09-29): the in-flight switching-sandwich
  // plan's id ('' = none). The core owns the whole plan state machine; this is
  // the shell's own latch so the network observer knows a switch belongs to a
  // plan and the tick polls only while one is in flight.
  private n2CdActive: string = '';
  // …one plan step at a time (the pre/post controls are async).
  private n2CdBusy: boolean = false;
  // The plan's measured STARTING bearer (recorded at plan time, reported to the
  // core with the switch so the ordering verdict has it even if the platform
  // reading changes between ticks).
  private n2CdBeforeBearer: string = 'unknown';
  // The in-flight plan's column/kind/rep (the id alone is not enough to compute
  // the column's direction verdict at switch time).
  private n2CdColumn: string = '';
  private n2CdKind: string = '';
  private n2CdRep: number = 0;
  // The shell's own view of the plan's progress. The CORE owns every verdict
  // (phase/latch are mirrored, never decided here):
  //   * `n2CdPreparePending` — D column: the Wi-Fi → cellular preparation hop
  //     was reported and the cellular-side recovery must still hand the core
  //     `n2_cd_ready` before the plan is in position.
  private n2CdPreparePending: boolean = false;
  // N2 first batch C/D columns: the FORMAL-switch production-recovery gate.
  // It is the ONE switch that keeps every recovery / new-connection supply
  // entry from handing the core a production connection of the newly measured
  // network before the core's OWN poll answers `step=recover` (its tee HIT the
  // pre control on that network — see n2CdGateRelease). The pre control itself
  // is sent by the shell's own opener (mgmtSocketOpen/iceSocketOpen +
  // n2_control_send), never through a supply entry, so the gate can never block
  // its own release.
  //   * `''`         — no gate: unarmed, no active plan, the D column's
  //                    preparation hop (which must recover ordinarily), or a
  //                    plan that never reported a formal switch;
  //   * `'blocked'`  — the formal switch was reported and the core has not
  //                    released it yet: every epoch is refused, so an in-flight
  //                    recovery of an older epoch cannot `continue` into the new
  //                    one and a later switch cannot slip past;
  //   * `'released'` — the core's poll answered `step=recover` for THIS plan at
  //                    THIS epoch: supply is allowed for `n2CdGateEpoch` ONLY —
  //                    an async answer that lands after another switch must
  //                    never release that other epoch;
  //   * `'held'`     — the formal switch (or the plan) ended UNSAT: the gate
  //                    stays CLOSED for every epoch until the evidence session is
  //                    explicitly terminated (the next start flow's
  //                    readDiagSwitches / teardown), so a later recovery can
  //                    never overwrite the failed epoch's record. This is
  //                    deliberate: the frozen criteria must not be relaxed, so
  //                    "the plan is gone" is NOT a release.
  private n2CdGateState: string = '';
  // The `networkEpoch` the gate was armed for at the formal switch.
  private n2CdGateEpoch: number = 0;
  // The plan id the gate belongs to: a release must name it.
  private n2CdGatePlan: string = '';
  // The armed session's ring statistics timer (-1 = no ring configured).
  private n2RingStatsTimer: number = -1;

  sendN2PositiveControl(target: string, protocol: N2ControlProtocol, id: string): void {
    sendN2Control(this.n2Evidence, target, protocol, id);
  }

  // 2026-10-08 memory-pressure OBSERVATION (no memory is released here).
  //
  // SDK evidence, verified against the local API-26 declarations: `Ability`
  // declares `onMemoryLevel(level: AbilityConstant.MemoryLevel)` and
  // `ExtensionAbility extends Ability`, but VpnExtensionAbility's declaration
  // (`@ohos.app.ability.VpnExtensionAbility.d.ts`) is a standalone class with
  // NO extends clause and NO onMemoryLevel member (only context/onCreate/
  // onDestroy). So this is NOT a declared override of a base member — it is an
  // ADDITIONAL typed method carrying the exact SDK signature (no `any`, no
  // cast), and whether the framework dispatches the callback into a VPN
  // extension process is UNVERIFIED (device experiment pending). It only ever
  // records the bounded enum token; it never frees, trims or evicts anything.
  onMemoryLevel(level: AbilityConstant.MemoryLevel): void {
    const milestoneFields: DiagFields = { 'stage': 'platform', 'state': String(level) };
    this.diagMilestone('vpn_memory_level', milestoneFields);
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

    // 2026-10-11 retry re-entry (fix/retry): the gate is the CONNECTOR RUN, not
    // the ability instance. The old unconditional instance latch made every
    // later entry a no-op for the whole life of the process, so a retry after an
    // aborted run could never re-run the connect flow (device evidence:
    // pc-connecting-diag/REPORT.md §4.3). `connector_status().running` is the
    // CORE's own answer — the same "engine is running" gate the official Android
    // client applies (android-client EngineRunner.java:136-147) — and this SDK
    // declaration has no onConnect to route a repeated start through
    // (@ohos.app.ability.VpnExtensionAbility.d.ts: only context/onCreate/
    // onDestroy; contrast AppServiceExtensionAbility.d.ts:110 onConnect).
    const connectorRunning: boolean | null = this.connectorRunState();
    if (connectEntryDecision(connectorRunning) === CONNECT_ENTRY_REJECT_RUNNING) {
      hilog.error(DOMAIN, TAG,
        'VPN_CREATE_REJECTED|requestId=%{public}s|phase=connect-run-active|' +
        'summary=connector-already-running',
        this.requestId);
      // Still an ANSWER, not a failure: an entry that arrives while a run is
      // active is the idempotent "already running" answer. It needs no evidence
      // document of its own: the ACTIVE run is publishing its own status document
      // on every watcher tick, and that document is the engine's own state report
      // the UI's pending action reads (2026-10-12: the connect-attempt ACK file is
      // gone — the state report IS the upstream mechanism).
      return;
    }

    // 2026-10-08 diagnostics wiring (as early as the extension lifecycle
    // allows): install the persistent VPN log tee and arm the two shell-side
    // diagnostic modules BEFORE anything else in the start flow can be
    // recorded or can fail. Fail-soft throughout — a refused tee or a refused
    // system-event watcher is logged and the VPN flow below is untouched.
    this.startDiagnosticsLogTee();

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
    const baselineLine: string = `VPN_NETWORK_BASELINE|requestId=${this.requestId}|` +
      `netId=${String(this.networkNetId)}`;
    hilog.info(DOMAIN, TAG, '%{public}s', baselineLine);
    this.n2RingAppend(baselineLine);
    const milestoneFields: DiagFields = { 'stage': 'onCreate', 'net': this.networkNetId };
    this.diagMilestone('vpn_ext_start', milestoneFields);
    this.watchPhysicalNetwork();
    this.startConnectorThenCreate(want, connection).catch((error: Error) => {
      hilog.error(DOMAIN, TAG,
        'VPN_START_FLOW_REJECTED|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error));
    });
  }

  // ---------------------------------------------------------------------------
  // 2026-10-08 diagnostics wiring (shell side)
  //
  // WHY the native writer is left alone: phone_log is a PROCESS-LIFETIME log
  // destination. The core tees its own events into netbird-vpn.log; this shell
  // only appends its own milestones + the system-watcher lines through the same
  // tee. The writer is never shut down (no phone_log_shutdown call): a stop
  // request could only race the closing records of Diagnostics.stop(), whose
  // drain is asynchronous by contract, and the OS closes the file when the
  // extension process ends anyway. A re-entered extension re-configures the
  // SAME path idempotently (the native module answers `idempotent:true` and
  // starts no second writer), so the file keeps its earlier records and gains a
  // new `sess=` token. There is NO keep-alive, NO timer re-arm and no claim of
  // process immortality here — this is a log destination, nothing more.
  //
  // START ORDER: native tee first (nothing can be appended before it exists),
  // then the diagnostics helper (marker + 1 s memory samples + the bounded
  // milestone queue), then the system-event watcher (its subscription boundary
  // is its own `sys_event_watcher_started` line — an event that already
  // happened before it cannot be reconstructed, and APP_KILLED only ever
  // arrives on a later launch). Both modules share ONE safe sink.
  //
  // STOP ORDER (see onDestroy): the system-event subscription is removed in
  // onDestroy; the diagnostics helper is stopped LATE, only after destroyOnce
  // has settled, because stop() drains the queued records through a promise
  // chain and the teardown's own closing records must still be enqueued first.
  // The native writer is never stopped.
  //
  // SYSTEM EVENTS ARE BEST-EFFORT: when the platform refuses the watcher,
  // SystemEvents.start() returns false and emits its own
  // `sys_event_watcher_unavailable` line through the sink. This extension does
  // NOT promise crash / freeze / resource-over-limit / APP_KILLED records for
  // this process, and nothing in the VPN flow changes on that refusal.
  // ---------------------------------------------------------------------------

  // Idempotent by construction: the configure latch is per PROCESS, while the
  // two module starts are safe to repeat (each is a no-op while running and can
  // arm again after a stop, e.g. a later extension instance in the same
  // process). Never throws: every leg is a defensive wrapper.
  private startDiagnosticsLogTee(): void {
    // Fail-soft even here: a diagnostics tee must never throw out of a
    // lifecycle callback, and `context` is the one platform read on this path
    // that can be absent. Without a filesDir there is no log destination at
    // all, so nothing is configured and nothing is recorded (Diagnostics.start
    // would reject the empty filesDir by its own contract anyway).
    let filesDir: string = '';
    try {
      filesDir = this.context.filesDir;
    } catch (error) {
      filesDir = '';
    }
    if (filesDir.length === 0) {
      return;
    }
    let configured: PhoneLogConfigureResult | null = null;
    if (!this.logTeeConfigured) {
      this.logTeeConfigured = true;
      configured = phoneLogConfigure(`${filesDir}/${VPN_LOG_FILE_NAME}`);
      if (!configured.ok) {
        // The tee IS the destination: a refused configure means no shell record
        // can land (the vpn_log_tee milestone below is dropped with everything
        // else), so this one HiLog/hdc line is the only place the failure stays
        // visible. Never fatal — the VPN flow is untouched.
        hilog.warn(DOMAIN, TAG, 'VPN_LOG_TEE_FAILED|requestId=%{public}s|code=%{public}s',
          this.requestId, configured.error ?? 'configure-failed');
      }
    }
    startDiagnostics(filesDir, 'vpn', (line: string): void => { this.diagSink(line); },
      (): string | undefined => nativeAllocStats());
    startSystemEvents('vpn', (line: string): void => { this.diagSink(line); });
    if (configured !== null) {
      // Recorded AFTER the helper started (else the queue would not accept it)
      // and after the tee was configured (else the append would be refused):
      // the one record that says whether the persistent log really works.
      const detail: string = configured.ok
        ? (configured.idempotent === true ? 'idempotent' : 'installed')
        : (configured.error ?? 'configure-failed');
      const milestoneFields: DiagFields = {
        'stage': 'native', 'result': configured.ok ? 'ok' : 'fail', 'detail': detail
      };
      this.diagMilestone('vpn_log_tee', milestoneFields);
    }
  }

  // The ONE sink both diagnostic modules share. Cheap and non-throwing by
  // contract (it runs inside the diagnostics drain loop and on the
  // system-event callback thread); the wrapper answers a code on any native
  // failure, so a broken log can never reach the VPN path.
  private diagSink(line: string): void {
    try {
      phoneLogAppend(line);
    } catch (error) {
      // unreachable through the wrapper; belt only, never into the VPN path
    }
  }

  // One shell milestone. Enqueue-only (bounded queue inside the helper): no
  // disk IO on this stack and no throw. The native core already tees every
  // NATIVE event, so only the shell facts it cannot see are recorded here.
  // `fields` is the diagnostics writer's own DiagFields bag; call sites declare
  // it in a typed constant (ArkTS rejects a bare object literal passed
  // straight into a Record-typed parameter).
  private diagMilestone(name: string, fields: DiagFields): void {
    try {
      diagEvent(name, fields);
    } catch (error) {
      // never into the VPN path
    }
  }

  // Writer health ONCE per minute, riding the EXISTING 5 s watcher tick (no
  // second timer, no per-tick write). The native counters are the only view of
  // records the file itself cannot show: refused at ingress (dropped, named
  // separately as rate_suppressed) and accepted-but-not-written (pending /
  // failed). Only the counters leave this function — never the raw path or the
  // session token.
  private samplePhoneLogStatus(): void {
    const now: number = Date.now();
    if (now - this.lastPhoneLogStatusAtMs < PHONE_LOG_STATUS_SAMPLE_MS) {
      return;
    }
    this.lastPhoneLogStatusAtMs = now;
    const summary: PhoneLogSummary = phoneLogSummary(phoneLogStatus());
    const milestoneFields: DiagFields = {
      'stage': 'native', 'state': summary.state, 'detail': summary.detail
    };
    this.diagMilestone('phone_log_status', milestoneFields);
    // 006: the native resource snapshot rides the SAME 60 s gate and the SAME
    // tick (one extra native read, no second timer, nothing per 5 s tick): the
    // counts are correlated with the sample above by the records' own ts/pid.
    this.sampleNativeResourceStatus();
  }

  // 006 native resource snapshot, ONCE per 60 s and ONLY as the tail of the
  // writer-health window above (same gate, same tick instant). The native
  // document is read through the defensive NetBirdConnector wrapper: a missing
  // export, a throw or an unparseable/malformed document answers null and every
  // leaf then renders the explicit `na` — the probe can neither invent a zero
  // nor break the watcher.
  //
  // TWO records, not one: the 22 scalar leaves do not fit one bounded
  // (< MAX_LINE_CHARS) line, and the record builder would SILENTLY cut the
  // tail. `stage` labels the group and `seq` its order, so no leaf is dropped;
  // an extra interval or a per-5 s write is deliberately NOT added.
  private sampleNativeResourceStatus(): void {
    const sample: NativeResourceSample = nativeResourceSample(connectorResourceStatus());
    const connectorFields: DiagFields = {
      'stage': 'connector', 'seq': 1,
      'conn_present': sample.conn_present, 'conn_gen': sample.conn_gen,
      'conn_running': sample.conn_running, 'conn_state': sample.conn_state,
      'conn_starts': sample.conn_starts, 'conn_stops': sample.conn_stops,
      'ice_available': sample.ice_available, 'ice_sessions': sample.ice_sessions,
      'ice_live': sample.ice_live, 'ice_stopped': sample.ice_stopped,
      'ice_fds': sample.ice_fds
    };
    this.diagMilestone('native_resource_sample', connectorFields);
    const transportFields: DiagFields = {
      'stage': 'transport', 'seq': 2,
      'icesock_ready': sample.icesock_ready, 'icesock_queued': sample.icesock_queued,
      'icesock_taken': sample.icesock_taken, 'supply_installed': sample.supply_installed,
      'supply_pending': sample.supply_pending, 'supply_queued': sample.supply_queued,
      'relay_ready': sample.relay_ready, 'relay_lanes': sample.relay_lanes,
      'relay_opens': sample.relay_opens, 'relay_foreign': sample.relay_foreign,
      'dns_pending': sample.dns_pending
    };
    this.diagMilestone('native_resource_sample', transportFields);
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
    // N2 first batch C/D columns: the switch evidence is exactly what the host
    // cannot read while the device is on cellular, so the line is appended to
    // the armed-only app-internal ring verbatim (same text as hilog).
    const pendingLine: string = `VPN_NETWORK_EVENT|event=${event}|netId=${eventNetId}|` +
      `physicalDefault=${String(eventNetId === physicalNetId && physicalNetId !== 0)}|` +
      `physicalNetId=${physicalNetId}|switched=pending|action=debounce`;
    hilog.info(DOMAIN, TAG, '%{public}s', pendingLine);
    this.n2RingAppend(pendingLine);
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
    const eventLine: string = `VPN_NETWORK_EVENT|event=${event}|netId=${eventNetId}|` +
      `physicalDefault=${String(eventNetId === netId && netId !== 0)}|` +
      `physicalNetId=${netId}|switched=${String(switched)}|action=${action}`;
    hilog.info(DOMAIN, TAG, '%{public}s', eventLine);
    this.n2RingAppend(eventLine);
    if (!switched) { return; }
    this.networkNetId = netId;
    this.networkDirty = true;
    this.networkEpoch++;
    // 2026-10-08: the physical loss/switch edge as ONE persistent record
    // (mode=network-lost|redial). The shell hilog line above is not part of
    // netbird-vpn.log (only the NATIVE funnel is teed), so this is the copy
    // that survives without a host channel.
    const milestoneFields: DiagFields = { 'net': netId, 'mode': action, 'trigger': event };
    this.diagMilestone('vpn_net_switch', milestoneFields);
    // 场景B roaming (§4 step 2): (re)arm the LAN-exclusion check window for
    // THIS switch. The check itself fires from the status watcher tick once
    // the network has held for LAN_ROAM_STABLE_MS (checkLanRoamRecreate); a
    // net-lost transition arms too, so the lost->available hop collapses
    // into ONE window ending on the network that stayed.
    this.lanRoamArmed = true;
    this.lanRoamLastSwitchAtMs = Date.now();
    // N2 first batch C/D columns: advance the plan BEFORE the re-feed starts,
    // so the core records the OLD live registration of the kind (the baseline
    // the new-network receipt must differ from). `gated` is the plan's own
    // answer: it OWNS the production-recovery gate of this switch, and the
    // recovery must then wait for the core's `step=recover` (its tee HIT the
    // pre control on the NEW network). A net-lost transition (netId 0) measures
    // no new network at all, so the plan keeps waiting for the real switch
    // instead of reporting an unmeasured one (the core's SWITCH_WAIT_MS bounds
    // that wait). The gate itself is armed SYNCHRONOUSLY inside n2CdOnSwitch,
    // before the identity capture and before the switch report.
    const gated: boolean = netId !== 0 && this.n2CdOnSwitch(netId);
    connectorPhysicalNetworkChanged('');
    // The pre control is owed on the NEW network, and the core's own
    // old-connection cleanup above must have happened first: a control sent
    // before it could still leave on the OLD registration. Nothing else is
    // needed here — the plan's next step (the recovery) is driven by the core's
    // `step=recover`, which the gate only accepts for this plan and this epoch.
    if (gated) {
      this.pollN2CdStep();
    }
    if (netId !== 0 && !gated) {
      this.recoverPhysicalNetwork().catch((error: Error): void => {
        this.n2NetworkRecoveryFail(error);
      });
    }
  }

  // 场景B roaming fix (LAN-ROUTE-ROAMING-ANALYSIS.md §4 step 2): after a
  // physical default-network switch has held stable for LAN_ROAM_STABLE_MS,
  // re-derive the LAN exclusion from the CURRENT network and compare it with
  // the one merged into the live platform VpnConfig. Only a CHANGE walks the
  // EXISTING N8 controlled-recreate path (performControlledRecreate — no
  // parallel rebuild): that leg re-derives the exclusion for the current
  // network at its own assembly (lanExclusionForConfig leg=recreate), keeps
  // the core session (only the platform connection is rebuilt and the fresh
  // TUN fd re-fed — no re-login), and fails closed on any leg. Unchanged =>
  // nothing is touched. Called from the status watcher tick; the armed flag
  // makes it one-shot per switch.
  private checkLanRoamRecreate(): void {
    if (!this.lanRoamArmed) {
      return;
    }
    if (!lanRoamCheckDue(Date.now(), this.lanRoamLastSwitchAtMs, LAN_ROAM_STABLE_MS)) {
      return; // still inside the stability window — a later tick decides
    }
    this.lanRoamArmed = false;
    // Re-derive through the SAME collector + pure derivation both config
    // assembly legs use, so the comparison can never drift from what a
    // rebuild would actually graft.
    const probe: LanExclusionProbe = physicalDefaultLanProbe();
    const current: LanExclusionResult =
      deriveLanExclusion(probe, loadVpnTunnelConfig().tunnelAddress);
    const fromSignature: string = this.appliedLanExclusion;
    const toSignature: string = lanExclusionSignature(current.entry);
    const decision: LanRoamDecision = lanRoamDecision({
      evidenceMode: this.n2Evidence !== undefined,
      destroyRequested: this.destroyRequested,
      recreateInFlight: this.recreateInFlight,
      hasSettledConnection:
        this.connection !== undefined && this.createSettled && this.tunFd >= 0,
      networkLost: physicalDefaultNetId() === 0,
      productionGateBlocked: this.n2CdProductionBlocked(),
      appliedSignature: fromSignature,
      currentSignature: toSignature
    });
    // One line per settled roam check: from/to = live vs re-derived exclusion
    // signature, action = recreate|skip, result = 'triggered' for a rebuild
    // (the follow-up line below carries its outcome) or the skip token.
    hilog.info(DOMAIN, TAG,
      'VPN_LAN_ROAM|requestId=%{public}s|from=%{public}s|to=%{public}s|' +
      'action=%{public}s|result=%{public}s',
      this.requestId, fromSignature, toSignature, decision.action,
      decision.action === 'recreate' ? 'triggered' : decision.reason);
    if (decision.action !== 'recreate') {
      return;
    }
    // The follow-up line records the rebuild's outcome ('ok' | 'fail:<stage>'
    // | 'skip:<reason>' — lastRecreateOutcome) on the roam trail; the
    // recreate's own lines keep the full per-leg truth.
    this.performControlledRecreate(decision.reason).then((): void => {
      hilog.info(DOMAIN, TAG,
        'VPN_LAN_ROAM|requestId=%{public}s|from=%{public}s|to=%{public}s|' +
        'action=recreate|result=%{public}s',
        this.requestId, fromSignature, toSignature, this.lastRecreateOutcome);
    }).catch((error: Error): void => {
      // performControlledRecreate never rejects (it catches internally and
      // fails closed); this belt only guards an unexpected throw.
      hilog.error(DOMAIN, TAG,
        'VPN_LAN_ROAM|requestId=%{public}s|from=%{public}s|to=%{public}s|' +
        'action=recreate|result=fail:unexpected-throw|summary=%{public}s',
        this.requestId, fromSignature, toSignature, safeError(error));
    });
  }

  // The one teardown-path line for a failed physical-network recovery, logged
  // AND forwarded to the armed ring verbatim (same string, no drift).
  private n2NetworkRecoveryFail(error: Error): void {
    const line: string = `VPN_NETWORK_RECOVERY_FAIL|summary=${safeError(error)}`;
    hilog.warn(DOMAIN, TAG, '%{public}s', line);
    this.n2RingAppend(line);
    // Persistent copy WITHOUT the error summary (that carries a message; the
    // record stays a fixed token plus the stage).
    const milestoneFields: DiagFields = { 'stage': 'physical-recovery', 'result': 'fail' };
    this.diagMilestone('vpn_net_recovery_failed', milestoneFields);
  }

  private async recoverPhysicalNetwork(): Promise<void> {
    if (!this.networkRecoveryReady || this.destroyRequested) { return; }
    // N2 first batch C/D columns: the formal-switch gate. Checked at the ENTRY
    // (the outer !gated path, the finally retry and the recreate-finally retry
    // all funnel through here) and again at EVERY round of the chase loop below,
    // so an in-flight attempt of an older epoch can never `continue` into a
    // gated one. A gated call returns WITHOUT touching networkRecoveryActive or
    // networkRecoveryPending: nothing about the plan's epoch has been attempted.
    if (this.n2CdProductionBlocked()) { return; }
    if (this.networkRecoveryActive) {
      this.networkRecoveryPending = true;
      return;
    }
    this.networkRecoveryActive = true;
    let attemptEpoch: number = 0;
    try {
      do {
        attemptEpoch = this.networkEpoch;
        if (this.n2CdProductionBlocked(attemptEpoch)) { break; }
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
          this.connection !== vpn || this.n2CdProductionBlocked(epoch)) { continue; }
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
          this.connection !== vpn || this.n2CdProductionBlocked(epoch)) { continue; }
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
              this.connection === vpn && !this.n2CdProductionBlocked(epoch)) {
              signalFed = connectorSignalFeed(sock.fd, signalAddr).ok;
            }
          } finally { socketSupplyCloseFd(sock.fd); }
          if (!signalFed) { break; }
        }
        if (!await this.feedSpareProtectedSocket(vpn, epoch)) { break; }
        if (epoch !== this.networkEpoch || this.destroyRequested || this.recreateInFlight ||
          this.connection !== vpn || this.n2CdProductionBlocked(epoch)) { continue; }
        this.networkDirty = false;
        const recoveredLine: string = `VPN_NETWORK_RECOVERED|requestId=${this.requestId}|` +
          `netId=${String(netId)}`;
        hilog.info(DOMAIN, TAG, '%{public}s', recoveredLine);
        this.n2RingAppend(recoveredLine);
        const milestoneFields: DiagFields = { 'net': netId };
        this.diagMilestone('vpn_net_recovered', milestoneFields);
        // N2 first batch C/D columns: on the D column this recovery IS the
        // preparation's landing — the pre-set plan is only in position for its
        // formal 蜂窝 → Wi-Fi switch after the core recorded it.
        this.n2CdReadyAfterRecovery();
      } while (attemptEpoch !== this.networkEpoch && !this.destroyRequested &&
        this.networkNetId !== 0 && !this.n2CdProductionBlocked());
    } finally {
      this.networkRecoveryActive = false;
      const retry: boolean = this.networkRecoveryPending;
      this.networkRecoveryPending = false;
      if (retry && this.networkDirty && !this.recreateInFlight && !this.destroyRequested &&
        !this.n2CdProductionBlocked()) {
        this.recoverPhysicalNetwork().catch((error: Error): void => {
          this.n2NetworkRecoveryFail(error);
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
      this.n2ErrorAndRing(`N2_EVIDENCE_REFUSED|requestId=${this.requestId}|` +
        `reason=${this.n2EvidenceError}|diag=true|is_evidence=false`);
      return;
    }
    if (this.n2Evidence !== undefined) {
      this.probeN2ProcNet();
      const armedLine: string = `N2_EVIDENCE_ARMED|requestId=${this.requestId}|` +
        `trusted=${SELF_BUNDLE_NAME}|capture32=${this.n2Evidence.capture32.join(',')}|` +
        `endpointExclusions=false|hdcControl=${this.n2Evidence.hdcControlIp || 'not-supplied'}|` +
        `allowHdcOverlap=${String(this.n2Evidence.allowHdcOverlap)}|diag=true|is_evidence=false`;
      hilog.warn(DOMAIN, TAG, '%{public}s', armedLine);
      this.n2RingAppend(armedLine);
    }
    if (!this.endpointExclusionsEnabled &&
      (this.protectDeviationRouteExclusion || this.diagCaptureRoute)) {
      hilog.error(DOMAIN, TAG, 'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=dev-switch-conflict',
        this.requestId);
      return;
    }

    let setupKey: string = '';
    let accountJwt: string = '';
    let authMode: string = ACCOUNT_MODE_SETUP_KEY;
    let device: DeviceConfigFile;
    let credentialFailure: string = 'credentials-missing';
    try {
      const modePath: string = `${filesDir}/${ACCOUNT_MODE_FILE_NAME}`;
      if (fs.accessSync(modePath)) {
        authMode = parseAuthMode(fs.readTextSync(modePath));
      }
      if (authMode === ACCOUNT_MODE_SETUP_KEY) {
        // Preserve the existing development-time /data/local/tmp override for
        // the legacy Setup Key and device-config files.
        setupKey = readCredentialFileText(filesDir, setupKeyFile);
        device = parseDeviceConfig(readCredentialFileText(filesDir, deviceFile));
      } else if (authMode === ACCOUNT_MODE_ACCOUNT) {
        credentialFailure = 'account-login-required';
        device = parseDeviceConfig(readPrivateFileText(filesDir, deviceFile));
        const stored = await loadAccountCredentials(filesDir, device.management_url);
        const decision: string = decideAccountCredentialMode(authMode, stored !== null,
          stored?.jwt ?? '', stored?.expiresAtMs ?? 0, Date.now());
        if (decision !== ACCOUNT_MODE_ACCOUNT || stored === null) {
          throw new Error('account-login-required');
        }
        accountJwt = stored.jwt;
      } else {
        credentialFailure = 'account-login-required';
        throw new Error('account-login-required');
      }
    } catch (error) {
      this.writeConnectFailureForUi(credentialFailure);
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=credentials|errorClass=%{public}s',
        this.requestId, credentialFailure);
      return;
    }

    if (this.destroyRequested) {
      return;
    }

    // 2026-10-07 per-NetID route selection (decision B): load the stored
    // selection for the management server CONFIGURED in this device config
    // exactly ONCE, before any socket is opened or the connector is started —
    // the selection is native generation state, fixed at start (a later UI
    // change applies through stop-ACK + reconnect, never a live prefs watch).
    //   * no document / a document scoped to ANOTHER server -> null = the
    //     pre-existing default all-on start JSON, BYTE-IDENTICAL (the third
    //     argument is omitted exactly like every pre-existing call);
    //   * an existing document that cannot be trusted (malformed, wrong
    //     version, unreadable) -> a CONTROLLED start failure with a stable
    //     class token. Never a silent fall back to all-on: that would route
    //     traffic the user disabled, and the next save would overwrite the
    //     evidence. No raw document text ever reaches a log.
    let routeSelection: RouteSelection | null = null;
    try {
      routeSelection = await loadRouteSelection(filesDir, device.management_url);
    } catch (error) {
      const reason: string = (error as RoutePreferencesError).reason;
      const selectionFailure: string = typeof reason === 'string' && reason.length > 0
        ? `route-selection-${reason}` : 'route-selection';
      hilog.error(DOMAIN, TAG,
        'VPN_ROUTE_SELECTION_INVALID|requestId=%{public}s|stage=route-selection|' +
        'errorClass=%{public}s|action=connector-not-started',
        this.requestId, selectionFailure);
      this.writeConnectFailureForUi(selectionFailure);
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
    const creds: Record<string, Object> = authMode === ACCOUNT_MODE_ACCOUNT
      ? { 'jwt': accountJwt } : { 'setup_key': setupKey };
    let start: ConnectorStartResult;
    try {
      start = connectorStartWithSocket(mgmtFd,
        buildConnectorConfigJson(device, forceDefaultRoute, routeSelection),
        JSON.stringify(creds), connectAddr, physicalDefaultDnsServers());
    } finally {
      socketSupplyCloseFd(mgmtFd);
    }
    if (!start.started) {
      // 2026-10-06 登录失败可见性: the connector refused the start (config /
      // credential class) — persist the class so the home card shows the
      // engine's message instead of a stale 连接中….
      this.writeConnectFailureForUi(
        start.error !== undefined && start.error !== null && start.error.length > 0
          ? start.error : 'none');
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
        const failedLine: string = `N2_TUN_DEST_CONFIG_FAILED|requestId=${this.requestId}|` +
          `error=${result.error ?? 'unknown'}|diag=true|is_evidence=false`;
        hilog.error(DOMAIN, TAG, '%{public}s', failedLine);
        this.n2RingAppend(failedLine);
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
    // 2026-10-07 client-detail increment: remember the server this start
    // dialed so the published document can name it (authority only).
    this.managementUrl = device.management_url;
    // seed ONE spare protected socket so a single reconnect dial can
    // re-protect; further resupply policy is an N4+ increment (a starved
    // queue fails dials CLOSED — never unprotected)
    this.startSocketSupply();
    this.feedSpareProtectedSocket(connection);
    hilog.info(DOMAIN, TAG, 'VPN_CONNECTOR_STARTED|requestId=%{public}s|state=%{public}s|' +
      'mgmtSocket=protected|connectAddr=%{public}s',
      this.requestId, start.state ?? 'unknown', connectAddr);

    // 2026-10-12 upstream tap handling: the engine reports its OWN state the moment
    // the run starts, before the bounded network-map wait below — this is the
    // cross-process form of the official client's synchronously fired state
    // callback (EngineRunner.java:182 `notifyServiceStateListeners(true)` on the
    // same lock that set engineIsRunning, consumed by MainActivity.java:1015-1027).
    // It replaces the deleted connect-attempt ACK file: the UI's pending action reads
    // the engine's state (connecting here) instead of a shell-invented document, and
    // it is what lets the 7 s pending window fall back onto a REAL state instead of
    // onto "nothing reported". Best-effort: a refused probe/write costs the UI one
    // fallback, never the VPN.
    this.publishEngineStateReport(true);

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
      // 2026-10-06 登录失败可见性: the map never arrived inside the box — in
      // practice the LOGIN failed (fatal auth or still retrying against an
      // unreachable/wrong server). Persist the connector's OWN last error
      // class BEFORE the stop wipes the live status channel, so the home
      // card carries the matching Chinese reason (e.g. 设备未注册或 Setup Key
      // 无效) next to its official state instead of a stale 连接中…. The rare
      // map-but-no-signal-uri case records the config token instead.
      let failureClass: string = net === null ? 'none' : 'no-signal-uri';
      if (net === null) {
        try {
          const loginStatus: ConnectorStatusResult | null = pollStatusOrNull();
          if (loginStatus !== null && loginStatus.last_error !== null &&
            loginStatus.last_error !== undefined &&
            typeof loginStatus.last_error.class === 'string' &&
            loginStatus.last_error.class.length > 0) {
            failureClass = loginStatus.last_error.class;
          }
        } catch (statusError) {
          failureClass = 'none';
        }
      }
      this.writeConnectFailureForUi(failureClass);
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
      : { fatal: false, routes: [], dnsRoutes: [] };
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
    this.frozenDnsEndpointExclusions = exclusion.dnsRoutes;
    // 动态 LAN 排除先于端点冻结集 graft（与旧静态项相同的落位：managed 路由
    // 之后、端点 /32 之前；mergeExcludedRouteEntries 对同 destination+prefix
    // 去重，端点 /32 与 LAN /24 不会冲突）
    const lan: LanExclusionResult =
      this.lanExclusionForConfig(baseTemplate.tunnelAddress, 'initial');
    const lanRoutes: ExcludedRouteEntry[] = lan.entry !== null ? [lan.entry] : [];
    const base: VpnTunnelConfig = withEndpointExclusions(baseTemplate,
      lanRoutes.concat(exclusion.routes));
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
        this.n2ErrorAndRing(`N2_EVIDENCE_REFUSED|requestId=${this.requestId}|` +
          `reason=${safeError(error as Error)}|diag=true|is_evidence=false`);
        connectorStop();
        return;
      }
      const routesLine: string = `N2_EVIDENCE_ROUTES|requestId=${this.requestId}|leg=initial|` +
        `capture32=${this.n2Evidence.capture32.join(',')}|defaultRoutes=0|excludedRoutes=0|` +
        `diag=true|is_evidence=false`;
      hilog.info(DOMAIN, TAG, '%{public}s', routesLine);
      this.n2RingAppend(routesLine);
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
    // base fallback shape is not the core's desired rendering. 2026-10-09: an
    // AVAILABLE snapshot with an EMPTY desired set is stashed as `[]` (not
    // null) so ackAppliedRoutesInitial ACKs the real empty applied set.
    this.pendingAppliedRoutes =
      net !== null && net.routes !== undefined && net.routes !== null
        ? net.routes.map((r: CoreRouteEntry): string => r.network)
        : null;
    // 场景B roaming: the exclusion actually being submitted — the
    // live-config truth a later network switch compares against
    // (checkLanRoamRecreate). Recorded at the submit point (not at
    // derivation) so an aborted leg can never leave a signature that was
    // never created behind.
    this.appliedLanExclusion = lanExclusionSignature(lan.entry);
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
      // 2026-10-12 upstream tap handling: the engine keeps REPORTING while it is
      // starting. Without this the document written at the run's start would go
      // stale (> STATUS_MAX_AGE_MS) inside the 15 s box and the UI would read
      // "nothing reported" mid-start. Throttled inside the helper.
      this.publishEngineStateReport();
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

  // 动态基础 LAN 排除 (scenario-A roaming fix, LAN-ROUTE-ROAMING-ANALYSIS.md
  // §4 step 1): the base host-LAN exclusion is no longer a static constant in
  // DEFAULT_VPN_TUNNEL_CONFIG — each config assembly leg (the initial
  // create() AND every recreate) derives it from the local IPv4 prefix of the
  // CURRENT physical default network (physicalDefaultLanProbe +
  // deriveLanExclusion): home Wi-Fi 192.168.50.x/24 reproduces the retired
  // static 192.168.50.0/24 entry verbatim, while cellular (or a probe with no
  // usable local prefix) adds NO LAN exclusion — the managed route to the
  // home subnet then reaches the tunnel, which is the whole point of remote
  // access over cellular. The N2 evidence mode keeps its own baseline
  // (withoutN2ExcludedRoutes strips the base exclusions; the LAN piece is not
  // grafted there either).
  private lanExclusionForConfig(gateway: string, leg: string): LanExclusionResult {
    if (this.n2Evidence !== undefined) {
      return { entry: null, network: '', prefixLength: 0, reason: 'n2-evidence-diag' };
    }
    const probe: LanExclusionProbe = physicalDefaultLanProbe();
    const outcome: LanExclusionResult = deriveLanExclusion(probe, gateway);
    hilog.info(DOMAIN, TAG,
      'VPN_LAN_EXCLUSION|requestId=%{public}s|leg=%{public}s|reason=%{public}s|' +
      'network=%{public}s|prefix=%{public}d|linkAddresses=%{public}d',
      this.requestId, leg, outcome.reason, outcome.network, outcome.prefixLength,
      probe.linkAddresses.length);
    return outcome;
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
    // roam-1005-0124: mark the RESOLVER subset of the built set. A recreate
    // after a network switch re-derives exactly this subset from the
    // then-current physical network (cross-network DNS /32 exclusions broke
    // on-device 192.168.50.1 reachability over cellular); the remaining
    // endpoint /32s stay frozen as before. A resolver address that also
    // appears as another kind's resolved IP collapses into one route and is
    // classified here as dns — the recreate then treats it as re-derivable.
    const dnsAddresses: string[] = [];
    for (let i = 0; i < dns.endpoints.length; i++) {
      const address: string = stripDialPort(dns.endpoints[i].host);
      if (dnsAddresses.indexOf(address) < 0) {
        dnsAddresses.push(address);
      }
    }
    const dnsRoutes: ExcludedRouteEntry[] = build.routes.filter(
      (route: ExcludedRouteEntry): boolean => dnsAddresses.indexOf(route.destination) >= 0);
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
    return { fatal: fatal, routes: build.routes, dnsRoutes: dnsRoutes };
  }

  // roam-1005-0124: the frozen endpoint /32s MINUS the resolver subset — the
  // pieces a recreate grafts verbatim (management/signal/relay/stun
  // resolutions are session-stable; only DNS follows the live network). The
  // N2 evidence leg keeps the WHOLE frozen set (no split, byte-identical to
  // the pre-existing behavior — its capture discipline is fixed at session
  // start).
  private frozenRoutesWithoutResolvers(): ExcludedRouteEntry[] {
    if (this.n2Evidence !== undefined) {
      return this.frozenEndpointExclusions;
    }
    const dnsRoutes: ExcludedRouteEntry[] = this.frozenDnsEndpointExclusions;
    const frozen: ExcludedRouteEntry[] = [];
    for (let i = 0; i < this.frozenEndpointExclusions.length; i++) {
      const route: ExcludedRouteEntry = this.frozenEndpointExclusions[i];
      let isDns: boolean = false;
      for (let j = 0; j < dnsRoutes.length; j++) {
        if (dnsRoutes[j].destination === route.destination &&
          dnsRoutes[j].prefixLength === route.prefixLength) {
          isDns = true;
          break;
        }
      }
      if (!isDns) {
        frozen.push(route);
      }
    }
    return frozen;
  }

  // roam-1005-0124: the recreate leg's resolver (/32) exclusions, derived
  // from the CURRENT physical default network — the same platform facts and
  // pure branch order the initial freeze used (derivePlatformDnsEndpoints),
  // with the resolver addresses short-circuiting the literal classification
  // (a resolver IS an IPv4 literal — no DNS leg runs inside the bounded
  // recreate). A derivation gap (API error / no physical net / no usable
  // resolver) yields NO resolver exclusions, honestly logged — never the
  // stale cross-network set (cellular must not keep excluding the home
  // Wi-Fi resolver). The N2 evidence leg returns nothing here (the whole
  // frozen set, resolvers included, was grafted above).
  private recreateResolverExclusionRoutes(gateway: string): ExcludedRouteEntry[] {
    if (this.n2Evidence !== undefined) {
      return [];
    }
    const dns: DnsDerivationResult = derivePlatformDnsEndpoints();
    const resolutions: EndpointResolution[] = [];
    for (let i = 0; i < dns.endpoints.length; i++) {
      resolutions.push({ kind: 'dns', host: dns.endpoints[i].host, ok: true,
        address: dns.endpoints[i].host });
    }
    const build: ExclusionBuildResult = buildExcludedRouteEntries(resolutions, gateway);
    hilog.info(DOMAIN, TAG,
      'VPN_RECREATE_DNS_EXCLUSIONS|requestId=%{public}s|dns-branch=%{public}s|' +
      'resolvers=%{public}d|routes=%{public}d|dnsGap=%{public}s|diag=true|is_evidence=false',
      this.requestId, dns.branch, dns.endpoints.length, build.routes.length,
      dns.gap ? dns.gapReason : 'none');
    return build.routes;
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
    // N2 first batch C/D columns: the app-internal evidence ring is armed-only —
    // an arming change stops it (the FILE stays for the operator to fetch) and
    // stops its 60 s statistics cadence with it.
    core.n2_ring_disable();
    this.n2RingStopStats();
    this.n2CdClearLatch();
    // N2 first batch C/D columns: this is the EXPLICIT termination of the
    // previous evidence collection (a fresh start flow — the only in-process
    // release of a held production gate, alongside the teardown paths): an
    // unarmed/new session must never inherit the previous session's block.
    this.n2CdGateReset();
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
      // N2 first batch C/D columns: the accepted evidence arm also opens the
      // app-internal evidence ring, so the lines of this armed session survive
      // the cellular-phase HiLog rotation/flow control.
      this.n2RingConfigure();
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
          const readableLine: string = `N2_PROC_NET|path=${path}|readable=true|` +
            `lines=${lines}|uid=${uid}|diag=true|is_evidence=false`;
          hilog.info(DOMAIN, TAG, '%{public}s', readableLine);
          this.n2RingAppend(readableLine);
        } catch (error) {
          const unreadableLine: string = `N2_PROC_NET|path=${path}|readable=false|lines=0|` +
            `uid=${uid}|code=${String((error as BusinessFailure).code ?? 'no-code')}|` +
            `diag=true|is_evidence=false`;
          hilog.warn(DOMAIN, TAG, '%{public}s', unreadableLine);
          this.n2RingAppend(unreadableLine);
        }
      }
    }
  }

  // -------------------------------------------------------------------------
  // N2 first batch C/D columns (2026-09-29): the armed-only app-internal
  // evidence ring, and the App-side switching sandwich.
  // -------------------------------------------------------------------------

  // The ring's SHELL half. The core appends every line IT emits (`N2_TUN_*`,
  // `N7_WG_FEED`, most `N2_*`) inside `hilog::emit`; the lines THIS process
  // formats for the armed session are forwarded here, verbatim, at the same
  // call sites that log them. Armed-only: the ring is configured by the
  // accepted `n2_evidence` arm and disabled on every arming change, so an
  // unarmed extension forwards nothing (and the core would drop it anyway).
  private n2RingAppend(line: string): void {
    if (this.n2Evidence === undefined) {
      return;
    }
    try {
      core.n2_ring_append(line);
    } catch (error) {
      // The core counts its own write failures and reports them through
      // N2_RING_STATS / N2_RING_WRITE_FAILED; a thrown forward is swallowed
      // here on purpose so HiLog output can never change the ring's writers.
    }
  }

  // One `N2_*` line at WARN level, logged and forwarded VERBATIM to the ring.
  // The failure paths of the private channels use this so the ring copy can
  // never drift from the hilog text (there is exactly one string).
  private n2WarnAndRing(line: string): void {
    hilog.warn(DOMAIN, TAG, '%{public}s', line);
    this.n2RingAppend(line);
  }

  private n2ErrorAndRing(line: string): void {
    hilog.error(DOMAIN, TAG, '%{public}s', line);
    this.n2RingAppend(line);
  }

  // Configure the ring for this armed session. The file lives in the App's own
  // sandbox (the ONLY place the App can write: /data/local/tmp is not in its
  // visible view and the hdc shell domain cannot read the sandbox), so the
  // fetch path is the bundle-scoped transfer — `hdc file recv -b
  // cn.alfadb.netbird ./data/storage/el2/base/files/<file> <host>` on a
  // DEBUG-signed, launched App — with the `n2_ring_dump` private channel as the
  // fallback when `-b` is unavailable (the OpenHarmony hdc contract refuses the
  // bundle transfer with E003001 for a non-debug-signed App and E005004 on an
  // SDK/ROM that does not support `-b`), and E005102 for a path outside the app
  // data directory. See docs/n2-cd-app-sandwich-20260929.md.
  private n2RingConfigure(): void {
    const path: string = `${this.context.filesDir}/${N2_RING_FILE_NAME}`;
    // The segment cap comes from the ACCEPTED ticket
    // (`n2_evidence.ring_segment_bytes`, validated by parseN2Evidence,
    // shrink-only 64 KiB..4 MiB); an absent key keeps the 4 MiB default.
    // Diagnostics-only — the campaign executor never sends it
    // (docs/n2-cd-app-sandwich-20260929.md).
    const segmentBytes: number = this.n2Evidence !== undefined ?
      this.n2Evidence.ringSegmentBytes : N2_RING_MAX_BYTES;
    try {
      const reply: Record<string, Object> = JSON.parse(
        core.n2_ring_configure(path, segmentBytes, N2_RING_SEGMENTS)) as
        Record<string, Object>;
      if (reply['ok'] !== true) {
        this.n2ErrorAndRing(`N2_RING_CONFIGURE_FAILED|file=${N2_RING_FILE_NAME}|` +
          `error=${String(reply['error'] ?? 'unknown')}|diag=true|is_evidence=false`);
      }
    } catch (error) {
      this.n2ErrorAndRing(`N2_RING_CONFIGURE_FAILED|file=${N2_RING_FILE_NAME}|` +
        `error=core-throw|diag=true|is_evidence=false`);
    }
    this.n2RingStartStats();
  }

  // The ring's own 60 s cadence. The line is emitted BY THE CORE
  // (`n2_ring_status`), so its name, every counter field and its membership in
  // the ring are the core's own status formatter — the shell adds no second
  // vocabulary. Started by the accepted arm (the only place the ring is
  // configured) and stopped by every arming change and by the teardown.
  private n2RingStartStats(): void {
    this.n2RingStopStats();
    this.n2RingStatsTimer = setInterval((): void => {
      if (this.n2Evidence === undefined || this.destroyRequested) {
        return;
      }
      try {
        core.n2_ring_status();
      } catch (error) { /* the core owns its counters; a throw is not fatal here */ }
    }, N2_RING_STATS_MS);
  }

  private n2RingStopStats(): void {
    if (this.n2RingStatsTimer !== -1) {
      clearInterval(this.n2RingStatsTimer);
      this.n2RingStatsTimer = -1;
    }
  }

  // N2 first batch C/D columns: the `n2_cd_plan` private channel. ONE launch =
  // ONE plan (the request file is deleted in `finally`), and the host issues it
  // while it still has a live link (Wi-Fi). The App then does everything else
  // alone: measure the starting bearer, wait for the switch, send the pre
  // control on the NEW network, wait for the kind's production receipt there,
  // send the post control.
  //
  // Trigger (documented in docs/n2-cd-app-sandwich-20260929.md):
  //   aa start -b cn.alfadb.netbird -a EntryAbility \
  //     --ps n2h_arm_options '<the same arm ticket that armed this session>' \
  //     --ps n2_cd_plan '{"column":"C","kind":"M","rep":1,"id":"cd-C-M-1"}'
  // EntryAbility writes it to <filesDir>/netbird-n2-cd-plan; this poll picks it
  // up on the next watcher tick (≤ STATUS_POLL_MS = 5 s) and consumes it. The
  // operator is then free to switch networks; the plan is executed on-device.
  private pollN2CdPlan(): void {
    if (this.destroyRequested) {
      return;
    }
    const path: string = `${this.context.filesDir}/${N2_CD_FILE_NAME}`;
    let text: string = '';
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      text = fs.readTextSync(path);
    } catch (error) {
      this.n2WarnAndRing(`N2_CD_PLAN_READ_FAILED|requestId=${this.requestId}|` +
        `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
      return;
    }
    try {
      const decision: N2CdPlanDecision = parseN2CdPlan(text);
      if (decision.request === undefined) {
        const reject: string = formatN2CdPlanShellReject(this.requestId, decision.error);
        hilog.warn(DOMAIN, TAG, '%{public}s', reject);
        this.n2RingAppend(reject);
        return;
      }
      const request: N2CdPlanRequest = decision.request;
      const armed: N2EvidenceOptions | undefined = this.n2Evidence;
      if (armed === undefined) {
        const refused: string = formatN2CdPlanShellLine(this.requestId, request, 'plan', 'none',
          'rejected', 'not-armed');
        hilog.warn(DOMAIN, TAG, '%{public}s', refused);
        return;
      }
      if (this.n2CdActive.length > 0 || this.n2CdBusy) {
        const busy: string = formatN2CdPlanShellLine(this.requestId, request, 'plan', 'none',
          'rejected', 'plan-already-active');
        hilog.warn(DOMAIN, TAG, '%{public}s', busy);
        this.n2RingAppend(busy);
        return;
      }
      const target: string = selectN2CdTarget(armed.outerDestinations, request.kind);
      const facts: PhysicalBearerFacts = physicalDefaultBearerFacts();
      const bearer: N2BearerFacts = classifyN2Bearer(facts.bearerTypes, facts.linkAddresses);
      this.n2CdBeforeBearer = bearer.bearer;
      const startVerdict: string = target.length === 0
        ? 'no-target'
        : n2CdStartVerdict(request.column, bearer.bearer);
      let reply: Record<string, Object> = {};
      try {
        reply = JSON.parse(core.n2_cd_plan(request.id, request.column, request.kind,
          request.rep, target, bearer.bearer, bearer.local, startVerdict)) as
          Record<string, Object>;
      } catch (error) {
        // ArkTS refuses an untyped object literal assigned to a Record, so the
        // thrown-call branch re-parses the same fixed refusal document.
        reply = JSON.parse('{"ok":false,"result":"rejected","reason":"core-throw"}') as
          Record<string, Object>;
      }
      const accepted: boolean = reply['ok'] === true;
      const line: string = formatN2CdPlanShellLine(this.requestId, request, 'plan',
        accepted ? String(reply['pre_marker'] ?? '') : 'none',
        accepted ? 'planned' : 'rejected',
        String(reply['reason'] ?? 'none'));
      hilog.info(DOMAIN, TAG, '%{public}s', line);
      this.n2RingAppend(line);
      if (!accepted || target.length === 0) {
        return;
      }
      this.n2CdActive = request.id;
      this.n2CdColumn = request.column;
      this.n2CdKind = request.kind;
      this.n2CdRep = request.rep;
      this.n2CdPreparePending = false;
      // The gate is deliberately NOT touched here: a terminal hold outlives any
      // later plan of the same armed session (see n2CdGateArm / n2CdGateHold) —
      // accepting a new plan is not the explicit termination the hold waits for.
      // Nothing else happens here ON PURPOSE: acceptance only REGISTERS the
      // plan. Both controls belong to the plan's own network — the pre control
      // is owed on the NEW network after the formal switch (n2CdOnSwitch /
      // pollN2CdStep), and a control sent at acceptance time would sit on the
      // OLD one, which the core refuses by name (`pre-before-switch`). The Wb
      // row is NOT exempt: the core's revised state machine owes it a pre and a
      // post control too (its receipt is the new generation's protect settle).
    } finally {
      try {
        fs.unlinkSync(path);
      } catch (error) {
        this.n2WarnAndRing(`N2_CD_PLAN_CONSUME_FAILED|requestId=${this.requestId}|` +
          `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
      }
    }
  }

  // The plan's own view of ONE measured network transition. Called from
  // `physicalNetworkAvailable` at the switch instant and BEFORE
  // `connectorPhysicalNetworkChanged`/the re-feed, so the core records the OLD
  // network's live registration as the baseline its new-network receipt must
  // differ from. Returns true when the plan OWNS the production-recovery gate of
  // this transition: the shell must then NOT recover before the core's own poll
  // answers `step=recover` (its tee HIT the pre control on the NEW network).
  //
  // D column: the Wi-Fi → 非-Wi-Fi hop is the PREPARATION hop, never the D
  // measurement — recorded, no control, no gate. The formal 蜂窝 → Wi-Fi switch
  // is reported only after the cellular-side recovery handed the core
  // `n2_cd_ready`; anything earlier is refused by the core's own state machine.
  //
  // N2 first batch C/D columns: on a FORMAL switch the gate is armed
  // SYNCHRONOUSLY here — before the identity capture and before the switch
  // report, and therefore before `connectorPhysicalNetworkChanged` and any
  // async continuation of an older recovery can run. The pre control is NOT
  // sent from here any more: it is owed on the new network only AFTER the core's
  // own old-connection cleanup (`physicalNetworkAvailable`), which is where the
  // step poll now runs.
  private n2CdOnSwitch(netId: number): boolean {
    const armed: N2EvidenceOptions | undefined = this.n2Evidence;
    const id: string = this.n2CdActive;
    if (id.length === 0 || armed === undefined || this.destroyRequested) {
      return false;
    }
    const facts: PhysicalBearerFacts = physicalDefaultBearerFacts();
    const bearer: N2BearerFacts = classifyN2Bearer(facts.bearerTypes, facts.linkAddresses);
    const before: string = this.n2CdBeforeBearer;
    const after: string = bearer.bearer;
    const request: N2CdPlanRequest = { column: this.n2CdColumn, kind: this.n2CdKind,
      rep: this.n2CdRep, id: id };
    const step: string = n2CdTransitionStep(this.n2CdColumn, before, after);
    if (step === 'none') {
      // Not this plan's transition at all (D, still on Wi-Fi, another Wi-Fi
      // network): the plan keeps waiting and the ordinary recovery runs.
      return false;
    }
    if (step === 'prepare') {
      const verdict: string = n2CdPrepareVerdict(after);
      let coreResult: string = 'rejected';
      let coreReason: string = 'core-throw';
      try {
        const reply: Record<string, Object> = JSON.parse(core.n2_cd_prepare(id, verdict, after,
          bearer.local)) as Record<string, Object>;
        coreResult = String(reply['result'] ?? 'rejected');
        coreReason = String(reply['reason'] ?? 'none');
      } catch (error) { /* the core's own line carries its refusal */ }
      this.n2CdBeforeBearer = after;
      this.n2CdPreparePending = coreResult === 'prepared';
      const line: string = formatN2CdPlanShellLine(this.requestId, request, 'prepare', 'none',
        coreResult === 'prepared' ? 'prepared' : (coreResult === 'unsat' ? 'unsat' : 'rejected'),
        coreReason);
      hilog.info(DOMAIN, TAG, '%{public}s', line);
      this.n2RingAppend(line);
      this.n2CdLatchAfter(coreResult);
      return false;
    }
    // THE GATE, armed before anything else can observe this epoch: the formal
    // switch below is about to make `networkDirty` true and to have the core
    // drop the OLD registrations; from this instant no supply entry may hand the
    // core a production connection of the new network until the core's own poll
    // answers `step=recover` for THIS plan at THIS epoch. Fail-closed on EVERY
    // outcome of the report below: a terminal refusal holds it (n2CdLatchAfter),
    // a non-terminal one leaves it closed.
    this.n2CdGateArm();
    const direction: string = n2CdDirectionVerdict(this.n2CdColumn, before, after);
    // The receipt's baseline: the identity of the kind's LIVE registration on
    // the OLD network, exactly as the `N2_SOCK` receipts name it — `fd` = the
    // shell's original descriptor, `dup_fd` = the core-owned copy,
    // `protect_generation` = the shell's network epoch. The `dup_fd` is core
    // owned, so it can only come from the core's own registry; -1 for every
    // field while it has no live registration of this kind, and the core then
    // closes the plan by name instead of receiving an invented baseline.
    let identityFd: number = -1;
    let identityDup: number = -1;
    let identityGeneration: number = -1;
    try {
      const identity: Record<string, Object> = JSON.parse(core.n2_cd_identity(this.n2CdKind)) as
        Record<string, Object>;
      if (identity['ok'] === true) {
        identityFd = Number(identity['fd'] ?? -1);
        identityDup = Number(identity['dup_fd'] ?? -1);
        identityGeneration = Number(identity['generation'] ?? -1);
      }
    } catch (error) { /* not measured: every field stays -1 (fail-closed) */ }
    let result: string = 'rejected';
    let reason: string = 'core-throw';
    try {
      const reply: Record<string, Object> = JSON.parse(core.n2_cd_switch_identified(id, direction,
        after, bearer.local, netId, identityFd, identityDup, identityGeneration)) as
        Record<string, Object>;
      result = String(reply['result'] ?? 'rejected');
      reason = String(reply['reason'] ?? 'none');
    } catch (error) { /* the core's own line carries its refusal */ }
    const line: string = formatN2CdPlanShellLine(this.requestId, request, 'switch', 'none',
      result === 'switched' ? 'switched' : (result === 'unsat' ? 'unsat' : 'rejected'), reason);
    hilog.info(DOMAIN, TAG, '%{public}s', line);
    this.n2RingAppend(line);
    this.n2CdLatchAfter(result);
    if (result !== 'switched') {
      // A terminal refusal already closed the core's plan AND (through
      // n2CdLatchAfter) HELD the gate armed above: the outer `!gated` path must
      // not recover, or the failed epoch would be polluted by a recovery the
      // core never released. A non-terminal one (`switch-already-reported`)
      // keeps BOTH the plan and the shell latch, and the gate keeps whatever it
      // held before (its release stays bound to its own plan and epoch).
      return false;
    }
    this.n2CdBeforeBearer = after;
    // The gate stays 'blocked' here; the pre control is owed on the NEW network
    // and is sent by `physicalNetworkAvailable` AFTER the core's own
    // old-connection cleanup (`connectorPhysicalNetworkChanged`). See there.
    return true;
  }

  // D column: the cellular-side recovery completed, so the PRE-SET plan is now
  // in position for its formal 蜂窝 → Wi-Fi switch. Reported once per
  // preparation hop; the core's own PREPARE_WAIT_MS bounds the wait when the
  // recovery never lands.
  private n2CdReadyAfterRecovery(): void {
    if (!this.n2CdPreparePending || this.n2CdActive.length === 0) {
      return;
    }
    const id: string = this.n2CdActive;
    const request: N2CdPlanRequest = { column: this.n2CdColumn, kind: this.n2CdKind,
      rep: this.n2CdRep, id: id };
    let result: string = 'rejected';
    let reason: string = 'core-throw';
    try {
      const reply: Record<string, Object> = JSON.parse(core.n2_cd_ready(id)) as
        Record<string, Object>;
      result = String(reply['result'] ?? 'rejected');
      reason = String(reply['reason'] ?? 'none');
    } catch (error) { /* the core's own line carries its refusal */ }
    this.n2CdPreparePending = false;
    const line: string = formatN2CdPlanShellLine(this.requestId, request, 'prepare', 'none',
      result === 'ready' ? 'ready' : (result === 'unsat' ? 'unsat' : 'rejected'), reason);
    hilog.info(DOMAIN, TAG, '%{public}s', line);
    this.n2RingAppend(line);
    this.n2CdLatchAfter(result);
  }

  // The shell's latch follows the CORE's plan, never the shell's own hope:
  //   * `unsat` is every terminal `close()` of the core's state machine — the
  //     plan is gone, so the latch drops AND the gate (if this plan armed one)
  //     is HELD: "the plan is gone" must never be a release, or the failed
  //     epoch would be overwritten by an immediate recovery;
  //   * `observed` is the post leg's own terminal outcome — the sandwich
  //     completed, so a gate this plan armed is released for good and the
  //     product recovers normally from here on;
  //   * everything else — including a NON-terminal `rejected` such as
  //     `pre-already-reported` / `switch-already-reported` / `invalid-result` —
  //     keeps the latch, because the core's plan is STILL ACTIVE. Dropping it
  //     there would leak the plan for the rest of the arming: nobody polls it
  //     any more and every later plan is refused `plan-already-active`.
  private n2CdLatchAfter(result: string): void {
    if (result === 'observed') {
      this.n2CdGateReset();
    } else if (result === 'unsat') {
      this.n2CdGateHold();
    }
    if (result === 'unsat' || result === 'observed') {
      this.n2CdClearLatch();
    }
  }

  private n2CdClearLatch(): void {
    this.n2CdActive = '';
    this.n2CdBeforeBearer = 'unknown';
    this.n2CdPreparePending = false;
    this.n2CdBusy = false;
  }

  // ---- N2 first batch C/D columns: the formal-switch production gate -------
  //
  // True while the gate forbids handing the core a production connection for
  // `epoch`. Every recovery / new-connection supply entry consults this:
  // recoverPhysicalNetwork (entry + every chase round), fulfillSocketRequest,
  // feedSpareProtectedSocket, feedOneIceSocket, topUpIceBatch and the
  // recreate-finally retry. `'blocked'` and `'held'` refuse EVERY epoch (so an
  // in-flight attempt of an older epoch cannot continue into a newer one, and a
  // later switch cannot slip past); `'released'` allows ONLY the epoch the gate
  // was armed for.
  private n2CdProductionBlocked(epoch: number = this.networkEpoch): boolean {
    if (this.n2CdGateState.length === 0) {
      return false;
    }
    if (this.n2CdGateState === 'released') {
      return epoch !== this.n2CdGateEpoch;
    }
    return true;
  }

  // Arm the gate for the plan's FORMAL switch at the current network epoch.
  // A 'held' gate is NEVER re-armed: once a formal switch failed, the block
  // outlives any later plan of the same armed session and only the explicit
  // termination paths clear it (readDiagSwitches / teardown). Otherwise a plan
  // that already owns a gate keeps it — a second switch of the same plan must
  // not re-arm a gate that was already released for its own epoch, because that
  // would release the newer epoch.
  private n2CdGateArm(): void {
    if (this.n2CdGateState === 'held') {
      return;
    }
    if (this.n2CdGateState.length > 0 && this.n2CdGatePlan === this.n2CdActive) {
      return;
    }
    this.n2CdGateState = 'blocked';
    this.n2CdGateEpoch = this.networkEpoch;
    this.n2CdGatePlan = this.n2CdActive;
  }

  // The core's own `step=recover` (its tee HIT the new network's pre control)
  // is the ONLY release. It is bound to BOTH the plan the gate belongs to and
  // the epoch the switch armed: an async answer that arrives after another
  // switch, or for another plan, releases nothing.
  private n2CdGateRelease(id: string): boolean {
    if (this.n2CdGateState !== 'blocked' || this.n2CdGatePlan !== id ||
      this.networkEpoch !== this.n2CdGateEpoch) {
      return false;
    }
    this.n2CdGateState = 'released';
    return true;
  }

  // Terminal failure of a gated plan: keep blocking every epoch until the
  // evidence session is explicitly terminated (readDiagSwitches on the next
  // start flow, or teardown). A no-op when this plan never armed a gate (e.g.
  // the D preparation hop's failures), which is what keeps ordinary product
  // recovery unaffected.
  private n2CdGateHold(): void {
    if (this.n2CdGateState.length === 0) {
      return;
    }
    this.n2CdGateState = 'held';
  }

  // Explicit termination / success: no gate at all (unarmed sessions never had
  // one, and a completed plan must not keep blocking the product).
  private n2CdGateReset(): void {
    this.n2CdGateState = '';
    this.n2CdGateEpoch = 0;
    this.n2CdGatePlan = '';
  }

  // One watcher-tick (or switch-instant) step of the plan. The CORE owns every
  // verdict; this carries out the step it answers:
  //   * `pre`     — send the PRE control on the NEW network NOW (the production
  //                 recovery stays blocked);
  //   * `recover` — the core's tee HIT the pre: release the gate for THIS plan
  //                 at THIS epoch and recover the production connections. The
  //                 core re-states this standing permission on every tick until
  //                 the receipt arrives, so the shell latches it: one plan
  //                 drives the recovery once;
  //   * `receipt` — send the POST control with the core's own marker;
  //   * `closed`  — the core already emitted the terminal line (`unsat` /
  //                 `observed`): mirror it, drop the shell latch, and
  //                 release/hold the gate by that terminal result;
  //   * `idle`    — the core has no plan at all (arming change / already
  //                 closed): drop the shell latch so it can never stick, and
  //                 HOLD any armed gate — a plan that vanished can never
  //                 release it.
  private pollN2CdStep(): void {
    if (this.destroyRequested || this.n2CdBusy || this.n2CdActive.length === 0) {
      return;
    }
    let step: string = '';
    let phase: string = '';
    let id: string = '';
    let kind: string = '';
    let marker: string = '';
    let remote: string = 'unknown';
    let result: string = '';
    let reason: string = '';
    try {
      const reply: Record<string, Object> = JSON.parse(core.n2_cd_poll()) as
        Record<string, Object>;
      step = String(reply['step'] ?? '');
      phase = String(reply['phase'] ?? '');
      id = String(reply['id'] ?? '');
      kind = String(reply['kind'] ?? '');
      marker = String(reply['marker'] ?? '');
      remote = String(reply['remote'] ?? 'unknown');
      result = String(reply['result'] ?? '');
      reason = String(reply['reason'] ?? '');
    } catch (error) {
      return;
    }
    if (step === 'idle') {
      this.n2CdGateHold();
      this.n2CdClearLatch();
      return;
    }
    if (step === 'closed') {
      const request: N2CdPlanRequest = { column: this.n2CdColumn, kind: this.n2CdKind,
        rep: this.n2CdRep, id: id };
      const terminal: string = result.length > 0 ? result : 'unsat';
      const line: string = formatN2CdPlanShellLine(this.requestId, request,
        phase.length > 0 ? phase : 'result', 'none', terminal, reason);
      hilog.info(DOMAIN, TAG, '%{public}s', line);
      this.n2RingAppend(line);
      // The core already emitted the terminal line: a success lets the product
      // recover, anything else holds the gate (never a silent release).
      if (terminal === 'observed') {
        this.n2CdGateReset();
      } else {
        this.n2CdGateHold();
      }
      this.n2CdClearLatch();
      return;
    }
    const armed: N2EvidenceOptions | undefined = this.n2Evidence;
    if (armed === undefined) {
      return;
    }
    if (step === 'recover') {
      // The standing permission is accepted ONCE, and only when it names the
      // plan the gate belongs to AND the network is still the epoch the formal
      // switch armed. Anything else (another id, a later epoch) leaves the gate
      // exactly as it is: an async answer must never release another epoch.
      if (!this.n2CdGateRelease(id)) {
        return;
      }
      this.recoverPhysicalNetwork().catch((error: Error): void => {
        this.n2NetworkRecoveryFail(error);
      });
      return;
    }
    const request: N2CdPlanRequest = { column: this.n2CdColumn, kind: kind,
      rep: this.n2CdRep, id: id };
    const target: string = selectN2CdTarget(armed.outerDestinations, kind);
    const protocol: N2ControlProtocol = n2CdProtocol(kind) === 'tcp' ? 'tcp' : 'udp';
    if (step === 'pre' && marker.length > 0) {
      if (target.length === 0) {
        return;
      }
      this.n2CdBusy = true;
      this.n2CdSendPreControl(armed, request, target, protocol, marker)
        .then((observed: boolean): void => {
          this.n2CdBusy = false;
          // Only a HIT reported back to the core moves the plan to its released
          // phase; ask for the next step at once so the recovery is not delayed
          // by a whole watcher tick. A failed pre is retried by the next tick
          // (the core's PRE_WAIT_MS bounds it), never in a tight loop.
          if (observed) {
            this.pollN2CdStep();
          }
        })
        .catch((error: Error): void => {
          this.n2CdBusy = false;
          this.n2WarnAndRing(`N2_CD_SANDWICH_PRE_FAILED|id=${id}|` +
            `reason=${safeError(error)}|diag=true|is_evidence=false`);
        });
      return;
    }
    if (step !== 'receipt' || marker.length === 0 || target.length === 0) {
      return;
    }
    this.n2CdBusy = true;
    this.n2CdSendPostControl(armed, request, target, protocol, marker, remote)
      .then((): void => { this.n2CdBusy = false; })
      .catch((error: Error): void => {
        this.n2CdBusy = false;
        this.n2WarnAndRing(`N2_CD_SANDWICH_POST_FAILED|id=${id}|` +
          `reason=${safeError(error)}|diag=true|is_evidence=false`);
      });
  }

  // The PRE leg: the core asks for it (`step=pre`) only AFTER the formal
  // switch, and it must go through the kind's OWN production opener. Answers
  // true only when the control registry itself saw the HIT — the shell then asks
  // for the next step immediately instead of waiting for a tick.
  private async n2CdSendPreControl(armed: N2EvidenceOptions, request: N2CdPlanRequest,
    target: string, protocol: N2ControlProtocol, marker: string): Promise<boolean> {
    let result: string = 'rejected';
    let reason: string = 'none';
    try {
      await sendAndWaitN2Control(armed, target, protocol, marker);
      result = 'hit';
      reason = 'ok';
    } catch (error) {
      result = 'rejected';
      reason = (error as Error).message !== undefined && (error as Error).message.length > 0
        ? (error as Error).message
        : 'control-send-throw';
    }
    let coreResult: string = 'rejected';
    try {
      const reply: Record<string, Object> = JSON.parse(core.n2_cd_pre(request.id, result, reason)) as
        Record<string, Object>;
      coreResult = String(reply['result'] ?? 'rejected');
    } catch (coreError) { /* the core's own line carries its refusal */ }
    const line: string = formatN2CdPlanShellLine(this.requestId, request, 'pre', marker, result,
      reason);
    hilog.info(DOMAIN, TAG, '%{public}s', line);
    this.n2RingAppend(line);
    this.n2CdLatchAfter(coreResult);
    return result === 'hit';
  }

  private async n2CdSendPostControl(armed: N2EvidenceOptions, request: N2CdPlanRequest,
    target: string, protocol: N2ControlProtocol, marker: string, remote: string): Promise<void> {
    let result: string = 'observed';
    let reason: string = 'ok';
    try {
      await sendAndWaitN2Control(armed, target, protocol, marker);
    } catch (error) {
      result = 'unsat';
      reason = 'post-control-miss';
    }
    let coreAnswered: boolean = false;
    let coreResult: string = '';
    try {
      const reply: Record<string, Object> = JSON.parse(core.n2_cd_post(request.id, result, reason)) as
        Record<string, Object>;
      coreAnswered = typeof reply['result'] === 'string';
      coreResult = coreAnswered ? String(reply['result']) : '';
    } catch (coreError) { /* the core's own line carries its refusal */ }
    const line: string = formatN2CdPlanShellLine(this.requestId, request, 'post', marker, result,
      reason);
    hilog.info(DOMAIN, TAG, '%{public}s', line);
    this.n2RingAppend(line);
    // EVERY branch of the core's `post` either closes the plan (observed /
    // unsat / invalid-result) or answers `no-active-plan` / `not-armed`: once it
    // ANSWERED, no core plan is left behind, so the shell latch drops — and a
    // gate this plan armed is released only by an `observed` terminal, never by
    // "the plan is gone". When the call itself failed the latch is KEPT — the
    // core's plan is still active then, and only its own poll can still close it
    // (`post-wait-exceeded`).
    if (coreAnswered) {
      if (coreResult === 'observed') {
        this.n2CdGateReset();
      } else {
        this.n2CdGateHold();
      }
      this.n2CdClearLatch();
    }
  }

  // N2 first batch C/D columns: the ring FETCH fallback (`n2_ring_dump`). One
  // launch re-emits a bounded, SEQ-ADDRESSED window of the ring on HiLog, which
  // the host reads live after returning to Wi-Fi — the path for a device where
  // the bundle transfer (`hdc file recv -b …`) is unavailable or the HAP is not
  // debug-signed. The core owns the ring file and does ALL the reading and
  // pagination (`{"from_seq":…,"to_seq":…,"count":…,"session":…}` over durable
  // seqs); the shell only validates the private-channel document, forwards it
  // verbatim, re-emits the records it answers and mirrors the watermark the
  // core reports. NOTHING the core returns here is written
  // back into the ring: the chunk records duplicate ring content and the
  // `N2_RING_DUMP` summary is not ring content at all.
  //
  // A DUMP ADVANCES NOTHING: the receipt mirrors the core's `acked_seq` as
  // `ack_watermark=` — the watermark AS IT STANDS. The retrieved watermark moves
  // ONLY through the explicit host ACK channel below (`N2_RING_ACK_SHELL`):
  // re-emitting a page proves the App READ it, not that the host PERSISTED it
  // and verified the window is complete.
  private pollN2RingDump(): void {
    if (this.destroyRequested) {
      return;
    }
    const path: string = `${this.context.filesDir}/${N2_RING_DUMP_FILE_NAME}`;
    let text: string = '';
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      text = fs.readTextSync(path);
    } catch (error) {
      this.n2WarnAndRing(`N2_RING_DUMP_READ_FAILED|requestId=${this.requestId}|` +
        `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
      return;
    }
    try {
      const decision: N2RingDumpDecision = parseN2RingDump(text);
      if (decision.request === undefined) {
        const reject: string = formatN2RingDumpShellReject(this.requestId, decision.error);
        hilog.warn(DOMAIN, TAG, '%{public}s', reject);
        // The shell's own refusal is a fetch event too: it belongs in the ring.
        this.n2RingAppend(reject);
        return;
      }
      const request: N2RingDumpRequest = decision.request;
      // -1 in the receipt's `to_seq` = the request did not freeze a tail: the
      // core then serves up to whatever the tail is at serve time. Printing the
      // parsed placeholder 0 would read as a real (empty) window.
      const wantedToSeq: number = request.hasToSeq ? request.toSeq : -1;
      // The manifest FIRST (a bare core call, the same shape the teardown uses
      // for the counters): the core formats and emits the `N2_RING_MANIFEST`
      // line ITSELF (HiLog + ring), so a fetch is always bracketed by the
      // ring's head/tail state, and the call rescans the kept segments — which
      // is also what makes the dump below see the file's real tail right after
      // a re-arm. Whether a ring exists is the CORE's answer (`not-configured`),
      // never a shell guess.
      try {
        core.n2_ring_manifest();
      } catch (error) { /* the dump below answers `not-configured` itself */ }
      // The document is forwarded verbatim: the CORE's parser is the only
      // authority on the window (`legacy-start-index-unsupported`,
      // `invalid-session`, …), so the shell never re-encodes it.
      let page: N2RingDumpPage = emptyN2RingDumpPage();
      try {
        page = JSON.parse(core.n2_ring_dump(text.trim())) as N2RingDumpPage;
      } catch (error) {
        const failed: string = formatN2RingDumpShellLine(this.requestId, request.fromSeq,
          wantedToSeq, request.count, 0, -1, 'rejected', 'core-throw');
        hilog.warn(DOMAIN, TAG, '%{public}s', failed);
        this.n2RingAppend(failed);
        return;
      }
      const records: string[] = Array.isArray(page.lines) ? page.lines : [];
      for (let i = 0; i < records.length; i++) {
        hilog.info(DOMAIN, TAG, '%{public}s', records[i]);
      }
      if (typeof page.summary === 'string' && page.summary.length > 0) {
        hilog.info(DOMAIN, TAG, '%{public}s', page.summary);
      }
      // The receipt reports the watermark AS IT STANDS (`ack_watermark=`), never
      // a confirmation: this call confirms nothing and never touches the core's
      // ACK surface, so a page that was re-emitted to a host which did NOT
      // persist it cannot release the segment it lives in. A refused page
      // answers no watermark (-1).
      let ackWatermark: number = -1;
      if (page.result === 'dumped' && typeof page.acked_seq === 'number') {
        ackWatermark = page.acked_seq;
      }
      const line: string = formatN2RingDumpShellLine(this.requestId, request.fromSeq, wantedToSeq,
        request.count, records.length, ackWatermark,
        page.result === 'dumped' ? 'dumped' : 'rejected',
        typeof page.reason === 'string' ? page.reason : 'none');
      hilog.info(DOMAIN, TAG, '%{public}s', line);
      // The shell's OWN receipt is a fetch event and belongs in the ring; the
      // core's `N2_RING_DUMP` summary and its chunk records do NOT (they are not
      // ring content — the chunks duplicate records already in it).
      this.n2RingAppend(line);
    } finally {
      try {
        fs.unlinkSync(path);
      } catch (error) {
        this.n2WarnAndRing(`N2_RING_DUMP_CONSUME_FAILED|requestId=${this.requestId}|` +
          `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
      }
    }
  }

  // N2 first batch C/D columns: the EXPLICIT host ACK channel (`n2_ring_ack`).
  // This is the ONLY path that may advance the ring's retrieved watermark, and
  // therefore the only thing that can release the oldest segment to rotation.
  // The host may send it ONLY after it has persisted the fetched ring bytes and
  // verified the window is complete:
  //
  //   aa start -b cn.alfadb.netbird -a EntryAbility \
  //     --ps n2h_arm_options '<the same arm ticket that armed this session>' \
  //     --ps n2_ring_ack '{"session":"<sess= from N2_RING_MANIFEST>","up_to_seq":<last verified durable seq>}'
  //
  // The shell validates the document (both keys required; the same named
  // refusals the core answers), forwards it VERBATIM and mirrors the outcome in
  // its own `N2_RING_ACK_SHELL` receipt. The BINDING is the core's: `n2_ring_ack`
  // re-parses the document and checks the session against the CURRENT arm
  // inside the ring's own lock, so a stale session / a seq beyond the tail / an
  // unarmed ring are refused there — the shell never reads the session once and
  // then acks unconditionally. `advanced=` is the core's answer; a repeat of an
  // already-acknowledged watermark is idempotent (`advanced=false`).
  private pollN2RingAck(): void {
    if (this.destroyRequested) {
      return;
    }
    const path: string = `${this.context.filesDir}/${N2_RING_ACK_FILE_NAME}`;
    let text: string = '';
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      text = fs.readTextSync(path);
    } catch (error) {
      this.n2WarnAndRing(`N2_RING_ACK_READ_FAILED|requestId=${this.requestId}|` +
        `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
      return;
    }
    try {
      const decision: N2RingAckDecision = parseN2RingAck(text);
      if (decision.request === undefined) {
        const reject: string = formatN2RingAckShellReject(this.requestId, decision.error);
        hilog.warn(DOMAIN, TAG, '%{public}s', reject);
        // A refused ACK document is an ACK event too: it belongs in the ring.
        this.n2RingAppend(reject);
        return;
      }
      const request: N2RingAckRequest = decision.request;
      // The document is forwarded verbatim: the CORE's parser and its
      // session/range checks are the only authority on whether the watermark
      // moves.
      let reply: N2RingAckReply = emptyN2RingAckReply();
      try {
        reply = JSON.parse(core.n2_ring_ack(text.trim())) as N2RingAckReply;
      } catch (error) {
        const failed: string = formatN2RingAckShellLine(this.requestId, request.session,
          request.upToSeq, -1, false, 'rejected', 'core-throw');
        hilog.warn(DOMAIN, TAG, '%{public}s', failed);
        this.n2RingAppend(failed);
        return;
      }
      const ackedSeq: number = typeof reply.acked_seq === 'number' ? reply.acked_seq : -1;
      // `advanced` is reported ONLY for an accepted answer, and only as the
      // core's own bit: a refusal can never read as a confirmation.
      const accepted: boolean = reply.ok === true && reply.result === 'acked';
      const line: string = formatN2RingAckShellLine(this.requestId, request.session,
        request.upToSeq, ackedSeq, accepted && reply.advanced === true,
        accepted ? 'acked' : 'rejected',
        typeof reply.reason === 'string' ? reply.reason : 'none');
      if (accepted) {
        hilog.info(DOMAIN, TAG, '%{public}s', line);
      } else {
        hilog.warn(DOMAIN, TAG, '%{public}s', line);
      }
      // The shell's OWN receipt is an ACK event and belongs in the ring.
      this.n2RingAppend(line);
    } finally {
      try {
        fs.unlinkSync(path);
      } catch (error) {
        this.n2WarnAndRing(`N2_RING_ACK_CONSUME_FAILED|requestId=${this.requestId}|` +
          `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
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
      this.n2WarnAndRing(`N2_CONTROL_READ_FAILED|requestId=${this.requestId}|` +
        `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
      return;
    }
    try {
      const decision: N2ControlDecision = parseN2ControlRequest(text);
      if (decision.request === undefined) {
        this.n2WarnAndRing(formatN2ControlShellReject(this.requestId, decision.error));
        return;
      }
      const request = decision.request;
      if (this.n2Evidence === undefined) {
        this.n2WarnAndRing(
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
      const controlLine: string =
        formatN2ControlShellLine(this.requestId, request, result, reason);
      hilog.info(DOMAIN, TAG, '%{public}s', controlLine);
      this.n2RingAppend(controlLine);
    } finally {
      // consume exactly once, whatever happened above
      try {
        fs.unlinkSync(path);
      } catch (error) {
        this.n2WarnAndRing(`N2_CONTROL_CONSUME_FAILED|requestId=${this.requestId}|` +
          `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
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
      this.n2WarnAndRing(`N2_STIMULUS_READ_FAILED|requestId=${this.requestId}|` +
        `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
      return;
    }
    try {
      const decision: N2StimulusDecision = parseN2Stimulus(text);
      if (decision.request === undefined) {
        this.n2WarnAndRing(formatN2StimulusShellReject(this.requestId, decision.error));
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
      const stimulusLine: string = formatN2StimulusShellLine(this.requestId, kind, id, reply);
      hilog.info(DOMAIN, TAG, '%{public}s', stimulusLine);
      this.n2RingAppend(stimulusLine);
    } finally {
      // consume exactly once, whatever happened above
      try {
        fs.unlinkSync(path);
      } catch (error) {
        this.n2WarnAndRing(`N2_STIMULUS_CONSUME_FAILED|requestId=${this.requestId}|` +
          `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
      }
    }
  }

  // Explicit user logout (stop-time Logout defect, 2026-10-01): the UI's
  // one-shot `netbird-logout` ticket, consumed on the watcher tick. This
  // process is the ONLY one that can send the Logout (the live connector
  // and its logged-in management client live here, not in the UI process).
  // Order is fixed: (1) ONE core.connector_logout() — best-effort, a failure
  // is logged and NEVER blocks; (2) the `netbird-logout-result` answer the
  // UI is bounded-waiting for; (3) the teardown — stop the watcher, stop
  // the connector (which itself never logs out), destroy the VPN
  // generation. A logged-out peer must not live on, and the dying Sync
  // stream must not reconnect and re-register the peer the Logout just
  // deleted server-side. Like the N2 private channels: consumed exactly
  // once (`finally` unlink), and a stale ticket (older than
  // LOGOUT_TICKET_MAX_AGE_MS) is refused so a leftover from a session that
  // died before its tick can never tear a LATER session down.
  private pollLogoutTicket(): void {
    if (this.destroyRequested || this.destroyPromise !== undefined) {
      return;
    }
    const path: string = `${this.context.filesDir}/${LOGOUT_FILE_NAME}`;
    let text: string = '';
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      text = fs.readTextSync(path);
    } catch (error) {
      hilog.warn(DOMAIN, TAG, 'VPN_LOGOUT_READ_FAILED|requestId=%{public}s|code=%{public}s',
        this.requestId, String((error as BusinessFailure).code ?? 'none'));
      return;
    }
    try {
      const issuedAtMs: number = parseLogoutTicketIssuedAtMs(text);
      if (issuedAtMs <= 0) {
        hilog.warn(DOMAIN, TAG,
          'VPN_LOGOUT_TICKET_REJECTED|requestId=%{public}s|reason=invalid-ticket', this.requestId);
        return;
      }
      const ageMs: number = Date.now() - issuedAtMs;
      if (ageMs > LOGOUT_TICKET_MAX_AGE_MS) {
        hilog.warn(DOMAIN, TAG,
          'VPN_LOGOUT_TICKET_REJECTED|requestId=%{public}s|reason=stale-ticket|ageMs=%{public}d',
          this.requestId, ageMs);
        return;
      }
      // (1) the ONE explicit Logout (bounded native-side by
      // EXPLICIT_LOGOUT_WAIT_MS); failure is logged, never blocking
      let ok: boolean = false;
      let reason: string = 'none';
      try {
        const result: ConnectorLogoutResult = connectorLogout();
        ok = result.ok;
        reason = result.reason !== undefined && result.reason !== null && result.reason.length > 0
          ? result.reason
          : (result.error !== undefined && result.error !== null ? result.error : 'none');
      } catch (error) {
        ok = false;
        reason = 'native-throw';
      }
      hilog.info(DOMAIN, TAG,
        'VPN_LOGOUT_TICKET|requestId=%{public}s|ok=%{public}s|reason=%{public}s|ticketMs=%{public}d',
        this.requestId, String(ok), reason, issuedAtMs);
      // (2) answer the UI BEFORE the teardown ends this process's work
      try {
        const file: fs.File = fs.openSync(`${this.context.filesDir}/${LOGOUT_RESULT_FILE_NAME}`,
          fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
        fs.writeSync(file.fd,
          `{"ok":${ok},"reason":"${clean(reason)}","ticket_ms":${issuedAtMs}}`);
        fs.closeSync(file);
      } catch (error) {
        hilog.error(DOMAIN, TAG,
          'VPN_LOGOUT_RESULT_WRITE_FAILED|requestId=%{public}s|code=%{public}s',
          this.requestId, String((error as BusinessFailure).code ?? 'none'));
      }
      // (3) the teardown (same shape as failRecreate): the connector's own
      // stop never logs out — the Logout above was the whole point
      this.stopStatusAndNetcfgWatcher();
      try {
        const stop = connectorStop();
        hilog.info(DOMAIN, TAG,
          'VPN_CONNECTOR_STOPPED|requestId=%{public}s|alreadyStopped=%{public}s|state=%{public}s|' +
          'cause=explicit-logout',
          this.requestId, String(stop.already_stopped), stop.state);
      } catch (err) {
        hilog.error(DOMAIN, TAG,
          'VPN_CONNECTOR_STOP_REJECTED|requestId=%{public}s|summary=%{public}s',
          this.requestId, safeError(err as Error));
      }
      this.destroyOnce('explicit-logout').catch((err: Error): void => {
        hilog.error(DOMAIN, TAG,
          'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=explicit-logout|summary=%{public}s',
          this.requestId, safeError(err));
      });
    } finally {
      // consume exactly once, whatever happened above
      try {
        fs.unlinkSync(path);
      } catch (error) {
        hilog.warn(DOMAIN, TAG,
          'VPN_LOGOUT_CONSUME_FAILED|requestId=%{public}s|code=%{public}s',
          this.requestId, String((error as BusinessFailure).code ?? 'none'));
      }
    }
  }

  // N2 P4「A 列生产事件触发原语」(v4.1 §4 A 列操作性定义): the shell's
  // armed-only private-channel entry. ONE file = ONE trigger: the request file
  // is deleted in `finally`, so the 5 s watcher tick can never replay it, and
  // the core's own `N2_A_EVENT|…` line stays the authoritative record (it
  // carries the real fd/dup of the EXISTING production socket).
  //
  // Trigger (documented in docs/n2-evidence-switch-20260928.md): the operator
  // writes ONE request through the existing non-secret want-parameter channel —
  //   aa start -b cn.alfadb.netbird -a EntryAbility \
  //     --ps n2h_arm_options '<the same arm ticket>' \
  //     --ps n2_a_event '{"kind":"M","id":"a-M-1"}'
  // `kind` is M (one read-only GetServerKey on the existing management
  // connection), R (wait for the next server-driven HealthCheck echo), D (the
  // Q1 requery) or S (always refused by name). UNARMED: the request is still
  // consumed and answered `rejected|reason=not-armed` WITHOUT any core call.
  private pollN2AEvent(): void {
    if (this.destroyRequested) {
      return;
    }
    const path: string = `${this.context.filesDir}/${N2_A_EVENT_FILE_NAME}`;
    let text: string = '';
    try {
      if (!fs.accessSync(path)) {
        return;
      }
      text = fs.readTextSync(path);
    } catch (error) {
      this.n2WarnAndRing(`N2_A_EVENT_READ_FAILED|requestId=${this.requestId}|` +
        `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
      return;
    }
    try {
      const decision: N2AEventDecision = parseN2AEvent(text);
      if (decision.request === undefined) {
        this.n2WarnAndRing(formatN2AEventShellReject(this.requestId, decision.error));
        return;
      }
      const request = decision.request;
      if (this.n2Evidence === undefined) {
        this.n2WarnAndRing(formatN2AEventShellLine(this.requestId, request.kind, request.id,
          { ok: false, result: 'rejected', reason: 'not-armed' }));
        return;
      }
      let reply: N2AEventReply = { ok: false, result: 'rejected', reason: 'core-throw' };
      try {
        reply = parseN2AEventReply(core.n2_a_event(request.kind, request.id));
      } catch (error) {
        // the core refuses a non-armed call; a thrown call is reported as its
        // own reason, never as a dispatched trigger
        reply = { ok: false, result: 'rejected', reason: 'core-throw' };
      }
      const aEventLine: string =
        formatN2AEventShellLine(this.requestId, request.kind, request.id, reply);
      hilog.info(DOMAIN, TAG, '%{public}s', aEventLine);
      this.n2RingAppend(aEventLine);
    } finally {
      // consume exactly once, whatever happened above
      try {
        fs.unlinkSync(path);
      } catch (error) {
        this.n2WarnAndRing(`N2_A_EVENT_CONSUME_FAILED|requestId=${this.requestId}|` +
          `code=${String((error as BusinessFailure).code ?? 'none')}|diag=true|is_evidence=false`);
      }
    }
  }

  // N2 P5「I/Ws 的 App 侧夹心编排」(v4.1 §4 A 列 I/Ws 行): register ONE leg with
  // the core's sandwich state machine and send its PRE control, waiting
  // (bounded by the core's own control window) for the HIT. Returns the
  // marker actually registered, or '' when the leg could not be set up — the
  // core's own `N2_SANDWICH|…|phase=pre` line carries the reason.
  //
  // Ordering the frozen text requires: the caller invokes this AFTER the TUN
  // tee attach (the core refuses with `no-attach-anchor` otherwise) and BEFORE
  // the first production send of that kind. Nothing here changes the ICE/WG
  // send content or order.
  private async n2SandwichPre(kind: string): Promise<string> {
    const armed: N2EvidenceOptions | undefined = this.n2Evidence;
    if (armed === undefined || this.destroyRequested) {
      return '';
    }
    const target: string = selectN2SandwichTarget(armed.outerDestinations);
    if (target.length === 0) {
      this.n2WarnAndRing(formatN2SandwichShellLine(kind, 'pre', 'none', 'none', 'no-target',
        'no-ice-destination'));
      return '';
    }
    // N2 T7a (item 1): the PRE and POST controls need DIFFERENT markers — the
    // core's control registry refuses a reused id, so a post control sent with
    // the pre leg's marker would be structurally unreachable. Both are derived
    // here from the instance request id and registered together, so the core
    // hands back the exact post marker to send (`n2_sandwich_poll`).
    const marker: string = formatN2SandwichLegMarker(kind, this.requestId, 'pre');
    const postMarker: string = formatN2SandwichLegMarker(kind, this.requestId, 'post');
    if (marker.length === 0 || postMarker.length === 0 || marker === postMarker) {
      this.n2WarnAndRing(
        formatN2SandwichShellLine(kind, 'pre', 'none', target, 'rejected', 'invalid-marker'));
      return '';
    }
    let registered: boolean = false;
    try {
      const reply: Record<string, Object> =
        JSON.parse(core.n2_sandwich_pre(kind, marker, postMarker)) as Record<string, Object>;
      registered = reply['ok'] === true;
    } catch (error) {
      registered = false;
    }
    if (!registered) {
      // the core already logged the exact unsat/rejected reason
      this.n2WarnAndRing(
        formatN2SandwichShellLine(kind, 'pre', marker, target, 'unsat', 'core-refused'));
      return '';
    }
    try {
      await sendAndWaitN2Control(armed, target, 'udp', marker);
    } catch (error) {
      // the pre control MISSed (or its send threw): close the leg honestly so
      // the first production send can never be reported as sandwiched. No post
      // control was sent, so the leg is closed by its POST marker (the id the
      // post control would have used) — the core's line says
      // `result=unsat|reason=pre-control-miss`.
      try {
        core.n2_sandwich_post(kind, postMarker, 'unsat', 'pre-control-miss');
      } catch (postError) { /* the core logs nothing for a refused post */ }
      this.n2WarnAndRing(formatN2SandwichShellLine(kind, 'pre', marker, target, 'unsat',
        (error as Error).message ?? 'pre-control-miss'));
      return '';
    }
    const preSandwichLine: string =
      formatN2SandwichShellLine(kind, 'pre', marker, target, 'sent', 'pre-control-hit');
    hilog.info(DOMAIN, TAG, '%{public}s', preSandwichLine);
    this.n2RingAppend(preSandwichLine);
    return marker;
  }

  // N2 P5: the I leg, called between the WG data-plane feed (the attach) and
  // the first ICE pool feed / gather — the frozen "首轮 ICE 池馈入之前".
  private async n2IceSandwichPre(): Promise<string> {
    this.n2IceSandwichMarker = await this.n2SandwichPre('I');
    return this.n2IceSandwichMarker;
  }

  // N2 P5: the Ws leg, called right after the ICE pool feed and therefore
  // before any ICE nomination can select the egress socket ("Ws 选中前").
  private async n2WsSandwichPre(): Promise<string> {
    this.n2WsSandwichMarker = await this.n2SandwichPre('Ws');
    return this.n2WsSandwichMarker;
  }

  // N2 P5: one watcher-tick step of the sandwich. The core decides when a leg
  // is ready (its first real send was observed after a pre HIT) and hands back
  // the marker; the shell sends the POST control and reports the outcome. The
  // call is a no-op (not even a core call) unless the session is armed.
  private pollN2Sandwich(): void {
    if (this.n2Evidence === undefined || this.destroyRequested || this.n2SandwichBusy) {
      return;
    }
    let raw: string = '';
    try {
      raw = core.n2_sandwich_poll();
    } catch (error) {
      return;
    }
    const posts: N2SandwichPost[] = parseN2SandwichPoll(raw);
    if (posts.length === 0) {
      return;
    }
    this.n2SandwichBusy = true;
    this.runN2SandwichPosts(posts)
      .catch((error: Error): void => {
        this.n2WarnAndRing(`N2_SANDWICH_POST_FAILED|reason=${safeError(error)}|` +
          `diag=true|is_evidence=false`);
      })
      .then((): void => {
        this.n2SandwichBusy = false;
      });
  }

  private async runN2SandwichPosts(posts: N2SandwichPost[]): Promise<void> {
    for (let i = 0; i < posts.length; i++) {
      const post: N2SandwichPost = posts[i];
      const armed: N2EvidenceOptions | undefined = this.n2Evidence;
      if (armed === undefined || this.destroyRequested) {
        return;
      }
      const target: string = selectN2SandwichTarget(armed.outerDestinations);
      let result: string = 'observed';
      let reason: string = 'post-control-hit';
      try {
        await sendAndWaitN2Control(armed, target, 'udp', post.marker);
      } catch (error) {
        result = 'unsat';
        reason = 'post-control-miss';
      }
      try {
        core.n2_sandwich_post(post.kind, post.marker, result, reason);
      } catch (error) { /* a refused post is the core's own named line */ }
      const postSandwichLine: string =
        formatN2SandwichShellLine(post.kind, 'post', post.marker, target, result, reason);
      hilog.info(DOMAIN, TAG, '%{public}s', postSandwichLine);
      this.n2RingAppend(postSandwichLine);
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
      // 2026-10-08: the persistent-log writer's own health, ONCE per 60 s on
      // this existing tick (see samplePhoneLogStatus). Deliberately the FIRST
      // statement: it must survive every early return below, including an
      // unreachable connector and a pending teardown.
      this.samplePhoneLogStatus();
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
      // N2 first batch C/D columns: the ring channels first (the FETCH fallback
      // works even with no armed session — the operator fetches the ring AFTER
      // returning to Wi-Fi, when the plan may already be over — while the
      // explicit ACK is accepted only by an ARMED ring: the core refuses it
      // `not-armed` otherwise, so a request that outlives the session releases
      // nothing). Then the plan channel and its step poll; the plan poll
      // consumes at most ONE plan per tick and the step poll is a bare core call
      // while no plan is in flight.
      this.pollN2RingDump();
      this.pollN2RingAck();
      this.pollN2CdPlan();
      this.pollN2CdStep();
      // N2 P4: the A-column trigger poll — one-shot + self-consuming like the
      // other private channels, no-op unless armed (and then still a bare stat
      // when nothing is pending). It rides BEFORE the control/stimulus polls
      // because it is independent of their fixed combination order.
      this.pollN2AEvent();
      // N2 P5: the automatic sandwich's POST step. The core offers a leg only
      // once its first real production send was observed after the pre HIT;
      // this is a bare core call (no file probe, no control send) unless the
      // session is armed and a leg is ready.
      this.pollN2Sandwich();
      // Explicit user logout: the one-shot `netbird-logout` ticket rides the
      // same top-of-tick slot as the N2 private channels (BEFORE the status
      // read and the terminal/recreate early returns) so a pending logout is
      // picked up within one 5 s tick in every tick form. No-op (not even a
      // file probe) unless the UI wrote the ticket.
      this.pollLogoutTicket();
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
        // roam-1005-0014 A.2/A.4: the async vpn-tun statistics probe is taken
        // ONCE per tick; the continuation publishes the document (the UI's
        // 已连接 gate rides iface_present) and then runs the interface
        // watchdog — the core's self-reported state can no longer paper over
        // a system-removed interface. The guards inside re-check the
        // teardown/rebuild latches, so a tick that already triggered a
        // recreate (its settle EXPECTS the interface absent) is excluded by
        // state, not by ordering.
        this.probeVpnIfacePresence().then((ifacePresence: string): void => {
          this.publishStatusForUi(status, ifacePresence);
          // same tick, same callback: the offline route catalog (guarded
          // independently, and never a second timer).
          this.publishRouteCatalogForUi();
          if (this.destroyRequested || this.connectorDeathHandled || this.watcherTimer < 0) {
            return;
          }
          this.checkVpnInterfaceAlive(ifacePresence);
        }).catch((probeError: Error): void => {
          hilog.warn(DOMAIN, TAG,
            'VPN_IFACE_PROBE_REJECTED|requestId=%{public}s|summary=%{public}s',
            this.requestId, safeError(probeError));
          this.publishStatusForUi(status, 'unknown');
          this.publishRouteCatalogForUi();
        });
        // Demand-sized ICE pool target from the LIVE peer count and the CORE's
        // own per-peer need (peers × need_per_peer, clamped 8..48 — smoke 0050:
        // 6 peers × need 4 = 24 against a fixed 8, 2 peers starved on every
        // round; smoke 0051: need 6 with 3 allowed interfaces → 36). A status
        // document without the field, or need_per_peer=-1 (interfaces not
        // enumerable), falls back to the shell's 4-per-peer constant. Re-derived
        // on every sample, also when no ice_sockets section is present, so the
        // timer below never acts on a target from an older peer count.
        this.iceTargetDepth =
          iceSocketTargetDepth(status.peer_count, iceSocketNeedPerPeer(status.ice_sockets?.need_per_peer));
        // Refresh the ICE queue depth from the LIVE status: a depth learned
        // only from our own feed calls goes stale and stops resupply exactly
        // when the queue has drained (the host CLI starved that way, and the
        // shell gate had the same flaw).
        if (status.ice_sockets !== undefined && status.ice_sockets !== null) {
          this.iceQueueDepth = status.ice_sockets.queued;
          if (this.iceQueueDepth < this.iceTargetDepth && !this.networkDirty && this.connection !== undefined) {
            this.topUpIceBatch().catch((error: Error): void => {
              hilog.warn(DOMAIN, TAG, 'VPN_ICE_TOPUP|rejected|summary=%{public}s', safeError(error));
            });
          }
        }
        if (status.terminal === true) {
          this.handleConnectorDeath(`terminal-state:${status.state}`, errorClass);
          return;
        }
        // 场景B roaming (§4 step 2): the debounced LAN-exclusion check rides
        // the same tick as the N8 trigger below — a triggered roam rebuild
        // sets recreateInFlight synchronously, so the N8 block behind it
        // skips exactly like it does behind any in-flight recreate.
        this.checkLanRoamRecreate();
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
        // roam-1005-0014 A.4: the interface watchdog no longer lives HERE —
        // it runs in this tick's probe continuation right after the publish
        // above, so it sees the same vpn-tun presence the document carries
        // and its guards exclude an already-triggered rebuild by state.
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
  private handleConnectorDeath(reason: string, errorClass: string = 'none'): void {
    if (this.connectorDeathHandled || this.destroyRequested) {
      return;
    }
    this.connectorDeathHandled = true;
    hilog.error(DOMAIN, TAG,
      'VPN_CONNECTOR_DEAD|requestId=%{public}s|reason=%{public}s|action=teardown-vpn',
      this.requestId, reason);
    // 2026-10-08: "the core ended by itself" as the persistent record (fatal
    // auth error / retry budget exhausted on `terminal-state:*`, or this
    // shell's own repeated status-poll failures). Fixed token only.
    const milestoneFields: DiagFields = { 'reason': reason, 'result': 'teardown-vpn' };
    this.diagMilestone('vpn_core_dead', milestoneFields);
    // 2026-10-06 登录失败可见性: a TERMINAL connector (login fatal auth /
    // retry budget exhausted) leaves no live status document once the
    // teardown below removes it — persist the failure class first so the
    // home card can show the engine's reason instead of snapping back to a
    // bare 未连接 / a stale 连接中…. Non-terminal deaths (status unreachable)
    // keep the document-vanish notice path.
    if (reason.indexOf('terminal-state:') === 0) {
      this.writeConnectFailureForUi(errorClass);
    }
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
  //
  // 2026-10-09: the ACK crosses with the EXPLICIT kind 'initial' (the bridge's
  // 3-argument native entry), and an available map whose desired set is EMPTY
  // (the user turned every network off) is ACKed as `[]` instead of being
  // skipped — the empty list is a real applied set and every route change must
  // be able to compare against it. Only a MISSING core snapshot (no map was
  // merged at all) still skips: there is nothing to ACK and a fabricated `[]`
  // would claim an applied set that was never submitted.
  private ackAppliedRoutesInitial(): void {
    const routes: string[] | null = this.pendingAppliedRoutes;
    this.pendingAppliedRoutes = null;
    if (routes === null) {
      hilog.warn(DOMAIN, TAG,
        'VPN_ROUTE_SET_ACK_SKIPPED|requestId=%{public}s|reason=no-core-snapshot-routes',
        this.requestId);
      return;
    }
    try {
      const ack: RouteSetAppliedResult = connectorRouteSetApplied(routes, false, 'initial');
      if (!ack.ok) {
        hilog.warn(DOMAIN, TAG,
          'VPN_ROUTE_SET_ACK_FAILED|requestId=%{public}s|error=%{public}s',
          this.requestId, ack.error ?? 'no-message');
        return;
      }
      hilog.info(DOMAIN, TAG,
        'VPN_ROUTE_SET_ACKED|requestId=%{public}s|routes=%{public}d|isRecreate=false|ackKind=initial',
        this.requestId, routes.length);
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'VPN_ROUTE_SET_ACK_FAILED|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error as Error));
    }
  }

  // `ackKind` (2026-10-09 live route apply): the NATIVE ACK kind this leg
  // reports. 'auto' (the default) is every pre-existing caller — the
  // desired-set-divergence watch, the LAN-roam rebuild and the iface-loss
  // rebuild — and keeps spending the core's automatic 3 / 30 s budget exactly
  // as before. 'user' is the explicit live route apply of this increment: it
  // records the applied set and clears the pending request WITHOUT touching the
  // automatic budget/cooldown. It is the ONE difference between the two paths:
  // the guards, the snapshot, the destroy → settle → create → verify sequence,
  // the TUN feed, the ACK and the fail-closed teardown are identical, and
  // `recreateInFlight` serializes the user path with the automatic and LAN legs
  // exactly as it always did.
  //
  // Return value (2026-10-09): the REAL outcome token — 'ok' (iface verified +
  // TUN fed + ACK accepted), 'fail:<stage>' (the existing fail-closed teardown
  // ran) or 'skip:<reason>' (nothing was rebuilt). It is the same value
  // lastRecreateOutcome carries; the pre-existing callers ignore it.
  //
  // `ackOut` (2026-10-09, OPTIONAL): when handed over, the EXACT route set this
  // leg's own ACK accepted is recorded into it (still `null` until then). It is
  // OPERATION-LOCAL state, not a shared "last snapshot": the one caller that
  // must report this rebuild as applied reads the set the recreate's OWN
  // snapshot ACKed, never the setter answer it saw one native call earlier. The
  // three pre-existing automatic call sites pass no `ackOut` and are unaffected.
  private async performControlledRecreate(reason: string, ackKind: string = 'auto',
    ackOut?: RecreateAckOut): Promise<string> {
    if (this.recreateInFlight) {
      this.lastRecreateOutcome = 'skip:already-in-flight';
      hilog.info(DOMAIN, TAG,
        'VPN_RECREATE_SKIP|requestId=%{public}s|reason=already-in-flight', this.requestId);
      return this.lastRecreateOutcome;
    }
    if (this.destroyRequested || this.connectorDeathHandled) {
      this.lastRecreateOutcome = 'skip:teardown-latched';
      return this.lastRecreateOutcome; // teardown already latched — nothing to rebuild
    }
    if (this.connection === undefined || !this.createSettled || this.tunFd < 0) {
      this.lastRecreateOutcome = 'skip:initial-generation-not-settled';
      hilog.warn(DOMAIN, TAG,
        'VPN_RECREATE_SKIP|requestId=%{public}s|reason=initial-generation-not-settled',
        this.requestId);
      return this.lastRecreateOutcome;
    }
    this.recreateInFlight = true;
    hilog.info(DOMAIN, TAG,
      'VPN_RECREATE_BEGIN|requestId=%{public}s|reason=%{public}s', this.requestId, reason);
    const milestoneFields: DiagFields = { 'reason': reason };
    this.diagMilestone('vpn_recreate_begin', milestoneFields);
    let stage: string = 'snapshot';
    try {
      if (this.n2Evidence !== undefined) {
        const recreateLine: string = `N2_EVIDENCE_RECREATE_DETECTED|requestId=${this.requestId}|` +
          `reason=${clean(reason)}|diag=true|is_evidence=false`;
        hilog.warn(DOMAIN, TAG, '%{public}s', recreateLine);
        this.n2RingAppend(recreateLine);
      }
      // 1. the DESIRED config, fresh from the core (live gate state)
      const net: CoreNetworkConfig = connectorNetworkConfig();
      if (!net.available || net.routes === undefined || net.routes === null) {
        throw new Error('recreate-no-usable-snapshot');
      }
      // 2026-10-09: an AVAILABLE snapshot with an EMPTY desired set is a legal
      // rebuild (every network turned off). The overlay graft below still
      // stands — the tunnel's own subnet entry is injected after the merge — and
      // the ACK crosses the empty list, which the kind-aware native entry
      // accepts for 'user' (see the connector's connectorRouteSetApplied).
      // N2-H: graft the endpoint exclusions frozen at THIS session's start
      // onto the rebuilt config — a recreate that dropped them would suck
      // the outer endpoints into the fresh (often default-routed) tunnel.
      // The frozen set is reused verbatim (no DNS inside the bounded
      // recreate leg); the next full start re-freezes with fresh
      // resolutions. The base host-LAN exclusion, in contrast, is RE-DERIVED
      // here (scenario-A fix): a recreate happening after a network switch
      // must not drag the OLD network's LAN prefix into the new config.
      // roam-1005-0124: the RESOLVER (/32) subset is re-derived here too,
      // from the CURRENT physical network (same timing as the LAN exclusion)
      // — the startup DNS freeze must not cross a network switch (the home
      // Wi-Fi resolver 192.168.50.1/32 stayed excluded over cellular and
      // broke on-device access to the gateway). The remaining endpoint /32s
      // (management/signal/relay/stun) are grafted frozen, as before.
      const recreateTemplate: VpnTunnelConfig = this.n2Evidence !== undefined
        ? withoutN2ExcludedRoutes(loadVpnTunnelConfig()) : loadVpnTunnelConfig();
      const lanRecreate: LanExclusionResult =
        this.lanExclusionForConfig(recreateTemplate.tunnelAddress, 'recreate');
      const lanRoutesRecreate: ExcludedRouteEntry[] =
        lanRecreate.entry !== null ? [lanRecreate.entry] : [];
      const frozenRoutesRecreate: ExcludedRouteEntry[] = this.frozenRoutesWithoutResolvers();
      const dnsRoutesRecreate: ExcludedRouteEntry[] =
        this.recreateResolverExclusionRoutes(recreateTemplate.tunnelAddress);
      const base: VpnTunnelConfig = withEndpointExclusions(recreateTemplate,
        lanRoutesRecreate.concat(frozenRoutesRecreate).concat(dnsRoutesRecreate));
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
        const recreateRoutesLine: string = `N2_EVIDENCE_ROUTES|requestId=${this.requestId}|` +
          `leg=recreate|capture32=${this.n2Evidence.capture32.join(',')}|defaultRoutes=0|` +
          `excludedRoutes=0|diag=true|is_evidence=false`;
        hilog.info(DOMAIN, TAG, '%{public}s', recreateRoutesLine);
        this.n2RingAppend(recreateRoutesLine);
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

      // 2.–4. (roam-1005-0014 fix) up to RECREATE_IFACE_MAX_ATTEMPTS full
      // destroy → settle → create → fd-contract → registry-verify passes.
      // The SETTLE window between destroy() and the new create() is the
      // device-proven race fix (the old ~168 ms gap let the system-side
      // teardown land after the new create and kill the fresh vpn-tun); the
      // registry verification replaces the fd probe as the truth about the
      // interface (an open in-process fd survived a removed netdev). A
      // verified generation breaks the loop; an absent interface consumes
      // one attempt (the retry waits a LONGER settle floor) and the last
      // attempt's absence throws — the shared catch below is the existing
      // fail-closed teardown.
      const RECREATE_IFACE_MAX_ATTEMPTS: number = 2;
      let acceptedFd: number = -1;
      for (let attempt: number = 1; attempt <= RECREATE_IFACE_MAX_ATTEMPTS; attempt++) {
        // roam-1005-0124: release the core's TUN dup FIRST (idempotent —
        // attempt 2 finds nothing left to release). The platform cannot
        // remove the OLD vpn-tun netdev while our dup is the last open fd
        // over it: the settle leg below then never sees the old iface
        // disappear (cap 3839 ms), and the dup close at replace time makes
        // the OLD teardown land and take the same-named NEW interface down
        // with it (~1 s after VERIFY ok). Releasing here lets the destroy()
        // below actually remove the netdev. The TUN half of the data plane
        // pauses; peers, WG sessions and the protected WG socket stay, and
        // the post-loop feed re-attaches the new fd.
        stage = 'tun-release';
        const release: TunReleaseResult = connectorTunRelease();
        const releaseResult: string = !release.ok
          ? `error:${release.error ?? 'unknown'}`
          : (release.released === true ? 'ok' : 'ok-nothing-to-release');
        hilog.info(DOMAIN, TAG,
          'VPN_RECREATE_TUN_RELEASE|requestId=%{public}s|attempt=%{public}d|result=%{public}s|' +
          'released=%{public}s|dup=%{public}d|deviceUp=%{public}s',
          this.requestId, attempt, releaseResult, String(release.released === true),
          release.fd !== undefined && release.fd !== null ? release.fd : -1,
          String(release.device_up === true));
        if (!release.ok && release.error !== 'device-not-up') {
          throw new Error(`recreate-tun-release-failed:${release.error ?? 'unknown'}`);
        }
        // destroy the OLD connection — the raw TUN fd dies here (fd
        // contract: its close belongs exclusively to VpnConnection.destroy());
        // the data plane keeps running on its own dups until the replacement
        // lands (no route-less window worse than the old config).
        stage = 'destroy';
        const oldConnection: vpnExtension.VpnConnection | undefined = this.connection;
        if (oldConnection !== undefined) {
          const destroyed: boolean = await this.destroyConnectionBounded(oldConnection);
          if (!destroyed) {
            throw new Error('recreate-destroy-failed');
          }
        }
        if (this.destroyRequested || this.connectorDeathHandled) {
          throw new Error('recreate-interrupted-by-teardown');
        }
        this.connection = undefined;
        this.tunFd = -1;
        this.fdConfirmedOpenAfterCreate = false;

        // settle: give the system-side teardown (netdev removal, VPN
        // unregister) the window to land BEFORE the new create() re-occupies
        // the global VPN slot. Bounded by floor + cap; see
        // waitRecreateSettle for the exit conditions and its log line.
        stage = 'settle';
        await this.waitRecreateSettle(attempt);

        if (this.destroyRequested || this.connectorDeathHandled) {
          throw new Error('recreate-interrupted-by-teardown');
        }

        // a FRESH VpnConnection + create() with the desired route set
        stage = 'create';
        const conn: vpnExtension.VpnConnection = vpnExtension.createVpnConnection(this.context);
        this.connection = conn;
        const config: vpnExtension.VpnConfig =
          buildPlatformVpnConfig(merged, this.diagBlockedApplicationList('recreate'),
            this.n2Evidence !== undefined ? [SELF_BUNDLE_NAME] : undefined);
        // 场景B roaming: this leg's re-derived exclusion becomes the
        // live-config truth the moment this create() lands (a failed leg tears
        // the whole session down fail-closed, so the recording can never
        // describe a generation that is not live).
        this.appliedLanExclusion = lanExclusionSignature(lanRecreate.entry);
        const fd: number = await this.createConnectionBounded(conn, config);

        // 2026-10-09 STOP-during-recreate gap: the bounded create() is a real
        // await inside which onDestroy (or the connector-death path) can latch.
        // Re-check BEFORE the fd contract / verification / feed / ACK — a
        // torn-down generation must never be fed, ACKed or reported applied.
        // The shared catch below runs the EXISTING fail-closed teardown
        // (failRecreate → destroyOnce, both idempotent), which destroys this
        // fresh, unfed connection: no orphan TUN, no late ACK.
        if (this.destroyRequested || this.connectorDeathHandled) {
          throw new Error('recreate-interrupted-by-teardown');
        }

        // the same fd contract as the initial create
        stage = 'fd-contract';
        const snapshot: FdSnapshot = this.captureFdSnapshot(fd);
        if (!(snapshot.integer && snapshot.nonnegative && snapshot.probed && snapshot.open)) {
          throw new Error('recreate-invalid-fd');
        }
        this.logFdSnapshot('recreate-post-create', snapshot, 'RECREATE_FD_ACCEPTED');

        // registry verification: the new vpn-tun must REALLY exist. An open
        // fd proves nothing (the device defect kept an open fd over a
        // removed netdev); the fd is NOT fed to the core and the generation
        // NOT accepted until the registry lists the interface.
        stage = 'iface-verify';
        const verifyOutcome: string =
          await this.verifyRecreatedIface(attempt, RECREATE_IFACE_MAX_ATTEMPTS);
        // 2026-10-09 STOP-during-recreate gap (same rule as after create()):
        // the verification probes are awaits too. A teardown latched inside them
        // must not be followed by the accept/feed/ACK legs.
        if (this.destroyRequested || this.connectorDeathHandled) {
          throw new Error('recreate-interrupted-by-teardown');
        }
        if (verifyOutcome !== 'ok') {
          continue;
        }
        acceptedFd = fd;
        this.tunFd = fd;
        this.fdConfirmedOpenAfterCreate = true;
        // N2-H I3a (v3.1 §2.1 C2 (4)): the recreate leg's ACCEPTANCE ANCHOR —
        // this generation's create() was accepted, with the SAME fd-contract
        // acceptance semantics as the initial leg's
        // `VPN_CREATE_RESOLVED|accepted=true`. It is logged after the
        // generation line and every gen==G route row (rule (4)'s ordering)
        // and before the wg-feed/ACK legs (a failure there is the same
        // fail-closed teardown as the initial leg's protect gate). The record
        // of the accepted generation is made at this anchor, so the anchor
        // and the value the arm reports cannot drift (R-2③).
        this.n2hAcceptedExclusionGeneration = gen;
        hilog.info(DOMAIN, TAG,
          'VPN_RECREATE_RESOLVED|requestId=%{public}s|gen=%{public}d|accepted=true',
          this.requestId, gen);
        break;
      }
      if (acceptedFd < 0) {
        throw new Error('recreate-iface-absent');
      }
      const fd: number = acceptedFd;

      // 5. replace only the TUN dup. The live core device already owns the
      // protected WG socket dup and retains its sessions across TUN replacement.
      stage = 'tun-feed';
      const tunFeed: WgFeedResult = connectorTunFdFeed(fd);
      if (!tunFeed.ok || tunFeed.device_up !== true) {
        throw new Error(`recreate-tun-feed-failed:${tunFeed.error ?? 'device-not-up'}`);
      }

      // 6. ACK the newly applied route set — completes the recreate (counts
      // against the core-side budget, arms the cooldown, clears `required`)
      // 2026-10-09: with the EXPLICIT kind of this leg (ackKind). The routes
      // array crosses verbatim, including an EMPTY one (every network off).
      stage = 'ack';
      const ack: RouteSetAppliedResult =
        connectorRouteSetApplied(routeNetworks, true, ackKind);
      if (!ack.ok) {
        throw new Error(`recreate-ack-failed:${ack.error ?? 'no-message'}`);
      }
      hilog.info(DOMAIN, TAG,
        'VPN_ROUTE_SET_ACKED|requestId=%{public}s|routes=%{public}d|isRecreate=true|' +
        'ackKind=%{public}s',
        this.requestId, routeNetworks.length, ackKind);
      // The ACCEPTED set, recorded for the ONE operation that asked for this
      // rebuild (operation-local `ackOut`, never a shared field): a caller that
      // reports "applied" must name THIS set — the recreate's own snapshot —
      // not the setter answer it saw before the rebuild. An EMPTY accepted set
      // is recorded verbatim as `[]`.
      if (ackOut !== undefined) {
        ackOut.routes = routeNetworks;
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
      const recreateFields: DiagFields = { 'result': 'ok', 'count': merged.routes.length };
      this.diagMilestone('vpn_recreate_done', recreateFields);
      // roam-1005-0014 A.2: one delayed recheck a few seconds after DONE —
      // the observed teardown race could land AFTER the immediate
      // verification passed (the device lost the interface somewhere inside
      // the first minute). scheduleIfaceRecheck owns the timer lifecycle.
      this.scheduleIfaceRecheck();
      // 场景B roaming: the roam trigger's follow-up VPN_LAN_ROAM line reads
      // this (log truth only — no gate, no behavior).
      this.lastRecreateOutcome = 'ok';
    } catch (error) {
      // fail-closed: ANY failed leg leaves no half-configured device — the
      // connector is stopped and whatever generation is current is destroyed
      this.failRecreate(stage, error as Error);
    } finally {
      this.recreateInFlight = false;
      // N2 first batch C/D columns: the recreate leg's own retry is a supply
      // entry like any other — a formal-switch gate (or a held one) must block
      // it, or the rebuild's tail would refill the core on a network the plan
      // never released.
      if (this.networkDirty && !this.destroyRequested && this.networkNetId !== 0 &&
        !this.n2CdProductionBlocked()) {
        this.recoverPhysicalNetwork().catch((error: Error): void => {
          this.n2NetworkRecoveryFail(error);
        });
      }
    }
    // The REAL outcome of this leg ('ok' | 'fail:<stage>' | 'skip:<reason>').
    return this.lastRecreateOutcome;
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

  // ---------------------------------------------------------------------------
  // roam-1005-0014: vpn-tun interface presence (settle / verify / watchdog).
  // PRIMARY probe: the official @ohos.net.statistics per-NIC byte counter —
  // device run 0032 (2026-09-26) proved it DISTINGUISHES existence:
  // getIfaceRxBytes('vpn-tun') resolved 1040/520 while the tunnel was up, and
  // the same call for a nonexistent name rejected with BusinessError code
  // 2103012 (never a silent 0). That is exactly the observable the device
  // defect hid: an open in-process fd over a REMOVED netdev. (/proc/net/dev
  // is Permission denied for the app — run 0017; /sys/class/net is
  // unverified.) Classification is code-exact on purpose: only 2103012 says
  // 'absent'; any other rejection or a non-numeric resolution is 'unknown'
  // and every consumer treats 'unknown' leniently — an unreadable counter
  // must never tear down a working tunnel, it only cannot save a broken one.
  // ---------------------------------------------------------------------------
  private async probeVpnIfacePresence(): Promise<string> {
    try {
      const rx: number = await statistics.getIfaceRxBytes('vpn-tun');
      return Number.isFinite(rx) && rx >= 0 ? 'present' : 'unknown';
    } catch (error) {
      const code: string = String((error as BusinessFailure).code ?? 'none');
      return code === '2103012' ? 'absent' : 'unknown';
    }
  }

  // The settle wait between a bounded destroy() and the next create():
  // at least the floor (800 ms first attempt / 1600 ms on the retry — the
  // system-side teardown window), then until the OLD vpn-tun's statistics
  // counter says the interface is gone, with floor+cap as the fuse. One
  // summary line carries what was waited and why it exited (gone /
  // still-present / unknown / cap).
  private async waitRecreateSettle(attempt: number): Promise<void> {
    const floorMs: number = attempt <= 1 ? RECREATE_SETTLE_MIN_MS : RECREATE_SETTLE_RETRY_MIN_MS;
    const startedAtMs: number = Date.now();
    let presence: string = await this.probeVpnIfacePresence();
    let elapsedMs: number = 0;
    while (true) {
      elapsedMs = Date.now() - startedAtMs;
      if (settleWaitAction(elapsedMs, floorMs, RECREATE_SETTLE_CAP_MS, presence) === 'proceed') {
        break;
      }
      await sleep(RECREATE_SETTLE_POLL_MS);
      presence = await this.probeVpnIfacePresence();
    }
    const exitReason: string = presence !== 'present'
      ? (presence === 'absent' ? 'gone' : 'unknown')
      : (elapsedMs >= floorMs + RECREATE_SETTLE_CAP_MS ? 'cap' : 'still-present');
    hilog.info(DOMAIN, TAG,
      'VPN_RECREATE_SETTLE|requestId=%{public}s|attempt=%{public}d|floorMs=%{public}d|' +
      'waitedMs=%{public}d|result=%{public}s',
      this.requestId, attempt, floorMs, elapsedMs, exitReason);
  }

  // Post-create registry verification with bounded short retries (registry
  // propagation lag). Logs the task-mandated VPN_RECREATE_VERIFY line per
  // try; returns 'ok' (interface listed / registry unreadable — lenient),
  // 'retry' (absent, another full attempt remains) or 'fail' (absent on the
  // last attempt — the caller's shared catch runs the fail-closed teardown).
  private async verifyRecreatedIface(attempt: number, maxAttempts: number): Promise<string> {
    let outcome: string = 'fail';
    for (let tryIndex: number = 1; tryIndex <= RECREATE_IFACE_VERIFY_TRIES; tryIndex++) {
      const presence: string = await this.probeVpnIfacePresence();
      outcome = ifaceVerifyOutcome(presence, attempt, maxAttempts);
      if (outcome === 'ok') {
        hilog.info(DOMAIN, TAG,
          'VPN_RECREATE_VERIFY|requestId=%{public}s|attempt=%{public}d|try=%{public}d|' +
          'iface=%{public}s|result=%{public}s',
          this.requestId, attempt, tryIndex, presence, 'ok');
        return 'ok';
      }
      if (tryIndex < RECREATE_IFACE_VERIFY_TRIES) {
        hilog.warn(DOMAIN, TAG,
          'VPN_RECREATE_VERIFY|requestId=%{public}s|attempt=%{public}d|try=%{public}d|' +
          'iface=%{public}s|result=%{public}s',
          this.requestId, attempt, tryIndex, presence, 'recheck');
        await sleep(RECREATE_IFACE_VERIFY_GAP_MS);
      } else {
        hilog.error(DOMAIN, TAG,
          'VPN_RECREATE_VERIFY|requestId=%{public}s|attempt=%{public}d|try=%{public}d|' +
          'iface=%{public}s|result=%{public}s',
          this.requestId, attempt, tryIndex, presence, outcome);
      }
    }
    return outcome;
  }

  // One delayed recheck after a COMPLETED recreate: if the registry dropped
  // the fresh vpn-tun inside the recheck window (the late-teardown race the
  // immediate verification cannot see), route it through the same
  // one-retry-then-fail-closed handler as the watchdog. One shot per
  // completed recreate; the timer is owned here and cleared on teardown.
  private scheduleIfaceRecheck(): void {
    if (this.ifaceRecheckTimer >= 0) {
      clearTimeout(this.ifaceRecheckTimer);
      this.ifaceRecheckTimer = -1;
    }
    this.ifaceRecheckTimer = setTimeout((): void => {
      this.ifaceRecheckTimer = -1;
      this.ifaceRecheckProbe().catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG,
          'VPN_IFACE_PROBE_REJECTED|requestId=%{public}s|summary=%{public}s',
          this.requestId, safeError(error));
      });
    }, RECREATE_IFACE_RECHECK_MS);
  }

  // The recheck's async body (the timer callback itself is synchronous):
  // probe, then — only for a definitive absence on a still-live generation —
  // log the task-mandated VPN_RECREATE_VERIFY line and route the loss into
  // the one-retry-then-fail-closed handler.
  private async ifaceRecheckProbe(): Promise<void> {
    const presence: string = await this.probeVpnIfacePresence();
    if (presence !== 'absent') {
      return;
    }
    if (this.destroyRequested || this.connectorDeathHandled || this.recreateInFlight) {
      return;
    }
    if (this.connection === undefined || !this.createSettled || this.tunFd < 0) {
      return;
    }
    hilog.error(DOMAIN, TAG,
      'VPN_RECREATE_VERIFY|requestId=%{public}s|attempt=recheck|try=1|iface=%{public}s|' +
      'result=%{public}s',
      this.requestId, presence, 'lost');
    this.handleVpnInterfaceLost('recheck');
  }

  // The watchdog tick (A.4): a settled, live generation whose vpn-tun the
  // registry definitively no longer lists — while the user did NOT stop —
  // gets the honest handling. EVERY loss re-arms the automatic full rebuild
  // (roam-1005-0124: per recreate EVENT, not per session — the session latch
  // starved the second network switch of its retry and fail-closed a
  // healthy session); the sliding-window cap is the anti-storm bound, and
  // past it the existing fail-closed teardown applies (connector stopped +
  // generation destroyed + status document removed → the UI shows
  // 未连接 / VPN 已断开，请重新连接).
  private checkVpnInterfaceAlive(presence: string): void {
    const generationLive: boolean = !this.destroyRequested && !this.connectorDeathHandled &&
      !this.recreateInFlight && this.destroyPromise === undefined &&
      this.connection !== undefined && this.createSettled && this.tunFd >= 0;
    const retryAllowed: boolean = ifaceLostRetryAllowed(this.ifaceLostRetryLogMs, Date.now(),
      IFACE_LOST_RETRY_WINDOW_MS, IFACE_LOST_RETRY_WINDOW_MAX);
    const decision: string = ifaceLostDecision(presence, retryAllowed, generationLive);
    if (decision === 'ignore') {
      return;
    }
    this.handleVpnInterfaceLost('watcher');
  }

  private handleVpnInterfaceLost(source: string): void {
    // re-check the guards INSIDE the same tick (the decision was taken just
    // above, but a concurrent recreate entry must still be impossible).
    if (this.destroyRequested || this.connectorDeathHandled || this.recreateInFlight) {
      return;
    }
    if (this.connection === undefined || !this.createSettled) {
      return;
    }
    const nowMs: number = Date.now();
    const retryAllowed: boolean = ifaceLostRetryAllowed(this.ifaceLostRetryLogMs, nowMs,
      IFACE_LOST_RETRY_WINDOW_MS, IFACE_LOST_RETRY_WINDOW_MAX);
    hilog.error(DOMAIN, TAG,
      'VPN_IFACE_LOST|requestId=%{public}s|source=%{public}s|retryUsed=%{public}s|' +
      'action=%{public}s',
      this.requestId, source, String(!retryAllowed),
      retryAllowed ? 'full-recreate-retry' : 'teardown-vpn-fail-closed');
    if (!retryAllowed) {
      this.failRecreate(`iface-lost:${source}`, new Error('vpn-iface-absent-after-retry'));
      return;
    }
    this.ifaceLostRetryLogMs.push(nowMs);
    this.performControlledRecreate(`vpn-iface-lost:${source}`).catch((error: Error): void => {
      hilog.error(DOMAIN, TAG,
        'VPN_RECREATE_REJECTED|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error));
      this.failRecreate('unexpected-throw', error);
    });
  }

  // N8 recreate failure = the N3-7 fail-closed teardown: stop the watcher,
  // stop the connector (idempotent), destroy whatever generation is current
  // (latched destroyOnce — the final teardown latch).
  private failRecreate(stage: string, error: Error): void {
    // 场景B roaming: record WHY the rebuild died, so the roam trigger's
    // follow-up VPN_LAN_ROAM line carries the failure stage (log truth only
    // — the fail-closed teardown below is unchanged).
    this.lastRecreateOutcome = `fail:${stage}`;
    hilog.error(DOMAIN, TAG,
      'VPN_RECREATE_FAIL|requestId=%{public}s|stage=%{public}s|' +
      'action=teardown-vpn-fail-closed|summary=%{public}s',
      this.requestId, stage, safeError(error));
    const milestoneFields: DiagFields = { 'stage': stage, 'result': 'fail' };
    this.diagMilestone('vpn_recreate_failed', milestoneFields);
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
      // 2026-10-09 live route apply: the SAME 100 ms tick carries the page's
      // apply-request mailbox (NO new timer, no commonEvent, no second
      // startVpnExtensionAbility). Teardown already returned above, and the
      // consumer holds its own busy latch, so a tick can never overlap an
      // in-flight apply.
      this.maybePollRouteApplyRequest();
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
        protectDeviation: this.protectDeviationRouteExclusion,
        productionGateBlocked: this.n2CdProductionBlocked()
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
        protectDeviation: this.protectDeviationRouteExclusion,
        productionGateBlocked: this.n2CdProductionBlocked()
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

  // ---------------------------------------------------------------------------
  // 2026-10-09 live route apply (instant apply: 路由选择页 → live VPE).
  // ---------------------------------------------------------------------------
  // The page saves the scoped preference and then writes a RETAINED mailbox
  // request (NetBirdRouteRequestStore) naming only WHICH scope changed. This
  // consumer rides the EXISTING 100 ms socket-supply tick (no new timer):
  //
  //   read request (bounded, never cached by mtime/size) → duplicate? → scope
  //   check → load the LATEST preference document for THIS management URL →
  //   re-check the live-generation guards → in ONE synchronous JS turn let the
  //   native core adopt the selection and run the EXISTING controlled recreate
  //   with kind 'user' → answer.
  //
  // The single synchronous turn is the correctness core of this increment:
  // performControlledRecreate sets `recreateInFlight` SYNCHRONOUSLY before its
  // first await, so no 5 s status-watcher tick can observe the native
  // `recreate.required` this setter just raised and "steal" the change as an
  // AUTOMATIC recreate — which would spend the automatic 3 / 30 s budget a
  // manual toggle must never touch. Any await between the setter and the
  // recreate call would open exactly that window.
  //
  // Nothing here stops, starts or re-logs-in the connector (same connector / WG
  // / ICE / management session); the ONLY teardown reachable from this file is
  // the pre-existing fail-closed one, and the socket-supply interval is never
  // touched (it stops only with the pre-existing teardown).
  private maybePollRouteApplyRequest(): void {
    if (this.routeRequestBusy || this.destroyRequested || this.destroyPromise !== undefined) {
      return;
    }
    this.routeRequestBusy = true;
    this.consumeRouteApplyRequest()
      .then((): void => {
        this.routeRequestBusy = false;
      })
      .catch((error: Error): void => {
        // The handler answers every request it reads; this belt only guards an
        // unexpected throw and MUST release the latch, or the mailbox wedges.
        this.routeRequestBusy = false;
        hilog.warn(DOMAIN, TAG,
          'VPN_ROUTE_APPLY_FAIL|requestId=%{public}s|stage=handler|summary=%{public}s',
          this.requestId, safeError(error));
      });
  }

  private async consumeRouteApplyRequest(): Promise<void> {
    const filesDir: string = this.context.filesDir;
    if (filesDir.length === 0 || this.managementUrl.length === 0) {
      return; // no shared scope to read/write the documents under
    }
    let request: RouteApplyRequest | null = null;
    try {
      request = await readRouteApplyRequest(filesDir);
      this.routeApplyReadErrorClass = '';
    } catch (error) {
      // Malformed / wrong version / unreadable: there is no id to answer
      // honestly, so NO result is written under a guessed one, and the document
      // is NEVER unlinked (a fresh write must never be lost). Logged once per
      // class instead of once per 100 ms tick.
      const reason: string = (error as RoutePreferencesError).reason;
      const errorClass: string = typeof reason === 'string' && reason.length > 0
        ? reason : 'unavailable';
      if (errorClass !== this.routeApplyReadErrorClass) {
        this.routeApplyReadErrorClass = errorClass;
        hilog.warn(DOMAIN, TAG,
          'VPN_ROUTE_APPLY_READ_FAIL|requestId=%{public}s|stage=request|errorClass=%{public}s',
          this.requestId, errorClass);
      }
      return;
    }
    if (request === null || request.id === this.lastRouteApplyId) {
      return; // no request, or the retained mailbox's id was already consumed
    }
    const requestId: string = request.id;
    if (request.management_url !== this.managementUrl) {
      // The request names ANOTHER server's scope: it can never be applied to
      // this session, and it is answered (consumed) so it cannot be retried
      // forever. The running connection is untouched.
      this.lastRouteApplyId = requestId;
      hilog.warn(DOMAIN, TAG,
        'VPN_ROUTE_APPLY_SCOPE|requestId=%{public}s|result=scope-mismatch', this.requestId);
      await this.writeRouteApplyAnswer(requestId, ROUTE_APPLY_STATE_FAILED,
        ROUTE_APPLY_MODE_NONE, 'scope-mismatch', []);
      return;
    }
    // The LATEST preference document for THIS management URL, always read here
    // — never a selection copy carried by the request. An old offline
    // notification therefore can never revert a newer state.
    let selection: RouteSelection | null = null;
    try {
      selection = await loadRouteSelection(filesDir, this.managementUrl);
    } catch (error) {
      const reason: string = (error as RoutePreferencesError).reason;
      const errorClass: string = typeof reason === 'string' && reason.length > 0
        ? reason : 'unavailable';
      this.lastRouteApplyId = requestId;
      hilog.error(DOMAIN, TAG,
        'VPN_ROUTE_APPLY_FAIL|requestId=%{public}s|stage=route-selection|errorClass=%{public}s',
        this.requestId, errorClass);
      // Nothing was handed to the native core: the running VPN is untouched.
      await this.writeRouteApplyAnswer(requestId, ROUTE_APPLY_STATE_FAILED,
        ROUTE_APPLY_MODE_NONE, `route-selection-${errorClass}`, []);
      return;
    }
    // null = no document for this scope: the pre-existing all-on default (the
    // pure module's own routeSelectionDefault shape).
    const desired: RouteSelection = selection !== null ? selection : ALL_ON_ROUTE_SELECTION;

    // ---- THE ONE SYNCHRONOUS DECISION TURN --------------------------------
    // Nothing may await between this re-check, the native setter and the
    // recreate call (see the block comment above).
    if (this.destroyRequested || this.connectorDeathHandled || this.recreateInFlight ||
      this.connection === undefined || !this.createSettled || this.tunFd < 0) {
      // Teardown latched, or the generation is not settled (start still
      // running, an AUTOMATIC or LAN recreate in flight). The request stays
      // UNCONSUMED — the retained document is read again by a later tick, which
      // is exactly the "the newest queued id is processed after the current
      // recreate" rule. A teardown answers nothing: the page reads the vanished
      // live document as 将在下次连接时生效, never as 已生效.
      return;
    }
    const setter: RouteSelectionSetResult | null = connectorRouteSelectionSet(desired);
    if (setter === null) {
      // Unreadable native answer (missing export / platform throw / malformed
      // document). NOTHING is claimed: no recreate and no "applied", and the
      // running connection is left alone.
      this.lastRouteApplyId = requestId;
      hilog.error(DOMAIN, TAG,
        'VPN_ROUTE_APPLY_FAIL|requestId=%{public}s|stage=selection-set|errorClass=unreadable',
        this.requestId);
      await this.writeRouteApplyAnswer(requestId, ROUTE_APPLY_STATE_FAILED,
        ROUTE_APPLY_MODE_NONE, 'selection-set-unreadable', []);
      return;
    }
    if (!setter.ok) {
      const error: string = setter.error !== undefined && setter.error.length > 0
        ? setter.error : 'selection-set-refused';
      this.lastRouteApplyId = requestId;
      hilog.error(DOMAIN, TAG,
        'VPN_ROUTE_APPLY_FAIL|requestId=%{public}s|stage=selection-set|errorClass=%{public}s',
        this.requestId, error);
      await this.writeRouteApplyAnswer(requestId, ROUTE_APPLY_STATE_FAILED,
        ROUTE_APPLY_MODE_NONE, error, []);
      if (error === 'wg-apply-failed') {
        // The native MUTATED its selection and its WG seam then refused the
        // routes: its wg_apply_failed latch is the documented fail-closed state
        // (the state names a route set the data plane does not have). The
        // EXISTING fail-closed teardown applies — never a tunnel that lies
        // about its routes.
        this.failRecreate('route-apply-wg', new Error('route-selection-wg-apply-failed'));
      }
      return;
    }
    if (!setter.available) {
      // No applied NetworkMap: the selection was stored core-side for the NEXT
      // map and NOTHING was handed to the OS. That is exactly the contract's
      // `stored-no-map` answer: state `applied` with a mode the ONE predicate
      // (routeApplyResultIsApplied) reads as NOT applied, so the page words it
      // 将在下次连接时生效 instead of 已生效. Never a false green.
      this.lastRouteApplyId = requestId;
      await this.writeRouteApplyAnswer(requestId, ROUTE_APPLY_STATE_APPLIED,
        ROUTE_APPLY_MODE_STORED_NO_MAP, '', setter.desired_routes);
      return;
    }
    if (!setter.recreate_needed) {
      // The native just PROVED the applied set already equals the desired set
      // (`recreate_needed` is computed against the ACKed applied set), so the OS
      // side needs no change at all: no destroy, no create, no feed, no ACK.
      this.lastRouteApplyId = requestId;
      hilog.info(DOMAIN, TAG,
        'VPN_ROUTE_APPLY_RESULT|requestId=%{public}s|mode=no-os-change|routes=%{public}d',
        this.requestId, setter.desired_routes.length);
      await this.writeRouteApplyAnswer(requestId, ROUTE_APPLY_STATE_APPLIED,
        ROUTE_APPLY_MODE_NO_OS_CHANGE, '', setter.desired_routes);
      return;
    }
    // A real OS-side change is required: the SAME controlled recreate the
    // automatic path uses, reported as the USER's ACK kind (no automatic
    // budget). The call is synchronous — its latch is set before this await.
    hilog.info(DOMAIN, TAG,
      'VPN_ROUTE_APPLY_BEGIN|requestId=%{public}s|mode=recreated|desired=%{public}d',
      this.requestId, setter.desired_routes.length);
    // Operation-local carrier for the set the recreate's OWN snapshot ACKs.
    // `setter.desired_routes` is the map as the SETTER saw it one native call
    // earlier; the recreate reads the map again, and the management worker may
    // have advanced it in between (see RecreateAckOut).
    const acked: RecreateAckOut = { routes: null };
    const outcome: string = await this.performControlledRecreate(
      `route-apply:${requestId}`, 'user', acked);
    // The accepted set, read ONCE as a local (the same null-narrowing shape the
    // initial-leg ACK uses): null = this operation never reached an accepted ACK.
    const ackedRoutes: string[] | null = acked.routes;
    this.lastRouteApplyId = requestId;
    if (outcome === 'ok' && ackedRoutes !== null) {
      // Applied ONLY now: the recreate verified the fresh interface, fed the
      // TUN fd and the ACK was accepted. The reported set is the one THIS
      // recreate's own snapshot ACKed (`ackedRoutes`, `[]` included) — never
      // `setter.desired_routes`, which is a different native read.
      await this.writeRouteApplyAnswer(requestId, ROUTE_APPLY_STATE_APPLIED,
        ROUTE_APPLY_MODE_RECREATED, '', ackedRoutes);
      return;
    }
    // 'fail:<stage>' (the fail-closed teardown already ran) or 'skip:<reason>'
    // (nothing was rebuilt): the OS side is NOT at the requested state. Honest
    // failure — never a synthesized 已生效. ('ok' without a recorded ACK set is
    // unreachable by construction; if it ever happened it is answered as a
    // failure, never as applied.)
    const failure: string = outcome === 'ok'
      ? 'recreate-ack-set-unrecorded' : `recreate-${outcome}`;
    hilog.error(DOMAIN, TAG,
      'VPN_ROUTE_APPLY_FAIL|requestId=%{public}s|stage=recreate|errorClass=%{public}s',
      this.requestId, failure);
    await this.writeRouteApplyAnswer(requestId, ROUTE_APPLY_STATE_FAILED,
      ROUTE_APPLY_MODE_NONE, failure, []);
  }

  // The ONE place the answer document is built and written, so the
  // id/state/mode/error vocabulary cannot drift between branches. The write is
  // the store's atomic temp+fsync+rename; a failure is logged as a class only
  // (never a body) and never disturbs the flow it answers.
  private async writeRouteApplyAnswer(id: string, state: string, mode: string,
    error: string, routes: string[]): Promise<void> {
    const result: RouteApplyResult = {
      id: id,
      state: state,
      mode: mode,
      error: error,
      routes: routes,
      at_ms: Date.now()
    };
    try {
      await writeRouteApplyResult(this.context.filesDir, result);
      hilog.info(DOMAIN, TAG,
        'VPN_ROUTE_APPLY_ANSWER|requestId=%{public}s|id=%{public}s|state=%{public}s|' +
        'mode=%{public}s|routes=%{public}d',
        this.requestId, id, state, mode, routes.length);
    } catch (writeError) {
      hilog.warn(DOMAIN, TAG,
        'VPN_ROUTE_APPLY_ANSWER_FAIL|requestId=%{public}s|state=%{public}s|summary=%{public}s',
        this.requestId, state, safeError(writeError as Error));
    }
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
        !this.networkRecoveryActive ||
        // N2 first batch C/D columns: the formal-switch gate, checked AFTER the
        // protect await and immediately BEFORE the actual core feed.
        this.n2CdProductionBlocked(epoch)) {
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
    // roam-1005-0014: the delayed iface recheck belongs to the watcher's
    // lifecycle — teardown cancels it (a firing recheck after teardown would
    // hit the guards, but the timer itself must not leak either).
    if (this.ifaceRecheckTimer >= 0) {
      clearTimeout(this.ifaceRecheckTimer);
      this.ifaceRecheckTimer = -1;
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
    // Startup race (2026-10-07): holding the fd is NOT a settled generation.
    // `createSettled` is announced at the END of this leg (below), once the
    // fail-closed protect gate, the WG data-plane feed, the ICE pool
    // protect/feed and the initial ACK have all returned. The raw fd is still
    // published here: the fd contract, the feeds and the pre-destroy audit all
    // read `tunFd`, and the rebuildable gate is the COMPOSITE
    // (connection && createSettled && tunFd >= 0) — never `tunFd` alone.
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
    //
    // N2 P5 (v4.1 §4 I row): when the evidence arm is on, the PRE control of
    // the I leg must HIT BEFORE this first ICE pool feed / gather — the TUN
    // tee is already attached (feedWgDataplane above), so the frozen anchor
    // holds. `n2IceSandwichPre` never throws and never changes the ICE send
    // content/order; it only waits (bounded by the core's 3 s control window)
    // for the pre marker's HIT, and the core records the verdict itself.
    if (this.n2Evidence !== undefined) {
      await this.n2IceSandwichPre();
    }
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

    // N2 P5 (v4.1 §4 Ws row): the Ws PRE control must be in place BEFORE the
    // selected egress socket is ever handed to WG. The ICE pool was just fed,
    // so no nomination can have happened yet; the core refuses honestly if one
    // somehow did (`first-send-before-pre-control`). The POST control is
    // offered later by the watcher tick, once the core has observed the first
    // real `wg_sendto`. NOTHING here defers or reorders `peer_conn.rs`'s
    // "attach egress, then apply endpoint" sequence.
    if (this.n2Evidence !== undefined) {
      await this.n2WsSandwichPre();
    }

    // VPN is up (create accepted + protect passed + WG data plane fed):
    // N8 — ACK the route set applied to the platform VpnConfig (initial
    // create; arms the core's desired-vs-applied recreate comparison), then
    // start the status / network-config watch. Runtime map changes are
    // logged as NOT applied (platform VpnConfig fixed at create — see the
    // watcher); a desired-route-set CHANGE goes through the controlled
    // recreate below.
    this.networkRecoveryReady = true;
    // `networkEpoch` is one-based (the sockets' protect generation), so "a
    // switch already happened while this session was starting" is `> 1`.
    if (this.networkEpoch > 1) {
      this.recoverPhysicalNetwork().catch((error: Error): void => {
        this.n2NetworkRecoveryFail(error);
      });
    }
    this.ackAppliedRoutesInitial();
    // Startup race (2026-10-07): the generation becomes settled — and so
    // rebuildable / live-appliable — only HERE, after this leg really
    // completed (protect gate + WG feed + ICE pool feed + initial ACK above).
    // Announcing it on the fd alone let a mailbox tick or a watcher tick act
    // on a generation whose own initialization was still running (and consume
    // the pending route request against a not-yet-ACKed applied set).
    // The ACK above is best-effort: a skip / refusal / throw is caught there
    // and must NOT leave the leg un-settled (that would make a session never
    // ready), so "ACK ok" is deliberately not a condition. A teardown or a
    // connector death latched inside this leg IS terminal: a late completion
    // must never reopen the rebuildable state — same latched predicate the
    // protected-socket gates read.
    if (!this.destroyRequested && !this.connectorDeathHandled &&
      this.destroyPromise === undefined) {
      this.createSettled = true;
    }
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
    const fedLine: string = `VPN_WG_DATAPLANE_FED|requestId=${this.requestId}|wgFd=${wgFd}|` +
      `tunFd=${this.tunFd}|deviceUp=true|` +
      `mono_ms=${typeof tun.mono_ms === 'number' ? tun.mono_ms : -1}|wall_ms=${Date.now()}`;
    hilog.info(DOMAIN, TAG, '%{public}s', fedLine);
    this.n2RingAppend(fedLine);
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
    // Size the pool from a LIVE status read: at this point the watcher has not
    // necessarily sampled the peer count yet (first feed / post-switch
    // recovery), and the fixed 8 of smoke 0050 starved 2 of 6 peers. The same
    // read carries the core's per-peer need (smoke 0051: need 6 with 3 allowed
    // interfaces); a missing sample keeps the target the watcher last derived
    // (never below the 8 minimum), and a missing need_per_peer field falls back
    // to the shell's 4-per-peer constant. No new timer — this is one status
    // read per pool feed.
    const live: ConnectorStatusResult | null = pollStatusOrNull();
    if (live !== null) {
      this.iceTargetDepth =
        iceSocketTargetDepth(live.peer_count, iceSocketNeedPerPeer(live.ice_sockets?.need_per_peer));
    }
    const target: number = this.iceTargetDepth;
    let fedCount: number = 0;
    for (let i = 0; i < target; i++) {
      const ok: boolean = await this.feedOneIceSocket(connection);
      if (ok) {
        fedCount++;
      }
    }
    if (fedCount === 0) {
      hilog.error(DOMAIN, TAG,
        'VPN_ICE_FEED_FAIL|requestId=%{public}s|stage=pool|fed=0|size=%{public}d',
        this.requestId, target);
      return false;
    }
    hilog.warn(DOMAIN, TAG,
      'VPN_ICE_POOL|fed=%{public}d|size=%{public}d', fedCount, target);
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
      if (this.iceQueueDepth >= this.iceTargetDepth || this.iceTopUpActive || this.networkDirty) {
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

  // Feed enough sockets to close the gap to the demand-sized target, in one
  // bounded pass of at most ICE_TOPUP_BATCH (8). One gather round needs one
  // socket per IPv4 interface, so the pre-0050 batch of 4 served only ONE peer
  // per cycle.
  private async topUpIceBatch(): Promise<number> {
    const connection: vpnExtension.VpnConnection | undefined = this.connection;
    if (this.iceTopUpActive) { return 0; }
    // N2 first batch C/D columns: the formal-switch gate blocks the whole
    // top-up funnel (the watcher's own call site and the interval's callback
    // both land here), so a HELD gate cannot keep topping the pool up after the
    // recovery it blocked.
    const paused: boolean = this.networkDirty || this.n2CdProductionBlocked() ||
      protectedConnectionRejectionReason({
        destroyRequested: this.destroyRequested, destroyInFlight: this.destroyPromise !== undefined,
        recreateInFlight: this.recreateInFlight, hasConnection: connection !== undefined,
        connectionUnchanged: this.connection === connection
      }).length > 0 || connection === undefined;
    // The gap comes from the LATEST queued depth the shell knows: every status
    // sample and every accepted feed refreshes it (feedOneIceSocket writes the
    // core's own post-feed `queued` back), and the loop below re-reads it after
    // each socket, so a queue that drained since the last sample is still
    // resupplied in full instead of on a stale reading.
    const want: number = iceSocketTopUpCount(this.iceQueueDepth, this.iceTargetDepth, paused);
    if (connection === undefined || want === 0) { return 0; }
    this.iceTopUpActive = true;
    let fed: number = 0;
    try {
      for (let n = 0; n < want; n++) {
        if (iceSocketTopUpCount(this.iceQueueDepth, this.iceTargetDepth, paused) === 0) { break; }
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
  // UI phase 1: the document is built by the pure NetBirdUiModel module from
  // this status snapshot PLUS one connector_network_config read (own
  // fqdn/address + the per-peer merged view). Best-effort: a failure here
  // must never disturb the VPN.
  //
  // Defect 2026-10-04 (home card "NetBird IP —、域名 —、对端 0/0 在线" while
  // the peers tab listed 6): a TRANSIENT connector_network_config failure
  // used to overwrite the document with the honest-empty merge (null
  // address/fqdn, empty peers) while state stayed connected — the home card
  // and the peers tab then disagreed until the next good tick. The map does
  // not vanish because one read hiccupped: keep the LAST GOOD network
  // snapshot and republish it (an explicit available:false — no map yet /
  // connector stopped — still publishes the honest empties and drops the
  // cache). 已连接 itself already requires the document's own address
  // (NetBirdUiModel.homeStateText), so a genuinely map-less document can
  // never render 已连接.
  private lastGoodNetwork: CoreNetworkConfig | null = null;

  // 2026-10-12 upstream tap handling: the ENGINE's own state report, written from
  // the live core status without any watcher tick. The official Android client gets
  // this for free (the run notifies its state listeners synchronously,
  // EngineRunner.java:182) and the home screen's whole tap handling is driven by
  // it; on HarmonyOS the two processes share no callback, so the status document IS
  // the report channel. Called (a) the moment the run starts — before the bounded
  // network-map wait, so the UI's 7 s pending window always falls back onto a REAL
  // state — and (b) on each map-wait poll so the report never goes stale while the
  // engine is still starting. Throttled to the watcher cadence; best-effort, never
  // throws, and it adds no new milestone name.
  private publishEngineStateReport(force: boolean = false): void {
    const nowMs: number = Date.now();
    // The RUN'S OWN first report is never throttled: a new attempt must be visible
    // to the UI even when the previous one reported milliseconds ago. Only the
    // map-wait repeats follow the watcher cadence.
    if (!force && nowMs - this.lastEngineReportAtMs < STATUS_POLL_MS) {
      return;
    }
    this.lastEngineReportAtMs = nowMs;
    let status: ConnectorStatusResult | null = null;
    try {
      status = pollStatusOrNull();
    } catch (error) {
      status = null;
    }
    if (status === null) {
      // the core did not answer: report NOTHING rather than invent a state.
      return;
    }
    this.publishStatusForUi(status, 'unknown', true);
    hilog.info(DOMAIN, TAG,
      'VPN_ENGINE_REPORT|requestId=%{public}s|state=%{public}s|running=%{public}s|' +
      'note=engine-state-report',
      this.requestId, status.state, String(status.running));
  }

  // ifacePresence is the vpn-tun statistics-counter probe computed once per
  // watcher tick by the caller: 'present' publishes iface_present=true,
  // 'absent' publishes iface_present=false (the UI's 已连接 gate), 'unknown'
  // OMITS the field (an unreadable counter must not flip the UI to a fake
  // disconnected). The VPN_UI_STATUS diagnostic line (change-or-60 s, same
  // dedup discipline as VPN_CONNECTOR_STATUS) pins, per publish, exactly the
  // three fields the home card renders — the 2026-10-04 defect investigation
  // had no such line, so document content vs UI display could not be told
  // apart from the device log.
  private publishStatusForUi(status: ConnectorStatusResult, ifacePresence: string,
    allowBeforeWatcher: boolean = false): void {
    // Teardown guard: the probe continuation calling this runs one microtask
    // behind its tick — a recreate triggered by that same tick may have
    // already failed closed (or the connector died) and REMOVED the document
    // by now. A late publish would resurrect a stale "connected" document no
    // one will ever refresh or remove again; the dead watcher latches make
    // that impossible.
    if (this.destroyRequested || this.connectorDeathHandled) {
      return;
    }
    // 2026-10-12: the START leg publishes its own report BEFORE the watcher exists
    // (the watcher only starts once the VPN really is up, :6420) — that is the
    // engine reporting its own state while it starts, not a late tick. Every
    // steady-state publish still requires a live watcher.
    if (this.watcherTimer < 0 && !allowBeforeWatcher) {
      return;
    }
    const path: string = `${this.context.filesDir}/netbird-status.json`;
    let file: fs.File | undefined = undefined;
    try {
      let network: CoreNetworkConfig | null = null;
      try {
        network = connectorNetworkConfig();
      } catch (netError) {
        // read hiccup: reuse the last good map instead of wiping the
        // identity/peers fields the UI already shows
        network = this.lastGoodNetwork;
      }
      if (network !== null && network.available) {
        this.lastGoodNetwork = network;
      } else {
        this.lastGoodNetwork = null;
      }
      const ifacePresent: boolean | undefined = ifacePresence === 'present'
        ? true : (ifacePresence === 'absent' ? false : undefined);
      // 2026-10-07 client-detail increment: the management authority (host +
      // port + scheme) rides the published document when this process has
      // parsed a management url; the URL string itself is never published.
      const doc: UiStatusDocument = buildUiStatusDocument(status, network,
        this.requestId, Date.now(), ifacePresent, this.managementUrl,
        this.managementEndpoint);
      file = fs.openSync(path, fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      fs.writeSync(file.fd, JSON.stringify(doc));
      const uiSignature: string = `${doc.state}|${doc.address !== null}|` +
        `${doc.peers.length}|${doc.iface_present === true}`;
      const now: number = Date.now();
      if (uiSignature !== this.lastUiStatusLogSignature ||
        now - this.lastUiStatusLogAtMs >= 60000) {
        this.lastUiStatusLogSignature = uiSignature;
        this.lastUiStatusLogAtMs = now;
        hilog.info(DOMAIN, TAG,
          'VPN_UI_STATUS|requestId=%{public}s|has_address=%{public}s|peers=%{public}d|' +
          'state=%{public}s|iface=%{public}s',
          this.requestId, String(doc.address !== null && doc.address !== undefined &&
            doc.address.length > 0), doc.peers.length, doc.state, ifacePresence);
      }
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

  // 2026-10-07 route-catalog publication: mirror the native `connector_routes`
  // getter into filesDir/netbird-routes.json so the UI can list the advertised
  // networks OFFLINE (no native process call from the UI process). Called from
  // the SAME status-tick callback as publishStatusForUi — no new timer.
  //
  // Publication rule: ONLY a readable catalog with `available:true` (an EMPTY
  // accepted map included) is written. `null` (unreadable / malformed native
  // document) and `available:false` (no connector / before the first map /
  // stopped) publish NOTHING, so the last known offline catalog is RETAINED
  // instead of erased. The document is the selection POLICY the core applied,
  // never a proof that the OS route table matches it — `enabled` keeps exactly
  // that meaning and no "applied" field is fabricated.
  //
  // Teardown discipline (same latches as publishStatusForUi): the guards are
  // re-checked BEFORE any write starts, so a tick that lands after
  // onDestroy / connector death / the watcher stopping can never seed a new
  // document; the async completion only RELEASES the in-flight latch — it never
  // publishes a second time, so a write that raced a teardown cannot be
  // followed by a stale one. The write itself is the store's atomic
  // temp+fsync+rename; failures are logged as a class only (never a document
  // body) and never disturb the tick.
  private publishRouteCatalogForUi(): void {
    if (this.destroyRequested || this.connectorDeathHandled || this.watcherTimer < 0) {
      return;
    }
    // The catalog document is scoped by the server this process started
    // against; before the first start there is no scope to write under.
    if (this.routeCatalogPublishInFlight || this.managementUrl.length === 0) {
      return;
    }
    const catalog: RouteCatalog | null = connectorRoutes();
    if (catalog === null || catalog.available !== true) {
      return;
    }
    const managementUrl: string = this.managementUrl;
    const requestId: string = this.requestId;
    this.routeCatalogPublishInFlight = true;
    writeRouteCatalog(this.context.filesDir, managementUrl, catalog, requestId, Date.now())
      .then((): void => {
        this.routeCatalogPublishInFlight = false;
      })
      .catch((error: Error): void => {
        this.routeCatalogPublishInFlight = false;
        hilog.warn(DOMAIN, TAG,
          'VPN_ROUTE_CATALOG_PUBLISH_FAIL|requestId=%{public}s|summary=%{public}s',
          requestId, safeError(error));
      });
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
        !this.networkRecoveryActive ||
        // N2 first batch C/D columns: the formal-switch gate, checked AFTER the
        // protect await and immediately BEFORE connectorIceSocketFeed.
        this.n2CdProductionBlocked(epoch)) {
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
      // N2 T7a (item 6): `protect_generation=` is the frozen E1 field name (the
      // earlier `generation=` spelling was unified with the core's `N2_SOCK`
      // receipts). N2 C/D first batch: the three shell protect phases also go to
      // the armed-only evidence ring (this is the fd-identity evidence of the
      // re-fed sockets during the cellular phase).
      const openLine: string = `N2_SOCK|phase=open|kind=${traceKind}|request_id=${traceId}|` +
        `fd=${fd}|protect_generation=${this.networkEpoch}|net_id=${this.networkNetId}|` +
        `wall_ms=${t0}|diag=true|is_evidence=false`;
      hilog.info(DOMAIN, TAG, '%{public}s', openLine);
      this.n2RingAppend(openLine);
      const beginLine: string = `N2_SOCK|phase=protect_begin|kind=${traceKind}|` +
        `request_id=${traceId}|fd=${fd}|protect_generation=${this.networkEpoch}|` +
        `net_id=${this.networkNetId}|wall_ms=${t0}|diag=true|is_evidence=false`;
      hilog.info(DOMAIN, TAG, '%{public}s', beginLine);
      this.n2RingAppend(beginLine);
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
      // N2 T7a (item 6): register with the core FIRST and take the socket's own
      // bound name from its reply, so the `phase=protect_settle` line can carry
      // `local=` (`unknown` when the number cannot be inspected — never an
      // invented address). The field names and the outcome vocabulary are the
      // core `N2_SOCK` ones (`status=success|failed`, `protect_generation=`,
      // `net_id=`, `elapsed_ms=`), matching the frozen E1 criterion
      // (`phase=protect_settle|status=success`).
      let local: string = 'unknown';
      try {
        const traced: Record<string, Object> = JSON.parse(core.n2_socket_trace_protect(fd,
          traceKind, `${traceId},${this.networkEpoch},${this.networkNetId},${elapsed}`,
          outcome)) as Record<string, Object>;
        if (typeof traced['local'] === 'string' && (traced['local'] as string).length > 0) {
          local = traced['local'] as string;
        }
      } catch (error) {
        local = 'unknown';
      }
      const settleLine: string = `N2_SOCK|phase=protect_settle|kind=${traceKind}|` +
        `request_id=${traceId}|fd=${fd}|status=${outcome ? 'success' : 'failed'}|` +
        `elapsed_ms=${elapsed}|protect_generation=${this.networkEpoch}|` +
        `net_id=${this.networkNetId}|local=${local}|wall_ms=${Date.now()}|` +
        `diag=true|is_evidence=false`;
      hilog.info(DOMAIN, TAG, '%{public}s', settleLine);
      this.n2RingAppend(settleLine);
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

  // 2026-10-09 (0.0.17) PRIOR_EXIT_UNCLEAN over-reporting fix: mark the
  // diagnostics marker as `stopping` SYNCHRONOUSLY on the onDestroy ENTRY. The
  // final `clean` state is still written by the late stopDiagnostics() only —
  // but on the device the JS context is destroyed ~3 ms after onDestroy returns
  // and the process exits within ~21 ms, so the destroyOnce continuation
  // (await createPromise -> await connection.destroy() -> stopDiagnostics())
  // NEVER runs: V016 ended at `vpn_destroy_phase phase=requested` with no
  // `settled` (netbird-vpn.log:1528-1547) and the marker stayed `open`, so the
  // NEXT session reported PRIOR_EXIT_UNCLEAN for an orderly user stop.
  // `stopping` records the entered teardown; NetBirdDiagnosticsCore's
  // isPriorUnclean() claims state=open ONLY, so the false report is gone — while
  // a real reclaim still leaves `open`, because the system's SIGKILL path never
  // calls onDestroy at all (negative control: 0 of the 4 system-killed v015
  // processes has a `vpn_lifecycle_stop` row).
  //
  // FAIL-SOFT and bounded: one fixed, allowlisted path; openSync/writeSync/
  // closeSync; NO fsync (a single line — a torn tail reads as the marker's
  // unknown state, never as a claim); every failure is swallowed into at most
  // ONE W line per ability instance (one onDestroy per extension instance, so in
  // practice one per process). Never throws into the lifecycle callback.
  //
  // WHY THE GUARD: a process that never reached startDiagnosticsLogTee() never
  // wrote its own `open` and does not own the marker file — writing `stopping`
  // there would silently DESTROY a previous session's real `open`, the only
  // evidence a prior reclaim left. That flag is set inside the same synchronous
  // function, immediately before it starts the diagnostics writer (the preceding
  // phoneLogConfigure() wrapper never throws), so false means "this process
  // never armed the marker" and the write is skipped.
  //
  // Residual window, stated honestly: if onDestroy lands BEFORE the helper's
  // asynchronous `open` write has reached the disk, that pending write lands
  // last and the marker still ends up `open` — exactly today's behaviour, never
  // worse.
  private writeStopMarkerSync(): void {
    if (this.stopMarkerWritten) {
      return;
    }
    this.stopMarkerWritten = true;
    if (!this.logTeeConfigured) {
      return;
    }
    let path: string = '';
    try {
      path = recordPath(this.context.filesDir, markerBasename('vpn'));
    } catch (error) {
      path = '';
    }
    if (path.length === 0) {
      return;
    }
    try {
      const file: fs.File = fs.openSync(path,
        fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      try {
        fs.writeSync(file.fd, buildMarker(new Date().toISOString(), process.pid,
          'vpn', MARKER_STATE_STOPPING));
      } finally {
        fs.closeSync(file);
      }
    } catch (error) {
      if (this.stopMarkerWarned) {
        return;
      }
      this.stopMarkerWarned = true;
      hilog.warn(DOMAIN, TAG, 'VPN_STOP_MARKER_WRITE_FAILED|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error as Error));
    }
  }

  onDestroy(): void {
    hilog.info(DOMAIN, TAG, 'VPN_ONDESTROY|requestId=%{public}s', this.requestId);
    // FIRST, and synchronously: nothing below this line is guaranteed to run
    // (see writeStopMarkerSync). It precedes the latch and every call that can
    // throw or take time (the native connector stop, the async destroy leg and
    // the late diagnostics stop that writes `clean`).
    this.writeStopMarkerSync();
    // Stop flow (N3-6): latch first (the async start flow must not create
    // past this point), stop the watch, stop the connector (idempotent,
    // safe with no connector), then the exactly-once VPN destroy — the
    // destroy semantics and fd contract are unchanged.
    this.destroyRequested = true;
    this.stopStatusAndNetcfgWatcher();
    this.n2RingStopStats();
    const milestoneFields: DiagFields = { 'stage': 'onDestroy', 'state': 'latched' };
    this.diagMilestone('vpn_lifecycle_stop', milestoneFields);
    // 2026-10-08: the system-event subscription dies with the extension
    // lifecycle; its closing line (and the suppressed-duplicate count of its
    // dedup window) goes through the SAME sink while the native tee is up.
    // The DIAGNOSTICS helper is stopped LATER on purpose — in the destroyOnce
    // continuation below, because its stop() drains the queue through a
    // promise chain and the teardown's closing records must be enqueued first.
    // The native writer itself is never stopped (process lifetime).
    stopSystemEvents();
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
    // N2 first batch C/D columns: the ring must outlive the WHOLE teardown, or
    // the closing lines are lost — `connectorStop` (the TUN gap), the destroy
    // itself and the descriptors it closes (the final `N2_SOCK` / `N2_FD_SEND`
    // lines of the session). The counters are therefore reported and the ring
    // disabled only AFTER the destroy settled; the FILE always stays for the
    // operator to fetch after returning to Wi-Fi.
    // 006 observational lifecycle probe (NO stop-marker semantics change): the
    // destroy leg is entered, and the EXISTING continuations report whether it
    // settled or failed. Only `phase`/`stage` of the already-whitelisted field
    // set leave here — no error summary, no "clean" marking (the marker is
    // still written by the late stopDiagnostics() only, after cleanup).
    const destroyRequestedFields: DiagFields = { 'phase': 'requested', 'stage': 'destroy' };
    this.diagMilestone('vpn_destroy_phase', destroyRequestedFields);
    let destroyFailed: boolean = false;
    this.destroyOnce('onDestroy').catch((error: Error) => {
      destroyFailed = true;
      const destroyFailedFields: DiagFields = { 'phase': 'failed', 'stage': 'destroy' };
      this.diagMilestone('vpn_destroy_phase', destroyFailedFields);
      hilog.error(DOMAIN, TAG,
        'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=unexpected|summary=%{public}s',
        this.requestId, safeError(error));
    }).then((): void => {
      if (!destroyFailed) {
        const destroySettledFields: DiagFields = { 'phase': 'settled', 'stage': 'destroy' };
        this.diagMilestone('vpn_destroy_phase', destroySettledFields);
      }
      try {
        core.n2_ring_status();
      } catch (error) { /* no ring configured: nothing to report */ }
      try {
        core.n2_ring_disable();
      } catch (error) { /* the file is already on disk */ }
      // 2026-10-08 LATE diagnostics stop — deliberately AFTER destroyOnce has
      // settled: every record the teardown enqueued (destroy, fd, connector
      // stop) is already in the queue, so stop() writes the clean marker and
      // drains them through its own promise chain. The native phone_log writer
      // is NOT shut down here: it keeps the process's log open until the OS
      // ends the extension process (a shutdown request would only race this
      // asynchronous drain and could lose the closing records).
      stopDiagnostics();
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

  // 2026-10-06 登录失败可见性: persist ONE connect-failure notice into the
  // shared filesDir (channel/shape: NetBirdConnector.CONNECT_FAILURE_FILE /
  // ConnectFailureNotice). Written BEFORE the teardown removes the live
  // status document (or when the document never existed — a startup abort).
  // Class tokens only, never a server message, never key material (the
  // class discipline of every other cross-process surface). Best-effort:
  // a failed write only costs the reason detail, the teardown continues.
  private writeConnectFailureForUi(errorClass: string): void {
    const path: string = `${this.context.filesDir}/${CONNECT_FAILURE_FILE_NAME}`;
    try {
      const token: string = clean(errorClass.length > 0 ? errorClass : 'none');
      // 2026-10-08: the ONE funnel every "the VPN never reached a live tunnel"
      // abort goes through (missing credentials, refused connector start, the
      // network-map box expiring without a map, a terminal connector death) —
      // so the persistent log carries the same class the UI notice carries,
      // next to the reason the core itself logged.
      const milestoneFields: DiagFields = { 'stage': 'connect-failure', 'reason': token };
      this.diagMilestone('vpn_start_failed', milestoneFields);
      const file: fs.File = fs.openSync(path,
        fs.OpenMode.READ_WRITE | fs.OpenMode.CREATE | fs.OpenMode.TRUNC);
      fs.writeSync(file.fd, `{"at_ms":${Date.now()},"error_class":"${token}"}`);
      fs.closeSync(file);
      hilog.warn(DOMAIN, TAG,
        'VPN_CONNECT_FAILURE_NOTICE|requestId=%{public}s|errorClass=%{public}s',
        this.requestId, token);
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'VPN_CONNECT_FAILURE_NOTICE_WRITE_FAILED|requestId=%{public}s|code=%{public}s',
        this.requestId, String((error as BusinessFailure).code ?? 'none'));
    }
  }

  // 2026-10-11 retry re-entry (fix/retry): the connector-run probe the onCreate
  // entry gate reads. `null` = the core did not answer (no connector object yet,
  // or a refused native call) — NOT proof of a running engine, so the gate lets
  // the entry through (NetBirdUiModel.connectEntryDecision). The wrapper already
  // answers null instead of throwing, so no VPN path can be lost to a probe.
  private connectorRunState(): boolean | null {
    const status: ConnectorStatusResult | null = pollStatusOrNull();
    if (status === null) {
      return null;
    }
    return status.running === true;
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
      // 2026-10-08: the teardown's own outcome, recorded from INSIDE
      // runDestroyOnce so it covers every trigger (onDestroy, connector death,
      // recreate fail-closed, invalid create) — and so it is enqueued before
      // onDestroy's late Diagnostics.stop() drains the queue.
      const milestoneFields: DiagFields = {
        'result': 'ok', 'stage': 'destroy', 'trigger': this.destroyTrigger
      };
      this.diagMilestone('vpn_destroy_done', milestoneFields);
    } catch (error) {
      const postDestroy: FdSnapshot = this.captureFdStatus();
      const marker: string = this.postDestroyMarker(postDestroy);
      this.logFdSnapshot('post-destroy-rejected', postDestroy, marker);
      hilog.error(DOMAIN, TAG,
        'VPN_DESTROY_REJECTED|requestId=%{public}s|phase=destroy|fdOpen=%{public}s|' +
        'fdMarker=%{public}s|summary=%{public}s',
        this.requestId, String(postDestroy.open), marker, safeError(error as Error));
      const milestoneFields: DiagFields = {
        'result': 'fail', 'stage': 'destroy', 'trigger': this.destroyTrigger
      };
      this.diagMilestone('vpn_destroy_done', milestoneFields);
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
