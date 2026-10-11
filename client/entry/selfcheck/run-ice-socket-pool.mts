// N5c demand-sized ICE socket pool self-check (host-side, no device, no
// network).
//
// The production module client/entry/src/main/ets/vpnextensionability/
// NetBirdIceSocketPool.ets is a zero-import pure module. node (v22.7+,
// type-stripping) cannot load the .ets extension directly, so this harness
// copies the REAL file byte-for-byte into an OS-temp fixture (.ts) at run
// time — the code under test is the shipped code, never a hand-held mirror —
// and then pins the call sites in the shipped extension source that make this
// math effective (target refresh from the live peer count AND the core's live
// `need_per_peer`; no fixed pool cap left).
//
// Run: node client/entry/selfcheck/run-ice-socket-pool.mts
// Exit 0 = all assertions pass; exit 1 = fixture unreadable or any failure.

import { readFileSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REAL = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability',
  'NetBirdIceSocketPool.ets');
const SHELL = join(HERE, '..', 'src', 'main', 'ets', 'vpnextensionability',
  'NetBirdVpnExtensionAbility.ets');

const fixtureDir: string = mkdtempSync(join(tmpdir(), 'nb-ice-socket-pool-'));
const fixture: string = join(fixtureDir, 'NetBirdIceSocketPool.ts');
writeFileSync(fixture, readFileSync(REAL)); // byte-identical copy of the real module

let passed: number = 0;
let failedCount: number = 0;

function check(name: string, condition: boolean, detail?: string): void {
  if (condition) {
    passed++;
    console.log(`ok   ${name}`);
  } else {
    failedCount++;
    console.log(`FAIL ${name}${detail !== undefined ? ` — ${detail}` : ''}`);
  }
}

try {
  const mod = await import(pathToFileURL(fixture).href);
  const shell: string = readFileSync(SHELL, 'utf8');

  // ---- constants ----------------------------------------------------------
  check('pool minimum is the pre-0050 size 8', mod.ICE_SOCKET_POOL_MIN === 8);
  check('pool maximum is the 0051 fd-budget cap 48', mod.ICE_SOCKET_POOL_MAX === 48);
  check('fallback per-peer demand is 4 (0050 Wi-Fi leg: 2 interfaces x 2)',
    mod.ICE_SOCKET_PER_PEER === 4);
  check('one refill pass may feed up to 8 (was 4)', mod.ICE_TOPUP_BATCH === 8);

  // ---- iceSocketNeedPerPeer (the core's live demand, 0051 §7 item 3) ------
  check('the core demand 6 passes through', mod.iceSocketNeedPerPeer(6) === 6);
  check('the core demand 2 passes through', mod.iceSocketNeedPerPeer(2) === 2);
  check('a fractional core demand is floored', mod.iceSocketNeedPerPeer(5.5) === 5);
  check('a missing field (undefined) falls back to 4',
    mod.iceSocketNeedPerPeer(undefined) === 4);
  check('a missing section (null) falls back to 4', mod.iceSocketNeedPerPeer(null) === 4);
  check('the core -1 (interfaces not enumerable) falls back to 4',
    mod.iceSocketNeedPerPeer(-1) === 4);
  check('0 falls back to 4', mod.iceSocketNeedPerPeer(0) === 4);
  check('NaN falls back to 4', mod.iceSocketNeedPerPeer(Number.NaN) === 4);
  check('Infinity falls back to 4', mod.iceSocketNeedPerPeer(Number.POSITIVE_INFINITY) === 4);

  // ---- iceSocketTargetDepth (the smoke-0050 case first) -------------------
  check('6 peers with no core field target 24 sockets (0050 fallback formula)',
    mod.iceSocketTargetDepth(6) === 24, `got ${mod.iceSocketTargetDepth(6)}`);
  check('6 peers x core need 6 target 36 sockets (0051: 3 interfaces)',
    mod.iceSocketTargetDepth(6, 6) === 36, `got ${mod.iceSocketTargetDepth(6, 6)}`);
  check('6 peers x core need 2 target 12 sockets (single-interface window)',
    mod.iceSocketTargetDepth(6, 2) === 12);
  check('8 peers x core need 6 hit the 48 cap',
    mod.iceSocketTargetDepth(8, 6) === 48, `got ${mod.iceSocketTargetDepth(8, 6)}`);
  check('100 peers x core need 6 stay at the 48 cap',
    mod.iceSocketTargetDepth(100, 6) === 48);
  check('1 peer clamps up to the 8 minimum', mod.iceSocketTargetDepth(1) === 8);
  check('2 peers clamp up to the 8 minimum', mod.iceSocketTargetDepth(2) === 8);
  check('3 peers target 12', mod.iceSocketTargetDepth(3) === 12);
  check('5 peers target 20', mod.iceSocketTargetDepth(5) === 20);
  check('7 peers target 28', mod.iceSocketTargetDepth(7) === 28);
  check('8 peers target 32 (no longer the cap)', mod.iceSocketTargetDepth(8) === 32);
  check('12 peers reach the 48 cap', mod.iceSocketTargetDepth(12) === 48);
  check('100 peers stay at the 48 cap', mod.iceSocketTargetDepth(100) === 48);
  check('0 peers (unknown map) keep the 8 minimum', mod.iceSocketTargetDepth(0) === 8);
  check('negative peers keep the 8 minimum', mod.iceSocketTargetDepth(-6) === 8);
  check('NaN peers keep the 8 minimum', mod.iceSocketTargetDepth(Number.NaN) === 8);
  check('a missing core need keeps the pre-0051 formula',
    mod.iceSocketTargetDepth(6, undefined) === 24);

  // ---- iceSocketTopUpCount ------------------------------------------------
  check('a drained 6-peer pool wants a full 8-socket pass',
    mod.iceSocketTopUpCount(0, 24, false) === 8);
  check('the pass is capped at 8 even for a 24-socket gap',
    mod.iceSocketTopUpCount(0, 32, false) === 8);
  check('a queue of 20 against target 24 wants 4',
    mod.iceSocketTopUpCount(20, 24, false) === 4);
  check('a queue at target wants 0', mod.iceSocketTopUpCount(24, 24, false) === 0);
  check('a queue above target wants 0', mod.iceSocketTopUpCount(30, 24, false) === 0);
  check('-1 (no shell-fed source attached) wants a full pass',
    mod.iceSocketTopUpCount(-1, 8, false) === 8);
  check('network-dirty pauses a drained pool',
    mod.iceSocketTopUpCount(0, 24, true) === 0);
  check('network-dirty pauses a partially drained pool',
    mod.iceSocketTopUpCount(20, 24, true) === 0);
  check('network-dirty pauses even the -1 reading',
    mod.iceSocketTopUpCount(-1, 32, true) === 0);

  // ---- convergence: the 6-peer case must fill in one top-up cycle --------
  {
    const target: number = mod.iceSocketTargetDepth(6);
    let queued: number = 0;
    let passes: number = 0;
    while (mod.iceSocketTopUpCount(queued, target, false) > 0 && passes < 12) {
      queued += mod.iceSocketTopUpCount(queued, target, false);
      passes++;
    }
    check('6-peer pool converges to 24 in 3 passes of 8',
      queued === 24 && passes === 3, `queued=${queued} passes=${passes}`);
    check('converged pool needs no further supply',
      mod.iceSocketTopUpCount(queued, target, false) === 0);
  }
  {
    // The core's 0051 demand (need=6, 6 peers → 36) must also converge.
    const target: number = mod.iceSocketTargetDepth(6, 6);
    check('the 0051 target is 36', target === 36);
    let queued: number = 0;
    let passes: number = 0;
    while (mod.iceSocketTopUpCount(queued, target, false) > 0 && passes < 12) {
      queued += mod.iceSocketTopUpCount(queued, target, false);
      passes++;
    }
    check('the 36-socket target converges in 5 passes of 8',
      queued === 36 && passes === 5, `queued=${queued} passes=${passes}`);
    check('converged 36-socket pool needs no further supply',
      mod.iceSocketTopUpCount(queued, target, false) === 0);
  }
  {
    // The core consumes mid-pass: after each fed socket the shell re-reads the
    // depth. A queue that drained between samples must still be filled within
    // ONE pass (8 sockets = 2 peers of need 4 in 0050).
    const target: number = mod.iceSocketTargetDepth(6);
    let queued: number = 4;
    const fedThisPass: number = mod.iceSocketTopUpCount(queued, target, false);
    check('a stale-high depth still opens a full 8-socket pass',
      fedThisPass === 8, `got ${fedThisPass}`);
  }

  // ---- shipped-extension wiring ------------------------------------------
  // STAGE 2/3 (official-alignment audit §2.7): the demand-sized pool is RETIRED —
  // the whole ICE plane rides ONE shared socket, and since stage 3 that socket IS
  // the WG outer socket, armed once by `armIceMux` over the device's own protected
  // fd (and re-armed on a physical-network change). The pool math above is still
  // pinned because the module and its (now uncalled) feeders stay in the extension
  // until stage 4; what must NOT happen is the watcher driving it again.
  check('stage 2: the watcher no longer tops the retired pool up',
    !shell.includes('this.topUpIceBatch().catch'));
  check('stage 3: the core arms the ONE shared mux OVER the WG device socket (no second socket)',
    shell.includes('const armed: IceMuxArmResult = connectorIceMuxArm();') &&
    !shell.includes('iceMuxOpen()') &&
    !shell.includes('connectorIceMuxFeed'));
  check('stage 3: both provision paths arm the mux (initial connect + physical-network change)',
    shell.includes('const iceOk: boolean = await this.armIceMux();') &&
    shell.includes('if (!await this.armIceMux()) { break; }'));
  check('the pool feed still sizes itself from a live status read (count + core need)',
    shell.includes('iceSocketTargetDepth(live.peer_count, iceSocketNeedPerPeer(live.ice_sockets?.need_per_peer))'));
  check('the shell imports the core-need reader from the pool module',
    shell.includes('iceSocketNeedPerPeer'));
  check('no fixed pool-size constant is left in the extension',
    !shell.includes('ICE_SOCKET_POOL_SIZE'));
  check('no fixed top-up batch constant is left in the extension',
    !shell.includes('ICE_TOPUP_BATCH: number'));
  check('the top-up interval is unchanged (4 s, no new timer)',
    shell.includes('const ICE_SOCKET_TOPUP_MS: number = 4000;'));
  check('the batch loop re-reads the latest queued depth after each feed',
    shell.includes('if (iceSocketTopUpCount(this.iceQueueDepth, this.iceTargetDepth, paused) === 0) { break; }'));
  check('the timer still skips while the network is dirty',
    shell.includes('this.iceQueueDepth >= this.iceTargetDepth || this.iceTopUpActive || this.networkDirty'));
  check('the top-up funnel still folds network-dirty into its paused decision',
    shell.includes('const paused: boolean = this.networkDirty || this.n2CdProductionBlocked() ||'));
  check('the ICE protect gate is still fail-closed before every feed',
    shell.includes("if (!await this.protectSocketWithBox(connection, fd, 'I') || epoch !== this.networkEpoch ||"));
} finally {
  rmSync(fixtureDir, { recursive: true, force: true });
}

console.log(`${passed} passed, ${failedCount} failed`);
if (failedCount > 0) { process.exitCode = 1; }
