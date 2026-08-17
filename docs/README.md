# 项目文档

最后核验：2026-08-15

本目录记录 `netbird-harmonyos` 当前阶段的环境调查、平台边界和实施建议。项目仍处于验证阶段；文档会明确区分已经观察到的现场事实、官方资料中的能力、建议方案和尚未完成的验证。

## 文档索引

- [R0 任务章程](r0-charter.md)
  - R0 唯一决策源、当前“进行中/未退出”状态、Emulator 客观可执行项总门和唯一 `E3-PHYS-PREFLIGHT` 例外
  - NetBird v0.76.3 正式基线、v0.74.6/v0.74.7 历史证据绑定、功能范围、补丁预算、初始 SLO、责任矩阵和退出 checklist
- [证据与脱敏 Schema](evidence-schema.md)
  - 信息状态、R/E 证据 ID、必填字段、状态枚举、脱敏规则和记录模板
  - E 门双判定、目标元组/哈希一致性、双向不外推、支持矩阵、动态调整、补丁记录和保留期
- [E0 API 24 Emulator 普通应用证据](evidence/e0-api24-emulator-2026-07-17.md)
  - 三次普通 `EntryAbility` 冷启动、生命周期/Node-API HiLog、可见截图、停止、sidecar、卸载与主机清理
  - `record_status: reviewed-pass`、`verdict: pass`；E0 已关闭，E8 仍 `CLOSED`；该历史记录不授权当前物理预检
- [E1 C-only ArkTS/native/fd Emulator 子证据](evidence/e1-c-bridge-api24-emulator-2026-07-17.md)
  - 普通 `EntryAbility` 三个不同 PID，各完成 10 轮同步 buffer、pthread threadsafe callback 与 fd ownership
  - `record_status: reviewed-pass`、`verdict: pass`；只关闭 E1 C-only 子门；v0.74.6 历史 loader 负面保持原绑定；当前R0正式基线（现v0.76.3）由 `EV-E1-EMU24-20260809-0003` 实测 `reviewed-pass/blocked` 且无 E1 pass
- [E1 v0.76.3 stock Go loader/runtime host-preflight blocked 记录](evidence/e1-stock-go-v0763-host-preflight-2026-08-09.md)
  - `EV-E1-EMU24HOST-20260809-0001`：host-only 前置记录，`execution: not-run-host-preflight`、`record_status: collected`、`verdict: blocked`；不占用后续 runtime evidence ID、不形成平台结论
  - 可续跑入口 `spikes/r1-api24-hap/e1-stock-go-replay.sh [--preflight]`：固定 v0.76.3/f65f7b3…/go 1.25.5/toolchain go1.25.12、复用 34d5125 runGoProbe 快照、仅操作 127.0.0.1:10000；恢复历史 Linux worker 后单命令完整重放
- [E1 v0.76.3 stock Go 重放 0001 consumed-failure 记录](evidence/e1-stock-go-v0763-replay-0001-consumed-failure-2026-08-09.md)
  - `EV-E1-EMU24-20260809-0001`：完整模式首次真实执行，`exit 1`、aborted-before-any-measurement；runner defect（readonly `GO_PROBE_OUTPUT_DIR` 前缀赋值中止，打印命令与真实执行不一致）；无 measured verdict/manifest/seal、Emulator 未启动；ID 已消耗、禁止同 ID 重跑；三份 raw 哈希已核对
  - 历史：runner 修复（build.sh 调用补 `env`，与 print_command 一致）后 `DEFAULT_EVIDENCE_ID` 曾前移至 `EV-E1-EMU24-20260809-0002`；0002 亦已消耗（见下）
- [E1 v0.76.3 stock Go 重放 0002 consumed-failure 记录](evidence/e1-stock-go-v0763-replay-0002-consumed-failure-2026-08-09.md)
  - `EV-E1-EMU24-20260809-0002`：完整模式第二次真实执行，`exit 1`、aborted-before-platform-measurement；runner defect（`readelf -lW` 字面 `PT_TLS` grep 假阴性——实际 ELF 有 `TLS` program header/`STATIC_TLS`/`R_X86_64_TPOFF64`，hash `84bd84…`）；无 measured verdict/manifest/seal、Emulator 未启动；ID 已消耗、禁止同 ID 重跑；三份 raw 哈希与 ELF 哈希已核对
  - runner 已修复（新增 `pt_tls_diag` helper + selftest 正反例）；`DEFAULT_EVIDENCE_ID` 前移至 `EV-E1-EMU24-20260809-0003`，为下一次唯一正式 ID
- [E1 v0.76.3 stock Go 重放 0003 measured-blocked 记录](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md)
  - `EV-E1-EMU24-20260809-0003`：完整模式第三次真实执行，全流程跑通、exit 0；真实 API 24 x86_64 phone Emulator（非物理设备）；guest loader 在 `dlopen` 阶段精确拒绝 stock Go 1.25.12 `libgoprobe.so`（`res_search: initial-exec TLS resolves to dynamic definition`，`loaderErrno=2`）；`MEASURED_VERDICT=blocked`/`VERDICT=blocked`/`RECORD_STATUS=collected`（runner sealed 的 pre-review 状态，formal raw 原样保留、不改写；经终审 `REV-E1-EMU24-20260809-0003` 后记录级为 `reviewed-pass/blocked`）；`AA_TEST_RC=0`（host）而 guest `TestFinished-ResultCode: 1`；baseline 在线核验 pass；清理完成、无残留；E8 保持 `CLOSED`；ID 已消耗、禁止同 ID 重跑；九份 raw 哈希与 manifest 自 hash 已核对；两份带外补充（QEMU boot 区间摘录、baseline 公开 API 复核）覆盖可复核缺口
- [E1 v0.76.3 stock Go 重放 0003 终审记录](evidence/e1-stock-go-v0763-replay-0003-review-2026-08-09.md)
  - `REV-E1-EMU24-20260809-0003`：双路独立终审（`anthropic/claude-opus-5` evidence-integrity + `moonshotai/kimi-k2.7-code` status-consistency）0 blocker/0 major；主会话重算 11 份材料 sha256 全部匹配；`record_status: reviewed-pass`、`verdict: blocked`；`reviewed-pass` 不是 E1 pass，E1 overall Go 未关闭，E8 保持 `CLOSED`
- [Go 1.26.0 stock c-shared ELF research 对照](evidence/go126-stock-cshared-elf-research-2026-08-09.md)
  - `RS-E1-GO126ELF-20260809-0001`：research-only host 对照（无 Emulator/hdc/guest loader）；同一冻结 snapshot `34d5125…` + 0003 runner 构建环境，仅 Go 换成 `go1.26.0 linux/amd64`，构建只在 `/tmp`；`BUILD_RC=0`，artifact sha `732fd6c6…`；与 0003 formal Go 1.25.12 对照：PT_TLS/STATIC_TLS/TPOFF64 count=1/res_search·pthread_create JUMP_SLOT/Go 导出均 same，artifact 字节 changed（预期）；`record_status: reviewed-pass`（独立复审 0 blocker/0 major/0 minor）、`verdict: research-only`；不构成 E1 pass，E8 保持 `CLOSED`
- [Debian 13 原 Linux worker：E1 v0.76.3 stock Go 单次重放交接](debian-e1-stock-go-handoff.md)
  - 交接文档已执行完毕：`EV-E1-EMU24-20260809-0003` 已于 2026-08-09 在恢复的历史 Linux worker 上完整重放（measured blocked/collected 为运行期 pre-review 状态；经终审 `REV-E1-EMU24-20260809-0003` 后为 `reviewed-pass/blocked`，见[0003 记录](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md)与[0003 终审记录](evidence/e1-stock-go-v0763-replay-0003-review-2026-08-09.md)）；ID 已消耗、禁止同 ID 重跑；证据审查已完成
- [E2 C 网络 API 24 Emulator 证据](evidence/e2-c-network-api24-emulator-2026-07-17.md)
  - 三个不同普通 `EntryAbility` PID 各完成 10 轮 TCP/UDP loopback、Pod 本机受控 endpoint、DNS、事件/错误和资源恢复验证，并显示可见 E2 PASS 页面
  - `record_status: reviewed-pass`、`verdict: pass`；E2 已关闭；当前R0正式基线（现v0.76.3）的 E1 已重跑 0003 为 `reviewed-pass/blocked`（`EV-E1-EMU24-20260809-0003`）且无 pass，E8 仍 `CLOSED`；该历史记录不授权当前物理预检
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
- [E3 S5 production layout 派生 fixture（2026-08-15）](evidence/e3-physical-preflight-production-layout-2026-08-15.md)
  - 当前 `settings-app-info` 规则已由 0004 sealed production layout 取代 `ADJ-20260808-0003` 当时的“无可信样本 / 无 ExpectedBundle / 不要求 `app-info-structure`”保守口径：必须有 Settings owner 下唯一可见 `Setting.AppDetail`，并在同一子树匹配 expected A/B distinct label 与 force-stop control；该历史登记保留原始事实、不回写。step4 post-force capture 是 observation-only，普通页面形状/capture 退化不独立判废，连续 HiLog stream degraded 仍按全局安全规则 blocked。
- [E3-PHYS-PREFLIGHT 物理设备预检计划与证据模板](e3-physical-preflight.md)
  - E8 前唯一物理 campaign 的治理边界、冻结输入、停止规则与历史证据模板
  - API23 initial 已消费；rebind 已完成；`ADJ-20260806-0003` 冻结 HarmonyOS 7/API 26 元组并曾准备 20260806 campaign ID；host reverify PASS；`ADJ-20260806-0004` 单条 build 确认 PASS；API26 live `E3-PHYS-PREFLIGHT-20260807-0001` 已 `consumed-blocked`（保留）；`E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` 已 Live 并 `consumed-blocked`（`reviewed-pass/blocked`；完整 1–7；双审查 0 B/5 M）；`ADJ-20260807-0003` 已批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径（runner/freeze/selftest 随执行 commit `e3fe0c6` 更新，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)）；**历史**（`ADJ-20260808-0002`/`0003` 后非当前规则）：S6 双 active 曾为 operator 三态确认（`NO-DUAL-ACTIVE-CAPTURED` 优先，false 时再独立 `DUAL-ACTIVE-CAPTURED`；仅 dual=true 且 noDual=false 为 fail，双 false/双 true 为 blocked），`ADJ-20260808-0002`/`0003` 后删除，改为机器判定唯一 A `CREATE_ACCEPTED` + 唯一 B `CREATE_REJECTED(2203002)`；S5 Settings>VPN 页 capture 曾为 observation-only（失败只写 `observation_only_degraded` 诊断，不进入全局 `capture_degraded`、不阻断 S5/overall），`ADJ-20260808-0003` 后 `settings_vpn_page_capture=not-required`、不再询问；`ADJ-20260808-0001` 已登记进程边界前瞻修复（[`进程边界前瞻修复`](evidence/e3-physical-preflight-process-target-2026-08-08.md)：S3/S5/S7 `PidOf` 目标改为 `<bundle>:vpn` Extension 进程 + `process_target`/freeze `process_probe_target`；UI last-known request；S4 deny 预截图；S5 探针实际 ≥3.0s；`operator-wait-state.json` 可轮询状态；aggregation 三态；只适用下一新 campaign/evidence，不回溯修改历史）；`ADJ-20260808-0002`/`0003` 已登记强可靠模式与执行细化（[`强可靠操作员信任模型`](evidence/e3-physical-preflight-operator-trust-2026-08-08.md)：`mechanical-action-only-machine-verified-v1` 单步回车、机器 layout/事件/冲突码判定、scenario invalid 停止后续、overall 优先级 integrity invalid > scenario invalid > fail > blocked > pass；`ADJ-20260808-0003`：真实 layout 校准、prompt-time 事件窗口、空档 UI action guard、infra capture 分类、S5 去除非决定性 VPN 页步骤、process-target verify；其中“App Info 真实结构尚未采样 / 无 ExpectedBundle / 无 `app-info-structure`”仅为当时历史口径，已由 [`2026-08-15 production layout`](evidence/e3-physical-preflight-production-layout-2026-08-15.md) 取代；只适用下一新 campaign/evidence，不回溯历史）；历史上 2026-08-10 用户曾显式授权**新**完整白名单 campaign（[`AUTH-E3-PHYS1API26-20260810-0002`](evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)：`authorization_status=granted`、`device_readiness=user-attested-ready`（用户就绪声明**不**替代机器 fresh confirmation）、`machine_fresh_confirmation=pending`、`plan_status=authorized-awaiting-windows-ready-freeze`；**取代**已消耗的 [`AUTH-E3-PHYS1API26-20260810-0001`](evidence/e3-physical-preflight-authorization-2026-08-10.md)，新候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`（旧 pair `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 因 sealed blocked 已消费不得复用；audit-1/audit-2 均 hash 记录）；新增**一次 host-prep `hdc list targets` 内存级窄例外**（恰一 token、设 process-scope `PHYS_1_TARGET`、不输出/持久化、不扩大 runner HDC 白名单）；attempt=initial/retry N/A、任一 gate 失败停止、不重试不换 ID；Live 前允许 runner+治理变更审查后提交/推送，campaign evidence 仍不提前提交；当前 Windows signing/build host `ready_for_commit_and_ordered_preflight`（工具/签名输入齐备；尚待未提交 clean final commit、新 pair audit-1/audit-2、全新 blocked/ready freeze、confirmation/review records、新 EvidenceRoot/RawRoot、process-scope `PHYS_1_TARGET` 映射；非全部 Live-ready），按序执行：同步 trusted refs/bundle → ID 审计① → blocked confirmation freeze 静态审查 → host-prep `hdc list targets` 映射 → 机器 fresh 确认 model/build → `ready` freeze draft（绑定 clean HEAD 与仓内 runner/freeze example/selftest SHA-256（以最终 commit 为准）及完整外部 hash，不得原地改旧 blocked freeze）→ 独立审查 record → 最终 `ready` freeze 绑定 review → ID 审计②（hash 记录）→ selftest `HDC_PROCESSES=0` → DryRun `is_evidence: false`/HDC0/integrity empty（DryRun 可 `blocked` freeze，Live 只 `ready`；`ready` DryRun/Live 强制 `machine_fresh_confirmation` + `independent_review_record` 双绑定）→ 独立 review-ready → 单次 Live；`blocked`/`fail`/`invalid`/no seal 后不得自动重跑（当前 AUTH 固定 initial，任何 retry 须新治理）；E8 `CLOSED`）
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
- [E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-17 · 0001，当前）](evidence/e3-physical-preflight-authorization-2026-08-17-0001.md)
  - `AUTH-E3-PHYS1API26-20260817-0001`：新 pair `E3-PHYS-PREFLIGHT-20260817-0001` / `EV-E3-PHYS1API26-20260817-0001`，attempt initial / retry N/A；当前 `blocked-awaiting-full-gates`、not ready，machine confirmation/review/freeze/DryRun/Live 均 pending；reviewer role exact 为 `isolated-anthropic-claude-opus-5-reviewer`；gate 1 禁止 HDC，gate 4 的 `tconn` 与 `list targets` 各只允许一次；HAP source basis `62409c5f...` 与最终 runner/governance `code_sha` 独立冻结；E8 `CLOSED`
- [E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-16 · 0003，governance-review-blocked-retired）](evidence/e3-physical-preflight-authorization-2026-08-16-0003.md)
  - gate 1-6 pass；gate 7 Opus 0 major / 1 blocker（历史 freeze role `isolated-static-reviewer` mismatch），未写 review record；gate 8-13 not-run，未 DryRun/Live、未 consumed，历史仓外对象字节保留且 pair 永久不可复用
- [E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-16 · 0002，governance-order-invalid-retired）](evidence/e3-physical-preflight-authorization-2026-08-16-0002.md)
  - 0002 在 gate 1 未完成时发生 gate 4 `list targets` 操作提前，输出未披露/未落盘；daemon 以 host process termination 清理并由进程表确认 HDC0，但命令级清理审计未持久化，不计作合规 gate 操作，亦不声称 device evidence 或具体清理命令；gate 2-13 not-run，仓外无 0002 audit/freeze/record/evidence/lock；未 Live、未 consumed，但 AUTH/pair 永久不可复用
- [E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-16 · 0001，gate-3 review-blocked retired）](evidence/e3-physical-preflight-authorization-2026-08-16-0001.md)
  - 0001 audit-1 与 blocked freeze 原字节保留；旧 source mismatch 导致 0B/1M、gate 4-13 not-run、非 Live consumed；AUTH/pair 永久不复用
- [E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-15 · 0005，sealed blocked consumed）](evidence/e3-physical-preflight-authorization-2026-08-15-0005.md)
  - 0005 AUTH/pair 已消费且不得复用；其 sealed evidence 与 S6 B production fixture 只保留历史/provenance
- [E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-15 · 0004，consumed sealed invalid）](evidence/e3-physical-preflight-authorization-2026-08-15-0004.md)
  - 0004 AUTH/pair 已消费，不得复用；production `强行停止` 样本只作为 0005 修复的 fixture provenance，不改写历史 evidence
- [E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-10 · 0002，historical）](evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)
  - `AUTH-E3-PHYS1API26-20260810-0002`：用户显式授权**新**完整白名单 campaign（`authorization_status=granted`），**取代** 已消耗的 `AUTH-E3-PHYS1API26-20260810-0001`；`device_readiness=user-attested-ready`、`machine_fresh_confirmation=pending`、`plan_status=authorized-awaiting-windows-ready-freeze`；新候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001` 为 `pending-two-consumption-audits`（audit-1/audit-2 均 hash 记录；旧 pair `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 因 sealed blocked 已消费不得复用）；当前 AUTH 固定候选 pair 与 `attempt=initial`（retry N/A；generic retry 分支不进入本路径，任一 gate 失败停止、不重试不换 ID，任何 retry 须新治理）；新增**一次 host-prep `hdc list targets` 内存级窄例外**（恰一 token、设 process-scope `PHYS_1_TARGET`、不输出/持久化、不扩大 runner HDC 白名单）；Live 前允许 runner+治理变更审查后提交/推送（覆盖旧禁令），campaign evidence 仍不提前提交，本登记任务禁提交/推送；允许范围仅现行 runner 白名单，禁额外 discovery/UDID/serial/`hidumper`/root/privileged/Go/NetBird/product
  - 门顺序新增 host-prep `hdc list targets` 映射（blocked confirmation freeze 静态审查之后、`-TargetBindingConfirm` 之前）：同步 trusted refs/bundle → ID 审计① → blocked confirmation freeze 静态审查（`independent_review_ready=true` 仅表示契约/角色静态就绪）→ host-prep `hdc list targets` 映射 → `-TargetBindingConfirm`（3 条白名单 target-binding 命令，仓外双文件 record + `.sha256` companion）→ `ready` freeze draft 绑定 record → 独立审查 record（`e3-ready-freeze-review`）→ 最终 `ready` freeze 绑定 review → ID 审计② → selftest `HDC_PROCESSES=0` → 同一 `ready` freeze DryRun `is_evidence: false`/HDC0/integrity empty（`ready` DryRun 强制 confirmation + review 双绑定，`blocked` DryRun 允许 `pending`）→ 审查 DryRun 且 freeze 字节不变 → 单次 Live
  - 仓内字节（当前工作区，执行前复算并绑定最终 commit 值）：runner `e1da598d…`、freeze example `5ea7acda…`、selftest `28c26b72…`；最终 commit/hash 在 Live 前审查后提交时冻结；非 live 授权登记，E3 open、E8 `CLOSED`
- [E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-10 · 0001，superseded/consumed）](evidence/e3-physical-preflight-authorization-2026-08-10.md)
  - （superseded/consumed：已被 `AUTH-E3-PHYS1API26-20260810-0002` 取代，旧候选 pair `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 因 sealed blocked 已消费不得复用，见上方 0002 条目）`AUTH-E3-PHYS1API26-20260810-0001`：用户显式授权完整白名单 campaign（`authorization_status=granted`）；`device_readiness=user-attested-ready`、`machine_fresh_confirmation=pending`（用户就绪声明不替代机器确认）、`plan_status=authorized-awaiting-windows-ready-freeze`；candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 为 `pending-windows-out-of-repo-consumption-audit`（执行前须两次候选 ID 消费审计并 hash 记录；发现任何 `execution_mode=live`/`is_evidence=true`/seal 记录占用则不得复用、停止等用户新 ID 授权）、不创建新 ID；当前 AUTH 固定候选 pair 与 `attempt=initial`（retry N/A；generic retry 分支不进入本路径，任何 retry 须新治理）；允许范围仅现行 runner 白名单，禁额外 discovery/UDID/serial/`hidumper`/root/privileged/Go/NetBird/product
  - `ADJ-20260810-0001` 已实现 runner `-TargetBindingConfirm`（host-governed 机器 fresh confirmation，解决 ready 前 catch-22）：互斥 + 必填仓外 `-ConfirmationRecord`；接受 `blocked`/`ready` confirmation freeze 但完整校验 freeze 结构/clean repo/`code_sha`/runner/HDC/外部输入 hash/`PHYS_1_TARGET` 单 token；真 HDC 仅按白名单 argv 执行 `Version`/`TupleModel`/`TupleBuild` 三次；不初始化 EvidenceRoot/RawRoot、不设 `is_evidence`、不消费 campaign/evidence ID；产出仓外双文件 record（JSON tmp + `.sha256` tmp 复算 + atomic move JSON + atomic move companion 作 completion marker；`record_kind=target-binding-confirmation`、`is_evidence=false`、`command_attempted`/`command_completed`（pass 均 3）、`target_redacted=true`；pre-record 门失败 exit1 无 record，probe/tuple/写失败 exit2 blocked）；Live 与 `ready` DryRun 强制 `machine_fresh_confirmation.status=pass` 与 `independent_review_record.status=pass` 双绑定（authorization_id/record_path+sha/内容一致 + `e3-ready-freeze-review` record 机械门），`blocked` DryRun 允许 `pending`（review 同规则：声明 `status=pass` 时同样完整校验，machine `pending`/缺省 而 review 声明 `pass` → 明确拒绝，review `pending` 允许）
  - 当前 Windows signing/build host preflight 复测 `ready_for_commit_and_ordered_preflight`（COMPUTERNAME=ALFADB-V-WIN、pwsh 7.6.4、HDC binary 定位+文件 hash 未执行、历史 freeze/签名 HAP/profile/cert 齐备、EvidenceRoot/RawRoot 存在、HDC 进程 0 未发设备命令；尚待未提交 clean final commit、新 pair audit-1/audit-2、全新 blocked/ready freeze、confirmation/review records、新 EvidenceRoot/RawRoot、process-scope `PHYS_1_TARGET` 映射；非全部 Live-ready）→ 按序执行：同步 trusted refs/bundle → ID 审计① → blocked confirmation freeze 静态审查（`independent_review_ready=true` 仅表示契约/角色静态就绪）→ `-TargetBindingConfirm` → 全新仓外 `ready` freeze draft 绑定 record hash（绑定 clean HEAD 与重建后的仓内 runner/freeze example/selftest SHA-256（以最终 commit 为准）及完整外部 hash，record 绑定稳定 `confirmation_contract_sha256`——完整 freeze contract 因 `preflight_inputs_frozen_at` 推进可不同，confirmation contract 必须两阶段字节相同；不得原地改旧 blocked freeze）→ 独立审查 record（`e3-ready-freeze-review`）→ 最终 `ready` freeze 绑定 review → ID 审计② → selftest `HDC_PROCESSES=0` → 同一 `ready` freeze DryRun `is_evidence: false`/HDC0/integrity empty（`ready` DryRun 强制 confirmation + review 双绑定）→ 审查 DryRun 且 freeze 字节不变 → 单次 Live；blocked/fail/invalid/no seal 后不得自动重跑（当前 AUTH 固定 initial，任何 retry 须新治理；confirmation record `is_evidence=false` 不构成 retry 依据）；非 live 授权登记，E3 open、E8 `CLOSED`
- [E3-PHYS-PREFLIGHT 进程边界前瞻修复（ADJ-20260808-0001）](evidence/e3-physical-preflight-process-target-2026-08-08.md)
  - host 侧方案/实现登记（非 live、非设备证据）：S3/S5/S7 `PidOf` 目标从 bundle UI 进程精确改为 `<bundle>:vpn` Extension 进程；`process_target`/freeze `process_probe_target`；UI last-known；S4 deny 预截图；S5 探针 ≥3.0s；`operator-wait-state`；aggregation 三态；只适用下一新 campaign/evidence，不回溯历史；不授权 Live/HDC；E3 open、E8 `CLOSED`
  - S4 人工确认/ACK 与 READY/nonce 等待字段被 `ADJ-20260808-0002`/`0003` 前瞻取代（新协议机器 layout + 机械回车；`operator-wait-state` schema v2；均为 historical）
- [E3-PHYS-PREFLIGHT 强可靠操作员信任模型（ADJ-20260808-0002 / 0003）](evidence/e3-physical-preflight-operator-trust-2026-08-08.md)
  - host 侧方案/实现登记（非 live、非设备证据）：用户选择强可靠模式；`operator_trust_model=mechanical-action-only-machine-verified-v1`；单步回车；机器 layout/事件/S6 冲突码 2203002；scenario invalid 停止后续为 not-run-due-to-invalid；删除人工三态/READY-ACK 作为当前规则；只适用下一新 campaign/evidence，不回溯历史；不授权 Live/HDC；E3 open、E8 `CLOSED`
  - `ADJ-20260808-0003` 已登记强可靠执行细化（**superseded-by-production-layout**：其中未采样 `settings-app-info` matcher 仅为历史口径，现行规则见 [`2026-08-15 production layout`](evidence/e3-physical-preflight-production-layout-2026-08-15.md)）：真实 layout 校准（C6 修订：API26 sealed raw 与仓库 EMU 样本均为顶层数组、每节点 `attributes/children`，`Test-CapturedLayoutProfile` 用通用精确 fact 正则匹配任意深度 `attributes.<field>`、不依赖自造 `window`/`resourceId`；entry 仍要求 ExpectedBundle + `start-vpn`/`stop-vpn` id/key；settings-app-info 仅 owner+精确 A 标签+Force Stop 控件，不要求 ExpectedBundle、**去掉**未采样 `app-info-structure` id/key 要求——A/B 同名不假 pass，A correctness 由 force-stop 后 A `:vpn` absent + bundle present 的 process effect gate 证明）、连续 capture infra 传播（`CampaignCapture.Degraded` 按 raw-hilog `category` 分类 → blocked + `hdc-usb-interruption`，绝不 invalid）、step 超时分类（UI action 缺失 invalid，正确 action 后平台 marker 缺失 blocked）、S6 A 可选 reauthorization（entry/authorization 判定 + 单步 Allow + 纯授权层结果 `authorization-outcome-unclassified` blocked，绝不 fail/invalid）、S6 非冻结 B 拒绝码 → blocked、prompt-time 事件窗口、空档 UI action guard、S5 去除非决定性 VPN 页步骤、process-target verify；App Info 真实结构尚未采样 → 下一 Live fail-closed 风险；只适用下一新 campaign，不回溯 0001
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
- [N0 原生客户端可行性门：决议、范围与兼容 Oracle](n0-native-client-feasibility.md)
  - 2026-08-09 T0+/T0 五席（fable-5、gpt-5.6-sol、kimi-coding/k3、MiniMax-M3、grok-4.5）Round 3 十条 `UNANIMOUS-SIGN`、用户批准的 N0 决议：方向 B「NetBird 行为兼容原生客户端的分阶段可行性验证」、A-E 处置、N0 范围/非范围、双轴验收、停止条件、E8 `CLOSED` 与后续 ADJ 条件、物理 E3 第一动作纪律
  - 固定 NetBird v0.76.3/f65f7b34 协议/行为/许可矩阵与 compat oracle：management/signal protobuf+gRPC、自定义 relay、NetBird ICE fork/conn state、WireGuard fork、routes/DNS/ACL/state；每项权威源码路径与许可边界（根 BSD-3，management/signal/relay/combined 目录 AGPLv3，法律效果待专业评估）；N0 全部不实现，只有单一 native WG core 的 C ABI
- [N0 native core host-preflight 记录](evidence/n0-native-core-host-preflight-2026-08-09.md)
  - `EV-N0-N0HOST-20260809-0001`：host-only 前置记录，`execution: not-run-host-preflight`、`record_status: collected`、`verdict: pass`（严格只表示 host-preflight，不是 N0 pass）；仓库 HEAD `2c567dc`、rustc 1.92.0/cargo 1.92.0/rustup 1.28.2、OHOS clang 15.0.4/SDK 6.1.1.125、BoringTun 0.7.1 crate checksum `15dd6a8a…`、发布包缺 LICENSE 文件注意、ffi-bindings 双 ABI 构建成功（NEEDED 仅 libc.so、14 个 C ABI）、device feature 因 socket2 0.4.10 IovLen blocked 且不 patch/不进入 N0
- [N0 native core Emulator 0001 consumed-failure 记录](evidence/n0-native-core-emulator-0001-consumed-failure-2026-08-10.md)
  - `EV-N0-EMU24-20260810-0001`：完整模式首次真实执行，guest 测量面 PASS（唯一 marker、NAPI 三项 1、aa host/guest0、HAP member 一致、arm64 compile-only；cleanup 旧检查只观察到 Emulator/qemu 与 10000/5555/8710 端口为空，旧 `hdc -m -s` matcher 不足以证明 hdc daemon 无残留，seal 卡住反证有设备阶段子进程持有 pipe 写端），但 runner 在 seal 的 `wait TEE_PID` 卡住并由执行会话 30min 终止；manifest 缺 final/seal 字段；`record_status: collected`、`verdict: fail`（runner defect after measurement）、ID 已消耗、绝非 N0 pass/reviewed-pass；unsealed raw 支持 positive guest observation 但不构成 accepted N0 pass；11 份 raw 哈希为提交时带外锚点、raw 未封印；双路只读独立审查（`anthropic/claude-opus-5` + `moonshotai/kimi-k2.7-code`）一致；E8 `CLOSED`/physical false；runner 已修复（O_APPEND transcript、seal 不等待、`pgrep -ax hdc`、daemonize 硬化、selftest 真实覆盖），不改变测量/判定/guard 语义、cleanup 残留检测按 fail-closed 收紧，下一 ID `EV-N0-EMU24-20260810-0002`；0001 不参与 0002 判定
- [N0 native core Emulator 0002 reviewed-pass 记录](evidence/n0-native-core-emulator-reviewed-pass-2026-08-10.md)
  - `EV-N0-EMU24-20260810-0002`：完整模式正式执行，全流程跑通、exit 0、seal 完整（六字段 `final_exit_code`/`run_status`/`fail_reason`/`transcript_final_bytes`/`transcript_final_sha256`/`manifest_sha256` 齐备且复算一致）；真实官方 API 24 x86_64 phone Emulator（非物理设备）；唯一 PASS marker（`keyLen=44`/`tickOp=0`/`tickSize=0`）、NAPI 三项 1、aa host/guest 0、HAP member 一致、arm64 compile-only；`record_status: reviewed-pass`、`verdict: pass`（终审 `REV-N0-EMU24-20260810-0002`：`openai/gpt-5.6-sol` + `xai/grok-4.5` 双路只读独立复算 0 blocker/0 major，分别 2/6 minor，均为非阻塞记录改进建议、不扩范围修）；**N0 overall `reviewed-pass/pass`**；能力接受使用固定正式发布（NetBird v0.76.3/f65f7b34、BoringTun 0.7.1）与真实 official Emulator runtime 数据，不是合成 fixture；不证明 VPN fd/TUN/protect/management/ICE/relay/UI/arm64 load/physical/product；下一步不是继续实现（仅未来用户显式授权后 `E3-PHYS-PREFLIGHT` 第一物理动作，N0+E3 pass 后才新 ADJ/T0）；E8 `CLOSED`/physical false；ID 已消耗、禁止同 ID 重跑；11 份 raw 哈希与 seal 复算已核对
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
  - 当前R0正式基线（现v0.76.3）、版本门、阶段依赖、受控并行工作和完整完成定义
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
  - 固定 Tailscale-OHOS SHA 与 NetBird v0.76.3 release/commit 源码映射（v0.74.7 历史基线仍保留为既有审计历史事实）
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

- R0 唯一决策源、当前未退出状态、Emulator 客观可执行项总门、唯一 `E3-PHYS-PREFLIGHT` 例外、当前R0正式基线（现v0.76.3）、v0.74.6/v0.74.7 历史 evidence 绑定、范围、补丁预算、初始 SLO 和角色责任。
- 证据 ID、必填字段、状态枚举、脱敏规则、支持矩阵、动态调整、补丁记录和保留期。
- N0 原生客户端可行性门已双轴验收：N0(a) 固定 NetBird v0.76.3/f65f7b34 协议/行为/许可 inventory 与 compat oracle 已定义；N0(b) 由 `EV-N0-EMU24-20260810-0002` 实测 `reviewed-pass/pass`（终审 `REV-N0-EMU24-20260810-0002`，0 blocker/0 major）；N0 overall `reviewed-pass/pass`。能力接受使用固定正式发布与真实 official Emulator runtime 数据，不是合成 fixture；不证明 VPN fd/TUN/protect/management/ICE/relay/UI/arm64 load/physical/product；下一步不是继续实现（仅未来用户显式授权后 `E3-PHYS-PREFLIGHT` 第一物理动作，N0+E3 pass 后才新 ADJ/T0）；E8 仍 `CLOSED`。
- E0 普通应用已在 API 24 x86_64 Emulator 完成三次独立 PID 冷启动、可见 UI、生命周期/Node-API marker、停止、sidecar、卸载和残留清理，独立审查结果为 `reviewed-pass`、`verdict: pass`；E0 已关闭。
- E1 C-only 子门已由普通 `EntryAbility` 在三个不同 PID 各完成 10 轮：每 PID 1000 个同步 guarded buffer、1000 个 C pthread 到主 ArkTS 上下文的公开 threadsafe callback、10 次真实 fd ownership，以及逐轮 `/proc/self/fd`/线程快照；记录为 `reviewed-pass/pass`。独立审查确认 0 blocker/major、5 minor，且不改变 measured artifact。v0.74.6 历史官方 Go 1.25.12 loader 负面保持原绑定；当前R0正式基线（现v0.76.3）由 `EV-E1-EMU24-20260809-0003` 实测 `reviewed-pass/blocked` 且没有 E1 pass。
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

- N0 pass 不改变以下未完成事项：N0 只接受固定正式发布（NetBird v0.76.3/f65f7b34、BoringTun 0.7.1）与真实 official Emulator runtime 数据的单一 native WG core C ABI 加载/冒烟，不证明 VPN fd/TUN/protect/management/ICE/relay/UI/arm64 load/physical/product，也不改变 E8 `CLOSED` 与下述未完成项。
- API 24 x86_64 Emulator 客观可执行项总门已建立但尚未通过；当前总门为 `CLOSED`。当前R0正式基线（现v0.76.3）的 E1 已重跑 0003 为 `reviewed-pass/blocked`（`EV-E1-EMU24-20260809-0003`）且无 pass。API23 initial live 已登记为 `EV-E3-PHYS1API23-20260806-0001`（`reviewed-pass/blocked`）。rebind `EV-E3-PHYS1REBIND7-20260806-0001` 已完成（`reviewed-pass/pass`，仅 rebind；API `26`/`aarch64`/`arm64-v8a`）。`ADJ-20260806-0003` 冻结 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` / Settings `7.0.0.100 (SP8C00E32R7P2patch09)` / API `26` 元组，批准复用原 FINAL HAP hashes（兼容性实测、不证明成功），并曾准备 `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001`（`superseded-unexecuted`）；host reverify 已 PASS。`ADJ-20260806-0004` / `EV-E3-PHYS1BUILD7-20260806-0001` 单条 `software.version` 实测确认 HDC build 逐字匹配（`reviewed-pass/pass`，仅 build-confirm）；API `26` 仍 rebind 实测、不从 build 推断。API26 live `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 已 `consumed-blocked`（operator-aborted；禁局部 scenario5 重放；保留）。`E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` 已 Live 并 `consumed-blocked`（`reviewed-pass/blocked`；完整 1–7；双审查 0 B/5 M）。当前 2026-08-10 用户已显式授权**新**完整白名单 campaign（[`AUTH-E3-PHYS1API26-20260810-0002`](evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)：`authorization_status=granted`、`device_readiness=user-attested-ready`、`machine_fresh_confirmation=pending`、`plan_status=authorized-awaiting-windows-ready-freeze`；**取代**已消耗的 [`AUTH-E3-PHYS1API26-20260810-0001`](evidence/e3-physical-preflight-authorization-2026-08-10.md)，新候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`（旧 pair 因 sealed blocked 已消费不得复用）；新增一次 host-prep `hdc list targets` 内存级窄例外（恰一 token、设 process-scope `PHYS_1_TARGET`、不输出/持久化）；attempt=initial/retry N/A、任一 gate 失败停止、不重试不换 ID；Live 前允许 runner+治理变更审查后提交/推送，campaign evidence 仍不提前提交；当前 Windows signing/build host `ready_for_commit_and_ordered_preflight`（工具/签名输入齐备；尚待未提交 clean final commit、新 pair audit-1/audit-2、全新 blocked/ready freeze、confirmation/review records、新 EvidenceRoot/RawRoot、process-scope `PHYS_1_TARGET` 映射；非全部 Live-ready），按序执行：同步 trusted refs/bundle → ID 审计① → blocked confirmation freeze 静态审查 → host-prep `hdc list targets` 映射 → 机器 fresh 确认 model/build → `ready` freeze draft 绑定 record → 独立审查 record（`e3-ready-freeze-review`）→ 最终 `ready` freeze 绑定 review → ID 审计②（hash 记录）→ selftest/DryRun/独立 review-ready/单次 Live；用户就绪声明不替代机器 fresh confirmation；`blocked`/`fail`/`invalid`/no seal 后不得自动重跑（当前 AUTH 固定 initial，任何 retry 须新治理）；本登记禁 HDC（host-prep 一次 `hdc list targets` 除外）；`ADJ-20260807-0003` runner 变更完成，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)；旧 candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 因 sealed blocked 已消费）；E8 仍 `CLOSED`。
- R0 已退出，或具名真机、完整目标元组、签名和华为应用市场闭环已经就绪。
- 威胁缓解已经实现，或依赖锁定、SBOM、漏洞审查和最终许可证合规已经完成。
- `/dev/dri`/图形模式或 Emulator gRPC 已经验证。
- 面向产品的 OpenHarmony 或 HarmonyOS 应用工程已经建立；当前只有不得演化为产品壳的短生命周期 E0/R1 API 24 探针和独立 E3 A/B 授权探针。
- NetBird Go 核心、当前R0正式基线（现v0.76.3）的官方 Go 1.25.12 loader、完整 E1 或 VPN 能力已经完成集成验证；现有 ArkTS/native/fd 正面只限 C-only 子证据。
- VPN Extension 授权门已经通过，或已在 Emulator/物理设备建立隧道；phone 公开 runtime blocked，Tablet/2in1 只在 registration-layer 前置边界 blocked。API23 物理预检 initial 为 `reviewed-pass/blocked`（安装前停止）；API26 0001/0002 live 均为 `consumed-blocked`（0001 operator-aborted；0002 完整 1–7 blocked）。`ADJ-20260807-0003` 已批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径。当前 2026-08-10 用户已显式授权**新**完整白名单 campaign（`AUTH-E3-PHYS1API26-20260810-0002`，见[授权登记 0002](evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)；取代已消耗的 `AUTH-E3-PHYS1API26-20260810-0001`），`plan_status: authorized-awaiting-windows-ready-freeze`；新候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`，旧 pair 因 sealed blocked 已消费不得复用；新增一次 host-prep `hdc list targets` 内存级窄例外（恰一 token、设 process-scope `PHYS_1_TARGET`、不输出/持久化）；attempt=initial/retry N/A、任一 gate 失败停止、不重试不换 ID；用户就绪声明不替代机器 fresh confirmation，机器 fresh 确认完成前不得 `ready`/Live；`blocked`/`fail`/`invalid`/no seal 后不得自动重跑（当前 AUTH 固定 initial，任何 retry 须新治理）；禁止私有 Go、NetBird、`MANAGE_VPN` 或 system/debug/enterprise/root 绕过。
- 任一市场的正式签名、审核、上架或更新流程已经跑通。
- Debian 13 已成为官方支持的 Emulator 宿主。

## 维护约定

新增调查结果时，应保留来源 URL，并把结果归入上述四种信息状态之一。环境现场发生变化时，优先更新“当前实测”和“尚未验证”；SDK 或官方文档发生变化时，同时更新核验日期。平台方案变更时，应分别说明 OpenHarmony 与 HarmonyOS 的影响，避免把一侧的测试结果直接推广到另一侧。
