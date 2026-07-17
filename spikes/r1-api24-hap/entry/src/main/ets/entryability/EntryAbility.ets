import AbilityConstant from '@ohos.app.ability.AbilityConstant';
import UIAbility from '@ohos.app.ability.UIAbility';
import Want from '@ohos.app.ability.Want';
import hilog from '@ohos.hilog';
import window from '@ohos.window';
import e2network from 'libe2network.so';
import { E2NetworkRoundResult } from 'libe2network.so';
import probe from 'libprobe.so';
import { AsyncRoundResult, BufferHashResult, FdPair, FdTransferResult, ResourceSnapshot } from 'libprobe.so';

const DOMAIN: number = 0x2900;
const TAG: string = 'R1Api24Probe';
const FULL_ROUNDS: number = 10;
const SAMPLES_PER_ROUND: number = 100;
const EBADF: number = 9;
const ECONNREFUSED: number = 111;

function delay(milliseconds: number): Promise<void> {
  return new Promise<void>((resolve: () => void) => {
    setTimeout(resolve, milliseconds);
  });
}

function fnv1a(bytes: Uint8Array): number {
  let hash: number = 2166136261;
  for (let index: number = 0; index < bytes.length; index++) {
    hash = Math.imul((hash ^ bytes[index]) >>> 0, 16777619) >>> 0;
  }
  return hash >>> 0;
}

function fnv1aString(value: string): number {
  const bytes: Uint8Array = new Uint8Array(value.length);
  for (let index: number = 0; index < value.length; index++) {
    bytes[index] = value.charCodeAt(index) & 0xff;
  }
  return fnv1a(bytes);
}

function callbackPayload(round: number, sequence: number): string {
  const sentinel: number = (round * 17 + sequence * 31) & 0xff;
  return `r${round.toString().padStart(2, '0')}-s${sequence.toString().padStart(3, '0')}-x${sentinel.toString(16).padStart(2, '0')}`;
}

function fdPayload(round: number): string {
  const sentinel: number = (round * 37) & 0xff;
  return `fd-round-${round.toString().padStart(2, '0')}-sentinel-${sentinel.toString(16).padStart(2, '0')}`;
}

function cleanDetail(error: Error): string {
  return error.message.replace(/\|/g, '/').replace(/\r/g, ' ').replace(/\n/g, ' ');
}

// This ordinary EntryAbility is a bounded E1 regression plus E2 C-network evidence probe.
export default class EntryAbility extends UIAbility {
  private onCreateObserved: boolean = false;
  private mainTid: number = -1;
  private activeRound: number = 0;
  private callbackCount: number = 0;
  private expectedSequence: number = 1;
  private callbackError: string = '';
  private callbackProducerTid: number = -1;
  private callbackConsumerTid: number = -1;
  private callbackHashAggregate: number = 2166136261;

  onCreate(_want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    this.onCreateObserved = true;
    AppStorage.setOrCreate<string>('e1Status', 'E1 C-only RUNNING');
    AppStorage.setOrCreate<string>('e1Detail', 'Initializing ordinary EntryAbility probe');
    AppStorage.setOrCreate<string>('e2Status', 'E2 C network RUNNING');
    AppStorage.setOrCreate<string>('e2Detail', 'E1 regression running before E2');
    hilog.info(DOMAIN, TAG, 'EntryAbility onCreate');
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    hilog.info(DOMAIN, TAG, 'EntryAbility windowStage onWindowStageCreate');
    windowStage.loadContent('pages/Index', (error) => {
      if (error.code) {
        const detail: string = JSON.stringify(error);
        AppStorage.set<string>('e1Status', 'E1 C-only FAIL');
        AppStorage.set<string>('e1Detail', detail);
        AppStorage.set<string>('e2Status', 'E2 C network FAIL');
        AppStorage.set<string>('e2Detail', detail);
        hilog.error(DOMAIN, TAG, 'ArkUI load failed: %{public}s', detail);
        return;
      }

      hilog.info(DOMAIN, TAG, 'EntryAbility onCreate observed=%{public}s',
        this.onCreateObserved ? 'true' : 'false');
      this.runIntegratedProbe().catch((error: Error) => {
        const detail: string = cleanDetail(error);
        AppStorage.set<string>('e2Status', 'E2 C network FAIL');
        AppStorage.set<string>('e2Detail', detail);
        hilog.error(DOMAIN, TAG, 'E2_C_PROBE_RESULT|verdict=FAIL|detail=%{public}s', detail);
      });
    });
  }

  onForeground(): void {
    hilog.info(DOMAIN, TAG, 'EntryAbility foreground onForeground');
  }

  private onAsyncCallback(round: number, sequence: number, payload: string, payloadHash: number,
    producerTid: number, consumerTid: number): void {
    const expectedPayload: string = callbackPayload(round, sequence);
    const expectedHash: number = fnv1aString(expectedPayload);
    let error: string = '';

    if (round !== this.activeRound) {
      error = `round mismatch expected=${this.activeRound} actual=${round}`;
    } else if (sequence !== this.expectedSequence) {
      error = `sequence mismatch expected=${this.expectedSequence} actual=${sequence}`;
    } else if (payload !== expectedPayload) {
      error = `payload mismatch sequence=${sequence}`;
    } else if (payloadHash !== expectedHash) {
      error = `payload hash mismatch sequence=${sequence} expected=${expectedHash} actual=${payloadHash}`;
    } else if (consumerTid !== this.mainTid) {
      error = `callback consumed off main ArkTS context mainTid=${this.mainTid} consumerTid=${consumerTid}`;
    } else if (producerTid === consumerTid) {
      error = `callback producer and consumer thread are identical tid=${producerTid}`;
    }

    if (this.callbackProducerTid === -1) {
      this.callbackProducerTid = producerTid;
    } else if (this.callbackProducerTid !== producerTid && error.length === 0) {
      error = `producer tid changed expected=${this.callbackProducerTid} actual=${producerTid}`;
    }
    if (this.callbackConsumerTid === -1) {
      this.callbackConsumerTid = consumerTid;
    } else if (this.callbackConsumerTid !== consumerTid && error.length === 0) {
      error = `consumer tid changed expected=${this.callbackConsumerTid} actual=${consumerTid}`;
    }
    if (error.length > 0 && this.callbackError.length === 0) {
      this.callbackError = error;
    }

    this.callbackCount++;
    this.expectedSequence++;
    this.callbackHashAggregate = Math.imul(
      (this.callbackHashAggregate ^ payloadHash ^ sequence) >>> 0, 16777619) >>> 0;
    if (round === 0) {
      hilog.info(DOMAIN, TAG,
        'E1_CALLBACK_WARMUP|round=0|sequence=%{public}d|producerTid=%{public}d|' +
        'consumerTid=%{public}d|payload=%{public}s|hash=%{public}s|valid=%{public}s',
        sequence, producerTid, consumerTid, payload, payloadHash.toString(), error.length === 0 ? 'true' : 'false');
    } else {
      hilog.info(DOMAIN, TAG,
        'E1_CALLBACK_SAMPLE|round=%{public}d|sequence=%{public}d|producerTid=%{public}d|' +
        'consumerTid=%{public}d|payload=%{public}s|hash=%{public}s|valid=%{public}s',
        round, sequence, producerTid, consumerTid, payload, payloadHash.toString(), error.length === 0 ? 'true' : 'false');
    }
  }

  private runBufferCases(round: number): number {
    let aggregate: number = 2166136261;
    for (let sample: number = 0; sample < SAMPLES_PER_ROUND; sample++) {
      const length: number = sample;
      const leftSentinel: number = (round * 41 + sample) & 0xff;
      const rightSentinel: number = leftSentinel ^ 0xff;
      const guarded: Uint8Array = new Uint8Array(length + 2);
      guarded[0] = leftSentinel;
      guarded[length + 1] = rightSentinel;
      for (let index: number = 0; index < length; index++) {
        guarded[index + 1] = (round * 29 + sample * 13 + index * 7) & 0xff;
      }
      const view: Uint8Array = guarded.subarray(1, length + 1);
      const expectedHash: number = fnv1a(view);
      const expectedFirst: number = length === 0 ? -1 : view[0];
      const expectedLast: number = length === 0 ? -1 : view[length - 1];
      const result: BufferHashResult = probe.hashBuffer(view);
      const guardsIntact: boolean = guarded[0] === leftSentinel && guarded[length + 1] === rightSentinel;
      if (result.hash !== expectedHash || result.length !== length || result.first !== expectedFirst ||
        result.last !== expectedLast || result.nativeTid !== this.mainTid || !guardsIntact) {
        throw new Error(`buffer case failed round=${round} sample=${sample} length=${length}`);
      }
      aggregate = Math.imul((aggregate ^ result.hash ^ length) >>> 0, 16777619) >>> 0;
      hilog.info(DOMAIN, TAG,
        'E1_BUFFER_SAMPLE|round=%{public}d|sample=%{public}d|length=%{public}d|' +
        'leftSentinel=%{public}d|rightSentinel=%{public}d|hash=%{public}s|guards=true|nativeTid=%{public}d',
        round, sample + 1, length, leftSentinel, rightSentinel, result.hash.toString(), result.nativeTid);
    }
    return aggregate;
  }

  private runFdCase(round: number): number {
    const pair: FdPair = probe.createFdPair();
    if (pair.fdA < 0 || pair.fdB < 0 || pair.fdA === pair.fdB || pair.creatorTid !== this.mainTid) {
      throw new Error(`invalid fd pair round=${round}`);
    }

    const result: FdTransferResult = probe.transferFdOwnership(pair.fdA, pair.fdB, round);
    const expectedPayload: string = fdPayload(round);
    const expectedHash: number = fnv1aString(expectedPayload);
    if (result.fdA !== pair.fdA || result.fdB !== pair.fdB || result.duplicateFd < 0 ||
      result.written !== expectedPayload.length || result.read !== expectedPayload.length ||
      result.closeOriginalA !== 0 || result.closeDuplicate !== 0 || result.closeOriginalB !== 0 ||
      result.duplicateCloseA !== -1 || result.duplicateCloseErrnoA !== EBADF ||
      result.duplicateCloseB !== -1 || result.duplicateCloseErrnoB !== EBADF ||
      result.payload !== expectedPayload || result.payloadHash !== expectedHash || result.nativeTid !== this.mainTid) {
      throw new Error(`fd ownership case failed round=${round}`);
    }

    hilog.info(DOMAIN, TAG,
      'E1_FD_RESULT|round=%{public}d|fdA=%{public}d|fdB=%{public}d|duplicateFd=%{public}d|' +
      'written=%{public}d|read=%{public}d|payload=%{public}s|payloadHash=%{public}s|' +
      'closeA=0|closeDuplicate=0|closeB=0|duplicateCloseA=-1|errnoA=9|duplicateCloseB=-1|errnoB=9|' +
      'owner=closed-by-native|nativeTid=%{public}d',
      round, result.fdA, result.fdB, result.duplicateFd, result.written, result.read,
      result.payload, result.payloadHash.toString(), result.nativeTid);
    return result.payloadHash;
  }

  private async waitForCallbacks(round: number): Promise<void> {
    for (let attempt: number = 1; attempt <= 400; attempt++) {
      if (this.callbackCount >= SAMPLES_PER_ROUND) {
        return;
      }
      await delay(25);
    }
    throw new Error(`async callback timeout round=${round} received=${this.callbackCount}`);
  }

  private async runIntegratedProbe(): Promise<void> {
    try {
      await this.runE1Probe();
    } catch (error) {
      const detail: string = cleanDetail(error as Error);
      AppStorage.set<string>('e1Status', 'E1 C-only FAIL');
      AppStorage.set<string>('e1Detail', detail);
      hilog.error(DOMAIN, TAG, 'E1_C_PROBE_RESULT|verdict=FAIL|detail=%{public}s', detail);
      throw new Error(detail);
    }
    AppStorage.set<string>('e2Status', 'E2 C network RUNNING');
    AppStorage.set<string>('e2Detail', 'E1 regression PASS; starting E2 network rounds');
    await delay(50);
    this.runE2Probe();
  }

  private runE2Probe(): void {
    const versionResult: string = e2network.version();
    if (versionResult !== 'e2-c-network-api24-probe/0.0.1') {
      throw new Error(`E2 native version mismatch actual=${versionResult}`);
    }

    const baseline: ResourceSnapshot = probe.resourceSnapshot();
    if (baseline.pid <= 0 || baseline.tid !== this.mainTid || baseline.fdCount <= 0 || baseline.threadCount <= 0) {
      throw new Error(`invalid E2 resource baseline pid=${baseline.pid} tid=${baseline.tid}`);
    }
    let expectedHostAddress: string = '';
    let expectedHostRoute: string = '';
    let tcpHashAggregate: number = 2166136261;
    let udpHashAggregate: number = 2166136261;
    let hostHashAggregate: number = 2166136261;

    hilog.info(DOMAIN, TAG,
      'E2_INITIAL_RESOURCE|pid=%{public}d|mainTid=%{public}d|fd=%{public}d|threads=%{public}d|' +
      'testRunner=false|scope=C-only',
      baseline.pid, baseline.tid, baseline.fdCount, baseline.threadCount);

    for (let round: number = 1; round <= FULL_ROUNDS; round++) {
      AppStorage.set<string>('e2Detail',
        `PID ${baseline.pid} | round ${round}/${FULL_ROUNDS} | TCP UDP DNS host endpoint`);
      const before: ResourceSnapshot = probe.resourceSnapshot();
      const result: E2NetworkRoundResult = e2network.runNetworkRound(round);
      const after: ResourceSnapshot = probe.resourceSnapshot();

      if (result.pid !== baseline.pid || result.tid !== this.mainTid || result.round !== round ||
        result.tcpLoopBytes !== 4096 || result.tcpLoopHash === 0 || result.tcpLoopEpollEvents <= 0 ||
        result.tcpLoopTimeoutMs < 15 || result.tcpLoopPeerClose !== 1 ||
        result.tcpLoopRefusedErrno !== ECONNREFUSED || result.tcpLoopListenerCloseErrno !== EBADF ||
        result.tcpLoopServerReads <= 1 || result.tcpLoopClientReads <= 1 ||
        result.udpLoopClientToServer !== 3 || result.udpLoopServerToClient !== 3 ||
        result.udpLoopClientAggregate === 0 || result.udpLoopServerAggregate === 0 ||
        result.hostRoute.length === 0 || result.hostAddress.length === 0 || result.hostAddress.startsWith('127.') ||
        result.hostTcpBytes !== 2048 || result.hostTcpHash === 0 ||
        result.hostUdpDatagrams !== 3 || result.hostUdpAggregate === 0 ||
        result.localhostCount <= 0 || result.invalidDnsCode === 0) {
        throw new Error(`E2 native result assertion failed round=${round}`);
      }
      if (round === 1) {
        expectedHostAddress = result.hostAddress;
        expectedHostRoute = result.hostRoute;
      } else if (result.hostAddress !== expectedHostAddress || result.hostRoute !== expectedHostRoute) {
        throw new Error(`route-derived host endpoint drift round=${round} ` +
          `expected=${expectedHostRoute}/${expectedHostAddress} actual=${result.hostRoute}/${result.hostAddress}`);
      }
      if (before.pid !== baseline.pid || after.pid !== baseline.pid ||
        before.tid !== this.mainTid || after.tid !== this.mainTid ||
        before.fdCount !== after.fdCount || before.threadCount !== after.threadCount ||
        after.fdCount !== baseline.fdCount || after.threadCount !== baseline.threadCount) {
        throw new Error(`E2 resource leak round=${round} fd=${before.fdCount}->${after.fdCount} ` +
          `threads=${before.threadCount}->${after.threadCount} baseline=${baseline.fdCount}/${baseline.threadCount}`);
      }

      tcpHashAggregate = Math.imul((tcpHashAggregate ^ result.tcpLoopHash ^ round) >>> 0, 16777619) >>> 0;
      udpHashAggregate = Math.imul((udpHashAggregate ^ result.udpLoopClientAggregate ^
        result.udpLoopServerAggregate ^ round) >>> 0, 16777619) >>> 0;
      hostHashAggregate = Math.imul((hostHashAggregate ^ result.hostTcpHash ^
        result.hostUdpAggregate ^ round) >>> 0, 16777619) >>> 0;
      hilog.info(DOMAIN, TAG,
        'E2_ROUND_RESULT|verdict=PASS|round=%{public}d|pid=%{public}d|mainTid=%{public}d|' +
        'tcpLoop=PASS|udpLoop=PASS|hostTcp=PASS|hostUdp=PASS|dns=PASS|events=PASS|' +
        'hostAddress=%{public}s|tcpHash=%{public}s|udpClientAggregate=%{public}s|' +
        'udpServerAggregate=%{public}s|hostTcpHash=%{public}s|hostUdpAggregate=%{public}s|' +
        'beforeFd=%{public}d|afterFd=%{public}d|beforeThreads=%{public}d|afterThreads=%{public}d|' +
        'fdLeak=false|threadLeak=false',
        round, baseline.pid, this.mainTid, result.hostAddress, result.tcpLoopHash.toString(),
        result.udpLoopClientAggregate.toString(), result.udpLoopServerAggregate.toString(),
        result.hostTcpHash.toString(), result.hostUdpAggregate.toString(),
        before.fdCount, after.fdCount, before.threadCount, after.threadCount);
    }

    const finalSnapshot: ResourceSnapshot = probe.resourceSnapshot();
    if (finalSnapshot.pid !== baseline.pid || finalSnapshot.tid !== this.mainTid ||
      finalSnapshot.fdCount !== baseline.fdCount || finalSnapshot.threadCount !== baseline.threadCount) {
      throw new Error(`E2 final resource drift fd=${baseline.fdCount}->${finalSnapshot.fdCount} ` +
        `threads=${baseline.threadCount}->${finalSnapshot.threadCount}`);
    }
    AppStorage.set<string>('e2Status', 'E2 C network PASS');
    AppStorage.set<string>('e2Detail',
      `PID ${baseline.pid} | 10 rounds | TCP/UDP loopback + host | DNS/error paths`);
    hilog.info(DOMAIN, TAG,
      'E2_C_PROBE_RESULT|verdict=PASS|pid=%{public}d|mainTid=%{public}d|rounds=10|' +
      'tcpLoopback=10|udpLoopback=10|hostTcp=10|hostUdpDatagrams=30|dnsLocalhost=10|' +
      'dnsInvalid=10|pollTimeout=10|peerClose=10|refused=10|listenerRecreated=10|' +
      'hostRoute=%{public}s|hostAddress=%{public}s|tcpHashAggregate=%{public}s|' +
      'udpHashAggregate=%{public}s|hostHashAggregate=%{public}s|' +
      'baselineFd=%{public}d|finalFd=%{public}d|baselineThreads=%{public}d|finalThreads=%{public}d|' +
      'fdLeak=false|threadLeak=false|testRunner=false|scope=C-only',
      baseline.pid, this.mainTid, expectedHostRoute, expectedHostAddress,
      tcpHashAggregate.toString(), udpHashAggregate.toString(), hostHashAggregate.toString(),
      baseline.fdCount, finalSnapshot.fdCount, baseline.threadCount, finalSnapshot.threadCount);
  }

  private async runE1Probe(): Promise<void> {
    const pingResult: string = probe.ping();
    const versionResult: string = probe.version();
    if (pingResult !== 'pong' || versionResult !== 'e1-c-api24-probe/0.0.1') {
      throw new Error(`native baseline mismatch ping=${pingResult} version=${versionResult}`);
    }

    const beforeWarmup: ResourceSnapshot = probe.initializeAsync((round: number, sequence: number, payload: string,
      payloadHash: number, producerTid: number, consumerTid: number) => {
      this.onAsyncCallback(round, sequence, payload, payloadHash, producerTid, consumerTid);
    });
    this.mainTid = beforeWarmup.tid;
    if (beforeWarmup.pid <= 0 || beforeWarmup.tid <= 0 || beforeWarmup.fdCount <= 0 ||
      beforeWarmup.threadCount <= 0) {
      throw new Error('invalid pre-warmup process resource snapshot');
    }

    this.activeRound = 0;
    this.callbackCount = 0;
    this.expectedSequence = 1;
    this.callbackError = '';
    this.callbackProducerTid = -1;
    this.callbackConsumerTid = -1;
    this.callbackHashAggregate = 2166136261;
    probe.startAsyncRound(0);
    await this.waitForCallbacks(0);
    const warmupCompletion: AsyncRoundResult = probe.completeAsyncRound(0);
    if (this.callbackError.length > 0 || this.callbackCount !== SAMPLES_PER_ROUND ||
      warmupCompletion.queued !== SAMPLES_PER_ROUND || warmupCompletion.callbackStatus !== 0 ||
      warmupCompletion.joinStatus !== 0 || warmupCompletion.producerTid !== this.callbackProducerTid ||
      this.callbackConsumerTid !== this.mainTid || this.callbackProducerTid === this.mainTid) {
      throw new Error(`TSFN warmup failed callbacks=${this.callbackCount} queued=${warmupCompletion.queued} ` +
        `detail=${this.callbackError}`);
    }
    await delay(50);
    const initial: ResourceSnapshot = probe.resourceSnapshot();
    if (initial.pid !== beforeWarmup.pid || initial.tid !== this.mainTid ||
      initial.fdCount < beforeWarmup.fdCount || initial.threadCount < beforeWarmup.threadCount) {
      throw new Error(`invalid stabilized resource baseline fd=${beforeWarmup.fdCount}->${initial.fdCount} ` +
        `threads=${beforeWarmup.threadCount}->${initial.threadCount}`);
    }
    hilog.info(DOMAIN, TAG,
      'E1_TSFN_WARMUP|verdict=PASS|callbacks=100|producerTid=%{public}d|consumerTid=%{public}d|' +
      'beforeFd=%{public}d|afterFd=%{public}d|beforeThreads=%{public}d|afterThreads=%{public}d',
      this.callbackProducerTid, this.callbackConsumerTid, beforeWarmup.fdCount, initial.fdCount,
      beforeWarmup.threadCount, initial.threadCount);
    hilog.info(DOMAIN, TAG,
      'E1_INITIAL_RESOURCE|pid=%{public}d|mainTid=%{public}d|fd=%{public}d|threads=%{public}d|' +
      'mechanism=napi_threadsafe_function|baseline=post-warmup',
      initial.pid, initial.tid, initial.fdCount, initial.threadCount);

    let totalBuffers: number = 0;
    let totalCallbacks: number = 0;
    let totalFdTransfers: number = 0;
    let priorAfterFd: number = initial.fdCount;
    let priorAfterThreads: number = initial.threadCount;

    for (let round: number = 1; round <= FULL_ROUNDS; round++) {
      AppStorage.set<string>('e1Detail', `Round ${round}/${FULL_ROUNDS}: buffer, fd, pthread callback`);
      const before: ResourceSnapshot = probe.resourceSnapshot();
      if (before.pid !== initial.pid || before.tid !== this.mainTid) {
        throw new Error(`resource snapshot identity drift round=${round}`);
      }

      const bufferAggregate: number = this.runBufferCases(round);
      totalBuffers += SAMPLES_PER_ROUND;
      const fdHash: number = this.runFdCase(round);
      totalFdTransfers++;

      this.activeRound = round;
      this.callbackCount = 0;
      this.expectedSequence = 1;
      this.callbackError = '';
      this.callbackProducerTid = -1;
      this.callbackConsumerTid = -1;
      this.callbackHashAggregate = 2166136261;
      probe.startAsyncRound(round);
      await this.waitForCallbacks(round);
      const completion: AsyncRoundResult = probe.completeAsyncRound(round);
      if (this.callbackError.length > 0) {
        throw new Error(`async callback assertion failed round=${round}: ${this.callbackError}`);
      }
      if (this.callbackCount !== SAMPLES_PER_ROUND || completion.round !== round ||
        completion.queued !== SAMPLES_PER_ROUND || completion.callbackStatus !== 0 || completion.joinStatus !== 0 ||
        completion.producerTid !== this.callbackProducerTid || this.callbackConsumerTid !== this.mainTid ||
        this.callbackProducerTid === this.mainTid) {
        throw new Error(`async completion failed round=${round} callbacks=${this.callbackCount} queued=${completion.queued}`);
      }
      totalCallbacks += this.callbackCount;

      await delay(25);
      const after: ResourceSnapshot = probe.resourceSnapshot();
      const fdStable: boolean = after.fdCount === before.fdCount && after.fdCount <= initial.fdCount;
      const threadsStable: boolean = after.threadCount === before.threadCount &&
        after.threadCount <= initial.threadCount;
      const monotonicIncrease: boolean = after.fdCount > priorAfterFd || after.threadCount > priorAfterThreads;
      if (!fdStable || !threadsStable || monotonicIncrease) {
        throw new Error(`resource growth round=${round} fd=${before.fdCount}->${after.fdCount} ` +
          `threads=${before.threadCount}->${after.threadCount} prior=${priorAfterFd}/${priorAfterThreads}`);
      }
      priorAfterFd = after.fdCount;
      priorAfterThreads = after.threadCount;

      hilog.info(DOMAIN, TAG,
        'E1_ROUND_RESULT|verdict=PASS|round=%{public}d|pid=%{public}d|mainTid=%{public}d|' +
        'producerTid=%{public}d|consumerTid=%{public}d|buffers=100|callbacks=100|fdTransfers=1|' +
        'bufferHashAggregate=%{public}s|callbackHashAggregate=%{public}s|fdPayloadHash=%{public}s|' +
        'beforeFd=%{public}d|afterFd=%{public}d|beforeThreads=%{public}d|afterThreads=%{public}d|' +
        'monotonicIncrease=false',
        round, initial.pid, this.mainTid, this.callbackProducerTid, this.callbackConsumerTid,
        bufferAggregate.toString(), this.callbackHashAggregate.toString(), fdHash.toString(),
        before.fdCount, after.fdCount, before.threadCount, after.threadCount);
    }

    probe.shutdownAsync();
    await delay(50);
    const finalSnapshot: ResourceSnapshot = probe.resourceSnapshot();
    if (finalSnapshot.pid !== initial.pid || finalSnapshot.tid !== this.mainTid ||
      finalSnapshot.fdCount > initial.fdCount || finalSnapshot.threadCount > initial.threadCount) {
      throw new Error(`final resource growth fd=${initial.fdCount}->${finalSnapshot.fdCount} ` +
        `threads=${initial.threadCount}->${finalSnapshot.threadCount}`);
    }

    AppStorage.set<string>('e1Status', 'E1 C-only PASS');
    AppStorage.set<string>('e1Detail',
      `PID ${initial.pid} | 10 rounds | 1000 buffers | 1000 callbacks | 10 fd transfers`);
    hilog.info(DOMAIN, TAG,
      'E1_C_PROBE_RESULT|verdict=PASS|pid=%{public}d|mainTid=%{public}d|rounds=10|' +
      'buffers=%{public}d|callbacks=%{public}d|fdTransfers=%{public}d|' +
      'baselineFd=%{public}d|finalFd=%{public}d|baselineThreads=%{public}d|finalThreads=%{public}d|' +
      'mechanism=napi_threadsafe_function|scope=C-only',
      initial.pid, this.mainTid, totalBuffers, totalCallbacks, totalFdTransfers,
      initial.fdCount, finalSnapshot.fdCount, initial.threadCount, finalSnapshot.threadCount);
  }
}
