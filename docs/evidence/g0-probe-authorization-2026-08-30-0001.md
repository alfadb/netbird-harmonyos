# G0 stock Go arm64 loader 物理探针执行授权登记（2026-08-30 · 0001）

最后核验：2026-08-30

本文登记用户（直接人类决策者）于 2026-08-30 的两次显式治理决定：① 路线选择「Go arm64 物理探针先行」；② 授权 [G0 探针计划](../g0-go-arm64-physical-probe.md) 按草案 ID 与规格进入实现阶段。本登记建立 `AUTH-G0PHYS1API26-20260830-0001`、campaign `G0-PHYS-PROBE-20260830-0001` 与 evidence `EV-G0PHYS1API26-20260830-0001`。这是全新 `attempt: initial`，无 retry；E3-PHYS-PREFLIGHT 已 `consumed-pass` 完结，本 AUTH 与其无继承关系、不复用其任何已消费对象。

## 依据

- N0 决议第 7 条（2026-08-09 五席 T0 `UNANIMOUS-SIGN`、用户批准）：E3 预检后、优先于任何 Go1.26 research，测 NetBird 正式工具链 Go1.25.12 arm64 c-shared 最小探针；结果可触发路线重议，**不是 E1 pass 的自动替代**。
- `EV-E3-PHYS1API26-20260829-0001`（`reviewed-pass/pass`，2026-08-30）：第一物理动作已履行并完结。
- 用户 2026-08-30 直接决策两次（路线 + 计划授权），按「人类直接决策者优先」规则替代这一次内部 T0 触发，**不声称执行了 T0**。

## 授权状态

```yaml
authorization_id: AUTH-G0PHYS1API26-20260830-0001
exception: G0-GOARM64-LOADER-PROBE
information_status: current-governance-registration
record_status: consumed-blocked-final
stage_or_gate: G0
related_stages_or_gates: [E1, E8, N0]
is_evidence: false
authorization_status: consumed
plan_status: consumed-blocked
device_readiness: not-yet-requested
machine_fresh_confirmation: pending
attempt: initial
retry: N/A
candidate:
  campaign_id: G0-PHYS-PROBE-20260830-0001
  evidence_id: EV-G0PHYS1API26-20260830-0001
  identity_status: consumed-blocked
  consumed: false
  reusable: false
target_tuple: 继承 EV-E3-PHYS1REBIND8-20260828-0001 四条只读实测（2026-08-28），本 AUTH 不重测
reviewer_role: 待 freeze 重新绑定（跨厂商 isolated reviewer）
```

## 范围声明（硬边界）

- 本探针只回答：冻结物理元组的动态 loader 是否接受 stock（零补丁）Go 1.25.12 arm64 c-shared，以及最小 Go runtime 冒烟是否可用。
- **不是 E1 pass**（E1 正式范围为 API 24 x86_64 Emulator，其 `reviewed-pass/blocked` 不改写）；**不是 E8 输入**（E8 保持 `CLOSED`）；不是产品实现开启；结果仅作为已触发的 native N1-Nx + E8 Go 前提处置 ADJ/T0 治理的实测输入。
- 探针内容仅限：单一普通应用 HAP（无 permission、无 Extension、无网络）、arm64 `libgoprobe.so`（stock Go 最小探针，导出 `Hello`/`RuntimeProbe`）与 `libgoloader.so`（纯 C NAPI dloader）。禁止 NetBird/WireGuard 代码、VPN API、`MANAGE_VPN`、system/debug/enterprise/root、隐藏 API、自动设备输入、外部 endpoint、任何 Go/NetBird 补丁（补丁计数保持 0）。
- HDC 白名单恰 15 项操作 + gate 4 一次 `tconn`/一次内存级 `list targets`，逐字清单见[计划文档](../g0-go-arm64-physical-probe.md)白名单一节；gate 4→gate 5 顺序纪律与 E3 收官登记一致（先 gate 4 后 confirmation；confirmation record 路径 single-use immutable；元组漂移即 blocked record 退役）。
- 双向不外推：结果只覆盖冻结元组，不外推其他设备/build/API/架构/Emulator；loader 拒绝（blocked）与接受（pass）均为有效终态实测，均消费本 pair。

## 完整门序列（13 门）

1. host-only：同步 trusted refs/bundle；核对 exact clean HEAD 含本登记、计划文档、spike 源码、runner（Python/PowerShell parity）、selftests、freeze example；记录 `code_sha`；HDC0 用固定绝对 host `ps` 探针第一列。
2. 候选 ID 消费审计 audit-1（仓外双文件 + `.sha256`）。
3. 新建 blocked confirmation freeze + exact reviewer 静态审查。
4. 用户本地恰一次 `tconn <runtime-endpoint>` + 恰一次内存级 `list targets`（本人执行或显式授权主会话代执行；token 不输出不持久化）。
5. `-TargetBindingConfirm`（3 白名单探针；漂移即 blocked record + 退役）。
6. ready freeze draft 绑定 confirmation record。
7. reviewer 生成 `g0-ready-freeze-review` record（0 blocker/0 major）。
8. 最终 ready freeze（绑定 clean HEAD、runner bytes、signed HAP/profile/cert/ELF 成员 hash、全部外部输入）。
9. 候选 ID 消费审计 audit-2（严格 host-only）。
10. host-only Python + PowerShell selftests（`HDC_PROCESSES=0`；marker 正反例、ELF 剖面断言、fake-hdc 沙箱）。
11. 同一 ready freeze DryRun（`is_evidence=false`、HDC0、integrity empty）。
12. 独立审查 DryRun，复算 freeze SHA-256，确认字节不变。
13. 单次 Live（`PYTHONUNBUFFERED=1`，按证据目录增量与状态文件时间监控；不 retry、不换 ID）。

## 签名链（新 App ID；enrollment 保持已消耗）

AGC 新建 App ID `cn.alfadb.netbird.g0probe`；新 Debug profile 勾选**已注册的 PHYS-1 设备**与该 App ID（设备已在 AGC，**不读取 UDID**；2026-07-18 单次 enrollment 保持已消耗）。复用既有 `.p12`/`.csr`/Debug `.cer`；构建/验签/回传按 [Windows 开发交接](../windows-development-handoff.md)模板；signed 内容审计：无 permission、无 Extension、唯一 arm64 成员为 `libgoprobe.so`+`libgoloader.so`。

## 预登记 ELF 剖面（2026-08-30 host 可行性构建实测）

stock Go 1.25.12 arm64 c-shared（host launcher `go version go1.25.12 linux/amd64`、`GOTOOLCHAIN=local`、`GOOS=linux GOARCH=arm64 CGO_ENABLED=1`、CC 为 SDK `aarch64-unknown-linux-ohos-clang`、`-trimpath -buildmode=c-shared`）：`PT_TLS` 恰一段（memsz `0x10`）；`R_AARCH64_TLS_TPREL64` 恰 1 条且 symbol index `0`（本地）；无 `STATIC_TLS` flag；`NEEDED` 仅 `libc.so`；`FLAGS_1=NOW|NODELETE`；导出 `Hello`/`RuntimeProbe`。可行性构建 SHA-256 `a39efc8d1ada624cc4f9e71bed7e688204e88a1925d85c86654eb3e6fe0f1760`（`/tmp` 一次性产物，**非 campaign 输入**；正式构建在基准 commit 下重做并逐项重核）。与 x86_64 被拒对照（`EV-E1-EMU24-20260809-0003`：`STATIC_TLS`+`R_X86_64_TPOFF64` 被 API 24 x86_64 Emulator musl loader 以 `initial-exec TLS resolves to dynamic definition` 拒绝）剖面实质不同；接受与否只由物理实测判定。

## 当前 host-only 边界

当前只允许：本登记与计划文档、spike 源码实现、runner（Python/PowerShell parity）与 selftests、freeze example、host-only 构建/ELF 断言/selftest 验证、独立审查，以及审查通过后的提交/推送。禁止：任何真实 HDC executable、设备命令、新 pair audit/freeze/record、TargetBindingConfirm、DryRun、Live（gate 4 起逐门推进且 Live 前须用户再次确认）。fake-hdc 只允许存在于 selftest 临时沙箱。

> **收官（2026-08-30）**：13 门全部执行完毕，Live 实测 `verdict: blocked`（`dlopen-blocked`，loader 拒绝逐字登记），证据 [`EV-G0PHYS1API26-20260830-0001`](g0-probe-live-2026-08-30-0001.md) `reviewed-pass`。本 AUTH `consumed-blocked`，pair 不可复用，无后继 AUTH。结果作为 native N1-Nx + E8 Go 前提处置 ADJ/T0 的实测输入（与 N0 pass 并列）。

## 门序列执行登记（2026-08-30）

| 门 | 状态 | 事实 |
| --- | --- | --- |
| 1 | pass（Windows） | HEAD `df0d90c` clean、基线双祖先、四文件在位、冻结制品 hash 一致、code_sha 记录；HDC0 探针发现 `/usr/bin/ps` 硬编码不可解析（→探针 OS 自适应补丁，见变更账本），Windows 侧 `hdc kill` 后 tasklist 计数 0 |
| 2 | pass（Windows；执行宿主重做） | audit-1：`outside-hits=0 inside-evidence-hits=0`，记录+companion `ee6f819f…`（code_sha `df0d90c`）；执行宿主迁移后在本机重做（记录 `6732c328…`，code_sha `6f82d21`，note 引用 Windows 原记录；两次审计之间无任何 ID 消费） |
| 3 | pass（v2） | Windows 版 freeze（`3b7aa373…`）经静态审查 pass 后未消费，因执行宿主迁移标记 superseded；本机 v2 freeze（`5f7d26db…e61de` + `.sha256` 伴生）经 runner 真实 `load_freeze` dry-run ACCEPTED + 增量静态审查 pass（F1 records/ 整改、F2 差异清单口径更正：v1→v2 另含 `runner_ps1_sha256`/`selftest_ps1_sha256` 因 `01d7296` 探针补丁、F3 freeze 伴生文件补齐） |
| 4 | pass（用户授权主会话代执行） | 恰一次 `tconn`（exit 0）+ 恰一次内存级 `list targets`（恰一 target 且匹配；token 未输出、未入库、未入任何证据对象；用户作为直接决策者将 endpoint 置于会话并显式授权代执行） |
| 5 | pass（主会话代执行） | `-TargetBindingConfirm` 三探针 3/3：期望=实测=`PLA-AL10` / `PLA-AL10 7.0.0.102(SP8C00E102R7P3)`（逐字）；记录+companion `3a16ca1a…419`（`target_redacted=true`、`is_evidence=false`、code_sha `6f82d21`）；随后 `hdc kill` 清理 server，HDC0 确认 |

**微修订（Live 前重连，2026-08-30 登记）**：gate 10 的 HDC0 selftest 要求与 gate 13 Live 的设备连接在单一 hdc server 生命周期内互斥（tconn 的连接随 server 终止而丢失）。据此修订：gate 4 的「恰一次 tconn」限定为**进入证据链前的绑定确认**；gates 6-12 在 HDC0 干净环境执行；**gate 13 Live 启动前**由用户本人（或再次显式授权主会话）从设备屏幕读取当时动态 endpoint 并执行恰一次新 `tconn`，随后的 `list targets` 校验与 Live 连续执行。该重连不产生新的元组绑定（元组已由 gate 5 封存）；若 Live 前重连时 `list targets` 元组探针显示漂移，立即停止并退役本 pair。

## 提交边界

实现完成、host-only 验证全部通过并取得独立审查 0 blocker/0 major 后，方允许 commit/push（含本登记与全部实现产物）；campaign evidence 仍不提前提交。（该边界已履行：实现与治理登记随 `1d31835`/`bdcb7aa`/`01d7296`/`6f82d21` 提交推送。）

## 独立审查链（2026-08-30，isolated-anthropic-claude-opus-5-reviewer 三轮）

| 轮 | 范围 | 结论 | 处置 |
| --- | --- | --- | --- |
| 1 | 全变更集盲审 | **fail**：2 blocker（py `CampaignBlocked` 缺 reason 属性致 blocked 路径崩溃无 seal；ps1 marker 键大小写不敏感哈希表致 fail-open）+ 2 major（ps1 数组解包破坏证据 parity；selftest 零覆盖）+ 8 minor | 全部 blocker/major 修复；顺手修 minor 1/3/4/5（fault 状态、文档 argv 前缀、ELF hash 说明、build 断言增至 9 项）；selftest 39→41 |
| 2 | 回归复审 | **pass**（0B/0M）：四发现独立验证全 fixed（自建 freeze 端到端、12 例大小写矩阵、跨 runner 全字段 parity、ELF 断言变异测试）；新 3 minor | NEW-MINOR-1（`runner_ps1_sha256` 空串逃生门）+ 两项文案 minor 当轮修复：两侧 runner 四实现 hash 强制重算、scenario-results 补 `runner_ps1_sha256`、selftest 42/42（含修复一个隐性 `[string]$null`→`''` 构造器 bug）；round-2 MINOR-2/6/7/8 与 round-3 MINOR-A/B 保留为记录项不阻塞 |
| 3 | 增量审查（NEW-MINOR 修复面） | **pass**（0B/0M）：12 组 hash 变异 × 双 runner CLI 子进程全部拒绝；正确值双侧 DryRun 接受且记录双 hash；死代码 0 残留 | 达到提交门槛，按提交边界 commit/push |
| 5-6 | freeze v2（执行宿主迁移）与 ready 链 | **pass**：v2 经 runner 真实 load_freeze ACCEPTED + 增量静态审查（F1 records/ 整改、F2 口径更正、F3 伴生补齐）；gate 7 review record 由 isolated-xai-grok-4-6-reviewer 出具（anthropic 席两次执行中断后按跨厂商换席处理，0B/0M/0m） | final ready freeze `89b5cce5…` 生效 |
| 7 | DryRun（gate 12）与 live 证据记录审查 | **pass（两轮）**：DryRun 全链独立复算 0B/0M/0m；live 证据记录第一轮 fail（1 major：faultlogger 表述越权，已更正）→ 第二轮 pass 0B/0M，定级 `reviewed-pass`、`verdict: blocked` 维持 | 证据定级与措辞更正随 `a52723f` 提交 |
| 4 | 增量审查（host HDC0 探针 OS 自适应补丁） | **pass**（0B/0M/4 minor）：Linux 分支字节级未变（md5 相同）；Windows tasklist 分支 9 场景进程级实测全部失败路径返回 -1、无 false-pass；15 例解析器边界通过；双侧 42/42 用例名集合一致 | minor 1/4（头注释漂移、死守卫）当轮修复；minor 2/3（本行 hash 快照更新 + 变更账本登记）随本登记落实 |

**变更账本（2026-08-30 · host HDC0 探针 OS 自适应）**：gate 1 实测发现 `.ps1` runner 沿用 E3 的 `/usr/bin/ps` 硬编码探针在 Windows 执行主机不可解析。补丁将 `Get-G0HdcProcessCount` 改为 OS 分支：Linux 侧 `/usr/bin/ps -eo comm=,args=` **逐字保留**；Windows 侧新增绝对 `%SystemRoot%\System32\tasklist.exe /FO CSV /NH` 首列（image-name）计数（`hdc`/`hdc.exe` 大小写不敏感），语义与 E3 探针等价（绝对路径、只比第一列、argv 不可能误匹配——tasklist CSV 结构上无 argv 列）。**本变更只扩大 host 侧只读进程列表探针的 OS 覆盖，不新增任何设备侧能力，不触碰 15 项 HDC 操作白名单**；计划文档白名单节（本 AUTH 按引用绑定的那一节）已同步更新。经第四轮聚焦独立审查 pass 后提交。

提交时实现产物 sha256（`spikes/g0-go-arm64-phys-hap/` 下）：runner py `e12f6387…a44f2`、runner ps1 `55b7cde0…b78cd`（含探针补丁与头注释同步）、selftest py `729b69a8…c558b`、selftest ps1 `cd620ae6…d8f1`、fake-hdc `962d28be…c2d9f`；正式 freeze 于 gate 3/6/8 现场创建时按上述强制重算规则绑定（工作树演进后 hash 随之重算，非静态值）。
