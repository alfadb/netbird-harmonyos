# G0 stock Go arm64 loader 物理探针计划与证据模板

最后核验：2026-08-30

> **状态：`consumed-blocked`（G0 探针已完结，2026-08-30）。** 13 门全部执行完毕，单次 Live 实测结论：**冻结物理元组的动态 loader 拒绝 stock Go 1.25.12 arm64 c-shared**（逐字 `initial-exec TLS resolves to dynamic definition`，与 x86_64 Emulator 同错误族；两元组一致、不外推），`EV-G0PHYS1API26-20260830-0001` 经两轮记录审查定级 `reviewed-pass`、`verdict: blocked`（有效实测终态）。pair 已消费、无后继 AUTH。**不是 E1 pass、不是 E8 输入**；E8 保持 `CLOSED`。结果作为已触发的 native N1-Nx + E8 Go 前提处置 ADJ/T0 治理的双实测输入之一（N0 pass + G0 blocked）。本文其余历史叙述按时间顺序理解。

## 定义与依据

`G0` 定义为：在唯一冻结物理目标元组上，对 **stock（零补丁）NetBird 声明工具链 Go 1.25.12** 构建的 **arm64 c-shared** 最小探针库执行的单次 loader/runtime 物理测量 campaign。它回答且只回答一个问题：**该冻结设备元组的动态 loader 是否接受 stock Go c-shared 库，且 Go runtime 最小冒烟（导出函数、goroutine、timer、分配）是否可用。**

依据链：

1. **N0 决议第 7 条（2026-08-09 五席 T0 `UNANIMOUS-SIGN`，用户批准）**：「用户授权后，在独立证据 ID 下、优先于任何 Go1.26 research，测 NetBird 正式工具链 Go1.25.12 arm64 c-shared 最小探针；结果可触发路线重议，但不是 E1 pass 的自动替代」（见 [N0 决议](n0-native-client-feasibility.md)「物理 E3 第一动作纪律」）。
2. **E3-PHYS-PREFLIGHT 已完结**（2026-08-30 `consumed-pass`，[AUTH](evidence/e3-physical-preflight-authorization-2026-08-29-0001.md) / [证据](evidence/e3-physical-preflight-api26-20260829-0001.md)）：N0 纪律中「第一物理动作」已履行，`G0` 是其预定的后续物理动作。
3. **用户于 2026-08-30 显式选择「Go arm64 物理探针先行」路线**（本文「人类直接决策记录」）。

## 范围与非范围

### 范围（仅此一项）

- 单个 campaign、单次 Live、单一冻结目标元组（见下节）。
- 单一普通应用 HAP（无 VPN Extension、无任何 permission、无网络能力），内嵌两个 arm64-v8a native 成员：
  - `libgoprobe.so`：stock Go 1.25.12 `c-shared` 最小探针（导出 `Hello`、`RuntimeProbe`；**无** `NetDialProbe`、无 NetBird/WireGuard 代码、无外部 endpoint）；
  - `libgoloader.so`：纯 C Node-API 薄 loader，`dlopen("libgoprobe.so", RTLD_NOW|RTLD_LOCAL)` 后 `dlsym` 调用上述两符号，并以单一 HiLog tag 输出机器可判定 marker。
- 机器判定 loader 接受/拒绝（拒绝时逐字保留 `dlerror()`/errno）与最小 runtime 冒烟结果。

### 非范围（显式排除）

- **不是 E1 pass**：E1 的正式范围是 API 24 x86_64 Emulator；`EV-E1-EMU24-20260809-0003` 的 `reviewed-pass/blocked` 判定不因本探针改写。本探针结果无论正负都**不是** E1 pass 的自动替代（N0 第 7 条原文）。
- **不是 E8 输入**：E8 保持 `CLOSED`；E8 `OPEN` 必要条件不含本探针，本探针任何结果都不直接改变 E8 状态。
- **不是路线决定本身**：结果只作为已触发的新 ADJ/T0 治理（N1-Nx 门定义与 E8 Go 专属前提处置）的实测输入。
- 不验证：VPN/TUN/protect/fd、NetBird 协议面（management/signal/relay/ICE）、数据面流量、性能、能耗、长稳、产品生命周期、其他设备/build/API/架构（双向不外推）。
- 不使用：Go/NetBird/WireGuard 任何补丁（补丁计数保持 0）、`MANAGE_VPN`、system/debug/enterprise/root、隐藏 API、自动设备输入、外部网络 endpoint。
- 不重跑：任何历史 discovery/rebind/build-confirm/campaign/enrollment 授权（均保持已消耗状态）。

## 人类直接决策记录（ADJ 条目）

- **提出与批准角色**：用户（直接人类决策者）于 2026-08-30 在路线选择中显式选定「Go arm64 物理探针先行」，并于同日显式授权本计划草案的 ID 命名与规格进入实现阶段。本记录按「人类直接决策者优先」规则替代这一次内部 T0 触发，**不声称执行了 T0**；N0 第 7 条的 T0 共识是本探针方向的既有依据，本记录只是发起授权。
- **日期与时区**：`2026-08-30`，`Asia/Shanghai (+08:00)`。
- **触发证据**：`EV-E3-PHYS1API26-20260829-0001`（`reviewed-pass/pass`，S1-S7 全 pass）使 N0 第 7 条预定的「E3 后 Go arm64 最小探针」成为当前可发起动作；`EV-E1-EMU24-20260809-0003` 证明 x86_64 Emulator loader 拒绝 stock Go 1.25.12（`initial-exec TLS resolves to dynamic definition`），而 arm64 物理目标从未实测。
- **调整原因**：在已触发的新 ADJ/T0 治理（N1-Nx + E8 Go 前提处置）形成决议前，取得冻结物理目标上 stock Go loader 行为的权威实测，消除「arm64 物理机可能与 x86_64 Emulator 不同」的未测假设。
- **受影响阶段**：仅新增 `G0` 一次性物理探针例外；不改变 E0-E8、N0、R0-R10 任何既有退出标准、SLO 或补丁预算。
- **已评估替代方案**：直接召集 T0（缺少物理实测输入，Go 路线可行性仍是假设）；Go1.26 research（N0 明确本探针优先于 Go1.26 research）；仅在 Emulator 复测（不回答物理 arm64 问题，且违反双向不外推）；不测量直接走 native 路线（放弃低成本澄清 Go 路线的机会）。
- **R0/SLO/补丁预算影响**：无。补丁计数 0 不变；不新增 SLO；不改变首目标候选。
- **重跑范围**：不重跑任何历史项；本探针为全新 campaign/新 ID、`attempt: initial`、retry N/A。
- **生效条件**：用户显式授权本计划（含 ID 命名）后，按「完整门序列」执行；Live 前须 clean worktree、完整仓外 freeze、独立审查 0 blocker/0 major。
- **回退条件**：用户撤销授权、任一 gate 失败（停止、不重试、不换 ID）、目标元组漂移、越界能力出现、复用任何已消费 ID/enrollment 时，立即停止并以新的动态调整记录重新界定。
- **审查状态**：用户已于 2026-08-30 授权计划与 ID；实现产物（spike/runner/selftest）完成后须经独立审查 0 blocker/0 major 才可提交推送；授权登记与 Live 证据审查角色在 freeze 中固定（跨厂商 isolated reviewer）。

## 生效条件补充（2026-08-30 授权事实）

用户授权的精确范围：① 按草案 ID（`AUTH-G0PHYS1API26-20260830-0001` / `G0-PHYS-PROBE-20260830-0001` / `EV-G0PHYS1API26-20260830-0001`）与 bundle `cn.alfadb.netbird.g0probe` 执行；② 先完成仓内实现（spike 源码、runner Python/PowerShell parity、selftests、freeze example）与 host-only 验证、独立审查，再提交推送；③ 设备端动作从 gate 4 起逐门推进，每门按「完整门序列」执行；④ Live 前须用户再次确认。

## 冻结目标元组（继承，不重测）

| 项 | 值 | 依据 |
| --- | --- | --- |
| distribution | `HarmonyOS` | 历史 measured 记录 |
| device_model | `PLA-AL10` | 历史 measured 记录 |
| full_system_build | `PLA-AL10 7.0.0.102(SP8C00E102R7P3)` | `EV-E3-PHYS1REBIND8-20260828-0001` 四条只读实测（2026-08-28） |
| api | `26` | 同上（rebind 实测，不从 build 推断） |
| kernel_architecture | `aarch64` | 同上 |
| app_abi | `arm64-v8a` | 同上 |
| 设备别名 | `PHYS-1`（仓外受控映射，target token 不入库） | 沿用 |
| HDC | `Ver: 3.2.0d`（执行主机以 freeze 绑定 SHA-256） | 与 E3 冻结一致 |

任何漂移（含设备 OTA）即 blocked record 并退役本 pair；后续须新治理与新 rebind 实测。

## 探针内容规格

### stock Go 构建配方（正式构建须在 campaign 基准 commit 下重做）

```sh
# host launcher（Linux worker，版本精确校验前缀 "go version go1.25.12 "，GOTOOLCHAIN=local）
GOOS=linux GOARCH=arm64 CGO_ENABLED=1 \
CC=<SDK>/native/llvm/bin/aarch64-unknown-linux-ohos-clang \
go build -trimpath -buildmode=c-shared -o libgoprobe.so .
```

- Go 源码：仅 `Hello() C.int`（返回 42）与 `RuntimeProbe(allocBytes C.longlong) C.longlong`（goroutine + 1ms timer + 分配并回传长度；上限 128MiB、负值返回 -1）。`import "C"`、`runtime`、`time` 三个依赖，无 `net`。
- `go.mod`：`go 1.25.5` / `toolchain go1.25.12`（与 NetBird v0.76.3 声明一致）。
- 固定路径：CLI `/home/worker/harmonyos/command-line-tools/6.1.1.290`（禁止 `current` 浮动链接入 freeze）。

### 预登记 ELF 剖面（2026-08-30 host 可行性构建实测；正式构建时须逐项重核）

可行性构建（`/tmp` 一次性产物，**不是** campaign 输入）SHA-256 `a39efc8d1ada624cc4f9e71bed7e688204e88a1925d85c86654eb3e6fe0f1760`、size `2041456`（路径/build-id 不同的早期一次性构建；spike 落地后 `go-probe/build-arm64.sh` 在仓内可复现构建的当前值为 SHA-256 `489f1aad8bfb0ee23b5da1713781b9bd2d69851fd14dd5cff840fe930b9b5ad7`、size `2041704`，三次重建逐字节一致），实测剖面：

- `PT_TLS` 存在（memsz `0x10`、align `0x8`）；
- TLS 重定位**恰 1 条** `R_AARCH64_TLS_TPREL64`，symbol index `0`（本地）、addend `0`；
- `DF_FLAGS = SYMBOLIC|BIND_NOW`，**无** `STATIC_TLS`；`FLAGS_1 = NOW|NODELETE`；
- `NEEDED` 仅 `libc.so`；导入含 `pthread_create`；导出 `Hello`/`RuntimeProbe`。

**与 x86_64 被拒对照**（`EV-E1-EMU24-20260809-0003`）：x86_64 版带 `STATIC_TLS` flag + `R_X86_64_TPOFF64`，被 API 24 x86_64 Emulator musl loader 以 `initial-exec TLS resolves to dynamic definition` 拒绝。arm64 版为本地 `TPREL64`、无 `STATIC_TLS` flag——剖面实质不同，物理 loader 接受与否**只有实测可判**。本对照只作预登记预期，不预设结论。

### HAP 规格

- 隔离目录 `spikes/g0-go-arm64-phys-hap/`（新目录，不改任何历史 spike）。
- 单 bundle（提案）：`cn.alfadb.netbird.g0probe`；单普通 `EntryAbility`，**无 permission**（探针无网络/无敏感能力）。
- `compileSdkVersion: 6.1.1(24)`、`targetSdkVersion`/`compatibleSdkVersion: 6.1.0(23)`（与 E3 冻结构建链一致：DevEco Studio `6.1.1.290` / SDK `6.1.1.125`）。
- native 成员仅 `arm64-v8a` 的 `libgoprobe.so` + `libgoloader.so`；`libgoloader.so` 为纯 C（Node-API 注册 + `dlopen`/`dlsym`/调用 + HiLog 输出），不含其他能力。
- EntryAbility 启动即自动调用 `runGoProbe()`（无 UI 人工步骤），页面显示机器结果（PASS/FAIL + loader 错误文本）；结果页仅作可视化证据，判定以 HiLog marker 为准。

### 机器判定 marker（单一 HiLog tag `G0GoProbe`）

```text
G0_RESULT|verdict=PASS|ok=true|pid=<pid>|stage=complete|dlopenLoaded=true|loaderErrno=0|loaderError=|hello=42|runtimeBytes=<n>
G0_RESULT|verdict=FAIL|ok=false|pid=<pid>|stage=dlopen|dlopenLoaded=false|loaderErrno=<errno>|loaderError=<dlerror verbatim>|hello=0|runtimeBytes=0
G0_RESULT|verdict=DRIFT|...（任何非预期字段组合）
```

runner 对 marker 的 campaign 判定映射（预注册，防歧义）：

| probe marker | campaign verdict |
| --- | --- |
| `verdict=PASS`（dlopen ok + hello=42 + runtimeBytes=请求值） | `pass` |
| `verdict=FAIL` 且 `stage=dlopen` 且 `loaderError` 非空 | `blocked`（**有效实测结果**，逐字登记 loader 错误；同样消费本 pair） |
| `DRIFT`、marker 缺失、多义、超时 | `blocked`（按缺证据 fail-closed；分类记录原因） |
| integrity violation / 白名单外命令 | `invalid` > 一切 |

## 签名链（新 App ID；enrollment 保持已消耗）——已完成登记（2026-08-30）

- **`libgoprobe.so` 经仓库分发**：冻结制品以仓内 blob 提供（`spikes/g0-go-arm64-phys-hap/entry/libs/arm64-v8a/libgoprobe.so`，SHA-256 `489f1aad8bfb0ee23b5da1713781b9bd2d69851fd14dd5cff840fe930b9b5ad7`、size `2041704`）。Windows 侧 `git pull` 后原位可用（打包前须复算 hash 逐字一致）；本地重建仅用于验证可复现性，不得以重建产物替代冻结制品（cgo 交叉编译器差异会使 Windows 重建字节不同）。git blob、本登记与 freeze 三方对账同一 hash。
- 用户在 AGC 为 `cn.alfadb.netbird.g0probe` 新建 App ID；新建 Debug profile 并勾选**已注册的 PHYS-1 设备**与该 App ID。
- **AGC 操作事实**：2026-08-30 由主会话在用户已登录的 AGC 会话（VNC 浏览器）中执行（用户显式授权代操作）：App ID `G0 Probe`（ID `6917615052466301467`）归属项目 `NetBird HarmonyOS Preflight`（与 E3 同项目）；Debug profile `G0 Debug`（证书=**NetBird E3 Debug** 复用、设备=PHYS-1、生效至 2027-08-06）；未申请任何开放能力。平台下载每次再生成 profile 文件（字节不同、同等有效）；签名所用副本以下方 hash 为准。
- **FINAL signed 制品（2026-08-30 更新：本机重签，取代 Windows 版）**：gate 4 前架构修正——确认本 Linux worker 为 E3 收官 campaign 的实际执行宿主（`/home/worker/harmonyos-signing/netbird-e3/` 全史在位；本机工具链 hdc 即 E3 证据登记的二进制 `03123a78…82c81`），gate 4-13 全部在本机执行。为此在本机以 hap-sign-tool 重签（`localSign`/`SHA256withECDSA`/`compatibleVersion 23`，材料：本机 E3 连续性 `.p12`（alias `netbird-harmonyos`）+ 同一 Debug `.cer` + **与 Windows 签名所用逐字相同的 profile** `beea954e…337`；keystore 密码经用户本机终端写入 600 权限临时文件、命令以 `$(cat)` 引用零入日志、签名后即删并核验删除）。全链机器验证 PASS（verify-profile/verify-app success；content.bundle-info.bundle-name/type=debug/devices=1/permissions=0；内嵌 profile 与外部字节一致；arm64 成员恰 `libgoprobe.so`+`libgoloader.so`；libgoprobe 为冻结原字节 `489f1aad…`；module.json 无 requestPermissions/extensionAbilities）：
  - signed HAP：SHA-256 `f98be37f9a3b5ba6d47d262be29204cf0728a0bbb4b4e3ebf08eb268d36c72a2`、size `2157924`（路径 `/home/worker/harmonyos-signing/netbird-g0/artifacts/entry-default-signed.hap`）
  - 成员 `libgoprobe.so`：`489f1aad…b5ad7`（**与仓内冻结制品逐字一致**）
  - 成员 `libgoloader.so`：`9ec9785b969cbe77e44a6c66f20ab1e733a49e4adac631c1634e21e74b0711f1`（本机 CMake 编译）
  - profile（.p7b，`G0 Debug`）：`beea954ec7a76e590bae7504dc819476f335e54a3fe4a5e6bf7c0a3f2f0e3337`（与下文 Windows 版所用同一文件，用户拷入本机）
  - Debug 证书（.cer）：`c13847ecd674a330acb1dfb9df027eb68b21ccadd90eca6e21ebd5a515d6d7fc`（**与 E3 登记值逐字一致**——证书链复用确认）
  - **Windows 版 signed HAP（`30881116…41f60`）标记 `superseded-unconsumed`**：曾在 Windows 构建并通过全链验证，但 gate 4-5 从未对其执行（无安装、无设备命令、无证据消费）；保留其登记作为审计痕迹，campaign 实际输入以上方本机版为准
  - 本机执行环境：hdc `Ver: 3.2.0d`、SHA-256 `03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81`（即 E3 收官证据登记的同一二进制）；OpenJDK `21.0.12.1`；hap-sign-tool.jar SHA-256 `2b35f5ae7c64ec24ef7a3deb129c22fe7c3acc26…`
  - 纪律确认：重签与验证期间未安装、未运行、未发任何设备命令；密码临时文件已删除并核验；验签临时产物已清理
- **不读取设备 UDID**：2026-07-18 的单次 enrollment 例外保持已消耗；设备已在 AGC 注册，新 profile 从 AGC 控制台选择既有设备即可，不需要新的设备端命令。
- 复用既有 `.p12`/`.csr`/Debug `.cer` 链；构建/签名/`verify-profile`/`verify-app`/hash 回传按 [Windows 开发交接](windows-development-handoff.md)模板执行；签名材料与验签临时产物全部仓外，回传仅版本/SHA-256/通过状态。
- signed 内容审计要求：无 permission、无 Extension、debug 普通开发签名、唯一 arm64 成员为 `libgoprobe.so`+`libgoloader.so`、无 Go/NetBird/WireGuard 之外的代码面（`libgoprobe.so` 即 stock Go 最小探针）。

## HDC 白名单与硬边界

除下列外，任何 HDC 子命令禁止（沿用 E3 收官登记的永久禁令风格）。campaign 机制沿用 E3 实战验证的 staging+`bm install -p` 安装路径（20260828/29 campaign 在本设备实测通过的机制，避免引入未实测命令）：

1. **gate 4 host-prep**：恰一次 `tconn <runtime-endpoint>` + 恰一次内存级 `list targets`（用户本人执行，或显式授权主会话代执行；token 不输出、不持久化，设 process-scope `PHYS_1_TARGET`）。
2. **gate 5 target-binding**：`Version`（`hdc version`，无 `-t` 前缀）/ `TupleModel`（`-t <T> shell param get const.product.model`）/ `TupleBuild`（`-t <T> shell param get const.product.software.version`）三探针（脱敏投影，逐字匹配冻结元组；`<T>` 为 `PHYS_1_TARGET` 占位）。
3. **campaign（runner 白名单，audit 形态 argv 逐字固定，占位符 `<PHYS_1_TARGET>`/`<HAP_G0>`；staging 根 `/data/local/tmp/netbird-g0`）**，恰 15 项操作：
   - `BundleDump`：`-t <T> shell bm dump -n cn.alfadb.netbird.g0probe`
   - `PidOf`：`-t <T> shell pidof cn.alfadb.netbird.g0probe`（UI 进程；无 `:vpn` 后缀）
   - `MkdirStaging`：`-t <T> shell mkdir -p /data/local/tmp/netbird-g0/hap`
   - `SendHap`：`-t <T> file send <HAP_G0> /data/local/tmp/netbird-g0/hap/g0.hap`
   - `InstallHap`：`-t <T> shell bm install -p /data/local/tmp/netbird-g0/hap`
   - `StartEntry`：`-t <T> shell aa start -a EntryAbility -b cn.alfadb.netbird.g0probe -m entry`
   - `HilogStream`：`-t <T> shell hilog -T G0GoProbe -v year -v zone`（采集窗口固定 60 秒）
   - `FaultProbe`：`-t <T> shell find /data/log/faultlog/faultlogger -maxdepth 1 -type f -name '*cn.alfadb.netbird.g0probe*' -print`
   - `ForceStop`：`-t <T> shell aa force-stop cn.alfadb.netbird.g0probe`（Reason 仅限 `exception-cleanup`/`final-cleanup`，cleanup-only）
   - `Uninstall`：`-t <T> shell bm uninstall -n cn.alfadb.netbird.g0probe`
   - `RemoveStaging`：`-t <T> shell rm -rf /data/local/tmp/netbird-g0`
   - `StagingProbe`：`-t <T> shell ls -ld /data/local/tmp/netbird-g0`
   - 外加 gate 5 的 `Version`/`TupleModel`/`TupleBuild` 三项
4. 操作名大小写不敏感；未知操作/多余参数/缺参数/bundle 不符/Reason 非法一律拒绝。

永久禁止：设备进程枚举/`ps`、宽泛进程发现、UDID/serial/target discovery、`hidumper`、root/privileged、`uiInput` 自动输入、`MANAGE_VPN`、任何 VPN API 调用、外部网络 endpoint、全量查询、截图/layout 采集（G0 无 UI 采集需求）。host HDC process count 只用**绝对路径探针且只比较第一列**：Linux 为 `/usr/bin/ps -eo comm=,args=`（逐字参数），Windows 为绝对 `%SystemRoot%\System32\tasklist.exe /FO CSV /NH`（只取 image-name 首列，`hdc`/`hdc.exe` 大小写不敏感计数；与 E3 探针同语义的 OS 自适应实现）。字节相等性判定用 `/usr/bin/diff`/`cmp`/sha256 复算（本机 PATH `diff` 被 SDK 遮蔽，沿用验证纪律）。

## Campaign 与重试纪律

- 一次 campaign = 同一冻结输入与 ID 下，从清理基线到最终清理的完整单次执行；首次安装为场景起点；不允许选择性运行或拼接子结果。
- **无 retry**：`attempt: initial`、retry N/A。loader 拒绝（blocked）是有效实测结果，**不是** infra retry 依据；只有新治理 + 新 ID 才可再次执行。
- 任一 gate 失败立即停止；blocked confirmation record 路径 single-use immutable。

## 场景与通过条件（单场景 S1）

1. **S1 基线与执行**：元组复核（3 探针逐字匹配）→ 清理基线（`BundleDump` 未安装、`PidOf` 空）→ `MkdirStaging` → `SendHap` → `InstallHap` → `StartEntry` → 探针自动运行 → `HilogStream` 采集完整窗口（固定 60 秒）→ 得到唯一 `G0_RESULT` marker → `FaultProbe` → finally `ForceStop(final-cleanup)` + `Uninstall` + `RemoveStaging` → 定向 absent 探针（bundle/process/staging 均 absent）。
2. 通过条件：唯一 marker、按上表映射 verdict、cleanup `verified-clean`、integrity 空、白名单外命令为 0。
3. 操作员负担最小化：无 UI 人工步骤（自动运行）；操作员只负责设备连接与 gate 4。

## 完整门序列（13 门，镜像 E3 收官治理）

1. host-only：同步 trusted refs/bundle；核对 exact clean HEAD 含本登记+runner+selftests+docs；记录 `code_sha`；HDC0 用固定绝对 host `ps` 探针。
2. 候选 ID 消费审计 audit-1（仓外双文件 + `.sha256`）。
3. 新建 blocked confirmation freeze + exact reviewer 静态审查。
4. gate 4 host-prep `tconn` + 一次内存级 `list targets`（用户或显式授权代执行）。
5. `-TargetBindingConfirm`（3 白名单探针；漂移即 blocked record + 退役）。
6. ready freeze draft 绑定 confirmation record。
7. reviewer 生成 `g0-ready-freeze-review` record（要求 0 blocker/0 major）。
8. 最终 ready freeze（绑定 clean HEAD、runner bytes、signed HAP/profile/cert/ELF 剖面 hash、全部外部输入）。
9. 候选 ID 消费审计 audit-2（严格 host-only）。
10. host-only Python + PowerShell selftests（`HDC_PROCESSES=0`；含 marker 正反例、ELF 剖面断言、fake-HDC 沙箱）。
11. 同一 ready freeze DryRun（`is_evidence=false`、HDC0、integrity empty）。
12. 独立审查 DryRun，复算 freeze SHA-256，确认字节不变。
13. 单次 Live（`PYTHONUNBUFFERED=1`，按证据目录增量与状态文件时间监控，不因暂无终端输出中断，不 retry）。

## 结果处置（预注册）

- **pass**：登记「stock Go 1.25.12 arm64 c-shared 在冻结物理元组 loader 接受 + 最小 runtime 冒烟可用」。不改 E1（Emulator 范围）判定、不开 E8、不开产品实现；作为新 ADJ/T0（N1-Nx + E8 Go 前提处置）的实测输入，Go 路线在该元组上恢复为可讨论选项。
- **blocked（loader 拒绝）**：逐字登记拒绝文本与 errno；同样作为该 T0 输入，强化 native 路线与 E8 Go 前提重构的论证。
- 两种结果均为**终态消费**（`consumed-pass` / `consumed-blocked`），无后继 AUTH；仓外对象逐字节保留不得复用。

## 证据与脱敏

按[证据与脱敏 Schema](evidence-schema.md)登记；EvidenceRoot/RawRoot 仓外受控（建议 `$HOME/harmonyos-signing/netbird-g0/` 层级），raw 保留 ≥90 天；仓内只登记脱敏 projection/判定/hash。target token、UDID、endpoint、签名材料、密码永不入库。证据模板复用 E3 收官格式（`evidence_id`/`campaign_id`/`authorization_id`/tuple/`hdc_execution`/marker 摘要/清理/integrity/verdict/scope_statement/reviewers）。
