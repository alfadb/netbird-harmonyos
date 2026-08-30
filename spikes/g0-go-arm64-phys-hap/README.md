# G0 stock Go arm64 c-shared loader 探针（host-only 构建工程）

## 目的

测量 **stock（零补丁）Go 1.25.12 arm64 c-shared 库能否被物理 HarmonyOS 设备 loader 接受**，并做 Go
runtime 最小冒烟（导出函数、goroutine、timer、分配）。架构镜像 E3 spike 的「C 机制 + ArkTS 打点」模式：

- `libgoprobe.so`：stock Go 1.25.12 `c-shared` 最小探针（`go-probe/`，导出 `Hello`、`RuntimeProbe`）；
- `libgoloader.so`：纯 C Node-API 薄 loader（`entry/src/main/cpp/goloader.c`），`dlopen("libgoprobe.so",
  RTLD_NOW|RTLD_LOCAL)` → `dlsym` → 调用，逐字段返回结果对象；
- ArkTS（`pages/Index.ets`）在 `aboutToAppear` 自动运行探针，组装**恰一行** HiLog marker 打点
  （domain `0x2900`，tag `G0GoProbe`）。

## 边界

- 研究探针，**不得演化为产品壳**；无签名配置，仅产出 unsigned HAP。
- 无权限、无网络、无 VPN、无 NetBird/WireGuard 代码、无外部 endpoint；`module.json5` 只有
  EntryAbility（无 extensionAbilities、无 requestPermissions）。
- 本目录只负责工程实现与 **host-only 构建**；任何设备端动作不在本目录执行。

## 构建步骤（host-only）

```bash
cd /home/worker/work/base/netbird-harmonyos/spikes/g0-go-arm64-phys-hap

# 1) 构建 stock Go 探针库（内含 9 项 ELF 断言，任一不满足 exit 1）
HARMONYOS_NATIVE_HOME=/home/worker/harmonyos/command-line-tools/6.1.1.290/sdk/default/openharmony/native \
GO_PROBE_OUTPUT_DIR=$PWD/entry/libs/arm64-v8a \
bash go-probe/build-arm64.sh

# 2) 依赖与 Unsigned HAP（固定工具路径，不用浮动 current 符号链接）
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/ohpm install
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw clean \
  --mode module -p product=default -p module=entry@default \
  -p buildMode=debug --no-daemon
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap \
  --mode module -p product=default -p module=entry@default \
  -p buildMode=debug --no-daemon
```

产物：`entry/build/default/outputs/default/entry-default-unsigned.hap`，native 成员
`libs/arm64-v8a/libgoloader.so` 与 `libs/arm64-v8a/libgoprobe.so`。打包成员必须与构建输入逐字节一致
（`nativeLib.debugSymbol.strip: false` 关闭 hvigor 打包前 strip；见下「与 e3 骨架的差异」）。

> **`libgoprobe.so` 以仓内制品为准**：`entry/libs/arm64-v8a/libgoprobe.so` 已提交入仓（SHA-256
> `489f1aad…b5ad7`，与 [治理计划](../../docs/g0-go-arm64-physical-probe.md) 预登记值一致），是 Windows
> 签名构建的**冻结输入**——clone/pull 后原位使用，打包前复算 hash。`go-probe/build-arm64.sh` 的本地
> 重建只用于可复现性验证（本 Linux 工具链下逐字节一致）；Windows 侧 cgo 交叉编译器不同会得到不同
> 字节，**不得**以 Windows 重建产物替代仓内冻结制品。

`go-probe/build-arm64.sh` 的 ELF 断言（9 项）：AArch64 ELF64 `ET_DYN`；恰 1 个 `PT_TLS`；`.rela.dyn` 中
`R_AARCH64_TLS_TPREL64` 恰 1 条；Dynamic `FLAGS` 无 `STATIC_TLS`；`NEEDED` 恰只 `libc.so`；动态符号导出
FUNC `Hello` 与 `RuntimeProbe`；该 TPREL64 为本地（r_info 符号索引 0、addend 0——与 x86_64 被拒剖面的
核心判别项）；`FLAGS_1` 含 `NOW|NODELETE`；导入 `pthread_create`（UND FUNC）。

## Marker 格式

成功路径（`runGoProbe()` 返回，恰一行，`hilog.info`，tag `G0GoProbe`）：

```text
G0_RESULT|verdict=PASS|ok=true|pid=<pid>|stage=complete|dlopenLoaded=true|loaderErrno=0|loaderError=|hello=42|runtimeBytes=1048576
```

- `verdict=PASS` 仅当 `stage=complete`；`stage` ∈ {`dlopen`,`dlsym`,`hello`,`runtime`,`complete`}。
- `dlopenLoaded = stage !== 'dlopen'`；`loaderError` 在 C 侧消毒（`<0x20`、`0x7f`、`|` → 空格），
  ArkTS 侧再消毒一次保证单行安全。
- `runGoProbe()` 抛异常时：`G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=native-throw|dlopenLoaded=false|loaderErrno=0|loaderError=<消毒后错误消息>|hello=0|runtimeBytes=0`。

## 纪律注记

- 本工程是 G0 计划的仓内实现物；**设备端执行只受 [docs/g0-go-arm64-physical-probe.md](../../docs/g0-go-arm64-physical-probe.md)
  治理约束**（gate 逐门推进、Live 前用户再次确认、ID 不复用）。本 README 不改变任何门序与授权状态。
- 与 e3 骨架的差异：单 product `default`、无 signingConfigs、bundleName `cn.alfadb.netbird.g0probe`、
  无 extensionAbilities/requestPermissions；`entry/build-profile.json5` 增加
  `nativeLib.debugSymbol.strip: false`——这是满足「打包 `libgoprobe.so` 成员与构建输入逐字节一致」的
  必要配置（hvigor 默认打包前 strip 会改写 stock Go 产物字节）。
