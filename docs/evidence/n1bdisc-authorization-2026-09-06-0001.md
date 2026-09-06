# N1BDISC 前置发现 campaign 第二对 pair ID 分配授权登记（2026-09-06 · 0001，host-only，重绑路径）

最后核验：2026-09-06

本文登记用户（直接人类决策者）于 2026-09-06 的显式治理决定：首对 pair 于 gate 5 元组漂移退役后，授权 N1BDISC 前置发现 campaign**按重绑新元组继续**——授权范围为重绑路径整体（CC-2 判据变更 + 跨厂商重审 + 本对新三 ID 分配 + 从 gate 1 重走门序列），gate 13 Live 前仍须用户全新确认（决议 §4.3.9）。据此建立 `AUTH-N1BDISC-PHYS1API26-20260906-0001`、campaign `N1BDISC-PHYS1API26-20260906-0001` 与 evidence `EV-N1BDISC-PHYS1API26-20260906-0001`（日期组件 `20260906` 已随本次正式授权冻结，判据 :267）。这是全新 `attempt: initial`，无 retry；首对 `AUTH-N1BDISC-PHYS1API26-20260905-0001` 已按判据 :266-267/:1436 裁 blocked record + 退役（`consumed-blocked-final`，无后继 AUTH），其 AUTH/pair/evidence ID 与全部仓外对象不复用、不继承；与 N1b 正式门禁止共用同一 AUTH/pair（决议 §4.3.8，`native-nx-n1b-adjudication.md:131`）。判据已按 CC-1/CC-2 重审通过、工程资产可引用首对既有物（见「依据」节），执行未开始；三 ID 当前为候选态（未消费）。

## 依据

- **用户 2026-09-06 显式授权**：「重绑新元组继续」——授权范围为重绑路径整体：CC-2 判据变更 + 跨厂商重审 + 本对新三 ID 分配 + 从 gate 1 重走门序列。按「人类直接决策者优先」规则登记，本登记不声称执行了 T0。
- **判据**：[N1BDISC 判据](../n1b-disc-gate-plan.md)，冻结基线 git `04cf222`（2026-09-02）；现行状态行（判据 :3）：`criteria-frozen-2026-09-02` + `criteria-change-1-reviewed-pass-2026-09-05` + `criteria-change-2-reviewed-pass-2026-09-06`：
  - **CC-1（2026-09-05）**：实现阶段独立审查 B-04 blocker 触发；用户授权方案①；grok 席跨厂商隔离重审 **0 blocker / 2 major / 4 minor，通过**（M-1/M-2 已按重审建议同补丁闭合），可 freeze 状态恢复（[审查登记册](../n1b-disc-r8-review-register.md):1014-1023）。
  - **CC-2（2026-09-06）**：首对 gate 5 元组漂移触发；冻结元组软件版本重绑 `PLA-AL10 7.0.0.102(SP8C00E102R7P3)` → `PLA-AL10 7.0.0.105(SP6C00E105R7P3)`（判据 :268；登记块文末追加、判据 :1790——`:10`-`:1786` 行号零位移，首对 723 处实现锚保持有效）；deepseek 席跨厂商隔离重审 **0 blocker / 0 major / 2 minor，通过**——m-1（`:268` 括注，1 字零位移）已修、m-2 已在判据 CC-2 块显式登记处置；状态 `criteria-change-2-reviewed-pass-2026-09-06`（[审查登记册](../n1b-disc-r8-review-register.md):1025-1030）。SDK/sysroot 锚点（26.0.0.821 实测对账 9/9 PASS）为 SDK 侧、与设备小版本无关，CC-2 后仍有效。冻结后任何判据修改属判据变更，须重新走跨厂商隔离独立审查（判据 :253；CC-1/CC-2 即按此履行）。
- **首对退役事实**：gate 5 三白名单探针实测设备软件版本 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)` ≠ 原冻结值 `PLA-AL10 7.0.0.102(SP8C00E102R7P3)`（设备于 2026-08-30 G0 gate-5 实测后 OTA 升级）——按判据 :266-267/:1436 裁 **blocked record + 退役本 pair**。blocked record：`~/harmonyos-signing/netbird-n1bdisc/records/target-binding-confirmation-20260906-0001.json`（SHA-256 `d2f59da0a66d9357e59fdcae47b390f3e8780e873a56f2a3267955f3665c7fbd`；本登记落笔时复算一致）；首对登记 [n1bdisc-authorization-2026-09-05-0001.md](n1bdisc-authorization-2026-09-05-0001.md)（`consumed-blocked-final`，2026-09-06 收官）。**首对退役不因 CC-2 或本登记撤销。**
- **实现资产引用**：`spikes/n1b-disc-phys-hap/` 全套（首对轮已建：ArkTS VpnExtensionAbility + Rust/NAPI 探针 + host runner；判据 :270 实现载体，bundle 名冻结值不变）。首对 **freeze-1** 判据符合性结论（gate 3 静态审查 PASS：`~/harmonyos-signing/netbird-n1bdisc/freeze/gate3-criteria-conformance-freeze-1.txt`，SHA-256 `05453491e691ce963ce7cbf787074dbe0806ba753d04287970902e835a2116e4`，绑首对 code_sha `11fabd66d66917b2fa0404f5ddbf53a73c288ae1`；本登记落笔时复算一致）**可被本对治理引用**，但**不替代本对 gate 3**：本对 gate 3 须按本对届时 clean HEAD 的 code_sha **重出 freeze-2 并重走静态审查**（A1-A12 机器检查 + reviewer 席）。**CC-2 m-2 登记**：API 26 / aarch64 / arm64-v8a 为「沿同设备历史实测沿用 + build 级 OTA 性质推断」，本对 gate 5 白名单仅三探针、不得扩，故三者本次未复测（arch 为硬件属性不可经 OTA 改变、API level 绑定主版本 7.0.0 不变）；与 rebind8「实测不从 build 推断」纪律的表面张力已登记于判据 CC-2 块（判据 :1790），不阻塞。
- **AGC 资产沿用**：应用 **NetBird N1BDISC**（包名 `cn.alfadb.netbird.n1bdisc`，与判据 :270 冻结 bundle 名逐字一致；APP ID `6917615611100883458`）+ 调试 Profile「NetBird N1BDISC Debug」（NetBird E3 Debug 证书、绑定 PHYS-1、失效 2027-08-06）——bundle 名判据冻结值不变，AGC 侧无需重建，详见下文「签名链」一节。

## 授权状态

```yaml
authorization_id: AUTH-N1BDISC-PHYS1API26-20260906-0001
campaign_id: N1BDISC-PHYS1API26-20260906-0001
evidence_id: EV-N1BDISC-PHYS1API26-20260906-0001
exception: N1BDISC-DISCOVERY-CAMPAIGN
information_status: current-governance-registration
record_status: active-governance-registration # 活跃治理登记（非证据记录本体）；执行后另立 EV-N1BDISC 证据记录
stage_or_gate: N1BDISC
related_stages_or_gates: [N1B]
execution: not-started-host-only # 本对未执行（无测量、无 HDC/设备命令、无 DryRun/Live）
is_evidence: false
authorization_status: granted-rebind-path-id-allocation # 重绑路径整体授权下的三 ID 分配；schema 无此字段值集，沿首对先例治理 kebab 值（见下方值域依据注）
plan_status: id-allocated-pending-gate-execution # 三 ID 已分配、13 门未执行（自 gate 1 起重走）
criteria_freeze: git-04cf222-plus-cc-1-plus-cc-2-reviewed-pass-2026-09-06 # 冻结基线 04cf222（2026-09-02）+ CC-1（2026-09-05，grok 席重审通过）+ CC-2（2026-09-06 元组重绑，deepseek 席重审通过）
device_readiness: not-yet-requested
machine_fresh_confirmation: not-yet-requested
attempt: initial
retry: N/A
candidate:
  campaign_id: N1BDISC-PHYS1API26-20260906-0001
  evidence_id: EV-N1BDISC-PHYS1API26-20260906-0001
  identity_status: candidate # 候选态（未消费），gate 2/9 消费审计的受检对象
  consumed: false # 本对未执行任何测量/DryRun/Live
  reusable: false
target_tuple: HarmonyOS / PLA-AL10 / PLA-AL10 7.0.0.105(SP6C00E105R7P3) / API 26 / aarch64 / arm64-v8a # 判据 :268 CC-2 重绑值；gate 5 实测复核，漂移即 blocked record + 退役（判据 :269/:1436）
bundle_name: cn.alfadb.netbird.n1bdisc # 判据 :270 冻结值，逐字一致
reviewer_role: 待 gate 3/7 freeze 重新绑定（跨厂商 isolated reviewer）
```

> **YAML 值域依据**：[`evidence-schema.md`](../evidence-schema.md) **未定义** `authorization_status` 与 `plan_status` 的合法值集（全文无此二字段）；`record_status` 七值（:60-71）针对证据记录本体，本治理登记不占用。本登记沿第一对先例（[首对授权登记](n1bdisc-authorization-2026-09-05-0001.md)，其「YAML 值域依据」注同）采用治理登记 kebab 值：`granted-rebind-path-id-allocation` / `id-allocated-pending-gate-execution` / `active-governance-registration`，取值字面表达「重绑路径整体授权下的 ID 分配、判据 04cf222+CC-1+CC-2 重审通过后、执行未开始、登记活跃」。schema 其余实际值集均针对证据记录本体，本登记不占用：信息状态四值（:11-17）、`verdict` 四值 `pass | fail | blocked | invalid`（:76-85、:160，`verdict: collected` 非法）；未来本对 `EV-N1BDISC` 证据记录须按其采用（`record_status` 执行后 `collected`、审查合格后 `reviewed-pass`，:89）。

## 范围声明（硬边界）

- **本次授权覆盖重绑路径整体**：本对新三 ID 分配（本文）+ **host-only gate 1-3 重走**（gate 1 host-only 同步〔clean HEAD 须含本登记〕→ gate 2 候选 ID 消费审计 audit-1 → gate 3 freeze-2 + 静态审查）+ **设备侧 gate 4-5 重测**（gate 4 host-prep `tconn` + 恰一次内存级 `list targets`；gate 5 `-TargetBindingConfirm` 三白名单探针，按重绑值 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)` 实测复核）；gate 6 起沿判据 :1432-1444 门序列逐门推进。
- **gate 13 单次 Live 须用户全新确认**（决议 §4.3.9，`native-nx-n1b-adjudication.md:132`）：本次授权不构成 Live 的执行确认；gate 13 前的 operator-ready 确认步照常是 ID 消费前的最后闸门（判据 :1053-1054）。
- **全新无继承**：`attempt: initial`、`retry: N/A`。首对（`consumed-blocked-final`，无后继 AUTH）的全部对象不复用、不继承；更早 G0（`consumed-blocked`）/E3（`consumed-pass`）已消费 pair 及其仓外对象同样不复用；**与 N1b 正式门禁止共用同一 AUTH/pair**（决议 §4.3.8，`native-nx-n1b-adjudication.md:131`）。
- **首对退役不撤销**：CC-2 与本登记均不重开、不改写首对 blocked record（`target-binding-confirmation-20260906-0001.json`）及其 `consumed-blocked-final` 终态。
- **失败代价同首对**：见下节「失败代价披露」。

## 失败代价披露

- **单次执行、不重试、不换 ID**（判据 :266；决议 §4.3.9 沿基础决议 §三）。
- **pass / fail / blocked / invalid 均为终态消费，无后继 AUTH**（`verdict` 枚举 schema :160；决议 :115）——任何结局都烧掉本对 pair，不得复用或改写（schema :34）。
- **gate 5 元组漂移即再次 blocked record + 退役本对**（判据 :1436；版本核对警示 :269：完整系统版本必须实测复核重绑值 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)`，漂移即停）——若再漂移，同样须全新治理：判据再变更 + 跨厂商隔离重审 + 新 AUTH/pair/evidence 三 ID + 再从 gate 1 重走门序列。
- **`StartEntry` 发出即已消费**：gate 13 前的 operator-ready 确认步是 ID 消费前的最后闸门，未取得确认记录不得发 `StartEntry`、不得消费任何 AUTH/pair 或 evidence ID（判据 :1053）；**Allow 超时按已消费收口 fail**——`StartEntry` 已实际发出即 gate 13 已开始，AUTH/pair 与 evidence ID 已消费、不得再声称未消费，evidence 记录正常产生（`record_status=collected`、`verdict=fail`，「操作员未 Allow」逐字登记，判据 :1054）。

## 门序列

13 门完整序列**不在本文复制**，逐字以判据「流程」节为准：[n1b-disc-gate-plan.md:1432-1444](../n1b-disc-gate-plan.md)（host-only 同步 → 候选 ID 消费审计 audit-1 → freeze + 静态审查（A1-A12）→ `tconn`/`list targets` → `-TargetBindingConfirm` 元组实测复核 → ready freeze draft → reviewer record → 最终 ready freeze → audit-2 → selftests → DryRun → DryRun 独立审查 → 单次 Live 不 retry）。本对自 gate 1 重走：gate 1 的 clean HEAD 要求届时须含本登记；gate 3 按本对 code_sha 重出 freeze-2（首对 freeze-1 结论仅作引用，见「依据」节）；gate 5 按重绑值 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)` 实测复核。

## 签名链

- **AGC 应用（沿用）**：HarmonyOS 应用 **NetBird N1BDISC**，包名 `cn.alfadb.netbird.n1bdisc`（与判据 :270 冻结 bundle 名逐字一致，本对不变），APP ID `6917615611100883458`，所属项目 NetBird HarmonyOS Preflight（2026-09-05 首对授权会话内创建）。
- **调试 Profile（沿用）**：「NetBird N1BDISC Debug」，类型 debug；证书复用 **NetBird E3 Debug** 证书；设备绑定已注册 **PHYS-1**；失效 **2027-08-06**；首对登记时 `hap-sign-tool verify-profile` **PASS**（见首对登记「签名链」节），本对沿用、AGC 侧无需重建。
- **Profile 文件**：`/home/worker/harmonyos-signing/netbird-n1bdisc/profiles/NetBird N1BDISC Debug.p7b`（4117 字节，SHA-256 `49c99c567d0e8693269a1bda74f5d921ae0771e2cc0859512d9b3cd950df19a4`；本登记落笔时复算一致）。
- **已知事实登记**：
  1. AGC 开放能力「定位服务」为**平台强制默认勾选**（is-disabled is-checked，无法取消），非本项目选择，对调试签名无影响；
  2. 仓外既有文件 `netbird-e3/cert/NetBird E3 Debug.cer` **文件名有误导**：其实际内容为华为 CBG Root CA G2 根证书，并非开发证书——本地签名应使用 profile 内嵌开发证书，且不得修改该既有文件。
- HDC 纪律（届时生效）：gate 4 起的任何 HDC/设备命令只能引用判据「流程」节白名单表（判据 :1397 S2：AUTH 只能引用该表不得扩），本登记不复制、不扩大。

## 当前 host-only 边界

当前执行未开始（三 ID 候选态）。本对授权面自 host-only 起步：host-only gate 1-3 重走（届时按门序产出 audit-1、freeze-2 与静态审查记录）、`spikes/n1b-disc-phys-hap/` 既有实现资产上的 host-only 验证（构建、selftest、静态断言准备），以及设备侧 gate 4-5 重测（本对授权范围，届时按判据「流程」节白名单执行）。禁止越过门序列提前消费 pair（audit/freeze/record 落盘须按门序届时产出）；禁止 DryRun 与 Live 提前执行；gate 13 Live 前须用户全新确认（决议 §4.3.9）。判据冻结块既有约束不变：判据修改 = 判据变更，须重审（判据 :253）。

## 门序列执行登记（随执行自 gate 1 起追加）

| 门 | 状态 | 事实 |
| --- | --- | --- |
| 1 host-only 同步 | pass | clean HEAD `3dd95ac`（0 未提交项，与 origin/main 同步）；`code_sha = 3dd95ac31805dd4da2d159df52064450596b400c`；登记/runner/selftests/docs 均在 clean HEAD；HDC0 固定绝对 `/usr/bin/ps` 探针计数 0 |
| 2 audit-1 | pass | 本对候选 ID 消费审计：仓外双文件+sha256——`~/harmonyos-signing/netbird-n1bdisc/audit/pair-20260906/new-pair-id-consumption-audit-1.txt`（SHA-256 `e830245fd77d4146a253896a0072429302d68d084b59d65763acfcab00705f42`）；三 ID 检索 7 行/2 文件均为登记性引用；消费标记零命中；`candidate.consumed=false` 保持 |
| 3 freeze + 静态审查 | **pass（freeze-2-v2，FAIL→整改→复审后）** | 首审 FAIL：freeze-2（`b8729f1d…eb5fa`，code_sha `3dd95ac`）经 sol 席（gpt-5.6-sol 跨厂商隔离）审查 7 blocker / 2 major——probe/entry 符合性面全过，FAIL 全落本轮新增 runner 派生层（U1/U3 身份匹配、U2 端口与 payload、u5 not_attempted、join 轴违 :693 与 pending、D8b 域校验、未注册 cause、U4 fail-open、ws/we 无符号、test 夹具镜像错误）；处置记录 `3b85f782…43fc5`（FAIL 不消费 pair）。整改 commit `630efe0`（producer 语义基准重写 + dw.rs B4 最小修一处 + test_platform producer-conformant 重写 + 5 反例剧本）。**freeze-2-v2**（`~/harmonyos-signing/netbird-n1bdisc/freeze/gate3-criteria-conformance-freeze-2-v2-20260906.txt`，SHA-256 `ee356ab8…eaf6`；附件 staticcheck `c73a1cd6…` / 符号 `ad61a4fb…`）绑 code_sha `630efe0`、`.so` 新 sha `1856d5dc…b240`、12/12（新指纹 `f66406e0395e`）、109 selftests + probe 15 单测。复审（grok 席，曾审首对 freeze-1；sol 席三次中断后按目录显式换席）：**PASS = freeze-2-v2 成立**——7B/2M 逐项消解（含纯函数复现外来包绝不 observed-true、pending 被十值域拒绝）、dw.rs 最小修合规（:443/:870 逐字）、audit-1 沿用可接受、FAIL→整改→v2 流程合规（无设备接触、pair 未消费）；遗留偏差 8 项全部接受；新引入仅 4 minor（freeze 六节偏差清单未内联、dw.rs 行号锚漂移至 :927-935、chunk 污染 F8 保险丝、S5 4B/20B 口径重合）不阻塞。gate 1 随整改重记 pass（clean HEAD `630efe0`、code_sha `630efe00aaa89663ea8ed2c914a4cd502660d80a`、HDC0=0）；gate 2 audit-1 沿用（新 code_sha 检索复检零消费） |

> 本表随本对执行逐门追加。gate 3 经 FAIL→整改→v2 复审 pass；截至本行落笔，三 ID 保持候选态（`identity_status: candidate`、`consumed: false`）。gate 4 起为设备侧，须用户逐项授权（tconn 由用户本人执行）。gate 5 记录若出现元组漂移，即按判据 :1436 收口 blocked record + 退役本对。
