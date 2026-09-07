# N1BDISC 候选三 ID 分配与候选本地提交授权登记（治理命名 2026-09-06 · 0002，host-only，live-candidate 工作区）

最后核验：2026-09-07

本文登记用户（直接人类决策者）本次会话的显式治理决定（question_id=`n1bdisc-candidate-new-ids-local-commit`，选择「授权 ID 分配与候选本地提交（Recommended）」）：为 live-candidate 候选工作区分配 N1BDISC 新三 ID 并授权候选本地提交。**治理命名日期在授权时固定为 `20260906-0002`，授权后不随实际落笔或执行日期变化**；本文实际 generated_at 为 2026-09-07 08:29:48 CST（`date` 实测，与命名日区分）。据此建立 `AUTH-N1BDISC-PHYS1API26-20260906-0002`、campaign `N1BDISC-PHYS1API26-20260906-0002` 与 evidence `EV-N1BDISC-PHYS1API26-20260906-0002`。这是全新 `attempt: initial`，无 retry；本文仅登记治理 ID 分配（allocated/unused，candidate 态，`consumed=false`/`reusable=false`，`is_evidence=false`），**任何门均未执行**。本文沿既有 AUTH 登记结构，不复制旧 pair 门执行记录，不宣称旧 confirmation 有效。

## 依据

- **用户本次精确批准**（question_id=`n1bdisc-candidate-new-ids-local-commit`）：授权范围为——上述三 ID 分配 + `n1bdisc/live-candidate` 分支候选 local 提交 + 本登记落盘；**不含**合并 main、push、设备/tconn/`list targets`/参数查询、签名、新 freeze 执行、Live。
- **实现阶段最终聚焦复核**：grok 席（agent `06c4431c-60f0-4090-a0a0-7eaea4bf818f`）——原 blocker/major 消解，204 tests 通过，可进入新 ID 申请与候选冻结准备；**不构成新 freeze 成立或 Live 授权**。
- **候选实现资产**：`spikes/n1b-disc-phys-hap/` 全套候选改动（未提交），工作区 `/home/worker/work/base/netbird-harmonyos-live-candidate`（独立 worktree，分支 `n1bdisc/live-candidate`），base `d4bb2ef7839877759c9641744e9d1a2b2992a94e`（= `main` 当前 HEAD）；**`code_sha` 不在本文填写，待 local commit 后由后续 gate 1 绑定**。候选现状与测试证据见 [n1bdisc-live-candidate-status.md](../n1bdisc-live-candidate-status.md)。
- **判据基线（引用，不变更）**：[N1BDISC 判据](../n1b-disc-gate-plan.md)，冻结基线 git `04cf222`（2026-09-02）+ CC-1（2026-09-05）+ CC-2（2026-09-06 元组重绑）；本登记不变更判据。
- **前两对 pair 终态（不继承、不复用、不撤销）**：
  - 首对 `AUTH-N1BDISC-PHYS1API26-20260905-0001`：gate 5 元组漂移，blocked record + 退役（`consumed-blocked-final`），登记 [n1bdisc-authorization-2026-09-05-0001.md](n1bdisc-authorization-2026-09-05-0001.md)。
  - 第二对 `AUTH-N1BDISC-PHYS1API26-20260906-0001`：用户按 exact-once 违规明确退役；退役记录 `/home/worker/harmonyos-signing/netbird-n1bdisc/reviews/pair2-retirement-exact-once.json`（SHA-256 `5dfd19c39ef45780ad1bb6e42a6afb77d886851f3d9d7ebab1e25bcda0e8c652`；本登记落笔时复算一致），原登记 [n1bdisc-authorization-2026-09-06-0001.md](n1bdisc-authorization-2026-09-06-0001.md)。
  - 旧 confirmation / ready-freeze 仅作历史审计对象，**不可继承其效力**；本对全部门未执行。
- **先前 host-only 开发授权继续可引用**（host-only 验证、构建、selftest），但本登记不自动扩大至正式门流程。

## 授权状态

```yaml
authorization_id: AUTH-N1BDISC-PHYS1API26-20260906-0002
campaign_id: N1BDISC-PHYS1API26-20260906-0002
evidence_id: EV-N1BDISC-PHYS1API26-20260906-0002
exception: N1BDISC-DISCOVERY-CAMPAIGN
information_status: current-governance-registration
record_status: active-governance-registration # 活跃治理登记（非证据记录本体）；执行后另立 EV-N1BDISC 证据记录
stage_or_gate: N1BDISC
related_stages_or_gates: [N1B]
execution: not-started # 无门执行、无测量、无 HDC/设备命令、无 DryRun/Live
is_evidence: false
authorization_status: granted-id-allocation-and-candidate-local-commit # 三 ID 分配 + 候选本地提交授权；schema 无此字段值集，沿先例治理 kebab 值（见下方值域依据注）
plan_status: id-allocated-local-commit-authorized-not-executed # 三 ID 已分配未执行；候选 local 提交已授权待执行；门序列未开始
criteria_freeze: git-04cf222-plus-cc-1-plus-cc-2-reviewed-pass-2026-09-06 # 判据基线引用，本登记不变更
device_readiness: not-yet-requested
machine_fresh_confirmation: not-yet-requested
attempt: initial
retry: N/A
candidate:
  campaign_id: N1BDISC-PHYS1API26-20260906-0002
  evidence_id: EV-N1BDISC-PHYS1API26-20260906-0002
  identity_status: candidate # 候选态（未消费），未来 gate 2/9 消费审计的受检对象
  consumed: false # 本登记不消费任何 ID；local commit 是候选工作区提交动作，不是门执行
  reusable: false
governance_naming_date: "20260906-0002" # 授权时固定，不随实际执行日期变化
generated_at: "2026-09-07 08:29:48 CST" # date 实测
workspace: /home/worker/work/base/netbird-harmonyos-live-candidate # 独立 worktree，分支 n1bdisc/live-candidate
candidate_base: d4bb2ef7839877759c9641744e9d1a2b2992a94e # 候选 base（= main 当前 HEAD）
code_sha: pending-gate-1-binding # 待 local commit 后由后续 gate 1 绑定；本文不填造
target_tuple: HarmonyOS / PLA-AL10 / PLA-AL10 7.0.0.105(SP6C00E105R7P3) / API 26 / aarch64 / arm64-v8a # 判据 :268 CC-2 重绑值（登记引用；设备当前状态不推断，见「设备状态与未来确认要求」节）
bundle_name: cn.alfadb.netbird.n1bdisc # 判据 :270 冻结值，逐字一致
reviewer_role: 待未来 gate 3/7 freeze 重新绑定（跨厂商 isolated reviewer）
```

> **YAML 值域依据**：[`evidence-schema.md`](../evidence-schema.md) **未定义** `authorization_status` 与 `plan_status` 的合法值集（全文无此二字段）；`record_status` 七值（:60-71）针对证据记录本体，本治理登记不占用。本登记沿前两对先例（[首对授权登记](n1bdisc-authorization-2026-09-05-0001.md)、[第二对授权登记](n1bdisc-authorization-2026-09-06-0001.md)，各带同款「YAML 值域依据」注）采用治理登记 kebab 值：`granted-id-allocation-and-candidate-local-commit` / `id-allocated-local-commit-authorized-not-executed` / `active-governance-registration`，取值字面表达「三 ID 分配 + 候选本地提交授权、执行未开始、登记活跃」。`governance_naming_date` / `generated_at` / `workspace` / `candidate_base` / `code_sha` 为本登记新增治理事实字段（schema 未定义，注释化治理，沿先例做法）。schema 其余实际值集均针对证据记录本体，本登记不占用：信息状态四值（:11-17）、`verdict` 四值 `pass | fail | blocked | invalid`（:76-85、:160）；未来本对 `EV-N1BDISC` 证据记录须按其采用（`record_status` 执行后 `collected`、审查合格后 `reviewed-pass`，:89）。

## 范围声明（硬边界）

- **本次授权范围（仅以下三项）**：三 ID 分配（本文登记）+ `n1bdisc/live-candidate` 分支候选 local 提交（候选改动与本登记一并本地提交，不推送）+ 本登记落盘。
- **明确不含**：合并 main、push（任何远程）、设备接触 / `tconn` / `list targets` / 参数查询、签名执行、新 freeze 执行、Live、正式门序列（gate 1-13）执行——门执行须按判据门序届时另行推进，本登记不自动扩大至正式门流程。
- **全新无继承**：`attempt: initial`、`retry: N/A`；前两对 pair 终态（元组漂移退役 / exact-once 违规退役）不复用、不继承、不撤销；旧 confirmation / ready-freeze 仅历史审计，不继承；与 N1b 正式门禁止共用同一 AUTH/pair（决议 §4.3.8，`native-nx-n1b-adjudication.md:131`）。
- **候选态保持**：本登记不消费任何 ID（`consumed=false`、`reusable=false`）；不得以本登记为由越过门序列消费 pair 或产出 audit/freeze/record。

## 设备状态与未来确认要求

- CC-2 元组登记引用：model `PLA-AL10`、完整软件版本 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)`；**API 26 / aarch64 / arm64-v8a 为沿同设备历史实测沿用，本次未复测**；本登记不对设备当前状态作任何推断。
- 新 machine confirmation 须由未来**独立授权的设备绑定流程**产生；旧 confirmation（含 pair2 `target-binding-confirmation-pair2-20260906-0002.json`）不因本登记恢复效力。
- 届时 target 句柄纪律：同一进程内**只查一次**、只 trim 首尾空白、失败即停，**不静默重查**。
- Live 重连规则**不得自动继承旧 pair 先例**，须届时按判据与用户确认另行确定；gate 13 Live 须用户全新确认（决议 §4.3.9）。

## 失败代价披露

- 单次执行、不重试、不换 ID（判据 :266；决议 §4.3.9 沿基础决议 §三）；pass / fail / blocked / invalid 均为终态消费、无后继 AUTH（`verdict` 枚举 schema :160）。
- gate 5 元组漂移即 blocked record + 退役本对（判据 :1436/:269：完整系统版本须实测复核重绑值 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)`，漂移即停）。
- 以上为未来门执行时生效的判据约束，本文仅登记引用、不预执行。

## 门序列

13 门完整序列**不在本文复制**，逐字以判据「流程」节为准：[n1b-disc-gate-plan.md:1432-1444](../n1b-disc-gate-plan.md)。本对**全部门未执行**；本次授权不含任何门执行。local commit 后的 `code_sha` 由未来 gate 1 绑定届时 clean HEAD（届时 HEAD 须含本登记与候选提交）；gate 2 起按门序届时推进。未来门执行记录按门序届时追加登记，本文不预建执行表。

## 签名资产引用

- AGC 应用 **NetBird N1BDISC**（包名 `cn.alfadb.netbird.n1bdisc`，与判据 :270 冻结 bundle 名逐字一致）与调试 Profile「NetBird N1BDISC Debug」为既有事实，沿第二对登记（[n1bdisc-authorization-2026-09-06-0001.md](n1bdisc-authorization-2026-09-06-0001.md)「签名链」节）引用；本文不复算、不新增签名事实。
- **本授权不含任何签名执行**；签名须届时另行授权。
