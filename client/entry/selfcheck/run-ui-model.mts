// Host-side checks for the UI phase-1 pure module (byte-identical import of
// client/entry/src/main/ets/vpnextensionability/NetBirdUiModel.ets).
// Run: node client/entry/selfcheck/run-ui-model.mts
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const source = join(here, '..', 'src', 'main', 'ets', 'vpnextensionability', 'NetBirdUiModel.ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-ui-model-'));
let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail?: string): void {
  console.log(`${condition ? 'ok  ' : 'FAIL'} ${name}`);
  if (condition) { passed++; } else { failed++; }
  if (!condition && detail !== undefined) {
    console.log(`     ${detail}`);
  }
}
try {
  const fixture = join(dir, 'NetBirdUiModel.ts');
  writeFileSync(fixture, readFileSync(source));
  const m = await import(pathToFileURL(fixture).href);

  // ---- peerOverlayIp -------------------------------------------------------
  check('overlay ip prefers the /32 entry',
    m.peerOverlayIp(['10.30.30.1/32', '192.168.7.0/24']) === '10.30.30.1');
  check('overlay ip falls back to the first entry without a /32',
    m.peerOverlayIp(['192.168.7.0/24', '10.0.0.0/8']) === '192.168.7.0');
  check('overlay ip strips a missing suffix',
    m.peerOverlayIp(['10.1.2.3']) === '10.1.2.3');
  check('overlay ip of nothing is empty', m.peerOverlayIp(undefined) === '');
  check('overlay ip of an empty list is empty', m.peerOverlayIp([]) === '');

  // ---- peerShortKey ---------------------------------------------------------
  check('short key truncates long keys', m.peerShortKey('QUJDREVGRw==AAAABBBB') === 'QUJDREVGRw…');
  check('short key keeps short keys', m.peerShortKey('UEVFUjA=') === 'UEVFUjA=');

  // ---- peerConnection -------------------------------------------------------
  check('connection: relay when on carrier',
    m.peerConnection({ pub_key_b64: 'k', session_established: true, expired: false, on_carrier: true }) === 'relay');
  check('connection: direct with endpoint',
    m.peerConnection({ pub_key_b64: 'k', session_established: true, expired: false, endpoint: '1.2.3.4:5' }) === 'direct');
  check('connection: unknown without endpoint or carrier',
    m.peerConnection({ pub_key_b64: 'k', session_established: true, expired: false }) === 'unknown');
  check('connection: unknown when offline',
    m.peerConnection({ pub_key_b64: 'k', session_established: false, expired: false, endpoint: '1.2.3.4:5' }) === 'unknown');
  check('connection: unknown when expired',
    m.peerConnection({ pub_key_b64: 'k', session_established: true, expired: true, endpoint: '1.2.3.4:5' }) === 'unknown');
  check('connection: unknown without a wg entry', m.peerConnection(undefined) === 'unknown');

  // ---- mergePeers -----------------------------------------------------------
  const net = {
    available: true,
    address: '10.64.0.7',
    fqdn: 'me.netbird.cloud',
    peers: [
      { pub_key: 'KEYA', fqdn: 'peer-a.netbird.cloud', vpn_addresses: ['10.30.30.1/32'] },
      { pub_key: 'KEYB', fqdn: null, vpn_addresses: ['192.168.7.0/24'] },
      { pub_key: 'KEYC', fqdn: 'peer-c.netbird.cloud', vpn_addresses: [] },
      // 2026-10-06: management moved this peer to offlinePeers
      { pub_key: 'KEYOFF', fqdn: 'peer-off.netbird.cloud', vpn_addresses: [],
        mgmt_online: false }
    ]
  };
  const wgPeers = [
    { pub_key_b64: 'KEYA', session_established: true, expired: false,
      endpoint: '203.0.113.9:40000', last_handshake_s: 42 },
    { pub_key_b64: 'KEYB', session_established: true, expired: false,
      on_carrier: true, last_handshake_s: 5 },
    { pub_key_b64: 'KEYD', session_established: true, expired: false }
  ];
  const merged = m.mergePeers(net, wgPeers);
  check('merge keeps management order and count', merged.length === 4);
  check('merge: direct online peer',
    merged[0].online === true && merged[0].connection === 'direct' &&
    merged[0].ip === '10.30.30.1' && merged[0].name === 'peer-a.netbird.cloud' &&
    merged[0].last_handshake_s === 42);
  check('merge: relay peer without fqdn uses the short key',
    merged[1].connection === 'relay' && merged[1].name.startsWith('KEYB') &&
    merged[1].fqdn === null);
  check('merge: session-less peer', merged[2].online === false &&
    merged[2].connection === 'unknown' && merged[2].last_handshake_s === null);
  check('merge: negative handshake stays null', m.mergePeers(net, [
    { pub_key_b64: 'KEYA', session_established: true, expired: false, last_handshake_s: -1 }
  ])[0].last_handshake_s === null);
  check('merge without a network map is empty', m.mergePeers(null, wgPeers).length === 0);
  check('merge skips malformed peers', m.mergePeers({
    available: true,
    peers: [{ pub_key: '' }, { pub_key: 'KEYA', fqdn: 'x', vpn_addresses: [] }]
  }, [])[0].pub_key === 'KEYA');

  // ---- 2026-10-06 peer three-state (management online vs local session) ----
  check('peerStatusOf: connected wins over mgmt state',
    m.peerStatusOf(true, true) === 'connected' &&
    m.peerStatusOf(true, false) === 'connected');
  check('peerStatusOf: mgmt-online without a session is idle, NOT offline',
    m.peerStatusOf(false, true) === 'idle');
  check('peerStatusOf: mgmt-offline is offline',
    m.peerStatusOf(false, false) === 'offline');
  check('merge: remote peer without session is idle',
    merged[2].status === 'idle' && merged[2].mgmt_online === true);
  check('merge: offlinePeers entry carries mgmt_online=false and status offline',
    merged[3].status === 'offline' && merged[3].mgmt_online === false);
  check('merge: missing mgmt_online is read as management-online',
    m.mergePeers({ available: true, peers: [{ pub_key: 'K' }] }, [])[0].status === 'idle');
  check('status texts', m.peerStatusText('connected') === '已连接' &&
    m.peerStatusText('idle') === '空闲' && m.peerStatusText('offline') === '离线' &&
    m.peerStatusText('anything-else') === '离线');
  check('status colors', m.peerStatusColor('connected') === '#176B3A' &&
    m.peerStatusColor('idle') === '#8A9AA6' &&
    m.peerStatusColor('offline') === '#C0C8D0');

  // ---- buildUiStatusDocument ------------------------------------------------
  const status = {
    running: true,
    state: 'connected',
    peer_count: 3,
    route_count: 2,
    reconnects: 1,
    last_error: null,
    signal: { registered: true, reconnects: 2 },
    wg: { ready: true, peers_with_session: 2, wg_peers: wgPeers },
    relay: { enabled: true, state: 'ready' }
  };
  const doc = m.buildUiStatusDocument(status, net, 'ui-1', 1800000000000);
  // legacy fields keep the exact pre-phase-1 meaning
  check('doc legacy fields',
    doc.updated_at_ms === 1800000000000 && doc.request_id === 'ui-1' &&
    doc.state === 'connected' && doc.peer_count === 3 && doc.route_count === 2 &&
    doc.last_error_class === 'none' && doc.signal_registered === true &&
    doc.signal_reconnects === 2 && doc.wg_ready === true && doc.wg_sessions === 2);
  // appended fields
  check('doc appended own identity',
    doc.fqdn === 'me.netbird.cloud' && doc.address === '10.64.0.7');
  check('doc appended relay/reconnects', doc.relay_state === 'ready' && doc.reconnects === 1);
  check('doc appended peers merge', doc.peers.length === 4 && doc.peers[0].online === true);
  // no relay url known: honest null, never invented
  check('doc relay_url null without relay urls',
    doc.relay_url === null || doc.relay_url === undefined);
  // no map yet: honest nulls, empty peers, legacy intact
  const noMap = m.buildUiStatusDocument(status, null, 'ui-2', 1);
  check('doc without a map is honest',
    noMap.fqdn === null && noMap.address === null && noMap.peers.length === 0 &&
    noMap.state === 'connected');
  const unavailable = m.buildUiStatusDocument(status,
    { available: false, peers: [{ pub_key: 'X', vpn_addresses: [] }] }, 'ui-3', 1);
  check('doc with an unavailable map carries no peers', unavailable.peers.length === 0);
  const errDoc = m.buildUiStatusDocument({
    running: false, state: 'failed', peer_count: 0, route_count: 0, reconnects: 0,
    last_error: { class: 'auth' }, signal: null, wg: null, relay: null
  }, null, 'ui-4', 1);
  check('doc error class and null sections',
    errDoc.last_error_class === 'auth' && errDoc.signal_registered === false &&
    errDoc.wg_ready === false && errDoc.relay_state === null);
  // the published document must survive a JSON round trip
  check('doc JSON round trip keeps peers',
    JSON.parse(JSON.stringify(doc)).peers[0].name === 'peer-a.netbird.cloud');
  // roam-1005-0014: the registry-probe field rides the tail — true/false
  // when definitive, OMITTED (dropped by JSON.stringify) when the probe
  // could not read the registry, so a legacy/unknown document reads lenient.
  check('doc iface_present rides the tail verbatim',
    m.buildUiStatusDocument(status, net, 'ui-1', 1, true).iface_present === true &&
    m.buildUiStatusDocument(status, net, 'ui-1', 1, false).iface_present === false);
  check('doc iface_present omitted when the probe was unreadable',
    !('iface_present' in JSON.parse(
      JSON.stringify(m.buildUiStatusDocument(status, net, 'ui-1', 1)))) &&
    !('iface_present' in JSON.parse(
      JSON.stringify(m.buildUiStatusDocument(status, net, 'ui-1', 1, undefined)))));
  // 2026-10-06 中继地址: the effective session URL wins; the first
  // CONFIGURED url is the fallback; neither → null
  const relayDoc = m.buildUiStatusDocument({
    ...status,
    relay: { enabled: true, state: 'ready', current_url: 'rels://live.relay:443',
      urls: ['rels://cfg.relay:443'] }
  }, net, 'ui-5', 1);
  check('doc relay_url prefers the live session url',
    relayDoc.relay_url === 'rels://live.relay:443');
  const relayCfgDoc = m.buildUiStatusDocument({
    ...status,
    relay: { enabled: true, state: 'disconnected', urls: ['rels://cfg.relay:443',
      'rel://cfg2.relay:80'] }
  }, net, 'ui-6', 1);
  check('doc relay_url falls back to the first configured url',
    relayCfgDoc.relay_url === 'rels://cfg.relay:443');
  const noRelayDoc = m.buildUiStatusDocument({ ...status, relay: null }, net, 'ui-7', 1);
  check('doc relay_url null without a relay section', noRelayDoc.relay_url === null);

  // ---- 2026-10-07 client-detail increment ---------------------------------
  // Per-peer WG-view detail: endpoint/egress_local/carrier flags/allowed_ips
  // come from the core's wg_peers entry; a map-only peer carries
  // carrier_known=false so the UI cannot read its absent flags as 直连/off.
  const detailWgPeers = [
    { pub_key_b64: 'KEYA', session_established: true, expired: false,
      endpoint: '203.0.113.9:40000', egress_local: '192.168.1.5:51820',
      allowed_ips: ['10.30.30.1/32', '10.30.30.0/24'],
      carrier_attached: true, on_carrier: false, last_handshake_s: 42 },
    { pub_key_b64: 'KEYB', session_established: true, expired: false,
      endpoint: null, egress_local: null, on_carrier: true }
  ];
  const detailNet = {
    available: true,
    address: '10.64.0.7',
    address_prefix_len: 16,
    interface_dns: '100.100.0.1',
    signal: 'signal.netbird.io:10000',
    fqdn: 'me.netbird.cloud',
    routes: [
      { network: '10.30.30.0/24', is_default: false },
      { network: '0.0.0.0/0', is_default: true }
    ],
    dns: { service_enable: true, servers: [
      { ip: '100.100.0.1', port: 53 }, { ip: '8.8.8.8', port: 53 }
    ] },
    default_route: { allowed: true,
      reason: 'default-route-allowed:peers-registered-and-tunnel-ready' },
    peers: [
      { pub_key: 'KEYA', fqdn: 'peer-a.netbird.cloud', vpn_addresses: ['10.30.30.1/32'] },
      { pub_key: 'KEYB', fqdn: 'peer-b.netbird.cloud', vpn_addresses: ['10.30.30.2/32'] },
      { pub_key: 'KEYC', fqdn: 'peer-c.netbird.cloud', vpn_addresses: [] }
    ]
  };
  const detailStatus = {
    running: true, state: 'connected', peer_count: 3, route_count: 2, reconnects: 4,
    last_error: null,
    signal: { registered: true, reconnects: 2 },
    wg: { ready: true, peers_with_session: 2, tx_packets: 11, tx_bytes: 1100,
      rx_packets: 9, rx_bytes_to_tun: 900, dropped_no_route: 3, decrypt_errors: 1,
      wg_peers: detailWgPeers },
    relay: { enabled: true, state: 'ready', current_url: 'rels://live.relay:443',
      urls: ['rels://cfg.relay:443', 'rel://cfg2.relay:80'], reconnects: 2,
      transport_bytes: 4096, token_valid: true, last_error_class: null },
    session_expiry: 'set', session_expires_at_unix: 1800003600
  };
  const detail = m.buildUiStatusDocument(detailStatus, detailNet, 'ui-8', 1,
    true, 'https://user:pw@mgmt.example:443/?token=SECRET',
    { host: 'mgmt.example', port: 443, ip: '' });
  check('detail: own overlay prefix carries the core prefix length',
    detail.network.available === true && detail.network.prefix === '10.64.0.7/16');
  check('detail: interface dns + signal uri verbatim',
    detail.network.interface_dns === '100.100.0.1' &&
    detail.network.signal_uri === 'signal.netbird.io:10000');
  check('detail: dns servers in management order',
    detail.network.dns_servers.length === 2 &&
    detail.network.dns_servers[0].ip === '100.100.0.1' &&
    detail.network.dns_servers[1].ip === '8.8.8.8');
  check('detail: configured routes + default-route gate verdict',
    detail.network.routes.length === 2 &&
    detail.network.routes[1].network === '0.0.0.0/0' &&
    detail.network.routes[1].is_default === true &&
    detail.network.default_route.allowed === true &&
    detail.network.default_route.reason.indexOf('allowed') >= 0);
  check('detail: wg traffic counters aggregated verbatim',
    detail.wg_traffic.tx_packets === 11 && detail.wg_traffic.tx_bytes === 1100 &&
    detail.wg_traffic.rx_packets === 9 && detail.wg_traffic.rx_bytes_to_tun === 900 &&
    detail.wg_traffic.dropped_no_route === 3 && detail.wg_traffic.decrypt_errors === 1);
  check('detail: relay view carries urls/current/counters/error',
    detail.relay.enabled === true && detail.relay.state === 'ready' &&
    detail.relay.current_url === 'rels://live.relay:443' &&
    detail.relay.urls.length === 2 && detail.relay.reconnects === 2 &&
    detail.relay.transport_bytes === 4096 && detail.relay.token_valid === true &&
    detail.relay.last_error_class === null);
  check('detail: session expiry token + unix seconds',
    detail.session_expiry === 'set' && detail.session_expires_at_s === 1800003600);
  check('detail: management authority only, never the url string',
    detail.management.scheme === 'https' && detail.management.host === 'mgmt.example' &&
    detail.management.port === 443 &&
    JSON.stringify(detail).indexOf('SECRET') < 0 &&
    JSON.stringify(detail).indexOf('user:pw') < 0);
  check('detail: peer wg-view fields ride the entry',
    detail.peers[0].endpoint === '203.0.113.9:40000' &&
    detail.peers[0].egress_local === '192.168.1.5:51820' &&
    detail.peers[0].allowed_ips[1] === '10.30.30.0/24' &&
    detail.peers[0].carrier_known === true &&
    detail.peers[0].carrier_attached === true &&
    detail.peers[0].on_carrier === false);
  check('detail: a null endpoint is the honest empty string, not a value',
    detail.peers[1].endpoint === '' && detail.peers[1].egress_local === '' &&
    detail.peers[1].on_carrier === true && detail.peers[1].carrier_attached === false &&
    detail.peers[1].carrier_known === true && detail.peers[1].allowed_ips.length === 0);
  check('detail: a map-only peer is carrier_known=false with empty detail',
    detail.peers[2].carrier_known === false && detail.peers[2].endpoint === '' &&
    detail.peers[2].egress_local === '' && detail.peers[2].on_carrier === false &&
    detail.peers[2].allowed_ips.length === 0 &&
    detail.peers[2].last_handshake_s === null);
  check('detail: held default route keeps the gate reason and drops the route',
    m.buildUiStatusDocument({ ...detailStatus, wg: null }, {
      ...detailNet,
      routes: [{ network: '10.30.30.0/24', is_default: false }],
      default_route: { allowed: false, reason: 'default-route-held:no-usable-peer' }
    }, 'ui-9', 1).network.default_route.reason === 'default-route-held:no-usable-peer' &&
    m.buildUiStatusDocument({ ...detailStatus, wg: null }, {
      ...detailNet,
      routes: [{ network: '10.30.30.0/24', is_default: false }],
      default_route: { allowed: false, reason: 'default-route-held:no-usable-peer' }
    }, 'ui-9', 1).network.routes.length === 1);
  // absent source ⇒ absent field (never a fabricated zero/success). The
  // PUBLISHED JSON is the contract the UI reads: JSON.stringify drops an
  // undefined member, so the round-tripped document is what a missing
  // section looks like on disk.
  const sparse = m.buildUiStatusDocument({
    running: true, state: 'connecting', peer_count: 0, route_count: 0,
    reconnects: 0, last_error: null, signal: null, wg: null, relay: null,
    session_expiry: 'unknown'
  }, { available: true, peers: [] }, 'ui-10', 1);
  const sparseJson = JSON.parse(JSON.stringify(sparse));
  check('detail: no relay/wg sections ⇒ no relay/wg_traffic blocks',
    !('relay' in sparseJson) && !('wg_traffic' in sparseJson) &&
    sparse.relay === undefined && sparse.wg_traffic === undefined);
  check('detail: unknown deadline has no unix value',
    sparse.session_expiry === 'unknown' && !('session_expires_at_s' in sparseJson) &&
    sparse.session_expires_at_s === undefined);
  // `management` is explicit on disk: null = this document's producer had not
  // parsed a management url (the key ABSENT = a pre-upgrade document).
  check('detail: no management endpoint ⇒ management null, never a host',
    sparse.management === null && sparseJson.management === null &&
    JSON.parse(JSON.stringify(m.buildUiStatusDocument(status, net, 'ui-11', 1)))
      .management === null);
  check('detail: a map-less document has no network block',
    !('network' in JSON.parse(
      JSON.stringify(m.buildUiStatusDocument(status, null, 'ui-12', 1)))) &&
    !('network' in JSON.parse(JSON.stringify(m.buildUiStatusDocument(status,
      { available: false, reason: 'no-network-map' }, 'ui-13', 1)))));
  check('detail: unavailable map is authoritative over stray peer fields',
    m.buildUiStatusDocument(status, { available: false,
      peers: [{ pub_key: 'X', vpn_addresses: [] }] }, 'ui-14', 1).peers.length === 0);
  // backward compatibility: a document written before `available`/the new
  // fields still publishes its map content (no upgrade-time data loss)
  const legacyDoc = m.buildUiStatusDocument(status,
    { address: '10.64.0.7', peers: [
      { pub_key: 'KEYA', fqdn: 'peer-a.netbird.cloud', vpn_addresses: ['10.30.30.1/32'] }
    ] }, 'ui-15', 1);
  check('detail: legacy map (no `available` key) keeps its address and peers',
    legacyDoc.address === '10.64.0.7' && legacyDoc.peers.length === 1 &&
    legacyDoc.network !== undefined && legacyDoc.network.prefix === null);

  // ---- 2026-10-07 per-peer traffic counters (PLAINTEXT, attributed) --------
  // wg_peers[].tx_bytes/rx_bytes are plaintext inner-IP bytes per peer,
  // cumulative since that peer's tunnel was created. Zero vs missing is the
  // whole point: a delivered 0 is a real value (no traffic yet) and must be
  // published as 0, while an absent/null counter must stay ABSENT (undefined,
  // dropped by JSON.stringify) so the UI renders 未知 instead of 0 B.
  check('bytes: a delivered counter is kept, a delivered 0 stays a real 0',
    m.peerByteCounter(1234) === 1234 && m.peerByteCounter(0) === 0);
  check('bytes: absent/null/wrong-type/negative counters are undefined',
    m.peerByteCounter(undefined) === undefined && m.peerByteCounter(null) === undefined &&
    m.peerByteCounter(-1) === undefined && m.peerByteCounter(Number.NaN) === undefined &&
    m.peerByteCounter(Number.POSITIVE_INFINITY) === undefined &&
    m.peerByteCounter('1234' as unknown as number) === undefined);
  const byteWgPeers = [
    // KEYA: both counters delivered, rx is a real zero
    { pub_key_b64: 'KEYA', session_established: true, expired: false,
      tx_bytes: 1234, rx_bytes: 0 },
    // KEYB: an older core — no counters at all
    { pub_key_b64: 'KEYB', session_established: true, expired: false },
    // KEYC: tx delivered, rx explicitly null
    { pub_key_b64: 'KEYC', session_established: true, expired: false,
      tx_bytes: 7, rx_bytes: null },
    // KEYD: has counters but is NOT in the map — the map is the identity source
    { pub_key_b64: 'KEYD', session_established: true, expired: false,
      tx_bytes: 5000, rx_bytes: 6000 }
  ];
  const byteNet = {
    available: true,
    address: '10.64.0.7',
    peers: [
      { pub_key: 'KEYA', fqdn: 'peer-a.netbird.cloud', vpn_addresses: ['10.30.30.1/32'] },
      { pub_key: 'KEYB', fqdn: 'peer-b.netbird.cloud', vpn_addresses: ['10.30.30.2/32'] },
      { pub_key: 'KEYC', fqdn: 'peer-c.netbird.cloud', vpn_addresses: ['10.30.30.3/32'] }
    ]
  };
  // Device-level counters stay ENCAPSULATED (tx_bytes) / TUN-delivered
  // (rx_bytes_to_tun == Σ peer rx_bytes core-side); the per-peer values are
  // plaintext and MUST NOT be presented as, or derived from, those.
  const byteStatus = {
    ...detailStatus,
    wg: { ready: true, peers_with_session: 4, tx_packets: 11, tx_bytes: 8000,
      rx_packets: 9, rx_bytes_to_tun: 6000, dropped_no_route: 0, decrypt_errors: 0,
      wg_peers: byteWgPeers }
  };
  const byteDoc = m.buildUiStatusDocument(byteStatus, byteNet, 'ui-16', 1);
  const byteJson = JSON.parse(JSON.stringify(byteDoc));
  check('bytes: counters are attributed to their own peer, zero kept as 0',
    byteDoc.peers[0].tx_bytes === 1234 && byteDoc.peers[0].rx_bytes === 0 &&
    byteJson.peers[0].tx_bytes === 1234 && byteJson.peers[0].rx_bytes === 0);
  check('bytes: a peer without counters carries NO keys, never a 0',
    byteDoc.peers[1].tx_bytes === undefined && byteDoc.peers[1].rx_bytes === undefined &&
    !('tx_bytes' in byteJson.peers[1]) && !('rx_bytes' in byteJson.peers[1]));
  check('bytes: the two directions are independent (null rx stays absent)',
    byteDoc.peers[2].tx_bytes === 7 && byteDoc.peers[2].rx_bytes === undefined &&
    !('rx_bytes' in byteJson.peers[2]));
  check('bytes: no cross-peer leakage and no map-invisible peer in the list',
    byteDoc.peers.length === 3 &&
    byteDoc.peers[0].tx_bytes !== byteDoc.peers[2].tx_bytes &&
    byteDoc.peers.every((p: { tx_bytes?: number; rx_bytes?: number }): boolean =>
      p.tx_bytes !== 5000 && p.rx_bytes !== 6000));
  check('bytes: a map-only peer gets no counters from another peer\'s session',
    m.mergePeers(byteNet, [{ pub_key_b64: 'KEYA', session_established: true,
      expired: false, tx_bytes: 5, rx_bytes: 5 }])[1].tx_bytes === undefined);
  check('bytes: the device aggregate keeps its own (encapsulated) semantics',
    byteDoc.wg_traffic.tx_bytes === 8000 && byteDoc.wg_traffic.rx_bytes_to_tun === 6000 &&
    byteDoc.peers[0].tx_bytes === 1234 &&
    byteDoc.peers[0].tx_bytes !== byteDoc.wg_traffic.tx_bytes);

  // ---- 2026-10-07 per-peer ACTUAL relay lane (core `peer_relays`) ----------
  // `peer_relays` is the ACTUAL attached-lane table (public key -> the
  // rel:///rels:// URL the lane REALLY rides) — never the remote's
  // advertisement and never the local/global home URL. Three states:
  //   null/absent source      -> 未知 (known=false, key ABSENT in the JSON)
  //   delivered [] / no entry -> no attached lane (null, known=true)
  //   entry                   -> the actual lane URL
  const laneNet = {
    available: true,
    address: '10.64.0.7',
    peers: [
      { pub_key: 'KEYA', fqdn: 'peer-a.netbird.cloud', vpn_addresses: ['10.30.30.1/32'] },
      { pub_key: 'KEYB', fqdn: 'peer-b.netbird.cloud', vpn_addresses: ['10.30.30.2/32'] },
      { pub_key: 'KEYC', fqdn: 'peer-c.netbird.cloud', vpn_addresses: [] }
    ]
  };
  const laneWg = [
    { pub_key_b64: 'KEYA', session_established: true, expired: false,
      on_carrier: true, last_handshake_s: 3 },
    { pub_key_b64: 'KEYB', session_established: true, expired: false,
      on_carrier: true, last_handshake_s: 4 }
  ];
  // TWO DIFFERENT actual relays for two peers of the SAME device: KEYA's lane
  // rides the home relay, KEYB's lane really rides a foreign one (the
  // controller rule picks the meeting point per peer). Each published peer
  // must carry its OWN lane URL.
  const lanes = m.mergePeers(laneNet, laneWg, [
    { pub_key_b64: 'KEYA', relay_url: 'rels://home.relay:443' },
    { pub_key_b64: 'KEYB', relay_url: 'rel://foreign.relay:33000' }
  ]);
  check('relay lane: different peers keep their own home vs foreign actual url',
    lanes[0].actual_relay_known === true &&
    lanes[0].actual_relay_url === 'rels://home.relay:443' &&
    lanes[1].actual_relay_known === true &&
    lanes[1].actual_relay_url === 'rel://foreign.relay:33000' &&
    lanes[0].actual_relay_url !== lanes[1].actual_relay_url);
  check('relay lane: a delivered table with no entry is "no attached lane" (null), not 未知',
    lanes[2].actual_relay_known === true && lanes[2].actual_relay_url === null &&
    'actual_relay_url' in JSON.parse(JSON.stringify(lanes[2])));
  const laneEmpty = m.mergePeers(laneNet, laneWg, []);
  check('relay lane: an empty table (carrier armed, no lane) is "no attached lane" for all',
    laneEmpty.every((p: { actual_relay_known: boolean;
      actual_relay_url?: string | null }): boolean =>
      p.actual_relay_known === true && p.actual_relay_url === null));
  const laneNull = m.mergePeers(laneNet, laneWg, null);
  const laneNullJson = JSON.parse(JSON.stringify(laneNull));
  const laneAbsent = m.mergePeers(laneNet, laneWg);
  check('relay lane: a null/absent source is 未知 — the key is OMITTED, never null',
    laneNull[0].actual_relay_known === false && laneNull[0].actual_relay_url === undefined &&
    !('actual_relay_url' in laneNullJson[0]) &&
    laneAbsent[0].actual_relay_known === false &&
    laneAbsent[0].actual_relay_url === undefined &&
    !('actual_relay_url' in JSON.parse(JSON.stringify(laneAbsent))[0]));
  const laneMalformed = m.mergePeers(laneNet, laneWg,
    [{ pub_key_b64: 'KEYA', relay_url: '' }]);
  check('relay lane: an unusable entry stays 未知, never "no attached lane"',
    laneMalformed[0].actual_relay_known === false &&
    laneMalformed[0].actual_relay_url === undefined &&
    laneMalformed[1].actual_relay_known === true &&
    laneMalformed[1].actual_relay_url === null);
  check('relay lane: a lane URL never fabricates a connection label',
    laneEmpty[2].connection === 'unknown' && laneEmpty[2].carrier_known === false &&
    m.mergePeers({ available: true, peers: [{ pub_key: 'KEYX', vpn_addresses: [] }] },
      [], [{ pub_key_b64: 'KEYX', relay_url: 'rels://home.relay:443' }])[0]
      .connection === 'unknown' &&
    m.mergePeers({ available: true, peers: [{ pub_key: 'KEYX', vpn_addresses: [] }] },
      [], [{ pub_key_b64: 'KEYX', relay_url: 'rels://home.relay:443' }])[0]
      .actual_relay_url === 'rels://home.relay:443');
  // The legacy document-level `relay_url` is the LOCAL relay session (the
  // configured home URL when nothing is dialed). It must NEVER be copied onto
  // a peer: with no `peer_relays` table the peers stay 未知, and the home URL
  // appears only as the document field (the UI's 本机中继地址 row).
  const homeDoc = m.buildUiStatusDocument({
    ...status,
    relay: { enabled: true, state: 'ready', current_url: 'rels://home.relay:443',
      urls: ['rels://home.relay:443'] },
    peer_relays: null
  }, net, 'ui-21', 1);
  check('relay lane: the local/home relay_url is never substituted for a peer actual url',
    homeDoc.relay_url === 'rels://home.relay:443' &&
    homeDoc.peers.every((p: { actual_relay_known: boolean;
      actual_relay_url?: string | null }): boolean => p.actual_relay_known === false &&
      p.actual_relay_url === undefined) &&
    JSON.stringify(homeDoc.peers).indexOf('rels://home.relay:443') < 0);
  const laneJoined = m.buildUiStatusDocument({
    ...status,
    relay: { enabled: true, state: 'ready', current_url: 'rels://home.relay:443',
      urls: ['rels://home.relay:443'] },
    peer_relays: [
      { pub_key_b64: 'KEYA', relay_url: 'rels://home.relay:443' },
      { pub_key_b64: 'KEYB', relay_url: 'rel://foreign.relay:33000' },
      { pub_key_b64: 'KEYD', relay_url: 'rel://not-in-map:1' }
    ]
  }, net, 'ui-22', 1);
  check('relay lane: peer_relays joins onto the published peers by public key',
    laneJoined.peers[0].actual_relay_url === 'rels://home.relay:443' &&
    laneJoined.peers[1].actual_relay_url === 'rel://foreign.relay:33000' &&
    laneJoined.peers[2].actual_relay_known === true &&
    laneJoined.peers[2].actual_relay_url === null &&
    laneJoined.peers.length === 4 &&
    JSON.stringify(laneJoined.peers).indexOf('not-in-map') < 0);

  // ---- 2026-10-07 per-peer ICE detail (core `ice_peers`) -------------------
  // `ice_peers` is the per-peer ICE table (one detail per ICE-orchestrator
  // peer, public-key sorted) whose published shape is pinned by the core's
  // connector.rs ice_detail_json / ice_endpoint_json: pub_key, state token,
  // controlling (bool|null), selected_local/selected_remote (endpoint|null),
  // endpoint_applied, reachable, last_error ({class,status}|null). The shell
  // joins it by public key. Three states:
  //   null/absent table, or no entry for the peer -> ICE 未知, key OMITTED
  //   readable entry with null endpoints          -> no pair selected yet
  //   readable entry with endpoints               -> that pair was selected
  const iceNet = {
    available: true,
    address: '10.64.0.7',
    peers: [
      { pub_key: 'KEYA', fqdn: 'peer-a.netbird.cloud', vpn_addresses: ['10.30.30.1/32'] },
      { pub_key: 'KEYB', fqdn: 'peer-b.netbird.cloud', vpn_addresses: ['10.30.30.2/32'] },
      { pub_key: 'KEYC', fqdn: 'peer-c.netbird.cloud', vpn_addresses: ['10.30.30.3/32'] }
    ]
  };
  const iceWg = [
    { pub_key_b64: 'KEYA', session_established: true, expired: false,
      endpoint: '203.0.113.9:40000', last_handshake_s: 3 },
    { pub_key_b64: 'KEYB', session_established: true, expired: false,
      on_carrier: true, last_handshake_s: 4 }
    // KEYC: no local WG session — its ICE detail must not label the connection
  ];
  const icePeers = [
    { pub_key: 'KEYA', state: 'connected', controlling: true,
      selected_local: { type: 'srflx', address: '198.51.100.7', port: 51820 },
      selected_remote: { type: 'prflx', address: '203.0.113.9', port: 40000 },
      endpoint_applied: true, reachable: true, last_error: null },
    { pub_key: 'KEYB', state: 'disconnected', controlling: false,
      selected_local: { type: 'host', address: '192.168.1.5', port: 51820 },
      selected_remote: { type: 'relay', address: '198.51.100.20', port: 3478 },
      endpoint_applied: true, reachable: false,
      last_error: { class: 'timeout', status: 0 } },
    { pub_key: 'KEYC', state: 'gathering', controlling: null,
      selected_local: null, selected_remote: null,
      endpoint_applied: false, reachable: false, last_error: null }
  ];
  const iced = m.mergePeers(iceNet, iceWg, null, icePeers);
  check('ice: the selected pair is published per peer with both endpoints',
    iced[0].ice.state === 'connected' && iced[0].ice.controlling === true &&
    iced[0].ice.selected_local.type === 'srflx' &&
    iced[0].ice.selected_local.address === '198.51.100.7' &&
    iced[0].ice.selected_local.port === 51820 &&
    iced[0].ice.selected_remote.type === 'prflx' &&
    iced[0].ice.selected_remote.address === '203.0.113.9' &&
    iced[0].ice.selected_remote.port === 40000 &&
    iced[0].ice.endpoint_applied === true && iced[0].ice.reachable === true &&
    iced[0].ice.last_error_class === null);
  check('ice: a disconnected peer RETAINS its selected endpoints but is unreachable',
    iced[1].ice.state === 'disconnected' && iced[1].ice.reachable === false &&
    iced[1].ice.endpoint_applied === true && iced[1].ice.controlling === false &&
    iced[1].ice.selected_local.address === '192.168.1.5' &&
    iced[1].ice.selected_remote.type === 'relay' &&
    iced[1].ice.selected_remote.address === '198.51.100.20' &&
    iced[1].ice.last_error_class === 'timeout');
  check('ice: no selected pair yet keeps both endpoints null and the role unknown',
    iced[2].ice.state === 'gathering' && iced[2].ice.controlling === null &&
    iced[2].ice.selected_local === null && iced[2].ice.selected_remote === null &&
    iced[2].ice.endpoint_applied === false && iced[2].ice.reachable === false);
  check('ice: endpoint presence never fabricates the connection label',
    iced[0].connection === 'direct' && iced[1].connection === 'relay' &&
    iced[2].connection === 'unknown' && iced[2].online === false &&
    iced[1].ice.reachable === false && iced[1].ice.state === 'disconnected');
  check('ice: details are attributed per peer, never shared or substituted',
    iced[0].ice.selected_remote.address !== iced[1].ice.selected_remote.address &&
    iced[1].ice.state !== iced[0].ice.state && iced[2].ice.state === 'gathering' &&
    iced[2].ice.selected_local === null);
  // unknown / missing source: NO fabricated state, the `ice` key is omitted
  const iceAbsent = m.mergePeers(iceNet, iceWg);
  const iceNull = m.mergePeers(iceNet, iceWg, null, null);
  const iceEmpty = m.mergePeers(iceNet, iceWg, null, []);
  check('ice: a null/absent table leaves every peer ICE 未知 — the key is OMITTED',
    iceAbsent[0].ice === undefined && iceNull[0].ice === undefined &&
    !('ice' in JSON.parse(JSON.stringify(iceAbsent))[0]) &&
    !('ice' in JSON.parse(JSON.stringify(iceNull))[1]));
  check('ice: a delivered table with no entry for a peer is 未知, not a default',
    iceEmpty[0].ice === undefined && iceEmpty[2].ice === undefined &&
    !('ice' in JSON.parse(JSON.stringify(iceEmpty))[0]));
  const iceExtra = m.mergePeers(iceNet, iceWg, null, [{ pub_key: 'KEYD',
    state: 'connected', controlling: true,
    selected_local: { type: 'host', address: '10.99.99.99', port: 1 },
    selected_remote: { type: 'host', address: '10.99.99.98', port: 2 },
    endpoint_applied: true, reachable: true, last_error: null }]);
  check('ice: an entry for a key outside the map creates no peer and leaks nowhere',
    iceExtra.length === 3 && JSON.stringify(iceExtra).indexOf('10.99.99.99') < 0 &&
    iceExtra[0].ice === undefined);
  // an entry that cannot be read as the documented contract is 未知 too —
  // never partially published, never a made-up state/endpoint/role
  const iceOf = (entry: object): unknown =>
    (m.mergePeers(iceNet, iceWg, null, [entry])[0] as { ice?: unknown }).ice;
  check('ice: an unknown state token is 未知, never a made-up state',
    iceOf({ pub_key: 'KEYA', state: 'weird', controlling: true,
      selected_local: null, selected_remote: null, endpoint_applied: false,
      reachable: false, last_error: null }) === undefined);
  check('ice: an unreadable endpoint (type/address/port) makes the detail 未知',
    iceOf({ pub_key: 'KEYA', state: 'connected', controlling: true,
      selected_local: { type: 'stun', address: '198.51.100.7', port: 51820 },
      selected_remote: null, endpoint_applied: true, reachable: true,
      last_error: null }) === undefined &&
    iceOf({ pub_key: 'KEYA', state: 'connected', controlling: true,
      selected_local: { type: 'host', address: '', port: 51820 },
      selected_remote: null, endpoint_applied: true, reachable: true,
      last_error: null }) === undefined &&
    iceOf({ pub_key: 'KEYA', state: 'connected', controlling: true,
      selected_local: null,
      selected_remote: { type: 'host', address: '1.2.3.4', port: 70000 },
      endpoint_applied: true, reachable: true, last_error: null }) === undefined);
  check('ice: missing landing/reachability booleans or a bad error class is 未知',
    iceOf({ pub_key: 'KEYA', state: 'connected', controlling: true,
      selected_local: null, selected_remote: null, endpoint_applied: true,
      last_error: null }) === undefined &&
    iceOf({ pub_key: 'KEYA', state: 'connected', controlling: true,
      selected_local: null, selected_remote: null, endpoint_applied: true,
      reachable: true, last_error: { status: 7 } }) === undefined);
  check('ice: an unreadable entry never degrades the rest of the peer',
    m.mergePeers(iceNet, iceWg, null, [{ pub_key: 'KEYA', state: 'weird' }])[0]
      .connection === 'direct' &&
    m.mergePeers(iceNet, iceWg, null, [{ pub_key: 'KEYA', state: 'weird' }])[0]
      .endpoint === '203.0.113.9:40000' &&
    m.mergePeers(iceNet, iceWg, null, [{ pub_key: 'KEYA', state: 'weird' }])[0]
      .carrier_known === true);
  // the whole pipeline: connector_status().ice_peers -> published document
  const iceDoc = m.buildUiStatusDocument({ ...status, ice_peers: icePeers },
    iceNet, 'ui-23', 1);
  const iceDocJson = JSON.parse(JSON.stringify(iceDoc));
  check('ice: connector_status().ice_peers joins onto the published peers',
    iceDoc.peers.length === 3 && iceDoc.peers[0].ice.state === 'connected' &&
    iceDoc.peers[1].ice.state === 'disconnected' &&
    iceDoc.peers[2].ice.state === 'gathering' &&
    iceDocJson.peers[1].ice.selected_remote.port === 3478);
  check('ice: the published detail carries exactly the documented public keys',
    Object.keys(iceDocJson.peers[0].ice).sort().join(',') ===
      'controlling,endpoint_applied,last_error_class,reachable,selected_local,' +
      'selected_remote,state' &&
    Object.keys(iceDocJson.peers[0].ice.selected_local).sort().join(',') ===
      'address,port,type' &&
    iceDocJson.peers[2].ice.selected_local === null &&
    'selected_local' in iceDocJson.peers[2].ice &&
    JSON.stringify(iceDocJson.peers).indexOf('ufrag') < 0 &&
    JSON.stringify(iceDocJson.peers).indexOf('pwd') < 0);
  check('ice: a status without the table publishes no ice member',
    !('ice' in JSON.parse(JSON.stringify(
      m.buildUiStatusDocument(status, iceNet, 'ui-24', 1))).peers[0]));

  // ---- 2026-10-07 core uptime stamps (verified shape: Unix SECONDS | null) --
  const uptimeDoc = m.buildUiStatusDocument({ ...byteStatus, started_at_unix: 1799999000,
    last_update_unix: 1800000000 }, byteNet, 'ui-17', 1);
  check('uptime: core process stamps ride the document verbatim (Unix seconds)',
    uptimeDoc.started_at_unix === 1799999000 && uptimeDoc.last_update_unix === 1800000000 &&
    JSON.parse(JSON.stringify(uptimeDoc)).started_at_unix === 1799999000);
  const noUptimeDoc = m.buildUiStatusDocument(byteStatus, byteNet, 'ui-18', 1);
  const noUptimeJson = JSON.parse(JSON.stringify(noUptimeDoc));
  check('uptime: no core stamps ⇒ both fields omitted, never an uptime from 1970',
    noUptimeDoc.started_at_unix === undefined && noUptimeDoc.last_update_unix === undefined &&
    !('started_at_unix' in noUptimeJson) && !('last_update_unix' in noUptimeJson));
  const nullUptime = m.buildUiStatusDocument({ ...byteStatus, started_at_unix: null,
    last_update_unix: null }, byteNet, 'ui-19', 1);
  const zeroUptime = m.buildUiStatusDocument({ ...byteStatus, started_at_unix: 0,
    last_update_unix: -5 }, byteNet, 'ui-20', 1);
  check('uptime: null / zero / negative stamps are omitted, not published',
    nullUptime.started_at_unix === undefined && nullUptime.last_update_unix === undefined &&
    zeroUptime.started_at_unix === undefined && zeroUptime.last_update_unix === undefined &&
    !('started_at_unix' in JSON.parse(JSON.stringify(zeroUptime))));
  check('uptime: the stamp is a process time, not a management state verdict',
    uptimeDoc.started_at_unix !== undefined && uptimeDoc.signal_registered ===
      detailStatus.signal.registered && uptimeDoc.state === detailStatus.state);

  // ---- home-card/document consistency (defect 2026-10-04) -----------------
  // The SAME document feeds the home card and the peers tab. 已连接 must
  // require the document's OWN overlay IP (the network map arrived and
  // carried one) AND the vpn-tun interface being listed (roam-1005-0014: the
  // interface was system-removed while the core kept reporting connected):
  // a map-less / interface-less document renders 连接中… / 无法连接服务器,
  // never 已连接 — and its peers list is empty in the same breath, so the
  // home card's 对端 row and the peers tab can never disagree.
  const homeHasIp = (d: { address?: string | null }): boolean =>
    d.address !== undefined && d.address !== null && d.address.length > 0;
  check('home: 已连接 requires the document address',
    m.homeStateText(doc.state, false, false, homeHasIp(doc), true) === '已连接' &&
    homeHasIp(doc) === true);
  check('home: a map-less document (state connected!) is never 已连接',
    noMap.state === 'connected' && homeHasIp(noMap) === false &&
    noMap.peers.length === 0 &&
    m.homeStateText(noMap.state, false, false, homeHasIp(noMap), true) === '连接中…');
  check('home: interface removed (roam-1005-0014) is never 已连接',
    m.homeStateText('connected', false, false, true, false) === '连接中…');
  check('home: interface present + map is 已连接',
    m.homeStateText('connected', false, false, true, true) === '已连接');

  // ---- filterPeers / sortPeers / onlinePeerCount ----------------------------
  check('filter by name (case-insensitive)',
    m.filterPeers(merged, 'PEER-A').length === 1);
  check('filter by ip', m.filterPeers(merged, '10.30').length === 1);
  check('filter by key fragment', m.filterPeers(merged, 'keyb').length === 1);
  check('blank filter keeps all', m.filterPeers(merged, '   ').length === 4);
  check('no match is empty', m.filterPeers(merged, 'zzz').length === 0);
  const sorted = m.sortPeers([merged[2], merged[0], merged[1]]);
  check('sort puts online peers first then name order',
    sorted[0].name.startsWith('KEYB') && sorted[1].name === 'peer-a.netbird.cloud' &&
    sorted[2].name === 'peer-c.netbird.cloud');
  check('sort ranks connected before idle before offline',
    m.sortPeers([merged[3], merged[2], merged[0], merged[1]]).map((p: { pub_key: string }) =>
      p.pub_key).join(',') === 'KEYB,KEYA,KEYC,KEYOFF');
  check('online count', m.onlinePeerCount(merged) === 2);

  // ---- Chinese display texts -----------------------------------------------
  check('active states', m.vpnActiveState('connected') && m.vpnActiveState('connecting') &&
    m.vpnActiveState('reconnecting'));
  check('dead states are inactive', !m.vpnActiveState('disconnected') &&
    !m.vpnActiveState('failed') && !m.vpnActiveState('stopped') &&
    !m.vpnActiveState('') && !m.vpnActiveState('closed'));
  check('home text: connected WITH the overlay ip',
    m.homeStateText('connected', false, false, true, true) === '已连接');
  check('home text: connected but no map yet is honest (连接中)',
    m.homeStateText('connected', false, false, false, true) === '连接中…');
  check('home text: connecting/reconnecting',
    m.homeStateText('connecting', false, false, false, false) === '连接中…' &&
    m.homeStateText('reconnecting', false, false, false, false) === '连接中…');
  // 2026-10-12 (official state set): a failed connect is upstream's DISCONNECTED —
  // the failure itself is only the Toast / the error line, never a latched state
  // (HomeFragment.java:501-522, :530-547; MainActivity.java:1030-1036).
  check('home text: failed is the official Disconnected, not 连接失败',
    m.homeStateText('failed', false, false, false, false) === '未连接');
  check('home text: disconnected/closed are Disconnected',
    m.homeStateText('disconnected', false, false, false, false) === '未连接' &&
    m.homeStateText('closed', false, false, false, false) === '未连接');
  check('home text: no document at all is Disconnected (upstream cold seed)',
    m.homeStateText('', false, false, false, false) === '未连接');
  check('home text: pending start without a document is 连接中 (the tap latch)',
    m.homeStateText('', true, false, false, false) === '连接中…');
  check('home text: pending stop wins',
    m.homeStateText('connected', true, true, true, true) === '正在断开…');
  check('home text: pending stop wins even without the map',
    m.homeStateText('connected', false, true, false, true) === '正在断开…');
  // upstream's sticky loginRequired flag replaces the Disconnected label
  // (HomeFragment.java:114, :873-883) — it must also beat the tap latch, so the
  // card says WHY the attempt is going to fail.
  check('home text: login required overrides Disconnected',
    m.homeStateText('', false, false, false, false, true) === '需要登录' &&
    m.homeStateText('failed', false, false, false, false, true) === '需要登录');
  check('home text: login required beats the tap latch too',
    m.homeStateText('', true, false, false, false, true) === '需要登录');
  check('home text: login required never overrides a live state',
    m.homeStateText('connecting', false, false, false, false, true) === '连接中…' &&
    m.homeStateText('connected', false, false, true, true, true) === '已连接');
  check('home text: pending stop beats login required',
    m.homeStateText('', true, true, false, false, true) === '正在断开…');
  check('home text: no failure flag / no login flag keeps the official default',
    m.homeStateText('', true, false, false, false, undefined) === '连接中…' &&
    m.homeStateText('', false, false, false, false, undefined) === '未连接');
  check('connection labels', m.connectionLabel('direct') === '直连' &&
    m.connectionLabel('relay') === '经中继' && m.connectionLabel('unknown') === '未知');
  check('handshake texts', m.lastHandshakeText(null) === '—' &&
    m.lastHandshakeText(5) === '刚刚' && m.lastHandshakeText(42) === '42 秒前' &&
    m.lastHandshakeText(120) === '2 分钟前' && m.lastHandshakeText(7200) === '2 小时前' &&
    m.lastHandshakeText(172800) === '2 天前');
  check('error texts', m.lastErrorText('none') === '' &&
    m.lastErrorText('auth').indexOf('Setup Key') >= 0 &&
    m.lastErrorText('timeout').indexOf('超时') >= 0 &&
    m.lastErrorText('network').indexOf('网络') >= 0 &&
    m.lastErrorText('other').length > 0);
  check('auth error text points at the settings reconfigure path',
    m.lastErrorText('auth') === '设备未注册或 Setup Key 无效，请到设置重新配置');
  // 2026-10-06 登录失败可见性: the failure-notice reason line
  check('failure texts: class mapping reuses lastErrorText',
    m.connectFailureText('auth') === m.lastErrorText('auth') &&
    m.connectFailureText('timeout').indexOf('超时') >= 0);
  check('failure texts: none/empty is the generic server line',
    m.connectFailureText('none') === '无法连接管理服务器，请检查网络或服务器配置' &&
    m.connectFailureText('') === '无法连接管理服务器，请检查网络或服务器配置');
  check('failure texts: extension stage tokens',
    m.connectFailureText('credentials-missing').indexOf('Setup Key') >= 0 &&
    m.connectFailureText('no-signal-uri').indexOf('Signal') >= 0);
  // 2026-10-12: the core's own needs-login class (upstream StatusNeedsLogin,
  // client/internal/state.go:15) is rendered as the OFFICIAL label — Android's
  // `main_status_login_required` = "Login required"
  // (android-client/app/src/main/res/values/strings.xml:32), i.e. 「需要登录」.
  // It is reachable from BOTH class surfaces (live status last_error_class and
  // the persistent connect-failure notice) and is NOT the setup-key/reconfigure
  // line. The class token itself is cross-checked against the REST of the
  // pipeline: the core's ErrorClass token and the Index.ets wiring both have to
  // spell it the same way.
  check('needs-login renders the official Login required label and is not the auth text',
    m.NEEDS_LOGIN_CLASS === 'needs-login' &&
    m.NEEDS_LOGIN_TEXT === '需要登录' &&
    m.lastErrorText(m.NEEDS_LOGIN_CLASS) === m.NEEDS_LOGIN_TEXT &&
    m.connectFailureText(m.NEEDS_LOGIN_CLASS) === m.NEEDS_LOGIN_TEXT &&
    m.connectFailureText(m.NEEDS_LOGIN_CLASS) !== m.connectFailureText('auth'));
  const CORE_CONNECTOR = readFileSync(join(here, '..', '..', 'core', 'src', 'connector.rs'), 'utf8');
  const PAGE = readFileSync(join(here, '..', 'src', 'main', 'ets', 'pages', 'Index.ets'), 'utf8');
  check('the needs-login token is the SAME string in the core, the UI model and the page',
    CORE_CONNECTOR.includes('ErrorClass::NeedsLogin { .. } => "needs-login"') &&
    CORE_CONNECTOR.includes('ManagementError::NeedsLogin') &&
    PAGE.includes("import { UiPeerEntry, UiPeerIceDetail, UiPeerIceEndpoint, NEEDS_LOGIN_CLASS,") &&
    PAGE.includes('if (this.connectFailureClass === NEEDS_LOGIN_CLASS)') &&
    PAGE.includes('return NEEDS_LOGIN_TEXT;') &&
    !PAGE.includes('账户登录已过期'));
  // 2026-10-12 rel/0.0.25 integration: the retry branch's official home-card
  // state set reaches 「需要登录」 through the sticky loginRequired flag, so that
  // flag has to be keyed on the CORE's own needs-login token — fix/retry wrote
  // it against the pre-auth shell string 'account-login-required', which
  // fix/auth-login retired together with its last producer (that string would
  // have made the official label permanently unreachable). The extension's
  // credentialFailure and the page's flag must name the SAME class.
  check('the sticky loginRequired flag is keyed on the core needs-login token',
    PAGE.includes('private loginRequired(): boolean') &&
    PAGE.includes('return this.connectFailureClass === NEEDS_LOGIN_CLASS;') &&
    !PAGE.includes("=== 'account-login-required'"));
  const EXTENSION = readFileSync(join(here, '..', 'src', 'main', 'ets',
    'vpnextensionability', 'NetBirdVpnExtensionAbility.ets'), 'utf8');
  check('the extension persists the SAME needs-login class the page keys on',
    (EXTENSION.match(/credentialFailure = 'needs-login';/g) ?? []).length === 2 &&
    !EXTENSION.includes("'account-login-required'") &&
    EXTENSION.includes('this.writeConnectFailureForUi(failureClass);'));

  // ---- 2026-10-11/12 retry re-entry: connect-entry + pending action ---------
  // The extension's entry gate: ONLY a connector run in flight refuses an entry.
  // A stopped/aborted run (`running:false`) and an unanswerable probe (null —
  // the core did not answer) both let the entry through, so a retry can never
  // be swallowed by a stale instance latch again.
  check('entry gate: a running connector refuses the entry',
    m.connectEntryDecision(true) === m.CONNECT_ENTRY_REJECT_RUNNING);
  check('entry gate: a stopped/aborted run re-enters',
    m.connectEntryDecision(false) === m.CONNECT_ENTRY_RUN);
  check('entry gate: an unanswerable probe re-enters (null is not proof)',
    m.connectEntryDecision(null) === m.CONNECT_ENTRY_RUN);
  // The page's start leg: only the extension's own stop evidence allows a
  // direct start.
  check('start leg: gone -> direct start, present -> stop first',
    m.connectStartLeg(true) === m.CONNECT_LEG_DIRECT &&
    m.connectStartLeg(false) === m.CONNECT_LEG_AFTER_STOP);
  // The engine report: the core's six states reduced to the three the official tap
  // handling switches on (HomeFragment.java:66; client/core/src/state.rs:31-52).
  check('engine report: connected',
    m.engineReportState('connected') === m.ENGINE_REPORT_CONNECTED);
  check('engine report: connecting and reconnecting are BOTH Connecting',
    m.engineReportState('connecting') === m.ENGINE_REPORT_CONNECTING &&
    m.engineReportState('reconnecting') === m.ENGINE_REPORT_CONNECTING);
  check('engine report: failed/disconnected/closed are ALL Disconnected',
    m.engineReportState('failed') === m.ENGINE_REPORT_DISCONNECTED &&
    m.engineReportState('disconnected') === m.ENGINE_REPORT_DISCONNECTED &&
    m.engineReportState('closed') === m.ENGINE_REPORT_DISCONNECTED);
  // No fresh document is NOT a Disconnected verdict: upstream releases the tap
  // latch on a REPORT or on the 7 s timer, never on the absence of one.
  check('engine report: no document at all is NO report, not Disconnected',
    m.engineReportState('') === m.ENGINE_REPORT_NONE &&
    m.ENGINE_REPORT_NONE !== m.ENGINE_REPORT_DISCONNECTED);
  // The 7 s pending action: upstream PENDING_ACTION_TIMEOUT_MS (:70).
  check('pending action window is the official 7 s, not our old 30 s',
    m.PENDING_ACTION_TIMEOUT_MS === 7000);
  check('force-cancel delay is the official 2 s',
    m.FORCE_CANCEL_DELAY_MS === 2000);
  const idle = { target: m.PENDING_TARGET_NONE, report: m.ENGINE_REPORT_DISCONNECTED,
    deadlineMs: 0, forceCancelArmedAtMs: 0, forceCancelOpen: false, nowMs: 5000 };
  const idleDecision = m.decideTransition(idle);
  check('pending action: no tap is idle and the force-cancel window is closed',
    idleDecision.target === m.PENDING_TARGET_NONE && idleDecision.deadlineMs === 0 &&
    idleDecision.transition === false && idleDecision.forceCancelOpen === false &&
    idleDecision.expired === false && idleDecision.settled === false);
  // A tap requesting connect, with the engine not yet reporting anything: the
  // window is armed at the tap and nothing settles it before 7 s.
  const tap = { target: m.PENDING_TARGET_CONNECTED, report: m.ENGINE_REPORT_CONNECTING,
    deadlineMs: 1000 + m.PENDING_ACTION_TIMEOUT_MS, forceCancelArmedAtMs: 0,
    forceCancelOpen: false, nowMs: 5000 };
  const tapDecision = m.decideTransition(tap);
  check('pending action: the tap latch is kept while the deadline runs',
    tapDecision.target === m.PENDING_TARGET_CONNECTED &&
    tapDecision.deadlineMs === 1000 + m.PENDING_ACTION_TIMEOUT_MS &&
    tapDecision.transition === true && tapDecision.settled === false);
  // …and a start with NO report at all is kept too: the engine has simply not
  // answered yet, which is not a Disconnected verdict.
  check('pending action: no report yet never settles the tap latch',
    m.decideTransition({ target: m.PENDING_TARGET_CONNECTED, report: m.ENGINE_REPORT_NONE,
      deadlineMs: 8000, forceCancelArmedAtMs: 4000, forceCancelOpen: true,
      nowMs: 5000 }).settled === false &&
    m.decideTransition({ target: m.PENDING_TARGET_CONNECTED, report: m.ENGINE_REPORT_NONE,
      deadlineMs: 8000, forceCancelArmedAtMs: 4000, forceCancelOpen: true,
      nowMs: 5000 }).target === m.PENDING_TARGET_CONNECTED);
  // The engine reached the requested state: the latch is released (upstream
  // shouldSuppressPaint state == target) and the real report paints.
  const reached = m.decideTransition({ target: m.PENDING_TARGET_CONNECTED,
    report: m.ENGINE_REPORT_CONNECTED, deadlineMs: 9000, forceCancelArmedAtMs: 4000,
    forceCancelOpen: false, nowMs: 5000 });
  check('pending action: the engine reaching the target releases the latch',
    reached.target === m.PENDING_TARGET_NONE && reached.deadlineMs === 0 &&
    reached.settled === true && reached.expired === false);
  // A connect that ends Disconnected FAILED: upstream clears the latch on that
  // report instead of latching on until the timeout (:515-522).
  const failed = m.decideTransition({ target: m.PENDING_TARGET_CONNECTED,
    report: m.ENGINE_REPORT_DISCONNECTED, deadlineMs: 9000, forceCancelArmedAtMs: 4000,
    forceCancelOpen: false, nowMs: 5000 });
  check('pending action: a failed connect settles the latch immediately',
    failed.target === m.PENDING_TARGET_NONE && failed.settled === true);
  // Nothing reported and the deadline passed: expirePendingAction — release the
  // latch and fall back to the last reported real state.
  check('pending action: nothing reported inside the window stays latched',
    m.decideTransition({ target: m.PENDING_TARGET_CONNECTED,
      report: m.ENGINE_REPORT_NONE, deadlineMs: 1000 + m.PENDING_ACTION_TIMEOUT_MS,
      forceCancelArmedAtMs: 1000, forceCancelOpen: false,
      nowMs: 1000 + m.PENDING_ACTION_TIMEOUT_MS - 1 }).expired === false);
  const expiredDecision = m.decideTransition({ target: m.PENDING_TARGET_CONNECTED,
    report: m.ENGINE_REPORT_NONE, deadlineMs: 1000 + m.PENDING_ACTION_TIMEOUT_MS,
    forceCancelArmedAtMs: 1000, forceCancelOpen: true,
    nowMs: 1000 + m.PENDING_ACTION_TIMEOUT_MS });
  check('pending action: past the window the latch is released (expirePendingAction)',
    expiredDecision.expired === true && expiredDecision.target === m.PENDING_TARGET_NONE &&
    expiredDecision.deadlineMs === 0 && expiredDecision.settled === false);
  // A transition that is really reporting Connecting stays a transition even after
  // the tap latch expired — the engine is still connecting (upstream
  // isTransitioning()), so the toggle stays in its force-cancel regime.
  check('pending action: a real Connecting report keeps the transition painted',
    m.decideTransition({ target: m.PENDING_TARGET_NONE,
      report: m.ENGINE_REPORT_CONNECTING, deadlineMs: 0, forceCancelArmedAtMs: 0,
      forceCancelOpen: false, nowMs: 5000 }).transition === true);
  // The force-cancel window: armed while a transition is painted, opened only after
  // FORCE_CANCEL_DELAY_MS (HomeFragment.java:83, :586-596, :598-612).
  const armedJustNow = m.decideTransition({ target: m.PENDING_TARGET_NONE,
    report: m.ENGINE_REPORT_CONNECTING, deadlineMs: 0, forceCancelArmedAtMs: 0,
    forceCancelOpen: false, nowMs: 10000 });
  check('force cancel: arming a transition records the arm moment, opens nothing',
    armedJustNow.forceCancelArmedAtMs === 10000 && armedJustNow.forceCancelOpen === false);
  check('force cancel: still closed one millisecond before the 2 s delay',
    m.decideTransition({ target: m.PENDING_TARGET_NONE,
      report: m.ENGINE_REPORT_CONNECTING, deadlineMs: 0, forceCancelArmedAtMs: 10000,
      forceCancelOpen: false, nowMs: 10000 + m.FORCE_CANCEL_DELAY_MS - 1 })
      .forceCancelOpen === false);
  check('force cancel: open at the 2 s delay, and it stays open',
    m.decideTransition({ target: m.PENDING_TARGET_NONE,
      report: m.ENGINE_REPORT_CONNECTING, deadlineMs: 0, forceCancelArmedAtMs: 10000,
      forceCancelOpen: false, nowMs: 10000 + m.FORCE_CANCEL_DELAY_MS })
      .forceCancelOpen === true &&
    m.decideTransition({ target: m.PENDING_TARGET_NONE,
      report: m.ENGINE_REPORT_CONNECTING, deadlineMs: 0, forceCancelArmedAtMs: 10000,
      forceCancelOpen: true, nowMs: 60000 }).forceCancelOpen === true);
  check('force cancel: a settled engine closes the window again',
    m.decideTransition({ target: m.PENDING_TARGET_NONE,
      report: m.ENGINE_REPORT_DISCONNECTED, deadlineMs: 0, forceCancelArmedAtMs: 10000,
      forceCancelOpen: true, nowMs: 60000 }).forceCancelOpen === false);
  check('force cancel: the arming timer is reset by a settled paint too',
    m.decideTransition({ target: m.PENDING_TARGET_NONE,
      report: m.ENGINE_REPORT_DISCONNECTED, deadlineMs: 0, forceCancelArmedAtMs: 10000,
      forceCancelOpen: true, nowMs: 60000 }).forceCancelArmedAtMs === 0);
  // A stop the user asked for: only the engine reporting Disconnected (or the 7 s
  // window) releases it — a stale Connected report must NOT.
  check('pending action: a stop is kept until the engine really reports Disconnected',
    m.decideTransition({ target: m.PENDING_TARGET_DISCONNECTED,
      report: m.ENGINE_REPORT_CONNECTED, deadlineMs: 9000, forceCancelArmedAtMs: 4000,
      forceCancelOpen: true, nowMs: 5000 }).target === m.PENDING_TARGET_DISCONNECTED &&
    m.decideTransition({ target: m.PENDING_TARGET_DISCONNECTED,
      report: m.ENGINE_REPORT_DISCONNECTED, deadlineMs: 9000, forceCancelArmedAtMs: 4000,
      forceCancelOpen: true, nowMs: 5000 }).settled === true);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
