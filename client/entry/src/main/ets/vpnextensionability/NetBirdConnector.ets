import fs from '@ohos.file.fs';
import hilog from '@ohos.hilog';
import core from 'libnetbird_core.so';

// N3-6: ArkTS-side wrapper around the synchronous connector_* NAPI exports
// of libnetbird_core.so (JSON string in / JSON string out; no callbacks).
// NAPI contract: client/core/src/lib.rs NAPI surface table +
// client/core/src/napi.rs / connector.rs (authoritative field names).
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

// Build the core.connector_start configJson (contract: ConnectorConfig::
// from_json in client/core/src/connector.rs). The private key crosses this
// boundary as config material — never logged, never echoed.
export function buildConnectorConfigJson(device: DeviceConfigFile): string {
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
  return JSON.stringify(cfg);
}

export function connectorStart(configJson: string, setupKeyJson: string): ConnectorStartResult {
  return JSON.parse(core.connector_start(configJson, setupKeyJson)) as ConnectorStartResult;
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
