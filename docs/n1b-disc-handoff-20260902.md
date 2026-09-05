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
