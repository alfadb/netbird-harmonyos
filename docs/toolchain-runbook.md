# HarmonyOS 工具链运行手册

最后核验：2026-09-05

本文是当前 Pod 中 HarmonyOS 单条稳定构建链路（Command Line Tools 26.0.0.821，API 26 Release）和 Linux Emulator 的日常运行手册，适用于 Pod 重建、新终端恢复、Emulator 启停、HDC 验收、故障取证和版本切换。工具制品来源、许可边界、文件大小和 SHA-256 详见[HarmonyOS CLI 登录、工具链与依赖下载](toolchain-bootstrap.md)；版本基线与冻结判据登记在[工具链基线](toolchain-baseline.md)，仓库侧只读校验入口为 `scripts/check-toolchain.sh`；宿主、持久化和实测能力详见[开发环境与 HarmonyOS Linux Emulator](development-environment.md)。本文不重复完整下载或许可研究。

## 当前固定矩阵

2026-09-05 现场核验：Pod 只保留一条工具链，全部组件来自同一个 26.0.0.821 Release 包；包内 `version.txt` 实测 `releaseType: release`、`apiVersion: 26`、`HarmonyOS SDK: HarmonyOS 26.0.0 Release（include Ohos_sdk_public 26.0.0.105 (API Version 26 Release)）`。

| 角色 | 当前版本 | 随包组件 | 入口 |
| --- | --- | --- | --- |
| 唯一构建与 Emulator 链路 | Command Line Tools 26.0.0.821（Release） | HarmonyOS SDK 26.0.0 Release（含 Ohos_sdk_public 26.0.0.105，API Version 26 Release）、hvigor 6.26.4、ohpm 26.0.0.630、HDC 3.2.0f、随包 Node.js 24.14.1（wrapper 局部使用）、Emulator 26.0.0.400 | 版本目录 `$HOME/harmonyos/command-line-tools/26.0.0.821`；`current` 与 `emulator-current` 均指向该目录，仅作交互入口 |
| Emulator 镜像 | `HarmonyOS 6.1.1(24)`，software `6.1.0.125` | 2026-07-16 安装的唯一镜像 | `$HOME/harmonyos/emulator-images/system-image/HarmonyOS-6.1.1` |
| Emulator 实例 | 无 | `emulator-instances/` 当前不存在，本机没有任何实例 | 实例重建前 `emulator-start` 以 `instance not found` 失败 |

路径纪律：

- 活跃脚本、冻结判据和自动化一律使用绝对版本路径 `$HOME/harmonyos/command-line-tools/26.0.0.821`；`$HOME/.init/harmonyos-check.sh` 已按此固定，仓库侧基线判据见[工具链基线](toolchain-baseline.md)。
- `current` 与 `emulator-current` 软链接只服务交互入口和 `env.sh` 的默认 PATH 注入；切换链接不允许改变任何脚本或判据的解析目标。
- 当前链路只有一个 HDC（3.2.0f）：`$HARMONYOS_STABLE_HDC` 与 `$HARMONYOS_EMULATOR_HDC` 解析到同一个二进制，`env.sh` 保留两个变量名只为兼容既有脚本。

### 历史已退役：双链结构（2026-07 至 2026-09 初）

2026-09-05 之前，Pod 采用双链结构：

- 稳定构建链路 Command Line Tools 6.1.1.290：SDK 6.1.1/API 24，随包 Node.js 18.20.1、ohpm 6.1.2.285、hvigorw 6.24.3、HDC 3.2.0d，不含 Emulator，由 `current` 指向。
- Beta Emulator 链路 Command Line Tools 26.0.0.461：随包 Node.js 24.14.1、Emulator 26.0.0.200、HDC 3.2.0e 和完整 API 26 Beta 工具链，由 `emulator-current` 指向。

截至 2026-09-05 现场核验，这两个版本目录已从本机移除，两条软链接改指 26.0.0.821；旧归档的大小与 SHA-256 指纹保留在[bootstrap 文档的历史小节](toolchain-bootstrap.md)。旧链时代的实测记录（镜像首次安装、Emulator 首次启动、约 25 分钟 HDC RPC 退化等）仍按录制日期有效，详见本文相应历史小节和 `docs/evidence/`；引用按旧链撰写的 evidence 时应结合录制日期理解，其中出现的“稳定 HDC 3.2.0d 与 Emulator HDC 3.2.0e 不混用”等纪律只描述当时结构，不适用于当前单链。

## Node.js 隔离原则

- 新 zsh 默认使用系统 Node.js（当前为 `/usr/bin/node`，实测 v24.20.0）。
- `.zshrc` 自动加载 `$HOME/harmonyos/env.sh`；其他 shell 可显式 `source`。
- `env.sh` 注入工具、SDK、HDC、Emulator 路径，但不设置 `NODE_HOME`/`DEVECO_NODE_HOME`，不把随包 Node 加入 `PATH`。
- 构建时只调用 26.0.0.821 版本目录的顶层 `bin/hvigorw` 和 `bin/ohpm`。
- 顶层 wrapper 在自身进程内使用随包 Node.js 24.14.1。
- 不直接调用内部 wrapper 或 Node 可执行文件，也不全局切换系统 Node 或手工导出上述 Node home 变量。

## Pod 重建或新终端恢复

按以下顺序检查；任一步出现 `WARN`、版本漂移或缺失库时，先处理该项，不启动 Emulator。

1. 运行只读健康检查：

```bash
"$HOME/.init/harmonyos-check.sh"
```

脚本按绝对路径检查 `26.0.0.821` 版本目录、两个软链接、KVM、随包 Node.js `v24.14.1`、HDC `3.2.0f` 和动态库。仓库侧可用 `scripts/check-toolchain.sh` 做只读复核（固定绝对路径 26.0.0.821 与版本判据，不覆盖 KVM 与实例状态），判据登记见[工具链基线](toolchain-baseline.md)。

1. 验证登录式 zsh 自动加载和 Node 隔离：

```bash
zsh -lic 'node --version; printf "%s\n" "$HARMONYOS_HOME" "$HARMONYOS_EMULATOR_HOME"'
zsh -lic 'env | grep -E "^(NODE_HOME|DEVECO_NODE_HOME)=" || true'
zsh -lic 'command -v node; command -v hvigorw; command -v ohpm; command -v emulator-connect'
```

预期 Node 为系统 `v24.20.0`，第二条无输出；交互 PATH 中构建 wrapper 解析到 `current/bin`（即 26.0.0.821），连接 helper 解析到 `$HOME/harmonyos/bin`。脚本内调用一律用绝对版本路径，不依赖上述交互解析。

1. 核对软链接、版本目录和镜像：

```bash
readlink -f "$HOME/harmonyos/command-line-tools/current"
readlink -f "$HOME/harmonyos/emulator-current"
test -d "$HOME/harmonyos/command-line-tools/26.0.0.821"
test -d "$HOME/harmonyos/emulator-images/system-image/HarmonyOS-6.1.1"
```

两个链接应都落到 `26.0.0.821`。`emulator-instances/` 当前不存在是预期状态；若 Pod 重建后需要 Emulator 验收，先按 bootstrap 文档完成镜像/实例准备，不要假设旧实例仍在。
HOME 中的版本目录、镜像和日志可持久化；根文件系统是易失 overlay。

1. 核对 KVM 文件描述符可打开：

```bash
ls -l /dev/kvm
exec 9<>/dev/kvm && printf 'KVM fd opened\n' && exec 9>&-
```

仅看到 `/dev/kvm` 路径不够；当前 worker 必须实际成功打开读写 fd。

1. 必要时复核动态库，不修改系统：

```bash
source "$HOME/harmonyos/env.sh"
ldd "$HARMONYOS_BUNDLED_NODE" | grep 'not found' || true
ldd "$HARMONYOS_STABLE_HDC" | grep 'not found' || true
ldd "$HARMONYOS_EMULATOR_HDC" | grep 'not found' || true
ldd "$HARMONYOS_EMULATOR_HOME/emulator/Emulator" | grep 'not found' || true
```

2026-09-05 健康检查中这些二进制的动态库均可解析，但库位于根 overlay，并不随 HOME 持久；重建后缺库应更新 base image，勿在健康检查中隐式执行 `apt`。

## Emulator 启动与连接

前置条件（2026-09-05 当前不满足）：本机只有 `HarmonyOS 6.1.1` 镜像，没有安装任何 API 26 镜像；`emulator-instances/` 不存在，没有任何实例。Emulator 26.0.0.400 能否直接使用既有 `HarmonyOS 6.1.1` 镜像尚未验证（列入 development-environment.md「尚未验证」），`emulator-start` 在实例 `.ini` 存在前会失败。因此以下流程只在镜像与实例准备完成、且明确需要运行验收时执行。不要把启动加入 `.zshrc`、Pod init 或健康检查。

1. 以显式端口 `10000` 在后台 tmux 中启动 helper：

```bash
tmux new-session -d -s harmonyos-emulator-run \
  "HDC_PORT=10000 $HOME/harmonyos/bin/emulator-start"
```

helper 校验端口并要求 `$HARMONYOS_EMULATOR_INSTANCE_ROOT` 下存在实例 `.ini`；缺失即退出，不会自动创建实例。

1. 用 `tmux capture-pane -pt harmonyos-emulator-run` 查看启动输出；不要立即反复重启。

1. 使用 `$HARMONYOS_EMULATOR_HDC` 的连接 helper 显式连接 `127.0.0.1:10000`：

```bash
HDC_PORT=10000 "$HOME/harmonyos/bin/emulator-connect"
```

当前单链下连接 helper 使用的 HDC 为 3.2.0f，与构建用 HDC 相同；旧链“稳定 HDC 与 Emulator HDC 不混用”的纪律随双链退役，不再适用。

1. 执行最小 shell smoke：

```bash
source "$HOME/harmonyos/env.sh"
HDC="$HARMONYOS_EMULATOR_HDC"
TARGET=127.0.0.1:10000
"$HDC" -t "$TARGET" shell echo netbird-hdc-smoke
"$HDC" -t "$TARGET" shell uname -a
"$HDC" -t "$TARGET" shell param get bootevent.boot.completed
"$HDC" -t "$TARGET" shell param get const.product.os.dist.name
```

预期依次得到 smoke 文本、guest 内核信息、`true` 和 `HarmonyOS`。

## 验收判据

一次可接受的 Emulator 验收必须同时满足：

- 镜像与实例前置条件已满足：实例存在并成功启动（当前 Pod 无实例，完成重建后才有可验收对象）。
- 当前用户成功打开 `/dev/kvm` 读写 fd。
- guest 参数 `bootevent.boot.completed` 为 `true`。
- `list targets -v` 显示 `127.0.0.1:10000` 为 `Connected`。
- `hdc shell echo` 返回预期文本。
- `hdc shell uname -a` 返回 guest 信息。
- `const.product.os.dist.name` 返回 `HarmonyOS`。

`boot.completed` 和 target `Connected` 都不是充分条件；TCP、heartbeat 或状态行仍在时，shell RPC 可能已经退化。
旧链（Emulator 26.0.0.200）曾在 2026-07 实测约 25 分钟后出现该退化；新链 26.0.0.400 是否复现未验证。超过该已观察窗口的 30-40 分钟周期探测仍只用于 HDC 退化诊断和工具链维护，不是 E7 或 API 24 x86_64 phone Emulator 总门必过项；E7 使用可靠窗口内的有界 lifecycle/故障短循环。
例如每两分钟执行一次，共约 38 分钟覆盖首末样本：

```bash
source "$HOME/harmonyos/env.sh"
HDC="$HARMONYOS_EMULATOR_HDC"; TARGET=127.0.0.1:10000
for attempt in {1..20}; do
  date -Is
  "$HDC" list targets -v
  timeout 20 "$HDC" -t "$TARGET" shell echo "smoke-$attempt"
  timeout 20 "$HDC" -t "$TARGET" shell uname -a
  timeout 20 "$HDC" -t "$TARGET" shell param get const.product.os.dist.name
  (( attempt == 20 )) || sleep 120
done
```

任一 shell 超时或输出不匹配即判失败；不要用后续仍显示的 `Connected` 覆盖失败结论。

## 已知故障与只读诊断

以下故障结论是 2026-07-16/17 在旧链（Emulator 26.0.0.200 + `HarmonyOS 6.1.1` 镜像 + `netbird_api24_phone` 实例）上的实测记录，保留用于解释同期 evidence；新链下的复现情况未验证，不作为当前链路的既成事实：

- 默认 bridge 端口 `5555` 的 HDC 连接失败。
- 该 Emulator 版本的显式 `-hdcport` 仅接受 `10000-16555`；`10000` 初始连接已成功（helper 沿用同一端口范围）。
- 实测运行约 25 分钟后，target 仍为 `Connected`，但 shell RPC 连续超时。
- 已排除残留 host client 及 host client/server HDC 版本错配；范围集中在 guest HDC daemon/`express_bridge` 数据面。
- `watchdog_service` 异常仅是伴随信号，没有证据证明其导致 HDC RPC 超时，也未形成根因结论。
- 不得把 `Connected` 当作 ready，也不建议在未归档现场前反复盲目重启。

上述实测对应的实例日志路径 `$HOME/harmonyos/emulator-instances/netbird_api24_phone/Log/`（保留 `Emulator.log`、`kernel.log`、`qemu.log`、`crash_server.log`）随实例退役已不存在；若在新链上重建实例，重点保留同样的日志集合。

最小只读诊断命令（target 与实例仅在实例存在时可用）：

```bash
date -Is
ps -u "$USER" -o pid,lstart,etime,args | grep '[E]mulator'
ss -ltnp | grep -E ':(10000|5555)\b' || true
source "$HOME/harmonyos/env.sh"
"$HARMONYOS_EMULATOR_HDC" -v
"$HARMONYOS_EMULATOR_HDC" list targets -v
timeout 20 "$HARMONYOS_EMULATOR_HDC" -t 127.0.0.1:10000 shell echo diagnostic-smoke
rg -n 'express_bridge|watchdog_service|timeout|boot.completed' \
  "$HARMONYOS_EMULATOR_INSTANCE_ROOT/netbird_api24_phone/Log" 2>/dev/null || true
```

先记录首个失败时间、最后成功时间、进程 elapsed time、target 状态和 smoke 结果，再决定是否停止。
诊断期间不要删除实例、清空日志或启动第二个同名实例。

## 正常停止与日志保留

使用停止 helper 请求实例正常退出（仅在实例存在并运行时执行）：

```bash
"$HOME/harmonyos/bin/emulator-stop"
```

随后确认进程退出；tmux 会话若仍存在，再清理已结束的会话容器：

```bash
ps -u "$USER" -o pid,args | grep '[E]mulator' || true
tmux list-sessions 2>/dev/null | grep '^harmonyos-emulator-run:' || true
tmux kill-session -t harmonyos-emulator-run 2>/dev/null || true
```

停止前后保留 `Log/`，多轮对比时复制到 UTC 时间戳 HOME 目录；日志不提交 Git，`kill -9` 仅用于 helper 失败且证据已保留后的升级处置。

## 版本升级与回滚

- 新工具包解压到新的不可变版本目录（未来的下一个版本号目录），不覆盖 `26.0.0.821`。
- 每次重新获取均核验文件名、大小、SHA-256 和解压内容；获取渠道、指纹和基线判据登记到[工具链基线](toolchain-baseline.md)，旧 SHA 不自动适用于新下载。
- 先在新版本目录内核验 `version.txt` 与关键工具版本（hvigor、ohpm、HDC、随包 Node、Emulator），再切换 `current` 或 `emulator-current` 软链接。
- 切换链接不是完成升级：必须同步把活跃脚本与冻结判据中的绝对版本路径更新到新目录（`$HOME/.init/harmonyos-check.sh`、`scripts/check-toolchain.sh`、[工具链基线](toolchain-baseline.md)的登记值），然后重新运行健康检查和完整验收。
- 26.0.0.821 同时承担构建、ohpm、Hvigor、HDC 和 Emulator；升级后按本文全部小节重新验证，镜像、实例格式可能随 Emulator 变化，升级前保留旧版本、实例元数据和日志，不在原实例上做不可逆迁移。
- 回滚只需把对应软链接和脚本绝对路径恢复到已验证旧目录，然后重新运行健康检查和完整验收。
- 新许可、来源、SHA 和内容核验遵循 bootstrap 文档，运行步骤按本文重新验证。

## 每次验收记录

记录以下非敏感证据，并标注开始、结束时间和操作者环境：

- Pod/base image 标识、内核、架构和核验日期。
- 两个软链接的解析目标（均应为 `26.0.0.821`）及固定工具版本输出（`hdc -v`、`node --version`、`ohpm -v`、hvigor 版本、`Emulator --version`）。
- shell 默认 Node 版本（系统 Node）、`command -v node`，以及两个 Node home 变量未设置的结果。
- 镜像 `HarmonyOS 6.1.1(24)`、software `6.1.0.125`；实例状态（当前为无实例，若已重建则记录实例名和路径）。
- `/dev/kvm` 权限及实际 fd 打开结果。
- `ldd` 缺失库检查结果。
- 启动命令、HDC 端口、Emulator PID、启动时间和运行时长。
- HDC 版本、target 列表及 `boot.completed` 输出。
- shell echo、`uname -a` 和 HarmonyOS 参数输出。
- 30-40 分钟探测的逐次时间、成功/失败和首个失败点。
- 停止命令结果、进程退出确认及保留日志路径。
- 若失败，记录观察与排除项；不要把推测写成根因。

证据中不得包含账号、Cookie、token、临时下载 URL、签名私钥、证书口令或其他凭据。
