# Debian 13 原 Linux worker：E1 v0.76.3 stock Go 单次重放交接（已执行）

最后核验：2026-08-09

本文原为 E1 v0.76.3 stock Go loader/runtime 单次完整重放（`spikes/r1-api24-hap/e1-stock-go-replay.sh` 完整模式）的交接文档。**该交接已执行完毕**：`EV-E1-EMU24-20260809-0003` 已于 2026-08-09 在恢复的历史 Linux worker（Debian 13 + Command Line Tools 6.1.1.290 + Beta 26.0.0.461 + Go 1.25.12 + ffmpeg + KVM Emulator）上完成完整重放，结果为 **measured blocked / collected**（运行期 pre-review 状态；经终审 `REV-E1-EMU24-20260809-0003` 后记录级为 `reviewed-pass/blocked`，见 [0003 measured-blocked 记录](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md) 与 [0003 终审记录](evidence/e1-stock-go-v0763-replay-0003-review-2026-08-09.md)）。**ID 已消耗，禁止同 ID 重跑**；本文不再保留可执行的 0003 命令，后续工作转入证据审查（已完成）。

本文不授权物理设备 campaign，不改变 `R0`、`E8` 或 `E3-PHYS-PREFLIGHT` 计划状态。E1 门语义见 [证据与脱敏 Schema](evidence-schema.md) 与 [R0 任务章程](r0-charter.md)。

## 1. 当前状态（交接基线 → 已执行）

| 项 | 状态 |
| --- | --- |
| NetBird 正式基线 | `v0.76.3` / commit `f65f7b347ee4e7de6d98c488d3d894cd018b02b6` / `go 1.25.5` / `toolchain go1.25.12`（runner 固定常量，0003 运行时在线核验 pass） |
| E1 v0.76.3 stock Go loader/runtime | **已重跑 0003 为 `reviewed-pass/blocked`（`EV-E1-EMU24-20260809-0003`，终审 `REV-E1-EMU24-20260809-0003`），无 E1 pass**；`EV-E1-EMU24-20260809-0001`（runner defect，exit 1，见 [0001 记录](evidence/e1-stock-go-v0763-replay-0001-consumed-failure-2026-08-09.md)）与 `EV-E1-EMU24-20260809-0002`（runner defect，exit 1，见 [0002 记录](evidence/e1-stock-go-v0763-replay-0002-consumed-failure-2026-08-09.md)）均已消耗；`EV-E1-EMU24-20260809-0003` 已完整执行并消耗（measured blocked/collected 为运行期 pre-review 状态，经终审后记录级 `reviewed-pass/blocked`，见 [0003 记录](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md) 与 [0003 终审记录](evidence/e1-stock-go-v0763-replay-0003-review-2026-08-09.md)）；**禁止对 0001/0002/0003 同 ID 重跑** |
| host-only 前置证据 | `EV-E1-EMU24HOST-20260809-0001` 已登记：`execution: not-run-host-preflight`、`record_status: collected`、`verdict: blocked`；只覆盖 host 侧核验，不形成平台结论、不占用 runtime evidence ID（见 [记录](evidence/e1-stock-go-v0763-host-preflight-2026-08-09.md)） |
| E8 | `CLOSED`（0003 manifest `e8_status=CLOSED`；0003 是 `reviewed-pass/blocked`（measured blocked 经终审），不构成 E1 pass，不改变 E8） |
| E3-PHYS-PREFLIGHT | **当前（2026-08-29 · 0001）**：[`AUTH-E3-PHYS1API26-20260829-0001`](evidence/e3-physical-preflight-authorization-2026-08-29-0001.md) 绑定新 pair `E3-PHYS-PREFLIGHT-20260829-0001` / `EV-E3-PHYS1API26-20260829-0001`，attempt initial / retry N/A；`plan_status: blocked-awaiting-full-gates`、not ready，reviewer role exact、HAP source basis `62409c5f...` 与最终 `code_sha` 逻辑不变；**S7 前置校准**（依据 20260828 Live 双处观察：B 的 `:vpn` 进程在冲突拒绝后存活为平台行为，S7 前置只门控 A active、B observed-only）；20260828-0001 已 Live consumed-blocked（S1-S6 首次全 pass、S7 前置模型与平台行为不符 blocked，证据 `reviewed-pass/blocked`，不得复用）；20260825-0001 已因 gate 5 TargetBindingConfirm 实测设备 OTA build 与冻结 build 漂移以 `governance-tuple-drift-retired` 退役（blocked confirmation record，非流程违规），未 DryRun/Live/consumed；20260817-0002 已在 gate 4 未完成时提前执行 gate 5 TargetBindingConfirm 产生 blocked confirmation record 以 `governance-order-invalid-retired` 退役，未 DryRun/Live/consumed；20260817-0001 因 gate 9 未授权设备进程枚举 `governance-operation-invalid-retired`，audit-2 文件不构成 gate pass，未 DryRun/Live/consumed。历史 pair 均不得复用。E8 `CLOSED`（见 [计划](e3-physical-preflight.md)） |

E3 的“禁 HDC”指物理设备 HDC。0003 runner 的 HDC 只连接固定 Emulator target `127.0.0.1:10000`，不触碰任何物理设备，与 E3 约束不冲突。

## 2. 0003 执行结果摘要（非敏感）

- 执行：`EVIDENCE_ID=EV-E1-EMU24-20260809-0003 bash spikes/r1-api24-hap/e1-stock-go-replay.sh`，2026-08-09T17:41:17+08:00 开始、17:43:10+08:00 结束，exit 0。
- 结果：`MEASURED_VERDICT=blocked`、`VERDICT=blocked`、`RECORD_STATUS=collected`（`RECORD_STATUS=collected` 为 runner 在运行结束时 sealed 的 pre-review 状态，formal raw 原样保留、不改写；经终审 `REV-E1-EMU24-20260809-0003` 后记录级为 `reviewed-pass/blocked`）；guest loader 在 `dlopen` 阶段精确拒绝 stock Go 1.25.12 `libgoprobe.so`（`res_search: initial-exec TLS resolves to dynamic definition`，`loaderErrno=2`）；`AA_TEST_RC=0`（host）而 guest `TestFinished-ResultCode: 1`；baseline 在线核验 pass；清理完成、`FINAL_RESIDUAL_PROCESS=false`、`FINAL_RESIDUAL_PORT=false`。
- 证据：`docs/evidence/raw/EV-E1-EMU24-20260809-0003-*`（九份 formal raw + 两份带外补充 `qemu-boot-section.log` / `baseline-postmortem.log`）；manifest 最终 hash `transcript_final_sha256=797882fd…` / `manifest_sha256=a171d6aa…`（自 hash 语义见 [0003 记录](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md)）。
- 未自行升级 reviewed-pass；未提交；未重跑同 ID。

## 3. runner 安全边界与固定输入（保留作参考）

runner 固定常量（不可由环境覆盖，`--selftest` 会逐项验证 guard）：

| 项 | 固定值 |
| --- | --- |
| baseline | `v0.76.3` / `f65f7b347ee4e7de6d98c488d3d894cd018b02b6` / `go 1.25.5` / `toolchain go1.25.12` |
| runGoProbe 源码快照 | `34d512541ca8047f8e3796abd6d85ef94cc13559`（`git archive` 提取到临时目录构建，不改当前 C-only 探针源码） |
| Emulator target | `127.0.0.1:10000`（仅此一个） |
| Emulator 实例 | `netbird_api24_phone`（HDC 端口固定 10000） |
| bundle / test module | `cn.alfadb.netbird.r1probe` / `entry_test` |
| 期望 loader 拒绝短语 | `initial-exec TLS resolves to dynamic definition` |

**禁止输入**（guard 在任何 evidence 文件创建前拒绝）：`PHYS_1_TARGET` 已设置、`TARGET`/`HDC_TARGET`/`EMULATOR_TARGET` 非 `127.0.0.1:10000`、`EMULATOR_HDC_PORT` ≠ `10000`、`EMULATOR_INSTANCE` ≠ `netbird_api24_phone` 均 fail；环境默认值指向历史 worker 固定路径，不要改动。

## 4. 后续顺序

1. E1 结果审查已完成：`EV-E1-EMU24-20260809-0003` 经终审 `REV-E1-EMU24-20260809-0003`（双路独立，`anthropic/claude-opus-5` evidence-integrity + `moonshotai/kimi-k2.7-code` status-consistency，0 blocker/0 major）为 `reviewed-pass/blocked`；stock Go loader 负面结论绑定 v0.76.3 基线；E1 overall Go 仍无 pass。
2. E1 结论已成立（0003 `reviewed-pass/blocked` 经终审）；E3 新授权已由 `AUTH-E3-PHYS1API26-20260810-0002` 授予（取代已消耗的 0001，新候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`），`plan_status: authorized-awaiting-windows-ready-freeze`；顺序门须按 [授权登记 0002](evidence/e3-physical-preflight-authorization-2026-08-10-0002.md) 执行（ID 审计① → blocked confirmation freeze 静态审查 → 一次内存级 host-prep `hdc list targets` 映射 → `-TargetBindingConfirm` 机器 fresh confirmation → ready freeze 绑定 → 独立审查 → ID 审计② → selftest → DryRun → 单次 Live）；机器 fresh confirmation 完成前无任何设备命令授权（见 [e3-physical-preflight.md](e3-physical-preflight.md)）。

## 引用文档

- [E1 v0.76.3 stock Go 重放 0003 measured-blocked 记录](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md)
- [E1 v0.76.3 stock Go 重放 0003 终审记录](evidence/e1-stock-go-v0763-replay-0003-review-2026-08-09.md)（`REV-E1-EMU24-20260809-0003`，`reviewed-pass/blocked`）
- [E1 v0.76.3 stock Go 重放 0001 consumed-failure 记录](evidence/e1-stock-go-v0763-replay-0001-consumed-failure-2026-08-09.md)
- [E1 v0.76.3 stock Go 重放 0002 consumed-failure 记录](evidence/e1-stock-go-v0763-replay-0002-consumed-failure-2026-08-09.md)
- [E1 v0.76.3 host preflight 记录](evidence/e1-stock-go-v0763-host-preflight-2026-08-09.md)
- [工具链运行手册](toolchain-runbook.md)（固定矩阵、健康检查、Emulator 启停）
- [开发环境与 Linux Emulator](development-environment.md)（HOME 布局、持久化、检查脚本）
- [证据与脱敏 Schema](evidence-schema.md)（`record_status`/`verdict` 语义）
- [R0 任务章程](r0-charter.md) 与 [双目标实施路线图](roadmap.md)（R0 基线、E1/E8 门）
- [E3-PHYS-PREFLIGHT 计划](e3-physical-preflight.md)（当前 `AUTH-E3-PHYS1API26-20260810-0002` 授权下 `authorized-awaiting-windows-ready-freeze`；机器 fresh confirmation 完成前禁 PHYS-1 HDC，唯一例外为一次内存级 host-prep `hdc list targets`）
