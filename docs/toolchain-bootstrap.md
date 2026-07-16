# HarmonyOS CLI 登录、工具链与依赖下载

最后核验：2026-07-16

本文记录 HarmonyOS Command Line Tools 首次获取、SDK 与 Emulator 管理、
公开依赖下载及后续自动化的边界。
本文不记录账号、Cookie、令牌、签名私钥或可复用的临时下载地址。
结论按“官方资料确认、当前实测、推荐流程、尚未验证”区分。
完成 bootstrap 后，日常恢复、Emulator 生命周期、HDC 验收和故障取证以
[HarmonyOS 工具链运行手册](toolchain-runbook.md)为准。

## 官方资料确认

### Command Line Tools 主包获取

HarmonyOS Command Line Tools 的官方入口包括使用说明和下载中心：

- [Command Line Tools 获取与安装说明](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/ide-commandline-get)
- [Command Line Tools for HarmonyOS 下载中心](https://developer.huawei.com/consumer/cn/download/command-line-tools-for-hmos)

主包首次获取需要由有权使用相应华为开发者账号的人员完成。
实际流程应在华为下载中心网页中登录，并由该人员阅读和处理页面展示的协议、
许可或其他交互要求，然后下载与目标宿主平台匹配的工具包。

截至最后核验日期，官方公开资料没有给出可据以实现主包首次获取的以下接口：

- CLI `login` 命令。
- device-code 登录流程。
- service account 或机器账号流程。
- 用于下载中心的长期访问 token。
- Cookie 注入规范。
- 保证稳定的主包二进制下载 API。

这表示项目自动化不能把上述机制当作公开、受支持的接口。
这不等于断言华为内部不存在其他接口，也不应据此探测或依赖未公开接口。
不应捕获浏览器 Cookie、会话信息或临时下载 URL 来绕过网页交互。

### SDK 随工具包交付

当前官方 Command Line Tools 文档说明，HarmonyOS SDK 已嵌入工具包的 `sdk`
目录，无需再通过 `sdkmgr` 额外下载 SDK。
升级 SDK 的受支持路径是升级 Command Line Tools 工具包，并检查新包所带的 SDK。

旧版本资料、历史文章或脚本中出现的 `sdkmgr`，不作为当前自动化设计依据。
若未来官方文档重新提供独立 SDK 管理机制，应以届时版本文档和许可条款为准，
并在迁移前重新核验命令、目录结构和兼容性。

- [HarmonyOS tools overview](https://developer.huawei.com/consumer/en/doc/harmonyos-guides/ide-tools-overview)

### Emulator 与镜像

- [HarmonyOS Linux Emulator](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/ide-commandline-emulator)
- [Emulator command-line tool](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/ide-emulator-command-line)

当前 CLI 提供下列镜像与配置相关入口：

- `Emulator -imageList`：列出可用或已知镜像。
- `Emulator -install`：安装镜像。
- `Emulator -uninstall`：卸载镜像。
- `Emulator -config`：配置默认实例路径、镜像路径或代理等。
- `Emulator -license accept`：接受命令行展示的相关许可。

按官方文档，工具位于带 Emulator 的 Command Line Tools 包中。规划脚本时应通过
`HOME` 下的绝对路径调用工具，并让镜像、配置和缓存留在持久目录中。
官方参数形态如下；路径变量使用本项目当前独立的 Emulator 链路，不能改成不含 Emulator 的稳定 `current`：

```bash
EMULATOR_ROOT="$HOME/harmonyos/emulator-current"
EMULATOR="$EMULATOR_ROOT/bin/Emulator"
IMAGE_ROOT="$HOME/harmonyos/emulator-images"
INSTANCE_ROOT="$HOME/harmonyos/emulator-instances"

"$EMULATOR" -imageList -deviceType phone -downloaded false
"$EMULATOR" -license accept
"$EMULATOR" -install -deviceType phone -osVersion "HarmonyOS 6.0.1(21)" -imageRoot "$IMAGE_ROOT"
"$EMULATOR" -uninstall -deviceType phone -osVersion "HarmonyOS 6.0.1(21)" -imageRoot "$IMAGE_ROOT" -force
"$EMULATOR" -config -instancePath "$INSTANCE_ROOT" -imageRoot "$IMAGE_ROOT"
```

`HarmonyOS 6.0.1(21)` 仅为官方文档示例的 OS 版本字符串。实际脚本必须从当前
`-imageList` 输出选择版本并固定，不能把示例字符串或尖括号占位符原样用于执行。
某些要求账号的 Emulator 镜像不能从 CLI 启动，需要在 DevEco Studio 中使用
已登录账号完成相应流程。
因此，“CLI 能列出或管理镜像”不能推导为“所有镜像都可无人值守启动”。

### ohpm 公开依赖与私仓

官方 ohpm 安装说明见：

- [使用 ohpm 安装依赖](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/ide-ohpm-install)

对于官方公开 registry 中的公开包，`ohpm install` 可匿名读取依赖；
`ohpm install --all` 可安装工程中声明的全部依赖，不需要华为下载中心登录态；
是否允许匿名读取仍应以目标 registry、包可见性和当时服务策略为准。

私有 registry 或私有包应使用该仓库单独签发的 Access Token，
并通过用户级配置、环境变量或 CI 密钥注入机制提供。
私仓 token 不应写入仓库、锁文件、构建日志或可归档的诊断输出。
华为下载中心网页登录状态不能替代私仓 Access Token。

### Hvigor 与插件

- [Hvigor command-line tool](https://developer.huawei.com/consumer/en/doc/harmonyos-guides/ide-hvigor-commandline)

工程通常通过 `hvigorw` 包装脚本运行构建。
包装脚本按工程声明，通过 npm 或 pnpm 获取 Hvigor 及相关 `@ohos` 插件。
这些依赖使用 Node.js 包管理器的 registry 和凭据配置，
与 ohpm 的 registry、认证配置和 Access Token 相互独立。

自动化中应分别配置 npm/pnpm 与 ohpm，不能假设一个系统的登录或 token 会被
另一个系统继承。公开依赖可按公开 registry 规则匿名读取；私有 Node.js 包则应
使用对应 registry 独立签发的最小权限凭据。

### Linux 系统依赖

Command Line Tools、Emulator 或 native 构建所需的 Linux 动态库和系统工具，
应由 `apt` 或预先构建的基础镜像安装，不属于 Huawei CLI 的职责。
系统包版本必须结合官方宿主要求和实际二进制依赖核验。

Huawei 工具包、随包 SDK、Emulator 镜像和用户级构建缓存适合放在 `HOME`；
宿主系统库则应固化在 Pod 基础镜像中。
不要用 Huawei CLI 代替操作系统包管理器，也不要假设解压工具包会补齐系统库。

## 当前实测

### 官方制品与本地指纹

以下两个归档均由授权人员通过官方入口取得；“官方”描述来源渠道和版本性质，文件大小与 SHA-256 是本次落盘后的本地实测值，不表述为华为公开校验值：

| 渠道 | 文件 | 大小（bytes） | 本地 SHA-256 | 安装目录 |
| --- | --- | ---: | --- | --- |
| 官方稳定包 | `commandline-tools-linux-x64-6.1.1.290.zip` | 2141528295 | `292a86fe0cdd28088d92be789649f2950dca915540e1d1261532edd6c7eb424b` | `$HOME/harmonyos/command-line-tools/6.1.1.290` |
| 官方 Beta 包 | `commandline-tools-linux-x64-26.0.0.461.zip` | 2371939057 | `b046da0a1a06fe13b3d82d198738abc54788d79c6c9d07c36617ec7fc31a3b3b` | `$HOME/harmonyos/command-line-tools/26.0.0.461` |

稳定链路 `$HOME/harmonyos/command-line-tools/current` 指向 `6.1.1.290`。
该归档实测包含 Node.js 18.20.1、ohpm 6.1.2.285、hvigorw 6.24.3、HDC 3.2.0d，以及 HarmonyOS/OpenHarmony SDK 6.1.1/API 24，但不含 Emulator。

Beta 包作为 Emulator 链路独立安装，`$HOME/harmonyos/emulator-current` 指向 `26.0.0.461`。
该归档实测包含 Node.js 24.14.1、Emulator 26.0.0.200、HDC 3.2.0e 和完整 API 26 Beta 工具链，不替代稳定构建链路。

### 网页授权与协议处理

2026-07-16，用户已在官方网页完成人工 Beta 试用协议处理，并明确阅读同意 Emulator 展示的 SDK License Agreement 和 Software License Agreement。
该记录只描述本次授权人员完成的交互，不代表可跳过未来版本、镜像或账号再次展示的协议，也不扩大缓存、复制、团队使用或 CI 分发权利。
文档未保存账号、Cookie、令牌、临时下载 URL 或协议文本副本。

### 持久化和系统边界

当前 Pod 的 `HOME` 位于持久存储，可用于保存人工下载的主包、解压后的工具、
随包 SDK、Emulator 镜像、设备配置和依赖缓存。
根文件系统是易失的 overlay；在运行中手工执行 `apt` 的结果可能随 Pod 重建丢失。
本轮对 Node、两套 HDC 和 Emulator 二进制的动态库检查均可解析，没有执行 `apt` 安装。
Pod 重建后仍须重新运行健康检查；需要长期存在的额外系统库应固化到基础镜像，而不是依赖当前 overlay。

建议的持久路径均以 `$HOME` 为根，例如：

```text
$HOME/downloads/harmonyos/
$HOME/harmonyos/command-line-tools/
$HOME/harmonyos/emulator-current
$HOME/harmonyos/emulator-images/
$HOME/harmonyos/emulator-instances/
$HOME/.cache/harmonyos/
$HOME/.ohpm/
$HOME/.npm/
```

实际目录应以工具包结构和各工具的受支持配置项为准；
仓库不应承载主包、SDK、镜像、依赖缓存或任何凭据。

### Emulator 镜像与启动结果

2026-07-16 当前 Pod 实测：

- 已把 `HarmonyOS 6.1.1(24)`、software `6.1.0.125` 镜像安装到 `$HOME/harmonyos/emulator-images`。
- 已在 `$HOME/harmonyos/emulator-instances` 建立 `netbird_api24_phone` 实例，并使用 KVM、`-noWindow` 成功启动。
- guest 已上报 `boot.completed`；Emulator HDC 3.2.0e 经 `127.0.0.1:10000` 初始显示 `Connected`，并成功读取设备参数 `const.product.os.dist.name=HarmonyOS`。
- 运行约 25 分钟后，HDC target 仍显示 `Connected`，TCP 连接和 heartbeat 仍保持，但 `hdc shell` RPC 连续超时。长期 HDC shell 以及安装、调试稳定性因此不视为验收通过。
- 已排除残留 host client 和 host client/server 版本错配；当前问题范围集中到 Emulator guest HDC daemon/`express_bridge` 数据面。同期 `watchdog_service` 异常仅作为伴随信号记录，尚未证明与 RPC 超时存在因果关系。
- 默认 bridge 端口 5555 未能连接。显式 `-hdcport` 在当前版本只接受 10000-16555，10000 实测成功。
- 稳定 HDC 3.2.0d 与 Emulator HDC 3.2.0e 不混用；连接 Emulator 时使用 Beta 包随附的 HDC。

这是一次有日期的首次启动和初始连接成功验证，不表示长期数据面稳定，也不替代 Pod 重建后的健康检查。本轮验收后已正常停止实例，镜像、实例配置和日志保留；这只记录本轮操作结果，不把停止状态写成长期事实。

### HOME 恢复入口

```bash
source "$HOME/harmonyos/env.sh"
tmux new-session -d -s harmonyos-emulator-run "$HOME/harmonyos/bin/emulator-start"
"$HOME/harmonyos/bin/emulator-connect"
"$HOME/harmonyos/bin/emulator-stop"
"$HOME/.init/harmonyos-check.sh"
```

新启动的 zsh 默认由 Volta 提供 Node.js 24.15.0，并通过 `.zshrc` 自动加载 `env.sh`；自动加载只注入 HarmonyOS CLI、SDK、稳定 HDC 和 Emulator 路径，不替换默认 Node.js。稳定 `bin/hvigorw`/`bin/ohpm` wrapper 调用时局部使用随包 Node.js 18.20.1，Beta wrapper 局部使用随包 Node.js 24.14.1。bash 或其他非 zsh shell 可手动 `source` 该脚本，同样不会替换其 Node.js。加载环境脚本不会下载或启动 Emulator。
恢复时使用上面的 `tmux` 命令后台启动实例，再显式运行连接或停止命令；健康检查不下载、不升级、不安装系统包，也不自动启动 Emulator。

### VNC 交互入口

当前 Pod 提供 VNC Server。
授权人员可通过 SSH 端口转发访问该 VNC 会话，在 Pod 内的图形浏览器中完成
首次网页登录、协议处理和主包下载。
这样下载的文件可以直接保存到持久化 `HOME`，避免在个人电脑和 Pod 之间转存。

VNC 只解决图形交互入口和文件落盘位置，不是 CI 认证方式。
不应让无人值守任务接管 VNC 会话、保存浏览器密码、导出 Cookie，
或把已登录浏览器配置作为构建前提。
SSH 与 VNC 的访问控制、会话隔离和审计仍由运行环境管理方负责。

## 推荐流程

本节负责制品准备与版本安装；安装完成后的日常操作以
[HarmonyOS 工具链运行手册](toolchain-runbook.md)为准。
以下流程是后续重装、升级或 CI 制品准备的建议。本轮人工获取、协议处理、指纹记录、双版本安装、镜像安装和启动验证已经完成，不应继续列为当前缺口。

### 阶段一：人工 bootstrap

由授权人员执行一次受控的人工引导：

1. 通过 SSH 转发连接当前 Pod 的 VNC Server。
2. 在 Pod 图形浏览器中打开官方 Command Line Tools 下载中心。
3. 使用获授权的华为开发者账号登录，阅读并处理当次展示的协议与许可。
4. 选择与 Linux 宿主架构、目标版本相符的主包，下载到 `$HOME` 持久目录。
5. 记录官方页面显示的版本、文件名、下载日期和适用平台。
6. 对落盘文件计算并记录 SHA-256；如官方提供校验值，同时核对官方值。
7. 按许可和组织政策判断是否允许上传到访问受控的内部制品库。
8. 若不允许内部再分发，则保留人工获取步骤，并只自动化后续本地处理。

阶段一不应采集 Cookie、临时 URL、浏览器 profile 或账号口令。
内部制品库也不能改变上游许可；上传前必须确认缓存、复制和团队使用权限。
SHA-256 用于确认文件一致性，不代表来源授权或许可审查已经完成。

### 阶段二：无人值守执行

在主包已经通过合规渠道进入受控存储后，自动化可执行：

1. 从持久目录或许可允许的内部制品库取得固定版本制品。
2. 校验文件名、版本、大小和预先登记的 SHA-256。
3. 解压到 `$HOME` 下的版本目录；稳定构建链路和 Emulator 链路分别切换 `current` 与 `emulator-current`。
4. 检查随包 `sdk` 目录及关键工具版本，不调用旧 `sdkmgr` 下载 SDK。
5. 在许可允许且命令支持的范围内处理 Emulator 展示的许可；网页或命令再次展示新协议时转由授权人员确认。
6. 使用 `Emulator -imageList` 检查镜像，再按固定标识安装或选择镜像。
7. 分别配置 ohpm 与 npm/pnpm 的公开 registry；私仓凭据按需单独注入。
8. 运行 `ohpm install --all`、`hvigorw` 依赖准备和确定的构建任务。
9. 输出版本、校验和、依赖锁定状态及构建结果，但过滤凭据和敏感路径。

自动化应在版本或校验不一致时失败，不应静默升级工具包、SDK 或镜像。
镜像许可、账号限制或 DevEco Studio 前置条件无法满足时，应明确停止并转人工处理。
系统库检查可以无人值守，但缺失系统库应通过更新基础镜像修复。

### 凭据、签名与账号

华为开发者账号、私仓 Access Token、npm/pnpm token、签名证书和私钥是不同资产。
应分别定义所有者、用途、有效期、最小权限、轮换方式和注入路径。

任何凭据、恢复码、Cookie、签名私钥或明文口令都不得写入 Git 仓库。
签名材料不得打包进通用工具链制品，也不应放入可由所有构建任务读取的缓存。
需要签名时，应由受控密钥服务或 CI secret 在最小作用域内临时注入，
并确保命令回显、错误日志和构建归档不会泄露其内容。

账号授权下载不自动授予应用签名、调试设备、发布或市场上架权限。
这些流程需要按实际团队角色和华为侧权限单独核验。

## 尚未验证

以下事项仍未在当前 Pod 中形成完整证据，不应写成已具备能力：

- `/dev/dri`、硬件图形加速及有窗口模式；本轮仅验证 KVM `-noWindow`。
- Emulator gRPC 的端口、认证、生命周期控制和并发限制。
- HDC 数据面的可复现时长测试、完整日志归档和上游 Emulator/HDC 版本对比；需用这些证据定位约 25 分钟后出现的 shell RPC 连续超时。
- 最小 HAP 的构建、调试签名、安装、启动、调试和日志采集闭环及其持续稳定性。
- NetBird Go 核心交叉编译，以及 NAPI、native fd、Network Kit 和 VPN Extension 的模拟器行为。
- 真机连接、设备 SysCap、VPN 行为、性能和稳定性。
- 正式签名、审核、上架、更新流程及相关角色划分。
- 官方 ohpm registry 与工程锁定版本下 npm/pnpm、hvigorw 的代理、限速、离线缓存和可复现行为。
- 内部制品库保存主包、SDK 或镜像是否满足对应许可与组织合规要求。
- Debian 13 的长期兼容性和官方支持边界；官方 Linux 宿主要求以 Ubuntu 为基线，本次成功不构成 Debian 官方支持。

后续验证仍应记录工具版本、官方来源 URL、执行日期、非敏感命令输出和 SHA-256。
涉及网页授权或许可变化时，应重新由授权人员确认，不能仅沿用本次记录。
