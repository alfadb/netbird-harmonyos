# 项目文档

最后核验：2026-08-06

本目录记录 `netbird-harmonyos` 当前阶段的环境调查、平台边界和实施建议。项目仍处于验证阶段；文档会明确区分已经观察到的现场事实、官方资料中的能力、建议方案和尚未完成的验证。

## 文档索引

- [R0 任务章程](r0-charter.md)
  - R0 唯一决策源、当前“进行中/未退出”状态、Emulator 客观可执行项总门和唯一 `E3-PHYS-PREFLIGHT` 例外
  - NetBird v0.74.7 正式基线、v0.74.6 历史证据绑定、功能范围、补丁预算、初始 SLO、责任矩阵和退出 checklist
- [证据与脱敏 Schema](evidence-schema.md)
  - 信息状态、R/E 证据 ID、必填字段、状态枚举、脱敏规则和记录模板
  - E 门双判定、目标元组/哈希一致性、双向不外推、支持矩阵、动态调整、补丁记录和保留期
- [E0 API 24 Emulator 普通应用证据](evidence/e0-api24-emulator-2026-07-17.md)
  - 三次普通 `EntryAbility` 冷启动、生命周期/Node-API HiLog、可见截图、停止、sidecar、卸载与主机清理
  - `record_status: reviewed-pass`、`verdict: pass`；E0 已关闭，E8 仍 `CLOSED`；该历史记录不授权当前物理预检
- [E1 C-only ArkTS/native/fd Emulator 子证据](evidence/e1-c-bridge-api24-emulator-2026-07-17.md)
  - 普通 `EntryAbility` 三个不同 PID，各完成 10 轮同步 buffer、pthread threadsafe callback 与 fd ownership
  - `record_status: reviewed-pass`、`verdict: pass`；只关闭 E1 C-only 子门；loader 负面绑定 v0.74.6，当前R0正式基线（现v0.74.7）尚未重跑且无 E1 pass
- [E2 C 网络 API 24 Emulator 证据](evidence/e2-c-network-api24-emulator-2026-07-17.md)
  - 三个不同普通 `EntryAbility` PID 各完成 10 轮 TCP/UDP loopback、Pod 本机受控 endpoint、DNS、事件/错误和资源恢复验证，并显示可见 E2 PASS 页面
  - `record_status: reviewed-pass`、`verdict: pass`；E2 已关闭；当前R0正式基线（现v0.74.7）的 E1 尚未重跑且无 pass，E8 仍 `CLOSED`；该历史记录不授权当前物理预检
- [E3 VPN Extension API 24 Emulator 证据](evidence/e3-vpn-extension-api24-emulator-2026-07-17.md)
  - A 三个 PID 与新鲜 B 均由正常 UI 触发公开 start；系统授权组件缺失，promise pending 且 Extension 无 `onCreate`
  - 0003/0004 保持 `reviewed-pass/blocked`；授权前置组件缺失使记录的 phone runtime 路径不可继续；E4-E7 因此前置依赖未启动
- [E3 VPN Extension API 24 2in1 Emulator 前置证据](evidence/e3-vpn-extension-api24-2in1-emulator-2026-07-17.md)
  - 官方 `pc_all_x86` image、独立 `netbird_api24_2in1` 实例、MateBook Pro profile、KVM/noWindow 和固定 HDC target `127.0.0.1:10001`
  - `0001` 与 user-0 supplemental `0002` 均为 `reviewed-pass/blocked`；完整 user 100/user 0 lists 分别为 49/7 bundles，三范围 direct vpndialog 与 Settings 注册查询均无组件，A/B HAP 未安装
- [E3 VPN Extension API 24 Tablet Emulator 前置证据](evidence/e3-vpn-extension-api24-tablet-emulator-2026-07-17.md)
  - 官方 `tablet_x86` image、独立 `netbird_api24_tablet` 实例、MatePad Pro 13 profile、KVM/noWindow 和固定 HDC target `127.0.0.1:10002`
  - `reviewed-pass/blocked`；current user 100、user 0 与 default direct query 均未发现 `vpndialog`/`VpnServiceExtAbility`，按停止条件未安装 A/B HAP；blocked 只覆盖 registration-layer 前置边界
- [E3 VPN Extension API 24 Emulator 矩阵审查](evidence/e3-vpn-extension-api24-emulator-matrix-2026-07-17.md)
  - 聚合 phone 0003/0004、Tablet 0001 和 2in1 0001/0002 的 authoritative manifest、hash、target tuple 与独立审查结论
  - phone 记录覆盖公开 runtime blocked；Tablet/2in1 只覆盖 registration-layer 前置 blocked 且未安装 HAP；三者只覆盖各自 image
- [E3-PHYS-PREFLIGHT 物理设备预检计划与证据模板](e3-physical-preflight.md)
  - E8 前唯一物理 campaign 的治理边界、冻结输入、停止规则与历史证据模板
  - API23 initial 已消费；rebind 已完成；`ADJ-20260806-0003` 冻结 HarmonyOS 7/API 26 元组并曾准备 20260806 campaign ID；host reverify PASS；`ADJ-20260806-0004` 单条 build 确认 PASS；API26 live `E3-PHYS-PREFLIGHT-20260807-0001` 已 `consumed-blocked`（保留）；`E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` 已 Live 并 `consumed-blocked`（`reviewed-pass/blocked`；完整 1–7；双审查 0 B/5 M）；`ADJ-20260807-0003` 已批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径（runner/freeze/selftest 随执行 commit `e3fe0c6` 更新，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)）；S6 双 active 为 operator 三态确认（`NO-DUAL-ACTIVE-CAPTURED` 优先，false 时再独立 `DUAL-ACTIVE-CAPTURED`；仅 dual=true 且 noDual=false 为 fail，双 false/双 true 为 blocked）；S5 Settings>VPN 页 capture 为 observation-only（失败只写 `observation_only_degraded` 诊断，不进入全局 `capture_degraded`、不阻断 S5/overall）；当前 `plan_status: blocked-awaiting-device-authorization`（candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`；下一步须用户显式设备授权 + fresh device confirmation 后重生 `ready` freeze，可绑定同候选身份但仅在明确授权后按治理决定；目前不可执行；禁 HDC；E8 `CLOSED`）
- [E3-PHYS-PREFLIGHT 物理设备 live 预检证据（API23）](evidence/e3-physical-preflight-2026-08-06.md)
  - `EV-E3-PHYS1API23-20260806-0001`：`execution: live`、`attempt: initial`、`record_status: reviewed-pass`、`verdict: blocked`
  - live model 匹配而 build 可见 suffix drift；`campaign_started=false`，A/B 未安装/运行，cleanup `verified-clean`，0 blocker/0 major；E3 未关闭、E8 `CLOSED`；旧 ID 不可复用，无 infra retry
- [E3-PHYS-PREFLIGHT 物理设备 live 预检证据（API26 0001 operator-aborted/blocked）](evidence/e3-physical-preflight-api26-2026-08-07.md)
  - `EV-E3-PHYS1API26-20260807-0001`：`consumed-blocked`、`reviewed-pass/blocked`、seal_mode `operator-aborted-procedural`（保留）
  - 场景 5 Settings 误确认并直接关窗；recovery cleanup `verified_absent`；独立 seal 审查 0 B/M；禁止局部 scenario5 重放；公开 hash 见该记录
- [E3-PHYS-PREFLIGHT 物理设备 live 预检证据（API26 0002 blocked）](evidence/e3-physical-preflight-api26-0002-2026-08-07.md)
  - `EV-E3-PHYS1API26-20260807-0002`：`consumed-blocked`、`reviewed-pass/blocked`；S1/S4 pass，S2/S3/S5/S6/S7 blocked；cleanup `verified-clean`
  - 双审查 isolated `kimi-coding/k3` + `anthropic/claude-sonnet-5` 均 0 B/5 M；opus timeout attempt-not-counted；`ADJ-20260807-0003` 已批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径；`plan_status: blocked-awaiting-device-authorization`；用户显式设备授权 + fresh device confirmation 完成前无 auto retry/新 ID/设备命令授权
- [E3-PHYS-PREFLIGHT host remediation 证据（ADJ-20260807-0003 runner）](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)
  - `EV-E3-PHYS1HOST-20260808-0001`：host-only、`is_evidence: false`；执行 commit `e3fe0c642c28b8a332c0f70db2217787884334e9`（parent `c6acae7`，M1/M3 probe fixes）；host selftest `HDC_PROCESSES=0`、独立审查 0 B/0 M；candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`；DryRun `is_evidence: false`/HDC0/integrity empty；旧 20260807 candidate `INVALID-TIMELINE` 不可用；host `reviewed-pass` 不等于 E3 pass，无 Live/HDC/install/device-ready 授权；E3 open、E8 `CLOSED`
- [E3-PHYS-PREFLIGHT HarmonyOS 7 最小只读 rebind 证据](evidence/e3-physical-rebind7-2026-08-06.md)
  - `EV-E3-PHYS1REBIND7-20260806-0001`：`reviewed-pass/pass` **严格只表示** ADJ-0002 三条 rebind 完成（API `26`/`aarch64`/`arm64-v8a`），不是 E3/campaign pass
  - `ADJ-20260806-0003` 已冻结新元组并曾准备 `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001`（`superseded-unexecuted`）；API26 0001/0002 均 `consumed-blocked`；host reverify PASS（HAP/signature/profile/member hashes 未变；不主张安装兼容性）；`ADJ-20260807-0003` 后 host 侧已重建（`EV-E3-PHYS1HOST-20260808-0001`），现 `blocked-awaiting-device-authorization`；E8 `CLOSED`
- [E3-PHYS-PREFLIGHT HarmonyOS 7 单条 software.version build 确认证据](evidence/e3-physical-build7-confirm-2026-08-06.md)
  - `EV-E3-PHYS1BUILD7-20260806-0001`：`reviewed-pass/pass` **严格只表示** ADJ-0004 单条 build 确认（精确 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` 逐字匹配），不是 E3/campaign pass
  - 消除合成 build 风险；API `26` 仍由前一 rebind 实测、不从 build 推断；历史原始边界不改 IDs；后续见 `ADJ-20260807-0001`/`0002`；新 campaign 须新 commit/freeze
- `spikes/e3-vpn-extension-physical-preflight-hap/`
  - API 23 物理预检探针与唯一 governed runner；旧 unsigned hash 仅为历史准备，设备执行只受专用计划授权
- [Windows + DevEco Studio 开发交接](windows-development-handoff.md)
  - `f44be17` 准备基线的 Windows 已完成回传；普通开发签名、A/B 独立 profile、已消耗 enrollment 边界、最终 hash 与完成即停止记录
- [R1 Go ABI 预探针与 API 24 HAP 构建证据](evidence/r1-go-abi-preflight-2026-07-16.md)
  - 固定NetBird、Go和SDK的编译、链接、DCE、`STATIC_TLS`、syscall及补丁预算边界
  - API 24短生命周期Stage HAP、unsigned产物、双ABI `libprobe.so`、哈希和内容清单
  - x86_64 Go 1.25.12 c-shared同一性打包、0006直接loader失败、0007普通TLS与`DT_NEEDED`传递late-load阻断，以及0010 PS4 历史候选十次 TestRunner 通过和官方 Go 1.25.12 机械移植 high-maintenance STOP；0010 不合格用于当前 E 门
- [安全与合规基线](security-and-compliance.md)
  - 初始威胁模型、资产、信任边界、攻击者及待验证缓解措施
  - NetBird、服务端、工具链、依赖、商标和 Huawei 工具的初始许可证边界
- [双目标实施路线图](roadmap.md)
  - API 24 x86_64 Emulator 客观可执行项总门、独立 2in1/Tablet 记录和唯一 `E3-PHYS-PREFLIGHT` 例外
  - `R0` 至 `R10` 的目标、验证、退出标准、依赖和停止条件
  - 当前R0正式基线（现v0.74.7）、版本门、阶段依赖、受控并行工作和完整完成定义
- [开发环境与 Linux Emulator](development-environment.md)
  - 当前 Pod 的操作系统、持久化边界和已安装工具链
  - HarmonyOS 官方 Linux 支持边界及 2026-07-16 Emulator 启动实测
  - 当前 HOME 恢复入口、环境维护建议和待验证清单
- [HarmonyOS 工具链运行手册](toolchain-runbook.md)
  - Pod 重建和新终端的工具链健康检查顺序
  - Emulator 启停、Beta HDC 连接、分层验收和长稳探测
  - HDC 退化的只读诊断、日志保留及升级回滚
- [HarmonyOS CLI 登录、工具链与依赖下载](toolchain-bootstrap.md)
  - Command Line Tools 的授权获取边界与 Pod 内 VNC 交互入口
  - 随包 SDK、Emulator 镜像、ohpm 和 Hvigor 依赖管理
  - 人工 bootstrap 与无人值守构建的两阶段流程
- [OpenHarmony/HarmonyOS 平台与发行策略](platform-strategy.md)
  - OpenHarmony 公共 API 可移植基线与双目标边界
  - 共享核心和两个应用壳的建议结构
  - 第三方 VPN API、系统权限边界及 native fd 桥接
  - HAP、App Pack、签名、测试和分发策略
- [Tailscale-OHOS VPN 数据通路审计与 NetBird 映射](tailscale-ohos-netbird-port-audit.md)
  - 固定 Tailscale-OHOS SHA 与 NetBird v0.74.7 release/commit 源码映射
  - Go/OpenHarmony 构建缺口、NAPI 线程/内存、TUN fd 所有权和 `tun.Device` 注入
  - 实际 bundle 级绕行与 NetBird socket protect、独立进程恢复、路由/DNS 和许可证边界
  - 外部真机自报不进入本仓 evidence，不授权 `E3-PHYS-PREFLIGHT`，且不改变 E3/E8 状态

## 信息状态

文档中的结论使用以下标记：

- **当前实测**：已在本仓库所在的当前 Pod 中检查，或已由当前项目现场直接确认。
- **官方资料确认**：来自 Huawei 或 OpenHarmony 官方文档；具体可用性仍受 SDK、设备 SysCap 和发行版配置影响。
- **方案建议**：根据当前目标提出的目录、构建、测试或发行安排，尚不代表项目已经实现。
- **尚未验证**：尚缺工程、图形/gRPC、平台集成、真机或发行流程等实际证据。

## 当前范围

当前文档已经覆盖：

- R0 唯一决策源、当前未退出状态、Emulator 客观可执行项总门、唯一 `E3-PHYS-PREFLIGHT` 例外、当前R0正式基线（现v0.74.7）、v0.74.6 历史 evidence 绑定、范围、补丁预算、初始 SLO 和角色责任。
- 证据 ID、必填字段、状态枚举、脱敏规则、支持矩阵、动态调整、补丁记录和保留期。
- E0 普通应用已在 API 24 x86_64 Emulator 完成三次独立 PID 冷启动、可见 UI、生命周期/Node-API marker、停止、sidecar、卸载和残留清理，独立审查结果为 `reviewed-pass`、`verdict: pass`；E0 已关闭。
- E1 C-only 子门已由普通 `EntryAbility` 在三个不同 PID 各完成 10 轮：每 PID 1000 个同步 guarded buffer、1000 个 C pthread 到主 ArkTS 上下文的公开 threadsafe callback、10 次真实 fd ownership，以及逐轮 `/proc/self/fd`/线程快照；记录为 `reviewed-pass/pass`。独立审查确认 0 blocker/major、5 minor，且不改变 measured artifact。现有官方 Go 1.25.12 loader 负面只绑定 v0.74.6；当前R0正式基线（现v0.74.7）尚未重跑且没有 E1 pass。
- E2 C 网络记录 `EV-E2-EMU24-20260717-0002` 已由三个不同普通 `EntryAbility` PID 各完成 10 轮 TCP/UDP loopback、route-derived Pod 本机 endpoint、确定性 DNS/错误和 fd/thread 恢复，并归档 host 双侧原始日志与三张可见 PASS 页面；现为 `reviewed-pass/pass`，E2 已关闭。
- E3 记录 `EV-E3-EMU24-20260717-0003` 及补充 `0004` 均保持 `reviewed-pass/blocked`：在精确 API 24 x86_64 phone Emulator image 上，`com.huawei.hmos.vpndialog` 缺失、普通公开 API 无旁路且 promise pending、Settings 无普通 VPN 管理入口。该 blocked 只覆盖记录的 phone runtime；历史记录及 raw evidence 不改写。E3-E7 在聚合中为 reviewed dependency-blocked aggregation exception，不是 `pass` 或 `N/A`；完整 E4-E7 义务移交 E8 `OPEN` 后物理设备 R2/R3 门。
- 独立 2in1 的 `EV-E3-2IN1EMU24-20260717-0001` 与 user-0 supplemental `0002` 均为 `reviewed-pass/blocked`：完整 user-100/user-0 lists 分别为 49/7 bundles，三范围 direct query 与 Settings 注册查询均无组件，A/B HAP 未发送或安装；审查确认 manifest authority、清理和 `0_bundles` 标签实际表示 0 个 VPN/vpndialog 匹配。
- 独立 Tablet 记录 `EV-E3-TABLETEMU24-20260717-0001` 为 `reviewed-pass/blocked`：官方 API 24 `tablet_x86` image 的 current user 100 与 user 0 完整 bundle list 均无 VPN/vpndialog，default/user 100/user 0 direct query 均失败，Settings 仅有 `MANAGE_VPN` 权限文本而无 `VpnServiceExtAbility`；按停止条件未发送或安装 A/B HAP。审查的 manifest authority、共享 `phone_settings` module 名和 registration-layer 停止边界三项 minor 已记录；该结果只覆盖 Tablet 目标元组。
- R1固定NetBird/Go/SDK预探针、独立审查修正、unsigned API 24应用/测试HAP、普通Node-API双ABI构建、可见Emulator、最小`aa test`、x86_64 Go c-shared `STATIC_TLS` loader负面证据，以及0010 的隔离 PS4 TLSDESC 历史候选十次运行通过和官方 Go 1.25.12 机械应用停止记录；PS4 未发布，0010 不作为当前门输入。
- 初始威胁模型，以及 NetBird 客户端/服务端、未来依赖、商标和 Huawei 工具的许可证基线。
- Debian Pod 内的工具链与持久化条件。
- HarmonyOS Command Line Tools 和 Linux Emulator 的官方支持信息。
- 稳定与 Beta Command Line Tools 的本地制品指纹、独立安装及 HOME 恢复入口。
- API 24 镜像、KVM 无窗口启动、guest boot 完成和 HDC 连接的 2026-07-16 实测。
- Pod 重建、新终端、Emulator 生命周期、30-40 分钟验收和 HDC 退化取证的可执行 runbook。
- Command Line Tools 授权获取、随包 SDK、镜像及公开/私有依赖下载边界。
- 已完成人工 bootstrap 后的无人值守工具链准备建议。
- OpenHarmony 与 HarmonyOS 双目标的代码、SDK、签名、制品、测试和分发边界。
- `vpnExtension`、`VpnExtensionAbility`、`VpnConnection` 的公开能力和 `MANAGE_VPN` 的边界。
- NetBird Go/WireGuard 交叉编译、NAPI/native fd 桥接和目标产品能力的待验证项。

当前文档不表示以下事项已经完成：

- API 24 x86_64 Emulator 客观可执行项总门已建立但尚未通过；当前总门为 `CLOSED`。当前R0正式基线（现v0.74.7）的 E1 未重跑且无 pass。API23 initial live 已登记为 `EV-E3-PHYS1API23-20260806-0001`（`reviewed-pass/blocked`）。rebind `EV-E3-PHYS1REBIND7-20260806-0001` 已完成（`reviewed-pass/pass`，仅 rebind；API `26`/`aarch64`/`arm64-v8a`）。`ADJ-20260806-0003` 冻结 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` / Settings `7.0.0.100 (SP8C00E32R7P2patch09)` / API `26` 元组，批准复用原 FINAL HAP hashes（兼容性实测、不证明成功），并曾准备 `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001`（`superseded-unexecuted`）；host reverify 已 PASS。`ADJ-20260806-0004` / `EV-E3-PHYS1BUILD7-20260806-0001` 单条 `software.version` 实测确认 HDC build 逐字匹配（`reviewed-pass/pass`，仅 build-confirm）；API `26` 仍 rebind 实测、不从 build 推断。API26 live `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 已 `consumed-blocked`（operator-aborted；禁局部 scenario5 重放；保留）。`E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` 已 Live 并 `consumed-blocked`（`reviewed-pass/blocked`；完整 1–7；双审查 0 B/5 M）。当前 `plan_status: blocked-awaiting-device-authorization`（无 auto retry/新 ID/设备命令授权；本登记禁 HDC；`ADJ-20260807-0003` runner 变更完成，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)；candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`；下一步须用户显式设备授权 + fresh device confirmation 后重生 `ready` freeze，可绑定同候选身份但仅在明确授权后按治理决定；目前不可执行）；E8 仍 `CLOSED`。
- R0 已退出，或具名真机、完整目标元组、签名和华为应用市场闭环已经就绪。
- 威胁缓解已经实现，或依赖锁定、SBOM、漏洞审查和最终许可证合规已经完成。
- `/dev/dri`/图形模式或 Emulator gRPC 已经验证。
- 面向产品的 OpenHarmony 或 HarmonyOS 应用工程已经建立；当前只有不得演化为产品壳的短生命周期 E0/R1 API 24 探针和独立 E3 A/B 授权探针。
- NetBird Go 核心、当前R0正式基线（现v0.74.7）的官方 Go 1.25.12 loader、完整 E1 或 VPN 能力已经完成集成验证；现有 ArkTS/native/fd 正面只限 C-only 子证据。
- VPN Extension 授权门已经通过，或已在 Emulator/物理设备建立隧道；phone 公开 runtime blocked，Tablet/2in1 只在 registration-layer 前置边界 blocked。API23 物理预检 initial 为 `reviewed-pass/blocked`（安装前停止）；API26 0001/0002 live 均为 `consumed-blocked`（0001 operator-aborted；0002 完整 1–7 blocked）。`ADJ-20260807-0003` 已批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径。当前 `plan_status: blocked-awaiting-device-authorization`，用户显式设备授权 + fresh device confirmation 完成前无 auto retry/新 ID/设备命令授权；禁止私有 Go、NetBird、`MANAGE_VPN` 或 system/debug/enterprise/root 绕过。
- 任一市场的正式签名、审核、上架或更新流程已经跑通。
- Debian 13 已成为官方支持的 Emulator 宿主。

## 维护约定

新增调查结果时，应保留来源 URL，并把结果归入上述四种信息状态之一。环境现场发生变化时，优先更新“当前实测”和“尚未验证”；SDK 或官方文档发生变化时，同时更新核验日期。平台方案变更时，应分别说明 OpenHarmony 与 HarmonyOS 的影响，避免把一侧的测试结果直接推广到另一侧。
