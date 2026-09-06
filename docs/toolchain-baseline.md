# 工具链版本基线（当前活跃开发）

> 登记日期：2026-09-05。本文件是本机当前活跃开发工具链的**唯一版本基线**：
> 在用版本、绝对路径、安装归档与关键二进制校验值、`current` / `emulator-current`
> 解析结果、路径使用规则，以及与已退役链路和历史记录的边界，以本文件为准。
> 全部数值为 2026-09-05 在本机实测（命令见[复检](#复检)），非官方文档转抄。
> 工具链升级时新增不可变版本目录并以新登记替换本文件的"当前基线"内容，
> 历史证据与冻结判据不随之改写（见[与历史记录的边界](#与历史记录的边界)）。

## 基线一览

| 项 | 实测值 | 依据 |
| --- | --- | --- |
| Command Line Tools | `26.0.0.821`（linux-x64，`releaseType: release`） | `/home/worker/harmonyos/command-line-tools/26.0.0.821/version.txt` |
| HarmonyOS SDK | `Ohos_sdk_public 26.0.0.105`，API Version 26 **Release** | `<CLT>/sdk/default/openharmony/*/oh-uni-package.json`：`version 26.0.0.105`、`releaseType Release`、`apiVersion 26` |
| hvigor / hvigorw | `6.26.4` | `<CLT>/hvigor/bin/hvigorw -v` |
| ohpm | `26.0.0.630` | `<CLT>/ohpm/bin/ohpm -v` |
| HDC | `3.2.0f` | `<CLT>/sdk/default/openharmony/toolchains/hdc version` → `Ver: 3.2.0f` |
| Node.js（随包） | `v24.14.1` | `<CLT>/tool/node/bin/node --version` |
| Emulator | `26.0.0.400`（Release） | `<CLT>/emulator/Emulator --version` → `HarmonyOS Emulator :26.0.0.400`；`<CLT>/emulator/sdk-pkg.json`：`version 26.0.0.400`、`releaseType Release` |
| codelinter | `6.0.240` | `version.txt` |
| hstack | `6.1.0` | `version.txt` |
| platformVersion / apiVersion | `26.0.0` / `26` | `version.txt` |
| Rust rustc / cargo | `1.98.1` / `1.98.1`（rustup stable，默认 host 工具链） | `$HOME/rust/cargo/bin/rustc --version`、`cargo --version` |
| rustup | `1.29.1` | `$HOME/rust/cargo/bin/rustup --version` |
| Rust OHOS targets | `aarch64-unknown-linux-ohos`、`x86_64-unknown-linux-ohos` 已安装 | `$HOME/rust/cargo/bin/rustup target list --installed` |

其中 `<CLT>` = `/home/worker/harmonyos/command-line-tools/26.0.0.821`（本文件余下同）。
`version.txt` 中 `HarmonyOS SDK` 一行原文为
`HarmonyOS 26.0.0 Release (include Ohos_sdk_public 26.0.0.105 (API Version 26 Release))`。

## 安装归档校验

`~/Downloads/commandline-tools-linux-x64-26.0.0.821.zip` 本机存在，2026-09-05 实测：

| 项 | 值 |
| --- | --- |
| 绝对路径 | `/home/worker/Downloads/commandline-tools-linux-x64-26.0.0.821.zip` |
| size | `2,347,882,285` 字节（约 2.19 GiB） |
| SHA-256 | `58da7359019e9360a8bb82da0cd1d3b3b26fedc338379f257849f2162e3ac1fc` |

## version.txt 全文

`<CLT>/version.txt`（2026-09-05 原样登记）：

```text
# ======================
# Command Line Tools(linux-x64)
# Version: 26.0.0.821
# ======================

releaseType    : release
hvigor         : 6.26.4
codelinter     : 6.0.240
hstack         : 6.1.0
ohpm           : 26.0.0.630
HarmonyOS SDK  : HarmonyOS 26.0.0 Release (include Ohos_sdk_public 26.0.0.105 (API Version 26 Release))
apiVersion     : 26
platformVersion: 26.0.0
```

## 关键二进制 SHA-256

2026-09-05 `sha256sum` 实测（至少 hdc 为必登项；node 与 Emulator 一并登记）：

| 二进制 | 绝对路径 | SHA-256 |
| --- | --- | --- |
| hdc（基线必登项） | `<CLT>/sdk/default/openharmony/toolchains/hdc` | `90c2554a44855a3c541c24bad5cffd46badb7eb7323c5464f6c0fb7160bdf5d0` |
| 随包 node | `<CLT>/tool/node/bin/node` | `bf9112eb83a827dc9f9c8d19bb7bddcbf00f7b2fedae18fdcaee05406981f20a` |
| Emulator | `<CLT>/emulator/Emulator` | `62e0824afcec13bfdd6786b0447e4ce73f950f7cea1d1aefc26995e12b779721` |

## 链接解析与单链路布局

2026-09-05 实测两条链接当前都解析到同一版本目录，即旧的
stable / Beta 双链路已合并为单一 `26.0.0.821` 链路：

| 链接 | 链接值 | `readlink -f` 解析结果 |
| --- | --- | --- |
| `/home/worker/harmonyos/command-line-tools/current` | `26.0.0.821`（相对） | `/home/worker/harmonyos/command-line-tools/26.0.0.821` |
| `/home/worker/harmonyos/emulator-current` | `/home/worker/harmonyos/command-line-tools/26.0.0.821`（绝对） | `/home/worker/harmonyos/command-line-tools/26.0.0.821` |

交互环境由 `~/harmonyos/env.sh` 定义（经 `current` / `emulator-current` 间接取值）：
`HARMONYOS_HOME` → `<CLT>`，`DEVECO_SDK_HOME` → `<CLT>/sdk`，
`HARMONYOS_EMULATOR_HOME` → `/home/worker/harmonyos/emulator-current`。
`HARMONYOS_STABLE_HDC` 与 `HARMONYOS_EMULATOR_HDC` 当前都落在上表同一个 hdc 二进制。
本机另有镜像根 `~/harmonyos/emulator-images` 与实例根 `~/harmonyos/emulator-instances`
（均为 `env.sh` 约定），镜像与实例内容不属于本基线登记范围。

## 路径使用规则：绝对版本路径 vs `current`

- **脚本、构建配置与判据冻结文本一律使用绝对版本路径**
  `/home/worker/harmonyos/command-line-tools/26.0.0.821`（或 `$HOME/harmonyos/command-line-tools/26.0.0.821`），
  **不使用 `current` / `emulator-current`**。
  理由：脚本与冻结判据需要可复现的固定版本指向，不能随链接切换静默漂移。
- **`current` 与 `emulator-current` 仅作为交互入口**（人工终端、`env.sh` 环境变量），
  其当前指向见上节；任何脚本不得把链接的存在或指向当作依赖或断言依据。
- 升级工具链时：新包解压到新的不可变版本目录，更新链接与本文件登记，
  不就地覆盖既有版本目录（操作细节沿用 [toolchain-runbook](toolchain-runbook.md)）。

## Rust 工具链（2026-09-05 安装并验证）

Rust 经官方 rustup 安装于独立前缀 `$HOME/rust`（即 `/home/worker/rust`），
与 HarmonyOS CLT 相互独立、互不注入：

| 项 | 实测值（2026-09-05） |
| --- | --- |
| 前缀 | `/home/worker/rust`（`RUSTUP_HOME=$HOME/rust/rustup`、`CARGO_HOME=$HOME/rust/cargo`，二进制在 `$CARGO_HOME/bin`） |
| rustc / cargo | `1.98.1` / `1.98.1`（stable-x86_64-unknown-linux-gnu，默认工具链） |
| rustup | `1.29.1` |
| 已装 targets | `aarch64-unknown-linux-ohos`、`x86_64-unknown-linux-ohos`（另有 host `x86_64-unknown-linux-gnu`） |

环境注入只经 `$HOME/rust/env.sh`（POSIX 兼容、幂等，可重复 source）：
该脚本仅设置 `RUSTUP_HOME` / `CARGO_HOME` 两个变量，并把 `$CARGO_HOME/bin` 幂等前置到 `PATH`。加载方式与边界：

- 新 zsh 由 `~/.zshrc` 条件加载：`[[ -r "$HOME/rust/env.sh" ]] && source "$HOME/rust/env.sh"`；
  bash 或其他非 zsh shell 需手动 `source`。
- **不设置全局 `CC` / `AR`**：env.sh 不注入任何编译器变量，host 构建继续使用系统 cc/gcc/ar；
  OHOS 交叉编译所需 clang/llvm 工具位于
  `<CLT>/sdk/default/openharmony/native/llvm/bin/`（按 target 前缀命名），
  只由构建配置显式指定，不经全局环境变量。
- **Node 不受影响**：env.sh 不触碰 Node.js；新 zsh 默认仍是系统 Node.js
  （口径与 [development-environment](development-environment.md) 的 HarmonyOS 环节一致）。
- 工具链冒烟已通过：API 26 HAP smoke 与 BoringTun OHOS smoke 均 PASS，
  见 [toolchain-smoke-20260905](toolchain-smoke-20260905.md)。

## 已退役版本

以下两条旧链路已退役，**本机不存在**（2026-09-05 实测：
`/home/worker/harmonyos/command-line-tools/` 下仅有 `26.0.0.821` 一个版本目录，
`~/Downloads/` 仅有 `26.0.0.821` 一个安装归档，两个旧版本号在本机无目录或归档残留）：

- 稳定链 `6.1.1.290`（SDK 6.1.1 / API 24，Node 18.20.1 / ohpm 6.1.2.285 / hvigorw 6.24.3 / HDC 3.2.0d，不含 Emulator）。
- Beta 链 `26.0.0.461`（Emulator 26.0.0.200 / HDC 3.2.0e，API 26 Beta）。

上述版本号只存在于历史文档记录中，不构成当前基线的一部分；
任何脚本或判据不得引用这两个版本路径。

## 与历史记录的边界

本文件只登记当前基线，不改写、不重跑以下既有记录：

- 历史证据（`docs/evidence/` 各条，含已标记 consumed 的记录）保持原文与原结论。
- 已冻结的判据文本（如 [n1b-gate-plan](n1b-gate-plan.md)、
  [n1b-disc-gate-plan](n1b-disc-gate-plan.md) 的判据正文与其冻结记录）保持原样。
- 已消费的 spike（含 `spikes/` 下各项及对应 evidence）不因本基线重跑或重新解读。
- [toolchain-bootstrap](toolchain-bootstrap.md)、[toolchain-runbook](toolchain-runbook.md)、
  [development-environment](development-environment.md) 中关于 `6.1.1.290` / `26.0.0.461`
  双链路的描述按写作时点理解，作为历史证据保留；与本文件冲突时，
  **当前活跃工具链以本文件为准**。

## 当前开发边界

- **新开发只用 API 26 Release**（本基线的 `26.0.0.821` / SDK `26.0.0.105`）。
  不为已退役的 API 24 / Beta 链路做兼容或回归。
- **Go 工具链与旧（API 24）模拟器不是当前 N1BDISC 的前置**：
  相关历史（`e1-stock-go-*` 等 evidence）已消费归档，不再作为开展 N1BDISC 的条件。
- **Rust 工具链前置已闭合**：rustc/cargo 1.98.1 + rustup 1.29.1 + OHOS 双 target
  已于 2026-09-05 安装并实测验证（见[Rust 工具链](#rust-工具链2026-09-05-安装并验证)节），
  API 26 HAP smoke 与 BoringTun OHOS smoke 均 PASS
  （见 [toolchain-smoke-20260905](toolchain-smoke-20260905.md)）。
  但 BoringTun 工程集成仍未落地：N1BDISC 的正式 vendor/lock/同产物 freeze 尚未开始。
  新 bundle AGC profile 已于 2026-09-05（用户授权会话内）完成：AGC 应用
  NetBird N1BDISC（包名 `cn.alfadb.netbird.n1bdisc`，与冻结值逐字一致，
  APP ID `6917615611100883458`）与调试 Profile「NetBird N1BDISC Debug」
  （证书复用 NetBird E3 Debug，绑定 PHYS-1，失效 2027-08-06，
  `hap-sign-tool verify-profile` PASS），完整登记与注意点见
  [n1b-disc-handoff-20260902](n1b-disc-handoff-20260902.md) 第九节；
  判据体系已冻结（见 n1b 各门计划），本基线登记不改变该工程缺口状态。
- `scripts/check-toolchain.sh`（与本文件并行新增）是本基线的自动复检入口，
  按上表逐项校验版本与校验值，且只使用绝对版本路径。

## 复检

自动复检：

```bash
scripts/check-toolchain.sh
```

手工复检命令（2026-09-05 基线实测所用，预期输出即上表登记值）：

```bash
CLT=/home/worker/harmonyos/command-line-tools/26.0.0.821
cat "$CLT/version.txt"
sha256sum ~/Downloads/commandline-tools-linux-x64-26.0.0.821.zip
stat -c %s ~/Downloads/commandline-tools-linux-x64-26.0.0.821.zip
"$CLT/sdk/default/openharmony/toolchains/hdc" version
"$CLT/hvigor/bin/hvigorw" -v
"$CLT/ohpm/bin/ohpm" -v
"$CLT/tool/node/bin/node" --version
"$CLT/emulator/Emulator" --version
readlink -f /home/worker/harmonyos/command-line-tools/current
readlink -f /home/worker/harmonyos/emulator-current
export RUSTUP_HOME="$HOME/rust/rustup" CARGO_HOME="$HOME/rust/cargo"
"$CARGO_HOME/bin/rustup" --version
"$CARGO_HOME/bin/rustc" --version
"$CARGO_HOME/bin/cargo" --version
"$CARGO_HOME/bin/rustup" target list --installed
```
