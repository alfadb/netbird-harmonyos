# netbird-harmonyos

`netbird-harmonyos` 是一个独立开发、非官方维护的 NetBird 客户端项目。长期目标覆盖 HarmonyOS 与具名 OpenHarmony 发行版双目标；当前首目标仍候选为 HarmonyOS API 24。`E3-PHYS-PREFLIGHT` 已在隔离目录完成 API 23 受限 A/B 适配和本地 unsigned 构建，但普通开发签名/profile、已签名 HAP、冻结源码/SDK/final hash、清理和 campaign 输入仍未齐备，R0 尚未完成目标锁定。项目目前处于早期研究与 R0 进行中阶段。

## 项目状态与目标

项目目标是在 HarmonyOS 与具名 OpenHarmony 发行版上探索并实现可用的 NetBird 客户端，共享协议与网络核心，并为两个目标分别维护应用壳、构建签名、制品、测试和分发流程。当前优先推进首目标候选 HarmonyOS API 24 的研究与证据门，不表示首目标已锁定，也不表示任一目标已具备产品支持或发布条件。

当前仓库尚未提供可用发行版本，功能范围、技术方案和平台能力适配仍在验证中。当前R0正式基线（现v0.74.7）固定为 commit `a1c9427d8004576e2cbb9e546d409847fa9df318`；所有绑定 v0.74.6 的历史 evidence 保持原输入和原判定。现有 Go loader 负面只绑定 v0.74.6，v0.74.7 尚未重跑且没有 E1 pass。官方 API 24 x86_64 phone 记录的公开 VPN runtime 路径 blocked；Tablet 与 2in1 只在 registration-layer 前置边界 blocked，未形成完整 runtime 结论。E3-E7 在 E8 中为 reviewed dependency-blocked aggregation exception，不是 `pass` 或 `N/A`。

E8 前只允许一个 `E3-PHYS-PREFLIGHT` campaign：在已冻结的一台具名 HarmonyOS `PLA-AL10` 设备上，用普通开发签名的纯 ArkTS/C 公共 VPN Extension A/B 探针判断 E3 可达性。隔离目录 `spikes/e3-vpn-extension-physical-preflight-hap/` 已完成 API 23 受限适配和本地 unsigned 构建，历史 `spikes/e3-vpn-extension-hap` 与 raw evidence 未修改；该快照不是已签名或 campaign 冻结制品。唯一一次最小只读发现的六条白名单设备 `shell` 已成功，冻结为 distribution `HarmonyOS`、完整 build `PLA-AL10 6.1.0.117(SP6C00E115R7P7)`、API `23`、kernel arch `aarch64` 与 app ABI `arm64-v8a`；真实 HDC endpoint/target 仅在仓外受控映射为 `PHYS-1`。任何已冻结输入漂移必须停止。除该六项发现外，尚未执行其他设备 `shell`、`install`、`send`、`start`、`stop`，campaign 未开始；签名/profile、已签名 A/B HAP、冻结源码/SDK/final hash、清理、采集/审查和 campaign 输入仍缺，计划保持 `blocked`，也未分配 evidence ID 或 verdict，E8 必须保持 `CLOSED`。只有后续预检证据达到 `reviewed-pass/pass` 才满足 E8 `OPEN` 的必要条件，但仍须当前R0正式基线（现v0.74.7）的 E1 通过、全部目标元组/哈希一致和独立聚合审查，不能自动开放。E8 `OPEN` 只许可后续物理投入；E4-E7 完整 VPN/数据面义务仍须在具名物理设备 R2/R3 门完成。

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
