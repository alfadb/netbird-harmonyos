# 项目文档

最后核验：2026-07-17

本目录记录 `netbird-harmonyos` 当前阶段的环境调查、平台边界和实施建议。项目仍处于验证阶段；文档会明确区分已经观察到的现场事实、官方资料中的能力、建议方案和尚未完成的验证。

## 文档索引

- [R0 任务章程](r0-charter.md)
  - R0 唯一决策源、当前“进行中/未退出”状态、E0-E8 Emulator 投入总门和真机执行禁令
  - 最新正式 NetBird release 基线、功能范围、补丁预算、初始 SLO、责任矩阵和退出 checklist
- [证据与脱敏 Schema](evidence-schema.md)
  - 信息状态、R/E 证据 ID、必填字段、状态枚举、脱敏规则和记录模板
  - E 门双判定、目标元组/哈希一致性、双向不外推、支持矩阵、动态调整、补丁记录和保留期
- [E0 API 24 Emulator 普通应用证据](evidence/e0-api24-emulator-2026-07-17.md)
  - 三次普通 `EntryAbility` 冷启动、生命周期/Node-API HiLog、可见截图、停止、sidecar、卸载与主机清理
  - `record_status: reviewed-pass`、`verdict: pass`；E0 已关闭，E8 仍 `CLOSED`、真机禁令不变
- [E1 C-only ArkTS/native/fd Emulator 子证据](evidence/e1-c-bridge-api24-emulator-2026-07-17.md)
  - 普通 `EntryAbility` 三个不同 PID，各完成 10 轮同步 buffer、pthread threadsafe callback 与 fd ownership
  - `record_status: reviewed-pass`、`verdict: pass`；只关闭 E1 C-only 子门，不改变 measured artifact；E1 整体仍因官方 Go 1.25.12 loader 失败而 blocked
- [E2 C 网络 API 24 Emulator 证据](evidence/e2-c-network-api24-emulator-2026-07-17.md)
  - 三个不同普通 `EntryAbility` PID 各完成 10 轮 TCP/UDP loopback、Pod 本机受控 endpoint、DNS、事件/错误和资源恢复验证，并显示可见 E2 PASS 页面
  - `record_status: reviewed-pass`、`verdict: pass`；E2 已关闭；E1 overall Go blocked，E8 仍 `CLOSED`，真机禁令不变
- [E3 VPN Extension API 24 Emulator 证据](evidence/e3-vpn-extension-api24-emulator-2026-07-17.md)
  - A 三个 PID 与新鲜 B 均由正常 UI 触发公开 start；系统授权组件缺失，promise pending 且 Extension 无 `onCreate`
  - 0003/0004 均为 `reviewed-pass/blocked`；精确 API 24 x86_64 phone Emulator image 缺少 `com.huawei.hmos.vpndialog`，普通公开 API 无旁路，Settings 无普通 VPN 管理入口；E3 不关闭，E4-E7 dependency blocked 且不开始
- [E3 VPN Extension API 24 2in1 Emulator 前置证据](evidence/e3-vpn-extension-api24-2in1-emulator-2026-07-17.md)
  - 官方 `pc_all_x86` image、独立 `netbird_api24_2in1` 实例、MateBook Pro profile、KVM/noWindow 和固定 HDC target `127.0.0.1:10001`
  - `0001` 与 user-0 supplemental `0002` 均为 `reviewed-pass/blocked`；完整 user 100/user 0 lists 分别为 49/7 bundles，三范围 direct vpndialog 与 Settings 注册查询均无组件，A/B HAP 未安装
- [E3 VPN Extension API 24 Tablet Emulator 前置证据](evidence/e3-vpn-extension-api24-tablet-emulator-2026-07-17.md)
  - 官方 `tablet_x86` image、独立 `netbird_api24_tablet` 实例、MatePad Pro 13 profile、KVM/noWindow 和固定 HDC target `127.0.0.1:10002`
  - `reviewed-pass/blocked`；current user 100、user 0 与 default direct query 均未发现 `vpndialog`/`VpnServiceExtAbility`，按停止条件未安装 A/B HAP；三项审查 minor 已记录，E3 仍不关闭
- [E3 VPN Extension API 24 Emulator 矩阵审查](evidence/e3-vpn-extension-api24-emulator-matrix-2026-07-17.md)
  - 聚合 phone 0003/0004、Tablet 0001 和 2in1 0001/0002 的 authoritative manifest、hash、target tuple 与独立审查结论
  - 官方 API 24 x86_64 phone/Tablet/2in1 均为 `reviewed-pass/blocked`；只覆盖各自 image，不外推 arm64、真机或其他 image
- [R1 Go ABI 预探针与 API 24 HAP 构建证据](evidence/r1-go-abi-preflight-2026-07-16.md)
  - 固定NetBird、Go和SDK的编译、链接、DCE、`STATIC_TLS`、syscall及补丁预算边界
  - API 24短生命周期Stage HAP、unsigned产物、双ABI `libprobe.so`、哈希和内容清单
  - x86_64 Go 1.25.12 c-shared同一性打包、0006直接loader失败、0007普通TLS与`DT_NEEDED`传递late-load阻断，以及0010 PS4 历史候选十次 TestRunner 通过和官方 Go 1.25.12 机械移植 high-maintenance STOP；0010 不合格用于当前 E 门
- [安全与合规基线](security-and-compliance.md)
  - 初始威胁模型、资产、信任边界、攻击者及待验证缓解措施
  - NetBird、服务端、工具链、依赖、商标和 Huawei 工具的初始许可证边界
- [双目标实施路线图](roadmap.md)
  - API 24 x86_64 phone Emulator 的 E0-E8 投入总门、独立 2in1/Tablet 补充记录，以及 E8 未开时禁止真机执行
  - `R0` 至 `R10` 的目标、验证、退出标准、依赖和停止条件
  - 最新正式 NetBird release 基线、版本门、阶段依赖、受控并行工作和完整完成定义
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

## 信息状态

文档中的结论使用以下标记：

- **当前实测**：已在本仓库所在的当前 Pod 中检查，或已由当前项目现场直接确认。
- **官方资料确认**：来自 Huawei 或 OpenHarmony 官方文档；具体可用性仍受 SDK、设备 SysCap 和发行版配置影响。
- **方案建议**：根据当前目标提出的目录、构建、测试或发行安排，尚不代表项目已经实现。
- **尚未验证**：尚缺工程、图形/gRPC、平台集成、真机或发行流程等实际证据。

## 当前范围

当前文档已经覆盖：

- R0 唯一决策源、当前未退出状态、E0-E8 Emulator 投入总门、真机执行禁令、最新正式 NetBird release 基线、范围、补丁预算、初始 SLO 和角色责任。
- 证据 ID、必填字段、状态枚举、脱敏规则、支持矩阵、动态调整、补丁记录和保留期。
- E0 普通应用已在 API 24 x86_64 Emulator 完成三次独立 PID 冷启动、可见 UI、生命周期/Node-API marker、停止、sidecar、卸载和残留清理，独立审查结果为 `reviewed-pass`、`verdict: pass`；E0 已关闭。
- E1 C-only 子门已由普通 `EntryAbility` 在三个不同 PID 各完成 10 轮：每 PID 1000 个同步 guarded buffer、1000 个 C pthread 到主 ArkTS 上下文的公开 threadsafe callback、10 次真实 fd ownership，以及逐轮 `/proc/self/fd`/线程快照；记录为 `reviewed-pass/pass`。独立审查确认 0 blocker/major、5 minor，且不改变 measured artifact；官方 Go 1.25.12 loader 仍失败，故 E1 整体仍 blocked。
- E2 C 网络记录 `EV-E2-EMU24-20260717-0002` 已由三个不同普通 `EntryAbility` PID 各完成 10 轮 TCP/UDP loopback、route-derived Pod 本机 endpoint、确定性 DNS/错误和 fd/thread 恢复，并归档 host 双侧原始日志与三张可见 PASS 页面；现为 `reviewed-pass/pass`，E2 已关闭。
- E3 记录 `EV-E3-EMU24-20260717-0003` 及补充 `0004` 均为 `reviewed-pass/blocked`：在精确 API 24 x86_64 phone Emulator image 上，`com.huawei.hmos.vpndialog` 缺失、普通公开 API 无旁路且 promise pending、Settings 无普通 VPN 管理入口，三类根因共同使授权链不可达；E3 不关闭，E4-E7 为 dependency blocked 且不开始。所有当前不依赖 Go 且合法可达的 E0、E1-C、E2 已完成，E1 官方 Go 仍 blocked；E8 仍 `CLOSED`，真机禁令不变。
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

- API 24 x86_64 phone Emulator E0-E8 总门已建立但尚未通过；当前总门为 `CLOSED`，禁止任何真机执行；E3 为 `reviewed-pass/blocked` 且不关闭，E4-E7 dependency blocked、不开始。
- R0 已退出，或具名真机、完整目标元组、签名和华为应用市场闭环已经就绪。
- 威胁缓解已经实现，或依赖锁定、SBOM、漏洞审查和最终许可证合规已经完成。
- `/dev/dri`/图形模式或 Emulator gRPC 已经验证。
- 面向产品的 OpenHarmony 或 HarmonyOS 应用工程已经建立；当前只有不得演化为产品壳的短生命周期 E0/R1 API 24 探针和独立 E3 A/B 授权探针。
- NetBird Go 核心、官方 Go 1.25.12 loader、完整 E1 或 VPN 能力已经完成集成验证；现有 ArkTS/native/fd 正面只限 C-only 子证据。
- VPN Extension 授权门已经通过，或已在模拟器/真机建立隧道；官方 API 24 x86_64 phone、Tablet、2in1 矩阵均为 `reviewed-pass/blocked`，三种设备形态均未建立隧道，彼此不能替代，真机行为仍未验证。当前等待包含组件的官方 image 和正式 NetBird/Go 输入变化后重跑对应门；不建议私有 Go 或 system/debug/enterprise 绕过。
- 任一市场的正式签名、审核、上架或更新流程已经跑通。
- Debian 13 已成为官方支持的 Emulator 宿主。

## 维护约定

新增调查结果时，应保留来源 URL，并把结果归入上述四种信息状态之一。环境现场发生变化时，优先更新“当前实测”和“尚未验证”；SDK 或官方文档发生变化时，同时更新核验日期。平台方案变更时，应分别说明 OpenHarmony 与 HarmonyOS 的影响，避免把一侧的测试结果直接推广到另一侧。
