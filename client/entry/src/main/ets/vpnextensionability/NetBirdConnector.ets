import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import connection from '@ohos.net.connection';
import resourceManager from '@ohos.resourceManager';
import util from '@ohos.util';
import core from 'libnetbird_core.so';
// type-only: erased at runtime, so the host selfcheck fixtures that stage
// this file byte-identically never resolve the sibling module
import type { UiPeerEntry } from './NetBirdUiModel';
import type { LanExclusionProbe, LanLinkAddress } from './NetBirdEndpointExclusion';
// type-only: erased at runtime, so the host selfcheck fixtures that stage this
// file byte-identically never resolve the sibling module. The route-selection
// and route-catalog types live in the pure route module (./NetBirdRoutePreferences,
// zero imports) and are NEVER re-declared here.
import type { RouteCatalog, RouteNetworkEntry, RouteSelection } from
  '../routes/NetBirdRoutePreferences';
import { DnsDerivationInput, DnsDerivationResult, DnsNetProbe, DnsPlatformNetAddress,
  StunUriEntry,
  deriveDnsEndpointExclusions, emptyDnsNetProbe, isPhysicalNetEvidence
} from './NetBirdEndpointExclusion';

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

// The shape of a platform `BusinessError` (a code-carrying Error) as far as
// this module reads it — one numeric field, so no extra @ohos.base import.
interface PlatformBusinessError extends Error {
  code?: number;
}

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

// {ok,already_stopped,state} — core.connector_stop (idempotent; NEVER sends
// a management Logout — stopping a connection is not deregistration).
export interface ConnectorStopResult {
  ok: boolean;
  already_stopped: boolean;
  state: string;
}

// {ok,reason} / {ok:false,error,reason} — core.connector_logout: the
// EXPLICIT user logout (ONE Logout RPC on the live logged-in client; the
// server deregisters and deletes the peer). reason: 'ok' |
// 'peer-already-gone' | '<error-class>' | 'timeout' |
// 'not-running' | 'login-not-completed-or-already-logged-out'.
export interface ConnectorLogoutResult {
  ok: boolean;
  reason?: string;
  error?: string;
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
  // N13-D1 per-peer session attribution (tail-appended by the core after the
  // carrier_* counters; read by name). UI phase 1: the peers tab's
  // online/direct-relay/handshake source.
  wg_peers?: WgPeerStatusEntry[];
}

// One connector_status().wg.wg_peers[] entry (N13-D1).
// 2026-10-07 client-detail increment: `allowed_ips` was always emitted by the
// core (`wg_peer_json`: masked prefixes, e.g. '10.30.30.2/32', '10.30.30.0/24')
// but had no declaration here — read-only, never a secret.
export interface WgPeerStatusEntry {
  pub_key_b64: string;
  endpoint?: string | null;
  allowed_ips?: string[];
  session_established: boolean;
  expired: boolean;
  last_handshake_s?: number | null;
  egress_local?: string | null;
  carrier_attached?: boolean;
  on_carrier?: boolean;
  // 2026-10-07 per-peer transfer counters (core N13-D4: tail-appended after
  // the carrier_* peer fields, always as an integer — a delivered `0` is a
  // real zero, not a missing value). PLAINTEXT inner-IP bytes, cumulative
  // since THIS peer's tunnel was created: tx_bytes = frames this device
  // delivered to the peer, rx_bytes = frames the peer delivered into this
  // device's TUN. They are NOT the device-level `wg.tx_bytes` /
  // `wg.rx_bytes_to_tun` (ENCAPSULATED wire bytes) — never mix the two.
  // Optional here so an older core snapshot still parses.
  tx_bytes?: number;
  rx_bytes?: number;
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
  // 2026-10-06 (详情页中继地址): the effective relay URL of the live (or
  // last live) session — the address every relayed peer's carrier traffic
  // flows through. Tail-appended by the core after `last_error_class`;
  // null while nothing was ever dialed. Read leniently.
  current_url?: string | null;
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

// 2026-10-07 per-peer ACTUAL relay lane (core `peer_relays` tail, appended
// AFTER the runtime `relay` object — every pre-existing field keeps its
// position). ONE entry per ATTACHED lane of the live relay carrier table:
// `pub_key_b64` is the peer's PUBLIC WireGuard key in the SAME key namespace
// as `wg.wg_peers[].pub_key_b64` (the shell joins them by key — a
// management/fqdn lookup stays the shell's business), `relay_url` is the
// canonical `rel://host:port` / `rels://host:port` that lane REALLY rides.
// The remote's ADVERTISED relay and the local configured/home URL are NOT
// evidence of an actual relay and never appear through this field. Public
// routing material only — no token, no userinfo.
export interface CorePeerRelayEntry {
  pub_key_b64: string;
  relay_url: string;
}

// One endpoint of a peer's SELECTED ICE pair (core `ice_peers[].selected_local`
// / `.selected_remote`), as the core projects it: the candidate TYPE token
// (host/srflx/prflx/relay — the same vocabulary as the signal wire) plus the
// PUBLIC ip:port. `foundation` / `priority` / `related_address` are check
// internals and deliberately absent; so are the ufrag/password credentials and
// any socket fd — none of them crosses this boundary.
export interface CoreIceEndpoint {
  type: string;
  address: string;
  port: number;
}

// One per-peer ICE detail (core `ice_peers[]` tail, appended AFTER
// `peer_relays` — every pre-existing field keeps its position; one entry per
// peer of the ICE orchestrator, PUBLIC-KEY sorted, joined by `pub_key`).
// `pub_key` is the peer's PUBLIC WireGuard key in the SAME namespace as
// `wg.wg_peers[].pub_key_b64` (base64) and `peer_relays[].pub_key_b64`.
// `selected_local` / `selected_remote` are BOTH null until a pair is really
// selected — never the first collected candidate — and a retained pair does
// NOT mean the peer is currently connected: `state` carries that verdict and
// `reachable` is the core's own `Connected && endpoint_applied` derivation
// (a `disconnected` peer can keep both endpoints and still be unreachable).
// `last_error` is the connector's class-only error JSON
// (`{class,status}`, never a server message).
export interface CoreIcePeerEntry {
  pub_key: string;
  state: string;
  controlling: boolean | null;
  selected_local: CoreIceEndpoint | null;
  selected_remote: CoreIceEndpoint | null;
  endpoint_applied: boolean;
  reachable: boolean;
  last_error: ConnectorErrorClass | null;
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
  // 0051 §7 item 3: the CORE's per-peer protected-socket demand for the current
  // interface set (allowed interfaces × 2), so the shell can size its pool from
  // core's real need instead of assuming 4. -1 = the core could not enumerate
  // its interfaces (the shell then keeps its ICE_SOCKET_PER_PEER fallback).
  // Optional on purpose: an older core / the no-connector snapshot omits it.
  need_per_peer?: number;
}

export interface ConnectorStatusResult {
  running: boolean;
  state: string;
  // 2026-10-07 uptime increment: the connector process times the core emits in
  // EVERY status directly after `state` (connector.rs `opt_unix_json`: an
  // integer Unix SECONDS stamp, or null when the core has no stamp yet).
  // Optional on purpose: an older core / the no-connector snapshot omits them.
  // These are CORE PROCESS times for the UI uptime row — they say nothing
  // about the management connection (that is signal_registered + state).
  started_at_unix?: number | null;
  last_update_unix?: number | null;
  peer_count: number;
  route_count: number;
  reconnects: number;
  last_error: ConnectorErrorClass | null;
  // N5c starvation observability: the ICE socket queue depth (queued = still
  // available to the core, taken = consumed so far; -1 = no shell-fed source).
  // The shell MUST read this instead of trusting its own last feed reading.
  ice_sockets?: IceSocketDepth;
  session_expiry: string;
  // 2026-10-07 client-detail increment: the deadline token above carries no
  // value; the core's `session_expires_at_unix` (Unix SECONDS, null for
  // unknown/disabled) is the machine-readable half. Optional on purpose: an
  // older core / the no-connector snapshot omits it.
  session_expires_at_unix?: number | null;
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
  // 2026-10-07: the ACTUAL per-peer relay lane table (core `peer_relays`,
  // the TAIL of the status document). The core emits: `null` = no relay
  // carrier armed at all (an array would be an invented statement), `[]` =
  // carrier armed with no lane attached, otherwise one entry per attached
  // lane, public-key sorted. NEVER an advertised / configured / global home
  // URL — only the live attached-lane table is evidence of a path. Absent on
  // an older core: the shell then treats every peer's ACTUAL relay as
  // UNKNOWN (never as "not relayed"). Public routing material only.
  peer_relays?: CorePeerRelayEntry[] | null;
  // 2026-10-07: the per-peer ICE DETAIL table (core `ice_peers`, the TAIL of
  // the status document, after `peer_relays`). `null` = no ICE orchestrator
  // at all (an array would assert one exists), `[]` = orchestrator armed with
  // ZERO peers, otherwise one `CoreIcePeerEntry` per peer, public-key sorted.
  // The shell joins it onto the published peers by `pub_key`; a peer with no
  // entry (or a table that is null/absent) keeps its ICE state UNKNOWN — the
  // UI must never invent a state, an endpoint or a role. Absent on an older
  // core. Public material only (see CoreIcePeerEntry).
  ice_peers?: CoreIcePeerEntry[] | null;
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
  // vpn_addresses (dotted-quad/prefix strings) — the peer's overlay address
  // lives here. UI phase 1: tail-appended by the core.
  vpn_addresses?: string[];
  // peer fqdn (the UI's peer NAME); null when management sent none.
  fqdn?: string | null;
  // 2026-10-06 peer-online semantics: TRUE when management listed the peer
  // in remotePeers (management-online), FALSE when it came from
  // offlinePeers. Tail-appended by the core after `fqdn`; read leniently
  // (missing = treated management-online, never a silent 离线).
  mgmt_online?: boolean;
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
  // N2-H I2 (design §13.13): the TAIL-appended announced
  // netbird_config.stuns[].uri list, VERBATIM (the shell's stun-class
  // exclusion source: parseStunUri mirrors the core's own `stun:` rules and
  // refuses `turn:` by name). Absent/empty = nothing announced — an honest
  // gap, never a fabricated endpoint.
  stuns?: StunUriEntry[] | null;
  // UI phase 1: own peer fqdn (the Home screen's domain line); null when
  // management sent none. Tail-appended by the core after `stuns`.
  fqdn?: string | null;
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
  // N13-D1: relay client switch (core ConnectorConfig JSON key
  // 'relay_enabled'). DEFAULT TRUE — official client parity: relay is always
  // on unless explicitly opted out. Parsed leniently below: only the exact
  // boolean false disables it; missing/true/non-boolean stays enabled.
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
  // N13-D1 lenient relay switch: relay is ON by default (official client
  // parity — cellular deployments depend on it); only the exact boolean
  // false opts out. A missing/true/non-boolean value (e.g. string/number)
  // normalizes to the enabled default and NEVER throws — a malformed flag
  // must not abort the existing start flow.
  if (device.relay_enabled !== false) {
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
//
// `routeSelection` (decision B, OPTIONAL): the startup per-NetID route
// selection. When omitted (`undefined`) or `null` NO `route_selection` key is
// written at all, so every pre-existing two-argument call stays BYTE-IDENTICAL
// (pinned by client/entry/selfcheck/run-relay-flag.mts). When provided it is
// written as a real JSON OBJECT — never a string-encoded document — because
// the core parses that value with the same strict `RouteSelector::from_value`
// it uses for the persisted document and fails the whole start with the stable
// `invalid-config` token on a malformed one (connector.rs `route_selection`
// branch). The core applies the selection BEFORE its first Sync (default
// filter, no automatic exit behaviour); loading the user's stored preference
// is the VPE's later job, not this builder's.
export function buildConnectorConfigJson(device: DeviceConfigFile,
  forceDefaultRoute: boolean, routeSelection?: RouteSelection | null): string {
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
  // N13-D1: relay switch — the core's own default is false, so the shell
  // must write the resolved value explicitly: true (the enabled default,
  // official client parity) unless the device config opted out with an
  // exact relay_enabled:false.
  if (device.relay_enabled === false) {
    cfg['relay_enabled'] = false;
  } else {
    cfg['relay_enabled'] = true;
  }
  // Appended last so the key order of every existing field is untouched.
  if (routeSelection !== undefined && routeSelection !== null) {
    cfg['route_selection'] = routeSelection;
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
  setupKeyJson: string, connectAddr: string, dnsServers: string[] = []): ConnectorStartResult {
  const addrJson: Record<string, Object> = { 'connect_addr': connectAddr, 'dns_servers': dnsServers };
  return JSON.parse(core.connector_start_with_socket(fd, configJson, setupKeyJson,
    JSON.stringify(addrJson))) as ConnectorStartResult;
}

// N3-7: feed a fresh protected socket (open + protect first) so reconnect
// dials can re-protect. Fail-closed without resupply: dials error, they
// never fall back to an unprotected direct connection.
export function connectorPhysicalNetworkChanged(connectAddr: string): SocketFeedResult {
  const address: Record<string, Object> = { 'connect_addr': connectAddr };
  return JSON.parse(core.connector_physical_network_changed(
    connectAddr.length === 0 ? JSON.stringify('') : JSON.stringify(address))) as SocketFeedResult;
}

export function connectorManagementAddressUpdate(connectAddr: string): SocketFeedResult {
  const address: Record<string, Object> = { 'connect_addr': connectAddr };
  return JSON.parse(core.connector_management_address_update(JSON.stringify(address))) as SocketFeedResult;
}

export interface ProtectedDnsResult {
  ok: boolean;
  id?: number;
  pending?: boolean;
  ip?: string;
}

export async function resolveProtectedIp(host: string): Promise<string> {
  if (isIPv4Literal(host)) { return host; }
  const started: ProtectedDnsResult = JSON.parse(core.connector_protected_dns_start(host)) as ProtectedDnsResult;
  if (!started.ok || started.id === undefined) { return ''; }
  for (let attempt = 0; attempt < 50; attempt++) {
    await new Promise<void>((resolve: () => void): void => { setTimeout(resolve, 100); });
    const result: ProtectedDnsResult = JSON.parse(core.connector_protected_dns_poll(started.id)) as ProtectedDnsResult;
    if (!result.ok) { return ''; }
    if (!result.pending) { return result.ip ?? ''; }
  }
  return '';
}

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

export interface SocketSupplyRequest {
  id: string;
  kind?: string;
  purpose?: string;
}

export function socketSupplyPoll(): SocketSupplyRequest {
  return JSON.parse(core.socket_supply_poll()) as SocketSupplyRequest;
}

export function socketSupplyReply(id: string, fd: number, result: string): SocketFeedResult {
  return JSON.parse(core.socket_supply_reply(id, fd, result)) as SocketFeedResult;
}

export function socketSupplyCloseFd(fd: number): void {
  core.socket_supply_close_fd(fd);
}

export function socketSupplyDnsServers(servers: string[]): SocketFeedResult {
  return JSON.parse(core.socket_supply_dns_servers(JSON.stringify(servers))) as SocketFeedResult;
}

export function socketSupplyShutdown(): void {
  core.socket_supply_shutdown();
}

// N7 feed results: {ok:true,device_up:bool,mono_ms:int} — device_up flips true
// once BOTH the WG socket and the TUN fd have been fed (the data plane is
// pumping). N2 T7a (item 6): `mono_ms` is the CORE's monotonic instant of this
// feed, the clock the core's own `N7_WG_FEED|adopted|…|mono_ms=` line uses, so
// the shell's `VPN_WG_DATAPLANE_FED` line can be aligned with it.
export interface WgFeedResult {
  ok: boolean;
  device_up?: boolean;
  error?: string;
  mono_ms?: number;
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

// roam-1005-0124: the pre-destroy TUN release result —
// {ok:true,released:bool,fd:number,device_up:bool} / {ok:false,error}.
export interface TunReleaseResult {
  ok: boolean;
  released?: boolean;
  fd?: number;
  device_up?: boolean;
  error?: string;
}

// roam-1005-0124: BEFORE the recreate's destroy(), have the core close its
// current TUN dup (TUN half of the data plane pauses; peers/WG sessions
// kept) — the platform cannot remove the OLD vpn-tun netdev while our dup
// is the last open fd over it, which is what made the old teardown land
// AFTER the new create() and kill the same-named fresh interface.
// `device-not-up` (nothing to release) is a NON-fatal answer.
export function connectorTunRelease(): TunReleaseResult {
  return JSON.parse(core.connector_tun_release()) as TunReleaseResult;
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
//
// `kind` (Core 5f, OPTIONAL): 'initial' | 'auto' | 'user', the ACK kind of the
// 3-argument native entry. `'user'` records the route set the USER's live route
// toggle actually applied to the OS and spends NONE of the automatic recreate
// budget (no count, no cooldown): a manual toggle must never eat the automatic
// churn budget. Omitted (`undefined`) calls the legacy TWO-argument native
// entry byte-identically to the pre-existing wrapper, so a core without the
// 3-argument entry keeps working unchanged.
//
// The `routes` array crosses EXPLICITLY, exactly as handed over — including an
// EMPTY one (the user turned every network off), which the kind-aware native
// entry accepts for `'user'`. This wrapper never short-circuits an empty list
// and never substitutes a placeholder route for it; a refusal must come from
// the native answer (`ok:false` + `error`), never from this side.
export function connectorRouteSetApplied(routes: string[], isRecreate: boolean,
  kind?: string): RouteSetAppliedResult {
  const payload: Record<string, Object> = { 'routes': routes };
  if (kind === undefined) {
    return JSON.parse(core.connector_route_set_applied(JSON.stringify(payload), isRecreate)) as
      RouteSetAppliedResult;
  }
  return JSON.parse(core.connector_route_set_applied(JSON.stringify(payload), isRecreate, kind)) as
    RouteSetAppliedResult;
}

// Cross-process status bridge, UI side (device-verified defect 2026-09-14):
// the connector runs in the VpnExtension process, so a NAPI status call from
// the UI process reads an empty slot and shows "disconnected / wg sessions: 0"
// while the tunnel is live. The extension publishes these fields into the
// shared filesDir instead; the UI reads that document.
export interface PublishedStatus {
  updated_at_ms: number;
  request_id?: string;
  state: string;
  peer_count: number;
  route_count: number;
  last_error_class: string;
  signal_registered: boolean;
  signal_reconnects: number;
  wg_ready: boolean;
  wg_sessions: number;
  // ---- UI phase 1 appended fields (optional: an older document omits
  //      them; the UI degrades gracefully) ----
  // own peer fqdn / overlay IP from the network map (null when unknown)
  fqdn?: string | null;
  address?: string | null;
  // relay client state token (null when no relay section)
  relay_state?: string | null;
  // management reconnect count (diagnostics page)
  reconnects?: number;
  // per-peer merged view (name/ip/online/connection/handshake)
  peers?: UiPeerEntry[];
  // 2026-10-05: the extension's vpn-tun interface probe (true/false); omitted
  // when the probe could not read the counter or the document predates the
  // field. '已连接' requires this not to be false.
  iface_present?: boolean;
  // 2026-10-06 (详情页中继地址): the effective relay URL of the live (or
  // last live) relay session — relay.current_url when set, else the first
  // configured url; null when neither is known. Read leniently.
  // LOCAL session only (the configured home URL when nothing is dialed): it
  // must NEVER be presented as, or copied into, a PEER's relay address — the
  // per-peer ACTUAL lane is UiPeerEntry.actual_relay_url (core
  // `peer_relays`), which stays 未知 when that table was not delivered.
  relay_url?: string | null;
  // ---- 2026-10-07 client-detail increment (all optional: an older document
  //      omits them and the UI must read "absent" as 未知, never as a
  //      success or a zero) ----
  // Local network view: own overlay address with prefix, interface resolver,
  // configured DNS servers, managed routes + the default-route gate verdict,
  // and the signal URI. Mirrors NetBirdUiModel.UiNetworkView.
  network?: Object;
  // Aggregate WireGuard data-plane counters (core `wg` names verbatim).
  wg_traffic?: Object;
  // Relay client view (counters + configured/current URLs, no token).
  relay?: Object;
  // Connector session deadline: the core's token ('unknown'|'disabled'|'set')
  // and the Unix SECONDS value when the token is 'set'.
  session_expiry?: string;
  session_expires_at_s?: number;
  // 2026-10-07 uptime increment: the core process's own Unix SECONDS stamps,
  // passed through verbatim from the core status. Both are omitted when the
  // core delivered no positive value, so the UI shows no uptime rather than
  // one computed from 1970. Process times only — never a management
  // connection verdict (that stays signal_registered + state).
  started_at_unix?: number;
  last_update_unix?: number;
  // The management authority (scheme + host + port only — never the URL).
  management?: Object;
}

// Read the published status. A missing, unreadable, malformed or STALE
// document returns null so the caller can render "unavailable" rather than
// passing off stale values as live state.
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
    // Same staleness rule as NetBirdStatusPolicy (two extension publish
    // ticks): a document the extension stopped refreshing is not live state.
    if (Date.now() - doc.updated_at_ms > STATUS_POLL_MS * 2) {
      return null;
    }
    return doc;
  } catch (error) {
    return null;
  }
}

// ---------------------------------------------------------------------------
// 2026-10-06 登录失败可见性 (connect-failure notice): the VPN extension writes
// ONE persistent notice when the connector never reaches a live tunnel —
// login fatal (auth), login retry budget exhausted, or a startup abort
// before the VPN was created (the 15 s network-map box expired without a
// map, the connector start was refused, the credentials are missing).
// Without it the home card stays at a stale '连接中…' forever: the status
// document either never appeared or was removed by the teardown. Same
// shared-filesDir channel as the published status / logout ticket; the UI
// deletes the file when it sends a fresh start request.
// ---------------------------------------------------------------------------

export const CONNECT_FAILURE_FILE: string = 'netbird-connect-failure';
// A notice older than this is not shown (returning to the app much later
// renders the honest 未连接 instead of a stale failure banner).
export const CONNECT_FAILURE_MAX_AGE_MS: number = 600000;

export interface ConnectFailureNotice {
  at_ms: number;
  // stable class token: the connector's own last_error class
  // (auth/timeout/network/request/server/...) or a stage token written by
  // the extension ('credentials-missing', 'no-signal-uri', ...).
  error_class: string;
}

// Read the connect-failure notice; null when absent, malformed or older
// than CONNECT_FAILURE_MAX_AGE_MS. Never throws.
export function readConnectFailure(filesDir: string): ConnectFailureNotice | null {
  const path: string = `${filesDir}/${CONNECT_FAILURE_FILE}`;
  try {
    if (!fs.accessSync(path)) {
      return null;
    }
    const text: string = fs.readTextSync(path);
    if (text.length === 0) {
      return null;
    }
    const notice: ConnectFailureNotice = JSON.parse(text) as ConnectFailureNotice;
    if (notice.at_ms === undefined || notice.at_ms === null || notice.at_ms <= 0) {
      return null;
    }
    if (Date.now() - notice.at_ms > CONNECT_FAILURE_MAX_AGE_MS) {
      return null;
    }
    return notice;
  } catch (error) {
    return null;
  }
}

// Remove the notice (fresh start attempt / cleanup). Best-effort, never
// throws — a leftover file only survives until the next failure overwrites
// it.
export function removeConnectFailure(filesDir: string): void {
  const path: string = `${filesDir}/${CONNECT_FAILURE_FILE}`;
  try {
    if (fs.accessSync(path)) {
      fs.unlinkSync(path);
    }
  } catch (error) {
    // best-effort: ignore
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
//
// RETIRED IN STAGE 2 (official-alignment audit §2.7): the ICE plane no longer
// takes a socket per gather round — it uses the ONE shared mux below. Kept (with
// `connectorIceSocketFeed` and `NetBirdIceSocketPool.ets`) until stage 4 deletes
// the shell-side socket rationing together.
export function iceSocketOpen(): NativeTcpSocketOpen {
  return JSON.parse(core.ice_socket_open()) as NativeTcpSocketOpen;
}

// Feed a PROTECTED ICE UDP socket (open + protect first). Every gathering
// round takes a fresh fd; a starved queue fails candidate gathering CLOSED —
// there is no unprotected UDP fallback (governance §二.4).
export function connectorIceSocketFeed(fd: number): SocketFeedResult {
  return JSON.parse(core.connector_ice_socket_feed(fd)) as SocketFeedResult;
}

// STAGE 2 (official-alignment audit §2.7): the ONE shared ICE mux socket —
// opened AND BOUND by the core (wildcard, one port), then protected by the shell
// and fed back on the SAME fd (the `wg_fwd_open` shape). `port` is the port every
// host candidate of every peer advertises; `replaced` is true when this feed
// superseded a previous mux (a physical-network change re-protects on the new
// VpnConnection).
export interface IceMuxOpen {
  fd: number;
  port: number;
  bind_rc: number;
  bind_errno: number;
}

export interface IceMuxFeedResult {
  ok: boolean;
  port?: number;
  replaced?: boolean;
  error?: string;
}

// Core opens + binds the shared mux socket (wildcard, ephemeral port). The shell
// protects `fd` and feeds it back through connectorIceMuxFeed; until then no
// datagram may flow (fail-closed).
export function iceMuxOpen(): IceMuxOpen {
  return JSON.parse(core.ice_mux_open()) as IceMuxOpen;
}

// Feed the PROTECTED shared ICE mux socket. Failure tokens: no-connector,
// no-mux-slot, socket-fd-missing, socket-fd-invalid, socket-unbound. Without a
// mux there are NO local candidates (no unprotected fallback).
export function connectorIceMuxFeed(fd: number): IceMuxFeedResult {
  return JSON.parse(core.connector_ice_mux_feed(fd)) as IceMuxFeedResult;
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
  timeoutMs: number, netId?: number): Promise<string> {
  if (endpoint.ip.length > 0) {
    return `${endpoint.ip}:${endpoint.port}`;
  }
  const handles: connection.NetHandle[] = netId === undefined ? [] : connection.getAllNetsSync();
  const physical: connection.NetHandle | undefined = handles.find((h: connection.NetHandle): boolean =>
    h.netId === netId);
  if (netId !== undefined && physical === undefined) {
    throw new Error('dns-failed');
  }
  const resolution: Promise<string> = (physical === undefined ?
    connection.getAddressesByName(endpoint.host) : physical.getAddressesByName(endpoint.host))
    .then((addresses: connection.NetAddress[]): string => {
      for (let i = 0; i < addresses.length; i++) {
        // prefer IPv4 (this client has no IPv6 data plane yet)
        if (addresses[i].address.indexOf(':') < 0) {
          return `${addresses[i].address}:${endpoint.port}`;
        }
      }
      throw new Error('dns-failed');
    });
  let boxTimer: number = -1;
  const box: Promise<string> = new Promise<string>((resolve: (value: string) => void,
    reject: (e: Error) => void) => {
    boxTimer = setTimeout((): void => {
      reject(new Error('dns-failed'));
    }, timeoutMs);
  });
  try {
    return await Promise.race([resolution, box]);
  } finally {
    clearTimeout(boxTimer);
  }
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
  let boxTimer: number = -1;
  const box: Promise<string> = new Promise<string>((resolve: (value: string) => void,
    reject: (e: Error) => void) => {
    boxTimer = setTimeout((): void => {
      reject(new Error('dns-failed'));
    }, timeoutMs);
  });
  try {
    return await Promise.race([resolution, box]);
  } finally {
    clearTimeout(boxTimer);
  }
}

let physicalDefaultDnsNetId: number = 0;

export function forgetPhysicalDefaultDns(): void {
  physicalDefaultDnsNetId = 0;
}

// N2 first batch C/D columns (2026-09-29): the RAW platform facts the pure
// `classifyN2Bearer` needs to judge a column's direction — the physical default
// network's `bearerTypes` and the string form of its link addresses. This
// function performs the two synchronous reads and NOTHING else: the
// classification (and every refusal token) lives in the pure module, so the
// host selfcheck drives exactly the shipped decision through the stubbed
// platform. A platform error answers empty facts, never an invented bearer.
//
// B5: the handle is the SAME one `physicalDefaultNetId()` selected, NOT
// whatever `getDefaultNetSync()` answers. Once this App's VPN is the default
// network, the raw default handle is the VPN: its `bearerTypes` is `[vpn]` and
// its link addresses can be the overlay, which would make every C/D direction
// read `bearer-unknown` (or the local address an overlay one). Resolving the
// already-selected physical netId back to its own handle keeps the direction
// judgement and the switch judgement on one and the same network.
export interface PhysicalBearerFacts {
  bearerTypes: number[];
  linkAddresses: string[];
}

export function physicalDefaultBearerFacts(): PhysicalBearerFacts {
  try {
    const netId: number = physicalDefaultNetId();
    if (netId === 0) {
      return { bearerTypes: [], linkAddresses: [] };
    }
    const handles: connection.NetHandle[] = connection.getAllNetsSync();
    let handle: connection.NetHandle | undefined = handles.find(
      (candidate: connection.NetHandle): boolean => candidate.netId === netId);
    if (handle === undefined || handle === null) {
      // The platform could not enumerate it: fall back to the default handle
      // ONLY when that IS the selected network. A default handle of a DIFFERENT
      // netId (the VPN) is never used — that is exactly the B5 defect.
      const fallback: connection.NetHandle = connection.getDefaultNetSync();
      if (fallback === undefined || fallback === null || fallback.netId !== netId) {
        return { bearerTypes: [], linkAddresses: [] };
      }
      handle = fallback;
    }
    const caps: connection.NetCapabilities = connection.getNetCapabilitiesSync(handle);
    const props: connection.ConnectionProperties =
      connection.getConnectionPropertiesSync(handle);
    const linkAddresses: string[] = [];
    const links: connection.LinkAddress[] | undefined = props.linkAddresses;
    if (links !== undefined && links !== null) {
      for (let i = 0; i < links.length; i++) {
        const address: connection.NetAddress | undefined = links[i].address;
        if (address !== undefined && address !== null && typeof address.address === 'string' &&
          address.address.length > 0) {
          linkAddresses.push(address.address);
        }
      }
    }
    return { bearerTypes: caps.bearerTypes ?? [], linkAddresses: linkAddresses };
  } catch (error) {
    return { bearerTypes: [], linkAddresses: [] };
  }
}

export function physicalDefaultNetId(): number {
  try {
    const handle: connection.NetHandle = connection.getDefaultNetSync();
    if (handle !== undefined && handle !== null && handle.netId !== 0) {
      const caps: connection.NetCapabilities = connection.getNetCapabilitiesSync(handle);
      const probe: DnsNetProbe = emptyDnsNetProbe();
      probe.handleId = handle.netId;
      probe.networkCap = caps.networkCap ?? [];
      probe.bearerTypes = caps.bearerTypes ?? [];
      probe.capsOk = true;
      if (isPhysicalNetEvidence(probe) && physicalDefaultDnsServers(handle.netId).length > 0) {
        return handle.netId;
      }
    }
    // Once the VPN is default, only a single physical candidate is unambiguous.
    const handles: connection.NetHandle[] = connection.getAllNetsSync();
    let candidate: number = 0;
    for (let i = 0; i < handles.length; i++) {
      const id: number = handles[i].netId;
      if (id !== 0 && physicalDefaultDnsServers(id).length > 0) {
        if (candidate !== 0 && candidate !== id) {
          forgetPhysicalDefaultDns();
          return 0;
        }
        candidate = id;
      }
    }
    return candidate;
  } catch (error) { return 0; }
}

// 动态基础 LAN 排除 (scenario-A roaming fix, LAN-ROUTE-ROAMING-ANALYSIS.md
// §4 step 1): the RAW platform facts the pure `deriveLanExclusion`
// (NetBirdEndpointExclusion) needs — the physical default network's
// `bearerTypes` and its link addresses WITH the prefix length (the string-only
// `physicalDefaultBearerFacts` view drops it, and the LAN exclusion needs the
// masked network, not the host address). Same B5 handle rule as
// physicalDefaultBearerFacts: the handle is the one `physicalDefaultNetId()`
// selected, NEVER the raw default handle (once this App's VPN is the default
// network that is the VPN, whose link address would be the overlay). A
// platform error answers an empty probe, never an invented prefix — the pure
// module turns that into "no LAN exclusion".
export function physicalDefaultLanProbe(): LanExclusionProbe {
  try {
    const netId: number = physicalDefaultNetId();
    if (netId === 0) {
      return { bearerTypes: [], linkAddresses: [] };
    }
    const handles: connection.NetHandle[] = connection.getAllNetsSync();
    let handle: connection.NetHandle | undefined = handles.find(
      (candidate: connection.NetHandle): boolean => candidate.netId === netId);
    if (handle === undefined || handle === null) {
      const fallback: connection.NetHandle = connection.getDefaultNetSync();
      if (fallback === undefined || fallback === null || fallback.netId !== netId) {
        return { bearerTypes: [], linkAddresses: [] };
      }
      handle = fallback;
    }
    const caps: connection.NetCapabilities = connection.getNetCapabilitiesSync(handle);
    const props: connection.ConnectionProperties =
      connection.getConnectionPropertiesSync(handle);
    const linkAddresses: LanLinkAddress[] = [];
    const links: connection.LinkAddress[] | undefined = props.linkAddresses;
    if (links !== undefined && links !== null) {
      for (let i = 0; i < links.length; i++) {
        const address: connection.NetAddress | undefined = links[i].address;
        const prefix: number = links[i].prefixLength;
        if (address !== undefined && address !== null && typeof address.address === 'string' &&
          address.address.length > 0) {
          linkAddresses.push({ address: address.address, prefixLength: prefix });
        }
      }
    }
    return { bearerTypes: caps.bearerTypes ?? [], linkAddresses: linkAddresses };
  } catch (error) {
    return { bearerTypes: [], linkAddresses: [] };
  }
}

export function physicalDefaultDnsServers(netId?: number): string[] {
  try {
    let handle: connection.NetHandle = connection.getDefaultNetSync();
    if (netId !== undefined) {
      const handles: connection.NetHandle[] = connection.getAllNetsSync();
      const selected: connection.NetHandle | undefined = handles.find((h: connection.NetHandle): boolean =>
        h.netId === netId);
      if (selected === undefined) { return []; }
      handle = selected;
    }
    if (handle === undefined || handle === null || handle.netId === 0) {
      return [];
    }
    let caps: connection.NetCapabilities = connection.getNetCapabilitiesSync(handle);
    let probe: DnsNetProbe = emptyDnsNetProbe();
    probe.handleId = handle.netId;
    probe.networkCap = caps.networkCap ?? [];
    probe.bearerTypes = caps.bearerTypes ?? [];
    probe.capsOk = true;
    if (!isPhysicalNetEvidence(probe)) {
      if (netId !== undefined || physicalDefaultDnsNetId === 0) {
        return [];
      }
      const handles: connection.NetHandle[] = connection.getAllNetsSync();
      let found: boolean = false;
      for (let i = 0; i < handles.length; i++) {
        if (handles[i].netId === physicalDefaultDnsNetId) {
          handle = handles[i];
          found = true;
          break;
        }
      }
      if (!found) {
        return [];
      }
      caps = connection.getNetCapabilitiesSync(handle);
      probe = emptyDnsNetProbe();
      probe.handleId = handle.netId;
      probe.networkCap = caps.networkCap ?? [];
      probe.bearerTypes = caps.bearerTypes ?? [];
      probe.capsOk = true;
      if (!isPhysicalNetEvidence(probe)) {
        return [];
      }
    } else {
      physicalDefaultDnsNetId = handle.netId;
    }
    readDnsResolvers(handle, probe);
    if (!probe.propertiesOk) {
      return [];
    }
    const derived: DnsDerivationResult = deriveDnsEndpointExclusions({
      defaultNet: probe, allNets: [], allNetsRead: false, apiError: ''
    });
    const servers: string[] = [];
    for (let i = 0; i < derived.endpoints.length; i++) {
      servers.push(derived.endpoints[i].host);
    }
    return servers;
  } catch (error) {
    return [];
  }
}

// N2-H v3.1 C1 dns: gather the PLATFORM FACTS the dns freeze caliber needs,
// then let the PURE module (NetBirdEndpointExclusion.deriveDnsEndpointExclusions)
// apply the preregistration's single branch order. This function performs the
// four synchronous `@ohos.net.connection` reads and NOTHING else:
//
//   step 1  getDefaultNetSync()             -> the default handle (netId 0 = none)
//   step 2  getNetCapabilitiesSync(handle)  -> NET_CAPABILITY_NOT_VPN present?
//           BEARER_VPN absent?  When both hold, THIS handle's
//           getConnectionPropertiesSync(handle).dnses[] is read and the
//           all-nets read below is SKIPPED entirely (the branch order is not
//           a choice to be made after seeing the data).
//   step 3  only when step 2 did not hold: getAllNetsSync() and the SAME
//           predicate per handle
//   step 4  an API error is captured as an `api-error…` token (the platform
//           throws BusinessError 201/2100002/2100003); the pure module turns
//           it — like every empty value — into a recorded dns gap. A gap is
//           NOT fatal in this increment (the fatal widening of the five
//           required kinds is a separate increment).
//
// Requires `ohos.permission.GET_NETWORK_INFO` (module.json5, normal /
// system_grant) — without it every call throws 201 and the derivation records
// the gap instead of freezing anything.
export function derivePlatformDnsEndpoints(): DnsDerivationResult {
  const input: DnsDerivationInput = {
    defaultNet: emptyDnsNetProbe(),
    allNets: [],
    allNetsRead: false,
    apiError: ''
  };
  let defaultHandle: connection.NetHandle | null = null;
  try {
    const handle: connection.NetHandle = connection.getDefaultNetSync();
    if (handle !== undefined && handle !== null && handle.netId !== 0) {
      defaultHandle = handle;
      input.defaultNet.handleId = handle.netId;
    }
  } catch (error) {
    input.apiError = dnsApiErrorToken(error as Error);
  }
  if (defaultHandle !== null) {
    try {
      const caps: connection.NetCapabilities = connection.getNetCapabilitiesSync(defaultHandle);
      input.defaultNet.networkCap = caps.networkCap !== undefined && caps.networkCap !== null
        ? caps.networkCap
        : [];
      input.defaultNet.bearerTypes = caps.bearerTypes !== undefined && caps.bearerTypes !== null
        ? caps.bearerTypes
        : [];
      input.defaultNet.capsOk = true;
    } catch (error) {
      if (input.apiError.length === 0) {
        input.apiError = dnsApiErrorToken(error as Error);
      }
    }
    if (isPhysicalNetEvidence(input.defaultNet)) {
      readDnsResolvers(defaultHandle, input.defaultNet);
      return deriveDnsEndpointExclusions(input); // step 2 held: the default branch
    }
  }
  // step 3 — reached ONLY because step 2 did not hold
  let handles: connection.NetHandle[] = [];
  try {
    const all: connection.NetHandle[] = connection.getAllNetsSync();
    handles = all !== undefined && all !== null ? all : [];
    input.allNetsRead = true;
  } catch (error) {
    if (input.apiError.length === 0) {
      input.apiError = dnsApiErrorToken(error as Error);
    }
    return deriveDnsEndpointExclusions(input);
  }
  for (let i = 0; i < handles.length; i++) {
    const handle: connection.NetHandle = handles[i];
    if (handle === undefined || handle === null || handle.netId === 0) {
      continue;
    }
    const probe: DnsNetProbe = emptyDnsNetProbe();
    probe.handleId = handle.netId;
    try {
      const caps: connection.NetCapabilities = connection.getNetCapabilitiesSync(handle);
      probe.networkCap = caps.networkCap !== undefined && caps.networkCap !== null
        ? caps.networkCap
        : [];
      probe.bearerTypes = caps.bearerTypes !== undefined && caps.bearerTypes !== null
        ? caps.bearerTypes
        : [];
      probe.capsOk = true;
    } catch (error) {
      if (input.apiError.length === 0) {
        input.apiError = dnsApiErrorToken(error as Error);
      }
      continue;
    }
    if (isPhysicalNetEvidence(probe)) {
      readDnsResolvers(handle, probe);
    }
    input.allNets.push(probe);
  }
  return deriveDnsEndpointExclusions(input);
}

// Read one handle's `dnses[]` into the probe (step 2/step 3 body). A read
// failure leaves `propertiesOk=false`, which the pure module records as a
// gap — the resolver list of a network that could not be read is never
// invented (and never inherited from another network).
function readDnsResolvers(handle: connection.NetHandle, probe: DnsNetProbe): void {
  try {
    const properties: connection.ConnectionProperties =
      connection.getConnectionPropertiesSync(handle);
    const raw: connection.NetAddress[] =
      properties !== undefined && properties !== null && properties.dnses !== undefined &&
        properties.dnses !== null
        ? properties.dnses
        : [];
    const dnses: DnsPlatformNetAddress[] = [];
    for (let i = 0; i < raw.length; i++) {
      const entry: connection.NetAddress = raw[i];
      dnses.push({
        address: entry.address,
        family: entry.family,
        port: entry.port
      });
    }
    probe.dnses = dnses;
    probe.propertiesOk = true;
  } catch (error) {
    probe.propertiesOk = false;
  }
}

// Stable, notes-safe token for a platform API failure: the BusinessError code
// when the platform reported one (201 permission denied, 2100002/2100003
// service errors), else a bare `api-error`. No error text ever enters the
// evidence document (log discipline; the token must stay in the frozen
// character shape the core validates).
function dnsApiErrorToken(error: Error): string {
  const code: number | undefined = (error as PlatformBusinessError).code;
  if (code !== undefined && code !== null && Number.isFinite(code)) {
    return `api-error-code-${code}`;
  }
  return 'api-error';
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

export function connectorLogout(): ConnectorLogoutResult {
  return JSON.parse(core.connector_logout()) as ConnectorLogoutResult;
}

// ---------------------------------------------------------------------------
// 2026-10-07 (decision B) route catalog getter — the `connector_routes` NAPI
// surface (client/core/src/connector.rs `connector_routes_json`; NAPI wrapper
// `napi_connector_routes` in client/core/src/napi.rs). The VPE is the ONLY
// caller; the UI never calls it (it reads the persisted catalog document
// offline through ./NetBirdRouteStore).
//
// ADDITIVE and READ-ONLY: a SEPARATE document from `connector_status()` /
// `connector_network_config()` (their frozen JSON and key order are NOT
// touched). It is the generation's startup selection verbatim plus the
// UNFILTERED advertised network catalog of the last applied NetworkMap — a
// user-disabled network (and every HA candidate behind it) stays visible with
// `enabled:false`. `available:false` + `serial:0` + `networks:[]` is the
// native "no route catalog" state (no connector, before the first map, after
// stop): the wrapper keeps it as such and never turns that honest state into
// the unreadable `null`, and never fabricates an empty-but-active catalog.
//
// EXACT native schema (copied field for field; public routing material only —
// NetIDs, prefixes and counts, never a key, an fd or a secret):
//   {"available":bool,"serial":n,
//    "selection":{"selected_routes":[..],"deselected_routes":[..],
//                 "deselect_all":bool},
//    "networks":[{"net_id":"..","networks":["a.b.c.d/len"],
//                 "candidate_count":n,"enabled":bool,"is_exit":bool}]}
//
// DEFENSIVE BY CONTRACT (the `connector_resource_status` pattern): the wrapper
// answers `null` — never a throw — on a missing export, a platform throw, an
// unparseable document or a structurally malformed one, and builds a FRESH
// object out of the declared fields, so an arbitrary extra field the native
// document may add is dropped rather than carried through. The projection
// checks the ESSENTIAL types only; the strict whole-document rules (length
// bounds, whitespace, duplicate NetIDs, prefix canonicalization) stay in the
// pure ./NetBirdRoutePreferences parser that the VPE's persist path already
// runs (`serializeRouteCatalogDocument` canonicalizes before any write) — this
// decoder does not copy that parser. Like the resource-status surface it logs
// NOTHING: no raw native payload and no field value.
// ---------------------------------------------------------------------------

// The un-validated parse shape: a malformed document may omit any field, so
// each one is optional until the projection has checked it.
interface RawRouteSelectionDoc {
  selected_routes?: string[] | null;
  deselected_routes?: string[] | null;
  deselect_all?: boolean;
}

interface RawRouteNetworkRow {
  net_id?: string;
  networks?: string[] | null;
  candidate_count?: number;
  enabled?: boolean;
  is_exit?: boolean;
}

interface RawRouteCatalogDoc {
  available?: boolean;
  serial?: number;
  selection?: RawRouteSelectionDoc | null;
  networks?: RawRouteNetworkRow[] | null;
}

// A type-valid string list kept as a COPY; a wrong-typed element (or a
// non-array) refuses the whole list — never a fabricated [].
function routeStringListOrNull(value: string[] | null | undefined): string[] | null {
  if (!Array.isArray(value)) {
    return null;
  }
  const list: string[] = [];
  for (let i: number = 0; i < value.length; i++) {
    const item: string = value[i];
    if (typeof item !== 'string') {
      return null;
    }
    list.push(item);
  }
  return list;
}

// null = not a selection object of the declared shape.
function routeSelectionOrNull(value: RawRouteSelectionDoc | null | undefined):
  RouteSelection | null {
  if (value === null || value === undefined || typeof value !== 'object' ||
    Array.isArray(value)) {
    return null;
  }
  const selected: string[] | null = routeStringListOrNull(value.selected_routes);
  const deselected: string[] | null = routeStringListOrNull(value.deselected_routes);
  if (selected === null || deselected === null || typeof value.deselect_all !== 'boolean') {
    return null;
  }
  return {
    selected_routes: selected,
    deselected_routes: deselected,
    deselect_all: value.deselect_all
  };
}

// null = structurally malformed: refuse the WHOLE document instead of
// half-believing it. `available:false` is a VALID catalog (see above) and is
// projected, never refused.
function projectRouteCatalog(doc: RawRouteCatalogDoc): RouteCatalog | null {
  if (doc === null || doc === undefined || typeof doc !== 'object' || Array.isArray(doc)) {
    return null;
  }
  if (typeof doc.available !== 'boolean') {
    return null;
  }
  // A serial is a map ordinal, not a generic count: a negative or fractional
  // one is a type anomaly, not a value to round or clamp.
  if (typeof doc.serial !== 'number' || !Number.isInteger(doc.serial) || doc.serial < 0) {
    return null;
  }
  const selection: RouteSelection | null = routeSelectionOrNull(doc.selection);
  if (selection === null) {
    return null;
  }
  if (!Array.isArray(doc.networks)) {
    return null;
  }
  const rawRows: (RawRouteNetworkRow | null | undefined)[] =
    doc.networks as (RawRouteNetworkRow | null | undefined)[];
  const networks: RouteNetworkEntry[] = [];
  for (let i: number = 0; i < rawRows.length; i++) {
    const row: RawRouteNetworkRow | null | undefined = rawRows[i];
    if (row === null || row === undefined || typeof row !== 'object' || Array.isArray(row)) {
      return null;
    }
    if (typeof row.net_id !== 'string') {
      return null;
    }
    const prefixes: string[] | null = routeStringListOrNull(row.networks);
    if (prefixes === null) {
      return null;
    }
    if (typeof row.candidate_count !== 'number' || !Number.isInteger(row.candidate_count) ||
      row.candidate_count < 0) {
      return null;
    }
    if (typeof row.enabled !== 'boolean' || typeof row.is_exit !== 'boolean') {
      return null;
    }
    networks.push({
      net_id: row.net_id,
      networks: prefixes,
      candidate_count: row.candidate_count,
      enabled: row.enabled,
      is_exit: row.is_exit
    });
  }
  return {
    available: doc.available,
    serial: doc.serial,
    selection: selection,
    networks: networks
  };
}

// null = the catalog could not be READ (missing export, platform throw,
// unparseable document or structurally malformed one). It is not a substitute
// for `available:false`, which is the honest "no route catalog yet" state.
export function connectorRoutes(): RouteCatalog | null {
  try {
    const doc: RawRouteCatalogDoc = JSON.parse(core.connector_routes()) as RawRouteCatalogDoc;
    return projectRouteCatalog(doc);
  } catch (error) {
    return null;
  }
}

// ---------------------------------------------------------------------------
// 2026-10-08 (Core 5f) runtime route-selection setter — the
// `connector_route_selection_set(selectionJson)` NAPI surface
// (client/core/src/connector.rs `connector_route_selection_set_json`; NAPI
// wrapper `napi_connector_route_selection_set` in client/core/src/napi.rs).
//
// ADDITIVE and separate from BOTH the startup `route_selection` config key of
// buildConnectorConfigJson and the persisted selection document: the document's
// full strict parser (length bounds, whitespace, canonicalization) stays in the
// pure ./NetBirdRoutePreferences module the VPE's load path already runs — this
// decoder does not copy it. The typed selection crosses as a real JSON OBJECT
// built out of its three DECLARED fields in declared order, never a
// string-encoded document; an extra key a caller's object may carry is dropped
// rather than crossing the bridge.
//
// EXACT native result schema (copied field for field; routing material only —
// NetIDs/prefixes, never a key, an fd or a secret):
//   {"ok":bool,"available":bool,"changed":bool,
//    "desired_routes":["a.b.c.d/len"],"recreate_needed":bool,
//    "error"?:"no-connector"|"selection-invalid"|"wg-apply-failed"}
//
// DEFENSIVE BY CONTRACT (the `connector_routes` pattern): the wrapper answers
// `null` — never a throw — on a missing export (an older core), a platform
// throw, an unparseable document or a structurally malformed one, and builds a
// FRESH object out of the declared fields, so an arbitrary extra field the
// native document may add is dropped rather than carried through. A well-formed
// `ok:false` + legal `error` token is a VALID answer and is preserved verbatim
// (never rewritten into `null`): the caller must be able to tell "the native
// refused this selection" from "the surface could not be read". Like the
// resource-status surface it logs NOTHING: no raw selector payload and no
// field value.
// ---------------------------------------------------------------------------

// The validated setter answer this side hands to callers. `error` is present
// only on a native refusal and is one of the three legal tokens.
export interface RouteSelectionSetResult {
  ok: boolean;
  error?: 'no-connector' | 'selection-invalid' | 'wg-apply-failed';
  available: boolean;
  changed: boolean;
  desired_routes: string[];
  recreate_needed: boolean;
}

// The un-validated parse shape: a malformed document may omit any field, so
// each one is optional until the projection has checked it.
interface RawRouteSelectionSetResult {
  ok?: boolean;
  error?: string;
  available?: boolean;
  changed?: boolean;
  desired_routes?: string[] | null;
  recreate_needed?: boolean;
}

// The `error` leaf, narrowed to the declared token union; null = not a legal
// token (an `ok:false` document without one is malformed, not a refusal this
// side may guess at).
function routeSelectionSetErrorOrNull(value: string | undefined):
  'no-connector' | 'selection-invalid' | 'wg-apply-failed' | null {
  if (value === 'no-connector' || value === 'selection-invalid' ||
    value === 'wg-apply-failed') {
    return value;
  }
  return null;
}

// null = structurally malformed: refuse the WHOLE document instead of
// half-believing it. `ok:true` and a legal `ok:false` are both projected.
function projectRouteSelectionSet(doc: RawRouteSelectionSetResult):
  RouteSelectionSetResult | null {
  if (doc === null || doc === undefined || typeof doc !== 'object' || Array.isArray(doc)) {
    return null;
  }
  if (typeof doc.ok !== 'boolean' || typeof doc.available !== 'boolean' ||
    typeof doc.changed !== 'boolean' || typeof doc.recreate_needed !== 'boolean') {
    return null;
  }
  const desired: string[] | null = routeStringListOrNull(doc.desired_routes);
  if (desired === null) {
    return null;
  }
  if (doc.ok) {
    return {
      ok: true,
      available: doc.available,
      changed: doc.changed,
      desired_routes: desired,
      recreate_needed: doc.recreate_needed
    };
  }
  const error: 'no-connector' | 'selection-invalid' | 'wg-apply-failed' | null =
    routeSelectionSetErrorOrNull(doc.error);
  if (error === null) {
    return null;
  }
  return {
    ok: false,
    error: error,
    available: doc.available,
    changed: doc.changed,
    desired_routes: desired,
    recreate_needed: doc.recreate_needed
  };
}

// The runtime setter the route-selection page calls after the user's toggle
// (the atomic write already happened; this only asks the live connector to
// adopt the selection). null = the setter could not be READ (missing export,
// platform throw, unparseable document or structurally malformed one) — it is
// never a substitute for a native `ok:false`, which crosses as such.
export function connectorRouteSelectionSet(selection: RouteSelection):
  RouteSelectionSetResult | null {
  try {
    const payload: RouteSelection = {
      selected_routes: selection.selected_routes,
      deselected_routes: selection.deselected_routes,
      deselect_all: selection.deselect_all
    };
    const doc: RawRouteSelectionSetResult =
      JSON.parse(core.connector_route_selection_set(JSON.stringify(payload))) as
        RawRouteSelectionSetResult;
    return projectRouteSelectionSet(doc);
  } catch (error) {
    return null;
  }
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

// ---------------------------------------------------------------------------
// 2026-10-08 (006) native resource snapshot — the `connector_resource_status`
// NAPI surface (client/core/src/connector.rs `connector_resource_status_json`;
// NAPI wrapper `napi_connector_resource_status` in client/core/src/napi.rs).
//
// ADDITIVE and READ-ONLY: a SEPARATE document from `connector_status()` (whose
// frozen JSON/order is NOT touched) reporting the counts core still OWNS. The
// native probe takes every leaf with a NON-BLOCKING try_lock and renders a busy
// leaf as JSON `null` (UNKNOWN), never a false 0; `present:false` is a KNOWN
// disposed state in which every connector-owned count is an honest zero.
//
// EXACT native schema (copied field for field; counts only — no peer key,
// relay URL, fd number, request id or secret):
//   {"ok":true,
//    "connector":{"present":bool|null,"generation":n|null,"running":bool|null,
//                 "state":"connecting"|...|null,"starts_total":n,"stops_total":n},
//    "ice":{"available":bool,"sessions_present":n|null,"sessions_live":n|null,
//           "sessions_stopped":n|null,"live_local_sockets":n|null},
//    "ice_sockets":{"available":bool,"queued":n|null,"taken":n|null},
//    "socket_supply":{"installed":bool|null,"pending":n|null,"queued":n|null},
//    "relay_carrier":{"available":bool,"attached_lanes":n|null,
//                     "pending_opens":n|null,"foreign_clients":n|null},
//    "dns_lookups":{"pending_receivers":n|null},
//    "wg":{"available":false,"reason":"not-collected"}}
//
// DEFENSIVE BY CONTRACT — a diagnostics probe must never be able to break the
// VPN: the wrapper is wrapped and answers `null` (never a throw) on a missing
// export, a platform throw, an unparseable document or a structurally
// malformed one. The projection builds a FRESH object out of the declared
// fields, so an arbitrary extra field the native document may add is dropped
// rather than carried through. `wg` is deliberately not declared: that native
// section is the constant "not-collected" marker, not a resource count.
// ---------------------------------------------------------------------------

export interface NativeConnectorCounts {
  // null = the global connector slot was busy (presence itself unknown).
  // `false` = no connector installed — a KNOWN disposed state whose counts are
  // honest zeros; `true` = a handle exists. Never conflated.
  present: boolean | null;
  // The handle's successful-start ordinal (an identity, NOT a live thread
  // count); null = no handle exists.
  generation: number | null;
  running: boolean | null;
  // The shared-state token ('connecting'|...); null = the leaf was busy (or no
  // handle). A controlled class token only.
  state: string | null;
  // Native contract: always numbers (process-cumulative — they outlive the
  // handle). Declared nullable like every other leaf so a type-anomalous
  // document renders the explicit `na` instead of a fabricated 0.
  starts_total: number | null;
  stops_total: number | null;
}

export interface NativeIceCounts {
  available: boolean | null;
  sessions_present: number | null;
  sessions_live: number | null;
  sessions_stopped: number | null;
  // fds (fd >= 0) the sessions still own, summed over all retained sessions.
  live_local_sockets: number | null;
}

export interface NativeIceSocketCounts {
  available: boolean | null;
  // The protected-ICE queue depth (queued) and the cumulative handed-out
  // count (taken) of the shell-fed UDP source.
  queued: number | null;
  taken: number | null;
}

export interface NativeSocketSupplyCounts {
  installed: boolean | null;
  // The GLOBAL per-dial bridge: requests awaiting a protect reply (pending)
  // and requests not yet polled (queued).
  pending: number | null;
  queued: number | null;
}

export interface NativeRelayCarrierCounts {
  available: boolean | null;
  attached_lanes: number | null;
  pending_opens: number | null;
  foreign_clients: number | null;
}

export interface NativeDnsLookupCounts {
  // DNS requests whose oneshot receiver is still registered (unpolled).
  pending_receivers: number | null;
}

// The validated snapshot this side hands to callers: every section present,
// every leaf of the schema's type (or null = UNKNOWN, rendered `na`).
export interface ConnectorResourceStatus {
  ok: boolean;
  connector: NativeConnectorCounts;
  ice: NativeIceCounts;
  ice_sockets: NativeIceSocketCounts;
  socket_supply: NativeSocketSupplyCounts;
  relay_carrier: NativeRelayCarrierCounts;
  dns_lookups: NativeDnsLookupCounts;
}

// The un-validated parse shape: a malformed document may omit any section, so
// each is optional until the projection has checked it.
interface RawConnectorResourceStatus {
  ok?: boolean;
  connector?: NativeConnectorCounts;
  ice?: NativeIceCounts;
  ice_sockets?: NativeIceSocketCounts;
  socket_supply?: NativeSocketSupplyCounts;
  relay_carrier?: NativeRelayCarrierCounts;
  dns_lookups?: NativeDnsLookupCounts;
}

// A type-valid leaf kept as-is; a JSON null / absent / type-anomalous leaf is
// the explicit UNKNOWN (null), never a fabricated value.
function nativeFlagOrNull(value: boolean | null | undefined): boolean | null {
  return typeof value === 'boolean' ? value : null;
}

function nativeCountOrNull(value: number | null | undefined): number | null {
  // A finite non-negative number is kept VERBATIM (never rounded): a value that
  // is not a count at all stays visible as itself instead of being fabricated
  // into a plausible integer.
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
}

function nativeStateOrNull(value: string | null | undefined): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function projectConnectorResourceStatus(doc: RawConnectorResourceStatus):
  ConnectorResourceStatus | null {
  // The document identity itself is refused when it is not a boolean: without
  // it the parse cannot be trusted at all.
  if (doc === null || doc === undefined || typeof doc.ok !== 'boolean') {
    return null;
  }
  const connector: NativeConnectorCounts | undefined = doc.connector;
  const ice: NativeIceCounts | undefined = doc.ice;
  const iceSockets: NativeIceSocketCounts | undefined = doc.ice_sockets;
  const supply: NativeSocketSupplyCounts | undefined = doc.socket_supply;
  const relay: NativeRelayCarrierCounts | undefined = doc.relay_carrier;
  const dns: NativeDnsLookupCounts | undefined = doc.dns_lookups;
  // A missing / non-object section is a structurally malformed document:
  // refuse the whole snapshot instead of half-believing it.
  if (connector === undefined || connector === null || typeof connector !== 'object' ||
    ice === undefined || ice === null || typeof ice !== 'object' ||
    iceSockets === undefined || iceSockets === null || typeof iceSockets !== 'object' ||
    supply === undefined || supply === null || typeof supply !== 'object' ||
    relay === undefined || relay === null || typeof relay !== 'object' ||
    dns === undefined || dns === null || typeof dns !== 'object') {
    return null;
  }
  return {
    ok: doc.ok,
    connector: {
      present: nativeFlagOrNull(connector.present),
      generation: nativeCountOrNull(connector.generation),
      running: nativeFlagOrNull(connector.running),
      state: nativeStateOrNull(connector.state),
      starts_total: nativeCountOrNull(connector.starts_total),
      stops_total: nativeCountOrNull(connector.stops_total)
    },
    ice: {
      available: nativeFlagOrNull(ice.available),
      sessions_present: nativeCountOrNull(ice.sessions_present),
      sessions_live: nativeCountOrNull(ice.sessions_live),
      sessions_stopped: nativeCountOrNull(ice.sessions_stopped),
      live_local_sockets: nativeCountOrNull(ice.live_local_sockets)
    },
    ice_sockets: {
      available: nativeFlagOrNull(iceSockets.available),
      queued: nativeCountOrNull(iceSockets.queued),
      taken: nativeCountOrNull(iceSockets.taken)
    },
    socket_supply: {
      installed: nativeFlagOrNull(supply.installed),
      pending: nativeCountOrNull(supply.pending),
      queued: nativeCountOrNull(supply.queued)
    },
    relay_carrier: {
      available: nativeFlagOrNull(relay.available),
      attached_lanes: nativeCountOrNull(relay.attached_lanes),
      pending_opens: nativeCountOrNull(relay.pending_opens),
      foreign_clients: nativeCountOrNull(relay.foreign_clients)
    },
    dns_lookups: {
      pending_receivers: nativeCountOrNull(dns.pending_receivers)
    }
  };
}

// null = the snapshot could not be read (missing export, platform throw,
// unparseable document or structurally malformed one): callers render the
// explicit `na` verdict instead of inventing zeroed counts.
export function connectorResourceStatus(): ConnectorResourceStatus | null {
  try {
    const doc: RawConnectorResourceStatus =
      JSON.parse(core.connector_resource_status()) as RawConnectorResourceStatus;
    return projectConnectorResourceStatus(doc);
  } catch (error) {
    return null;
  }
}

// The 22 scalar leaves of `connector_resource_status()`, each already rendered
// as a diagnostics-safe token: a finite non-negative number keeps its decimal
// value, `true`/`false` keep their boolean meaning, and a JSON null (the native
// "busy leaf = unknown") renders `na` — never a fabricated 0 or false.
export interface NativeResourceSample {
  conn_present: string;
  conn_gen: string;
  conn_running: string;
  conn_state: string;
  conn_starts: string;
  conn_stops: string;
  ice_available: string;
  ice_sessions: string;
  ice_live: string;
  ice_stopped: string;
  ice_fds: string;
  icesock_ready: string;
  icesock_queued: string;
  icesock_taken: string;
  supply_installed: string;
  supply_pending: string;
  supply_queued: string;
  relay_ready: string;
  relay_lanes: string;
  relay_opens: string;
  relay_foreign: string;
  dns_pending: string;
}

const NATIVE_RESOURCE_NA: string = 'na';

function nativeCountText(value: number | null | undefined): string {
  // A finite non-negative number is rendered VERBATIM (never rounded); every
  // anomaly — JSON null (busy leaf), absent, wrong type, negative, non-finite —
  // is the explicit `na`, never a fabricated 0.
  if (typeof value === 'number' && Number.isFinite(value) && value >= 0) {
    return String(value);
  }
  return NATIVE_RESOURCE_NA;
}

function nativeFlagText(value: boolean | null | undefined): string {
  if (value === true) { return 'true'; }
  if (value === false) { return 'false'; }
  return NATIVE_RESOURCE_NA;
}

function nativeStateText(value: string | null | undefined): string {
  if (typeof value === 'string' && value.length > 0) { return value; }
  return NATIVE_RESOURCE_NA;
}

// Render the whole snapshot (or an unreadable null) into the flat token set
// the `native_resource_sample` event carries. An unreadable snapshot renders
// every leaf `na`: an explicit "not measured", never a row of fabricated
// zeros.
export function nativeResourceSample(status: ConnectorResourceStatus | null): NativeResourceSample {
  const sample: NativeResourceSample = {
    conn_present: NATIVE_RESOURCE_NA,
    conn_gen: NATIVE_RESOURCE_NA,
    conn_running: NATIVE_RESOURCE_NA,
    conn_state: NATIVE_RESOURCE_NA,
    conn_starts: NATIVE_RESOURCE_NA,
    conn_stops: NATIVE_RESOURCE_NA,
    ice_available: NATIVE_RESOURCE_NA,
    ice_sessions: NATIVE_RESOURCE_NA,
    ice_live: NATIVE_RESOURCE_NA,
    ice_stopped: NATIVE_RESOURCE_NA,
    ice_fds: NATIVE_RESOURCE_NA,
    icesock_ready: NATIVE_RESOURCE_NA,
    icesock_queued: NATIVE_RESOURCE_NA,
    icesock_taken: NATIVE_RESOURCE_NA,
    supply_installed: NATIVE_RESOURCE_NA,
    supply_pending: NATIVE_RESOURCE_NA,
    supply_queued: NATIVE_RESOURCE_NA,
    relay_ready: NATIVE_RESOURCE_NA,
    relay_lanes: NATIVE_RESOURCE_NA,
    relay_opens: NATIVE_RESOURCE_NA,
    relay_foreign: NATIVE_RESOURCE_NA,
    dns_pending: NATIVE_RESOURCE_NA
  };
  if (status === null || status === undefined) {
    return sample;
  }
  const c: NativeConnectorCounts = status.connector;
  const ice: NativeIceCounts = status.ice;
  const iceSockets: NativeIceSocketCounts = status.ice_sockets;
  const supply: NativeSocketSupplyCounts = status.socket_supply;
  const relay: NativeRelayCarrierCounts = status.relay_carrier;
  const dns: NativeDnsLookupCounts = status.dns_lookups;
  sample.conn_present = nativeFlagText(c.present);
  sample.conn_gen = nativeCountText(c.generation);
  sample.conn_running = nativeFlagText(c.running);
  sample.conn_state = nativeStateText(c.state);
  sample.conn_starts = nativeCountText(c.starts_total);
  sample.conn_stops = nativeCountText(c.stops_total);
  sample.ice_available = nativeFlagText(ice.available);
  sample.ice_sessions = nativeCountText(ice.sessions_present);
  sample.ice_live = nativeCountText(ice.sessions_live);
  sample.ice_stopped = nativeCountText(ice.sessions_stopped);
  sample.ice_fds = nativeCountText(ice.live_local_sockets);
  sample.icesock_ready = nativeFlagText(iceSockets.available);
  sample.icesock_queued = nativeCountText(iceSockets.queued);
  sample.icesock_taken = nativeCountText(iceSockets.taken);
  sample.supply_installed = nativeFlagText(supply.installed);
  sample.supply_pending = nativeCountText(supply.pending);
  sample.supply_queued = nativeCountText(supply.queued);
  sample.relay_ready = nativeFlagText(relay.available);
  sample.relay_lanes = nativeCountText(relay.attached_lanes);
  sample.relay_opens = nativeCountText(relay.pending_opens);
  sample.relay_foreign = nativeCountText(relay.foreign_clients);
  sample.dns_pending = nativeCountText(dns.pending_receivers);
  return sample;
}

// ---------------------------------------------------------------------------
// 2026-10-08 phone-side persistent log — the `phone_log_*` NAPI surface
// (client/core/src/phone_log.rs; NAPI wrappers in client/core/src/napi.rs).
//
// The native module ALREADY tees every NATIVE event into
// filesDir/netbird-vpn.log (2 MiB x 4 segments, its own rotation and its own
// redaction); these four wrappers are the ArkTS-facing counterpart, not a
// second forwarder:
//   phone_log_configure(path) — install/replace the file tee. ONCE PER PATH:
//     a repeat on the installed path answers `idempotent:true` and starts NO
//     second writer (two writers on one file would interleave records and race
//     the rotation); a disabled-but-draining writer answers
//     `error:'shutdown-pending'` and must be retried later.
//   phone_log_append(line)    — hand ONE shell line to the tee. NEVER blocks
//     and does NO disk I/O on this stack. `ok:true` means "accepted at
//     ingress", NOT "on disk": the native module admits lines through a
//     per-second budget and a bounded queue and DROPS the overflow by design
//     (counted in `dropped`; the budget part in `rate_suppressed`). An
//     unconfigured/stopped tee answers `not-configured` instead of buffering.
//   phone_log_status()        — the writer's own counters (the only view of
//     records that were refused or never reached the OS).
//   phone_log_shutdown()      — a bounded (<= 5 ms) stop REQUEST. Provided for
//     completeness; the VPN shell deliberately does NOT call it (see the
//     process-lifetime note in NetBirdVpnExtensionAbility): a stop request can
//     only race the closing records of an asynchronous diagnostics drain.
//
// DEFENSIVE BY CONTRACT — a diagnostics log must never be able to break the
// VPN: every wrapper is wrapped, answers `{ok:false,error:<code>}` / null on a
// missing export, a platform throw or an unparseable reply, and NEVER throws
// into its caller. No reply echoes the argument, so no path or line content is
// logged from here.
//
// The native reply documents also carry `path` and `session`. They are
// deliberately NOT declared below: nothing on this side needs the raw path or
// the session token, and a field that is not declared cannot be printed by
// accident.
// ---------------------------------------------------------------------------

export interface PhoneLogConfigureResult {
  ok: boolean;
  // true = the requested path was already installed: same writer, same session.
  idempotent?: boolean;
  enabled?: boolean;
  running?: boolean;
  // stable code only ('invalid-path' | 'shutdown-pending' | '<native code>'),
  // never the path or the line
  error?: string;
}

export interface PhoneLogAppendResult {
  ok: boolean;
  error?: string;
}

export interface PhoneLogShutdownResult {
  ok: boolean;
  // stopped = the writer confirmed it drained, closed and exited
  stopped?: boolean;
  running?: boolean;
  // accepted records not yet handed to the OS at the time of the request
  pending?: number;
  error?: string;
}

// The counters the shell reads out of `phone_log_status()`. Same omission rule
// as above: no `path`, no `session`.
export interface PhoneLogStatusResult {
  component: string;
  enabled: boolean;
  running: boolean;
  offered: number;
  accepted: number;
  written: number;
  dropped: number;
  rate_suppressed: number;
  failed: number;
  pending: number;
  rotations: number;
  last_error: string | null;
}

// Stable token for a failed native call: the platform BusinessError code when
// the platform reported one, else `native-unavailable` for a missing/absent
// export (the .so without this surface) and a bare `native-throw` otherwise.
// Never passes an error message through (log discipline).
function phoneLogFailureToken(error: Error): string {
  const code: number | undefined = (error as PlatformBusinessError).code;
  if (code !== undefined && code !== null && Number.isFinite(code)) {
    return `platform-error-${code}`;
  }
  if (error instanceof TypeError) {
    return 'native-unavailable';
  }
  return 'native-throw';
}

export function phoneLogConfigure(path: string): PhoneLogConfigureResult {
  try {
    if (path.length === 0) {
      return { ok: false, error: 'empty-path' };
    }
    const reply: PhoneLogConfigureResult =
      JSON.parse(core.phone_log_configure(path)) as PhoneLogConfigureResult;
    if (reply === null || reply === undefined || typeof reply.ok !== 'boolean') {
      return { ok: false, error: 'malformed-reply' };
    }
    // PROJECTED onto the declared fields: the native document also carries
    // `path` and `session`, and a value that never enters this object cannot be
    // printed by accident (log discipline). A boolean that is absent stays
    // absent — never normalized to a fabricated false.
    const projected: PhoneLogConfigureResult = { ok: reply.ok };
    if (typeof reply.idempotent === 'boolean') { projected.idempotent = reply.idempotent; }
    if (typeof reply.enabled === 'boolean') { projected.enabled = reply.enabled; }
    if (typeof reply.running === 'boolean') { projected.running = reply.running; }
    if (typeof reply.error === 'string') { projected.error = reply.error; }
    return projected;
  } catch (error) {
    return { ok: false, error: phoneLogFailureToken(error as Error) };
  }
}

export function phoneLogAppend(line: string): PhoneLogAppendResult {
  try {
    const reply: PhoneLogAppendResult =
      JSON.parse(core.phone_log_append(line)) as PhoneLogAppendResult;
    if (reply === null || reply === undefined || typeof reply.ok !== 'boolean') {
      return { ok: false, error: 'malformed-reply' };
    }
    return reply;
  } catch (error) {
    return { ok: false, error: phoneLogFailureToken(error as Error) };
  }
}

// null = the writer's status could not be read (missing export, platform
// throw, unparseable document): callers render the explicit `unavailable`
// verdict instead of inventing zeroed counters. The reply is PROJECTED onto
// the declared counters (no `path`, no `session`), and a type-anomalous counter
// is carried through unchanged so the summary can render its explicit `na`
// instead of a fabricated 0.
export function phoneLogStatus(): PhoneLogStatusResult | null {
  try {
    const reply: PhoneLogStatusResult =
      JSON.parse(core.phone_log_status()) as PhoneLogStatusResult;
    if (reply === null || reply === undefined || typeof reply.component !== 'string' ||
      reply.component.length === 0) {
      return null;
    }
    const projected: PhoneLogStatusResult = {
      component: reply.component,
      enabled: reply.enabled,
      running: reply.running,
      offered: reply.offered,
      accepted: reply.accepted,
      written: reply.written,
      dropped: reply.dropped,
      rate_suppressed: reply.rate_suppressed,
      failed: reply.failed,
      pending: reply.pending,
      rotations: reply.rotations,
      last_error: reply.last_error
    };
    return projected;
  } catch (error) {
    return null;
  }
}

export function phoneLogShutdown(): PhoneLogShutdownResult {
  try {
    const reply: PhoneLogShutdownResult =
      JSON.parse(core.phone_log_shutdown()) as PhoneLogShutdownResult;
    if (reply === null || reply === undefined || typeof reply.ok !== 'boolean') {
      return { ok: false, error: 'malformed-reply' };
    }
    return reply;
  } catch (error) {
    return { ok: false, error: phoneLogFailureToken(error as Error) };
  }
}

// ---------------------------------------------------------------------------
// 2026-10-08 Rust-heap readout bridge (`alloc_stats`).
//
// The native connector's ADDITIVE read-only counter snapshot of the process's
// RUST global allocator (client/core/src/alloc_stats.rs; the NAPI export is
// `alloc_stats()`, zero arguments, synchronous). It answers one question for
// the VPN process: how much memory the Rust heap holds right now — NOT process
// RSS, NOT the ArkTS/JS heap, libace_napi or any C/C++ platform buffer.
//
// The wrapper NEVER throws: a missing export (a .so without this surface), a
// platform throw, an empty reply or a garbled one (not the four-counter JSON
// object) answers `undefined`, and the diagnostics sampler writes its explicit
// `na`. Only the SHAPE is checked here; the KiB conversion and the `na`
// rendering live in the pure diagnostics core (buildRustAllocFields), which is
// host-testable and re-parses this text. The document carries counters only —
// no path, no key, no session material.
// ---------------------------------------------------------------------------

const ALLOC_STAT_KEYS: string[] = ['bytes', 'peak', 'allocs', 'frees'];

export function nativeAllocStats(): string | undefined {
  try {
    const text: string = core.alloc_stats();
    if (typeof text !== 'string' || text.length === 0) {
      return undefined;
    }
    const doc: Record<string, Object> = JSON.parse(text) as Record<string, Object>;
    if (doc === null || typeof doc !== 'object') {
      return undefined;
    }
    for (let i = 0; i < ALLOC_STAT_KEYS.length; i++) {
      const value: Object = doc[ALLOC_STAT_KEYS[i]];
      if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
        return undefined; // a missing/anomalous counter is not a reading
      }
    }
    return text;
  } catch (error) {
    return undefined; // absent export, platform failure or unparseable reply
  }
}

// ---------------------------------------------------------------------------
// The once-per-minute `phone_log_status` summary a shell watcher logs.
//
// The value of this record is the ADMISSION/queue truth the persisted records
// themselves cannot show: how many lines the native module refused at ingress
// (`dropped`), how many accepted lines had not reached the OS yet (`pending`)
// and how many were lost to write errors (`failed`). Only those three counters
// plus a verdict leave this function — never the raw path or the session token
// (neither is even declared on PhoneLogStatusResult).
//
// `state`: 'ok' | 'degraded' (a drop or a write failure was counted) |
// 'stopped' (the tee is off or the writer is not running: nothing is being
// recorded) | 'unavailable' (no usable status document). An unreadable counter
// renders the explicit `na` token, never a fabricated 0.
// ---------------------------------------------------------------------------

const PHONE_LOG_NA: string = 'na';

export interface PhoneLogSummary {
  state: string;
  detail: string;
}

function phoneLogCounterText(value: number | undefined | null): string {
  if (typeof value === 'number' && Number.isFinite(value) && value >= 0) {
    return String(Math.floor(value));
  }
  return PHONE_LOG_NA;
}

function phoneLogDetail(dropped: string, pending: string, failed: string): string {
  return `dropped=${dropped},pending=${pending},failed=${failed}`;
}

export function phoneLogSummary(status: PhoneLogStatusResult | null): PhoneLogSummary {
  if (status === null || status === undefined) {
    return { state: 'unavailable',
      detail: phoneLogDetail(PHONE_LOG_NA, PHONE_LOG_NA, PHONE_LOG_NA) };
  }
  const dropped: string = phoneLogCounterText(status.dropped);
  const pending: string = phoneLogCounterText(status.pending);
  const failed: string = phoneLogCounterText(status.failed);
  const detail: string = phoneLogDetail(dropped, pending, failed);
  if (status.enabled !== true || status.running !== true) {
    return { state: 'stopped', detail: detail };
  }
  const droppedCount: number = typeof status.dropped === 'number' ? status.dropped : 0;
  const failedCount: number = typeof status.failed === 'number' ? status.failed : 0;
  if (droppedCount > 0 || failedCount > 0) {
    return { state: 'degraded', detail: detail };
  }
  return { state: 'ok', detail: detail };
}
