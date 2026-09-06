# 开发环境与 HarmonyOS Linux Emulator

最后核验：2026-09-05

本文记录当前 Pod 的开发条件、HarmonyOS 官方 Linux 工具支持、历轮实际安装与启动结果（含已退役的历史双链），以及建议的环境维护方式。
信息按“当前实测、官方资料确认、方案建议、尚未验证”区分；一次成功的现场验证不代表跨 Pod 或长期运行保证。
Command Line Tools 的授权获取、VNC 首次交互、随包 SDK、镜像和依赖下载流程见
[HarmonyOS CLI 登录、工具链与依赖下载](toolchain-bootstrap.md)，本文不重复这些操作边界。
当前工具链基线（Command Line Tools 26.0.0.821，API 26 Release）与冻结判据登记在
[工具链基线](toolchain-baseline.md)，仓库侧只读校验入口为 `scripts/check-toolchain.sh`。
日常恢复、Emulator 启停、HDC 验收和故障取证以
[HarmonyOS 工具链运行手册](toolchain-runbook.md)为准。

## 当前实测：Pod 现场

### 基础系统

- 操作系统为 Debian GNU/Linux 13，架构为 x86_64。
- `HOME` 位于持久化 ZFS 文件系统，适合保存 SDK、缓存、源码和用户级配置。
- 根文件系统位于临时 overlay，Pod 重建后不应依赖其中的手工安装和修改。
- 因此，后续需要长期保留的 HarmonyOS/OpenHarmony 工具和下载缓存应放在 `HOME` 下。

### 已有工具链

当前宿主环境已具备 Node.js、JDK 21、C/C++、Rust、Go 和 Android SDK。
新启动的 zsh 默认使用系统 Node.js（当前为 `/usr/bin/node`，实测 v24.20.0），并通过 `.zshrc` 自动加载 `$HOME/harmonyos/env.sh`；该脚本只向环境注入 HarmonyOS CLI、SDK、HDC 和 Emulator 路径，不替换默认 Node.js。`bin/hvigorw` 和 `bin/ohpm` wrapper 在各自调用进程内使用 26.0.0.821 包随附的 Node.js 24.14.1。bash 或其他非 zsh shell 可手动 `source` 该脚本，同样不会替换其 Node.js。
Rust 已于 2026-09-05 经官方 rustup 安装到 `$HOME/rust` 并实测验证：rustc/cargo 1.98.1、rustup 1.29.1（stable-x86_64-unknown-linux-gnu 默认工具链），并已安装 `aarch64-unknown-linux-ohos` 与 `x86_64-unknown-linux-ohos` 两个 OHOS target。`.zshrc` 对 `$HOME/rust/env.sh` 做条件加载（`[[ -r ... ]] && source`，文件不存在时不报错）；该脚本只设置 `RUSTUP_HOME`/`CARGO_HOME` 并把 `$CARGO_HOME/bin` 幂等前置到 `PATH`，不设置全局 `CC`/`AR`，也不触碰 Node.js——系统 cc/gcc/ar 与系统 Node.js 保持默认。工具链冒烟（API 26 HAP、BoringTun OHOS）均已通过，记录见[工具链 smoke 记录](toolchain-smoke-20260905.md)；版本与路径登记见[工具链基线](toolchain-baseline.md)。
这些基础工具的存在仍不表示版本已经满足未来 NetBird 工程的全部构建约束；项目建立后应由版本锁定文件和持续集成任务检查。

### HarmonyOS 工具现状

2026-09-05 在当前 Pod 实测：

- 唯一工具链为 Command Line Tools 26.0.0.821（Release），安装于 `$HOME/harmonyos/command-line-tools/26.0.0.821`；包内 `version.txt` 实测 `releaseType: release`、hvigor 6.26.4、ohpm 26.0.0.630、`HarmonyOS SDK: HarmonyOS 26.0.0 Release（include Ohos_sdk_public 26.0.0.105 (API Version 26 Release)）`、`apiVersion: 26`；另实测随包 Node.js 24.14.1、HDC 3.2.0f、Emulator 26.0.0.400。顶层 ohpm/hvigorw wrapper 调用时局部选择包内 Node.js。
- `$HOME/harmonyos/command-line-tools/current` 与 `$HOME/harmonyos/emulator-current` 均指向 `26.0.0.821`，仅作交互入口；活跃脚本与冻结判据（含 `$HOME/.init/harmonyos-check.sh`、`scripts/check-toolchain.sh`）使用绝对版本路径。
- 构建与 Emulator 由同一条链路承担，本机只有一个 HDC 3.2.0f；历史双链（稳定 6.1.1.290 + Beta 26.0.0.461）已于 2026-09-05 前退役移除，退役前的包内容与归档指纹见[bootstrap 文档的历史小节](toolchain-bootstrap.md)。
- 已安装镜像仍只有 `HarmonyOS 6.1.1(24)`（software `6.1.0.125`），位于 `$HOME/harmonyos/emulator-images/system-image/HarmonyOS-6.1.1`；没有安装任何 API 26 镜像。
- `$HOME/harmonyos/emulator-instances/` 当前不存在，本机没有任何实例；26.0.0.821 链路下尚未执行过 Emulator 启动或连接。
- 当前没有项目签名材料或真机连接记录；外部 DevEco Studio 也不属于本 Pod 的已验证能力。

### 虚拟化、启动与设备连接

- `/dev/kvm` 对当前 worker 可读写；2026-09-05 健康检查确认 KVM 可写，Android SDK 的 `emulator -accel-check` 以及 HarmonyOS Emulator 日志也曾在历轮确认 KVM 路径可用。
- 历史结果（2026-07-16，旧 Beta 链 Emulator 26.0.0.200 + `netbird_api24_phone` 实例）：使用 KVM 和 `-noWindow` 成功启动，guest 上报 `boot.completed`，HDC 经 `127.0.0.1:10000` 初始显示 `Connected` 并读取到 `const.product.os.dist.name=HarmonyOS`；运行约 25 分钟后 target 仍显示 `Connected`、TCP 和 heartbeat 保持，但 `hdc shell` RPC 开始连续超时，因此首次连接成功从未视为长期稳定性验收。该实例及旧链目录现已移除。
- 该 25 分钟退化现象只在旧链形成过实测；Emulator 26.0.0.400 下是否复现未验证，也没有任何“已修复”结论。
- 当时复测已排除残留 host client，以及 host client/server 使用不同 HDC 版本所致的错配；问题范围集中在 Emulator guest HDC daemon/`express_bridge` 数据面。日志中的 `watchdog_service` 异常是伴随信号，尚无证据证明其与 RPC 超时存在因果关系。
- 旧链 Emulator 的默认 bridge 端口 5555 未能建立 HDC 连接，显式 `-hdcport` 当时只接受 10000-16555；helper 脚本沿用同一端口校验。这些参数在新链上的适用性未复测。
- HDC 3.2.0d/3.2.0e 并存的“稳定/Emulator 不混用”纪律随双链退役失效；当前链路只有一个 HDC 3.2.0f。
- 当前 `/dev/dri` 节点不可由 worker 使用。无窗口启动成功不等于硬件图形或有窗口模式已经可用，图形路径仍需单独验证。

## 官方资料确认：Linux 命令行能力

Huawei 官方命令行构建文档提供不依赖 IDE 界面的应用构建入口，可作为自动化构建和 Pod 内验证的依据：

- [使用命令行构建应用](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides-V13/ide-command-line-building-app-V13)

HarmonyOS Command Line Tools 从 6.1.0 Release 起集成 Emulator。
Emulator 26.0.0 Beta1 起提供 Linux 支持；官方 Linux Emulator 文档列出的关键条件与能力包括：

- Linux 主机要求 Ubuntu 18.04 或更高版本。
- 使用 KVM 提供虚拟化加速。
- 支持 `-noWindow` 无窗口运行方式。
- 提供 gRPC 控制接口，便于命令行和自动化场景使用。

官方说明见：

- [HarmonyOS Linux Emulator](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/ide-commandline-emulator#section15887175165919)

上述信息意味着 Linux 已进入 HarmonyOS 官方 Emulator 支持范围。
项目文档和自动化设计不应再以“Linux 没有 HarmonyOS 模拟器”为前提。
但官方列出的宿主发行版是 Ubuntu，不能据此直接确认 Debian 13 与下载包兼容。

## 持久化边界

建议把可恢复成本较高或需要跨 Pod 保留的内容放入 `HOME`：

- HarmonyOS/OpenHarmony SDK 与命令行工具。
- Emulator 程序、系统镜像和设备配置。
- Gradle、ohpm、npm、Go、Rust 等构建缓存。
- 签名工具和仅用于开发的本地签名配置。
- 源码工作区及构建诊断日志。

临时 overlay 只适合以下内容：

- 可由脚本重新生成的中间文件。
- 单次测试产生的临时目录和短期日志。
- 不需要跨 Pod 保留的系统级软链接或探测结果。

任何签名私钥都不应提交到仓库。
后续若接入持续集成，应由密钥管理机制在任务运行时注入签名材料。

## 当前实测：持久目录布局

当前 HOME 内目录布局（2026-09-05 现场核验）：

```text
$HOME/
  .init/harmonyos-check.sh
  rust/
    env.sh
    cargo/
    rustup/
  harmonyos/
    env.sh
    bin/
      emulator-start
      emulator-connect
      emulator-stop
    command-line-tools/
      26.0.0.821/
      current -> 26.0.0.821/
    emulator-current -> command-line-tools/26.0.0.821/
    emulator-images/
      system-image/
        HarmonyOS-6.1.1/
```

`emulator-instances/` 当前不存在——本机没有实例，需 Emulator 验收时先按 bootstrap 文档完成镜像/实例准备。构建与 Emulator 由同一条 26.0.0.821 链路承担；`current` 与 `emulator-current` 都只作交互入口，脚本与判据一律使用绝对版本路径。版本目录和镜像位于持久化 `HOME`；根 overlay 仍不能作为 Pod 重建后的持久保证。

## 当前实测：环境恢复入口

以下入口的日常执行顺序、验收判据和故障处置以
[HarmonyOS 工具链运行手册](toolchain-runbook.md)为准。
当前 HOME 提供以下入口：

```bash
source "$HOME/harmonyos/env.sh"
source "$HOME/rust/env.sh"
"$HOME/.init/harmonyos-check.sh"
tmux new-session -d -s harmonyos-emulator-run "$HOME/harmonyos/bin/emulator-start"
"$HOME/harmonyos/bin/emulator-connect"
"$HOME/harmonyos/bin/emulator-stop"
```

`env.sh` 由 `.zshrc` 在新 zsh 中自动加载，只注入 HarmonyOS CLI、SDK、HDC 和 Emulator 路径；zsh 默认继续使用系统 Node.js（实测 v24.20.0）。bash 或其他非 zsh shell 可手动 `source`，同样不会替换其 Node.js。`$HOME/rust/env.sh` 亦由 `.zshrc` 条件加载（手动 `source` 亦可，可重复执行），只设置 `RUSTUP_HOME`/`CARGO_HOME` 并前置 `$CARGO_HOME/bin`，不设置 `CC`/`AR`、不影响默认 Node.js。`bin/hvigorw`/`bin/ohpm` wrapper 调用时局部使用 26.0.0.821 随包 Node.js 24.14.1。环境脚本不下载或启动 Emulator。`tmux` 命令通过启动脚本恢复后台实例，连接和停止脚本分别处理 HDC 连接与实例退出，默认实例名为 `netbird_api24_phone`、HDC 端口为 10000；当前没有实例，启动 helper 在实例 `.ini` 重建前会失败，属预期。
`harmonyos-check.sh` 按绝对路径检查 26.0.0.821 版本目录、两个软链接、KVM、可执行文件、动态库和 HDC/Node 版本（预期 3.2.0f / v24.14.1），不下载工具、不安装系统包，也不自动启动 Emulator；仓库侧只读复核入口为 `scripts/check-toolchain.sh`（固定绝对路径与版本判据），判据登记见[工具链基线](toolchain-baseline.md)。

历轮验收（2026-07-16 首轮、2026-08 复测轮）结束后均已通过停止脚本正常停止实例；实例本体后来随双链退役移除，镜像保留至今，日志按轮次归档在 `docs/evidence/` 所引用的 HOME 路径。这些句子只记录历史操作结果，不表示实例会长期保持某一状态。

2026-09-05 健康检查显示当前系统动态库均可解析，本轮没有执行 `apt` 安装。
Pod 重建后仍应先运行健康检查；即使当前根 overlay 中存在可用系统库，也不能将其当作持久保证。

## 方案建议：环境维护

- 保持新 zsh 自动加载同一个 `env.sh`，并让 bash、其他非 zsh shell 和自动化任务按需显式调用。
- 大型 SDK、Emulator 和镜像只由显式安装命令更新，同时记录版本、大小和校验值；获取与指纹登记到[工具链基线](toolchain-baseline.md)。
- 升级到新版本目录时，先验包再切交互链接，并同步更新活跃脚本与冻结判据中的绝对版本路径；只切链接不算完成升级。
- 检查失败时报告缺失项，不在 shell 启动或 Pod 初始化过程中隐式下载、升级或启动 Emulator。
- 不在 `.bashrc`、`.profile` 或 Pod 启动脚本中执行大型下载。

## 方案建议：DevEco Studio 的角色

Linux 命令行工具和 Emulator 应作为 Pod 内自动化验证的首选路径。
外部 DevEco Studio 可在以下情况作为必要补充：

- 命令行工具没有覆盖某项工程配置或调试流程。
- 需要可视化布局检查、性能分析或设备管理。
- 签名申请、市场发布或特定 SDK 操作只在 IDE 流程中可用。
- Linux 下载包或 Emulator 在 Debian 13 上无法稳定运行。

外部 DevEco Studio 是补充环境，不应替代仓库内可复现的命令行构建说明。

## 尚未验证

工具链安装、镜像安装、KVM 无窗口首次启动、guest 完成启动、HDC 10000 初始连接及 HarmonyOS 参数读取已在旧链（2026-07-16 起）验证完成；这些结果不自动延伸到 26.0.0.821 链路。长期 HDC shell 以及安装、调试稳定性从未视为验收通过；仍需验证：

1. Emulator 26.0.0.400 与既有 `HarmonyOS 6.1.1`(API 24) 镜像的兼容性；API 26 镜像的获取、许可处理、安装与实例创建。
2. 26.0.0.821 链路下的首次 Emulator 启动、HDC 连接与 guest 参数读取闭环。
3. `/dev/dri` 可用性、硬件图形加速和有窗口图形模式。
4. Emulator gRPC 的端口配置、认证、生命周期控制和并发限制。
5. HDC 数据面的可复现时长测试、完整日志归档和上游 Emulator/HDC 版本对比，以确认旧链约 25 分钟后出现的 shell RPC 连续超时是否复现并定位。
6. 最小 HAP 的命令行构建、正式签名前的调试签名、安装、启动、调试和日志采集闭环及其持续稳定性。
7. NetBird Go 核心交叉编译，以及 Emulator 对 Network Kit、VPN Extension、NAPI 和 native fd 的实际支持程度。
8. 至少一台目标真机的 SysCap、VPN、性能和稳定性行为。
9. 正式签名、审核、上架和更新流程。
10. Debian 13 的长期兼容性；官方资料列出的 Linux 宿主是 Ubuntu，历次成功不能扩展为 Debian 获得官方支持。

当前可写成“本 Pod 曾在 2026-07-16 于旧链成功运行 HarmonyOS Linux Emulator，镜像保留至今”；26.0.0.821 链路下尚无任何 Emulator 运行记录。不应把 Emulator 当前进程状态、跨 Pod 可恢复性或未验证的平台能力写成永久事实。
