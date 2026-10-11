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
    decrypt_errors: typeof wg.decrypt_errors === 'number' ? wg.decrypt_errors : 0
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

// 主页 status card text for the connector state. pendingStart/pendingStop are
// the page's in-flight request latches (no live document yet / stop in
// flight), so the card never shows a stale '未连接' while a request runs.
// hasOverlayIp is the document's own overlay address (the network map arrived
// and carried one); ifacePresent is the document's vpn-tun interface probe
// (false = the interface no longer exists — the fake-connected defect of
// roam-1005-0014). '已连接' requires the CORE state connected AND
// the address AND the interface. A tunnel that is already up while management
// has not delivered the map yet (no own IP, no peers), or whose interface
// disappeared (being rebuilt / torn down), shows the honest
// '连接中…' — or '无法连接服务器' when the error hint is one of the
// server-unreachable classes (timeout / network — the SAME classes
// lastErrorText renders as server-unreachable), so the card and the red error
// line can no longer disagree ('已连接' + an unreachable-server hint).
// connectFailed (2026-10-06, optional — older callers omit it): the
// extension recorded a terminal connect failure (login fatal auth / retry
// budget exhausted / startup abort before the VPN existed) and no live
// document is around to carry state 'failed' — the card then shows
// '连接失败' instead of a stale '连接中…' forever.
export function homeStateText(state: string, pendingStart: boolean,
  pendingStop: boolean, hasOverlayIp: boolean, ifacePresent: boolean,
  errorClass: string, connectFailed?: boolean): string {
  if (pendingStop) {
    return '正在断开…';
  }
  if (connectFailed === true) {
    return '连接失败';
  }
  if (state === 'connected') {
    if (hasOverlayIp === true && ifacePresent === true) {
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
  if (errorClass === 'timeout') {
    return '连接管理服务器超时，请检查网络后重试';
  }
  if (errorClass === 'network') {
    return '网络不可用或服务器无法访问，请检查网络';
  }
  return `连接出现问题（${errorClass}），请重试`;
}

// ---------------------------------------------------------------------------
// 2026-10-11 retry re-entry (fix/retry) — the connect-entry / ack model.
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
// ---------------------------------------------------------------------------

// The extension's connect-attempt acknowledgement (filesDir). ONE document per
// entry the extension ACCEPTED — a run it really started, or a run that was
// already active (the idempotent answer) — so the page can tell "the new attempt
// demonstrably started" from "the platform delivered nothing at all".
export const CONNECT_ACK_FILE: string = 'netbird-connect-ack';
// An ack counts as THIS attempt's answer when it is not older than the attempt
// (2 s clock margin — the SAME rule the failure notice is read with).
export const CONNECT_ACK_CLOCK_MARGIN_MS: number = 2000;
// stage tokens of the ack document.
export const CONNECT_ACK_STAGE_STARTED: string = 'connect-run-started';
export const CONNECT_ACK_STAGE_RUNNING: string = 'connect-run-active';
// The bounded outcome window of a pending start: a request with NO verdict (no
// live document, no failure notice) inside this window releases the latch and
// renders a failure, so the user always has a working retry. Consistent with the
// extension's own boxes (NETWORK_MAP_WAIT_MS = 15 s, STATUS_POLL_MS = 5 s): a
// start flow ends — with a live document or with a failure notice — well inside
// 30 s, so the window only ever fires on an attempt that never ran.
export const PENDING_START_TIMEOUT_MS: number = 30000;
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
export const PENDING_START_IDLE: string = 'idle';
export const PENDING_START_WAIT: string = 'wait';
export const PENDING_START_ACKNOWLEDGED: string = 'acknowledged';
export const PENDING_START_TIMEOUT: string = 'timeout';

// ONE connect-attempt acknowledgement. `request_id` is attribution only — the
// platform hands `want.parameters` over "when the VPN is started for the FIRST
// time" (from API 22 that note is explicit, @ohos.net.vpnExtension.d.ts:78-79),
// so a re-entered instance may still carry the previous id. The freshness
// comparison is on `at_ms`, exactly like the failure notice.
export interface ConnectAttemptAck {
  request_id: string;
  at_ms: number;
  stage: string;
}

export function buildConnectAttemptAck(requestId: string, atMs: number, stage: string): string {
  const ack: ConnectAttemptAck = {
    'request_id': requestId,
    'at_ms': atMs,
    'stage': stage
  };
  return JSON.stringify(ack);
}

// null = absent, malformed or unusable (no positive at_ms / no string fields);
// never throws. The caller applies the freshness rule below.
export function parseConnectAttemptAck(text: string): ConnectAttemptAck | null {
  try {
    if (text.length === 0) {
      return null;
    }
    const ack: ConnectAttemptAck = JSON.parse(text) as ConnectAttemptAck;
    if (ack.at_ms === undefined || ack.at_ms === null || ack.at_ms <= 0) {
      return null;
    }
    if (typeof ack.request_id !== 'string' || typeof ack.stage !== 'string') {
      return null;
    }
    const parsed: ConnectAttemptAck = {
      'request_id': ack.request_id, 'at_ms': ack.at_ms, 'stage': ack.stage
    };
    return parsed;
  } catch (error) {
    return null;
  }
}

// Does this ack belong to the attempt the page last started? The 2 s margin
// absorbs the two processes' clock/publish skew (same rule as the notice).
export function connectAckFresh(ackAtMs: number, lastStartAtMs: number): boolean {
  return ackAtMs > 0 && ackAtMs >= lastStartAtMs - CONNECT_ACK_CLOCK_MARGIN_MS;
}

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

export interface PendingStartFacts {
  requestSent: boolean;
  stopSent: boolean;
  // this attempt ALREADY produced an outcome (a live document, or the attempt's
  // own failure notice): its window is closed for good — a tunnel that dies much
  // later is the disconnect/reconnect paths' business, never this timeout's.
  outcomeSeen: boolean;
  // at_ms of the connect ack read this tick (0 = none).
  ackAtMs: number;
  lastStartAtMs: number;
  // 0 = not armed yet.
  deadlineMs: number;
  nowMs: number;
}

export interface PendingStartDecision {
  action: string;
  deadlineMs: number;
}

// The bounded exit of the pending-start state (a request must never leave the
// card on 连接中… forever):
//   * no request in flight, a stop in flight, or an outcome   -> idle
//   * otherwise the window is armed ONCE at the request and EXTENDED to
//     ack + PENDING_START_TIMEOUT_MS as soon as the extension acknowledges: the
//     ack IS progress (the attempt really entered the flow), so the outcome
//     window is measured from the ack, never from the tap.
//   * past the window                                          -> timeout: the
//     caller releases the latch and renders the failure.
export function decidePendingStart(facts: PendingStartFacts): PendingStartDecision {
  if (!facts.requestSent || facts.stopSent || facts.outcomeSeen) {
    return { 'action': PENDING_START_IDLE, 'deadlineMs': 0 };
  }
  let deadlineMs: number = facts.deadlineMs;
  if (deadlineMs <= 0) {
    deadlineMs = facts.lastStartAtMs + PENDING_START_TIMEOUT_MS;
  }
  let action: string = PENDING_START_WAIT;
  if (connectAckFresh(facts.ackAtMs, facts.lastStartAtMs)) {
    action = PENDING_START_ACKNOWLEDGED;
    const ackDeadlineMs: number = facts.ackAtMs + PENDING_START_TIMEOUT_MS;
    if (ackDeadlineMs > deadlineMs) {
      deadlineMs = ackDeadlineMs;
    }
  }
  if (facts.nowMs >= deadlineMs) {
    return { 'action': PENDING_START_TIMEOUT, 'deadlineMs': 0 };
  }
  return { 'action': action, 'deadlineMs': deadlineMs };
}

// 2026-10-06 登录失败可见性: the Chinese reason shown under a '连接失败'
// card. `errorClass` comes from the persistent connect-failure notice —
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
