# netbird-harmonyos

`netbird-harmonyos` 是一个独立开发、非官方维护的 NetBird 客户端项目。长期目标覆盖 HarmonyOS 与具名 OpenHarmony 发行版双目标；当前首目标仅候选为 HarmonyOS API 24，尚未完成目标元组锁定。项目目前处于早期研究与 R0 进行中阶段。

## 项目状态与目标

项目目标是在 HarmonyOS 与具名 OpenHarmony 发行版上探索并实现可用的 NetBird 客户端，共享协议与网络核心，并为两个目标分别维护应用壳、构建签名、制品、测试和分发流程。当前优先推进首目标候选 HarmonyOS API 24 的研究与证据门，不表示首目标已锁定，也不表示任一目标已具备产品支持或发布条件。

当前仓库尚未提供可用发行版本，功能范围、技术方案和平台能力适配仍在验证中。API 24 x86_64 phone Emulator 的 E0 已以 `record_status: reviewed-pass`、`verdict: pass` 关闭；E1 C-only 子门证据 `EV-E1-EMU24-20260717-0005` 已为 `reviewed-pass/pass`，但官方 NetBird 基线 Go 1.25.12 loader 仍失败，因此 E1 overall Go blocked。E2 C 网络证据 `EV-E2-EMU24-20260717-0002` 已为 `reviewed-pass/pass` 并关闭。E3 记录 `EV-E3-EMU24-20260717-0003` 与补充 `0004` 均为 `record_status: reviewed-pass`、`verdict: blocked`：独立审查确认精确 API 24 x86_64 phone Emulator image 缺少 `com.huawei.hmos.vpndialog`，普通公开 API 无旁路且请求 pending，Settings 无普通 VPN 管理入口；E3 不关闭，E4-E7 为 dependency blocked 且不开始。当前所有不依赖 Go 且合法可达的 E0、E1-C、E2 已完成，但 E1 官方 Go 仍 blocked；模拟器总门 E8 仍为 `CLOSED`，真机执行禁令不变。该模拟器缺项不外推到其他架构、具名真机或华为商用 HarmonyOS。

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
