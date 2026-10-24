// First-run setup / login form — PURE LOGIC only (input validation and
// credential-file content construction). The page (pages/Index.ets) owns the
// fs writes into the SAME files the development-only --ps channel writes
// (EntryAbility) and the VpnExtensionAbility credential reader reads:
// filesDir/netbird-device-config.json + filesDir/netbird-setup-key. No
// @ohos imports in this file: the host-side selfcheck imports it
// byte-identical (client/entry/selfcheck/run-setup-config.mts).
//
// Logging discipline (enforced by the page, restated here): the setup key
// and the device private key are SECRET — they never appear in logs, only
// file names + byte counts.

// Users supply their own management server; the client provides no network service.
export const DEFAULT_MANAGEMENT_URL: string = '';

// Exact credential basenames shared by EntryAbility (--ps channel) and the
// extension's readCredentialFileText: the UI writes the very same files,
// never a new protocol.
export const SETUP_CONFIG_FILE: string = 'netbird-device-config.json';
export const SETUP_KEY_FILE: string = 'netbird-setup-key';

// Loose setup-key shape: NetBird setup keys are UUID-style hex
// (8-4-4-4-12, dashes optional, case-insensitive). LOOSE by design — a
// non-matching key only produces a HINT, never a refusal; the management
// server stays the sole authority.
const SETUP_KEY_SHAPE_DASHED: RegExp =
  /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/;
const SETUP_KEY_SHAPE_DASHLESS: RegExp = /^[0-9a-fA-F]{32}$/;

export interface UrlCheck {
  ok: boolean;
  error: string; // blocking, human-readable Chinese ('' when ok)
  url: string;   // normalized https://host[:port] ('' when !ok)
}

export interface KeyCheck {
  ok: boolean;
  error: string; // blocking, human-readable Chinese ('' when ok)
  hint: string;  // non-blocking hint ('' when nothing to say)
}

// Validate + normalize the management URL. Hard requirements: https scheme,
// parseable non-empty host, numeric in-range port when present. Normalized
// output keeps scheme://host[:port] only — the core dials exactly that
// shape (no path/query), so a pasted URL with a trailing path still
// configures cleanly.
export function checkManagementUrl(raw: string): UrlCheck {
  const input: string = raw.trim();
  if (input.length === 0) {
    return { ok: false, error: '请输入管理服务器地址', url: '' };
  }
  const schemeEnd: number = input.indexOf('://');
  if (schemeEnd < 0) {
    return { ok: false, error: '地址缺少协议，请以 https:// 开头', url: '' };
  }
  const scheme: string = input.substring(0, schemeEnd).toLowerCase();
  if (scheme !== 'https') {
    return { ok: false, error: `管理服务器仅支持 https:// 地址（当前为 ${scheme}://）`, url: '' };
  }
  let hostPart: string = input.substring(schemeEnd + 3);
  // host[:port] ends at the first path / query / fragment separator
  const slashAt: number = hostPart.indexOf('/');
  const queryAt: number = hostPart.indexOf('?');
  const fragAt: number = hostPart.indexOf('#');
  let cut: number = hostPart.length;
  if (slashAt >= 0 && slashAt < cut) {
    cut = slashAt;
  }
  if (queryAt >= 0 && queryAt < cut) {
    cut = queryAt;
  }
  if (fragAt >= 0 && fragAt < cut) {
    cut = fragAt;
  }
  hostPart = hostPart.substring(0, cut);
  if (hostPart.length === 0) {
    return { ok: false, error: '地址缺少主机名，例如 https://api.netbird.io:443', url: '' };
  }
  if (hostPart.indexOf('@') >= 0 || /\s/.test(hostPart)) {
    return { ok: false, error: '主机名包含非法字符，请检查地址', url: '' };
  }
  let host: string = hostPart;
  let port: string = '';
  const colonAt: number = hostPart.indexOf(':');
  if (colonAt >= 0) {
    host = hostPart.substring(0, colonAt);
    port = hostPart.substring(colonAt + 1);
    if (host.length === 0) {
      return { ok: false, error: '地址缺少主机名，例如 https://api.netbird.io:443', url: '' };
    }
    if (!/^[0-9]+$/.test(port)) {
      return { ok: false, error: '端口必须是数字（例如 :443）', url: '' };
    }
    const portNum: number = Number(port);
    if (portNum < 1 || portNum > 65535) {
      return { ok: false, error: '端口超出范围（1-65535）', url: '' };
    }
  }
  const url: string = port.length > 0 ? `https://${host.toLowerCase()}:${port}` :
    `https://${host.toLowerCase()}`;
  return { ok: true, error: '', url: url };
}

// Validate the setup key. Hard rule: non-empty. Everything else is a loose
// hint only (see SETUP_KEY_SHAPE above).
export function checkSetupKey(raw: string): KeyCheck {
  const key: string = raw.trim();
  if (key.length === 0) {
    return { ok: false, error: '请输入 Setup Key', hint: '' };
  }
  if (SETUP_KEY_SHAPE_DASHED.test(key) || SETUP_KEY_SHAPE_DASHLESS.test(key)) {
    return { ok: true, error: '', hint: '' };
  }
  return { ok: true, error: '', hint: '该 Key 不是常见的 UUID 样式（如 6dc45f77-5f0d-4a0f-8a75-9df5b6a04bbb），请核对后再保存' };
}

// Content for netbird-device-config.json — the SAME shape the extension's
// parseDeviceConfig reads (management_url + private_key required; ca_pem
// stays optional and is injected by the extension for https when absent).
// privateKeyB64: base64 std of 32 bytes (the x25519/WireGuard device
// identity — a fresh random one generated by the page, or the existing one
// reused on reconfigure).
// relayEnabled: relay client switch carried into the file. true for a fresh
// configure (relay ON by default — official client parity); on reconfigure
// the page passes the value read back by readStoredRelayEnabled so an
// explicit relay_enabled:false survives the rewrite.
export function buildDeviceConfigJson(managementUrl: string, privateKeyB64: string,
  relayEnabled: boolean = true): string {
  const device: Record<string, Object> = {
    'management_url': managementUrl,
    'private_key': privateKeyB64,
    'relay_enabled': relayEnabled
  };
  return `${JSON.stringify(device)}\n`;
}

// Content for netbird-setup-key — byte-identical to the --ps channel's
// writeFile(SETUP_KEY_FILE, `${setupKey}\n`).
export function buildSetupKeyFileText(setupKey: string): string {
  return `${setupKey.trim()}\n`;
}

interface StoredDeviceConfig {
  management_url?: string;
  private_key?: string;
  relay_enabled?: boolean;
}

// Read the management URL back out of an existing netbird-device-config.json
// for display. ANY parse/validation failure returns '' — the caller treats
// '' as "not configured" and shows the first-run form.
export function readStoredManagementUrl(configText: string): string {
  try {
    const device: StoredDeviceConfig = JSON.parse(configText) as StoredDeviceConfig;
    if (device.management_url !== undefined && device.management_url.length > 0) {
      return device.management_url;
    }
  } catch (error) {
    // unreadable config = not configured (fall through to '')
  }
  return '';
}

// Existing device private key for identity reuse on reconfigure ('' = none
// stored → the page generates a fresh one). SECRET: the caller must never
// log this value.
export function readStoredPrivateKey(configText: string): string {
  try {
    const device: StoredDeviceConfig = JSON.parse(configText) as StoredDeviceConfig;
    if (device.private_key !== undefined && device.private_key.length > 0) {
      return device.private_key;
    }
  } catch (error) {
    // unreadable config = no reusable identity (fall through to '')
  }
  return '';
}

// Stored relay switch for reconfigure: ONLY the exact boolean false reads
// back false (the explicit opt-out must survive a reconfigure rewrite);
// missing/true/malformed/unreadable reads back true — the enabled default
// the extension's parseDeviceConfig also applies.
export function readStoredRelayEnabled(configText: string): boolean {
  try {
    const device: StoredDeviceConfig = JSON.parse(configText) as StoredDeviceConfig;
    if (device.relay_enabled === false) {
      return false;
    }
  } catch (error) {
    // unreadable config = keep the enabled default (fall through to true)
  }
  return true;
}
