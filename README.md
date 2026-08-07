# netbird-harmonyos

`netbird-harmonyos` 是一个独立开发、非官方维护的 NetBird 客户端项目。长期目标覆盖 HarmonyOS 与具名 OpenHarmony 发行版双目标；当前首目标仍候选为 HarmonyOS API 24。API23 `E3-PHYS-PREFLIGHT` initial live 已因冻结 build drift 在 continuous capture/install 前停止并登记为 `reviewed-pass/blocked`；HarmonyOS 7 只读 rebind 已完成，新元组已冻结，host reverify 已 PASS，单条 build 确认已 PASS（仅 build-confirm）；API26 live `EV-E3-PHYS1API26-20260807-0001` 已 `consumed-blocked`（operator-aborted）；`ADJ-20260807-0002` 批准中文完整重跑，当前准备 `E3-PHYS-PREFLIGHT-20260807-0002`，`plan_status: pending-commit-freeze`（需 commit/freeze/dryrun/device-ready；禁 HDC）；E3 未关闭，R0 尚未完成目标锁定。项目目前处于早期研究与 R0 进行中阶段。

## 项目状态与目标

项目目标是在 HarmonyOS 与具名 OpenHarmony 发行版上探索并实现可用的 NetBird 客户端，共享协议与网络核心，并为两个目标分别维护应用壳、构建签名、制品、测试和分发流程。当前优先推进首目标候选 HarmonyOS API 24 的研究与证据门，不表示首目标已锁定，也不表示任一目标已具备产品支持或发布条件。

当前仓库尚未提供可用发行版本，功能范围、技术方案和平台能力适配仍在验证中。当前R0正式基线（现v0.74.7）固定为 commit `a1c9427d8004576e2cbb9e546d409847fa9df318`；所有绑定 v0.74.6 的历史 evidence 保持原输入和原判定。现有 Go loader 负面只绑定 v0.74.6，v0.74.7 尚未重跑且没有 E1 pass。官方 API 24 x86_64 phone 记录的公开 VPN runtime 路径 blocked；Tablet 与 2in1 只在 registration-layer 前置边界 blocked，未形成完整 runtime 结论。E3-E7 在 E8 中为 reviewed dependency-blocked aggregation exception，不是 `pass` 或 `N/A`。

E8 前 API23 initial live 已于 2026-08-06 消费并登记为 `EV-E3-PHYS1API23-20260806-0001`（`reviewed-pass/blocked`）。rebind `EV-E3-PHYS1REBIND7-20260806-0001` 已完成三条只读（`reviewed-pass/pass`，严格只表示 rebind，不是 E3/campaign pass）：API `26` / `aarch64` / `arm64-v8a`。`ADJ-20260806-0003` 冻结 `HarmonyOS` / `PLA-AL10` / HDC build `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` / Settings `7.0.0.100 (SP8C00E32R7P2patch09)` / API `26` / `aarch64` / `arm64-v8a`，批准复用原 FINAL HAP hashes（兼容性实测、不证明成功），并曾准备 campaign `E3-PHYS-PREFLIGHT-20260806-0002` / evidence `EV-E3-PHYS1API26-20260806-0001`（从未 Live、未占用；`ADJ-20260807-0001` 标 `superseded-unexecuted`）。host reverify 已 PASS（public manifest `66a70a52c92b927d4b23e528ae6eaf1b52169e504291c6ff0e7efa4c7ffee010`；HAP/signature/profile/member hashes 未变；不主张安装兼容性）。`ADJ-20260806-0004` / `EV-E3-PHYS1BUILD7-20260806-0001` 以单条 `software.version` 实测确认 HDC build 逐字匹配（`reviewed-pass/pass`，仅 build-confirm）；API `26` 仍 rebind 实测、不从 build 推断。API26 live `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 已 `consumed-blocked`（operator-aborted；禁局部 scenario5 重放；独立 seal 审查 0 B/M）。`ADJ-20260807-0002` 批准中文完整 1–7 重跑（protocol usability correction，非设备行为 retry），当前准备 `E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002`（同 tuple/HAP；runner 中文变更；prior blocked 显式绑定）。当前 `plan_status: pending-commit-freeze`（需新 commit/freeze/dryrun/device-ready；本登记禁 HDC）。E3 未关闭，E8 保持 `CLOSED`。完整记录见 [API23 物理预检证据](docs/evidence/e3-physical-preflight-2026-08-06.md)、[API26 blocked 证据](docs/evidence/e3-physical-preflight-api26-2026-08-07.md)、[rebind 证据](docs/evidence/e3-physical-rebind7-2026-08-06.md) 与 [build 确认证据](docs/evidence/e3-physical-build7-confirm-2026-08-06.md)。

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
- [E3-PHYS-PREFLIGHT 物理设备预检计划与证据模板](docs/e3-physical-preflight.md)
- [E3-PHYS-PREFLIGHT 物理设备 live 预检证据](docs/evidence/e3-physical-preflight-2026-08-06.md)
- [E3-PHYS-PREFLIGHT HarmonyOS 7 最小只读 rebind 证据](docs/evidence/e3-physical-rebind7-2026-08-06.md)
- [E3-PHYS-PREFLIGHT HarmonyOS 7 单条 software.version build 确认证据](docs/evidence/e3-physical-build7-confirm-2026-08-06.md)
- [Windows + DevEco Studio 开发交接](docs/windows-development-handoff.md)
- [安全与合规基线](docs/security-and-compliance.md)
- [双目标实施路线图](docs/roadmap.md)
- [开发环境与 HarmonyOS Linux Emulator](docs/development-environment.md)
- [HarmonyOS 工具链运行手册](docs/toolchain-runbook.md)
- [HarmonyOS CLI 登录、工具链与依赖下载](docs/toolchain-bootstrap.md)
- [OpenHarmony/HarmonyOS 平台与发行策略](docs/platform-strategy.md)
- [Tailscale-OHOS VPN 数据通路审计与 NetBird 映射](docs/tailscale-ohos-netbird-port-audit.md)

## 上游资源

- [NetBird 官网](https://netbird.io/)
- [NetBird 文档](https://docs.netbird.io/)
- [NetBird GitHub 仓库](https://github.com/netbirdio/netbird)
- [HarmonyOS 支持相关议题](https://github.com/netbirdio/netbird/issues/2270)

## 许可证

本项目以 [MIT 许可证](LICENSE) 发布。

NetBird 名称及商标归其各自权利人所有。NetBird 上游项目以及本项目引用或衍生的上游代码遵循各自的许可证；本项目的 MIT 许可证不改变这些许可条款。
