# netbird-harmonyos

> **当前 E3-PHYS-PREFLIGHT 治理（2026-08-17 · 0001）**：[`AUTH-E3-PHYS1API26-20260817-0001`](docs/evidence/e3-physical-preflight-authorization-2026-08-17-0001.md) 绑定全新 initial pair `E3-PHYS-PREFLIGHT-20260817-0001` / `EV-E3-PHYS1API26-20260817-0001`，当前 `blocked-awaiting-full-gates`、not ready，reviewer role 固定为 `isolated-anthropic-claude-opus-5-reviewer`。0003 已 `governance-review-blocked-retired`，未 Live、未 consumed、不得复用；gate 1 禁止 HDC，gate 4 的 `tconn` 与 `list targets` 各只允许一次。本文其余旧“当前/下一步”叙述均按历史快照理解。

`netbird-harmonyos` 是一个独立开发、非官方维护的 NetBird 客户端项目。长期目标覆盖 HarmonyOS 与具名 OpenHarmony 发行版双目标；当前首目标仍候选为 HarmonyOS API 24。API23 `E3-PHYS-PREFLIGHT` initial live 已因冻结 build drift 在 continuous capture/install 前停止并登记为 `reviewed-pass/blocked`；HarmonyOS 7 只读 rebind 已完成，新元组已冻结，host reverify 已 PASS，单条 build 确认已 PASS（仅 build-confirm）；API26 live `EV-E3-PHYS1API26-20260807-0001` 已 `consumed-blocked`（operator-aborted，保留）；`EV-E3-PHYS1API26-20260807-0002` 已 Live 并 `consumed-blocked`（`reviewed-pass/blocked`，完整 1–7；双审查 0 B/5 M）；`ADJ-20260807-0003` 已批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径（runner/freeze/selftest 随执行 commit `e3fe0c6` 更新，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](docs/evidence/e3-physical-preflight-host-remediation-2026-08-08.md)）；`ADJ-20260808-0001` 已登记进程边界前瞻修复（[`进程边界前瞻修复`](docs/evidence/e3-physical-preflight-process-target-2026-08-08.md)）；`ADJ-20260808-0002` 已登记强可靠操作员信任模型（[`强可靠操作员信任模型`](docs/evidence/e3-physical-preflight-operator-trust-2026-08-08.md)：`mechanical-action-only-machine-verified-v1`、单步回车、机器 layout/事件/S6 冲突码、scenario invalid 停止后续、无人工三态；只适用下一新 campaign/evidence，不回溯修改历史）；`ADJ-20260808-0003` 已登记强可靠执行细化（真实 layout 校准、prompt-time 事件窗口、空档 UI action guard、infra capture 分类、S5 去除非决定性 VPN 页步骤、process-target verify；App Info 真实结构未采样为下一 Live fail-closed 风险；只适用下一新 campaign，不回溯 0001）；历史（2026-08-10 前）`plan_status: blocked-awaiting-device-authorization`（candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`；2026-08-10 用户已显式授权**新**完整白名单 campaign（[`AUTH-E3-PHYS1API26-20260810-0002`](docs/evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)，取代已消耗的 [0001](docs/evidence/e3-physical-preflight-authorization-2026-08-10.md)：新候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`，旧 pair 因 sealed blocked 已消费不得复用；新增一次 host-prep `hdc list targets` 内存级窄例外——恰一 token、设 process-scope `PHYS_1_TARGET`、不输出/持久化；attempt=initial/retry N/A，任一 gate 失败停止、不重试不换 ID；Live 前允许 runner+治理变更审查后提交/推送，campaign evidence 仍不提前提交），`ADJ-20260810-0001` 已实现 runner `-TargetBindingConfirm` 模式（host-governed 机器 fresh confirmation：3 条白名单 target-binding 命令，仓外 record + `.sha256`，不建 evidence roots、不设 `is_evidence`、不消费 campaign/evidence ID），候选 identity 为 `pending-two-consumption-audits`（audit-1/audit-2 均 hash 记录，发现任何 `execution_mode=live`/`is_evidence=true`/seal 记录占用则不得复用、停止等用户新 ID 授权）；下一步须 Windows：同步 trusted refs/bundle → ID 审计① → blocked confirmation freeze 静态审查 → host-prep `hdc list targets` 映射（一次，内存级）→ `-TargetBindingConfirm` → `ready` freeze draft 绑定 record → 独立审查 record（`e3-ready-freeze-review`）→ 最终 `ready` freeze 绑定 review → ID 审计② → selftest → 同一 `ready` freeze DryRun → 审查且 freeze 字节不变 → 单次 Live；目前不可执行；禁 HDC（host-prep 一次 `hdc list targets` 除外））；E3 未关闭，R0 尚未完成目标锁定。项目目前处于早期研究与 R0 进行中阶段。

## 项目状态与目标

项目目标是在 HarmonyOS 与具名 OpenHarmony 发行版上探索并实现可用的 NetBird 客户端，共享协议与网络核心，并为两个目标分别维护应用壳、构建签名、制品、测试和分发流程。当前优先推进首目标候选 HarmonyOS API 24 的研究与证据门，不表示首目标已锁定，也不表示任一目标已具备产品支持或发布条件。

当前仓库尚未提供可用发行版本，功能范围、技术方案和平台能力适配仍在验证中。当前R0正式基线（现v0.76.3）固定为 commit `f65f7b347ee4e7de6d98c488d3d894cd018b02b6`；所有绑定 v0.74.6/v0.74.7 的历史 evidence 保持原输入和原判定。v0.74.6 历史 Go loader 负面保持原绑定；v0.76.3 由 `EV-E1-EMU24-20260809-0003` 实测 `reviewed-pass/blocked`（无 E1 pass）。官方 API 24 x86_64 phone 记录的公开 VPN runtime 路径 blocked；Tablet 与 2in1 只在 registration-layer 前置边界 blocked，未形成完整 runtime 结论。E3-E7 在 E8 中为 reviewed dependency-blocked aggregation exception，不是 `pass` 或 `N/A`。

E8 前 API23 initial live 已于 2026-08-06 消费并登记为 `EV-E3-PHYS1API23-20260806-0001`（`reviewed-pass/blocked`）。rebind `EV-E3-PHYS1REBIND7-20260806-0001` 已完成三条只读（`reviewed-pass/pass`，严格只表示 rebind，不是 E3/campaign pass）：API `26` / `aarch64` / `arm64-v8a`。`ADJ-20260806-0003` 冻结 `HarmonyOS` / `PLA-AL10` / HDC build `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` / Settings `7.0.0.100 (SP8C00E32R7P2patch09)` / API `26` / `aarch64` / `arm64-v8a`，批准复用原 FINAL HAP hashes（兼容性实测、不证明成功），并曾准备 campaign `E3-PHYS-PREFLIGHT-20260806-0002` / evidence `EV-E3-PHYS1API26-20260806-0001`（从未 Live、未占用；`ADJ-20260807-0001` 标 `superseded-unexecuted`）。host reverify 已 PASS（public manifest `66a70a52c92b927d4b23e528ae6eaf1b52169e504291c6ff0e7efa4c7ffee010`；HAP/signature/profile/member hashes 未变；不主张安装兼容性）。`ADJ-20260806-0004` / `EV-E3-PHYS1BUILD7-20260806-0001` 以单条 `software.version` 实测确认 HDC build 逐字匹配（`reviewed-pass/pass`，仅 build-confirm）；API `26` 仍 rebind 实测、不从 build 推断。API26 live `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 已 `consumed-blocked`（operator-aborted；禁局部 scenario5 重放；独立 seal 审查 0 B/M；保留）。`ADJ-20260807-0002` 批准的中文完整 1–7 重跑已 Live 并登记为 [`EV-E3-PHYS1API26-20260807-0002`](docs/evidence/e3-physical-preflight-api26-0002-2026-08-07.md)（`reviewed-pass/blocked`，`consumed-blocked`；S1/S4 pass，S2/S3/S5/S6/S7 blocked；cleanup `verified-clean`；双审查 0 B/5 M；opus timeout attempt-not-counted）。`ADJ-20260807-0003` 已由用户直接批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径（S3/S7 callback 优先、严格 fallback 到 `PidOf`/`BundleDump` 连续 absent 观察，`FD_STILL_OPEN` 不可覆盖，S3 fallback 需 S5 fresh create 作为 clean reactivation proof；S5 改为人工 Settings>应用信息>A>强制停止 + 截图确认，HDC force-stop 明确 cleanup-only）；runner/freeze example/selftest 随执行 commit `e3fe0c6` 更新（即使后续 docs commit，execution bytes 仍为 `e3fe0c642c28b8a332c0f70db2217787884334e9`）。历史（2026-08-10 前）`plan_status: blocked-awaiting-device-authorization`（host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](docs/evidence/e3-physical-preflight-host-remediation-2026-08-08.md)；candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`）。2026-08-10 用户（直接人类决策者）已显式授权**新**完整白名单 campaign（[`AUTH-E3-PHYS1API26-20260810-0002`](docs/evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)：`device_readiness=user-attested-ready`，用户就绪声明不替代机器 fresh confirmation；取代已消耗的 [`AUTH-E3-PHYS1API26-20260810-0001`](docs/evidence/e3-physical-preflight-authorization-2026-08-10.md)，新候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`，旧 pair 因 sealed blocked 已消费不得复用；新增一次 host-prep `hdc list targets` 内存级窄例外——恰一 token、设 process-scope `PHYS_1_TARGET`、不输出/持久化；attempt=initial/retry N/A，任一 gate 失败停止、不重试不换 ID；Live 前允许 runner+治理变更审查后提交/推送，campaign evidence 仍不提前提交）；`ADJ-20260810-0001` 已实现 runner `-TargetBindingConfirm` host-governed 模式解决 ready 前 catch-22（3 条白名单 `Version`/`TupleModel`/`TupleBuild` 命令，仓外双文件 record + `.sha256` companion，接受 `blocked`/`ready` confirmation freeze 但完整校验 freeze 结构/clean repo/`code_sha`/runner/HDC/外部输入 hash/`PHYS_1_TARGET`，固定候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001` 与 `attempt=initial`（retry N/A；generic retry 分支不进入本路径），不初始化 EvidenceRoot/RawRoot、不设 `is_evidence`、不消费 campaign/evidence ID；Live 与 `ready` DryRun 强制 `machine_fresh_confirmation.status=pass` 与 `independent_review_record.status=pass` 绑定）。候选 identity 为 `pending-two-consumption-audits`（audit-1/audit-2 均 hash 记录；audit-1 同时确认旧 pair `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 因 sealed blocked 已消费，发现任何 `execution_mode=live`/`is_evidence=true`/seal 记录占用则不得复用、停止等用户新 ID 授权）。下一步须 Windows：同步 trusted refs/bundle → ID 审计① → blocked confirmation freeze 静态审查（`independent_review_ready=true` 仅表示契约/角色静态就绪）→ host-prep `hdc list targets` 映射（一次，内存级，恰一 token，设 process-scope `PHYS_1_TARGET`）→ `-TargetBindingConfirm` → `ready` freeze draft 绑定 record → 独立审查 record → 最终 `ready` freeze 绑定 review → ID 审计② → selftest → 同一 `ready` freeze DryRun → 审查且 freeze 字节不变 → 单次 Live；目前不可执行；禁 HDC（host-prep 一次 `hdc list targets` 除外）。E3 未关闭，E8 保持 `CLOSED`。完整记录见 [API23 物理预检证据](docs/evidence/e3-physical-preflight-2026-08-06.md)、[API26 0001 blocked 证据](docs/evidence/e3-physical-preflight-api26-2026-08-07.md)、[API26 0002 blocked 证据](docs/evidence/e3-physical-preflight-api26-0002-2026-08-07.md)、[host remediation 证据](docs/evidence/e3-physical-preflight-host-remediation-2026-08-08.md)、[进程边界前瞻修复证据（ADJ-20260808-0001）](docs/evidence/e3-physical-preflight-process-target-2026-08-08.md)、[rebind 证据](docs/evidence/e3-physical-rebind7-2026-08-06.md)、[build 确认证据](docs/evidence/e3-physical-build7-confirm-2026-08-06.md) 与 [2026-08-10 授权登记（0002，historical）](docs/evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)（[0001（superseded / consumed）](docs/evidence/e3-physical-preflight-authorization-2026-08-10.md)）。

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
- [E3-PHYS-PREFLIGHT 物理设备 live 预检证据（API23）](docs/evidence/e3-physical-preflight-2026-08-06.md)
- [E3-PHYS-PREFLIGHT 物理设备 live 预检证据（API26 0001）](docs/evidence/e3-physical-preflight-api26-2026-08-07.md)
- [E3-PHYS-PREFLIGHT 物理设备 live 预检证据（API26 0002）](docs/evidence/e3-physical-preflight-api26-0002-2026-08-07.md)
- [E3-PHYS-PREFLIGHT host remediation 证据（ADJ-20260807-0003 runner）](docs/evidence/e3-physical-preflight-host-remediation-2026-08-08.md)
- [E3-PHYS-PREFLIGHT 进程边界前瞻修复（ADJ-20260808-0001）](docs/evidence/e3-physical-preflight-process-target-2026-08-08.md)
- [E3-PHYS-PREFLIGHT HarmonyOS 7 最小只读 rebind 证据](docs/evidence/e3-physical-rebind7-2026-08-06.md)
- [E3-PHYS-PREFLIGHT HarmonyOS 7 单条 software.version build 确认证据](docs/evidence/e3-physical-build7-confirm-2026-08-06.md)
- [E3-PHYS-PREFLIGHT 2026-08-16 授权登记（0003，当前）](docs/evidence/e3-physical-preflight-authorization-2026-08-16-0003.md)（[0002（governance-order-invalid-retired；未 Live、未 consumed）](docs/evidence/e3-physical-preflight-authorization-2026-08-16-0002.md)，[0001（gate-3 review-blocked retired）](docs/evidence/e3-physical-preflight-authorization-2026-08-16-0001.md)）
- [E3-PHYS-PREFLIGHT 2026-08-10 授权登记（0002，historical）](docs/evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)（[0001（superseded / consumed）](docs/evidence/e3-physical-preflight-authorization-2026-08-10.md)）
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
