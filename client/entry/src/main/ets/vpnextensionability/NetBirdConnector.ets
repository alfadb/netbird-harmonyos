import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import connection from '@ohos.net.connection';
import core from 'libnetbird_core.so';

// N3-6: ArkTS-side wrapper around the synchronous connector_* NAPI exports
// of libnetbird_core.so (JSON string in / JSON string out; no callbacks).
// NAPI contract: client/core/src/lib.rs NAPI surface table +
// client/core/src/napi.rs / connector.rs (authoritative field names).
// N3-7: protected management socket wiring (mgmt_socket_open /
// connector_start_with_socket / connector_socket_feed) and the connector
// `terminal` status flag — see docs/n3-shell-integration-notes.md N3-7.
//
// CREDENTIAL SOURCE — DEVELOPMENT-TIME PRACTICE, NOT SECURE STORAGE:
// the setup key and the device identity (management URL + device private
// key) are read from plain files under the app private directory (filesDir).
// They are unencrypted, unprivileged app-sandbox files; anyone with the
// device (or an app-sandbox compromise) can read them. This exists ONLY to
// keep secrets out of source code during development. HarmonyOS secure
// storage (asset store / keystore) integration is future work — see
// docs/n3-shell-integration-notes.md. File names are configurable per
// request via the want parameters 'setupKeyFile' / 'deviceConfigFile'
// (resolved under filesDir; no absolute-path escape is honored here).

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';

// Default file names under filesDir.
export const DEFAULT_SETUP_KEY_FILE: string = 'netbird-setup-key';
export const DEFAULT_DEVICE_CONFIG_FILE: string = 'netbird-device-config.json';

// Bounds for the network-map wait (house rule: every ArkTS-side wait is
// bounded). The control plane needs management to answer Login+Sync before
// the map exists; if it does not within the box, the shell starts the VPN
// with the local fallback config and marks the snapshot NOT applied.
export const NETWORK_MAP_WAIT_MS: number = 15000;
export const NETWORK_MAP_POLL_MS: number = 500;
// Runtime watch interval once the VPN is up.
export const STATUS_POLL_MS: number = 5000;

// {started,state} / {started:false,error} — core.connector_start.
export interface ConnectorStartResult {
  started: boolean;
  state?: string;
  error?: string;
}

// {ok,already_stopped,state} — core.connector_stop (idempotent).
export interface ConnectorStopResult {
  ok: boolean;
  already_stopped: boolean;
  state: string;
}

// {open,errno} shape of core.fd_status.
export interface FdStatusNative {
  open: boolean;
  errno: number;
}

// {fd,bind_rc,bind_errno} JSON returned by the synchronous core.mgmt_socket_open()
// NAPI export (N3-7): native opens + binds a TCP management socket WITHOUT
// connecting so the shell can VpnConnection.protect(fd) BEFORE any packet
// flows (wg_fwd_open split pattern, TCP variant).
export interface NativeTcpSocketOpen {
  fd: number;
  bind_rc: number;
  bind_errno: number;
}

// core.connector_socket_feed(fd) result (N3-7 shell-side resupply of fresh
// protected sockets for reconnect dials).
export interface SocketFeedResult {
  ok: boolean;
  queued?: number;
  error?: string;
}

// Error classification only — never a server/transport message
// (connector credential discipline).
export interface ConnectorErrorClass {
  class: string;
  status: number;
}

// core.connector_status snapshot (subset the shell consumes).
export interface ConnectorStatusResult {
  running: boolean;
  state: string;
  peer_count: number;
  route_count: number;
  reconnects: number;
  last_error: ConnectorErrorClass | null;
  session_expiry: string;
  wg_apply_failed: boolean;
  wg_apply_errors: number;
  // N3-7: worker ENDED BY ITSELF (fatal / retry budget exhausted). This is
  // the signal the shell watcher tears the VPN down on (gap 3).
  terminal: boolean;
}

// One managed route from core.connector_network_config (canonical masked
// network, e.g. '172.16.0.0/12'; is_default marks 0.0.0.0/0).
export interface CoreRouteEntry {
  network: string;
  is_default: boolean;
}

// One resolver (ip as sent — may be any family; port for visibility only:
// VpnConfig.dnsAddresses consumes bare IPs).
export interface CoreDnsServer {
  ip: string;
  port: number;
}

export interface CoreDnsConfig {
  service_enable: boolean;
  servers: CoreDnsServer[];
}

export interface CorePeerSummary {
  pub_key: string;
  allowed_ips: number;
}

// N3-7 default-route safety gate verdict on the snapshot: 0.0.0.0/0 is
// exported in `routes` ONLY when allowed; a held route carries a
// `default-route-held:*` reason token (and is absent from `routes`).
export interface CoreDefaultRouteGate {
  allowed: boolean;
  reason: string;
}

// core.connector_network_config snapshot. available=false carries only the
// reason token ('no-network-map').
export interface CoreNetworkConfig {
  available: boolean;
  reason?: string;
  serial?: number;
  address?: string | null;
  address_prefix_len?: number | null;
  interface_dns?: string | null;
  routes?: CoreRouteEntry[];
  dns?: CoreDnsConfig | null;
  peer_count?: number;
  peers?: CorePeerSummary[];
  default_route?: CoreDefaultRouteGate;
}

// netbird-device-config.json shape (app-private dir, development-time —
// NOT secure storage). The device private key is SECRET material: it must
// never be logged nor embedded in source.
export interface DeviceConfigFile {
  management_url: string;
  private_key: string;
  ca_pem?: string;
  server_name?: string;
  hostname?: string;
}

// Read one UTF-8 text file from the app private directory. Throws on
// missing/empty file (caller maps it to an explicit error class).
export function readPrivateFileText(filesDir: string, fileName: string): string {
  // name-only resolution under filesDir (want parameters may override the
  // name, never the directory)
  const path: string = `${filesDir}/${fileName}`;
  if (!fs.accessSync(path)) {
    throw new Error(`missing file under filesDir: ${fileName}`);
  }
  const text: string = fs.readTextSync(path);
  if (text.length === 0) {
    throw new Error(`empty file under filesDir: ${fileName}`);
  }
  return text.trim();
}

// Parse + minimally validate netbird-device-config.json.
export function parseDeviceConfig(text: string): DeviceConfigFile {
  const device = JSON.parse(text) as DeviceConfigFile;
  if (device.management_url === undefined || device.management_url.length === 0) {
    throw new Error('device config: management_url missing');
  }
  if (device.private_key === undefined || device.private_key.length === 0) {
    throw new Error('device config: private_key missing');
  }
  return device;
}

// Build the core.connector_start / connector_start_with_socket configJson
// (contract: ConnectorConfig::from_json in client/core/src/connector.rs).
// The private key crosses this boundary as config material — never logged,
// never echoed. `forceDefaultRoute` is the N3-7 DEVELOPMENT opt-in that
// overrides the default-route safety gate (data-plane-not-ready = traffic
// black hole); it must stay false in production wiring.
export function buildConnectorConfigJson(device: DeviceConfigFile,
  forceDefaultRoute: boolean): string {
  const cfg: Record<string, Object> = {
    'management_url': device.management_url,
    'private_key': device.private_key,
    'hostname': device.hostname !== undefined ? device.hostname : 'netbird-ohos',
    'os_name': 'harmonyos',
    'os_version': 'unknown',
    'netbird_version': '0.1.0'
  };
  if (device.ca_pem !== undefined && device.ca_pem.length > 0) {
    cfg['ca_pem'] = device.ca_pem;
  }
  if (device.server_name !== undefined && device.server_name.length > 0) {
    cfg['server_name'] = device.server_name;
  }
  if (forceDefaultRoute) {
    cfg['force_default_route'] = true;
  }
  return JSON.stringify(cfg);
}

export function connectorStart(configJson: string, setupKeyJson: string): ConnectorStartResult {
  return JSON.parse(core.connector_start(configJson, setupKeyJson)) as ConnectorStartResult;
}

// N3-7 production start: the management socket fd (opened by core.mgmt_socket_open
// and ALREADY VpnConnection.protect'ed by the shell) plus the shell-resolved
// management address. Fail-closed: any refusal keeps the VPN un-created.
export function connectorStartWithSocket(fd: number, configJson: string,
  setupKeyJson: string, connectAddr: string): ConnectorStartResult {
  const addrJson: Record<string, Object> = { 'connect_addr': connectAddr };
  return JSON.parse(core.connector_start_with_socket(fd, configJson, setupKeyJson,
    JSON.stringify(addrJson))) as ConnectorStartResult;
}

// N3-7: feed a fresh protected socket (open + protect first) so reconnect
// dials can re-protect. Fail-closed without resupply: dials error, they
// never fall back to an unprotected direct connection.
export function connectorSocketFeed(fd: number): SocketFeedResult {
  return JSON.parse(core.connector_socket_feed(fd)) as SocketFeedResult;
}

// N3-7: open + bind (NO connect) a TCP management socket for the protect
// gate. fd stays native-created; the shell protects it, then hands the SAME
// number back via connectorStartWithSocket/connectorSocketFeed.
export function mgmtSocketOpen(): NativeTcpSocketOpen {
  return JSON.parse(core.mgmt_socket_open()) as NativeTcpSocketOpen;
}

// ---------------------------------------------------------------------------
// N3-7 shell-side management endpoint resolution (DNS stays OUT of native —
// the protected dial must never trigger a nameserver trip through our own
// tunnel; governance §二.4 / notes N3-7)
// ---------------------------------------------------------------------------

// management_url split into host + port; `ip` is non-empty only when the
// host is already an IPv4 literal (no DNS needed).
export interface ManagementEndpoint {
  host: string;
  port: number;
  ip: string;
}

// Parse `https://host[:port]` / `http://host[:port]` (no path allowed past
// the authority for this client's management_url). Throws a stable error
// text on malformed input — the caller aborts the start (fail-closed).
export function parseManagementEndpoint(url: string): ManagementEndpoint {
  const schemeSep: number = url.indexOf('://');
  if (schemeSep <= 0) {
    throw new Error('management-url-malformed');
  }
  const scheme: string = url.slice(0, schemeSep);
  if (scheme !== 'https' && scheme !== 'http') {
    throw new Error('management-url-malformed');
  }
  let authority: string = url.slice(schemeSep + 3);
  const slash: number = authority.indexOf('/');
  if (slash >= 0) {
    if (slash !== authority.length - 1) {
      throw new Error('management-url-malformed');
    }
    authority = authority.slice(0, slash);
  }
  if (authority.length === 0) {
    throw new Error('management-url-malformed');
  }
  const colon: number = authority.lastIndexOf(':');
  let host: string = authority;
  let port: number = scheme === 'https' ? 443 : 80;
  if (colon >= 0) {
    host = authority.slice(0, colon);
    port = Number(authority.slice(colon + 1));
    if (!Number.isInteger(port) || port <= 0 || port > 65535) {
      throw new Error('management-url-malformed');
    }
  }
  // bare IPv4 literal → no DNS leg at all
  const octets: string[] = host.split('.');
  const isV4Literal: boolean = octets.length === 4 &&
    octets.every((o: string): boolean => {
      const n: number = Number(o);
      return o.length > 0 && Number.isInteger(n) && n >= 0 && n <= 255;
    });
  return { host: host, port: port, ip: isV4Literal ? host : '' };
}

// Shell-side DNS over the DEFAULT network, bounded by `timeoutMs` (house
// rule: every ArkTS-side wait is bounded). Returns 'ip:port' for the
// management dial. Resolution failure throws 'dns-failed' — the caller must
// NOT start the connector (fail-closed: an unresolved host would make native
// fall back to its own resolution, i.e. un-protected DNS through the tunnel).
export async function resolveManagementIp(endpoint: ManagementEndpoint,
  timeoutMs: number): Promise<string> {
  if (endpoint.ip.length > 0) {
    return `${endpoint.ip}:${endpoint.port}`;
  }
  const resolution: Promise<string> = connection.getAddressesByName(endpoint.host)
    .then((addresses: connection.NetAddress[]): string => {
      for (let i = 0; i < addresses.length; i++) {
        // prefer IPv4 (this client has no IPv6 data plane yet)
        if (addresses[i].address.indexOf(':') < 0) {
          return `${addresses[i].address}:${endpoint.port}`;
        }
      }
      throw new Error('dns-failed');
    });
  const box: Promise<string> = new Promise<string>((resolve: (value: string) => void,
    reject: (e: Error) => void) => {
    setTimeout((): void => {
      reject(new Error('dns-failed'));
    }, timeoutMs);
  });
  return Promise.race([resolution, box]);
}

export function connectorStatus(): ConnectorStatusResult {
  return JSON.parse(core.connector_status()) as ConnectorStatusResult;
}

export function connectorNetworkConfig(): CoreNetworkConfig {
  return JSON.parse(core.connector_network_config()) as CoreNetworkConfig;
}

export function connectorStop(): ConnectorStopResult {
  return JSON.parse(core.connector_stop()) as ConnectorStopResult;
}

export function sleep(ms: number): Promise<void> {
  return new Promise<void>((resolve: () => void) => {
    setTimeout(resolve, ms);
  });
}

// One status poll, exceptions mapped to null (UI/timer callers poll again).
export function pollStatusOrNull(): ConnectorStatusResult | null {
  try {
    return connectorStatus();
  } catch (error) {
    hilog.error(DOMAIN, TAG, 'CONNECTOR_STATUS_POLL_REJECTED|summary=%{public}s',
      (error as Error).message ?? 'no-message');
    return null;
  }
}
