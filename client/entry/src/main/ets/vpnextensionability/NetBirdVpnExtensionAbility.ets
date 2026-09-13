import VpnExtensionAbility from '@ohos.app.ability.VpnExtensionAbility';
import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';
import vpnExtension from '@ohos.net.vpnExtension';
import core from 'libnetbird_core.so';
import { VpnRouteConfig, VpnTunnelConfig, buildPlatformVpnConfig, gateDefaultRoutes,
  loadVpnTunnelConfig, applyNetworkConfig } from './NetBirdVpnConfig';
import {
  CoreNetworkConfig, ConnectorStartResult, ConnectorStatusResult, DeviceConfigFile,
  DEFAULT_DEVICE_CONFIG_FILE, DEFAULT_SETUP_KEY_FILE, NETWORK_MAP_POLL_MS, NETWORK_MAP_WAIT_MS,
  STATUS_POLL_MS, buildConnectorConfigJson, connectorNetworkConfig, connectorStartWithSocket,
  connectorSocketFeed, connectorStop, mgmtSocketOpen, parseDeviceConfig, parseManagementEndpoint,
  pollStatusOrNull, readPrivateFileText, resolveManagementIp, sleep
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
// N3-7: bound for the shell-side DNS resolution of the management host
// (DNS must stay OUT of native so the protected dial never pulls nameserver
// traffic through our own tunnel).
const DNS_RESOLVE_BOX_MS: number = 5000;
// N3-7 gap 3: consecutive connector_status() native-call failures before the
// watcher concludes the connector is unreachable and tears the VPN down.
const CONNECTOR_POLL_FAILURE_LIMIT: number = 3;

interface BusinessFailure extends Error {
  code?: number;
}

interface CreateOutcome {
  destroyRequired: boolean;
  accepted: boolean;
  reason: string;
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
// - Start flow (N3-6 + N3-7): credentials read from app-private filesDir
//   files (DEVELOPMENT-TIME practice, NOT secure storage) -> shell-side DNS
//   of the management host -> core.mgmt_socket_open (TCP, unconnected) ->
//   fail-closed VpnConnection.protect(fd) gate -> core.connector_start_with_socket
//   -> bounded wait for core.connector_network_config -> merge
//   address/routes/DNS into VpnConfig under the default-route safety gate
//   (0.0.0.0/0 held unless the core gate allows / forceDefaultRoute opt-in)
//   -> connection.create() -> fd contract + fail-closed protect gate for the
//   WG outer socket. The map snapshot is applied at create() time (platform
//   VpnConfig is fixed then); later map changes are logged as explicitly NOT
//   applied.
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
  // connection.destroy(); the sole native call that receives it is the
  // read-only core.fd_status (F_GETFD) probe. Any future native consumer
  // must operate on a dup copy (no such export exists yet — TODO/未验证).
  private tunFd: number = -1;
  private fdConfirmedOpenAfterCreate: boolean = false;
  // Native-owned tunnel-outer UDP socket fd opened for the protect gate.
  // Native owns its lifecycle (no close export in core yet — TODO/未验证);
  // the shell only audits it read-only via core.fd_status.
  private outboundSocketFd: number = -1;
  private outboundProtected: boolean = false;
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

    let setupKey: string;
    let device: DeviceConfigFile;
    try {
      // DEVELOPMENT-TIME credential source, NOT secure storage (module doc
      // + docs/n3-shell-integration-notes.md): plain filesDir files.
      setupKey = readPrivateFileText(filesDir, setupKeyFile);
      device = parseDeviceConfig(readPrivateFileText(filesDir, deviceFile));
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
    // 1. shell-side endpoint parse + DNS resolution (never native)
    let connectAddr: string;
    try {
      const endpoint = parseManagementEndpoint(device.management_url);
      connectAddr = await resolveManagementIp(endpoint, DNS_RESOLVE_BOX_MS);
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

    const base: VpnTunnelConfig = loadVpnTunnelConfig();
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

    const config: vpnExtension.VpnConfig = buildPlatformVpnConfig(merged);
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
        hilog.info(DOMAIN, TAG,
          'VPN_CONNECTOR_STATUS|requestId=%{public}s|running=%{public}s|state=%{public}s|' +
          'peers=%{public}d|routes=%{public}d|reconnects=%{public}d|lastError=%{public}s|' +
          'terminal=%{public}s',
          this.requestId, String(status.running), status.state, status.peer_count,
          status.route_count, status.reconnects, errorClass, String(status.terminal));
        if (status.terminal === true) {
          this.handleConnectorDeath(`terminal-state:${status.state}`);
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
    // TODO(未验证): the native data plane that would now CONNECT through the
    // protected socket (WireGuard handshake towards the configured endpoint)
    // is not wired — it needs a dup-copy-taking core export (Rust side). The
    // protect gate itself is verified platform shape (e8e2cf9).
    // VPN is up (create accepted + protect passed): start the status /
    // network-config watch. Runtime map changes are logged as NOT applied
    // (platform VpnConfig fixed at create — see the watcher).
    this.startStatusAndNetcfgWatcher();
    return {
      destroyRequired: true,
      accepted: true,
      reason: 'create-open-fd-confirmed-protect-ok'
    };
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

  // Bounded VpnConnection.protect() box (same shape as the verified spike
  // helper protectSocketWithBox, e8e2cf9). Never throws. Box expiry is
  // reported as failure (fail-closed) — stricter than the spike's
  // best-effort treatment, per governance item 4.
  private async protectSocketWithBox(connection: vpnExtension.VpnConnection, fd: number): Promise<boolean> {
    const settled: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      connection.protect(fd).then((): void => {
        resolve(true);
      }).catch((error: Error): void => {
        hilog.warn(DOMAIN, TAG,
          'VPN_PROTECT_REJECTED|requestId=%{public}s|fd=%{public}d|summary=%{public}s',
          this.requestId, fd, safeError(error));
        resolve(false);
      });
    });
    const box: Promise<boolean> = new Promise<boolean>((resolve: (value: boolean) => void) => {
      setTimeout((): void => {
        hilog.warn(DOMAIN, TAG,
          'VPN_PROTECT_TIMEOUT|requestId=%{public}s|fd=%{public}d|boxMs=%{public}d',
          this.requestId, fd, PROTECT_BOX_MS);
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
