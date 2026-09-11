# N1BDISC 交接文档（2026-09-02 判据冻结后）

> 供新会话接续。本会话完成了 N1BDISC 发现实验判据的起草与 26 轮三席跨厂商审查循环，**判据已冻结**（用户授权）。下一步是 ID 分配申请——**尚未开始**。

## 一、当前状态（一眼版）

| 项 | 状态 |
|---|---|
| 判据文档 | `docs/n1b-disc-gate-plan.md`（1784 行，git `04cf222`，状态行 `criteria-frozen-2026-09-02`） |
| 审查登记册 | `docs/n1b-disc-r8-review-register.md`（26 轮全程记录，冻结记录在末尾） |
| 审查结论 | 第二十六轮三席全 pass：sol 0B/0M/0m（首次三零票）、deepseek 0B/3M（勘误级）、grok 0B/2M/3m——**0 blocker 达成** |
| blocker 轨迹 | 12→8→4→3→5→3→2→1→2→3→5→1→2→1→0（26 轮） |
| 判据冻结 | ✅ 已冻结（用户 2026-09-02 授权；冻结前 grok 两项 M 已落地） |
| AUTH/pair、evidence ID | ❌ **未分配**——需用户逐项显式授权 |
| 物理执行（DryRun/Live） | ❌ 未请求、未执行 |
| N1b r2 判据写作 | ❌ 未开始 |

## 二、硬边界（不可违反）

1. **冻结判据的任何修改 = 判据变更**，必须重新走跨厂商隔离独立审查（已写入冻结登记块）。
2. **ID 分配与一切物理执行以用户逐项显式授权为前置**（决议 §4.2/§4.4）。冻结 ≠ 授权执行。
3. 审查席必须跨厂商隔离（决议 §4.3.7）：grok-4.6（xai）/ gpt-5.6-sol（openai）/ deepseek-v4-pro（deepseek-official），主会话模型家族不得充任。
4. 实验单次执行、不可重试、不可换 ID——判据的全部设计围绕「烧掉不可复用 ID」这个损失函数。

## 三、治理结构（关键文件与决策链）

- **`docs/native-nx-n1b-adjudication.md`**（`ADJ-T0-N1B-20260831-0001`）：最高决议。§4.2 verdict 语义（pass 效力封死、不得引用为平台结论）、§4.3.7 审查隔离、§4.4 N1b 复测关系。
- **`docs/evidence-schema.md`**：门代码已扩展（N1BDISC 在 `:29` 登记），但 **ID 分配仍以判据冻结 + 0 blocker + 用户显式授权为前置**。
- **`docs/n1b-disc-r9-death-facts-spec.md`**：已归档（`spec-archived-superseded-by-criteria-text`），冲突以判据正文为准。

## 四、判据核心设计（新会话需要知道的架构）

1. **verdict 只评价基础设施与完整性**；平台行为永不 fail；三态记录（observed-true/false/unobservable(cause=…)）；全函数；不许 fail-open，成功终态必须能 pass。
2. **七分量证据向量**（r9 归因停机改造）：`process_death_observed` / `last_visible_site` / `fault_type_observed` / `signal_observed` / `destroy_call_state`（五态）/ `marker_tail_state`（五值）/ `probe_crash_signature_observed`（三支闭集）。fail 闭集 F1–F6、F8、F9（F7 归 invalid 轴）。
3. **D-W 派生体系**（r14 起四步有序互斥）：合法域门 → 0/0b 前置检查 → 未知位门 → 普通 1-11；`dw_return_class` 20 值 = 13 类 + skip 2 + 死亡收口 3 + poll-never + flag-race；`dw_join_result` 10 值。
4. **cut-state 体系**（r22 采纳、r23-r25 完善——本会话最深的架构决策）：
   - POST 新增 `worker_terminal_at_p12`（cut 记录）= P12 写 class 时终态标志读值；
   - runner 对 JT=1 格校验 cut-aware：(A) cut=true 正常重建比对 + RETURN/EXIT 必在；(B) cut=false **不从终态 capture 重建**（迟到完成合法非矛盾），只验分支一致性 + 允许集闭表（poll raw 同 cause、watchdog=⑤ 恰一值）；(C) JT=0 不受影响；
   - watchdog ①-⑤ 表适用域：JT=0 / cut=true / pre-only 用本表；JT=1∧cut=false 不走（P12 直接赋 ⑤ = cut-imputed，A12 五输出绑同一次 F load）；
   - P12 冻结发射序：① RACEWIN → ② 落 cause → ③ POST，禁止 POST 后再发任何 marker。
5. **静态断言 A1-A12**（gate 3 执行面）：A9 轮询退出路径、A10 join-timeout 分支零 fd_dup 操作、A11 poll raw 单读同源两写、A12 五输出与 F 分支控制流绑定。
6. **计数**：class 20 / join 10 / 单调钟 14 / N1BDISC 字面 58（56 active + 2 豁免）。

## 五、审查流程的运作纪律（新会话若需再审必须遵守）

1. **轮次制**：每轮三席独立审查 → 主会话逐条核实（不轻信——本会话抓到审查员报错数字/归属多次）→ 修复（拆包派执行层）→ 下一轮。
2. **席位分工**：grok/sol 全文四层次（修复落地核对 / 新声明走查 / 构造性反例 / 程序核对）；deepseek 只做机械枚举（16 格矩阵类任务，广判断任务会失败）。
3. **审查提示词必须包含**：最坏读法、算术重算、"你的定量主张会被独立核实"、pass 票必须列具体高风险点、"每个新声明首轮被证伪"警示。
4. **修复纪律**：审查在飞不落盘（版本纪律）；「同一钉多处落盘」——r16 字段传播/r23 轴传播/r24 规则位传播四次的同型教训；执行层失败 2 次后主会话才接手。
5. **登记册**：每轮终账（票型、去重 blocker、轨迹、主会话裁量记录）写入 register 对应「九之N」节；裁量与分歧（含主会话判错被驳回的）如实登记。
6. **行宽 ≤400 字符**；标注 `rN`；修订登记头只增不改。

## 六、冻结说明登记项（在案不阻塞，冻结后修改须走判据变更流程）

- grok m-01：`:688`「EXIT 只喂 ④」无适用域指针（残留误引面）
- grok m-02：A12「无二次读 F」全称 vs `:753` 盒到期再读（可读实现无碍）
- grok m-03：cut-imputed ⑤ 与表内 ⑤ 字面同理据不同（已定名记录）
- deepseek r26 M-01/02/03：勘误级（派发稿框定/行号/拼法计数）
- `:871` 粗体嵌套渲染问题（冻结收尾残留，改动须走变更流程）
- 历史裁量与程序失误登记（#1–#32）全部在 register 内

## 七、下一步（按序、每步需用户授权）

1. **ID 分配申请**：向用户报请分配 AUTH/pair 与 evidence ID（判据冻结 + 0 blocker 已满足前置，差的只是用户显式授权）。申请材料需引用：判据 git `04cf222`、冻结记录、决议 §4.2。
2. **runner 实现与静态断言核对**：A1-A12 是 freeze 前实现的源码层检查面。
3. **DryRun（门 11）**：`is_evidence=false` + HDC0 host-only——只能验证 runner/parser/状态机，**不能**验证设备侧行为（禁止用 DryRun 为残余风险背书——程序失误 #19 的教训）。
4. **Live 执行**：逐项用户授权。
5. **N1b r2 判据**：以 DISC 事实为预注册设计输入（决议 §4.4）。

## 八、本会话的经验教训速查（新会话校准用）

- **新声明首轮被证伪**是 r14-r25 每轮的规律——任何新规则/新表/新裁定的首轮审查要加倍怀疑。
- **点时观测 vs 终态重建**（r19-r25 的 blocker 族）：两者比较必有 TOCTOU，解法是记录 cut 状态、按 cut 比对。
- **单轴 vs 全轴**（r23 教训）：修复的传播必须在全部受影响字段/规则位闭合，「同一钉两处落盘」。
- 席位失败模式：sol 会连接中断（3 次后主会话接手并留局限声明，下一轮原席复核）；glm-5.3-flash（执行层）偶发无消息失败（2 次后接手）；deepseek 只适合机械枚举。
- 主会话 32 次同型失误登记在 register——批量落盘时未核对数量/引用/传播是主要模式，新会话自警。

## 九、工具链迁移补充（2026-09-05 追加，上文不改）

> 本节为 2026-09-05 的追加登记，不修改上文任何历史内容；与上文表述冲突时，冻结判据与决议为准。

- **当前新开发一律使用 CLT `26.0.0.821`（HarmonyOS SDK `26.0.0.105`，API 26 Release）**：唯一版本基线见 [`docs/toolchain-baseline.md`](toolchain-baseline.md)；稳定链 `6.1.1.290` 与 Beta 链 `26.0.0.461` 已退役、本机不存在，本文涉及其的历史叙述按写作时点理解。
- **SDK 锚点对账 9/9 PASS**：冻结判据正文引用的 SDK/sysroot 锚点主张已在 821 实际内容上逐项复核一致，见 [`docs/n1bdisc-sdk-anchor-reconciliation-20260905.md`](n1bdisc-sdk-anchor-reconciliation-20260905.md)（只读对账存档：非设备 evidence、不占用 evidence ID）。
- **冻结判据正文不改**：该对账不构成判据修订、不触发重新审查义务；461→821 的行号漂移已在对账 §3.8 照实登记，判据的任何修改仍按硬边界第 1 条走判据变更流程。
- **Rust 工具链已安装验证（2026-09-05）**：前缀 `$HOME/rust`，rustc/cargo `1.98.1`、rustup `1.29.1`，OHOS `aarch64-unknown-linux-ohos` 与 `x86_64-unknown-linux-ohos` targets 已装；`$HOME/rust/env.sh` 由 `.zshrc` 条件加载，不设置全局 `CC`/`AR`，不影响系统 Node.js。登记见 [`docs/toolchain-baseline.md`](toolchain-baseline.md)；工具链 smoke：API 26 HAP 与 BoringTun OHOS 均 PASS，见 [`docs/toolchain-smoke-20260905.md`](toolchain-smoke-20260905.md)。
- **AGC profile 已完成（2026-09-05，用户授权会话内）；vendor/lock freeze 仍是后续前置**：AGC 侧已创建 HarmonyOS 应用 NetBird N1BDISC（包名 `cn.alfadb.netbird.n1bdisc`，与冻结值逐字一致；APP ID `6917615611100883458`；所属项目 NetBird HarmonyOS Preflight）与调试 Profile「NetBird N1BDISC Debug」（类型 debug；证书复用 NetBird E3 Debug，SHA-256 指纹 `F9:A6:EB:86:4E:C1:D1:9A:A3:0B:6D:7B:FC:B2:AF:A6:3C:09:0A:5C:CC:1D:17:73:D5:46:F3:5D:B4:9D:72:F0`，与历史 E3 A/G0 profile 内嵌证书一致；设备绑定已注册 PHYS-1（UDID `C863D22EDE37…D833`），失效 2027-08-06）；profile 文件 `/home/worker/harmonyos-signing/netbird-n1bdisc/profiles/NetBird N1BDISC Debug.p7b`（4117 字节，SHA-256 `49c99c567d0e8693269a1bda74f5d921ae0771e2cc0859512d9b3cd950df19a4`），`hap-sign-tool verify-profile` PASS。注意点：① AGC 开放能力「定位服务」为平台强制默认勾选（is-disabled is-checked，无法取消），非本项目选择，对调试签名无影响；② 仓外既有文件 `netbird-e3/cert/NetBird E3 Debug.cer` 实际内容为华为 CBG Root CA G2 根证书（序列号 `4A18699F9D7D8CD0`），文件名有误导，本地签名应使用 profile 内嵌开发证书或 `verify-temp/c6acae7-host-remediation/profile-dev-cert.pem`，勿改该既有文件。Rust 工具链的技术前置已闭合（安装验证 + smoke PASS）；BoringTun 同版本 checksum / `--offline --locked` 构建链已证明可构建；但 N1BDISC 的正式 vendor/lock/同产物 freeze 仍未开始——仍是 §七 第 1 步（ID 分配申请）与第 2 步（runner 实现）之前须另行处理的后续前置项。
- **ID 分配已完成（2026-09-05 用户显式授权「授权分配」——仅 ID 分配，不含测量/设备命令/DryRun/Live）**：`AUTH-N1BDISC-PHYS1API26-20260905-0001` / campaign `N1BDISC-PHYS1API26-20260905-0001` / evidence `EV-N1BDISC-PHYS1API26-20260905-0001`；`attempt: initial`、`retry: N/A`，候选态未消费；正式授权登记见 [`docs/evidence/n1bdisc-authorization-2026-09-05-0001.md`](evidence/n1bdisc-authorization-2026-09-05-0001.md)（含授权状态 YAML、硬边界、失败代价披露与签名链）。据此 **§七 第 1 步（ID 分配申请）已完成**；§七 其余步骤（runner 实现、DryRun、Live、N1b r2 判据）仍须逐项用户授权，当前保持 host-only 边界。本节为唯一更新点，第一至八节原文不动。
- **gate 3 静态审查待办（已裁决并闭合，2026-09-06）**：`entry/src/main/ets/vpnextensionability/N1BDiscVpnExtensionAbility.ets` 三处 `:463` 锚在冻结编号下即指向空行（语义应为「至多一个在途 create()」）。gate 3 静态审查席（grok，跨厂商隔离）裁决：**语义读作 `:468`**；注释校正属 freeze 后编辑性修正，已随本更新落地（L25/L56/L80 `:463`→`:468`），不回写 freeze-1 记录。
- **gate 1-3 已执行（2026-09-06，host-only）**：gate 1 clean HEAD `11fabd6`（code_sha=`11fabd66d66917b2fa0404f5ddbf53a73c288ae1`，与 origin/main 同步）+ HDC0（发现并清理一个 2026-09-05 遗留 hdc server，`hdc kill` 后归零，零设备接触）；gate 2 audit-1 仓外双文件+sha256（`82b24b90…54f6`，候选 pair 零消费）；gate 3 freeze-1 落盘（`~/harmonyos-signing/netbird-n1bdisc/freeze/gate3-criteria-conformance-freeze-1.txt`，sha256 `05453491…16e4`，绑 code_sha/14 符号誊录/A1-A12 12 PASS（指纹 `19e67c52bf9b`）/时间盒/Fault_Type 冻结候选集+未实测依赖登记）+ grok 席静态审查 **PASS（0 blocker/0 major/2 minor）**，freeze-1 成立。执行登记表见授权登记文档「门序列执行登记」。
- **gate 4-5 已执行（2026-09-06，设备侧）：gate 5 元组漂移 → blocked + 首对 pair 退役**：gate 4 用户本人 tconn + 主会话恰一次内存级 `list targets`（targets_count=1，target 未输出未持久化）。gate 5 三探针：model `PLA-AL10` **MATCH**；software **实测 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)` ≠ 冻结 `PLA-AL10 7.0.0.102(SP8C00E102R7P3)`**（设备在 2026-08-30 G0 gate-5 实测后 OTA 升级）——按判据 :266-267/:1436 裁 blocked record + 退役 pair。blocked record：`~/harmonyos-signing/netbird-n1bdisc/records/target-binding-confirmation-20260906-0001.json`（SHA-256 `d2f59da0…c7fbd`）；`hdc kill` 后 HDC0 归零。首对 AUTH `consumed-blocked`（campaign 未执行，无后继 AUTH）。授权登记文档「门序列执行登记」含 gate 4-5 行与收官注记。
- **CC-2 元组重绑已完成（2026-09-06，用户授权；deepseek 席重审通过）**：首对 pair gate 5 漂移退役后，判据冻结元组重绑至 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)`（文末登记块、零锚位移；m-1/m-2 已处置）；登记册补 CC-2 节。后续路径：新 AUTH/pair/evidence 三 ID（20260906 日期段）+ 从 gate 1 重走门序列；实现资产与 freeze-1 符合性结论可被新治理引用。
- **跨文档待办（CC-2 重审 o-1）**：`docs/n1b-gate-plan.md:25`（N1b 正式门）仍冻结 `7.0.0.102(SP8C00E102R7P3)`，同一设备 OTA 后同样漂移，须经其自身判据变更流程重绑（非 CC-2 范围；DISC 事实产出前 N1b r2 尚未开始，暂不阻塞）。
- **第二对 pair gate 1-5 执行事实（2026-09-11 追加；事件 2026-09-06）**：gate 5 元组实测 **MATCH** `PLA-AL10 7.0.0.105(SP6C00E105R7P3)`（CC-2 重绑冻结值，仅尾随空白差异）；gate 3 首审 **FAIL**（sol 席跨厂商隔离，7 blocker / 2 major）→ 整改 commit `630efe0` → 复审 **PASS**（freeze-2-v2 成立；FAIL 不消费 pair）；gate 1 随整改重记 pass、gate 2 audit-1 沿用。逐门登记见 [第二对授权登记「门序列执行登记」](evidence/n1bdisc-authorization-2026-09-06-0001.md)。
- **第二对 pair gate 6-12 执行与终态退役（2026-09-11 追加；事件 2026-09-06）**：gate 7 审查（deepseek 席）、gate 8 最终 ready freeze + 签名 HAP、gate 9 audit-2、gate 10 selftests、gate 11 DryRun、gate 12 离线 DryRun 独立审查 **PASS**（opus 席，0 blocker / 0 major / 3 minor / 1 note）；**随后因 gate 4 恰一次违规退役**——gate 4 `list targets` 实际执行两次，违反判据「恰一次」约束，用户不追认、审查席无豁免权；**未执行 Live、未消费测量**。仓外退役记录 `~/harmonyos-signing/netbird-n1bdisc/reviews/pair2-retirement-exact-once.json`（sha256 `5dfd19c39ef45780ad1bb6e42a6afb77d886851f3d9d7ebab1e25bcda0e8c652`）；该对门序表与 gate 12 摘要仅作历史审计材料，**不绑定第三对**。
- **第三对 pair 三 ID 已分配（候选态未消费）+ 候选实现落地（2026-09-11 追加；ID 授权登记 2026-09-07）**：`AUTH-N1BDISC-PHYS1API26-20260906-0002` / campaign `N1BDISC-PHYS1API26-20260906-0002` / evidence `EV-N1BDISC-PHYS1API26-20260906-0002`（candidate 态，`consumed=false`/`reusable=false`，门未执行），登记见 [evidence/n1bdisc-authorization-2026-09-06-0002.md](evidence/n1bdisc-authorization-2026-09-06-0002.md)；候选可执行链 commit `ea44b87` 已快进合并入 `main` 并推送。**下一步**：第三对 gate 1-3（host-only）；gate 4-5 需用户重新连接设备；gate 13 Live 须用户全新确认（决议 §4.3.9）。
- **第三对 pair gate 1-3 执行与 freeze-3-v4 成立（2026-09-11 追加）**：gate 1 clean HEAD `5a56ba3`（`code_sha = 5a56ba30e9d672ac0e543cb1e0175f03a853509c`；`git status --porcelain` 空、与 origin/main 同步 left/right=0/0）+ HDC0 探针零匹配（`ps -eo pid,args` 经 `grep -E '[h]dc'` 过滤无匹配 rc=1；无 hdc 进程、未执行 `hdc kill`、零设备接触）；gate 2 audit-1 仓外双文件（`~/harmonyos-signing/netbird-n1bdisc/audit/pair-20260906-0002/new-pair-id-consumption-audit-1.txt`，4525 B，sha256 `35825ebde9a267cf78f14202c983894021d381157643bba0221d558082274a4c`）+ 同名 sidecar；**新 code_sha 有界 grep 复检**（`README.md docs scripts spikes`，三 ID 9 行/4 文件全为登记性引用，消费标记零命中）；gate 3 **freeze-3-v4 成立**（`~/harmonyos-signing/netbird-n1bdisc/freeze/gate3-criteria-conformance-freeze-3-v4-20260911.txt`，sha256 `b0a018f424d4f87513bbae26091c8b5324c03189103214c93148a88caa191894`；附件 staticcheck `ae3bd883fcc61e08a8f7eaaa22df79ef9af7f7b34b0b8f49ae58080c6339cd54` / 符号誊录 `ad61a4fb147482379adda6b1ee326a88f917ef79acaa5efe00d88a3560df7f57` / 维度复检 `2258fcf243d7fe5716d187f67b4c8dc88d88133f955658acf477599280a121fd`），跨厂商隔离复审 **0 blocker / 0 major / 2 minor**。关键实测：selftests **207 passed**、探针 host 单测 **15 passed**（fresh 重编译）、A1-A12 **12/12**（指纹 `e6e93916ef18` / 45 文件）、14 符号不变、`.so` sha256 `5e5408772e75b78f3b01d7a6297bc2ab9fe16a9860ba9c36df248bed667a297c`（1160224 B，`entry/libs` 与 `probe/target/.../release/` 两处一致）。判据基线 = `04cf222` + CC-1 + CC-2 + CC-3 + CC-4（全 reviewed-pass）。两条 minor：**m-1**（陈旧元数据/注释，**freeze 后不得修正**——会撞判据 `:1124` 资产哈希 invalid 条件；只能本 campaign 结束后或下一对 freeze 前一次性更正）、**m-2**（v4 §12 引用计数：v1/v2 当前实为各 7 文件而非「3+2」，**不改冻结资产**、后续记录更正）；两条另出 **v4 勘误**收口。逐门登记见 [第三对授权登记「门序列执行登记」](evidence/n1bdisc-authorization-2026-09-06-0002.md)。
- **本轮整改链全貌（2026-09-11 追加）**：gate 3 冻结 **v2 FAIL → v3 FAIL**（B-1 `d1_elapsed_ms` 未落盘 / M-1 467 上界法理不自洽 / m-1 陈旧元数据）→ 判据解释裁决 → **CC-3**（P1 时间盒语义收窄，`e53bb3d`）→ **M1**（`Fault_Type` 空值 → `fault-type-unparsable`，`dccd66e`）→ 用户授权**路线甲**（实现持久化 + CC-4，全套）→ `53faa1f` 实现持久化（`N1BDISC_D1_END` payload 追加 `|elapsed_ms=<ms>`、`.so` 重建、新增 selftest）→ CC-4 判据同步（`4b81f53`：`:521` payload 冻结扩展 / `:526` 落盘清单加 `d1_elapsed_ms` / `:1059` 上界→名义预算 + 525 硬到点 / `:1067` 收口法理改写）+ **跨厂商重审通过**（0 blocker / 0 major / 2 minor）→ `5a56ba3` 转正 → **freeze-3-v4 成立**。上述整改链提交（`e53bb3d`/`dccd66e`/`53faa1f`/`4b81f53`/`5a56ba3`）均已推送；pair 全程未消费。
- **下一步 = gate 4（2026-09-11 追加）**：设备侧 `tconn` + **恰一次**内存级 `list targets`；**须用户本人连接设备**；gate 5 三探针对照 CC-2 重绑元组 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)`；**gate 13 Live 须用户全新确认**（决议 §4.3.9）。
- **执行注意项（本对踩过的坑，务必写入，供后续 pair 复用；2026-09-11 追加）**：
  1. **gate 2 audit 双文件误报 ×2**（freeze-3-v2、v3 两次误称「本对未另出双文件」，均被勘误）：根因是并行工作时时序盲区、只看了上一对的目录；冻结记录必须**现场检索本对 audit 目录**再下结论。
  2. **Rust 工具链误判 ×1**（freeze-3-v3 曾写「未安装」）：Rust 装在独立前缀 `/home/worker/rust`，须 `. /home/worker/rust/env.sh` 才进 PATH。
  3. **探针 `.so` 重建必须带交叉环境**：裸跑 `cargo build` 会因 ring build script 回退 host `cc` 而链接失败（`incompatible with aarch64linux`）；须按 `probe/build.sh` 补齐 `CC_/CXX_/AR_aarch64_unknown_linux_ohos`。
  4. **判据文档登记块一律文末追加**：文首插入会造成全文件 `:NNNN` 锚位移（CC-1 曾如此）。
  5. **冻结之后不得改动被冻结资产**（含注释/元数据），否则撞判据 `:1124` 资产哈希 invalid 条件；陈旧注释只能在 campaign 结束后或下一对 freeze 前一次性更正。

- **第三对 pair gate 4-5 执行与元组漂移退役（2026-09-11 追加；事件 2026-09-11）**：用户本人 `tconn`；主会话执行**恰一次**内存级 `list targets`（`rc=0`、`targets_count=1`、target 形状 `###.###.##.###:#####`，长度 20；**target 值未输出、未持久化**）。gate 5 三探针：`hdc version` → `Ver: 3.2.0f`；`hdc -t <T> shell param get const.product.model` → 含尾随空白（trim 后 `PLA-AL10`，**MATCH**；首次自动比对因未剥尾随空白误报 DRIFT，属**比对脚本缺陷**）；`hdc -t <T> shell param get const.product.software.version` → `od -c` 逐字节 **36 字节**：`PLA-AL10 7.0.0.105(SP10C00E105R7P3)` + 1 个尾随空格（trim 后 `PLA-AL10 7.0.0.105(SP10C00E105R7P3)`）。**漂移**：数字版本 `7.0.0.105` 相同，构建分支标签 `SP6C00E105R7P3` → `SP10C00E105R7P3` 不同；与冻结元组 `PLA-AL10 7.0.0.105(SP6C00E105R7P3)`（CC-2，判据 `:268`）不符。**历史交叉印证（排除抄录错误）**：仓外两份 2026-09-06 记录（`records/target-binding-confirmation-20260906-0001.json` 的 `measured_target_tuple.software_version_verbatim`、`records/target-binding-confirmation-pair2-20260906-0002.json` 的 `measured.software_version_trimmed` 与 `comparison.software_version = "MATCH (逐字)"`）均逐字记为 `SP6C00E105R7P3` → 本漂移为**设备侧真实变更**（09-06 与 09-11 之间再次更新），非判据抄录错误。按判据 `:1436`/`:1162` 裁 `blocked-tuple-drift` → **第三对 pair `retired-terminal`**：`consumed: false`（未测量/未 Live）、`reusable: false`、**无后继 AUTH**；`hdc kill` 已执行、HDC0 归零。仓外 blocked record：`~/harmonyos-signing/netbird-n1bdisc/records/target-binding-confirmation-pair3-20260911.json`（sha256 `aa664446723139c388176d026c8c0be64927f695d6b4c3abb767d475c7fbba8e`，同名 sidecar）；仓内终态登记见 [第三对授权登记「终态处置」](evidence/n1bdisc-authorization-2026-09-06-0002.md)。**下一步待用户决策**：是否再次 rebind 到新元组并以**新三 ID** 重走门序列（gate 1 起），或暂停 N1BDISC 物理路径——本对无后继 AUTH，gate 1-3 成果（freeze-3-v4 等）仅作可复用资产保留、不构成继续执行依据。（本条取代本节前文「下一步 = gate 4」条目。）
