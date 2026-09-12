# N1BDISC 第四对 pair 三 ID 分配授权登记（治理命名 2026-09-11 · 0001，host-only，CC-5 重绑后重走门序列）

最后核验：2026-09-11

本文登记用户（直接人类决策者）2026-09-11 会话内的显式治理决定（**选项 A**）：「再次 rebind 到新元组并以新三 ID 重走门序列」——为 N1BDISC 发现 campaign 分配第四对新三 ID 并登记落盘。**治理命名日期在授权时固定为 `20260911-0001`，授权后不随实际落笔或执行日期变化**；本文实际 generated_at 为 2026-09-11 13:20:12 CST（`date` 实测，与命名日区分）。据此建立 `AUTH-N1BDISC-PHYS1API26-20260911-0001`、campaign `N1BDISC-PHYS1API26-20260911-0001` 与 evidence `EV-N1BDISC-PHYS1API26-20260911-0001`。这是全新 `attempt: initial`，无 retry；本文仅登记治理 ID 分配（allocated/unused，candidate 态，`consumed=false`/`reusable=false`，`is_evidence=false`），**任何门均未执行**。本文沿既有 AUTH 登记结构（含顶部 YAML 块、依据、范围声明、YAML 值域依据注），不复制旧 pair 门执行记录，不宣称旧 confirmation 有效。

## 依据

- **用户 2026-09-11 会话内显式授权（选项 A）**：授权范围为——上述三 ID 分配 + 本登记落盘；**不含**任何门执行、设备接触 / `tconn` / `list targets` / 参数查询、签名、freeze、DryRun、Live。
- **触发事实 = 第三对 pair gate 5 元组漂移退役**：第三对 `AUTH-N1BDISC-PHYS1API26-20260906-0002` 于 2026-09-11 gate 5 三探针实测 `PLA-AL10 7.0.0.105(SP10C00E105R7P3)` ≠ 前冻结值 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)`（数字版本相同、构建分支标签 SP6C→SP10C），按判据 `:1436` 裁 `blocked-tuple-drift` + blocked record + 退役；用户据此显式授权 rebind 至新元组并以新三 ID 重走门序列。该重绑即**判据变更 CC-5**（commit `afd04ea448f37c701539c0d65936e5ef325ed592`），当前状态 `criteria-change-5-pending-review-2026-09-11`。
- **判据基线（引用，不变更）**：[N1BDISC 判据](../n1b-disc-gate-plan.md)，冻结基线 git `04cf222`（2026-09-02）+ CC-1（2026-09-05）+ CC-2（2026-09-06 元组重绑）+ CC-3（2026-09-11）+ CC-4（2026-09-11），**CC-1~CC-4 全部 `reviewed-pass`**；外加 **CC-5（2026-09-11 设备元组重绑，`criteria-change-5-pending-review-2026-09-11`）**——按判据 CC-5 文末登记块，**须经跨厂商隔离重审通过后方可按新元组恢复可 freeze/测量状态**。本登记不变更判据。
- **前三对 pair 终态（不继承、不复用、不撤销）**：
  - 首对 `AUTH-N1BDISC-PHYS1API26-20260905-0001`：gate 5 元组漂移 `7.0.0.102(SP8C00E102R7P3)` → `7.0.0.105(SP6C00E105R7P3)`（2026-09-06），`consumed-blocked-final`；仓外 blocked record `/home/worker/harmonyos-signing/netbird-n1bdisc/records/target-binding-confirmation-20260906-0001.json`（sha256 `d2f59da0a66d9357e59fdcae47b390f3e8780e873a56f2a3267955f3665c7fbd`）；登记 [n1bdisc-authorization-2026-09-05-0001.md](n1bdisc-authorization-2026-09-05-0001.md)。
  - 第二对 `AUTH-N1BDISC-PHYS1API26-20260906-0001`：治理违规退役——gate 4 `list targets` 实际执行两次、违反判据「恰一次」约束，用户不追认、审查席无豁免权（2026-09-06），`retired-unused-governance-violation`、`consumed: false`（未 Live、未产生测量）；仓外退役记录 `/home/worker/harmonyos-signing/netbird-n1bdisc/reviews/pair2-retirement-exact-once.json`（sha256 `5dfd19c39ef45780ad1bb6e42a6afb77d886851f3d9d7ebab1e25bcda0e8c652`）；登记 [n1bdisc-authorization-2026-09-06-0001.md](n1bdisc-authorization-2026-09-06-0001.md)。
  - 第三对 `AUTH-N1BDISC-PHYS1API26-20260906-0002`：gate 5 元组漂移 `7.0.0.105(SP6C00E105R7P3)` → `7.0.0.105(SP10C00E105R7P3)`（2026-09-11），`blocked-tuple-drift` + `retired-terminal`、`consumed: false`（未测量、未 Live）、`reusable: false`、无后继 AUTH；仓外 blocked record `/home/worker/harmonyos-signing/netbird-n1bdisc/records/target-binding-confirmation-pair3-20260911.json`（sha256 `aa664446723139c388176d026c8c0be64927f695d6b4c3abb767d475c7fbba8e`，同名 `.sha256` sidecar）；登记 [n1bdisc-authorization-2026-09-06-0002.md](n1bdisc-authorization-2026-09-06-0002.md)。
  - 三对终态均**不复用、不继承、不撤销**；旧 confirmation / ready-freeze / 门执行记录仅作历史审计对象，**不可继承其效力**。
- **可复用资产声明（不因前对退役撤销）**：`freeze-3-v4` 成立结论；实现 commit `53faa1f`（制品 `.so` sha256 `5e5408772e75b78f3b01d7a6297bc2ab9fe16a9860ba9c36df248bed667a297c`，1160224 B）；判据 CC-1~CC-4；第三对 gate 2 audit-1 仓外双文件 `/home/worker/harmonyos-signing/netbird-n1bdisc/audit/pair-20260906-0002/new-pair-id-consumption-audit-1.txt`（sha256 `35825ebde9a267cf78f14202c983894021d381157643bba0221d558082274a4c`）。以上可被本新治理引用，但**不构成本对任何门已执行或已通过**。
- **AGC 资产沿用（无需重建）**：bundle 名 `cn.alfadb.netbird.n1bdisc` 冻结值不变；AGC 应用 NetBird N1BDISC 与调试 Profile「NetBird N1BDISC Debug」为既有事实，有效期至 2027-08-06、绑定 PHYS-1，沿用既有引用；本文不复算、不新增签名事实。
- **先前 host-only 开发授权继续可引用**（host-only 验证、构建、selftest），但本登记不自动扩大至正式门流程。

## 授权状态

```yaml
authorization_id: AUTH-N1BDISC-PHYS1API26-20260911-0001
campaign_id: N1BDISC-PHYS1API26-20260911-0001
evidence_id: EV-N1BDISC-PHYS1API26-20260911-0001
exception: N1BDISC-DISCOVERY-CAMPAIGN
information_status: current-governance-registration
record_status: active-governance-registration # 活跃治理登记（非证据记录本体）；执行后另立 EV-N1BDISC 证据记录
stage_or_gate: N1BDISC
related_stages_or_gates: [N1B]
execution: not-started # 无门执行、无测量、无 HDC/设备命令、无签名、无 freeze、无 DryRun/Live
is_evidence: false
authorization_status: granted-id-allocation-and-registration # 三 ID 分配 + 本登记落盘授权；schema 无此字段值集，沿先例治理 kebab 值（见下方值域依据注）
plan_status: id-allocated-not-executed # 三 ID 已分配未执行；门序列未开始
criteria_freeze: git-04cf222-plus-cc-1-plus-cc-2-plus-cc-3-plus-cc-4-reviewed-pass-plus-cc-5-pending-review-2026-09-11 # 判据基线引用，本登记不变更
device_readiness: not-yet-requested
machine_fresh_confirmation: not-yet-requested
attempt: initial
retry: N/A
candidate:
  campaign_id: N1BDISC-PHYS1API26-20260911-0001
  evidence_id: EV-N1BDISC-PHYS1API26-20260911-0001
  identity_status: candidate # 候选态（未消费），未来 gate 2/9 消费审计的受检对象
  consumed: false # 本登记不消费任何 ID；本登记仅登记 ID 分配，不是门执行
  reusable: false
governance_naming_date: "20260911-0001" # 授权时固定，不随实际执行日期变化
generated_at: "2026-09-11 13:20:12 CST" # date 实测
workspace: /home/worker/work/base/netbird-harmonyos # main 工作区（候选实现已合并入 main，无独立候选 worktree）
code_sha: pending-gate-1-binding # 待未来 gate 1 绑定届时 clean HEAD；本文不填造
target_tuple: HarmonyOS / PLA-AL10 / PLA-AL10 7.0.0.105(SP10C00E105R7P3) / API 26 / aarch64 / arm64-v8a # 判据 :268 CC-5 重绑值（登记引用；设备当前状态不推断，见「设备状态与未来确认要求」节）
bundle_name: cn.alfadb.netbird.n1bdisc # 判据 :270 冻结值，逐字一致
reviewer_role: 待未来 gate 3/7 freeze 重新绑定（跨厂商 isolated reviewer）
```

> **YAML 值域依据**：[`evidence-schema.md`](../evidence-schema.md) **未定义** `authorization_status` 与 `plan_status` 的合法值集（全文无此二字段）；`record_status` 七值（:60-71）针对证据记录本体，本治理登记不占用。本登记沿前三对先例（[首对授权登记](n1bdisc-authorization-2026-09-05-0001.md)、[第二对授权登记](n1bdisc-authorization-2026-09-06-0001.md)、[第三对授权登记](n1bdisc-authorization-2026-09-06-0002.md)，各带同款「YAML 值域依据」注）采用治理登记 kebab 值：`granted-id-allocation-and-registration` / `id-allocated-not-executed` / `active-governance-registration`，取值字面表达「三 ID 分配、执行未开始、登记活跃」。`governance_naming_date` / `generated_at` / `workspace` / `code_sha` 为本登记治理事实字段（schema 未定义，注释化治理，沿先例做法）；第三对登记中的 `candidate_base`（独立候选 worktree 基线）与 `terminal_disposition`（退役终态）在本对**不适用故不设**——候选实现 `spikes/n1b-disc-phys-hap/` 已合并入 `main`、无独立候选 worktree，且本对为活跃候选、未退役。schema 其余实际值集均针对证据记录本体，本登记不占用：信息状态四值（:11-17）、`verdict` 四值 `pass | fail | blocked | invalid`（:76-85、:160）；未来本对 `EV-N1BDISC` 证据记录须按其采用（`record_status` 执行后 `collected`、审查合格后 `reviewed-pass`，:89）。

## 范围声明（硬边界）

- **本次授权范围（仅以下两项）**：三 ID 分配（本文登记）+ 本登记落盘。
- **明确不含**：任何门执行（gate 1-13）、设备接触 / `tconn` / `list targets` / 参数查询（含 gate 5 三探针）、签名执行、freeze 执行、DryRun、Live。
- **gate 4 起为设备侧**：`tconn` + **恰一次**内存级 `list targets`，**须用户本人连接设备**；gate 5 三探针对照 CC-5 重绑元组 `PLA-AL10 7.0.0.105(SP10C00E105R7P3)`；**gate 13 Live 须用户全新确认**（决议 §4.3.9）。
- **全新无继承**：`attempt: initial`、`retry: N/A`；前三对 pair 终态（元组漂移退役 / exact-once 治理违规退役 / 元组漂移退役）不复用、不继承、不撤销；旧 confirmation / ready-freeze 仅历史审计，不继承；与 N1b 正式门禁止共用同一 AUTH/pair（决议 §4.3.8，`native-nx-n1b-adjudication.md:131`）。
- **候选态保持**：本登记不消费任何 ID（`consumed=false`、`reusable=false`）；不得以本登记为由越过门序列消费 pair 或产出 audit/freeze/record。
- **CC-5 生效前提**：本登记仅按判据 CC-5 文末登记块引用新元组；**CC-5 重审通过前不得据此恢复可 freeze/测量状态**。

## 设备状态与未来确认要求

- CC-5 元组登记引用：model `PLA-AL10`、完整软件版本 `PLA-AL10 7.0.0.105(SP10C00E105R7P3)`（2026-09-11 gate 5 实测值；`od` 逐字节 **36 字节含 1 尾随空格**，trim 后登记）；**API 26 / aarch64 / arm64-v8a 为沿同设备历史实测沿用，本次未复测**；本登记不对设备当前状态作任何推断。
- 新 machine confirmation 须由未来**独立授权的设备绑定流程**产生；旧 confirmation（含 pair2 `target-binding-confirmation-pair2-20260906-0002.json`、pair3 `target-binding-confirmation-pair3-20260911.json`）不因本登记恢复效力。
- 届时 target 句柄纪律：同一进程内**只查一次**、只 trim 首尾空白、失败即停，**不静默重查**。
- **设备自动更新须关闭**：前三对中两对（首对、第三对）均因 gate 5 元组漂移退役，设备 OTA 是连续漂移的直接成因；执行前须关闭设备自动更新，避免 gate 1-3 host-only 期间再次漂移。
- Live 重连规则**不得自动继承旧 pair 先例**，须届时按判据与用户确认另行确定；gate 13 Live 须用户全新确认（决议 §4.3.9）。

## 失败代价披露

- 单次执行、不重试、不换 ID（判据 :266；决议 §4.3.9 沿基础决议 §三）；pass / fail / blocked / invalid 均为终态消费、无后继 AUTH（`verdict` 枚举 schema :160）。
- gate 5 元组漂移即 blocked record + 退役本对（判据 :1436/:269：完整系统版本须实测复核 CC-5 重绑值 `PLA-AL10 7.0.0.105(SP10C00E105R7P3)`，漂移即停）。
- 以上为未来门执行时生效的判据约束，本文仅登记引用、不预执行。

## 门序列

13 门完整序列**不在本文复制**，逐字以判据「流程」节为准：[n1b-disc-gate-plan.md:1432-1444](../n1b-disc-gate-plan.md)。本对**全部门未执行**；本次授权不含任何门执行。`code_sha` 由未来 gate 1 绑定届时 clean HEAD；gate 2 起按门序届时推进。未来门执行记录按门序届时追加登记，本文不预建执行表。

## 签名资产引用

- AGC 应用 **NetBird N1BDISC**（包名 `cn.alfadb.netbird.n1bdisc`，与判据 :270 冻结 bundle 名逐字一致）与调试 Profile「NetBird N1BDISC Debug」为既有事实，沿第二/第三对登记引用；有效期至 2027-08-06、绑定 PHYS-1，**无需重建**。本文不复算、不新增签名事实。
- **本授权不含任何签名执行**；签名须届时另行授权。

## 三 ID 状态

`AUTH-N1BDISC-PHYS1API26-20260911-0001` / campaign `N1BDISC-PHYS1API26-20260911-0001` / evidence `EV-N1BDISC-PHYS1API26-20260911-0001` 保持 **candidate 态、未消费**（`identity_status: candidate`、`consumed=false`、`reusable=false`、`is_evidence=false`）；**任何门均未执行**（无 gate 执行、无测量、无设备/HDC 命令、无签名、无 freeze、无 DryRun/Live）。

## 门序列执行登记（第四对，2026-09-11，host-only 门 1-3）

> 本节为 2026-09-11 的追加登记，**上文（含 YAML 顶部字段、依据、范围声明、门序列节）不改**；与上文表述冲突时以本节执行事实为准。体例沿第三对授权登记「门序列执行登记」节。

| 门 | 状态 | 事实 |
| --- | --- | --- |
| 1 host-only 同步 | pass | 现场核验（2026-09-11）：`git status --short --branch` → `## main...origin/main`；`git status --porcelain` → 空（0 未提交项）；`git rev-parse HEAD` → `8f926fb6581febab58df6487b0b818b3b8f9fab6`；`git rev-list --left-right --count origin/main...HEAD` → 左右计数均 0（原始输出以 TAB 分隔，即 `0`/`0`，与 origin/main 同步）；HDC0 只读探针 `pgrep -x hdc -a` → `618318 hdc -m -s ::ffff:127.0.0.1:8710`、`ps -eo pid,comm \| awk '$2=="hdc"'` → `618318 hdc`（同一次实测，同一进程；`ps -o lstart` 实测启动于 14:05:07，PPID 1）——该进程为 **hdc server/daemon**（`-m -s` + endpoint 参数形态），非设备命令执行；**未执行任何设备/hdc 命令**、未连接设备、未执行 `hdc kill`，**零设备接触** |
| 2 audit-1 | pass | 本对 gate 2 证据 = 仓外双文件（现场复核存在 + 复算一致）：`~/harmonyos-signing/netbird-n1bdisc/audit/pair-20260911-0001/new-pair-id-consumption-audit-1.txt`（5626 B，SHA-256 `9a56c006a4dbdef95f52887e28e7517459b84a0f95a6bcd9df74ce3e5d9e5cf7`）+ 同名 `.sha256` sidecar（内容即上述哈希，`sha256sum -c` → `OK`）。**新 code_sha 有界 grep 复检**（限定 `README.md docs scripts spikes`，排除 `build/.hvigor/target/node_modules/.git/__pycache__`）：三 ID 命中 **AUTH 5 行 / campaign 9 行 / EV 6 行**，全部为治理登记本体与索引/交接引用；消费标记（`consumed: true`/`consumed=true`、本对非 candidate `identity_status`、本对 evidence 记录本体 `record_status: collected/reviewed-pass`）**零命中**（仓内既有 `consumed: true` 命中均为 E3 历史记录与第三对执行表自述，无一绑定本对）→ candidate 态保持 |
| 3 freeze + 静态审查 | pass（**freeze-4 成立**） | 有效记录 = **freeze-4 原文 + 更新版沿革勘误**的组合（审查席裁决「成立、0 blocker / 0 major / 1 minor、**无需重出 freeze-4**」）。freeze-4：`~/harmonyos-signing/netbird-n1bdisc/freeze/gate3-criteria-conformance-freeze-4-20260911.txt`（SHA-256 `b5aed745a25c3029c86100a2ac5a4ff441a413e2ccea1b3eba5d59e1c3ad4a84`，现场复算与 sidecar 一致）；附件 `gate3-staticcheck-output-pair4.json`（`ae3bd883fcc61e08a8f7eaaa22df79ef9af7f7b34b0b8f49ae58080c6339cd54`）、`gate3-symbol-transcription-pair4.txt`（`ad61a4fb147482379adda6b1ee326a88f917ef79acaa5efe00d88a3560df7f57`）、`gate3-criteria-dimension-recheck-pair4-20260911.txt`（`905ac168824852ce5a4c07ebde1c371b07e7b5b299977ecdb3a24dc4f1c909ad`），三附件现场复算一致；沿革勘误 `gate3-freeze4-erratum-lineage-20260911.txt`（`9b85b1c4a4439782c67c7708bff24a7142afaf358e2c999fe167bd2de90bc289`） |
| 4 host-prep `tconn` + 一次内存级 `list targets` | pass | 设备侧（2026-09-11）：`tconn` 由**用户本人**执行，连接建立于 **2026-09-11 14:05:07 CST**（设备 LAN 地址已脱敏，形状 `###.###.##.###:#####`）；**用户已确认该连接即为 gate 4 的 `tconn`**。主会话执行**恰一次**内存级 `list targets`：`rc=0`、`targets_count=1`、target 形状 `###.###.##.###:#####`、长度 20；**target 值未输出、未持久化**（`endpoint_or_token_persisted: false`）。**过程注记**：该连接建立于 gate 3 复审结论落定之前，属用户预备连接；该期间**未执行任何设备查询**——gate 4 的 `list targets` 与 gate 5 三探针均在其后一次性执行（详见下方追加段） |
| 5 `-TargetBindingConfirm` | pass（**元组逐字 MATCH**） | 三探针（同一次执行，逐字 argv）：① `hdc version` → `Ver: 3.2.0f`；② `hdc -t <T> shell param get const.product.model` → verbatim **9 字节**（含 1 个尾随 ASCII 空格）→ trim 后 `PLA-AL10` → **MATCH（trim 后）**；③ `hdc -t <T> shell param get const.product.software.version` → verbatim **36 字节**、`od -c` 逐字节证据 → trim 后 `PLA-AL10 7.0.0.105(SP10C00E105R7P3)` → **MATCH（逐字）**。对照 CC-5 冻结元组（判据 `:268`）→ `verdict = pass-tuple-bind-confirmed`（详见下方追加段） |

**判据基线**：`docs/n1b-disc-gate-plan.md` @ git `04cf222`（冻结，`criteria-frozen-2026-09-02`）+ **CC-1**（`criteria-change-1-reviewed-pass-2026-09-05`）+ **CC-2**（`criteria-change-2-reviewed-pass-2026-09-06`，元组重绑）+ **CC-3**（`criteria-change-3-reviewed-pass-2026-09-11`，P1 `dlopen`/`dlsym` 时间盒语义收窄）+ **CC-4**（`criteria-change-4-reviewed-pass-2026-09-11`，D1 `elapsed_ms` 落盘依据 + 467/525 上界法理修正）+ **CC-5**（`criteria-change-5-reviewed-pass-2026-09-11`，设备元组重绑至 `PLA-AL10 7.0.0.105(SP10C00E105R7P3)`，判据 `:268`），**六者全部 `reviewed-pass`**；设备冻结元组 = `PLA-AL10 7.0.0.105(SP10C00E105R7P3)`（判据 `:268`）。

**关键实测数字（freeze-4）**：selftests **207 passed**（0 failed/error）；探针 host 单测 **15 passed**（fresh 重编译 `cargo test --offline --locked`）；A1-A12 **12/12**（被检树指纹 `e6e93916ef18` / 45 文件，与 freeze-3-v4 逐字节相同）；导出 T 符号 **14/14 不变**；制品 `.so` SHA-256 **`5e5408772e75b78f3b01d7a6297bc2ab9fe16a9860ba9c36df248bed667a297c`**（1160224 B；`entry/libs/arm64-v8a/` 与 `probe/target/aarch64-unknown-linux-ohos/release/` 两处一致）。

**gate 3 有效记录构成（审查席裁决）**：freeze-4 原文（`gate3-criteria-conformance-freeze-4-20260911.txt`，SHA-256 `b5aed745a25c3029c86100a2ac5a4ff441a413e2ccea1b3eba5d59e1c3ad4a84`）**+** 更新版沿革勘误（`gate3-freeze4-erratum-lineage-20260911.txt`，SHA-256 `9b85b1c4a4439782c67c7708bff24a7142afaf358e2c999fe167bd2de90bc289`）——两者并存，沿革段 `:27` 表述冲突时以勘误为准；**审查席明确「无需重出 freeze-4」**。勘误只更正 freeze-4 `:27` 的**跨 pair 归属错误**（第三对初版冻结被误写作 `freeze-1`，实为 `gate3-criteria-conformance-freeze-3-20260911.txt`）并新增「术语消歧」节（绑定输入 / 分析结论 / 结论一致性三层面 + 建议统一表述句）；不改写 freeze-4 原文、不触碰任何被冻结资产。

**1 项 minor 与既定处置**：

- **m-1（陈旧注释）**：`spikes/n1b-disc-phys-hap/runner/n1bdisc_fsm.py:75-77` 注释仍写「唯一时间盒豁免 = pthread_join」（CC-3 后该「单一豁免」措辞已收窄，属陈旧）。**freeze 后不得修正**被冻结资产（含注释/元数据）——freeze 后改动资产会撞判据 `:1124` 的 `invalid` 条件（ready freeze 后 HAP/`.so`/runner/配置矩阵/符号清单/marker 集冻结文件 SHA-256 与 freeze 记录不一致即判 invalid）；陈旧注释只能在**本 campaign 结束后**或**下一对 freeze 前**统一更正。

**三 ID 状态**：`AUTH-N1BDISC-PHYS1API26-20260911-0001` / campaign `N1BDISC-PHYS1API26-20260911-0001` / evidence `EV-N1BDISC-PHYS1API26-20260911-0001` 保持 **candidate 态、未消费**（`identity_status: candidate`、`consumed=false`、`reusable=false`、`is_evidence=false`）；**gate 4 起为设备侧**（`tconn` + 恰一次内存级 `list targets`，**须用户本人连接设备**；gate 5 三探针对照 CC-5 重绑元组 `PLA-AL10 7.0.0.105(SP10C00E105R7P3)`）；**gate 13 Live 须用户全新确认**（决议 §4.3.9）。

**收官（2026-09-11）**：第四对 pair host-only 门 **gate 1-3 全部 pass**，**freeze-4 成立**（0 blocker / 0 major / 1 minor，含沿革勘误消解）。三 ID 仍未消费；全程零设备接触、零 HDC 命令、pair 未消费。下一步 **gate 4**（设备侧，须用户本人连接设备）；gate 5 三探针对照 CC-5 重绑元组；gate 13 Live 须用户全新确认。逐门登记随执行继续追加。

**执行提醒**：**设备自动更新须关闭**——首对与第三对两次 gate 5 元组漂移退役均系设备 OTA 所致；执行 gate 4-5 前须关闭设备自动更新，避免再次漂移退役。

### 门 4-5 执行登记追加（第四对，2026-09-11，设备侧；本节为追加，上文不改）

**gate 4（host-prep `tconn` + 恰一次内存级 `list targets`）**：`tconn` 由**用户本人**执行，连接建立于 **2026-09-11 14:05:07 CST**（设备 LAN 地址已脱敏，形状 `###.###.##.###:#####`）；**用户已确认该连接即为 gate 4 的 `tconn`**。主会话执行**恰一次**内存级 `list targets`：`rc=0`、`targets_count=1`、target 形状 `###.###.##.###:#####`、长度 20；**target 值未输出、未持久化**（`endpoint_or_token_persisted: false`）。

**gate 5（`-TargetBindingConfirm`）三探针逐字 argv 与实测值**（同一次执行）：

1. `hdc version` → `Ver: 3.2.0f`
2. `hdc -t <T> shell param get const.product.model` → verbatim **9 字节**（含 1 个尾随 ASCII 空格）→ **只剥尾随空白** trim 后 = `PLA-AL10` → **MATCH（trim 后）**
3. `hdc -t <T> shell param get const.product.software.version` → verbatim **36 字节**，`od -c` 逐字节证据（末行行尾为 1 个空格字节，此处以 `␠` 标记以免行尾空白；逐字原文见仓外 record 的 `measured.software_version_verbatim_od_c`）：

```text
0000000   P   L   A   -   A   L   1   0       7   .   0   .   0   .   1
0000020   0   5   (   S   P   1   0   C   0   0   E   1   0   5   R   7
0000040   P   3   )   ␠
0000044
```

   trim 后 = `PLA-AL10 7.0.0.105(SP10C00E105R7P3)` → **MATCH（逐字）**。比对纪律：**只剥尾随空白、内部空格必须保留**——前两对曾因 trim 误删分隔空格而误报 DRIFT。对照 CC-5 冻结元组（判据 `:268`）→ **`verdict = pass-tuple-bind-confirmed`**。

**过程注记（tconn 时点）**：该 `tconn` 建立于 **14:05:07**，**早于 gate 3 复审结论落定**，属用户为 gate 4 预备而建立的**预备连接**；该期间**未执行任何设备查询**——gate 4 的 `list targets` 与 gate 5 三探针均在其后一次性执行。用户已确认该连接即为 gate 4 的 `tconn`，故按过程注记如实登记。

**gate 1 HDC0 表述澄清（2026-09-11 追加，重要）**：本节 gate 1 一栏记录的「登记时只读探针命中 1 个 `hdc` server」须按下述读法理解——**gate 1 实际执行于 13:42（当时 HDC0 = 0，已核验）**；登记时（约 14:18）探针所见的该 server 系 **14:05:07 用户为 gate 4 预备而建立的连接**，**属 gate 1 执行之后的状态变化**，**不构成 gate 1 的 HDC0 不合格**（gate 1 执行时点 HDC0 为 0，合规）。

**仓外 confirmation record**：`~/harmonyos-signing/netbird-n1bdisc/records/target-binding-confirmation-pair4-20260911-0001.json`，sha256 `80f110087bfa6e0cb365d4f0585e99a2297dbd1c9a7debafbb8ec16e59c90528`（同名 `.sha256` sidecar，`sha256sum -c` → `OK`）。

**清理**：`hdc kill` 已由父会话执行；HDC0 已复核归零（`pgrep -x hdc -a`、`ps -eo pid,comm | awk '$2=="hdc"'`、`ss -ltnp | grep 8710` 均零匹配、无 8710 监听）。

**下一步**：**gates 6-12（host-only，HDC0 已恢复）**——gate 8 用既有 AGC profile 重新签名 HAP；**gate 13 Live 须用户全新确认**（决议 §4.3.9）。三 ID 保持 candidate 态、`consumed=false`、未执行 DryRun/Live。

> 注（2026-09-12 追加）：本句已被本文件末尾「终态追加登记（2026-09-12，只追加不改写）」一节取代，见该节；原句逐字保留如上。

---

## 终态追加登记（2026-09-12，只追加不改写）

> 本节 2026-09-12 追加：只追加、不改写上文任何内容；只登记已发生事实及其出处，不构成任何新授权、不重试、不分配新 ID。以下 sha256 均为 2026-09-12 以 `sha256sum` 现算，无手写值。

1. **gate 6-12 已于 2026-09-11 晚在仓外执行**：`~/harmonyos-signing/netbird-n1bdisc/reviews/gate12-dryrun-review-pair4-20260911.txt`（生成时刻 2026-09-11 19:30:40 CST；OpenAI 判断席 gpt-5.6-sol；结论 PASS，0 blocker / 0 major / 1 minor），sha256 `2fd116b161e51a0a44d96ca6f281889fbd817956389dc1649674a7d85bf2bd8f`（实算）。
2. **gate 13 Live 已执行且终态 fail**：StartEntry 发出后 300 s allow-box 内 0 个首 marker（E6 allow-deadline）；独立审查 0 blocker / 3 major / 1 minor，`terminal_verdict=fail`。出处：`~/harmonyos-signing/netbird-n1bdisc/reviews/gate13-live-terminal-review-pair4-20260911.txt`（2026-09-11 20:20:55 CST），sha256 `5781823f1a1b63d014540b03096e0879b533232df29a8282ac47e44cac66429c`（实算）。
3. **终态登记**：`~/harmonyos-signing/netbird-n1bdisc/records/terminal-disposition-pair4-20260911.json`（2026-09-11 20:24:49 CST）：`verdict=fail`、`identity_status=consumed-terminal-fail`、`consumed=true`、`reusable=false`、`retry_allowed=false`、`successor_auth=none`、`code_sha 0616aa7`（全串 `0616aa7804364c838e98d81877276f62d326b496`）；sha256 `bb46b3be9f0ec603e018c002b3b3a5a29a0db225aa45b810422236fd4cb1c23d`（实算，同名 `.sha256` sidecar `sha256sum -c` → `OK`）。**三 ID 已被单次 Live 终态消费：不得重试、不得复用；任何新 Live / 新 campaign 须用户全新授权新三 ID 并重走门序列。**
4. **Live fail 根因与修复**：NAPI 函数只 `napi_create_function` 未挂 exports → `PROBE_FLOW_ERROR|TypeError: version is not callable`；修复提交 `6014363`（2026-09-11 20:36:28 +0800，`fix(n1bdisc): attach NAPI functions to module exports`；出处：仓A `git log` + 上述 gate13 终态审查/登记）。
5. **冻结漂移（重要）**：pair4 冻结 `code_sha 0616aa78` 与当前 main `33a987d` 已不一致；窗口内起点提交 `97ab073`（2026-09-11 20:18:16 +0800）之后的 9 个提交（自冻结以来共 10 个）全部改冻结实现 `spikes/n1b-disc-phys-hap/`（实算：`git log --format='%h' 0616aa78..33a987d -- spikes/n1b-disc-phys-hap/ | wc -l` = 10；`97ab073..33a987d` = 9），其中 `e8e2cf9`（2026-09-12 01:33:19）引入的 MR4 **不在冻结 MR 表**（`docs/n1b-disc-gate-plan.md:453-456` 只有 MR1/MR1B/MR2/MR3；`:457` 为 MB1 兜底行），触 `:464` 硬停止条款 → **后继工作须重新 freeze + 重审（跨厂商独立审查）**。
6. **09-12 ad-hoc 真机验证（N 轮 `20260912T191913`）**：总判定由**联调脚本自身**给出：`N1BDISC_WG_FWD_END|verdict=pass|tun_rx=108|sent=112|recv=19|tun_write=7|sink_recv=1|elapsed_ms=90649|reason=deadline`（设备时刻 CST 2026-09-12 19:21:07.167；仓外 `diagnostics/n1bdisc-dns-test-20260912T191913/run.log:70`，run.log sha256 `9be9681cd0f897d023f4dff3b79be84ddd85b3a05eed9538db4448ce47c9d0f8` 实算）。**必须写明：这是 ad-hoc 开发验证脚本的判定，不是任何门（gate）的判定，`is_evidence:false`，不得作为门结论或判据输入。**
7. **证据归档**：`~/harmonyos-signing/netbird-n1bdisc/archive-manifest-20260912.md`，sha256 `5912e328fc08e029c41149dc2bbcd65d3641fc9bab130c154595aec686d939e9`（实算）：39 个证据文件 / 1,411,761 字节；39 个证据 sidecar 逐个 `sha256sum -c` OK，含清单自身 sidecar 全量 40 sidecar 终校验 OK（清单 `:74-76`、`:91`）。
8. **治理定性（独立跨厂商 T0 判断席）**：09-11 20:18 → 09-12 19:36 的真机操作（HAP 安装/替换 ≥11 次、uitest 点击 ≥9 次含 VPN「允许」弹窗、hdc 拉日志 ≥10 次、`run-dns-joint-test.sh --execute` 真机 3 次）定性为 **(b) 有授权主体实质同意、但无合规 AUTH 登记的越界执行 = 治理登记缺口**，不是无授权越权。许可来源：用户 09-11 22:38:53 / 22:46:28 的逐字指示「（为什么不能）直接写代码在实际设备上验证VPN功能」（口述，未登记为 AUTH；逐字引文与出处见仓外事后登记 `authority_basis` 节——转录当前字节 `today-A-part1.md:481/:490`，T0 裁定与取证报告引作 `:86/:95`，该引用行号偏差如实记录）。违规依据（逐字引文见仓外登记 `governance_violation` 节）：`docs/evidence-schema.md:118`、`docs/native-nx-governance.md:53`、`docs/native-nx-n1b-adjudication.md:132`、`docs/n1b-disc-gate-plan.md:253`。加重情节两条：① 三份 diagnostics AUTH 系 agent 自签（log-prep JSON 自陈 `the id was assigned by the agent`），且 agent 自行出具 main_session_ruling；② 冻结实现被改并 push。T0 裁定归档：`~/harmonyos-signing/netbird-n1bdisc/reviews/t0-auth-boundary-ruling-20260912.md`（sha256 `080d4f44356d0fda8398c68c5fc8af8d4003e6669870c93a1d35efeb8490869a` 实算）；只读取证报告归档：`~/harmonyos-signing/netbird-n1bdisc/diagnostics/auth-boundary-facts-20260912.md`（sha256 `73c07729719e279c16216c8bf8c6d1394970c5c6447db54f184d40bcaba5f8c6` 实算）。
9. **事后登记与提案（均在仓外，本登记不批准任何事）**：事后登记 `~/harmonyos-signing/netbird-n1bdisc/records/retrospective-device-activity-20260912.json`（性质声明 `is_new_authorization=false`、`is_evidence=false`、`grants_nothing=true`、`ratifies_nothing=true`、`produces_n1bdisc_verdict=false`、`creates_claim=false`、`uses_pair4_ids=false`、`modifies_terminal_disposition=false`），sha256 `90feab1f8b7af8f475a00f807e5e41246fa29ef76163a537b6152a52e98aef85`（实算，含 `.sha256` sidecar）；诊断授权类别提案 `~/harmonyos-signing/netbird-n1bdisc/proposals/diagnostic-authorization-class-20260912.md`（**提案，待用户批准/判据变更流程**）。

**结论性状态**：本三 ID（`AUTH-N1BDISC-PHYS1API26-20260911-0001` / campaign / `EV-N1BDISC-PHYS1API26-20260911-0001`）已终态消费（fail），上文「下一步 = gates 6-12」「consumed=false、未执行 DryRun/Live」一句自此被本节事实超越（原句逐字保留）；在用户全新授权新三 ID、且重新 freeze + 重审完成之前，不存在可继续的 N1BDISC 物理路径。
