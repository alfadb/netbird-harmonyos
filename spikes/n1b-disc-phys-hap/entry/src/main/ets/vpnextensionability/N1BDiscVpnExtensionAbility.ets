import VpnExtensionAbility from '@ohos.app.ability.VpnExtensionAbility';
import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';
import vpnExtension from '@ohos.net.vpnExtension';
import probe from 'libn1bdisc_probe.so';

const DOMAIN: number = 0x2900;
const TAG: string = 'N1BDiscVpn';

// Frozen timeboxes (gate-plan :1031-1049): P2 per-entry create() 60 s, P2 late
// observation window 60 s (once per matrix), P9 destroy() resolve wait 10 s.
// All other waits live inside the native probe (bounded polls / D7 / D-W).
const CREATE_BOX_MS: number = 60000;
const LATE_WINDOW_MS: number = 60000;
const DESTROY_BOX_MS: number = 10000;

const OUTCOME_RESOLVED: string = 'resolved';
const OUTCOME_REJECTED: string = 'rejected';
const OUTCOME_TIMEOUT: string = 'timeout';
const OUTCOME_LATE_RESOLVED: string = 'late-resolved';
const OUTCOME_LATE_REJECTED: string = 'late-rejected';
const OUTCOME_INDETERMINATE: string = 'indeterminate';
const OUTCOME_NOT_ATTEMPTED: string = 'not_attempted';

// Matrix arbitration states: at most one in-flight create() (:468); the late
// observation window opens at most once per matrix (:468-475).
const STATE_RUNNING: string = 'running';
const STATE_LATE_WINDOW: string = 'lateWindow';
const STATE_TERMINATED: string = 'terminated';

// JSON shapes returned by the probe NAPI face (every export is synchronous and
// returns a JSON string; fd numbers cross as int per probe/src/lib.rs header).
interface ProbeLockResult {
  fd_dup: number;
}

interface ProbeBarrierResult {
  confirmed: boolean;
}

interface ProbeTerminalResult {
  terminal: boolean;
}

interface BusinessFailure extends Error {
  code?: number;
}

interface MatrixEntrySpec {
  id: string;
  rejItem: number;
  config: vpnExtension.VpnConfig;
}

// Per-entry settle record handed back to the matrix loop by the create()
// then/catch callbacks registered synchronously at the call site (:468).
interface EntryResult {
  kind: string;
  fd: number;
  conn: vpnExtension.VpnConnection | null;
}

interface RetainedEntry {
  fd: number;
  conn: vpnExtension.VpnConnection;
  isMb1: boolean;
}

class MatrixEntry {
  id: string = '';
  closed: boolean = false;
  boxTimer: number = -1;
  windowTimer: number = -1;
}

// Matrix row literals (:453-457, MJ-10 frozen; VpnConfig shapes per
// @ohos.net.vpnExtension.d.ts VpnConfig / connection RouteInfo / LinkAddress).
// MR1B runs only when MR1 closed `rejected` and the matrix is not terminated
// (:457 MR table row + :468-475 matrix-termination rule). The matrix is serial:
// at most one in-flight create() at any time (:468).
function buildMatrixSpecs(): MatrixEntrySpec[] {
  const specs: MatrixEntrySpec[] = [];
  // MR1 — main candidate (N1b r2 target shape), isBlocking omitted.
  specs.push({
    id: 'MR1',
    rejItem: 0,
    config: {
      addresses: [{ address: { address: '10.99.0.1', family: 1 }, prefixLength: 32 }],
      routes: [{
        interface: 'wlan0',
        destination: { address: { address: '10.99.0.0', family: 1 }, prefixLength: 24 },
        gateway: { address: '10.99.0.1', family: 1 },
        hasGateway: true,
        isDefaultRoute: false
      }],
      mtu: 1400
    }
  });
  // MR1B — explicit isBlocking variant (= MR1 + isBlocking: true).
  specs.push({
    id: 'MR1B',
    rejItem: 1,
    config: {
      addresses: [{ address: { address: '10.99.0.1', family: 1 }, prefixLength: 32 }],
      routes: [{
        interface: 'wlan0',
        destination: { address: { address: '10.99.0.0', family: 1 }, prefixLength: 24 },
        gateway: { address: '10.99.0.1', family: 1 },
        hasGateway: true,
        isDefaultRoute: false
      }],
      mtu: 1400,
      isBlocking: true
    }
  });
  // MR2 — interface candidate eth0 (= MR1 but interface: "eth0").
  specs.push({
    id: 'MR2',
    rejItem: 2,
    config: {
      addresses: [{ address: { address: '10.99.0.1', family: 1 }, prefixLength: 32 }],
      routes: [{
        interface: 'eth0',
        destination: { address: { address: '10.99.0.0', family: 1 }, prefixLength: 24 },
        gateway: { address: '10.99.0.1', family: 1 },
        hasGateway: true,
        isDefaultRoute: false
      }],
      mtu: 1400
    }
  });
  // MR3 — empty interface control (= MR1 but interface: "").
  specs.push({
    id: 'MR3',
    rejItem: 3,
    config: {
      addresses: [{ address: { address: '10.99.0.1', family: 1 }, prefixLength: 32 }],
      routes: [{
        interface: '',
        destination: { address: { address: '10.99.0.0', family: 1 }, prefixLength: 24 },
        gateway: { address: '10.99.0.1', family: 1 },
        hasGateway: true,
        isDefaultRoute: false
      }],
      mtu: 1400
    }
  });
  // MB1 — E3-proven shape verbatim (addresses only, no routes/mtu/isBlocking).
  specs.push({
    id: 'MB1',
    rejItem: 4,
    config: {
      addresses: [{ address: { address: '192.0.2.1', family: 1 }, prefixLength: 32 }]
    }
  });
  return specs;
}

function sanitizeText(value: string): string {
  return value.replace(/\|/g, '/').replace(/\r/g, ' ').replace(/\n/g, ' ').slice(0, 160);
}

function describeError(err: Error): string {
  const biz = err as BusinessFailure;
  const code = biz.code !== undefined ? `${biz.code}` : 'none';
  const name = sanitizeText(err.name || 'Error');
  const message = sanitizeText(err.message || 'no-message');
  return `code=${code},name=${name},message=${message}`;
}

// P0-P12 frozen full order (:1012-1027). Every frozen marker is emitted by
// the native probe; this file only orchestrates calls and carries no marker
// literals (:1095-1097). All waits are bounded: the create boxes and the
// destroy resolve box below, everything else inside native (:1031-1049).
export default class N1BDiscVpnExtensionAbility extends VpnExtensionAbility {
  private matrixState: string = STATE_RUNNING;
  private settleSink: ((r: EntryResult) => void) | null = null;
  private currentConn: vpnExtension.VpnConnection | null = null;

  onCreate(want: Want): void {
    hilog.info(DOMAIN, TAG, 'VPN_ONCREATE|bundle=%{public}s', want.bundleName ?? 'unknown');
    this.runProbeFlow();
  }

  onConnect(): void {
    hilog.info(DOMAIN, TAG, 'VPN_ONCONNECT');
  }

  onDisconnect(): void {
    hilog.info(DOMAIN, TAG, 'VPN_ONDISCONNECT');
  }

  onDestroy(): void {
    hilog.info(DOMAIN, TAG, 'VPN_ONDESTROY');
  }

  // ------------------------------------------------------------------
  // P0/P1: entry + D1 dlopen/dlsym probe (10 s box inside native), then
  // P2 matrix.
  // ------------------------------------------------------------------
  private async runProbeFlow(): Promise<void> {
    try {
      hilog.info(DOMAIN, TAG, 'PROBE_FLOW_BEGIN');
      hilog.info(DOMAIN, TAG, 'PROBE_VERSION|%{public}s', probe.version());
      probe.d1_probe('libn1bdisc_probe.so');
      const retained: RetainedEntry | null = await this.runD2Matrix();
      if (retained === null) {
        await this.runNoLiveFdBranch();
      } else {
        await this.runRetainedLine(retained);
      }
      hilog.info(DOMAIN, TAG, 'PROBE_FLOW_END');
    } catch (err) {
      const e = err as Error;
      hilog.error(DOMAIN, TAG, 'PROBE_FLOW_ERROR|name=%{public}s|message=%{public}s',
        e.name || 'Error', e.message || 'no-message');
    }
  }

  // ------------------------------------------------------------------
  // P2: D2 candidate-config matrix (:414-509). Serial entries; first
  // accepted entry is retained and terminates the matrix
  // (first-accept-lock); timeout opens the single 60 s late observation
  // window; no destroy() ever runs inside the matrix (:473-479).
  // ------------------------------------------------------------------
  private async runD2Matrix(): Promise<RetainedEntry | null> {
    const specs: MatrixEntrySpec[] = buildMatrixSpecs();
    let mr1Rejected = false;
    for (let i = 0; i < specs.length; i++) {
      const spec = specs[i];
      if (this.matrixState !== STATE_RUNNING) {
        // first-accept-lock / matrix-terminated: not attempted, not driven (:475-479).
        probe.d2_entry_outcome(spec.id, OUTCOME_NOT_ATTEMPTED, -1);
        continue;
      }
      if (spec.id === 'MR1B' && !mr1Rejected) {
        probe.d2_entry_outcome(spec.id, OUTCOME_NOT_ATTEMPTED, -1);
        continue;
      }
      const result = await this.runOneEntry(spec);
      if (result.kind === OUTCOME_RESOLVED || result.kind === OUTCOME_LATE_RESOLVED) {
        this.matrixState = STATE_TERMINATED;
        if (result.conn !== null) {
          const retained: RetainedEntry = {
            fd: result.fd,
            conn: result.conn,
            isMb1: spec.id === 'MB1'
          };
          // Remaining entries recorded not_attempted (first-accept-lock).
          for (let k = i + 1; k < specs.length; k++) {
            probe.d2_entry_outcome(specs[k].id, OUTCOME_NOT_ATTEMPTED, -1);
          }
          return retained;
        }
      }
      if (spec.id === 'MR1' && result.kind === OUTCOME_REJECTED) {
        mr1Rejected = true;
      }
      if (result.kind === OUTCOME_LATE_REJECTED || result.kind === OUTCOME_INDETERMINATE) {
        // All three window outcomes terminate the matrix (:468-475).
        this.matrixState = STATE_TERMINATED;
      }
    }
    return null;
  }

  private async runOneEntry(spec: MatrixEntrySpec): Promise<EntryResult> {
    probe.d2_entry_attempted(spec.id);
    const entry = new MatrixEntry();
    entry.id = spec.id;
    const conn = vpnExtension.createVpnConnection(this.context);
    this.currentConn = conn;
    const settlePromise = new Promise<EntryResult>((resolve) => {
      this.settleSink = resolve;
    });
    const createPromise = conn.create(spec.config);
    createPromise.then((fd: number): void => {
      this.onEntryResolved(entry, fd);
    }).catch((err: Error): void => {
      this.onEntryRejected(entry, err);
    });
    entry.boxTimer = setTimeout((): void => {
      this.onEntryBoxTimeout(entry);
    }, CREATE_BOX_MS);
    const result = await settlePromise;
    return result;
  }

  // Settled-entry or post-termination callbacks may only emit late markers
  // (:468-471); a late-resolved fd is registered, never closed (:471-475).
  private onEntryResolved(entry: MatrixEntry, fd: number): void {
    if (entry.closed || this.matrixState === STATE_TERMINATED) {
      probe.d2_late(entry.id, 'resolve');
      probe.d2_late_fd(fd);
      return;
    }
    entry.closed = true;
    clearTimeout(entry.boxTimer);
    clearTimeout(entry.windowTimer);
    const inWindow = this.matrixState === STATE_LATE_WINDOW;
    probe.d2_entry_outcome(entry.id, inWindow ? OUTCOME_LATE_RESOLVED : OUTCOME_RESOLVED, fd);
    const sink = this.settleSink;
    this.settleSink = null;
    if (sink !== null) {
      sink({ kind: inWindow ? OUTCOME_LATE_RESOLVED : OUTCOME_RESOLVED, fd: fd, conn: this.currentConn });
    }
  }

  private onEntryRejected(entry: MatrixEntry, err: Error): void {
    if (entry.closed || this.matrixState === STATE_TERMINATED) {
      probe.d2_late(entry.id, 'reject');
      return;
    }
    entry.closed = true;
    clearTimeout(entry.boxTimer);
    clearTimeout(entry.windowTimer);
    // Rejection text travels via the CHUNK stream=rejtext, item = matrix id
    // domain MR1->0 .. MB1->4 (:421, :507-509).
    probe.rejtext_emit(this.rejItemOf(entry.id), describeError(err));
    const inWindow = this.matrixState === STATE_LATE_WINDOW;
    probe.d2_entry_outcome(entry.id, inWindow ? OUTCOME_LATE_REJECTED : OUTCOME_REJECTED, -1);
    const sink = this.settleSink;
    this.settleSink = null;
    if (sink !== null) {
      sink({ kind: inWindow ? OUTCOME_LATE_REJECTED : OUTCOME_REJECTED, fd: -1, conn: null });
    }
  }

  private rejItemOf(id: string): number {
    if (id === 'MR1') {
      return 0;
    }
    if (id === 'MR1B') {
      return 1;
    }
    if (id === 'MR2') {
      return 2;
    }
    if (id === 'MR3') {
      return 3;
    }
    return 4;
  }

  // 60 s create box expiry -> record outcome=timeout, pause the matrix and
  // open the single 60 s late observation window (:468-475).
  private onEntryBoxTimeout(entry: MatrixEntry): void {
    if (entry.closed || this.matrixState !== STATE_RUNNING) {
      return;
    }
    probe.d2_entry_outcome(entry.id, OUTCOME_TIMEOUT, -1);
    this.matrixState = STATE_LATE_WINDOW;
    entry.windowTimer = setTimeout((): void => {
      this.onEntryWindowExpiry(entry);
    }, LATE_WINDOW_MS);
  }

  // Window expiry without settlement -> indeterminate; matrix terminates.
  private onEntryWindowExpiry(entry: MatrixEntry): void {
    if (entry.closed || this.matrixState !== STATE_LATE_WINDOW) {
      return;
    }
    entry.closed = true;
    this.matrixState = STATE_TERMINATED;
    probe.d2_entry_outcome(entry.id, OUTCOME_INDETERMINATE, -1);
    const sink = this.settleSink;
    this.settleSink = null;
    if (sink !== null) {
      sink({ kind: OUTCOME_INDETERMINATE, fd: -1, conn: null });
    }
  }

  // ------------------------------------------------------------------
  // Retained-entry main line, P2 lock through P12 (:1012-1027). The P2
  // retention lock sequence S1-S7 (:481-508) runs entirely inside native
  // d2_lock; ArkTS only calls it and reads fd_dup.
  // ------------------------------------------------------------------
  private async runRetainedLine(retained: RetainedEntry): Promise<void> {
    const lock = JSON.parse(probe.d2_lock(retained.fd)) as ProbeLockResult;
    if (lock.fd_dup < 0) {
      await this.runDupFailedBranch(retained);
      return;
    }
    const fdDup = lock.fd_dup;
    const mb1 = retained.isMb1;
    // P3 D4 socket->tun delivery + first frame dump.
    probe.d4_probe(fdDup, mb1);
    // P4 D5 tun write -> sink.
    probe.d5_probe(fdDup);
    // P5 D8a 10-level MTU write-return ladder.
    probe.d8a_probe(fdDup, mb1);
    // P5T PRE (P1-P5 facts + fd-ledger P5T snapshot digest; no skips on this path).
    probe.pre_emit('');
    // P6 D7 20 s frozen load (blocks; that is the probe).
    probe.d7_probe();
    // P7 D8b storm (10 s / 4 MiB / 50k writes, fused inside native).
    probe.d8b_probe(fdDup, mb1);
    // P8 D-W: the one worker spawn -> bounded barrier wait -> in-wait evidence.
    probe.dw_start(fdDup);
    let barrier = JSON.parse(probe.dw_wait_barrier()) as ProbeBarrierResult;
    if (!barrier.confirmed) {
      // destroy deferral: never destroy before the barrier marker (:714-723).
      // Explicit deferral entry — budget <= 8 s counted from the barrier-box
      // expiry (:715); NOT a second 7 s box. P8 budget note (:1059): on this
      // branch 7 s + <=8 s replaces in-wait (2 s) + P9 destroy (10 s), so the
      // 525 s observation-window bound still holds.
      barrier = JSON.parse(probe.dw_wait_barrier_defer()) as ProbeBarrierResult;
    }
    let notCreatedCause = '';
    let destroyResolved = false;
    if (barrier.confirmed) {
      probe.dw_inwait_collect();
      // P9 destroy shared subprotocol (sole destroy() of the whole protocol).
      destroyResolved = await this.destroyOnce(retained.conn, retained.fd);
    } else {
      // Barrier never observed -> worker anomaly, destroy not executed
      // (:714-723, pre-registered skip site).
      probe.skip_emit('destroy', 'barrier-never-observed');
      notCreatedCause = 'barrier-never-observed';
    }
    // P10 bounded terminal poll -> join + D6b, or join-timeout abandonment
    // (:954-985, r18: D6b skipped entirely on join-timeout).
    const term = JSON.parse(probe.dw_wait_terminal()) as ProbeTerminalResult;
    if (term.terminal) {
      probe.dw_join();
      probe.d6b(fdDup);
    } else {
      probe.skip_emit('D6b', 'join-timeout-abandoned');
      // Serial append, not overwrite (m-06): barrier-never-observed followed
      // by join-timeout keeps BOTH registered causes.
      notCreatedCause = notCreatedCause === ''
        ? 'join-timeout-abandoned'
        : `${notCreatedCause},join-timeout-abandoned`;
    }
    // P11 probe-owned socket cleanup (three roles; fd_dup untouched, :1045).
    probe.cleanup_sockets(notCreatedCause);
    // P12 POST (late-race window and class derivation inside native).
    probe.post_emit(destroyResolved);
  }

  // Shared destroy subprotocol (:709-713): _T immediately before the
  // invocation -> invoke (obtain the promise, do not await) -> _C immediately
  // after the invocation, before resolve waiting -> bounded 10 s resolve wait.
  // D6a runs inside the resolve callback on the same stack (:954-958).
  private async destroyOnce(conn: vpnExtension.VpnConnection, fdOrig: number): Promise<boolean> {
    probe.dw_destroy_t();
    const destroyPromise: Promise<void> = conn.destroy();
    probe.dw_destroy_c();
    const settled = new Promise<boolean>((resolveSettle) => {
      destroyPromise.then((): void => {
        probe.d6a(fdOrig);
        resolveSettle(true);
      }).catch((err: Error): void => {
        hilog.warn(DOMAIN, TAG, 'DESTROY_REJECTED|name=%{public}s|message=%{public}s',
          err.name || 'Error', err.message || 'no-message');
        resolveSettle(false);
      });
    });
    const box = new Promise<boolean>((resolveBox) => {
      setTimeout((): void => {
        resolveBox(false);
      }, DESTROY_BOX_MS);
    });
    const resolved = await Promise.race([settled, box]);
    return resolved;
  }

  // ------------------------------------------------------------------
  // dup-failed branch (:987-1006 column 2): VPN still live, so D7 executes
  // and destroy runs once via the shared subprotocol with D6a; everything
  // fd_dup-dependent skips.
  // ------------------------------------------------------------------
  private async runDupFailedBranch(retained: RetainedEntry): Promise<void> {
    probe.skip_emit('D4', 'dup-failed');
    probe.skip_emit('D5', 'dup-failed');
    probe.skip_emit('D8a', 'dup-failed');
    probe.pre_emit('D4:dup-failed,D5:dup-failed,D8a:dup-failed');
    probe.d7_probe();
    probe.skip_emit('D8b', 'dup-failed');
    const destroyResolved = await this.destroyOnce(retained.conn, retained.fd);
    // P10 skip: no worker exists; straight to P11, no join wait (:1001-1002).
    probe.skip_emit('D-W', 'dup-failed');
    probe.skip_emit('D6b', 'dup-failed');
    probe.cleanup_sockets('dup-failed');
    probe.post_emit(destroyResolved);
  }

  // ------------------------------------------------------------------
  // no-live-fd branch (:987-1006 column 1): every create rejected/timeout-
  // terminated/indeterminate. No destroy() call (nothing live to destroy),
  // D7 unobservable, D-W skipped, main line still runs P5T -> P11 -> P12.
  // ------------------------------------------------------------------
  private async runNoLiveFdBranch(): Promise<void> {
    probe.skip_emit('D4', 'no-live-fd');
    probe.skip_emit('D5', 'no-live-fd');
    probe.skip_emit('D8a', 'no-live-fd');
    probe.pre_emit('D4:no-live-fd,D5:no-live-fd,D8a:no-live-fd');
    probe.skip_emit('D7', 'no-live-fd');
    probe.skip_emit('D8b', 'no-live-fd');
    probe.skip_emit('destroy', 'no-live-connection');
    probe.skip_emit('D6a', 'no-live-fd');
    probe.skip_emit('D-W', 'no-live-fd');
    probe.skip_emit('D6b', 'no-live-fd');
    probe.cleanup_sockets('no-live-fd');
    probe.post_emit(false);
  }
}
