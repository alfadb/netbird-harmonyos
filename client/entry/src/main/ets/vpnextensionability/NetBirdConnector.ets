import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import connection from '@ohos.net.connection';
import resourceManager from '@ohos.resourceManager';
import util from '@ohos.util';
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
//
// DEVELOPMENT-ONLY OVERRIDE (2026-09-13, device-validation aid): hdc can
// only push files to /data/local/tmp, not into the app sandbox. When a
// file with the SAME NAME exists under DEV_CREDENTIAL_DIR, it is read
// INSTEAD of the filesDir copy (readCredentialFileText). That directory is
// a world-readable device scratch path — never secure storage, never for
// production. The fail-closed invariant is unchanged: credentials found in
// NEITHER location still abort the start (stage=credentials, no VPN).

const DOMAIN: number = 0x2900;
const TAG: string = 'NetBirdVpn';

// Default file names under filesDir.
export const DEFAULT_SETUP_KEY_FILE: string = 'netbird-setup-key';
export const DEFAULT_DEVICE_CONFIG_FILE: string = 'netbird-device-config.json';
// Development-time credential override directory (see the DEVELOPMENT-ONLY
// OVERRIDE note above). Device-local absolute path, hdc-writable.
export const DEV_CREDENTIAL_DIR: string = '/data/local/tmp';

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

// N5d: core.connector_status().signal — real signal link state
// (registered / reconnects / class-only last error).
export interface SignalStatus {
  registered: boolean;
  reconnects: number;
  last_error: ConnectorErrorClass | null;
}

// N7: core.connector_status().wg — real WireGuard data-plane status of the
// shell-fed device seam (counters only, no key material).
export interface WgDataplaneStatus {
  fed_socket: boolean;
  fed_tun: boolean;
  device_up: boolean;
  ready: boolean;
  peers_with_session: number;
  handshakes: number;
  tx_packets: number;
  rx_packets: number;
  // N2-H byte-reconciliation counters (device path): rx_bytes_to_tun must
  // equal the TUN side's delivered bytes over the same window.
  tx_bytes?: number;
  rx_bytes_to_tun?: number;
  dropped_no_route: number;
  decrypt_errors: number;
}

// N8: core.connector_status().recreate — controlled-recreate state. The
// platform VpnConfig is fixed at create() time; when the DESIRED route set
// (live default-route gate applied to the map's routes) diverges from the
// route set the shell last ACKed as applied, `required=true` tells the shell
// to rebuild the connection. `count` = completed recreates (budget 3
// core-side, RECREATE_MAX), `exhausted` = budget spent (never re-requested),
// `cooling_down` = a divergence exists but is parked inside the 30 s
// cooldown window, `reason` = stable token
// ('none' | 'route-set-changed' | 'limit-reached').
export interface RecreateStatus {
  required: boolean;
  count: number;
  exhausted: boolean;
  cooling_down: boolean;
  reason: string;
}

// N13-D1: core.connector_status().relay — the relay client section (counts +
// shape tokens + the configured PUBLIC relay URL set; no token material —
// core-side status discipline). `urls` ride `rel://`/`rels://` scheme URIs
// (RelayUrl::parse contract, relay_client.rs); the shell freezes their
// host:port into the N2-H exclusion set (NetBirdEndpointExclusion.ets).
export interface RelayStatus {
  enabled: boolean;
  state: string;
  urls: string[];
  reconnects: number;
  frames_tx: number;
  frames_rx: number;
  transport_bytes: number;
  token_valid: boolean;
  last_error_class: string | null;
}

// N13 loop-dependency fix (core 95f6c02): the MANAGEMENT-ADVERTISED relay url
// set — the `NetbirdConfig.relay.urls` of the LAST sync carrying a netbird
// config — rendered by the core as the SIBLING document section
// `relay_advertised` BEFORE the frozen runtime `relay` object. It is exposed
// in EVERY state, also `relay_enabled:false`, so the shell can freeze the
// relay endpoint into its N2-H exclusion set BEFORE it enables relay
// (docs/n13-relay-increment-plan-20260914.md §3.4/§5). URLs only — public
// routing material; never the relay token.
export interface RelayAdvertisedStatus {
  advertised: boolean;
  advertised_urls: string[];
}

// Lenient shell-side view of the `relay_advertised` section (see
// parseRelayAdvertised): `advertised` is the USABLE verdict (valid string
// urls present), `malformed` marks a type anomaly for the grep-able hint —
// both are observations, neither may throw.
export interface RelayAdvertisedView {
  advertised: boolean;
  urls: string[];
  malformed: boolean;
}

// core.connector_status snapshot (subset the shell consumes).
// N5c: LIVE ICE socket queue depth from connector_status().ice_sockets.
export interface IceSocketDepth {
  queued: number;
  taken: number;
}

export interface ConnectorStatusResult {
  running: boolean;
  state: string;
  peer_count: number;
  route_count: number;
  reconnects: number;
  last_error: ConnectorErrorClass | null;
  // N5c starvation observability: the ICE socket queue depth (queued = still
  // available to the core, taken = consumed so far; -1 = no shell-fed source).
  // The shell MUST read this instead of trusting its own last feed reading.
  ice_sockets?: IceSocketDepth;
  session_expiry: string;
  wg_apply_failed: boolean;
  wg_apply_errors: number;
  // N3-7: worker ENDED BY ITSELF (fatal / retry budget exhausted). This is
  // the signal the shell watcher tears the VPN down on (gap 3).
  terminal: boolean;
  // N5d: the real signal link (registered = frames may flow).
  signal: SignalStatus;
  // N7: the real WG data plane (fed/device/ready + device counters).
  wg?: WgDataplaneStatus;
  // N8: the controlled-recreate trigger (the shell rebuilds when required).
  recreate?: RecreateStatus;
  // N13-D1: the relay client section (urls = the freeze/exclusion source).
  relay?: RelayStatus;
  // N13 loop-dependency fix (core 95f6c02): the management ADVERTISEMENT,
  // exposed ALWAYS (also relay_enabled:false) as a SIBLING section rendered
  // before the runtime `relay` object. The N2-H "exclude before enable"
  // source; consumed leniently (parseRelayAdvertised) — a missing/type-
  // anomalous section is treated as NOT advertised, never a throw.
  relay_advertised?: RelayAdvertisedStatus;
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
  // N5d: the signal server URI from netbird_config.signal (host:port,
  // possibly scheme-prefixed). The shell resolves it, opens + protects a
  // socket and feeds it back via connectorSignalFeed.
  signal?: string | null;
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
  // N13-D1: opt-in relay client switch (core ConnectorConfig JSON key
  // 'relay_enabled', default false). Parsed leniently below: only the exact
  // boolean true enables it; missing/false/non-boolean stays disabled.
  relay_enabled?: boolean;
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

// Read a credential file with the DEVELOPMENT-ONLY /data/local/tmp override
// (DEV_CREDENTIAL_DIR — see the note at the top of this file). Same name-only
// resolution and same fail-closed semantics as readPrivateFileText: an empty
// override file throws; a missing/unreachable override (the app sandbox may
// simply not see that path) falls back to filesDir; credentials found in
// NEITHER location throw and the caller aborts the VPN start.
export function readCredentialFileText(filesDir: string, fileName: string): string {
  const devPath: string = `${DEV_CREDENTIAL_DIR}/${fileName}`;
  let devAvailable: boolean = false;
  try {
    devAvailable = fs.accessSync(devPath);
  } catch (error) {
    devAvailable = false;
  }
  if (devAvailable) {
    hilog.warn(DOMAIN, TAG,
      'CRED_DEV_OVERRIDE|file=%{public}s|source=data-local-tmp|note=development-only-not-secure-storage',
      fileName);
    const text: string = fs.readTextSync(devPath);
    if (text.length === 0) {
      throw new Error(`empty dev override file: ${devPath}`);
    }
    return text.trim();
  }
  return readPrivateFileText(filesDir, fileName);
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
  // N13-D1 lenient relay opt-in: only the exact boolean true enables; a
  // missing/false/non-boolean value (e.g. string/number) stays disabled and
  // NEVER throws — a malformed flag must not abort the existing start flow.
  if (device.relay_enabled !== true) {
    device.relay_enabled = undefined;
  }
  return device;
}

// ---------------------------------------------------------------------------
// Packaged public CA roots (https management without an explicit device
// ca_pem): the core reads NO system trust store — it requires the caller to
// inject the PEM roots (client/core/src/grpc.rs:63-66) and treats a missing
// ca_pem for https:// as a configuration error (connector.rs:683).
// HarmonyOS exposes no API to export the platform root store, so the PUBLIC
// Let's Encrypt anchors (ISRG Root X1 + X2) ship inside the HAP rawfile and
// are injected shell-side for https deployments; an explicit device.ca_pem
// (private CA / self-hosted) keeps priority. Rawfile path is relative to
// resources/rawfile, no leading slash (resourceManager rawfile contract).
// ---------------------------------------------------------------------------
export const PACKAGED_CA_ROOTS_RAWFILE: string = 'nb-public-roots.pem';

// Count PEM CERTIFICATE blocks — meta information only (logging discipline:
// certificate bodies are never logged, only this count).
export function countPemCertificates(pem: string): number {
  return pem.split('-----BEGIN CERTIFICATE-----').length - 1;
}

// Read the packaged public roots PEM from the HAP rawfile and decode it as
// UTF-8. Throws on read failure, empty content or certificate-less content —
// the caller maps ANY throw to the fail-closed https-ca-roots abort (an
// https management connection must never start with an empty root set).
// API signatures verified against the local SDK
// (@ohos.resourceManager.d.ts / @ohos.util.d.ts):
//   getRawFileContentSync(path: string): Uint8Array  (@since 10)
//   util.TextDecoder.create(encoding?: string): TextDecoder
//   TextDecoder.decodeToString(input: Uint8Array): string
export function readPackagedCaRootsPem(resManager: resourceManager.ResourceManager): string {
  const bytes: Uint8Array = resManager.getRawFileContentSync(PACKAGED_CA_ROOTS_RAWFILE);
  if (bytes.length === 0) {
    throw new Error('empty packaged ca roots rawfile');
  }
  const decoder: util.TextDecoder = util.TextDecoder.create('utf-8');
  const pem: string = decoder.decodeToString(bytes).trim();
  if (pem.length === 0) {
    throw new Error('empty decoded ca roots pem');
  }
  if (countPemCertificates(pem) === 0) {
    throw new Error('packaged ca roots pem has no certificate');
  }
  return pem;
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
  // N13-D1: opt-in relay switch — written ONLY when true (core default is
  // false); enabling on a device requires the N2-H endpoint exclusion set
  // to be ready for the relay URLs.
  if (device.relay_enabled === true) {
    cfg['relay_enabled'] = true;
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

// N5d: feed a fresh PROTECTED signal socket (open + protect first) plus the
// shell-resolved signal address. The real signal link dials ONLY over such
// sockets — every dial (initial AND per reconnect) takes a fresh one; a
// starved queue fails the dial CLOSED (no unprotected fallback). The FIRST
// fed address wins for the lifetime of the link (same fixed-address
// contract as the management factory).
export function connectorSignalFeed(fd: number, connectAddr: string): SocketFeedResult {
  const addrJson: Record<string, Object> = { 'connect_addr': connectAddr };
  return JSON.parse(core.connector_signal_socket_feed(fd, JSON.stringify(addrJson))) as
    SocketFeedResult;
}

// N7 feed results: {ok:true,device_up:bool} — device_up flips true once BOTH
// the WG socket and the TUN fd have been fed (the data plane is pumping).
export interface WgFeedResult {
  ok: boolean;
  device_up?: boolean;
  error?: string;
}

// N7: feed the PROTECTED WG outer UDP socket (native-opened via
// core.wg_fwd_open, then VpnConnection.protect'ed BEFORE any datagram flows
// — governance §二.4). fd crosses as a number; native dup-consumes it at
// device construction and never uses/closes the original. Fail-closed:
// until BOTH feeds land the data plane stays down, `tunnel_ready` stays
// false and the default route stays HELD (no unprotected fallback).
export function connectorWgSocketFeed(fd: number): WgFeedResult {
  return JSON.parse(core.connector_wg_socket_feed(fd)) as WgFeedResult;
}

// N7: feed the platform TUN fd from VpnConnection.create(). The shell KEEPS
// the raw fd — its close belongs exclusively to VpnConnection.destroy();
// native consumes ONLY a dup copy at device construction (fd contract).
// N8: while the device is up this REPLACES the platform TUN fd (controlled
// recreate — new dup adopted, old dup deactivated, WG sessions kept).
export function connectorTunFdFeed(fd: number): WgFeedResult {
  return JSON.parse(core.connector_tun_fd_feed(fd)) as WgFeedResult;
}

// N8: connector_route_set_applied result —
// {ok:true,recreate:{required,count,exhausted,cooling_down,reason}} /
// {ok:false,error}.
export interface RouteSetAppliedResult {
  ok: boolean;
  recreate?: RecreateStatus;
  error?: string;
}

// N8: ACK the route set the shell just applied to the platform VpnConfig at
// its (re)create() time (`routes` = the canonical networks from
// connector_network_config(), e.g. ['0.0.0.0/0','172.16.0.0/12']).
// `isRecreate=false` records the INITIAL create's applied set (arms the
// desired-vs-applied comparison); `true` records a COMPLETED controlled
// recreate (counts against the core-side budget and arms the cooldown).
// Without this ACK the core never raises `recreate.required` (N7 behavior
// preserved for an older shell).
export function connectorRouteSetApplied(routes: string[], isRecreate: boolean):
  RouteSetAppliedResult {
  const payload: Record<string, Object> = { 'routes': routes };
  return JSON.parse(core.connector_route_set_applied(JSON.stringify(payload), isRecreate)) as
    RouteSetAppliedResult;
}

// Cross-process status bridge, UI side (device-verified defect 2026-09-14):
// the connector runs in the VpnExtension process, so a NAPI status call from
// the UI process reads an empty slot and shows "disconnected / wg sessions: 0"
// while the tunnel is live. The extension publishes these fields into the
// shared filesDir instead; the UI reads that document.
export interface PublishedStatus {  updated_at_ms: number;
  state: string;
  peer_count: number;
  route_count: number;
  last_error_class: string;
  signal_registered: boolean;
  signal_reconnects: number;
  wg_ready: boolean;
  wg_sessions: number;
}

// Read the published status. A missing, unreadable, malformed or STALE
// document returns null so the caller can render "unavailable" rather than
// passing off默 zeroes as live state.
export function readPublishedStatus(filesDir: string): PublishedStatus | null {
  const path: string = `${filesDir}/netbird-status.json`;
  try {
    if (!fs.accessSync(path)) {
      return null;
    }
    const text: string = fs.readTextSync(path);
    if (text.length === 0) {
      return null;
    }
    const doc: PublishedStatus = JSON.parse(text) as PublishedStatus;
    if (doc.updated_at_ms === undefined || doc.updated_at_ms === null) {
      return null;
    }
    if (Date.now() - doc.updated_at_ms > STATUS_POLL_MS * 3) {
      return null;
    }
    return doc;
  } catch (error) {
    return null;
  }
}

// N3-7: open + bind (NO connect) a TCP management socket for the protect
// gate. fd stays native-created; the shell protects it, then hands the SAME
// number back via connectorStartWithSocket/connectorSocketFeed.
export function mgmtSocketOpen(): NativeTcpSocketOpen {
  return JSON.parse(core.mgmt_socket_open()) as NativeTcpSocketOpen;
}

// Device-path ICE socket opener (N5c/N7 device gap): the same protect-gate
// split as the management socket, UDP variant. Core opens an UNBOUND UDP
// socket (binding here would pick the port before the interface is known);
// the ICE consumer dups the fd, binds the copy per interface and reads the
// chosen port back via getsockname.
export function iceSocketOpen(): NativeTcpSocketOpen {
  return JSON.parse(core.ice_socket_open()) as NativeTcpSocketOpen;
}

// Feed a PROTECTED ICE UDP socket (open + protect first). Every gathering
// round takes a fresh fd; a starved queue fails candidate gathering CLOSED —
// there is no unprotected UDP fallback (governance §二.4).
export function connectorIceSocketFeed(fd: number): SocketFeedResult {
  return JSON.parse(core.connector_ice_socket_feed(fd)) as SocketFeedResult;
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

// N5d: parse the `netbird_config.signal` URI into host + port (same
// ManagementEndpoint shape as the management parser so resolveManagementIp
// is reusable). Handles bare `host:port` (the wire shape, e.g.
// 'signal.netbird.io:10000'), optional http(s):// prefixes; `defaultPort`
// is the caller's TLS decision (443 for https-style configs, 80 otherwise).
// Malformed input throws a stable error text — the caller aborts
// (fail-closed).
export function parseSignalEndpoint(uri: string, defaultPort: number): ManagementEndpoint {
  if (uri.length === 0) {
    throw new Error('signal-uri-malformed');
  }
  let rest: string = uri;
  let port: number = defaultPort;
  const schemeSep: number = rest.indexOf('://');
  if (schemeSep > 0) {
    const scheme: string = rest.slice(0, schemeSep);
    if (scheme !== 'https' && scheme !== 'http') {
      throw new Error('signal-uri-malformed');
    }
    port = scheme === 'https' ? 443 : 80;
    rest = rest.slice(schemeSep + 3);
  }
  const slash: number = rest.indexOf('/');
  if (slash >= 0) {
    rest = rest.slice(0, slash);
  }
  if (rest.length === 0) {
    throw new Error('signal-uri-malformed');
  }
  const colon: number = rest.lastIndexOf(':');
  let host: string = rest;
  if (colon >= 0) {
    host = rest.slice(0, colon);
    port = Number(rest.slice(colon + 1));
    if (!Number.isInteger(port) || port <= 0 || port > 65535) {
      throw new Error('signal-uri-malformed');
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

// Bare IPv4 dotted-quad literal check (digits only, 0-255 per octet) — same
// shape as the inline checks in parseManagementEndpoint/parseSignalEndpoint.
function isIPv4Literal(host: string): boolean {
  const octets: string[] = host.split('.');
  if (octets.length !== 4) {
    return false;
  }
  return octets.every((o: string): boolean => {
    if (o.length === 0 || o.length > 3) {
      return false;
    }
    for (let i = 0; i < o.length; i++) {
      const code: number = o.charCodeAt(i);
      if (code < 0x30 || code > 0x39) {
        return false;
      }
    }
    return Number(o) <= 255;
  });
}

// N2-H: resolve ONE outer-endpoint host for the route-exclusion set (see
// NetBirdEndpointExclusion.ets). FRESH per call by contract — DDNS hosts
// (e.g. the relay endpoint, TTL 600) must never ride a cached IP: every
// config build re-resolves. Shell-side DNS only (same rule as
// resolveManagementIp — nameserver traffic never enters native or the
// tunnel), bounded by `timeoutMs`. IPv4 answers are preferred; an IPv6
// answer is returned as-is (the exclusion renders it as a /128). IP literals
// short-circuit DNS. Throws 'dns-failed' — the caller maps failure to
// skip-with-log (relay not required) or fail-closed (relay required).
export async function resolveExclusionEndpointIp(host: string, timeoutMs: number):
  Promise<string> {
  if (host.length === 0) {
    throw new Error('dns-failed');
  }
  if (host.indexOf(':') >= 0) {
    return host; // bracketed IPv6 literal — no DNS leg
  }
  if (isIPv4Literal(host)) {
    return host; // IPv4 literal — no DNS leg
  }
  const resolution: Promise<string> = connection.getAddressesByName(host)
    .then((addresses: connection.NetAddress[]): string => {
      for (let i = 0; i < addresses.length; i++) {
        if (addresses[i].address.indexOf(':') < 0) {
          return addresses[i].address; // prefer IPv4 (the dial path's family)
        }
      }
      for (let i = 0; i < addresses.length; i++) {
        if (addresses[i].address.indexOf(':') >= 0) {
          return addresses[i].address; // IPv6 fallback — still freezable (/128)
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

// Lenient parse of the status document's `relay_advertised` sibling section
// (core 95f6c02). NEVER throws: a missing/absent section, a non-object
// section, a non-boolean `advertised` flag, a non-array or mixed-element
// `advertised_urls` — every anomaly lands in `malformed` and the caller
// treats the verdict as NOT advertised (with a grep-able hint), keeping the
// existing start flow alive. Only well-formed string elements feed `urls`
// (deduplicated, order preserved). No token material passes through —
// public routing URLs only.
export function parseRelayAdvertised(status: ConnectorStatusResult):
  RelayAdvertisedView {
  const view: RelayAdvertisedView = { advertised: false, urls: [], malformed: false };
  const section = status.relay_advertised;
  if (section === undefined || section === null) {
    return view;
  }
  const flagIsBoolean: boolean = typeof section.advertised === 'boolean';
  const rawUrls = section.advertised_urls;
  const urlsIsArray: boolean = Array.isArray(rawUrls);
  if (!flagIsBoolean || !urlsIsArray) {
    view.malformed = true;
    return view;
  }
  const rawList: Object[] = rawUrls as Object[];
  for (let i = 0; i < rawList.length; i++) {
    const item: Object | null = rawList[i];
    if (typeof item === 'string') {
      const url: string = item as string;
      if (url.length > 0 && view.urls.indexOf(url) < 0) {
        view.urls.push(url);
      }
    } else {
      view.malformed = true;
    }
  }
  // Core semantics: advertised ⇔ advertised_urls non-empty. A present but
  // empty section is an honest "not advertised", NOT a type anomaly.
  view.advertised = view.urls.length > 0;
  return view;
}

// Counter renderer for the relay status line: only finite numbers pass
// through; every anomaly (null / string / NaN / Infinity) degrades to '-1'.
function relayCounterOrUnknown(value: number | null | undefined): string {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return String(Math.floor(value));
  }
  return '-1';
}

// Relay observability line (VPN_RELAY_STATUS): the periodic
// VPN_CONNECTOR_STATUS log carries no relay segment, so a live relay's
// counters were unreadable from the device side. formatRelayStatusLine
// renders the `relay` + `relay_advertised` sections of ONE connector_status
// snapshot into a single grep-able line, logged by the shell watcher right
// after the status line (same poll, no extra timer). Lenient by contract: a
// missing section, a non-object section, or any null/type-anomalous field
// degrades to a placeholder ('unknown' / -1 / false) and NEVER throws, so
// the surrounding watcher loop is untouched. Sensitive discipline: token
// material cannot pass through by construction — token_valid is a BOOLEAN
// verdict, and URLs render as a COUNT only (parseRelayAdvertised feeds the
// count; no url text, no token_payload/token_signature).
export function formatRelayStatusLine(requestId: string,
  status: ConnectorStatusResult | null): string {
  let enabled: string = 'unknown';
  let state: string = 'unknown';
  let framesTx: string = '-1';
  let framesRx: string = '-1';
  let transportBytes: string = '-1';
  let reconnects: string = '-1';
  let tokenValid: string = 'false';
  let lastError: string = 'unknown';
  let advertised: string = 'false';
  let advertisedUrls: string = '-1';
  try {
    if (status !== null && status !== undefined) {
      const relay = status.relay;
      if (relay !== undefined && relay !== null) {
        if (typeof relay.enabled === 'boolean') {
          enabled = relay.enabled ? 'true' : 'false';
        }
        if (typeof relay.state === 'string' && relay.state.length > 0) {
          state = relay.state;
        }
        framesTx = relayCounterOrUnknown(relay.frames_tx);
        framesRx = relayCounterOrUnknown(relay.frames_rx);
        transportBytes = relayCounterOrUnknown(relay.transport_bytes);
        reconnects = relayCounterOrUnknown(relay.reconnects);
        if (typeof relay.token_valid === 'boolean') {
          tokenValid = relay.token_valid ? 'true' : 'false';
        }
        if (relay.last_error_class === null || relay.last_error_class === undefined) {
          lastError = 'none'; // no error class = healthy relay (status-line convention)
        } else if (typeof relay.last_error_class === 'string' &&
          relay.last_error_class.length > 0) {
          lastError = relay.last_error_class;
        }
      }
      if (status.relay_advertised !== undefined && status.relay_advertised !== null) {
        const view: RelayAdvertisedView = parseRelayAdvertised(status);
        advertised = view.advertised ? 'true' : 'false';
        advertisedUrls = String(view.urls.length);
      }
    }
  } catch (error) {
    // Lenient by contract: keep the placeholders gathered so far.
  }
  return 'VPN_RELAY_STATUS' +
    '|requestId=' + requestId +
    '|enabled=' + enabled +
    '|state=' + state +
    '|framesTx=' + framesTx +
    '|framesRx=' + framesRx +
    '|transportBytes=' + transportBytes +
    '|reconnects=' + reconnects +
    '|tokenValid=' + tokenValid +
    '|lastError=' + lastError +
    '|advertised=' + advertised +
    '|advertisedUrls=' + advertisedUrls;
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
