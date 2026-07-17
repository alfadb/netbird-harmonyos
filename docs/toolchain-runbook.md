# HarmonyOS 工具链运行手册

最后核验：2026-07-17

本文是当前 Pod 中 HarmonyOS 稳定构建工具链和 Beta Linux Emulator 的日常运行手册，适用于 Pod 重建、新终端恢复、Emulator 启停、HDC 验收、故障取证和版本切换。工具制品来源、许可边界、文件大小和 SHA-256 详见[HarmonyOS CLI 登录、工具链与依赖下载](toolchain-bootstrap.md)；宿主、持久化和实测能力详见[开发环境与 HarmonyOS Linux Emulator](development-environment.md)。本文不重复完整下载或许可研究。

## 固定矩阵

| 角色 | 固定版本 | 随包组件 | 当前入口 |
| --- | --- | --- | --- |
| 稳定构建链路 | Command Line Tools 6.1.1.290，SDK 6.1.1/API 24 | Node.js 18.20.1（wrapper 局部使用）、ohpm 6.1.2.285、hvigorw 6.24.3、HDC 3.2.0d；不含 Emulator | `$HOME/harmonyos/command-line-tools/6.1.1.290`，由 `current` 链接 |
| Beta Emulator 链路 | Command Line Tools 26.0.0.461 | Node.js 24.14.1（wrapper 局部使用）、Emulator 26.0.0.200、HDC 3.2.0e | `$HOME/harmonyos/command-line-tools/26.0.0.461`，由 `emulator-current` 链接 |
| Emulator 镜像 | `HarmonyOS 6.1.1(24)` | software `6.1.0.125` | `$HOME/harmonyos/emulator-images/system-image/HarmonyOS-6.1.1` |
| Emulator 实例 | `netbird_api24_phone` | API 24 phone 实例 | `$HOME/harmonyos/emulator-instances/netbird_api24_phone` |

## Node.js 隔离原则

- 新 zsh 默认使用 Volta Node.js 24.15.0。
- `.zshrc` 自动加载 `$HOME/harmonyos/env.sh`；其他 shell 可显式 `source`。
- `env.sh` 注入工具、SDK、HDC、Emulator 路径，但不设置 `NODE_HOME`/`DEVECO_NODE_HOME`，不把随包 Node 加入 `PATH`。
- 构建时只调用对应版本目录的顶层 `bin/hvigorw` 和 `bin/ohpm`。
- 稳定顶层 wrapper 在自身进程内使用随包 Node.js 18.20.1；Beta wrapper 局部使用 24.14.1。
- 不直接调用内部 wrapper 或 Node 可执行文件，也不全局切换 Volta Node 或手工导出上述 Node home 变量。

## Pod 重建或新终端恢复

按以下顺序检查；任一步出现 `WARN`、版本漂移或缺失库时，先处理该项，不启动 Emulator。

1. 运行只读健康检查：

```bash
"$HOME/.init/harmonyos-check.sh"
```

1. 验证登录式 zsh 自动加载和 Node 隔离：

```bash
zsh -lic 'node --version; printf "%s\n" "$HARMONYOS_HOME" "$HARMONYOS_EMULATOR_HOME"'
zsh -lic 'env | grep -E "^(NODE_HOME|DEVECO_NODE_HOME)=" || true'
zsh -lic 'command -v node; command -v hvigorw; command -v ohpm; command -v emulator-connect'
```

预期 Node 为 `v24.15.0`，第二条无输出；构建 wrapper 解析到稳定 `current/bin`，连接 helper 解析到 `$HOME/harmonyos/bin`。

1. 核对软链接和持久目录：

```bash
readlink -f "$HOME/harmonyos/command-line-tools/current"
readlink -f "$HOME/harmonyos/emulator-current"
test -d "$HOME/harmonyos/emulator-images/system-image/HarmonyOS-6.1.1"
test -f "$HOME/harmonyos/emulator-instances/netbird_api24_phone.ini"
```

两个链接应分别落到 `6.1.1.290` 和 `26.0.0.461`。
HOME 中的版本、镜像、实例和日志可持久化；根文件系统是易失 overlay。

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

当前 base image 可解析这些系统库，但库位于根 overlay，并不随 HOME 持久；重建后缺库应更新 base image，勿在健康检查中隐式执行 `apt`。

## Emulator 启动与连接

以下命令会启动 Emulator，只在明确需要运行验收时执行。不要把启动加入 `.zshrc`、Pod init 或健康检查。

1. 以显式端口 `10000` 在后台 tmux 中启动 helper：

```bash
tmux new-session -d -s harmonyos-emulator-run \
  "HDC_PORT=10000 $HOME/harmonyos/bin/emulator-start"
```

1. 用 `tmux capture-pane -pt harmonyos-emulator-run` 查看启动输出；不要立即反复重启。

1. 使用 Beta Emulator HDC helper 显式连接 `127.0.0.1:10000`：

```bash
HDC_PORT=10000 "$HOME/harmonyos/bin/emulator-connect"
```

连接 helper 固定使用 Beta HDC 3.2.0e；稳定 HDC 3.2.0d 只服务稳定 SDK，不得用于 Beta Emulator。

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

- 当前用户成功打开 `/dev/kvm` 读写 fd。
- guest 参数 `bootevent.boot.completed` 为 `true`。
- `list targets -v` 显示 `127.0.0.1:10000` 为 `Connected`。
- `hdc shell echo` 返回预期文本。
- `hdc shell uname -a` 返回 guest 信息。
- `const.product.os.dist.name` 返回 `HarmonyOS`。

`boot.completed` 和 target `Connected` 都不是充分条件；TCP、heartbeat 或状态行仍在时，shell RPC 可能已经退化。
超过已观察到的约 25 分钟窗口的 30-40 分钟周期探测只用于 HDC 退化诊断和工具链维护，不是 E7 或 API 24 x86_64 phone Emulator 总门必过项；E7 使用可靠窗口内的有界 lifecycle/故障短循环。
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

- 默认 bridge 端口 `5555` 的 HDC 连接失败。
- 当前 Emulator 的显式 `-hdcport` 仅接受 `10000-16555`；`10000` 初始连接已成功。
- 实测运行约 25 分钟后，target 仍为 `Connected`，但 shell RPC 连续超时。
- 已排除残留 host client 及 host client/server HDC 版本错配；范围集中在 guest HDC daemon/`express_bridge` 数据面。
- `watchdog_service` 异常仅是伴随信号，没有证据证明其导致 HDC RPC 超时，也未形成根因结论。
- 不得把 `Connected` 当作 ready，也不建议在未归档现场前反复盲目重启。

实例日志位于 `$HOME/harmonyos/emulator-instances/netbird_api24_phone/Log/`，重点保留 `Emulator.log`、`kernel.log`、`qemu.log` 和 `crash_server.log`。

最小只读诊断命令：

```bash
date -Is
ps -u "$USER" -o pid,lstart,etime,args | grep '[E]mulator'
ss -ltnp | grep -E ':(10000|5555)\b' || true
source "$HOME/harmonyos/env.sh"
"$HARMONYOS_EMULATOR_HDC" -v
"$HARMONYOS_EMULATOR_HDC" list targets -v
timeout 20 "$HARMONYOS_EMULATOR_HDC" -t 127.0.0.1:10000 shell echo diagnostic-smoke
rg -n 'express_bridge|watchdog_service|timeout|boot.completed' \
  "$HOME/harmonyos/emulator-instances/netbird_api24_phone/Log"
```

先记录首个失败时间、最后成功时间、进程 elapsed time、target 状态和 smoke 结果，再决定是否停止。
诊断期间不要切换到稳定 HDC、删除实例、清空日志或启动第二个同名实例。

## 正常停止与日志保留

使用停止 helper 请求实例正常退出：

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

- 新工具包解压到新的不可变版本目录，不覆盖 `6.1.1.290` 或 `26.0.0.461`。
- 每次重新获取均核验文件名、大小、SHA-256 和解压内容；旧 SHA 不自动适用于新下载。
- 先验证版本目录，再原子切换 `current` 或 `emulator-current` 软链接。
- stable 继续承担 API 24 构建、ohpm、Hvigor 和稳定 HDC；Beta 继续承担 Emulator 和其 HDC。
- 不因 Beta 包组件更新而把整个默认构建链路切到 Beta。
- 回滚只需把对应软链接恢复到已验证旧目录，然后重新运行健康检查和完整验收。
- 镜像、实例格式可能随 Emulator 变化；升级前保留旧版本、实例元数据和日志，不在原实例上做不可逆迁移。
- 新许可、来源、SHA 和内容核验遵循 bootstrap 文档，运行步骤按本文重新验证。

## 每次验收记录

记录以下非敏感证据，并标注开始、结束时间和操作者环境：

- Pod/base image 标识、内核、架构和核验日期。
- 两个软链接的解析目标及固定工具版本输出。
- shell 默认 Node 版本、`command -v node`，以及两个 Node home 变量未设置的结果。
- 镜像 `HarmonyOS 6.1.1(24)`、software `6.1.0.125`、实例名和路径。
- `/dev/kvm` 权限及实际 fd 打开结果。
- `ldd` 缺失库检查结果。
- 启动命令、HDC 端口、Emulator PID、启动时间和运行时长。
- Beta HDC 版本、target 列表及 `boot.completed` 输出。
- shell echo、`uname -a` 和 HarmonyOS 参数输出。
- 30-40 分钟探测的逐次时间、成功/失败和首个失败点。
- 停止命令结果、进程退出确认及保留日志路径。
- 若失败，记录观察与排除项；不要把推测写成根因。

证据中不得包含账号、Cookie、token、临时下载 URL、签名私钥、证书口令或其他凭据。
