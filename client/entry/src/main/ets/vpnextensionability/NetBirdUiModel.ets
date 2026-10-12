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
  CorePeerRelayEntry, CoreIcePeerEntry, CoreIceEndpoint, CoreRouteEntry,
  CoreDnsServer, ManagementEndpoint,
  WgDataplaneStatus, WgPeerStatusEntry } from './NetBirdConnector';

// ---------------------------------------------------------------------------
// published document (legacy fields + appended phase-1 fields)
// ---------------------------------------------------------------------------

// One endpoint of the peer's SELECTED ICE pair, published (public material
// only: the candidate type token + the public ip:port). The core's check
// internals (foundation/priority/related_address) and every credential
// (ufrag/password) or fd stay on the core side and never reach here.
export interface UiPeerIceEndpoint {
  // 'host' | 'srflx' | 'prflx' | 'relay'
  type: string;
  address: string;
  port: number;
}

// The peer's ICE detail, joined from the core's `ice_peers` table by public
// key. This object EXISTS only when the core really delivered a readable
// detail for THIS peer: an absent/null `ice_peers` source, a peer with no
// entry, or an entry that cannot be read keeps `UiPeerEntry.ice` ABSENT (never
// a default state, never a placeholder endpoint).
//
// The three-state reading the UI must respect:
//   - `ice` absent                    -> ICE 未知 (nothing was observed);
//   - `ice` present, endpoints null   -> the core did observe this peer and
//     no pair is selected YET (e.g. gathering);
//   - `ice` present, endpoints set    -> that pair was really selected.
// The last case is NOT a statement about the current connection: a
// `disconnected` peer retains its selected pair (see `reachable`), and the
// endpoint presence never overrides the WG-session verdict in
// `UiPeerEntry.connection` / `online`.
export interface UiPeerIceDetail {
  // 'idle' | 'gathering' | 'checking' | 'connected' | 'disconnected' | 'failed'
  state: string;
  // TRUE = this device is the controlling agent of the ICE session, FALSE =
  // controlled, null = the core stated no role (no session established, or
  // the value was not delivered) — an unknown role, never a default.
  controlling: boolean | null;
  // selected pair endpoints; null = no pair selected (never the first
  // candidate), NOT "unknown": the detail itself was delivered.
  selected_local: UiPeerIceEndpoint | null;
  selected_remote: UiPeerIceEndpoint | null;
  // the WG endpoint landing succeeded
  endpoint_applied: boolean;
  // the core's own usability verdict (Connected AND endpoint applied)
  reachable: boolean;
  // the class token of the last ICE/peer error ({class,status} flattened;
  // the numeric status stays on the core side — see CoreIcePeerEntry);
  // null when the core recorded none.
  last_error_class: string | null;
}

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
  // 2026-10-06 peer-online semantics: management's own online verdict
  // (remotePeers = true, offlinePeers = false). A peer can be
  // management-online while this device has no WG session with it yet.
  mgmt_online: boolean;
  // 2026-10-06 three-state peer status: 'connected' (local WG session),
  // 'idle' (management-online, no local session — 空闲, NOT offline),
  // 'offline' (management lists it under offlinePeers).
  status: string;
  // ---- 2026-10-07 client-detail increment (all optional, see the block
  //      comment below) ----
  // The peer's masked overlay prefixes from the WG session view
  // (wg_peers[].allowed_ips, e.g. ['10.30.30.2/32']); EMPTY when the core
  // reported none. The network map's own allowed-ip COUNT is not this list
  // (the map exports no prefixes) — see peerPrefixes.
  allowed_ips: string[];
  // The peer's current WG endpoint (wg_peers[].endpoint, 'ip:port');
  // '' when the core reported null (no endpoint known). Public.
  endpoint: string;
  // The local socket address the peer's traffic egresses from
  // (wg_peers[].egress_local, 'ip:port'); '' when null. Public.
  egress_local: string;
  // Bearer flags verbatim from the WG session view; false when the core
  // reported no value (the field is only meaningful WITH a live session —
  // see peerCarrierKnown).
  on_carrier: boolean;
  carrier_attached: boolean;
  // TRUE only when the WG session view really carried this peer (the
  // booleans above then mean what they say). FALSE for a map-only peer —
  // the UI must render 未知, never a fabricated 直连/off.
  carrier_known: boolean;
  // ---- 2026-10-07 per-peer ACTUAL relay lane (core `peer_relays`) ----
  // The canonical rel://host:port / rels://host:port the peer's ACTUAL
  // attached relay lane really rides, joined from the core's live
  // attached-lane table by public key. Three states, and the UI must read
  // them apart:
  //   actual_relay_known=false, key ABSENT  -> the lane table was not
  //     delivered (null/no carrier armed/older core): 未知;
  //   actual_relay_known=true, value null   -> the table WAS delivered and
  //     this peer has NO attached lane: 未走中继 — an observation, NOT the
  //     same fact as 直连 (connection stays the WG-session verdict);
  //   actual_relay_known=true, value string -> the actual lane URL.
  // NEVER the document's local relay session url / the configured home url
  // (`UiStatusDocument.relay_url`): a global or merely configured relay is
  // not a path for this peer. Public routing material only.
  actual_relay_known: boolean;
  actual_relay_url?: string | null;
  // PLAINTEXT inner-IP bytes transferred with THIS peer, cumulative since the
  // peer's tunnel was created (they survive a resnapshot; the core resets them
  // only when the peer is removed and re-added). tx_bytes: this device → peer,
  // rx_bytes: peer → this device's TUN. Both are ABSENT (undefined — dropped
  // by JSON.stringify) when the core did not deliver them (older core, or a
  // map-only peer with no WG session view), while a delivered 0 stays a real
  // 0 (no traffic yet) — so the UI can tell 未知 from 0 B.
  // NOT the device-level aggregate UiWgTraffic.tx_bytes, which counts
  // ENCAPSULATED wire bytes (inner frame + transport overhead): different
  // accounting, never one label for both.
  tx_bytes?: number;
  rx_bytes?: number;
  // ---- 2026-10-07 per-peer ICE detail (core `ice_peers`) ----
  // The peer's ICE state/role + selected-pair endpoints, joined from the
  // core's per-peer detail table by public key. OPTIONAL and ABSENT (undefined
  // — dropped by JSON.stringify) whenever the core did not deliver a readable
  // detail for THIS peer; the UI then renders ICE 未知. A present detail with
  // null endpoints means "no pair selected yet". Endpoint presence is NOT a
  // connection verdict: `connection`/`online`/`status` above stay sourced from
  // the WG session view, and a `disconnected` peer keeps its retained pair
  // with reachable=false.
  ice?: UiPeerIceDetail;
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
  // 2026-10-05 (roam-1005-0014): does the vpn-tun interface really exist
  // right now? Published only when the extension's probe is definitive
  // (true = the per-NIC counter resolves, false = it rejects with the
  // not-exist code); omitted when the probe could not read the counter — an
  // older document omits it too, and the UI gate treats "not false" as
  // lenient-pass.
  iface_present?: boolean;
  // 2026-10-06 (详情页中继地址): the effective relay URL of the live (or
  // last live) relay session (relay.current_url), falling back to the first
  // configured relay url; null when neither is known.
  // LOCAL session only (the configured home URL when nothing is dialed): a
  // PEER's relay address is a DIFFERENT fact — UiPeerEntry.actual_relay_url
  // from the core's `peer_relays` lane table — and this field must never be
  // presented as, or copied into, a peer's relay address.
  relay_url?: string | null;
  // ---- 2026-10-07 client-detail increment ----
  // Everything below is APPENDED and optional: an older document omits it,
  // a field whose source the core has not delivered is OMITTED rather than
  // defaulted (see buildUiStatusDocument) — the UI reads "absent" as 未知,
  // never as a success or a zero. No secret material crosses this boundary:
  // public keys / masked prefixes / public endpoint + relay addresses /
  // signal URI / the management authority. The management URL is published
  // as SCHEME + host + port only (never a URL string — a userinfo or query
  // in a hand-written management_url must not survive into the document).
  network?: UiNetworkView;
  // Aggregate WireGuard data-plane counters from the core (omitted when the
  // status carries no `wg` section — nothing observed, nothing invented).
  wg_traffic?: UiWgTraffic;
  // The relay client view (omitted when the status carries no `relay`
  // section). Counters/URLs only — never the relay token.
  relay?: UiRelayView;
  // Connector session deadline: the core's token ('unknown' | 'disabled' |
  // 'set') plus the Unix SECONDS deadline when the token is 'set'. Either
  // half is omitted when the connector did not deliver it.
  session_expiry?: string;
  session_expires_at_s?: number;
  // 2026-10-07 uptime increment: the CORE connector process's own timestamps,
  // passed through verbatim as Unix SECONDS (core `started_at_unix` /
  // `last_update_unix`, fields the core emits in every status). Either is
  // OMITTED when the core delivered no positive value — the UI then shows no
  // uptime. Process times only: they are NOT a management connection state
  // (management reachability stays signal_registered / reconnect counters).
  started_at_unix?: number;
  last_update_unix?: number;
  // The management authority the connector was started against, split into
  // its parts: scheme + host + port only. The configured URL string is NEVER
  // published — a hand-written management_url may carry userinfo or a query
  // and only the authority survives. Emitted as null when the extension has
  // not parsed a management url yet; the KEY ABSENT means a document written
  // before this field existed.
  management?: UiManagementEndpoint | null;
}

export interface UiManagementEndpoint {
  // 'https' | 'http' (the scheme the connector dialed)
  scheme: string;
  // host as configured (DNS name or IPv4 literal) — public
  host: string;
  port: number;
}

// The management authority of a configured management_url, for publication.
// The URL is parsed by NetBirdConnector.parseManagementEndpoint (scheme +
// authority only, malformed input throws); the endpoint carries the already
// validated host/port, so only the scheme is re-read here. NOTHING else from
// the URL is copied: no userinfo, no path, no query — a hand-written
// management_url must not leak credential material into the published
// document. Returns null when the endpoint is missing or carries no host.
export function managementEndpointView(url: string, endpoint: ManagementEndpoint |
  null | undefined): UiManagementEndpoint | null {
  if (endpoint === undefined || endpoint === null ||
    typeof endpoint.host !== 'string' || endpoint.host.length === 0) {
    return null;
  }
  const schemeSep: number = typeof url === 'string' ? url.indexOf('://') : -1;
  const scheme: string = schemeSep > 0 ? url.substring(0, schemeSep).toLowerCase() : '';
  const view: UiManagementEndpoint = {
    scheme: scheme === 'http' || scheme === 'https' ? scheme : 'https',
    host: endpoint.host,
    port: typeof endpoint.port === 'number' && endpoint.port > 0 ? endpoint.port : 443
  };
  return view;
}

// The local half of the connector's network view (own overlay address, the
// configured resolver/route set and the two server URIs the core exposes).
// Every value is read from the map's own key and is present only when the
// core delivered it (null / [] otherwise) — never a placeholder.
export interface UiNetworkView {
  // Always true in a published view: the object itself exists only when the
  // core delivered a map (a map-less status omits `network` entirely, so the
  // UI sees "no map" rather than an empty one).
  available: boolean;
  // Own overlay address with the prefix the core assigned
  // (network.address + network.address_prefix_len), e.g. '10.64.0.7/16';
  // null when the core sent no IPv4 address (IPv6 is not modelled yet).
  prefix: string | null;
  // Management's interface resolver (network.interface_dns); null when none.
  interface_dns: string | null;
  // The configured resolver set (network.dns.servers) in management order —
  // priority order, verbatim. Empty when none.
  dns_servers: UiDnsServer[];
  // The managed route set as exported (network.routes). A held 0.0.0.0/0 is
  // ABSENT here by contract, not by omission — see default_route.
  routes: UiRouteEntry[];
  // The core's default-route gate verdict with its stable reason token
  // ('default-route-allowed:*' / '-held:*' / '-forced:*').
  default_route: UiDefaultRoute | null;
  // netbird_config.signal as announced (network.signal, host:port or with a
  // scheme); null when no signal config was seen. A dial target, not a
  // registration verdict (that is signal_registered).
  signal_uri: string | null;
}

export interface UiDnsServer {
  ip: string;
  port: number;
}

export interface UiRouteEntry {
  // canonical masked network, e.g. '172.16.0.0/12'
  network: string;
  // TRUE only for 0.0.0.0/0, and only when the gate exported it
  is_default: boolean;
}

export interface UiDefaultRoute {
  allowed: boolean;
  reason: string;
}

// Aggregate WireGuard data-plane counters (core's `wg`, counter names kept
// verbatim so a value can be traced back to the core field). All numbers
// are cumulative since the device came up; `tx_bytes` is encapsulated
// transport bytes, `rx_bytes_to_tun` plaintext bytes delivered to the TUN.
export interface UiWgTraffic {
  tx_packets: number;
  tx_bytes: number;
  rx_packets: number;
  rx_bytes_to_tun: number;
  dropped_no_route: number;
  decrypt_errors: number;
  // 0.0.26 gap 3: the device's THIRD drop counter (inbound datagrams matching
  // no registered peer). Exported next to the other two; 0 when the core
  // section predates the field (never a missing key).
  unknown_peer_drops: number;
}

// The relay client view (core's `relay`). `current_url` is the effective
// address of the live (or last live) session; `urls` is the configured set
// (what the next session dials). `state` is the runtime token already
// published as relay_state; repeated here so a detail panel can read ONE
// object. transport_bytes = outbound + inbound payload.
export interface UiRelayView {
  enabled: boolean;
  state: string;
  current_url: string | null;
  urls: string[];
  reconnects: number;
  transport_bytes: number;
  token_valid: boolean;
  last_error_class: string | null;
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

// One optional string from the WG session view: the value when the core
// really sent a non-empty string, '' otherwise (null / absent / wrong type).
// Never a placeholder: '' is the honest "the core reported none".
function optionalText(value: string | null | undefined): string {
  return typeof value === 'string' && value.length > 0 ? value : '';
}

// The peer's masked overlay prefixes from the WG session view
// (wg_peers[].allowed_ips); [] when the core sent none. Only well-formed
// strings are kept — no synthesised prefixes (the map carries a COUNT only,
// see CorePeerSummary.allowed_ips, which is deliberately NOT expanded here).
export function peerPrefixes(allowedIps: string[] | undefined): string[] {
  if (allowedIps === undefined || allowedIps === null) {
    return [];
  }
  const kept: string[] = [];
  for (let i = 0; i < allowedIps.length; i++) {
    const raw: string = allowedIps[i];
    if (typeof raw === 'string' && raw.length > 0) {
      kept.push(raw);
    }
  }
  return kept;
}

// One optional cumulative byte counter from the WG per-peer view
// (wg_peers[].tx_bytes / rx_bytes, PLAINTEXT inner-IP bytes): the number when
// the core delivered a finite, non-negative value — a delivered 0 IS a value
// (no traffic yet) — and undefined otherwise (absent / null / wrong type /
// NaN / Infinity / negative). undefined is the honest "not delivered":
// JSON.stringify drops the member, so the published peer carries no key at
// all and the UI renders 未知 instead of a fabricated 0 B.
export function peerByteCounter(value: number | null | undefined): number | undefined {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    return undefined;
  }
  return value;
}

// The core's ICE state vocabulary (peer_conn PeerIceState::as_str). A state
// token outside this set is not a state this module gives a meaning to: the
// whole detail then reads as unknown (iceDetailView).
const ICE_STATE_TOKENS: string[] = ['idle', 'gathering', 'checking', 'connected',
  'disconnected', 'failed'];

// The candidate-type vocabulary of a selected-pair endpoint
// (ice CandidateType::as_str) — the same tokens the signal wire carries.
const ICE_CANDIDATE_TYPES: string[] = ['host', 'srflx', 'prflx', 'relay'];

// One selected-pair endpoint from a core `ice_peers` entry. null = the core
// really sent null/absent (no pair selected — an observation); undefined =
// the value cannot be read as an endpoint (unknown type / empty address /
// impossible port), which makes the CALLER keep the whole detail unknown
// rather than publish a half-invented endpoint.
function iceEndpointView(endpoint: CoreIceEndpoint | null | undefined):
  UiPeerIceEndpoint | null | undefined {
  if (endpoint === undefined || endpoint === null) {
    return null;
  }
  if (typeof endpoint.type !== 'string' ||
    ICE_CANDIDATE_TYPES.indexOf(endpoint.type) < 0) {
    return undefined;
  }
  if (typeof endpoint.address !== 'string' || endpoint.address.length === 0) {
    return undefined;
  }
  if (typeof endpoint.port !== 'number' || !Number.isInteger(endpoint.port) ||
    endpoint.port < 0 || endpoint.port > 65535) {
    return undefined;
  }
  const view: UiPeerIceEndpoint = {
    type: endpoint.type,
    address: endpoint.address,
    port: endpoint.port
  };
  return view;
}

// One peer's ICE detail as published, or undefined = ICE 未知 for this peer:
// no source entry (the peer is absent from a delivered `ice_peers` table, or
// the table itself is null/absent) or an entry that cannot be read as the
// documented contract (unknown state token, unreadable endpoint, missing
// landing/reachability boolean, malformed error class). NEVER upgraded into a
// plausible-looking state/endpoint/role — and never interpreted as a current
// connection: `state` and `reachable` are carried verbatim from the core, so
// a `disconnected` peer that still holds its selected pair stays
// reachable=false.
export function iceDetailView(entry: CoreIcePeerEntry | null | undefined):
  UiPeerIceDetail | undefined {
  if (entry === undefined || entry === null) {
    return undefined;
  }
  if (typeof entry.state !== 'string' ||
    ICE_STATE_TOKENS.indexOf(entry.state) < 0) {
    return undefined;
  }
  const local: UiPeerIceEndpoint | null | undefined =
    iceEndpointView(entry.selected_local);
  const remote: UiPeerIceEndpoint | null | undefined =
    iceEndpointView(entry.selected_remote);
  if (local === undefined || remote === undefined) {
    return undefined;
  }
  if (typeof entry.endpoint_applied !== 'boolean' ||
    typeof entry.reachable !== 'boolean') {
    return undefined;
  }
  // Role: only the core's own boolean is a role. null (or anything else) is
  // the honest "the core stated no role" — no session established yet.
  let role: boolean | null = null;
  if (entry.controlling === true) {
    role = true;
  } else if (entry.controlling === false) {
    role = false;
  }
  // Class-only error: the numeric status stays on the core side (connector
  // credential discipline — same choice as UiRelayView.last_error_class).
  let errorClass: string | null = null;
  if (entry.last_error !== undefined && entry.last_error !== null) {
    if (typeof entry.last_error.class !== 'string' ||
      entry.last_error.class.length === 0) {
      return undefined;
    }
    errorClass = entry.last_error.class;
  }
  const view: UiPeerIceDetail = {
    state: entry.state,
    controlling: role,
    selected_local: local,
    selected_remote: remote,
    endpoint_applied: entry.endpoint_applied,
    reachable: entry.reachable,
    last_error_class: errorClass
  };
  return view;
}

// Own overlay address with its prefix, as the core modelled it
// (network.address + network.address_prefix_len). null unless BOTH are
// present — a bare address without a prefix would state a /32 the core
// never confirmed (IPv6 addresses arrive as null/None core-side).
export function networkPrefix(address: string | null | undefined,
  prefixLen: number | null | undefined): string | null {
  if (typeof address !== 'string' || address.length === 0) {
    return null;
  }
  if (typeof prefixLen !== 'number' || !Number.isInteger(prefixLen) ||
    prefixLen < 0 || prefixLen > 128) {
    return null;
  }
  return `${address}/${prefixLen}`;
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

// The peer's three-state status (2026-10-06, official-client semantics):
// 'connected' when THIS device has a live WG session with the peer;
// 'idle' when management lists the peer as online (remotePeers) but no
// local session exists yet — 空闲, deliberately NOT 离线 (a peer can be
// reachable for others while our phone simply has no traffic/session with
// it); 'offline' only when management itself moved the peer to
// offlinePeers. `mgmtOnline` missing (older core/document) is read as
// management-online, never a silent 离线.
export function peerStatusOf(online: boolean, mgmtOnline: boolean): string {
  if (online) {
    return 'connected';
  }
  return mgmtOnline ? 'idle' : 'offline';
}

// Merge the network map's peer list with the WG per-peer session view into
// the published peers[] array. Order: management order (remote then offline,
// exactly what connector_network_config exports); unknown keys in the WG view
// (not in the map) are ignored — the map is the identity source.
// peerRelays is the core's `peer_relays` tail (the ACTUAL attached-lane
// table, same public key namespace as the WG view); omitted/null means the
// core did not deliver the table, so every peer's actual relay stays 未知.
// icePeers is the core's `ice_peers` tail (one detail per ICE-orchestrator
// peer, public-key sorted): it is joined onto the SAME peers by public key.
// A null/absent table, a peer with no entry in a delivered table, or an entry
// that cannot be read leaves that peer's `ice` ABSENT — ICE 未知, never a
// fabricated state/endpoint/role. Nothing here feeds `connection`/`online`:
// endpoint presence is not a connection verdict.
export function mergePeers(network: CoreNetworkConfig | null,
  wgPeers: WgPeerStatusEntry[] | undefined,
  peerRelays?: CorePeerRelayEntry[] | null,
  icePeers?: CoreIcePeerEntry[] | null): UiPeerEntry[] {
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
  // The ACTUAL attached relay lanes, keyed by the SAME public key namespace.
  // A delivered table (even []) is an observation about every peer; a
  // null/absent tail is not. An entry whose URL is not a usable string is
  // kept as '' — that peer's lane is then 未知, never "not relayed".
  const relayLanes: Map<string, string> = new Map();
  const relaySourceDelivered: boolean =
    peerRelays !== undefined && peerRelays !== null;
  if (peerRelays !== undefined && peerRelays !== null) {
    for (let i = 0; i < peerRelays.length; i++) {
      const lane: CorePeerRelayEntry = peerRelays[i];
      if (lane !== undefined && lane !== null &&
        typeof lane.pub_key_b64 === 'string' && lane.pub_key_b64.length > 0) {
        relayLanes.set(lane.pub_key_b64,
          typeof lane.relay_url === 'string' && lane.relay_url.length > 0
            ? lane.relay_url : '');
      }
    }
  }
  // The per-peer ICE details, keyed by the SAME public key namespace. Only a
  // readable entry is stored (iceDetailView); an unreadable one is simply not
  // there, so that peer's `ice` stays absent. A null/absent table and a
  // delivered table without this peer's key are BOTH 未知 for that peer —
  // nobody made a statement about it, so nothing is published.
  const iceDetails: Map<string, UiPeerIceDetail> = new Map();
  if (icePeers !== undefined && icePeers !== null) {
    for (let i = 0; i < icePeers.length; i++) {
      const ice: CoreIcePeerEntry = icePeers[i];
      if (ice === undefined || ice === null ||
        typeof ice.pub_key !== 'string' || ice.pub_key.length === 0) {
        continue;
      }
      const detail: UiPeerIceDetail | undefined = iceDetailView(ice);
      if (detail !== undefined) {
        iceDetails.set(ice.pub_key, detail);
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
    const mgmtOnline: boolean = peer.mgmt_online !== false;
    // The WG-view half: present only when the core's wg_peers carried this
    // key. carrier_known gates every flag below — a map-only peer must not
    // read as a confirmed direct/off-carrier peer.
    const carrierKnown: boolean = wg !== undefined && wg !== null;
    // Per-peer plaintext byte counters: attributed by the peer's OWN key (the
    // map lookup above), never summed or shared across peers; undefined when
    // this peer has no WG view or the core did not deliver the counter.
    const peerTxBytes: number | undefined = wg !== undefined && wg !== null
      ? peerByteCounter(wg.tx_bytes) : undefined;
    const peerRxBytes: number | undefined = wg !== undefined && wg !== null
      ? peerByteCounter(wg.rx_bytes) : undefined;
    // This peer's ACTUAL relay lane, resolved ONLY from the delivered lane
    // table by public key (never from the local/home relay url). known=true
    // means the resolution is definitive: a URL string, or a delivered table
    // with no entry for this key (null = no attached lane). An unusable
    // entry leaves the lane 未知 (known=false, key absent).
    const lane: string | undefined = relaySourceDelivered
      ? relayLanes.get(peer.pub_key) : undefined;
    const relayLaneKnown: boolean = relaySourceDelivered &&
      (lane === undefined || lane.length > 0);
    const relayLaneUrl: string | null | undefined = relayLaneKnown
      ? (lane !== undefined && lane.length > 0 ? lane : null) : undefined;
    const entry: UiPeerEntry = {
      pub_key: peer.pub_key,
      name: fqdn !== null ? fqdn : peerShortKey(peer.pub_key),
      fqdn: fqdn,
      ip: peerOverlayIp(peer.vpn_addresses),
      online: online,
      connection: online ? peerConnection(wg) : 'unknown',
      last_handshake_s: handshake,
      mgmt_online: mgmtOnline,
      status: peerStatusOf(online, mgmtOnline),
      allowed_ips: wg !== undefined && wg !== null
        ? peerPrefixes(wg.allowed_ips) : [],
      endpoint: wg !== undefined && wg !== null ? optionalText(wg.endpoint) : '',
      egress_local: wg !== undefined && wg !== null
        ? optionalText(wg.egress_local) : '',
      on_carrier: wg !== undefined && wg !== null && wg.on_carrier === true,
      carrier_attached: wg !== undefined && wg !== null && wg.carrier_attached === true,
      carrier_known: carrierKnown,
      actual_relay_known: relayLaneKnown,
      actual_relay_url: relayLaneUrl,
      tx_bytes: peerTxBytes,
      rx_bytes: peerRxBytes,
      // undefined when this peer has no readable ICE detail: JSON.stringify
      // drops the member, so the published peer carries no `ice` key at all
      // and the UI renders ICE 未知 instead of a fabricated state.
      ice: iceDetails.get(peer.pub_key)
    };
    merged.push(entry);
  }
  return merged;
}

// Whether the network document counts as a delivered map. `available` is the
// core's own verdict and is authoritative when present (`available:false` is
// the explicit no-network-map answer — the peer list stays empty even if the
// document happens to carry stray peer fields). A document written before
// that key existed has no verdict at all, so its real map content counts —
// otherwise a valid map would be dropped by an upgrade.
function networkDelivered(network: CoreNetworkConfig | null): boolean {
  if (network === null || network === undefined) {
    return false;
  }
  if (network.available === true) {
    return true;
  }
  if (network.available === false) {
    return false;
  }
  return (typeof network.address === 'string' && network.address.length > 0) ||
    (typeof network.fqdn === 'string' && network.fqdn.length > 0) ||
    (network.peers !== undefined && network.peers !== null && network.peers.length > 0) ||
    (network.routes !== undefined && network.routes !== null && network.routes.length > 0) ||
    (network.dns !== undefined && network.dns !== null) ||
    (typeof network.signal === 'string' && network.signal.length > 0);
}

// The local network view. Each field mirrors the core's own key; anything the
// core did not deliver is left absent (null / empty / omitted section) —
// never defaulted into a plausible-looking value.
function buildNetworkView(network: CoreNetworkConfig): UiNetworkView {
  const prefix: string | null = networkPrefix(network.address,
    network.address_prefix_len);
  const interfaceDns: string | null =
    typeof network.interface_dns === 'string' && network.interface_dns.length > 0
      ? network.interface_dns
      : null;
  const signalUri: string | null =
    typeof network.signal === 'string' && network.signal.length > 0
      ? network.signal
      : null;
  const dnsServers: UiDnsServer[] = [];
  if (network.dns !== undefined && network.dns !== null &&
    network.dns.servers !== undefined && network.dns.servers !== null) {
    const listed: CoreDnsServer[] = network.dns.servers;
    for (let i = 0; i < listed.length; i++) {
      const server: CoreDnsServer = listed[i];
      if (server !== undefined && server !== null &&
        typeof server.ip === 'string' && server.ip.length > 0) {
        dnsServers.push({
          ip: server.ip,
          port: typeof server.port === 'number' ? server.port : 0
        });
      }
    }
  }
  const routes: UiRouteEntry[] = [];
  if (network.routes !== undefined && network.routes !== null) {
    const listed: CoreRouteEntry[] = network.routes;
    for (let i = 0; i < listed.length; i++) {
      const route: CoreRouteEntry = listed[i];
      if (route !== undefined && route !== null &&
        typeof route.network === 'string' && route.network.length > 0) {
        routes.push({ network: route.network, is_default: route.is_default === true });
      }
    }
  }
  let defaultRoute: UiDefaultRoute | null = null;
  if (network.default_route !== undefined && network.default_route !== null) {
    defaultRoute = {
      allowed: network.default_route.allowed === true,
      reason: network.default_route.reason
    };
  }
  const view: UiNetworkView = {
    available: true,
    prefix: prefix,
    interface_dns: interfaceDns,
    dns_servers: dnsServers,
    routes: routes,
    default_route: defaultRoute,
    signal_uri: signalUri
  };
  return view;
}

// Aggregate WG counters. Present only when the status carried a `wg` section
// (undefined otherwise — the caller omits the whole block).
function buildWgTraffic(wg: WgDataplaneStatus): UiWgTraffic {
  const traffic: UiWgTraffic = {
    tx_packets: typeof wg.tx_packets === 'number' ? wg.tx_packets : 0,
    tx_bytes: typeof wg.tx_bytes === 'number' ? wg.tx_bytes : 0,
    rx_packets: typeof wg.rx_packets === 'number' ? wg.rx_packets : 0,
    rx_bytes_to_tun: typeof wg.rx_bytes_to_tun === 'number' ? wg.rx_bytes_to_tun : 0,
    dropped_no_route: typeof wg.dropped_no_route === 'number' ? wg.dropped_no_route : 0,
    decrypt_errors: typeof wg.decrypt_errors === 'number' ? wg.decrypt_errors : 0,
    unknown_peer_drops:
      typeof wg.unknown_peer_drops === 'number' ? wg.unknown_peer_drops : 0
  };
  return traffic;
}

// Build the WHOLE published document. The legacy fields carry the exact
// pre-phase-1 meaning; the appended fields ride the tail. ifacePresent is
// the extension's vpn-tun interface probe (true/false); undefined leaves
// the field out of the JSON (probe unreadable / legacy caller) — the UI
// gate then stays lenient instead of failing a live tunnel. managementUrl /
// managementEndpoint are the extension's configured management url and its
// parsed endpoint: host and port only are published, never the URL string
// (see managementEndpointView — a hand-written management_url may carry
// userinfo or a query and none of it may reach the document). The whole
// `management` object is omitted unless the endpoint carries a host.
export function buildUiStatusDocument(status: ConnectorStatusResult,
  network: CoreNetworkConfig | null, requestId: string,
  nowMs: number, ifacePresent?: boolean, managementUrl?: string,
  managementEndpoint?: ManagementEndpoint | null): UiStatusDocument {
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
  // 2026-10-06 (详情页中继地址): the effective relay URL of the live (or
  // last live) session; the first CONFIGURED url is the fallback (what the
  // next session will dial), null when neither is known — never invented.
  let relayUrl: string | null = null;
  if (status.relay !== undefined && status.relay !== null) {
    if (typeof status.relay.current_url === 'string' &&
      status.relay.current_url.length > 0) {
      relayUrl = status.relay.current_url;
    } else if (status.relay.urls !== undefined && status.relay.urls !== null &&
      status.relay.urls.length > 0) {
      relayUrl = status.relay.urls[0];
    }
  }
  const mapDelivered: boolean = networkDelivered(network);
  const ownFqdn: string | null = mapDelivered && network !== null &&
    typeof network.fqdn === 'string' && network.fqdn.length > 0 ? network.fqdn : null;
  const ownAddress: string | null = mapDelivered && network !== null &&
    typeof network.address === 'string' && network.address.length > 0
      ? network.address
      : null;
  // ---- 2026-10-07 client-detail increment ----
  const networkView: UiNetworkView | undefined =
    mapDelivered && network !== null ? buildNetworkView(network) : undefined;
  const wgTraffic: UiWgTraffic | undefined =
    status.wg !== undefined && status.wg !== null ? buildWgTraffic(status.wg) : undefined;
  // The relay view mirrors the core's `relay` section 1:1; `current_url` is
  // the live session's address (null while nothing was ever dialed), `urls`
  // the configured set. Omitted entirely when the core sent no relay section.
  let relayView: UiRelayView | undefined = undefined;
  if (status.relay !== undefined && status.relay !== null) {
    const configured: string[] = status.relay.urls !== undefined &&
      status.relay.urls !== null ? status.relay.urls : [];
    relayView = {
      enabled: status.relay.enabled === true,
      state: typeof status.relay.state === 'string' ? status.relay.state : '',
      current_url: typeof status.relay.current_url === 'string' &&
        status.relay.current_url.length > 0 ? status.relay.current_url : null,
      urls: configured,
      reconnects: typeof status.relay.reconnects === 'number'
        ? status.relay.reconnects : 0,
      transport_bytes: typeof status.relay.transport_bytes === 'number'
        ? status.relay.transport_bytes : 0,
      token_valid: status.relay.token_valid === true,
      last_error_class: typeof status.relay.last_error_class === 'string' &&
        status.relay.last_error_class.length > 0 ? status.relay.last_error_class : null
    };
  }
  // Session deadline: the core's own token, plus the Unix SECONDS value it
  // sends alongside (null for unknown/disabled) — omitted when absent, so an
  // older core cannot be read as "expires at 0".
  const sessionExpiry: string | undefined =
    typeof status.session_expiry === 'string' && status.session_expiry.length > 0
      ? status.session_expiry
      : undefined;
  const sessionExpiresAt: number | undefined =
    typeof status.session_expires_at_unix === 'number' &&
      status.session_expires_at_unix > 0 ? status.session_expires_at_unix : undefined;
  // Core process times (Unix SECONDS), for the UI uptime row. Passed through
  // verbatim and ONLY when the core really delivered a positive integer stamp:
  // an absent/null/non-numeric/0 value is omitted, so the UI shows no uptime
  // instead of computing one from 1970. Nothing is derived or invented here,
  // and neither value is a management connection verdict.
  const coreStartedAt: number | undefined =
    typeof status.started_at_unix === 'number' &&
      Number.isInteger(status.started_at_unix) && status.started_at_unix > 0
      ? status.started_at_unix : undefined;
  const coreLastUpdate: number | undefined =
    typeof status.last_update_unix === 'number' &&
      Number.isInteger(status.last_update_unix) && status.last_update_unix > 0
      ? status.last_update_unix : undefined;
  // Management authority: scheme + host + port, never the URL string.
  const management: UiManagementEndpoint | null = managementEndpointView(
    typeof managementUrl === 'string' ? managementUrl : '',
    managementEndpoint);
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
    peers: mapDelivered && network !== null
      ? mergePeers(network, status.wg !== undefined && status.wg !== null
        ? status.wg.wg_peers : undefined, status.peer_relays, status.ice_peers)
      : [],
    iface_present: ifacePresent,
    relay_url: relayUrl,
    network: networkView,
    wg_traffic: wgTraffic,
    relay: relayView,
    session_expiry: sessionExpiry,
    session_expires_at_s: sessionExpiresAt,
    started_at_unix: coreStartedAt,
    last_update_unix: coreLastUpdate,
    management: management
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

// Display order (2026-10-06): connected peers first, then idle
// (management-online), then offline — stable within the group, then by
// name (plain code-point compare — deterministic on host and device alike).
export function peerStatusRank(status: string): number {
  if (status === 'connected') {
    return 0;
  }
  if (status === 'idle') {
    return 1;
  }
  return 2;
}

export function sortPeers(peers: UiPeerEntry[]): UiPeerEntry[] {
  const list: UiPeerEntry[] = peers.slice();
  list.sort((a: UiPeerEntry, b: UiPeerEntry): number => {
    const rankA: number = peerStatusRank(a.status);
    const rankB: number = peerStatusRank(b.status);
    if (rankA !== rankB) {
      return rankA - rankB;
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

// 主页 status card text for the connector state — the OFFICIAL state
// vocabulary, not our own.
//
// Upstream has exactly five engine states and one label each
// (android-client HomeFragment.java:66, strings.xml:27-32):
//   CONNECTING          -> Connecting…
//   CONNECTED           -> Connected
//   DISCONNECTING       -> Disconnecting…
//   DISCONNECTED        -> Disconnected        (loginRequired overrides it)
//   NO_NETWORK          -> No network available
// with `loginRequired` replacing the DISCONNECTED label (HomeFragment.java:114,
// :873-883), and a FAILED connect repainting DISCONNECTED — the failure itself is
// only a transient Toast carrying the engine's own message
// (MainActivity.java:1030-1036). There is no 连接失败 and no 无法连接服务器
// state: both were our inventions (ALIGNMENT-AUDIT R9 / §1.5).
//
// `pendingStart` is the page's tap latch (the upstream beginPendingAction +
// paintTransition(CONNECTING): HomeFragment.java:198-200, :623-630), so the card says
// 连接中… from the tap until the engine reports. `pendingStop` is the stop in
// flight (upstream DISCONNECTING). `loginRequired` is the upstream sticky flag.
// The Chinese wording is this app's own project language; the STATE SET is
// upstream's.
//
// hasOverlayIp is the document's own overlay address (the network map arrived
// and carried one); ifacePresent is the document's vpn-tun interface probe
// (false = the interface no longer exists — the fake-connected defect of
// roam-1005-0014). '已连接' requires the CORE state connected AND the address
// AND the interface; a tunnel that is up while management has not delivered
// the map yet, or whose interface disappeared, is honestly still 连接中….
export function homeStateText(state: string, pendingStart: boolean,
  pendingStop: boolean, hasOverlayIp: boolean, ifacePresent: boolean,
  loginRequired?: boolean): string {
  if (pendingStop) {
    return '正在断开…';
  }
  if (state === 'connected') {
    if (hasOverlayIp === true && ifacePresent === true) {
      return '已连接';
    }
    return '连接中…';
  }
  if (state === 'connecting' || state === 'reconnecting') {
    return '连接中…';
  }
  // 'failed' / 'disconnected' / 'closed' / '' (no document at all) are all
  // upstream's DISCONNECTED, whose label the login flag overrides.
  if (loginRequired === true) {
    return '需要登录';
  }
  if (pendingStart) {
    return '连接中…';
  }
  return '未连接';
}

// 连接方式 label for the peers list/detail (2026-10-06: '经中继' — the
// relayed path goes THROUGH the relay server, 直连 is point-to-point).
export function connectionLabel(connection: string): string {
  if (connection === 'direct') {
    return '直连';
  }
  if (connection === 'relay') {
    return '经中继';
  }
  return '未知';
}

// 对端 three-state status text (2026-10-06): a management-online peer
// without a local WG session is 空闲 — gray, but never called 离线; only
// management's own offlinePeers verdict is 离线.
export function peerStatusText(status: string): string {
  if (status === 'connected') {
    return '已连接';
  }
  if (status === 'idle') {
    return '空闲';
  }
  return '离线';
}

// 对端 three-state dot/text color: connected = green, idle = neutral gray
// (present but not connected), offline = the muted light gray.
export function peerStatusColor(status: string): string {
  if (status === 'connected') {
    return '#176B3A';
  }
  if (status === 'idle') {
    return '#8A9AA6';
  }
  return '#C0C8D0';
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

// 2026-10-12: the stable token the CORE reports when this peer needs a fresh
// interactive login — upstream `StatusNeedsLogin`
// (client/internal/state.go:15, set at client/internal/connect.go:373-377).
// It reaches the UI as the LAST_ERROR class in the status document and as the
// connect-failure notice token. It is NOT the generic `auth` class: no retry of
// the same credential can succeed.
export const NEEDS_LOGIN_CLASS: string = 'needs-login';
// The official label: android-client `main_status_login_required` = "Login
// required" (app/src/main/res/values/strings.xml:32), the text the home screen
// paints through HomeFragment.onLoginRequired (HomeFragment.java:873-883) and
// the session-expiry path (MainActivity.java:1030-1036). The official UI adds
// NO separate action hint: on Android the connect toggle itself runs the
// interactive login ("the connect toggle already runs the interactive login",
// HomeFragment.java:193-199), so the label is the whole message.
export const NEEDS_LOGIN_TEXT: string = '需要登录';

// Actionable hint for the last error class ('' when none) — mirrors the
// core's class-only discipline (network / timeout / auth).
export function lastErrorText(errorClass: string): string {
  if (errorClass === 'none' || errorClass === null || errorClass === undefined ||
    errorClass.length === 0) {
    return '';
  }
  if (errorClass === 'auth') {
    return '设备未注册或 Setup Key 无效，请到设置重新配置';
  }
  // 2026-10-12: the core's own needs-login class (upstream StatusNeedsLogin —
  // the management server refused this peer: its session expired, or it is not
  // registered). Retrying the same credential cannot help, so the line is the
  // official label itself.
  if (errorClass === NEEDS_LOGIN_CLASS) {
    return NEEDS_LOGIN_TEXT;
  }
  if (errorClass === 'timeout') {
    return '连接管理服务器超时，请检查网络后重试';
  }
  if (errorClass === 'network') {
    return '网络不可用或服务器无法访问，请检查网络';
  }
  return `连接出现问题（${errorClass}），请重试`;
}

// ---------------------------------------------------------------------------
// 2026-10-11 retry re-entry (fix/retry) — the connect-entry model.
//
// Device evidence (pc-connecting-diag/REPORT.md §4.2-§4.4, 0.0.24): after a
// connect attempt failed the retry path set requestSent=true, deleted the
// failure notice and called startVpnExtensionAbility against the STILL ALIVE
// extension process. The whole connect flow hangs off onCreate behind an
// unconditional instance latch and this extension has no onConnect (the API-26
// declaration is context/onCreate/onDestroy only), so nothing re-ran:
// published === null + requestSent === true + the notice gone = 连接中… forever.
//
// The decisions below are the PURE half of the fix; the extension and the page
// only gather facts and act. All of them are host-checked byte-identically
// (selfcheck/run-ui-model.mts + run-connect-retry.mts).
//
// 2026-10-12 (same branch, upstream tap handling): the entry gate above is kept,
// but the 30 s "pending start" window and the connect-attempt ACK file are gone.
// Upstream bounds the TAP, not the attempt (ALIGNMENT-AUDIT §1.3 R5): the home
// screen latches on the user action for PENDING_ACTION_TIMEOUT_MS = 7 s and
// re-enables the toggle after FORCE_CANCEL_DELAY_MS = 2 s so a stuck transition
// can be aborted. The state that drives all of it is the engine's own report —
// never a document the shell invented.
// ---------------------------------------------------------------------------

// The stop leg of a retry: how long the page waits for the extension's OWN stop
// evidence (the `stopping` marker written synchronously in onDestroy) before it
// starts anyway. Bounded by its own clock — the platform's stop promise is
// best-effort and is never the thing the start waits on.
export const EXTENSION_STOP_WAIT_MS: number = 3000;
export const EXTENSION_STOP_POLL_MS: number = 200;

export const CONNECT_ENTRY_RUN: string = 'run';
export const CONNECT_ENTRY_REJECT_RUNNING: string = 'reject-connector-running';
export const CONNECT_LEG_DIRECT: string = 'start';
export const CONNECT_LEG_AFTER_STOP: string = 'stop-then-start';

// The extension's entry gate — the replacement for the unconditional instance
// latch. `connectorRunning` is the CORE's own answer
// (connector_status().running); `null` = the core did not answer (no connector
// object yet / a refused native call), which is NOT proof of a running engine,
// so the entry proceeds. A stopped or aborted run can therefore never block a
// re-entry: the official Android client's entire guard is
// `if (engineIsRunning) return;` (EngineRunner.java:136-147).
export function connectEntryDecision(connectorRunning: boolean | null): string {
  return connectorRunning === true ? CONNECT_ENTRY_REJECT_RUNNING : CONNECT_ENTRY_RUN;
}

// The page's start leg. A repeated startVpnExtensionAbility against a LIVE
// extension instance is not a re-entry (parameters are a first-start channel and
// there is no onConnect), so the instance must be gone first: when the page cannot
// read the extension's own stop evidence it drives ONE stop and waits for it.
export function connectStartLeg(extensionGone: boolean): string {
  return extensionGone ? CONNECT_LEG_DIRECT : CONNECT_LEG_AFTER_STOP;
}

// ---------------------------------------------------------------------------
// 2026-10-12 upstream tap handling — pending action + force-cancel window.
//
// Replaces our 30 s pending-start timeout. Upstream never times out the
// CONNECTING state nor the engine: the connect loop retries forever with
// exponential backoff (client/internal/connect.go:240-247, :531) and the home
// screen bounds only the TAP (ALIGNMENT-AUDIT §1.3 "Direct answer").
//
//   * PENDING_ACTION_TIMEOUT_MS / beginPendingAction / expirePendingAction
//     (HomeFragment.java:70, :623-630, :640-647): after 7 s without the requested
//     state the latch is released and the toggle is repainted from "whatever the
//     engine last reported" instead of staying latched.
//   * FORCE_CANCEL_DELAY_MS / armForceCancel / openForceCancelWindow
//     (HomeFragment.java:83, :586-596, :598-612): while a transition is painted the
//     toggle is disabled, and 2 s into it the toggle RE-ENABLES so a tap aborts
//     the stuck transition (force-cancel = stop) instead of leaving the user
//     watching an endless Connecting….
//
// The report is the ENGINE's own state, which on HarmonyOS is the status
// document the extension publishes (there is no in-process callback across the
// two processes): the core's ConnState reduced to the states the tap handling
// switches on. 'connecting' also covers reconnecting, and every dead state is
// upstream's DISCONNECTED — the official applyEngineState paints a failed connect
// as DISCONNECTED (HomeFragment.java:501-522, :530-547) — while the ABSENCE of any
// report is its own token, never a verdict.
// ---------------------------------------------------------------------------

// The engine's report, reduced to the states the tap handling switches on.
export const ENGINE_REPORT_CONNECTED: string = 'connected';
export const ENGINE_REPORT_CONNECTING: string = 'connecting';
export const ENGINE_REPORT_DISCONNECTED: string = 'disconnected';
// Nothing has been reported since the tap. This is NOT "the engine reported
// Disconnected": upstream releases the latch on a REPORT or on the 7 s timer, and
// its cold-start DISCONNECTED seed only ever paints — onEngineState() never fires
// for it (HomeFragment.java:239, :484-497). Reading "no report" as a Disconnected
// report would clear the tap latch on the first tick after every tap.
export const ENGINE_REPORT_NONE: string = '';

// The requested state of a tap — upstream's pendingTarget IS an engine state.
export const PENDING_TARGET_NONE: string = '';
export const PENDING_TARGET_CONNECTED: string = ENGINE_REPORT_CONNECTED;
export const PENDING_TARGET_DISCONNECTED: string = ENGINE_REPORT_DISCONNECTED;

// The bounded tap latch: 7 s (HomeFragment.java:70) and the force-cancel delay that
// re-opens the toggle: 2 s (HomeFragment.java:83).
export const PENDING_ACTION_TIMEOUT_MS: number = 7000;
export const FORCE_CANCEL_DELAY_MS: number = 2000;

// The core publishes six states (client/core/src/state.rs ConnState::as_str:
// connected / connecting / reconnecting / failed / disconnected / closed); the
// official Android client's EngineState has CONNECTING / CONNECTED /
// DISCONNECTING / DISCONNECTED / NO_NETWORK (HomeFragment.java:66). The tokens
// below are the intersection the tap handling needs; an EMPTY document state means
// the page has no fresh report at all (ENGINE_REPORT_NONE), never a verdict.
export function engineReportState(documentState: string): string {
  if (documentState === 'connected') {
    return ENGINE_REPORT_CONNECTED;
  }
  if (documentState === 'connecting' || documentState === 'reconnecting') {
    return ENGINE_REPORT_CONNECTING;
  }
  if (documentState.length === 0) {
    return ENGINE_REPORT_NONE;
  }
  // failed / disconnected / closed are ALL upstream's DISCONNECTED: a failed
  // connect repaints Disconnected, the failure itself is only the Toast.
  return ENGINE_REPORT_DISCONNECTED;
}

export interface TransitionFacts {
  // the tap's requested state (PENDING_TARGET_*); '' = no action in flight.
  target: string;
  // the engine's last report (ENGINE_REPORT_*); ENGINE_REPORT_NONE when nothing
  // has been reported since the tap.
  report: string;
  // 0 = not armed.
  deadlineMs: number;
  // 0 = not armed.
  forceCancelArmedAtMs: number;
  forceCancelOpen: boolean;
  nowMs: number;
}

export interface TransitionDecision {
  target: string;
  deadlineMs: number;
  forceCancelArmedAtMs: number;
  forceCancelOpen: boolean;
  // upstream isTransitioning(): a transition is being painted.
  transition: boolean;
  // the engine reported the requested state (or the connect demonstrably
  // failed): the latch is released and the real report paints.
  settled: boolean;
  // 7 s passed without the requested state: release the latch and fall back to
  // the last reported real state.
  expired: boolean;
}

// The pure half of shouldSuppressPaint (:498-522) + expirePendingAction
// (:636-647) + the force-cancel window (:567-612). Never extends the window on
// progress: the tap latch is a fixed 7 s from the tap, exactly like upstream's
// posted Runnable.
export function decideTransition(facts: TransitionFacts): TransitionDecision {
  let target: string = facts.target;
  let deadlineMs: number = facts.deadlineMs;
  let settled: boolean = false;
  let expired: boolean = false;
  if (target.length > 0) {
    if (facts.report === target) {
      // state == target: the requested state arrived.
      settled = true;
    } else if (target === PENDING_TARGET_CONNECTED &&
      facts.report === ENGINE_REPORT_DISCONNECTED) {
      // the connect ended disconnected, so it failed — including the attempt
      // that never got as far as Connecting (the engine refused up front).
      settled = true;
    } else if (deadlineMs <= 0) {
      // armed lazily only if the caller did not arm it at the tap.
      deadlineMs = facts.nowMs + PENDING_ACTION_TIMEOUT_MS;
    } else if (facts.nowMs >= deadlineMs) {
      expired = true;
    }
  }
  if (settled || expired) {
    target = PENDING_TARGET_NONE;
    deadlineMs = 0;
  }
  const transition: boolean = target.length > 0 ||
    facts.report === ENGINE_REPORT_CONNECTING;
  let armedAtMs: number = facts.forceCancelArmedAtMs;
  let open: boolean = facts.forceCancelOpen;
  if (!transition) {
    // closeForceCancelWindow(): a settled paint resets both.
    armedAtMs = 0;
    open = false;
  } else if (!open) {
    if (armedAtMs <= 0) {
      armedAtMs = facts.nowMs;
    } else if (facts.nowMs - armedAtMs >= FORCE_CANCEL_DELAY_MS) {
      // openForceCancelWindow(): only while the transition is still running.
      open = true;
    }
  }
  return {
    'target': target, 'deadlineMs': deadlineMs,
    'forceCancelArmedAtMs': armedAtMs, 'forceCancelOpen': open,
    'transition': transition, 'settled': settled, 'expired': expired
  };
}

// 2026-10-06 登录失败可见性: the Chinese reason shown next to the card's OFFICIAL
// state (未连接 / 需要登录) — the engine's message, which upstream shows as a Toast
// (MainActivity.java:1030-1036) and which this app additionally keeps on the red
// error line. `errorClass` comes from the persistent connect-failure notice —
// the connector's own last_error class (auth/timeout/network/...) or an
// extension abort-stage token. The none/empty case is the honest generic
// server-unreachable line (the map never arrived, no finer class exists).
export function connectFailureText(errorClass: string): string {
  if (errorClass === 'credentials-missing') {
    return '缺少登录凭据（Setup Key 或设备配置），请到设置重新配置';
  }
  if (errorClass === 'no-signal-uri') {
    return '管理端配置缺少 Signal 服务器，无法建立对端连接';
  }
  const mapped: string = lastErrorText(errorClass);
  if (mapped.length > 0) {
    return mapped;
  }
  return '无法连接管理服务器，请检查网络或服务器配置';
}

// ---------------------------------------------------------------------------
// 2026-10-13 会话延长 (the interactive session extend — home-screen offer)
//
// Upstream Android offers the extend on the home screen whenever the engine
// knows a deadline: `updateSessionRow()` shows the row exactly when the
// deadline is > 0 and hides it otherwise (HomeFragment.java:901-912), the label
// comes from `formatSessionExpiry()` (HomeFragment.java:920-941) and the row
// carries the constant hint `session_banner_hint` (fragment_home.xml:424, the
// secondary TextView inside the `session_expiry_row` block :377-427).
// The tap runs `ServiceAccessor.extendSession()` (HomeFragment.java:219-223) →
// `MainActivity.extendSession()` (MainActivity.java:1068-1085): the interactive
// SSO plus the management `ExtendAuthSession`, whose two outcomes are the
// toasts `session_extended` / `session_extend_failed` (strings.xml:296-297).
//
// NO lead threshold gates the Android row: the T-10min / T-2min leads
// (`WarningLead` / `FinalWarningLead`, sessionwatch/watcher.go:48-57) drive the
// DAEMON's T-10 OS notification and its T-2 fallback dialog (consumers:
// client/ui/tray_session.go, client/server/server.go:2289-2294), never this
// row. The row's own rule is exactly `deadline > 0` — with the remaining time
// measured against the page's clock, exactly like upstream's
// `System.currentTimeMillis()` (HomeFragment.java:921).
//
// The Chinese wording is upstream's OWN zh-rCN translation
// (app/src/main/res/values-zh-rCN/strings.xml:233-246) — carried verbatim, not
// re-translated here.
// ---------------------------------------------------------------------------

// `session_banner_hint` (strings.xml:300) → "点按以延长会话" (zh-rCN:237).
export const SESSION_BANNER_HINT: string = '点按以延长会话';
// `session_extended` (strings.xml:296) → "会话已延长" (zh-rCN:233): the success
// Toast (MainActivity.java:1073-1080).
export const SESSION_EXTENDED_TEXT: string = '会话已延长';
// `session_banner_expired` / `session_banner_expires_soon` (strings.xml:298-299)
// → "会话已过期" / "会话将在不到一分钟后过期" (zh-rCN:235-236). The English
// minute/hour/day texts are plurals (strings.xml:301-313); zh-rCN carries a
// single `other` form (:238-246), so each renders as ONE sentence.
export const SESSION_EXPIRED_TEXT: string = '会话已过期';
export const SESSION_EXPIRES_SOON_TEXT: string = '会话将在不到一分钟后过期';

const SESSION_MINUTE_MS: number = 60 * 1000;
const SESSION_HOUR_MS: number = 60 * SESSION_MINUTE_MS;
const SESSION_DAY_MS: number = 24 * SESSION_HOUR_MS;

// One home-screen session row: `offer` = the official visibility rule (the
// engine knows a positive deadline), `text` = the official label, `hint` = the
// constant official hint. When `offer` is false the other two are empty — the
// row is ABSENT (upstream `View.GONE`), never emptied-but-shown.
export interface SessionRowView {
  offer: boolean;
  text: string;
  hint: string;
}

// Round UP (upstream `ceilDiv`, HomeFragment.java:943-945, applied at :929/:934/
// :938): the figure shown is never more time than the user actually has, and the
// sub-minute tail keeps its own wording rather than rounding to "1 分钟".
export function sessionRowView(deadlineS: number | undefined, nowMs: number): SessionRowView {
  if (deadlineS === undefined || !Number.isFinite(deadlineS) || deadlineS <= 0 ||
    !Number.isFinite(nowMs)) {
    return { offer: false, text: '', hint: '' };
  }
  const remainingMs: number = deadlineS * 1000 - nowMs;
  if (remainingMs <= 0) {
    return { offer: true, text: SESSION_EXPIRED_TEXT, hint: SESSION_BANNER_HINT };
  }
  if (remainingMs < SESSION_MINUTE_MS) {
    return { offer: true, text: SESSION_EXPIRES_SOON_TEXT, hint: SESSION_BANNER_HINT };
  }
  if (remainingMs <= 59 * SESSION_MINUTE_MS) {
    const minutes: number = Math.ceil(remainingMs / SESSION_MINUTE_MS);
    return { offer: true, text: `会话将在 ${minutes} 分钟后过期`, hint: SESSION_BANNER_HINT };
  }
  if (remainingMs <= 23 * SESSION_HOUR_MS) {
    const hours: number = Math.ceil(remainingMs / SESSION_HOUR_MS);
    return { offer: true, text: `会话将在 ${hours} 小时后过期`, hint: SESSION_BANNER_HINT };
  }
  const days: number = Math.ceil(remainingMs / SESSION_DAY_MS);
  return { offer: true, text: `会话将在 ${days} 天后过期`, hint: SESSION_BANNER_HINT };
}

// The deadline the row renders. Upstream has ONE process and ONE writer: the
// engine's `ApplySessionDeadline` (engine_authsession.go:102) moves the anchor
// and the UI field follows the callback (MainActivity.java:1053-1059 →
// HomeFragment.onSessionDeadlineChanged, :866-870).
//
// Here the connector lives in the VPN-extension process while the SSO surface
// (and therefore the extend itself) lives in the UI process, so the UI holds
// the server's own answer from the extend alongside the connector's published
// anchor. The rule is simply the LATER of the two known observations: the
// published document wins as soon as the connector re-anchors from the server
// (its next SyncResponse carries `sessionExpiresAt`, conversion.go:214-217),
// and a stale published value can never drag the display backwards after a
// successful extend. 0 on either side = that side knows nothing.
export function sessionRowDeadlineS(publishedS: number | undefined, extendedS: number): number {
  const published: number = publishedS !== undefined && Number.isFinite(publishedS) && publishedS > 0
    ? publishedS : 0;
  const extended: number = Number.isFinite(extendedS) && extendedS > 0 ? extendedS : 0;
  return Math.max(published, extended);
}

// `session_extend_failed` = "Failed to extend session: %1$s" (strings.xml:297)
// → "延长会话失败：%1$s" (zh-rCN:234); `%1$s` is the reason. Upstream
// interpolates the engine's own error message (MainActivity.java:1082-1086);
// this core's surfaces are class-only (no server text ever crosses a boundary),
// so the reason is the stable token rendered as Chinese. '' = the caller-visible
// "no message" (a user cancellation, which upstream also reports as nothing:
// `cancelExtendSession` only ends the flow, MainActivity.java:1130-1140).
const SESSION_EXTEND_FAILED_PREFIX: string = '延长会话失败：';

export function sessionExtendFailureText(reason: string): string {
  if (reason === 'cancelled') {
    return '';
  }
  if (reason === 'timeout' || reason === 'extend_timeout') {
    return `${SESSION_EXTEND_FAILED_PREFIX}连接管理服务器超时，请检查网络后重试`;
  }
  if (reason === 'network-error') {
    return `${SESSION_EXTEND_FAILED_PREFIX}网络不可用或服务器无法访问，请检查网络后重试`;
  }
  if (reason === 'management-error') {
    return `${SESSION_EXTEND_FAILED_PREFIX}管理服务器拒绝了本次延长`;
  }
  if (reason === 'jwt-required' || reason === 'invalid-input' || reason === 'invalid-config') {
    return `${SESSION_EXTEND_FAILED_PREFIX}请求无效，请重试`;
  }
  if (reason === 'discovery_unavailable' || reason === 'login_unavailable' ||
    reason === 'extend_unavailable' || reason === 'pkce_crypto_unavailable' ||
    reason === 'redirect_unavailable') {
    return `${SESSION_EXTEND_FAILED_PREFIX}当前应用不支持该操作，请更新应用后重试`;
  }
  if (reason === 'discovery_failed' || reason === 'metadata_invalid' ||
    reason === 'discovery_timeout' || reason === 'login_failed' ||
    reason === 'login_timeout' || reason === 'token_exchange_failed' ||
    reason === 'token_invalid' || reason === 'code_invalid' ||
    reason === 'extend_failed' ||
    reason === 'login_not_prepared' || reason === 'exchange_already_used') {
    return `${SESSION_EXTEND_FAILED_PREFIX}登录流程未完成，请重试`;
  }
  if (reason === 'busy') {
    return `${SESSION_EXTEND_FAILED_PREFIX}已有一次登录或延长正在进行`;
  }
  return `${SESSION_EXTEND_FAILED_PREFIX}未知错误（${reason}）`;
}
