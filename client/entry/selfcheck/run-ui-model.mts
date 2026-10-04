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
      { pub_key: 'KEYC', fqdn: 'peer-c.netbird.cloud', vpn_addresses: [] }
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
  check('merge keeps management order and count', merged.length === 3);
  check('merge: direct online peer',
    merged[0].online === true && merged[0].connection === 'direct' &&
    merged[0].ip === '10.30.30.1' && merged[0].name === 'peer-a.netbird.cloud' &&
    merged[0].last_handshake_s === 42);
  check('merge: relay peer without fqdn uses the short key',
    merged[1].connection === 'relay' && merged[1].name.startsWith('KEYB') &&
    merged[1].fqdn === null);
  check('merge: offline peer without session', merged[2].online === false &&
    merged[2].connection === 'unknown' && merged[2].last_handshake_s === null);
  check('merge: negative handshake stays null', m.mergePeers(net, [
    { pub_key_b64: 'KEYA', session_established: true, expired: false, last_handshake_s: -1 }
  ])[0].last_handshake_s === null);
  check('merge without a network map is empty', m.mergePeers(null, wgPeers).length === 0);
  check('merge skips malformed peers', m.mergePeers({
    available: true,
    peers: [{ pub_key: '' }, { pub_key: 'KEYA', fqdn: 'x', vpn_addresses: [] }]
  }, [])[0].pub_key === 'KEYA');

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
  check('doc appended peers merge', doc.peers.length === 3 && doc.peers[0].online === true);
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

  // ---- filterPeers / sortPeers / onlinePeerCount ----------------------------
  check('filter by name (case-insensitive)',
    m.filterPeers(merged, 'PEER-A').length === 1);
  check('filter by ip', m.filterPeers(merged, '10.30').length === 1);
  check('filter by key fragment', m.filterPeers(merged, 'keyb').length === 1);
  check('blank filter keeps all', m.filterPeers(merged, '   ').length === 3);
  check('no match is empty', m.filterPeers(merged, 'zzz').length === 0);
  const sorted = m.sortPeers([merged[2], merged[0], merged[1]]);
  check('sort puts online peers first then name order',
    sorted[0].name.startsWith('KEYB') && sorted[1].name === 'peer-a.netbird.cloud' &&
    sorted[2].name === 'peer-c.netbird.cloud');
  check('online count', m.onlinePeerCount(merged) === 2);

  // ---- Chinese display texts -----------------------------------------------
  check('active states', m.vpnActiveState('connected') && m.vpnActiveState('connecting') &&
    m.vpnActiveState('reconnecting'));
  check('dead states are inactive', !m.vpnActiveState('disconnected') &&
    !m.vpnActiveState('failed') && !m.vpnActiveState('stopped') &&
    !m.vpnActiveState('') && !m.vpnActiveState('closed'));
  check('home text: connected', m.homeStateText('connected', false, false) === '已连接');
  check('home text: connecting/reconnecting',
    m.homeStateText('connecting', false, false) === '连接中…' &&
    m.homeStateText('reconnecting', false, false) === '连接中…');
  check('home text: failed', m.homeStateText('failed', false, false) === '连接失败');
  check('home text: disconnected', m.homeStateText('disconnected', false, false) === '未连接');
  check('home text: pending start without a document',
    m.homeStateText('', true, false) === '连接中…');
  check('home text: pending stop wins',
    m.homeStateText('connected', true, true) === '正在断开…');
  check('connection labels', m.connectionLabel('direct') === '直连' &&
    m.connectionLabel('relay') === '中继' && m.connectionLabel('unknown') === '未知');
  check('handshake texts', m.lastHandshakeText(null) === '—' &&
    m.lastHandshakeText(5) === '刚刚' && m.lastHandshakeText(42) === '42 秒前' &&
    m.lastHandshakeText(120) === '2 分钟前' && m.lastHandshakeText(7200) === '2 小时前' &&
    m.lastHandshakeText(172800) === '2 天前');
  check('error texts', m.lastErrorText('none') === '' &&
    m.lastErrorText('auth').indexOf('Setup Key') >= 0 &&
    m.lastErrorText('timeout').indexOf('超时') >= 0 &&
    m.lastErrorText('network').indexOf('网络') >= 0 &&
    m.lastErrorText('other').length > 0);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
