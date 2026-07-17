@Entry
@Component
struct Index {
  @StorageLink('e2Status') e2Status: string = 'E2 C network RUNNING';
  @StorageLink('e2Detail') e2Detail: string = 'E1 regression running before E2';

  build() {
    Column({ space: 14 }) {
      Text('E2 C network')
        .fontSize(28)
        .fontWeight(FontWeight.Bold)
        .fontColor('#17324D')
      Text(this.e2Status)
        .fontSize(20)
        .fontWeight(FontWeight.Medium)
        .fontColor(this.e2Status.includes('PASS') ? '#176B3A' :
          this.e2Status.includes('FAIL') ? '#A12622' : '#3D4A55')
      Text(this.e2Detail)
        .fontSize(15)
        .fontColor('#3D4A55')
        .textAlign(TextAlign.Center)
        .maxLines(4)
    }
    .width('100%')
    .height('100%')
    .justifyContent(FlexAlign.Center)
    .padding(24)
    .backgroundColor('#F2F6F8')
  }
}
