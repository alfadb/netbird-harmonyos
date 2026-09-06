import hilog from '@ohos.hilog';

const DOMAIN: number = 0x2900;
const TAG: string = 'N1BDiscVpn';
const BUNDLE_NAME: string = 'cn.alfadb.netbird.n1bdisc';

@Entry
@Component
struct Index {
  build() {
    Column({ space: 18 }) {
      Text('N1BDISC physical HAP skeleton')
        .fontSize(24)
        .fontWeight(FontWeight.Bold)
        .fontColor('#17324D')
      Text(BUNDLE_NAME)
        .fontSize(14)
        .fontColor('#3D4A55')
        .textAlign(TextAlign.Center)
      Button('Trigger (HiLog only)')
        .fontSize(17)
        .width(220)
        .height(48)
        .backgroundColor('#17324D')
        .onClick(() => {
          hilog.info(DOMAIN, TAG, 'UI_TRIGGER_CLICKED|bundle=%{public}s', BUNDLE_NAME);
        })
    }
    .width('100%')
    .height('100%')
    .justifyContent(FlexAlign.Center)
    .padding(24)
    .backgroundColor('#F2F6F8')
  }
}
