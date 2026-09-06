# N1BDISC 前置发现 campaign ID 分配授权登记（2026-09-05 · 0001，host-only）

最后核验：2026-09-05

本文登记用户（直接人类决策者）于 2026-09-05 的显式治理决定：授权 N1BDISC 前置发现 campaign 的 **ID 分配**（仅此一项，不含任何测量、HDC/设备命令、DryRun 或 Live）。据此建立 `AUTH-N1BDISC-PHYS1API26-20260905-0001`、campaign `N1BDISC-PHYS1API26-20260905-0001` 与 evidence `EV-N1BDISC-PHYS1API26-20260905-0001`。这是全新 `attempt: initial`，无 retry；G0 与 E3 的已消费 AUTH/pair 及其仓外对象不复用、不继承；与 N1b 正式门禁止共用同一 AUTH/pair（决议 §4.3.8）。判据已冻结、工程前置已闭合，执行未开始；三 ID 当前为候选态（未消费），日期组件 `20260905` 已随本次正式授权冻结（判据 :267）。

## 依据

- **用户 2026-09-05 显式授权**：授权范围为「授权分配」（仅 ID 分配）；按「人类直接决策者优先」规则登记，本登记不声称执行了 T0。
- **判据冻结**：[N1BDISC 判据](../n1b-disc-gate-plan.md)（git `04cf222`，1784 行，状态行 `criteria-frozen-2026-09-02`）；2026-09-02 冻结时点经实测，当前 worktree 与冻结 commit 逐字节一致（`git diff 04cf222 -- docs/n1b-disc-gate-plan.md` 为空、`cmp` 一致；本登记于 2026-09-05 落笔引用该实测结论）；**2026-09-05 CC-1 变更后为 04cf222+CC-1（判据登记块 +2 行位移，grok 席跨厂商隔离重审 0B/2M/4m 通过，可 freeze 状态恢复——见审查登记册 CC-1 节）**。冻结后任何判据修改属判据变更，须重新走跨厂商隔离独立审查（CC-1 即按此履行）。
- **冻结记录**：[审查登记册](../n1b-disc-r8-review-register.md):1008-1012——26 轮审查-修复循环，blocker 轨迹 12→8→4→3→5→3→2→1→2→3→5→1→2→1→**0**，终局票型 sol 0B/0M/0m（首次三零票）、deepseek 0B/3M（勘误级）、grok 0B/2M/3m；grok 两项 M 冻结前落地，m-01/02/03 冻结说明登记项在案不阻塞。
- **决议 §4.2**（[`ADJ-T0-N1B-20260831-0001`](../native-nx-n1b-adjudication.md):111-120）：ID 三形态（`:113`）；`verdict` 枚举不新增取值（`:114`）；schema 扩展是 ID 分配的前置且**已完成**——门代码 `N1BDISC` 已登记于 [evidence-schema.md](../evidence-schema.md):29。
- **判据 ID 条款**（判据 :266-267）：attempt `initial`、retry `N/A`、单次执行不重试不换 ID；日期在正式授权时冻结；**不得与 N1b 共用同一 AUTH/pair**（决议 §4.3.8，`native-nx-n1b-adjudication.md:131`）。
- **已闭合工程前置**（均为 2026-09-05 实测/登记）：
  - 工具链唯一版本基线 CLT `26.0.0.821` / SDK `26.0.0.105`（API 26 Release），见[工具链版本基线](../toolchain-baseline.md)；
  - SDK 锚点对账 9/9 PASS（判据引用的 SDK/sysroot 锚点在 821 实际内容上逐项一致），见[对账存档](../n1bdisc-sdk-anchor-reconciliation-20260905.md)；
  - Rust 1.98.1 + OHOS `aarch64-unknown-linux-ohos`/`x86_64-unknown-linux-ohos` targets + BoringTun 0.7.1 `--offline --locked` 构建 smoke PASS，及 API 26 HAP 构建 smoke PASS，见[工具链 smoke 记录](../toolchain-smoke-20260905.md)；
  - AGC 应用 **NetBird N1BDISC**（包名 `cn.alfadb.netbird.n1bdisc`，与判据 :270 冻结值逐字一致；APP ID `6917615611100883458`）+ 调试 Profile（NetBird E3 Debug 证书、绑定 PHYS-1、失效 2027-08-06；`hap-sign-tool verify-profile` PASS），见下文签名链一节。

## 授权状态

```yaml
authorization_id: AUTH-N1BDISC-PHYS1API26-20260905-0001
campaign_id: N1BDISC-PHYS1API26-20260905-0001
evidence_id: EV-N1BDISC-PHYS1API26-20260905-0001
exception: N1BDISC-DISCOVERY-CAMPAIGN
information_status: current-governance-registration
record_status: consumed-blocked-final # gate 5 元组漂移（判据 :1436：漂移即 blocked record + 退役）——2026-09-06 收官，见「门序列执行登记」与文末收官注记
stage_or_gate: N1BDISC
related_stages_or_gates: [N1B]
execution: not-started-host-only # campaign 未执行（无测量、无 Live）；pair 于 gate 5 退役
is_evidence: false
authorization_status: consumed # 2026-09-06 gate 5 元组漂移退役，终态
plan_status: consumed-blocked # 2026-09-06 gate 5 元组漂移退役，终态
criteria_freeze: git-04cf222-plus-cc-1-reviewed-pass-2026-09-05 # 冻结基线 04cf222（2026-09-02）；2026-09-05 CC-1 判据变更经跨厂商隔离重审通过后为此值
device_readiness: not-yet-requested
machine_fresh_confirmation: not-yet-requested
attempt: initial
retry: N/A
candidate:
  campaign_id: N1BDISC-PHYS1API26-20260905-0001
  evidence_id: EV-N1BDISC-PHYS1API26-20260905-0001
  identity_status: consumed-blocked # gate 5 元组漂移退役（2026-09-06）；无后继 AUTH
  consumed: false # campaign 未执行（无测量/Live 未消费 pair）；退役由 gate 5 漂移规则驱动
  reusable: false
target_tuple: HarmonyOS / PLA-AL10 / PLA-AL10 7.0.0.102(SP8C00E102R7P3) / API 26 / aarch64 / arm64-v8a # gate 5 实测复核；漂移即 blocked record + 退役（判据 :269/:1436）
bundle_name: cn.alfadb.netbird.n1bdisc # 判据 :270 冻结值，逐字一致
reviewer_role: 待 gate 3/7 freeze 重新绑定（跨厂商 isolated reviewer）
```

> **YAML 值域依据**：`evidence-schema.md` **未定义** `authorization_status` 与 `plan_status` 的合法值集（全文无此二字段）；本登记按授权登记先例（[G0 授权登记](g0-probe-authorization-2026-08-30-0001.md):23-24、E3 授权登记 2026-08-10-0002 :25/:28）采用治理登记 kebab 值，取值字面表达「仅 ID 分配、判据冻结后、执行未开始」。schema 实际定义的值集均针对证据记录本体，本登记不占用：信息状态四值（:11-17）、`record_status` 七值（:60-71）、`verdict` 四值 `pass | fail | blocked | invalid`（:76-85、:160，`verdict: collected` 非法）；未来 `EV-N1BDISC` 证据记录须按其采用（`record_status` 执行后 `collected`、审查合格后 `reviewed-pass`，:89）。

## 范围声明（硬边界）

- **本次授权仅覆盖 ID 分配**：上述三 ID 的建立与登记。不含测量，不含任何 HDC/设备命令（含 gate 4 `tconn`/`list targets`），不含 DryRun，不含 Live（判据 :255 冻结块与 :1411：本判据不构成设备 Live 授权，物理执行须用户显式授予 AUTH；决议 §4.3.9）。
- **全新无继承**：`attempt: initial`、`retry: N/A`。G0（`AUTH-G0PHYS1API26-20260830-0001`，consumed-blocked）与 E3（`AUTH-E3-PHYS1API26-20260829-0001`，consumed-pass）已消费 AUTH/pair 及其全部仓外对象不复用；**与 N1b 正式门禁止共用同一 AUTH/pair**（决议 §4.3.8，`native-nx-n1b-adjudication.md:131`）。
- **后续须逐项另行授权的事项清单**（本次授权一概不含）：
  1. vendor/lock 同产物 freeze（N1BDISC 正式依赖冻结，当前未开始）；
  2. runner 实现 + A1-A12 静态断言（gate 3 执行面，判据 :1434）；
  3. gate 4 host-prep `tconn` + 一次内存级 `list targets`（判据 :1435）；
  4. gate 11 同一 ready freeze DryRun（`is_evidence=false` + HDC0 + integrity empty，判据 :1442）；
  5. gate 13 单次 Live（须用户全新确认，决议 §4.3.9；判据 :1444 不 retry）；
  6. N1b r2 判据写作（以 DISC 事实为预注册设计输入，决议 §4.4；DISC `verdict: pass` 不得被引用为平台行为结论，schema :90）。

## 失败代价披露

- **单次执行、不重试、不换 ID**（判据 :266；决议 §4.3.9 沿基础决议 §三）。
- **pass / fail / blocked / invalid 均为终态消费，无后继 AUTH**（`verdict` 枚举 schema :160；决议 :115）——任何结局都烧掉本 pair，不得复用或改写（schema :34）。
- **gate 5 元组漂移即 blocked record + 退役 pair**（判据 :1436；版本核对警示 :269：完整系统版本必须实测复核冻结值 `PLA-AL10 7.0.0.102(SP8C00E102R7P3)`，漂移即停）。
- **`StartEntry` 发出即已消费**：gate 13 前的 operator-ready 确认步是 ID 消费前的最后闸门，未取得确认记录不得发 `StartEntry`、不得消费任何 AUTH/pair 或 evidence ID（判据 :1053）；**Allow 超时按已消费收口 fail**——`StartEntry` 已实际发出即 gate 13 已开始，AUTH/pair 与 evidence ID 已消费、不得再声称未消费，evidence 记录正常产生（`record_status=collected`、`verdict=fail`，「操作员未 Allow」逐字登记，判据 :1054）。

## 门序列

13 门完整序列**不在本文复制**，逐字以判据「流程」节为准：[n1b-disc-gate-plan.md:1432-1444](../n1b-disc-gate-plan.md)（host-only 同步 → 候选 ID 消费审计 audit-1 → freeze + 静态审查（A1-A12）→ `tconn`/`list targets` → `-TargetBindingConfirm` 元组实测复核 → ready freeze draft → reviewer record → 最终 ready freeze → audit-2 → selftests → DryRun → DryRun 独立审查 → 单次 Live 不 retry）。gate 1 的 clean HEAD 要求届时须含本登记。

## 签名链

- **AGC 应用**：HarmonyOS 应用 **NetBird N1BDISC**，包名 `cn.alfadb.netbird.n1bdisc`（与判据 :270 冻结 bundle 名逐字一致），APP ID `6917615611100883458`，所属项目 NetBird HarmonyOS Preflight（2026-09-05 用户授权会话内创建）。
- **调试 Profile**：「NetBird N1BDISC Debug」，类型 debug；证书复用 **NetBird E3 Debug** 证书；设备绑定已注册 **PHYS-1**；失效 **2027-08-06**；`hap-sign-tool verify-profile` **PASS**。
- **Profile 文件**：`/home/worker/harmonyos-signing/netbird-n1bdisc/profiles/NetBird N1BDISC Debug.p7b`（4117 字节，SHA-256 `49c99c567d0e8693269a1bda74f5d921ae0771e2cc0859512d9b3cd950df19a4`；本登记落笔时复算一致）。
- **已知事实登记**：
  1. AGC 开放能力「定位服务」为**平台强制默认勾选**（is-disabled is-checked，无法取消），非本项目选择，对调试签名无影响；
  2. 仓外既有文件 `netbird-e3/cert/NetBird E3 Debug.cer` **文件名有误导**：其实际内容为华为 CBG Root CA G2 根证书，并非开发证书——本地签名应使用 profile 内嵌开发证书，且不得修改该既有文件。
- HDC 纪律（届时生效）：gate 4 起的任何 HDC/设备命令只能引用判据「流程」节白名单表（判据 :1397 S2：AUTH 只能引用该表不得扩），本登记不复制、不扩大。

## 当前 host-only 边界

当前只允许：本登记文档，以及后续 spike 实现（`spikes/n1b-disc-phys-hap/`，判据 :270）与 host-only 验证（构建、selftest、静态断言准备）。禁止：任何真实 HDC executable、任何设备命令、pair 消费（含 audit/freeze/record 落盘）、TargetBindingConfirm、DryRun、Live——gate 4 起逐门推进且每项均须用户另行授权；gate 13 Live 前须用户全新确认（决议 §4.3.9）。判据冻结块的既有约束不变：判据修改 = 判据变更，须重审（判据 :253）。

## 门序列执行登记（2026-09-06 起，host-only 门 1-3）

| 门 | 状态 | 事实 |
| --- | --- | --- |
| 1 host-only 同步 | pass | clean HEAD `11fabd6`（0 未提交项，与 origin/main 同步）；`code_sha = 11fabd66d66917b2fa0404f5ddbf53a73c288ae1`；登记/runner/selftests/docs 均在 clean HEAD；HDC0 固定绝对 `/usr/bin/ps` 探针最终计数 0——过程记录：发现遗留 hdc server（pid 1726，2026-09-05 23:53 启动，先于本 campaign），`hdc kill` 清理后复核归零，全程零设备接触 |
| 2 audit-1 | pass | 候选 ID 消费审计：仓外双文件+sha256——`~/harmonyos-signing/netbird-n1bdisc/audit/audit-1/new-pair-id-consumption-audit-1.txt`（SHA-256 `82b24b90eff329911e22458ad976510b508c552d242b2cad97bc3dba894354f6`）；三 ID 全文检索 12 行/3 文件均为登记性引用；消费标记零命中；结论 `outside-consumption-hits=0 inside-evidence-consumption-hits=0`，candidate.consumed=false 保持 |
| 3 freeze + 静态审查 | pass | freeze-1：`~/harmonyos-signing/netbird-n1bdisc/freeze/gate3-criteria-conformance-freeze-1.txt`（SHA-256 `05453491e691ce963ce7cbf787074dbe0806ba753d04287970902e835a2116e4`；附件 `gate3-staticcheck-output.json` `3c940bac…`、`gate3-symbol-transcription.txt` `ad61a4fb…`），绑 code_sha `11fabd6…`、14/14 符号誊录（制品 `.so` SHA-256 `b2a34b05…eef1`）、A1-A12 机器检查 12/12 PASS（被检树指纹 `19e67c52bf9b`）、时间盒表逐项核对、Fault_Type 冻结候选集 `{APPFREEZE, CPPCRASH, JSRAWERROR}` + 未实测依赖登记（取不到目标元组真实 faultlogger 文本，按判据 :1434 条款以冻结候选集执行）；reviewer 静态审查（grok，跨厂商隔离席）：**PASS = freeze 成立**，0 blocker / 0 major / 2 minor；裁决：(a) `.ets` 三处 `:463` 锚语义读作 `:468`（注释校正已落地，L25/L56/L80），(b) A3 取址保链口径采纳为 freeze 口径，(c) 记录结构符合判据 :1434、gate 1/2 事实充分 |
| 4 host-prep `tconn` + `list targets` | pass（用户执行 tconn，主会话执行 list targets） | 恰一次内存级 `hdc list targets`：targets_count=1、target_redacted=true、endpoint/token 未输出未持久化；tconn 由用户（设备持有人）本人执行 |
| 5 `-TargetBindingConfirm` | **blocked（元组漂移）** | 三探针逐字 argv（判据 :1621-1622）：`hdc version`→`Ver: 3.2.0f`；`-t <T> shell param get const.product.model`→逐字节 `PLA-AL10` 后随一个尾随空格（trim 后与冻结 **MATCH**）；`-t <T> shell param get const.product.software.version`→逐字节 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)` 后随一个尾随空格（与冻结值 `PLA-AL10 7.0.0.102(SP8C00E102R7P3)` **DRIFT**——设备于 2026-08-30 G0 gate-5 实测 7.0.0.102 之后 OTA 升级至 7.0.0.105）。按判据 :266-267/:1436「漂移即 blocked record + 退役」：**blocked record + 本 pair 退役**；blocked confirmation record：`~/harmonyos-signing/netbird-n1bdisc/records/target-binding-confirmation-20260906-0001.json`（SHA-256 `d2f59da0a66d9357e59fdcae47b390f3e8780e873a56f2a3267955f3665c7fbd`，is_evidence=false、target_redacted=true、逐字节实测值含尾随空格以 od 原样登记）；随后 `hdc kill` 清理 server，HDC0 复核归零 |

> gate 4 起为设备侧（`tconn` / `TargetBindingConfirm` / DryRun / Live），逐门推进且每项均须用户另行授权；本表后续门的登记随执行追加。

**收官（2026-09-06）**：13 门执行至 gate 5 终止。gate 1-4 pass（host-only 同步/audit-1/freeze-1 静态审查 PASS/用户 tconn + 恰一次内存级 list targets）。**gate 5 元组漂移：设备软件版本实测 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)` ≠ 冻结值 `PLA-AL10 7.0.0.102(SP8C00E102R7P3)`（设备在 2026-08-30 G0 实测后 OTA 升级）——按判据 :266-267/:1436 裁 blocked record + 退役本 pair**。blocked record 见 `~/harmonyos-signing/netbird-n1bdisc/records/target-binding-confirmation-20260906-0001.json`（SHA-256 `d2f59da0…c7fbd`）。本 AUTH `consumed-blocked`，campaign 未执行（无测量、无 DryRun、无 Live），pair 不可复用，**无后继 AUTH**；`hdc kill` 后 HDC0 归零。若在当前设备版本上继续 DISC，须全新治理：判据按新元组重绑（判据变更，须跨厂商隔离重审）+ 新 AUTH/pair/evidence ID + 从 gate 1 重走门序列。实现资产（spikes/n1b-disc-phys-hap/）与 freeze-1 不因退役失效，其判据符合性结论可被新治理引用，但 gate 5 需按新元组重测。
