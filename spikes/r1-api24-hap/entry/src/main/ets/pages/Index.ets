@Entry
@Component
struct Index {
  build() {
    Column({ space: 12 }) {
      Text('R1 API 24')
        .fontSize(28)
        .fontWeight(FontWeight.Bold)
        .fontColor('#17324D')
      Text('Ready')
        .fontSize(16)
        .fontColor('#3D4A55')
    }
    .width('100%')
    .height('100%')
    .justifyContent(FlexAlign.Center)
    .padding(24)
    .backgroundColor('#F2F6F8')
  }
}
