// Host-side checks for the 0.0.35 对端列表重绘 fix (device report 2026-10-14: the
// peer status labels only refreshed after a round trip through 对端详情).
//
// Run: node client/entry/selfcheck/run-peer-list-render.mts
//
// The fix has two halves and both are pinned here against the byte-identical
// production sources:
//   * the ROW KEY is now NetBirdUiModel.peerRowKey — identity PLUS every field
//     `Index.peerRow` renders (the pure function is imported byte-identically);
//   * the WIRING in pages/Index.ets (which key is passed to ForEach, that the
//     list value is still the @State document, that the 0.0.34 stamp skip still
//     returns BEFORE that assignment, and that the detail page keeps re-reading
//     the peer by pub_key from the applied document).
// The rendered list model below is the documented ArkUI ForEach contract: a key
// that existed in the previous render reuses its child and never re-runs that
// item's generator, so only a key that CHANGED can bring new content to screen.
import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const ets = join(here, '..', 'src', 'main', 'ets');
const dir = mkdtempSync(join(tmpdir(), 'nb-peer-list-render-'));
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
  const modelFixture = join(dir, 'NetBirdUiModel.ts');
  writeFileSync(modelFixture, readFileSync(join(ets, 'vpnextensionability', 'NetBirdUiModel.ets')));
  const m = await import(pathToFileURL(modelFixture).href);
  const policyFixture = join(dir, 'NetBirdStatusPolicy.ts');
  writeFileSync(policyFixture, readFileSync(join(ets, 'vpnextensionability', 'NetBirdStatusPolicy.ets')));
  const policy = await import(pathToFileURL(policyFixture).href);

  // -------------------------------------------------------------------------
  // 1. the row key: identity + everything peerRow renders
  // -------------------------------------------------------------------------
  // Only the fields peerRowKey reads are needed; UiPeerEntry's rest is optional
  // client-detail material (the real objects come out of mergePeers).
  const peer = { pub_key: 'KEYA', name: 'peer-a.netbird.cloud', ip: '10.30.30.1',
    online: true, connection: 'relay', status: 'connected' };
  const same = { ...peer };
  check('key: stable for the same peer content (an unchanged row keeps its node)',
    m.peerRowKey(peer) === m.peerRowKey(same));
  check('key: a status change (the label the user reads) changes the key',
    m.peerRowKey(peer) !== m.peerRowKey({ ...peer, status: 'idle' }));
  check('key: a direct<->relay change (the 直连/中继 chip) changes the key',
    m.peerRowKey(peer) !== m.peerRowKey({ ...peer, connection: 'direct' }));
  check('key: the online flip (the chip appearing/disappearing) changes the key',
    m.peerRowKey(peer) !== m.peerRowKey({ ...peer, online: false }));
  check('key: a name or ip change changes the key',
    m.peerRowKey(peer) !== m.peerRowKey({ ...peer, name: 'renamed' }) &&
    m.peerRowKey(peer) !== m.peerRowKey({ ...peer, ip: '10.30.30.9' }));
  check('key: still unique per peer (it starts with the pub_key)',
    m.peerRowKey(peer).startsWith('KEYA|') &&
    m.peerRowKey(peer) !== m.peerRowKey({ ...peer, pub_key: 'KEYB' }));
  // 0.0.39 (I4): the 连接概览 pane's 离线对端 rows render `last_handshake_s` as
  // their last-seen text, so the key has to carry it — a row whose ONLY change is
  // the stamp would otherwise keep its key, keep its node and show the old time.
  check('key: the 离线对端 last-seen stamp is part of the key (I4\'s rendered field)',
    m.peerRowKey({ ...peer, last_handshake_s: 7200 }) !==
      m.peerRowKey({ ...peer, last_handshake_s: 42 }) &&
    m.peerRowKey({ ...peer, last_handshake_s: null }) !==
      m.peerRowKey({ ...peer, last_handshake_s: 0 }) &&
    m.peerRowKey({ ...peer, last_handshake_s: 42 }) ===
      m.peerRowKey({ ...peer, last_handshake_s: 42 }));

  // -------------------------------------------------------------------------
  // 2. the rendered list model — the documented ArkUI ForEach contract
  // -------------------------------------------------------------------------
  // previous: key -> the content the framework already painted for that key.
  // A key that already exists REUSES its child: the generator is not called, so
  // only `rebuilt` (the keys the generator really ran for) can show new content.
  function render(previous: Map<string, string>, keys: string[], labels: string[]):
    { rows: Map<string, string>, rebuilt: string[] } {
    const rows = new Map<string, string>();
    const rebuilt: string[] = [];
    for (let i = 0; i < keys.length; i++) {
      const key: string = keys[i];
      if (previous.has(key)) {
        rows.set(key, previous.get(key) as string);
      } else {
        rows.set(key, labels[i]);
        rebuilt.push(key);
      }
    }
    return { rows: rows, rebuilt: rebuilt };
  }
  const identityKey = (p: { pub_key: string }): string => p.pub_key;
  const label = (p: { status: string, connection: string }): string =>
    `${p.status}/${p.connection}`;
  const before = { ...peer };
  const after = { ...peer, status: 'idle', connection: 'direct' };
  const empty = new Map<string, string>();
  // v1 render, then the v2 render after an edge publication re-parsed the document
  const stale = render(render(empty, [identityKey(before)], [label(before)]).rows,
    [identityKey(after)], [label(after)]);
  const fixed = render(render(empty, [m.peerRowKey(before)], [label(before)]).rows,
    [m.peerRowKey(after)], [label(after)]);
  check('model: the retired identity key reproduces the defect — nothing is rebuilt, ' +
    'the old label stays on screen',
    stale.rebuilt.length === 0 && stale.rows.get('KEYA') === 'connected/relay' &&
    stale.rows.get('KEYA') !== label(after), `rendered=${stale.rows.get('KEYA')}`);
  check('model: peerRowKey rebuilds the changed row and the new content reaches it',
    fixed.rebuilt.length === 1 && fixed.rebuilt[0] === m.peerRowKey(after) &&
    fixed.rows.get(m.peerRowKey(after)) === label(after),
    `rendered=${fixed.rows.get(m.peerRowKey(after))}`);
  // an unaffected peer keeps its key -> its node is reused, not rebuilt
  const peerB = { pub_key: 'KEYB', name: 'peer-b.netbird.cloud', ip: '10.30.30.2',
    online: false, connection: 'unknown', status: 'offline' };
  const both = render(render(empty, [m.peerRowKey(before), m.peerRowKey(peerB)],
    [label(before), label(peerB)]).rows,
    [m.peerRowKey(after), m.peerRowKey(peerB)], [label(after), label(peerB)]);
  check('model: only the changed row is rebuilt — the untouched peer reuses its node',
    both.rebuilt.length === 1 && both.rebuilt[0] === m.peerRowKey(after) &&
    both.rows.get(m.peerRowKey(peerB)) === label(peerB));;

  // -------------------------------------------------------------------------
  // 3. the reader side: the 0.0.34 skip stays, and a real change re-applies
  // -------------------------------------------------------------------------
  check('reader: the SAME document is still skipped (no re-derivation, no re-render)',
    policy.shouldReapplyPublishedStatus('111|5', '111|5') === false &&
    policy.statusDocumentStamp({ updated_at_ms: 111, status_revision: 5 }) === '111|5');
  check('reader: a new stamp is applied at once (the list is re-derived from it)',
    policy.shouldReapplyPublishedStatus('111|5', policy.statusDocumentStamp(
      { updated_at_ms: 112, status_revision: 5 })) === true);

  // -------------------------------------------------------------------------
  // 4. the wiring, against the shipped page source
  // -------------------------------------------------------------------------
  const PAGE = readFileSync(join(ets, 'pages', 'Index.ets'), 'utf8');
  check('wiring: the peers ForEach key is the content-bearing peerRowKey',
    PAGE.includes('}, (peer: UiPeerEntry) => peerRowKey(peer))') &&
    PAGE.includes('peerRowKey, peerStatusText,'));
  check('wiring: the retired identity-only key is gone (negative control)',
    !PAGE.includes('(peer: UiPeerEntry) => peer.pub_key)'));
  check('wiring: the list still comes from the @State document, re-derived on render',
    PAGE.includes('@State published: PublishedStatus | null = null;') &&
    PAGE.includes('private peersList(): UiPeerEntry[] {') &&
    PAGE.includes('return this.published !== null && this.published.peers !== undefined &&') &&
    PAGE.includes('private visiblePeers(): UiPeerEntry[] {') &&
    PAGE.includes('return sortPeers(filterPeers(this.peersList(), this.peerQuery));'));
  check('wiring: the stamp skip still returns BEFORE the assignment (0.0.34 kept)',
    PAGE.indexOf('shouldReapplyPublishedStatus(this.appliedStatusStamp, nextStamp)') > 0 &&
    PAGE.indexOf('shouldReapplyPublishedStatus(this.appliedStatusStamp, nextStamp)') <
      PAGE.indexOf('this.published = live;'));
  check('wiring: leaving the detail page is what rebuilt the list (the reported workaround)',
    PAGE.includes('if (this.selectedPeer !== null) {') &&
    PAGE.includes('this.selectedPeer = null;') &&
    PAGE.indexOf('if (this.selectedPeer !== null) {') <
      PAGE.indexOf('ForEach(this.visiblePeers(), (peer: UiPeerEntry) => {'));
  check('wiring: the detail page still re-reads the peer by pub_key from that list',
    PAGE.includes('private selectedPeerLive(): UiPeerEntry | null {') &&
    PAGE.includes('if (list[i].pub_key === selected.pub_key) {') &&
    PAGE.includes('}, (field: DetailField) => `${field.label}=${field.value}`)'));
  check('wiring: no published-derived dot goes through a by-value @Builder any more',
    !PAGE.includes('this.statusDot(') &&
    PAGE.includes('.fill(this.stateColor())') &&
    PAGE.includes('.fill(this.selectedPeerColor())') &&
    PAGE.includes('.fill(this.peerStatusResource(peer.status))'));
  // 0.0.37 (I2) DELIBERATE UPDATE: the home card's three info rows became the
  // STACKED RailInfoRow @Component (the I1 device defect: the fixed 96 vp label
  // column squeezed "0/0 已连接" into 已连/接). The freshness contract this check
  // exists to protect is UNCHANGED — each row is still constructed from a METHOD
  // CALL on the observed document at the call site, and the '—' fallback still
  // happens inside the child on the live value (never a frozen by-value @Builder
  // argument, never a snapshot taken at tap time).
  check('wiring: the home card keeps its inline (fresh) document reads',
    PAGE.includes("label: 'NetBird IP',") &&
    PAGE.includes('value: this.homeAddressText()') &&
    PAGE.includes("label: '对端',") &&
    PAGE.includes('value: this.homePeersText()') &&
    PAGE.includes("Text(this.value ? this.value : '—')") &&
    PAGE.includes('struct RailInfoRow {'));
  check('wiring: the routes rows read this.routeDraft inside their own element',
    PAGE.includes('isOn: routeSelectionIsEnabled(this.routeDraft, entry.net_id) })'));
} catch (error) {
  failed++;
  console.log(`FAIL harness — ${(error as Error).message}`);
} finally {
  rmSync(dir, { recursive: true, force: true });
}
console.log(`${passed} passed, ${failed} failed`);
if (failed > 0) { process.exitCode = 1; }
