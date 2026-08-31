/**
 * N1a gate data-plane probe visible result page (C9).
 *
 * Staged by prepare-hap-snapshot.sh over the r1 snapshot's pages/Index so
 * the ordinary EntryAbility cold start runs the real probe once and renders
 * the verdict. The page text is produced from the SAME probe result object
 * whose NAPI overlay emitted the single-line N1A_RESULT marker, so the
 * "page agrees with the marker" clause of frozen criterion C9 holds by
 * construction (E2 precedent: the visible page is rendered from the measured
 * result, never from a separate or synthetic source).
 *
 * The probe is synchronous and takes several seconds (handshake + pump +
 * tick gaps + backpressure); the page renders after it returns. No key
 * material is ever displayed: only the verdict, the C5 state, throughput
 * and counters.
 */
import n1a from 'libentry.so';
import { N1aProbeResult } from 'libentry.so';
import hilog from '@ohos.hilog';

const DOMAIN: number = 0x2900;
const TAG: string = 'N1aProbe';

function cleanText(value: string): string {
  return value.replace(/\|/g, '/').replace(/\r/g, ' ').replace(/\n/g, ' ');
}

@Entry
@Component
struct Index {
  @State private probeDone: boolean = false;
  @State private probeOk: boolean = false;
  @State private c5: string = 'pending';
  @State private throughput: string = '0.00';
  @State private verifiedPackets: string = '0';
  @State private pumpMs: string = '0';

  aboutToAppear(): void {
    hilog.info(DOMAIN, TAG, 'N1a result page aboutToAppear');
    try {
      const result: N1aProbeResult = n1a.runN1aProbe();
      this.probeDone = true;
      this.probeOk = result.ok;
      this.c5 = cleanText(result.c5);
      this.throughput = result.throughputMiBps.toFixed(2);
      this.verifiedPackets = result.verifiedPacketsTotal.toString();
      this.pumpMs = result.pumpMs.toFixed(1);
      hilog.info(DOMAIN, TAG, 'N1a result page rendered from probe result');
    } catch (error) {
      // Fail-closed: a thrown probe renders FAIL. The native overlay has
      // already emitted its FAIL marker for the same execution.
      this.probeDone = true;
      this.probeOk = false;
      this.c5 = 'native-throw';
      hilog.error(DOMAIN, TAG, 'N1a result page probe threw: %{public}s',
        cleanText((error as Error).message || 'no-message'));
    }
  }

  build() {
    Column({ space: 18 }) {
      Text('N1a 数据面探针')
        .fontSize(26)
        .fontWeight(FontWeight.Bold)
        .fontColor('#17324D')
      if (this.probeDone) {
        Text(this.probeOk ? 'PASS' : 'FAIL')
          .fontSize(42)
          .fontWeight(FontWeight.Bold)
          .fontColor(this.probeOk ? '#176B3A' : '#A12622')
        Text(`verdict: ${this.probeOk ? 'pass' : 'fail'}`)
          .fontSize(15)
          .fontColor('#3D4A55')
        Text(`c5: ${this.c5}`)
          .fontSize(15)
          .fontColor('#3D4A55')
        Text(`throughput: ${this.throughput} MiB/s`)
          .fontSize(15)
          .fontColor('#3D4A55')
        Text(`verifiedPackets: ${this.verifiedPackets}`)
          .fontSize(15)
          .fontColor('#3D4A55')
        Text(`pumpMs: ${this.pumpMs}`)
          .fontSize(13)
          .fontColor('#3D4A55')
      } else {
        Text('running...')
          .fontSize(18)
          .fontColor('#3D4A55')
      }
    }
    .width('100%')
    .height('100%')
    .justifyContent(FlexAlign.Center)
    .padding(24)
    .backgroundColor('#F2F6F8')
  }
}