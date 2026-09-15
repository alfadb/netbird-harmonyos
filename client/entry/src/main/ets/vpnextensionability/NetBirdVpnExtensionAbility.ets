import VpnExtensionAbility from '@ohos.app.ability.VpnExtensionAbility';
import Want from '@ohos.app.ability.Want';
import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import vpnExtension from '@ohos.net.vpnExtension';
import core from 'libnetbird_core.so';
import { VpnRouteConfig, VpnTunnelConfig, buildPlatformVpnConfig, gateDefaultRoutes,
  loadVpnTunnelConfig, applyNetworkConfig, withEndpointExclusions } from './NetBirdVpnConfig';
import { RouteCheckResult, RouteViolation, formatRouteTable, formatRouteViolation,
  validateRouteEntries } from './NetBirdRouteValidation';
import { ExcludedRouteEntry, EndpointResolution, ExclusionBuildResult, ExclusionEndpoint,
  RelayEndpoint, buildExcludedRouteEntries, isExclusionFailClosed, parseRelayUrl
} from './NetBirdEndpointExclusion';
import {
  ConnectorStatusResult, CoreNetworkConfig, CoreRouteEntry, ConnectorStartResult,
  DeviceConfigFile, ManagementEndpoint, NativeTcpSocketOpen, RouteSetAppliedResult,
  SocketFeedResult, WgFeedResult,
  DEFAULT_DEVICE_CONFIG_FILE, DEFAULT_SETUP_KEY_FILE, NETWORK_MAP_POLL_MS, NETWORK_MAP_WAIT_MS,
  PACKAGED_CA_ROOTS_RAWFILE, STATUS_POLL_MS, buildConnectorConfigJson, connectorIceSocketFeed,
  connectorNetworkConfig, connectorRouteSetApplied, connectorSignalFeed, connectorStartWithSocket,
  connectorSocketFeed, connectorStatus, connectorStop, connectorTunFdFeed, connectorWgSocketFeed,
  countPemCertificates,
  iceSocketOpen, mgmtSocketOpen, parseDeviceConfig, parseManagementEndpoint, parseSignalEndpoint,
  pollStatusOrNull, readCredentialFileText, readPackagedCaRootsPem, resolveExclusionEndpointIp,
  resolveManagementIp, sleep
} from './NetBirdConnector';

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
// consumes fresh sockets again (the pool is one-shot). Top it up on a bounded
// cadence while the dev deviation is armed — but only while the core's queue
// is actually draining (the feed reports its depth): a core that is not
// gathering must not make the shell pile up fds (device run 3 reached
// queue=26 while nothing consumed them).
const ICE_SOCKET_TOPUP_MS: number = 4000;
const ICE_SOCKET_TOPUP_MAX: number = 2000;
// Keep a full pool queued, not a trickle: ONE gather round binds one socket
// per IPv4 interface and the round's checks take more, so a queue of 2-3
// drains mid-round and the core reports "protected-udp: no-protected-socket"
// while the shell still believes it is fine (device run 5, queue=3 > high).
const ICE_QUEUE_HIGH_WATER: number = 8;
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
//   (relay-enabled + unresolved endpoint = fail-closed, no VPN) -> merge
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
  // Native-owned tunnel-outer UDP socket fd, opened for the protect gate and
  // (N7) fed to the WG data plane after the protect gate passes. Native owns
  // its lifecycle (dup-consumed by the WG device; no close export in core
  // yet — TODO/未验证); the shell only audits it read-only via core.fd_status.
  private outboundSocketFd: number = -1;
  private outboundProtected: boolean = false;
  // N5c/N7 device gap: the ICE UDP socket (core-opened, shell-protected,
  // fed via connectorIceSocketFeed). Without it candidate gathering fails
  // closed and ICE never leaves idle (device run 1: 61x "pump error").
  private iceSocketFd: number = -1;
  private iceProtected: boolean = false;
  private iceTopUpTimer: number = -1;
  // Depth reported by the last accepted feed — the top-up uses it to tell a
  // draining core (keep feeding) from an idle one (stop piling up).
  private iceQueueDepth: number = 0;
  // Process-level protect (protectProcessNet, API 22+) — attempted once per
  // session; when it resolves it covers every socket of this process, which
  // is the whole outer-socket set (management/signal/ICE/WG).
  private processProtectAttempted: boolean = false;
  private processProtected: boolean = false;
  // DEV-ONLY (user-approved 2026-09-14): see onCreate — when armed, an
  // unavailable protect (MANAGE_VPN gated) is recorded as a deviation and the
  // flow continues on route exclusion. NEVER a gate pass.
  private protectDeviationRouteExclusion: boolean = false;
  // N3-6 control-plane wiring state. destroyRequested latches onDestroy so
  // the (async) start flow never creates a VPN after teardown began.
  private destroyRequested: boolean = false;
  private watcherTimer: number = -1;
  // Serial of the network-config snapshot merged into the platform
  // VpnConfig at create() time (-1 = none applied).
  private appliedNetSerial: number = -1;
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
  // N2-H: the endpoint-derived exclusion set frozen at THIS session's start
  // (before the first create()). The controlled recreate grafts the SAME
  // frozen set onto its rebuilt config — a recreate that dropped the
  // exclusions would suck the outer endpoints into the fresh (often
  // default-routed) tunnel. A fresh freeze (re-resolution) happens on the
  // next full start, never from a cached cross-session set.
  private frozenEndpointExclusions: ExcludedRouteEntry[] = [];

  onCreate(want: Want): void {
    const wantRequestId: string = String(want.parameters?.['requestId'] ?? '');
    if (wantRequestId.length > 0 && wantRequestId.length <= 96) {
      this.requestId = clean(wantRequestId);
    } else {
      this.requestId = 'missing';
    }
    hilog.info(DOMAIN, TAG, 'VPN_ONCREATE|bundle=%{public}s|requestId=%{public}s',
      want.bundleName ?? 'unknown', this.requestId);

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
    this.startConnectorThenCreate(want, connection).catch((error: Error) => {
      hilog.error(DOMAIN, TAG,
        'VPN_START_FLOW_REJECTED|requestId=%{public}s|summary=%{public}s',
        this.requestId, safeError(error));
    });
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
    // DEV-ONLY DEVIATION (user-approved 2026-09-14; T0 ruling in flight):
    // on this platform BOTH VpnConnection.protect(fd) AND protectProcessNet()
    // are gated internally by the restricted ohos.permission.MANAGE_VPN
    // (netmanager: "Caller is not allowed, need sys permissive"), the
    // permission is not appliable in the AGC ACL list, and merely DECLARING
    // it makes the install fail (9568289 "grant request permissions failed").
    // With this opt-in the shell treats protect as UNAVAILABLE and leans on
    // the VpnConfig isExcludedRoute entry that keeps the outer endpoints out
    // of the tunnel. It is loudly logged, recorded as a deviation and is
    // NEVER a gate pass; the default (flag absent) stays fail-closed.
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
        'reason=manage-vpn-permission-missing|is_evidence=false|not_a_gate_pass=true',
        this.requestId);
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
    const mgmtProtected: boolean = await this.protectSocketWithBox(connection, mgmtFd);
    if (!mgmtProtected) {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=mgmt-protect|' +
        'errorClass=protect-fail-closed|fd=%{public}d',
        this.requestId, mgmtFd);
      return;
    }

    // 4. start the connector over the protected socket (Rust dup-consumes
    // the fd per dial; the original number stays shell-owned — fd contract)
    const creds: Record<string, Object> = { 'setup_key': setupKey };
    const start: ConnectorStartResult =
      connectorStartWithSocket(mgmtFd, buildConnectorConfigJson(device, forceDefaultRoute),
        JSON.stringify(creds), connectAddr);
    if (!start.started) {
      hilog.error(DOMAIN, TAG,
        'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=start|errorClass=%{public}s',
        this.requestId, start.error ?? 'start-failed');
      return;
    }
    if (forceDefaultRoute) {
      hilog.warn(DOMAIN, TAG,
        'VPN_DEFAULT_ROUTE_FORCED|requestId=%{public}s|' +
        'warning=data-plane-not-ready-black-hole-risk|optIn=forceDefaultRoute',
        this.requestId);
    }
    // seed ONE spare protected socket so a single reconnect dial can
    // re-protect; further resupply policy is an N4+ increment (a starved
    // queue fails dials CLOSED — never unprotected)
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
      if (signalProvisioned === null) {
        hilog.error(DOMAIN, TAG,
          'VPN_CONNECTOR_ABORTED|requestId=%{public}s|stage=signal-provision|' +
          'errorClass=signal-provision-fail-closed|action=connector-stopped-no-vpn',
          this.requestId);
        connectorStop();
        return;
      }
    } else {
      hilog.warn(DOMAIN, TAG,
        'VPN_SIGNAL_NOT_PROVISIONED|requestId=%{public}s|reason=%{public}s',
        this.requestId, net === null ? 'no-network-map' : 'no-signal-uri-in-config');
    }

    // N2-H outer-endpoint freeze (docs/native-nx-governance.md 追加登记;
    // docs/n13-relay-increment-plan-20260914.md §3.4/§5): resolve the REAL
    // outer endpoints (management / signal / relay) and derive the host
    // /32|/128 exclusion routes BEFORE create() — the platform VpnConfig
    // routes are fixed at create time, and this deployment leans on route
    // exclusion instead of the (unavailable) protect path. Relay-enabled
    // deployments fail CLOSED here if any endpoint cannot be frozen.
    const baseTemplate: VpnTunnelConfig = loadVpnTunnelConfig();
    const exclusion: EndpointExclusionOutcome = await this.deriveEndpointExclusions(
      mgmtEndpoint, connectAddr, signalProvisioned, baseTemplate.tunnelAddress);
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
    merged = gateDefaultRoutes(merged, gateAllowed);
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
    this.logRouteTable(merged);
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

    const config: vpnExtension.VpnConfig = buildPlatformVpnConfig(merged);
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
      return this.settleCreate(fd);
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
    const protectedOk: boolean = await this.protectSocketWithBox(connection, fd);
    if (!protectedOk) {
      hilog.error(DOMAIN, TAG,
        'VPN_SIGNAL_PROVISION_FAIL|requestId=%{public}s|stage=protect|fd=%{public}d|' +
        'errorClass=protect-fail-closed', this.requestId, fd);
      return null;
    }
    // 4. feed the protected socket + the resolved address to the core
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
  //   - relay: connector_status().relay.urls (N13-D1), `rel://`/`rels://`
  //     parsed per the core's own RelayUrl::parse contract, then resolved
  //     FRESH at config-build time — DDNS (relay TTL 600) must never freeze a
  //     stale IP.
  // Additionally every endpoint host gets a FRESH build-time resolution so a
  // DDNS flip inside this start flow is covered too (dial IP + fresh IP both
  // frozen; pure dedup collapses matches).
  //
  // Severity (isExclusionFailClosed): relay in the required freeze set + any
  // resolution failure = fatal (fail-closed, no VPN); without relay a failed
  // host is skipped but logged with its reason. Management/signal dial IPs
  // are always frozen regardless (they are known-good: the flow aborts
  // earlier when those resolutions fail).
  //
  // Log discipline: hosts/ports/counts only — no tokens, no resolved IPs.
  private async deriveEndpointExclusions(mgmt: ManagementEndpoint, mgmtDialIp: string,
    signal: SignalProvisionResult | null, gateway: string): Promise<EndpointExclusionOutcome> {
    // relay state: the connector status is the only source (relay_enabled
    // rides the core config; the url set arrives with NetbirdConfig.relay).
    let relayUrls: string[] = [];
    try {
      const status: ConnectorStatusResult = connectorStatus();
      if (status.relay !== undefined && status.relay !== null &&
        status.relay.urls !== undefined && status.relay.urls !== null) {
        relayUrls = status.relay.urls;
      }
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'VPN_ENDPOINT_EXCLUSION_RELAY_STATE_UNKNOWN|requestId=%{public}s|' +
        'summary=%{public}s|treatment=relay-not-required',
        this.requestId, safeError(error as Error));
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
    for (let i = 0; i < relayUrls.length; i++) {
      const parsed: RelayEndpoint | null = parseRelayUrl(relayUrls[i]);
      if (parsed === null) {
        // The core would refuse this url typed as well — an unparsable relay
        // url is an unfreezable endpoint: record as failed resolution.
        resolutions.push({ kind: 'relay', host: 'unparsable-relay-url', ok: false, address: '' });
        hilog.error(DOMAIN, TAG,
          'VPN_ENDPOINT_EXCLUSION_URL_MALFORMED|requestId=%{public}s|index=%{public}d|' +
          'schemes=rel|rels',
          this.requestId, i);
        continue;
      }
      endpoints.push({ kind: 'relay', host: parsed.host, port: parsed.port });
    }

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
      'duplicates=%{public}d|failed=%{public}d|routes=%{public}d|relayRequired=%{public}s',
      this.requestId, endpoints.length, build.resolvedCount, build.duplicatesRemoved,
      build.failed.length, build.routes.length, String(relayRequired));

    const fatal: boolean = isExclusionFailClosed(build, relayRequired);
    if (fatal) {
      hilog.error(DOMAIN, TAG,
        'VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED|requestId=%{public}s|' +
        'reason=relay-enabled-unresolved-endpoint|failedHosts=%{public}d|' +
        'action=no-vpn-created|is_evidence=false|not_a_gate_pass=true',
        this.requestId, build.failed.length);
    }
    return { fatal: fatal, routes: build.routes };
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
        if (serial >= 0 && serial !== this.appliedNetSerial) {
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
      const base: VpnTunnelConfig = withEndpointExclusions(loadVpnTunnelConfig(),
        this.frozenEndpointExclusions);
      let merged: VpnTunnelConfig = applyNetworkConfig(base, net);
      const gateAllowed: boolean = net.default_route !== undefined && net.default_route !== null
        ? net.default_route.allowed
        : false;
      merged = gateDefaultRoutes(merged, gateAllowed);
      // H1 fail-closed gate on the RECREATE leg too: a snapshot entry that
      // violates the SDK RouteInfo contract must never reach create().
      // Failing BEFORE the destroy leg keeps the current generation intact
      // (same early-throw discipline as the snapshot preconditions above).
      this.logRouteTable(merged);
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
      const config: vpnExtension.VpnConfig = buildPlatformVpnConfig(merged);
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

      // 5. feed the data plane: the protected outer socket FIRST (SAME
      // native socket — an idempotent no-op while the device is up; a fresh
      // pair if the device somehow went down), then the NEW TUN fd (the
      // core replaces its TUN in place and keeps the WG sessions).
      stage = 'wg-feed';
      if (!this.feedWgDataplane()) {
        throw new Error('recreate-wg-feed-failed');
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

  // N3-7: seed ONE spare protected management socket into the core queue so
  // a single reconnect dial can re-protect without new shell involvement.
  // Best-effort: failures are logged, never fatal (a starved queue fails
  // dials CLOSED — never unprotected); systematic resupply is N4+ work.
  private feedSpareProtectedSocket(connection: vpnExtension.VpnConnection): void {
    let fd: number = -1;
    try {
      const opened = mgmtSocketOpen();
      fd = opened.fd;
      if (fd < 0 || opened.bind_rc !== 0) {
        hilog.warn(DOMAIN, TAG,
          'VPN_MGMT_SOCKET_FEED_SKIPPED|requestId=%{public}s|stage=open|fd=%{public}d',
          this.requestId, fd);
        return;
      }
    } catch (error) {
      hilog.warn(DOMAIN, TAG,
        'VPN_MGMT_SOCKET_FEED_SKIPPED|requestId=%{public}s|stage=open-throw|summary=%{public}s',
        this.requestId, safeError(error as Error));
      return;
    }
    this.protectSocketWithBox(connection, fd).then((ok: boolean): void => {
      if (!ok) {
        hilog.warn(DOMAIN, TAG,
          'VPN_MGMT_SOCKET_FEED_SKIPPED|requestId=%{public}s|stage=protect|fd=%{public}d',
          this.requestId, fd);
        return;
      }
      const fed = connectorSocketFeed(fd);
      hilog.info(DOMAIN, TAG,
        'VPN_MGMT_SOCKET_FED|requestId=%{public}s|fd=%{public}d|ok=%{public}s|queued=%{public}s|' +
        'error=%{public}s',
        this.requestId, fd, String(fed.ok), String(fed.queued ?? -1), fed.error ?? 'none');
    });
  }

  private stopStatusAndNetcfgWatcher(): void {
    if (this.watcherTimer >= 0) {
      clearInterval(this.watcherTimer);
      this.watcherTimer = -1;
    }
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

  // H1 pre-create truth dump (diagnosis §5 T0 / §6-4): every route entry
  // with its per-field runtime truth, BEFORE connection.create() — one
  // hilog line each (header + one per entry). Field names and route values
  // only; never tokens or credentials.
  private logRouteTable(config: VpnTunnelConfig): void {
    const lines: string[] = formatRouteTable(this.requestId, config.interfaceName, config.routes);
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
  // (governance item 4).
  private async settleCreate(fd: number): Promise<CreateOutcome> {
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
    if (!accepted) {
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
  // feeds. Both numbers cross as BORROWED fd numbers — native dups them
  // (F_DUPFD_CLOEXEC) at device construction and never uses/closes the
  // originals: the raw TUN fd keeps belonging to connection.destroy(), the
  // raw WG socket stays native-opened for audits. Returns true only when the
  // core reports device_up (both feeds landed and the device is pumping).
  private feedWgDataplane(): boolean {
    if (this.outboundSocketFd < 0 || !this.outboundProtected || this.tunFd < 0) {
      hilog.error(DOMAIN, TAG,
        'VPN_WG_FEED_FAIL|requestId=%{public}s|stage=precondition|wgFd=%{public}d|' +
        'protected=%{public}s|tunFd=%{public}d',
        this.requestId, this.outboundSocketFd, String(this.outboundProtected), this.tunFd);
      return false;
    }
    // 1. the protected WG outer socket (data plane feed #1)
    let sock: WgFeedResult;
    try {
      sock = connectorWgSocketFeed(this.outboundSocketFd);
    } catch (error) {
      hilog.error(DOMAIN, TAG,
        'VPN_WG_FEED_FAIL|requestId=%{public}s|stage=wg-socket-throw|fd=%{public}d|summary=%{public}s',
        this.requestId, this.outboundSocketFd, safeError(error as Error));
      return false;
    }
    if (!sock.ok) {
      hilog.error(DOMAIN, TAG,
        'VPN_WG_FEED_FAIL|requestId=%{public}s|stage=wg-socket|fd=%{public}d|error=%{public}s',
        this.requestId, this.outboundSocketFd, sock.error ?? 'no-message');
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
        this.requestId, this.outboundSocketFd, this.tunFd);
      return false;
    }
    hilog.info(DOMAIN, TAG,
      'VPN_WG_DATAPLANE_FED|requestId=%{public}s|wgFd=%{public}d|tunFd=%{public}d|deviceUp=true',
      this.requestId, this.outboundSocketFd, this.tunFd);
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
      hilog.error(DOMAIN, TAG,
        'VPN_PROTECT_FAIL_CLOSED|requestId=%{public}s|stage=outbound-open|fd=%{public}d|' +
        'bindRc=%{public}d|bindErrno=%{public}d',
        this.requestId, fd, bindRc, bindErrno);
      return false;
    }
    this.outboundSocketFd = fd;
    hilog.info(DOMAIN, TAG,
      'VPN_PROTECT_BEGIN|requestId=%{public}s|fd=%{public}d', this.requestId, fd);

    const protectedOk: boolean = await this.protectSocketWithBox(connection, fd);
    if (!protectedOk) {
      hilog.error(DOMAIN, TAG,
        'VPN_PROTECT_FAIL_CLOSED|requestId=%{public}s|stage=protect|fd=%{public}d', this.requestId, fd);
      return false;
    }
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
    this.startIceSocketTopUp(connection);
    return true;
  }

  // Bounded top-up of the ICE socket pool: a failed session is renegotiated
  // after a cooldown and needs fresh sockets again (device run 3: the retry
  // hit "no-protected-socket" forever because the pool was one-shot).
  private startIceSocketTopUp(connection: vpnExtension.VpnConnection): void {
    if (this.iceTopUpTimer !== -1) {
      return;
    }
    let given: number = 0;
    this.iceTopUpTimer = setInterval((): void => {
      if (given >= ICE_SOCKET_TOPUP_MAX || this.destroyRequested) {
        clearInterval(this.iceTopUpTimer);
        this.iceTopUpTimer = -1;
        hilog.warn(DOMAIN, TAG,
          'VPN_ICE_TOPUP|stopped|given=%{public}d|max=%{public}d', given, ICE_SOCKET_TOPUP_MAX);
        return;
      }
      this.topUpIceBatch(connection).then((fed: number): void => {
        given += fed;
        if (fed === 0 && this.iceQueueDepth > ICE_QUEUE_HIGH_WATER) {
          hilog.warn(DOMAIN, TAG,
            'VPN_ICE_TOPUP|satisfied|queue=%{public}d|given=%{public}d',
            this.iceQueueDepth, given);
        } else if (fed === 0) {
          hilog.warn(DOMAIN, TAG, 'VPN_ICE_TOPUP|feed-failed|given=%{public}d', given);
        }
      }).catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG, 'VPN_ICE_TOPUP|rejected|summary=%{public}s', safeError(error));
      });
    }, ICE_SOCKET_TOPUP_MS);
  }

  // Feed up to ICE_TOPUP_BATCH sockets while the live queue depth is at or
  // below the high-water mark (one gather round needs one socket per IPv4
  // interface, so a trickle starves it mid-round).
  private async topUpIceBatch(connection: vpnExtension.VpnConnection): Promise<number> {
    let fed: number = 0;
    for (let n = 0; n < ICE_TOPUP_BATCH && this.iceQueueDepth <= ICE_QUEUE_HIGH_WATER; n++) {
      const ok: boolean = await this.feedOneIceSocket(connection);
      if (!ok) {
        break;
      }
      fed++;
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
    if (this.iceSocketFd < 0) {
      this.iceSocketFd = fd;
    }
    const protectedOk: boolean = await this.protectSocketWithBox(connection, fd);
    if (!protectedOk) {
      hilog.error(DOMAIN, TAG,
        'VPN_ICE_FEED_FAIL|requestId=%{public}s|stage=protect|fd=%{public}d', this.requestId, fd);
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
  }

  // Bounded VpnConnection.protect() box (same shape as the verified spike
  // helper protectSocketWithBox, e8e2cf9). Never throws. Box expiry is
  // reported as failure (fail-closed) — stricter than the spike's
  // best-effort treatment, per governance item 4.
  private async protectSocketWithBox(connection: vpnExtension.VpnConnection, fd: number): Promise<boolean> {
    const t0: number = Date.now();
    // Device-verified 2026-09-14: on this platform a per-fd protect() whose
    // internal ohos.permission.MANAGE_VPN check fails WEDGES the ArkTS thread
    // outright — observed twice: the extension's main thread went permanently
    // silent right after the call (not even the box below could resume, so the
    // fail-closed teardown never ran either). protectProcessNet() was tried as
    // a public alternative and also never settles.
    //
    // Therefore, when the user-approved dev opt-in is armed, the platform call
    // is NOT attempted at all — "treat protect as unavailable" is precisely
    // what the deviation says, and calling it would only wedge the shell. The
    // deviation is recorded per socket; it is never a gate pass.
    if (this.protectDeviationRouteExclusion) {
      hilog.warn(DOMAIN, TAG,
        'VPN_PROTECT_DEVIATION|requestId=%{public}s|fd=%{public}d|mode=route-exclusion|' +
        'reason=manage-vpn-permission-missing|platform_call=not-attempted|' +
        'is_evidence=false|not_a_gate_pass=true',
        this.requestId, fd);
      return true;
    }
    const settled: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      connection.protect(fd).then((): void => {
        hilog.info(DOMAIN, TAG,
          'VPN_PROTECT_SETTLED|requestId=%{public}s|fd=%{public}d|elapsedMs=%{public}d',
          this.requestId, fd, Date.now() - t0);
        resolve(true);
      }).catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG,
          'VPN_PROTECT_REJECTED|requestId=%{public}s|fd=%{public}d|elapsedMs=%{public}d|summary=%{public}s',
          this.requestId, fd, Date.now() - t0, safeError(error));
        resolve(false);
      });
    });
    const box: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      setTimeout((): void => {
        hilog.warn(DOMAIN, TAG,
          'VPN_PROTECT_TIMEOUT|requestId=%{public}s|fd=%{public}d|boxMs=%{public}d|stillPending=true',
          this.requestId, fd, PROTECT_BOX_MS);
        resolve(false);
      }, PROTECT_BOX_MS);
    });
    const outcome: boolean = await Promise.race([settled, box]);
    if (!outcome && this.protectDeviationRouteExclusion) {
      hilog.warn(DOMAIN, TAG,
        'VPN_PROTECT_DEVIATION|requestId=%{public}s|fd=%{public}d|mode=route-exclusion|' +
        'reason=manage-vpn-permission-missing|is_evidence=false|not_a_gate_pass=true',
        this.requestId, fd);
      return true;
    }
    return outcome;
  }

  // Bounded process-level protect (protectProcessNet). Same house rule: every
  // ArkTS wait is bounded; expiry = failure and the per-fd path takes over.
  private async protectProcessNetWithBox(connection: vpnExtension.VpnConnection): Promise<boolean> {
    const t0: number = Date.now();
    const settled: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      try {
        connection.protectProcessNet().then((): void => {
          hilog.info(DOMAIN, TAG,
            'VPN_PROTECT_PROCESS_NET|requestId=%{public}s|result=resolved|elapsedMs=%{public}d',
            this.requestId, Date.now() - t0);
          resolve(true);
        }).catch((error: Error): void => {
          hilog.warn(DOMAIN, TAG,
            'VPN_PROTECT_PROCESS_NET|requestId=%{public}s|result=rejected|elapsedMs=%{public}d|summary=%{public}s',
            this.requestId, Date.now() - t0, safeError(error));
          resolve(false);
        });
      } catch (error) {
        hilog.warn(DOMAIN, TAG,
          'VPN_PROTECT_PROCESS_NET|requestId=%{public}s|result=throw|summary=%{public}s',
          this.requestId, safeError(error as Error));
        resolve(false);
      }
    });
    const box: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      setTimeout((): void => {
        hilog.warn(DOMAIN, TAG,
          'VPN_PROTECT_PROCESS_NET|requestId=%{public}s|result=timeout|boxMs=%{public}d',
          this.requestId, PROTECT_BOX_MS);
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

  private destroyOnce(trigger: string): Promise<void> {
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

  // Post-destroy lifecycle audit of the protected tunnel-outer socket. The
  // fd is native-owned; core has no close export yet (TODO/未验证, Rust side
  // in flight), so the shell only verifies its state read-only and reports.
  private auditOutboundSocketAfterDestroy(): void {
    const fd: number = this.outboundSocketFd;
    if (fd < 0) {
      hilog.info(DOMAIN, TAG,
        'VPN_OUTBOUND_AUDIT|requestId=%{public}s|socket=never-opened', this.requestId);
      return;
    }
    const snapshot: FdSnapshot = this.captureFdSnapshot(fd);
    hilog.info(DOMAIN, TAG,
      'VPN_OUTBOUND_AUDIT|requestId=%{public}s|fd=%{public}d|protected=%{public}s|open=%{public}s|' +
      'errno=%{public}d|owner=native-close-export-pending',
      this.requestId, fd, String(this.outboundProtected), String(snapshot.open), snapshot.errno);
  }
}
