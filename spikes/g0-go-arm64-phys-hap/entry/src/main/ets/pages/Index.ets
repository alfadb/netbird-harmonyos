import { runGoProbe, GoProbeResult } from 'libgoloader.so';
import hilog from '@ohos.hilog';

const DOMAIN: number = 0x2900;
const TAG: string = 'G0GoProbe';

function cleanMarkerField(value: string): string {
  return value.replace(/\|/g, ' ').replace(/\r/g, ' ').replace(/\n/g, ' ');
}

@Entry
@Component
struct Index {
  @State private probeDone: boolean = false;
  @State private probeOk: boolean = false;
  @State private probeStage: string = 'pending';
  @State private loaderErrno: number = 0;
  @State private loaderError: string = '';
  @State private hello: number = 0;
  @State private runtimeBytes: number = 0;
  @State private pid: number = 0;

  aboutToAppear(): void {
    this.runProbe();
  }

  private runProbe(): void {
    let marker: string;
    try {
      const result: GoProbeResult = runGoProbe();
      this.probeDone = true;
      this.probeOk = result.ok;
      this.probeStage = result.stage;
      this.loaderErrno = result.loaderErrno;
      this.loaderError = cleanMarkerField(result.loaderError);
      this.hello = result.hello;
      this.runtimeBytes = result.runtimeBytes;
      this.pid = result.pid;
      marker = `G0_RESULT|verdict=${result.ok ? 'PASS' : 'FAIL'}|ok=${result.ok}` +
        `|pid=${result.pid}|stage=${result.stage}|dlopenLoaded=${result.stage !== 'dlopen'}` +
        `|loaderErrno=${result.loaderErrno}|loaderError=${cleanMarkerField(result.loaderError)}` +
        `|hello=${result.hello}|runtimeBytes=${result.runtimeBytes}`;
    } catch (error) {
      const message: string = cleanMarkerField((error as Error).message || 'no-message');
      this.probeDone = true;
      this.probeOk = false;
      this.probeStage = 'native-throw';
      this.loaderErrno = 0;
      this.loaderError = message;
      this.hello = 0;
      this.runtimeBytes = 0;
      this.pid = 0;
      marker = `G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=native-throw` +
        `|dlopenLoaded=false|loaderErrno=0|loaderError=${message}|hello=0|runtimeBytes=0`;
    }
    hilog.info(DOMAIN, TAG, '%{public}s', marker);
  }

  build() {
    Column({ space: 18 }) {
      Text('G0 Go Loader 探针')
        .fontSize(26)
        .fontWeight(FontWeight.Bold)
        .fontColor('#17324D')
      Text(this.probeDone ? (this.probeOk ? '通过' : '失败') : '待运行')
        .fontSize(18)
        .fontWeight(FontWeight.Medium)
        .fontColor(this.probeDone ? (this.probeOk ? '#176B3A' : '#A12622') : '#3D4A55')
      Text(`stage: ${this.probeStage}`)
        .fontSize(15)
        .fontColor('#3D4A55')
      Text(this.loaderError.length > 0 ? this.loaderError : 'loaderError: (empty)')
        .fontSize(13)
        .fontColor('#3D4A55')
        .textAlign(TextAlign.Center)
        .maxLines(6)
      Text(`hello: ${this.hello}`)
        .fontSize(15)
        .fontColor('#3D4A55')
      Text(`runtimeBytes: ${this.runtimeBytes}`)
        .fontSize(15)
        .fontColor('#3D4A55')
      Text(`pid: ${this.pid}`)
        .fontSize(15)
        .fontColor('#3D4A55')
    }
    .width('100%')
    .height('100%')
    .justifyContent(FlexAlign.Center)
    .padding(24)
    .backgroundColor('#F2F6F8')
  }
}
