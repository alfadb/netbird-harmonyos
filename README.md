# netbird-harmonyos

`netbird-harmonyos` 是一个独立开发、非官方维护的 NetBird 客户端项目。长期目标覆盖 HarmonyOS 与具名 OpenHarmony 发行版双目标；当前首目标仅候选为 HarmonyOS API 24，尚未完成目标元组锁定。项目目前处于早期研究与 R0 进行中阶段。

## 项目状态与目标

项目目标是在 HarmonyOS 与具名 OpenHarmony 发行版上探索并实现可用的 NetBird 客户端，共享协议与网络核心，并为两个目标分别维护应用壳、构建签名、制品、测试和分发流程。当前优先推进首目标候选 HarmonyOS API 24 的研究与证据门，不表示首目标已锁定，也不表示任一目标已具备产品支持或发布条件。

当前仓库尚未提供可用发行版本，功能范围、技术方案和平台能力适配仍在验证中。官方 API 24 x86_64 Emulator 矩阵已穷尽 phone、Tablet 与 2in1 三种独立 image/instance 形态，五个最终 E3 记录均为 `reviewed-pass/blocked`：三者的 BMS/Settings 注册层均缺少 `com.huawei.hmos.vpndialog`/`VpnServiceExtAbility`；phone 的公开 API runtime 还记录了 promise pending 与 `onCreate=0`，Tablet/2in1 则按预定停止条件未安装 HAP。E3 不关闭，E4-E7 为 dependency blocked 且不开始；E1 官方 NetBird 基线 Go 1.25.12 loader 仍 blocked；E8 仍为 `CLOSED`，真机执行禁令不变。当前动作是等待包含所需组件的官方 image，以及正式 NetBird/Go 输入变化后重跑对应门；不建议私有 Go 或 system/debug/enterprise 绕过。该矩阵不外推到 arm64、具名真机、其他 image 或华为商用 HarmonyOS。

## 开发状态

项目处于早期开发阶段，接口与实现可能发生较大变化。现阶段不应将其用于生产环境。

## 文档

- [文档索引](docs/README.md)
- [R0 任务章程](docs/r0-charter.md)
- [证据与脱敏 Schema](docs/evidence-schema.md)
- [E0 API 24 Emulator 普通应用证据](docs/evidence/e0-api24-emulator-2026-07-17.md)
- [E1 C-only ArkTS/native/fd Emulator 子证据](docs/evidence/e1-c-bridge-api24-emulator-2026-07-17.md)
- [E2 C 网络 API 24 Emulator 证据](docs/evidence/e2-c-network-api24-emulator-2026-07-17.md)
- [E3 VPN Extension API 24 Emulator 证据](docs/evidence/e3-vpn-extension-api24-emulator-2026-07-17.md)
- [E3 VPN Extension API 24 Emulator 矩阵审查](docs/evidence/e3-vpn-extension-api24-emulator-matrix-2026-07-17.md)
- [安全与合规基线](docs/security-and-compliance.md)
- [双目标实施路线图](docs/roadmap.md)
- [开发环境与 HarmonyOS Linux Emulator](docs/development-environment.md)
- [HarmonyOS 工具链运行手册](docs/toolchain-runbook.md)
- [HarmonyOS CLI 登录、工具链与依赖下载](docs/toolchain-bootstrap.md)
- [OpenHarmony/HarmonyOS 平台与发行策略](docs/platform-strategy.md)

## 上游资源

- [NetBird 官网](https://netbird.io/)
- [NetBird 文档](https://docs.netbird.io/)
- [NetBird GitHub 仓库](https://github.com/netbirdio/netbird)
- [HarmonyOS 支持相关议题](https://github.com/netbirdio/netbird/issues/2270)

## 许可证

本项目以 [MIT 许可证](LICENSE) 发布。

NetBird 名称及商标归其各自权利人所有。NetBird 上游项目以及本项目引用或衍生的上游代码遵循各自的许可证；本项目的 MIT 许可证不改变这些许可条款。
