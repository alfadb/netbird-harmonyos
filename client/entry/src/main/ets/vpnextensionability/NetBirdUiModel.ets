// UI phase 1 (home + peers + settings) — PURE LOGIC only, no @ohos imports.
// The host-side selfcheck imports this file byte-identical
// (client/entry/selfcheck/run-ui-model.mts), so everything here must stay
// plain TypeScript: no ArkUI decorators, no platform APIs.
//
// Two halves, one document (filesDir/netbird-status.json):
//  1. EXTENSION side — buildUiStatusDocument(): merges ONE connector_status
//     snapshot with ONE connector_network_config snapshot into the document
//     published for the UI process. The legacy fields keep their exact
//     names/shapes (backward compatible); the phase-1 fields are APPENDED.
//  2. UI side — peer filter/sort for the 对端 tab, display-name/label
//     mapping and the Chinese status texts for the 主页 card.
//
// INPUT TYPES: the connector document types are imported TYPE-ONLY from
// NetBirdConnector.ets (the NAPI JSON wrapper) — erased at runtime, so this
// module stays zero-import for the host selfcheck fixtures, and the extension
// hands its connector_status()/connector_network_config() results straight
// through with no structural-typing cast (arkts-no-structural-typing).
import type { ConnectorStatusResult, CoreNetworkConfig, CorePeerSummary,
  WgPeerStatusEntry } from './NetBirdConnector';

// ---------------------------------------------------------------------------
// published document (legacy fields + appended phase-1 fields)
// ---------------------------------------------------------------------------

// One peers[] entry of the published document.
export interface UiPeerEntry {
  pub_key: string;
  // display name: fqdn when management sent one, else the key's short form
  name: string;
  // fqdn verbatim, null when management sent none
  fqdn: string | null;
  // first /32 vpn address with the suffix stripped ('' when none)
  ip: string;
  // WG session established and not expired
  online: boolean;
  // 'direct' | 'relay' | 'unknown'
  connection: string;
  // seconds since the last WireGuard handshake, null when no session yet
  last_handshake_s: number | null;
}

export interface UiStatusDocument {
  // ---- legacy fields (exact shape of the pre-phase-1 document) ----
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
  // ---- appended phase-1 fields (read leniently, all optional in older
  //      documents) ----
  fqdn: string | null;
  address: string | null;
  relay_state: string | null;
  reconnects: number;
  peers: UiPeerEntry[];
}

// ---------------------------------------------------------------------------
// merge helpers (pure)
// ---------------------------------------------------------------------------

// The peer's overlay IP: the FIRST /32 allowed-ip wins (NetBird peer
// addresses are /32 host routes; wider prefixes are routed subnets), the
// first entry serves as fallback, the CIDR suffix is always stripped.
export function peerOverlayIp(vpnAddresses: string[] | undefined): string {
  if (vpnAddresses === undefined || vpnAddresses === null) {
    return '';
  }
  let fallback: string = '';
  for (let i = 0; i < vpnAddresses.length; i++) {
    const raw: string = vpnAddresses[i];
    if (typeof raw !== 'string' || raw.length === 0) {
      continue;
    }
    const slashAt: number = raw.indexOf('/');
    const bare: string = slashAt >= 0 ? raw.substring(0, slashAt) : raw;
    if (fallback.length === 0) {
      fallback = bare;
    }
    if (slashAt < 0 || raw.substring(slashAt + 1) === '32') {
      return bare;
    }
  }
  return fallback;
}

// Display name fallback when a peer has no fqdn: the key's head, the way the
// core logs shorten peer keys (peer_short) — never the full key on a list
// row, but the detail page shows the full one.
export function peerShortKey(pubKey: string): string {
  if (pubKey.length <= 10) {
    return pubKey;
  }
  return `${pubKey.substring(0, 10)}…`;
}

// The peer's connection label: relay when the outbound bearer is the relay
// carrier, direct when a UDP endpoint is set, unknown otherwise (also for
// offline peers — no bearer to name).
export function peerConnection(entry: WgPeerStatusEntry | undefined): string {
  if (entry === undefined || entry === null) {
    return 'unknown';
  }
  if (entry.session_established !== true || entry.expired === true) {
    return 'unknown';
  }
  if (entry.on_carrier === true) {
    return 'relay';
  }
  if (entry.endpoint !== undefined && entry.endpoint !== null &&
    entry.endpoint.length > 0) {
    return 'direct';
  }
  return 'unknown';
}

// Merge the network map's peer list with the WG per-peer session view into
// the published peers[] array. Order: management order (remote then offline,
// exactly what connector_network_config exports); unknown keys in the WG view
// (not in the map) are ignored — the map is the identity source.
export function mergePeers(network: CoreNetworkConfig | null,
  wgPeers: WgPeerStatusEntry[] | undefined): UiPeerEntry[] {
  const byKey: Map<string, WgPeerStatusEntry> = new Map();
  if (wgPeers !== undefined && wgPeers !== null) {
    for (let i = 0; i < wgPeers.length; i++) {
      const entry: WgPeerStatusEntry = wgPeers[i];
      if (entry !== undefined && entry !== null &&
        typeof entry.pub_key_b64 === 'string' && entry.pub_key_b64.length > 0) {
        byKey.set(entry.pub_key_b64, entry);
      }
    }
  }
  const list: CorePeerSummary[] = network !== null && network.peers !== undefined &&
    network.peers !== null ? network.peers : [];
  const merged: UiPeerEntry[] = [];
  for (let i = 0; i < list.length; i++) {
    const peer: CorePeerSummary = list[i];
    if (peer === undefined || peer === null ||
      typeof peer.pub_key !== 'string' || peer.pub_key.length === 0) {
      continue;
    }
    const wg: WgPeerStatusEntry | undefined = byKey.get(peer.pub_key);
    const fqdn: string | null =
      typeof peer.fqdn === 'string' && peer.fqdn.length > 0 ? peer.fqdn : null;
    let handshake: number | null = null;
    if (wg !== undefined && wg.last_handshake_s !== undefined &&
      wg.last_handshake_s !== null && wg.last_handshake_s >= 0) {
      handshake = wg.last_handshake_s;
    }
    const online: boolean = wg !== undefined && wg.session_established === true &&
      wg.expired !== true;
    const entry: UiPeerEntry = {
      pub_key: peer.pub_key,
      name: fqdn !== null ? fqdn : peerShortKey(peer.pub_key),
      fqdn: fqdn,
      ip: peerOverlayIp(peer.vpn_addresses),
      online: online,
      connection: online ? peerConnection(wg) : 'unknown',
      last_handshake_s: handshake
    };
    merged.push(entry);
  }
  return merged;
}

// Build the WHOLE published document. The legacy fields carry the exact
// pre-phase-1 meaning; the appended fields ride the tail.
export function buildUiStatusDocument(status: ConnectorStatusResult,
  network: CoreNetworkConfig | null, requestId: string,
  nowMs: number): UiStatusDocument {
  const errorClass: string =
    status.last_error !== undefined && status.last_error !== null
      ? status.last_error.class
      : 'none';
  const signalRegistered: boolean =
    status.signal !== undefined && status.signal !== null
      ? status.signal.registered === true
      : false;
  const signalReconnects: number =
    status.signal !== undefined && status.signal !== null &&
      typeof status.signal.reconnects === 'number'
      ? status.signal.reconnects
      : 0;
  const wgReady: boolean =
    status.wg !== undefined && status.wg !== null ? status.wg.ready === true : false;
  const wgSessions: number =
    status.wg !== undefined && status.wg !== null &&
      typeof status.wg.peers_with_session === 'number'
      ? status.wg.peers_with_session
      : 0;
  const relayState: string | null =
    status.relay !== undefined && status.relay !== null &&
      typeof status.relay.state === 'string' && status.relay.state.length > 0
      ? status.relay.state
      : null;
  const ownFqdn: string | null = network !== null && network.available &&
    typeof network.fqdn === 'string' && network.fqdn.length > 0 ? network.fqdn : null;
  const ownAddress: string | null = network !== null && network.available &&
    typeof network.address === 'string' && network.address.length > 0
      ? network.address
      : null;
  const doc: UiStatusDocument = {
    updated_at_ms: nowMs,
    request_id: requestId,
    state: status.state,
    peer_count: status.peer_count,
    route_count: status.route_count,
    last_error_class: errorClass,
    signal_registered: signalRegistered,
    signal_reconnects: signalReconnects,
    wg_ready: wgReady,
    wg_sessions: wgSessions,
    fqdn: ownFqdn,
    address: ownAddress,
    relay_state: relayState,
    reconnects: status.reconnects,
    peers: network !== null && network.available
      ? mergePeers(network, status.wg !== undefined && status.wg !== null
        ? status.wg.wg_peers : undefined)
      : []
  };
  return doc;
}

// ---------------------------------------------------------------------------
// UI-side pure helpers (对端 tab + 主页 texts)
// ---------------------------------------------------------------------------

// Case-insensitive substring filter over the display name and the overlay IP.
// An empty/blank query returns the list unchanged.
export function filterPeers(peers: UiPeerEntry[], query: string): UiPeerEntry[] {
  const needle: string = query.trim().toLowerCase();
  if (needle.length === 0) {
    return peers.slice();
  }
  const kept: UiPeerEntry[] = [];
  for (let i = 0; i < peers.length; i++) {
    const peer: UiPeerEntry = peers[i];
    if (peer.name.toLowerCase().indexOf(needle) >= 0 ||
      peer.pub_key.toLowerCase().indexOf(needle) >= 0 ||
      peer.ip.toLowerCase().indexOf(needle) >= 0) {
      kept.push(peer);
    }
  }
  return kept;
}

// Display order: online peers first (stable within the group), then by name
// (plain code-point compare — deterministic on host and device alike).
export function sortPeers(peers: UiPeerEntry[]): UiPeerEntry[] {
  const list: UiPeerEntry[] = peers.slice();
  list.sort((a: UiPeerEntry, b: UiPeerEntry): number => {
    if (a.online !== b.online) {
      return a.online ? -1 : 1;
    }
    if (a.name !== b.name) {
      return a.name < b.name ? -1 : 1;
    }
    return 0;
  });
  return list;
}

export function onlinePeerCount(peers: UiPeerEntry[]): number {
  let count: number = 0;
  for (let i = 0; i < peers.length; i++) {
    if (peers[i].online) {
      count++;
    }
  }
  return count;
}

// ---------------------------------------------------------------------------
// Chinese display texts
// ---------------------------------------------------------------------------

// The toggle reflects ON for every state a live tunnel runs through;
// failed/disconnected/closed/stopped are OFF.
export function vpnActiveState(state: string): boolean {
  return state === 'connected' || state === 'connecting' || state === 'reconnecting';
}

// 主页 status card text for the connector state. pendingStart/pendingStop are
// the page's in-flight request latches (no live document yet / stop in
// flight), so the card never shows a stale '未连接' while a request runs.
// hasOverlayIp is the document's own overlay address (the network map arrived
// and carried one): '已连接' requires the CORE state connected AND that
// address. A tunnel that is already up while management has not delivered the
// map yet (no own IP, no peers) shows the honest '连接中…' — or
// '无法连接服务器' when the error hint is one of the server-unreachable
// classes (timeout / network — the SAME classes lastErrorText renders as
// server-unreachable), so the card and the red error line can no longer
// disagree ('已连接' + an unreachable-server hint).
export function homeStateText(state: string, pendingStart: boolean,
  pendingStop: boolean, hasOverlayIp: boolean, errorClass: string): string {
  if (pendingStop) {
    return '正在断开…';
  }
  if (state === 'connected') {
    if (hasOverlayIp === true) {
      return '已连接';
    }
    return errorClass === 'timeout' || errorClass === 'network'
      ? '无法连接服务器' : '连接中…';
  }
  if (state === 'connecting' || state === 'reconnecting') {
    return '连接中…';
  }
  if (state === 'failed') {
    return '连接失败';
  }
  if (pendingStart) {
    return '连接中…';
  }
  return '未连接';
}

// 连接方式 label for the peers list/detail.
export function connectionLabel(connection: string): string {
  if (connection === 'direct') {
    return '直连';
  }
  if (connection === 'relay') {
    return '中继';
  }
  return '未知';
}

// Relative text for the last WireGuard handshake ('—' when none).
export function lastHandshakeText(seconds: number | null): string {
  if (seconds === null || seconds === undefined || seconds < 0) {
    return '—';
  }
  if (seconds < 10) {
    return '刚刚';
  }
  if (seconds < 60) {
    return `${seconds} 秒前`;
  }
  const minutes: number = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${minutes} 分钟前`;
  }
  const hours: number = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${hours} 小时前`;
  }
  return `${Math.floor(hours / 24)} 天前`;
}

// Actionable hint for the last error class ('' when none) — mirrors the
// core's class-only discipline (network / timeout / auth).
export function lastErrorText(errorClass: string): string {
  if (errorClass === 'none' || errorClass === null || errorClass === undefined ||
    errorClass.length === 0) {
    return '';
  }
  if (errorClass === 'auth') {
    return '登录凭据被拒绝，请检查 Setup Key 或重新配置';
  }
  if (errorClass === 'timeout') {
    return '连接管理服务器超时，请检查网络后重试';
  }
  if (errorClass === 'network') {
    return '网络不可用或服务器无法访问，请检查网络';
  }
  return `连接出现问题（${errorClass}），请重试`;
}
