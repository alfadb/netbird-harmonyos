@Entry
@Component
struct Index {
  @StorageLink('e1Status') e1Status: string = 'E1 C-only RUNNING';
  @StorageLink('e1Detail') e1Detail: string = 'Initializing ordinary EntryAbility probe';

  build() {
    Column({ space: 14 }) {
      Text('E1 C-only')
        .fontSize(28)
        .fontWeight(FontWeight.Bold)
        .fontColor('#17324D')
      Text(this.e1Status)
        .fontSize(20)
        .fontWeight(FontWeight.Medium)
        .fontColor(this.e1Status.includes('PASS') ? '#176B3A' :
          this.e1Status.includes('FAIL') ? '#A12622' : '#3D4A55')
      Text(this.e1Detail)
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
