# N1BDISC 判据 r8 —— 第九轮跨厂商独立审查发现登记

最后核验：2026-09-01 ｜ 状态：`findings-consolidated-pending-t0-adjudication`

被审对象：`docs/n1b-disc-gate-plan.md` @ commit `c9240a5`（状态行 `criteria-r8-pending-independent-review`）。

本轮三席**全部返回、全部判 fail**。这是 r0 以来第一次三席齐返。去重后 **9 条 blocker、5 条 major、2 条 minor**。

## 一、席位与投票

| 席 | 模型 | 厂商 | 范围 | 结论 | 计数 |
|---|---|---|---|---|---|
| A | grok-4.6 | xai | 全文 | fail | 2 B / 2 M / 2 m |
| B | gpt-5.6-sol | openai | 全文 | fail | 6 B / 3 M / 0 m |
| C | deepseek-v4-pro | deepseek | 仅 r8 增量（5 个定位点） | fail | 2 B / 1 M |

席 C 此前连续四轮空输出、已记 attempt-not-counted。本轮把范围从 1142 行全文收窄到 5 个可 grep 定位点后正常返回并产出高质量发现。**结论：空输出与单次派发的判断密度相关，与模型档位无关**（与 glm-5.3-flash 在 r6 的失败曲线同型）。此结论已可复用于后续派发切分。

席 A 中途失败一次（截断），续跑时附分段读取协议（禁止 `cat` 整篇、改用 `read` 的 offset/limit 每段 ≤250 行）后完成。

## 二、Blocker（去重后 9 条）

### BL-1 死因分类行 2 是空集，且与 gate 10 ⑪ 自相矛盾
**席 A B-01 + 席 B B-01 独立收敛；席 C B-02 为其下层症状。主会话已独立核实成立。**

推导链：
1. 行 2 命中条件 = 死亡位点 P6 ∧ `Fault_Type=APPFREEZE` ∧ `elapsed_proxy > 27000`（`n1b-disc-gate-plan.md:869`）。
2. 全文 marker 清单中 D7 阶段**只有** `D7_BEGIN`/`D7_END` 两枚，无中途进度 marker（主会话对全部 57 个 `N1BDISC_*` 字面量做了穷尽提取核验）。
3. `D7_END` 在死亡路径上永不发出（`:868`）。
4. 由 2+3：死亡位点判为 P6 时，最后可见阶段 marker **必然**是 `D7_BEGIN`。
5. 于是「静默跨度」与 `elapsed_proxy` **是同一个差值**（同被减数「死亡证据墙钟」、同减数 `D7_BEGIN` 墙钟）。
6. `site_uncertainty` 置位条件 = 静默跨度 > `T_uncertainty(P6) = 25000`（r8 Y3）。
7. X5 规则：`site_uncertainty` 置位 ⟹ 行 2 一律不命中。
8. 由 5+6+7：`elapsed_proxy > 27000` ⟹ 静默跨度 > 27000 > 25000 ⟹ 置位 ⟹ 行 2 不命中。

**行 2 永不可命中。** 而 gate 10 ⑪ U8 仍要求 `elapsed_proxy = 27001` 必须命中行 2 → 判据自相矛盾，gate 10 自检必失败。

后果（席 B 构造）：探针因 deadline 计算错误在 D7 挂住、30 s 后产生 `APPFREEZE` → 落行 4(i) → `platform-termination` → pre-only 可 pass。**真实 probe-fault 被洗白。**

责任归属：**本条由主会话 r8 的 Y3 引入。** Y3 把阈值从 37.5 s 降到 25 s，本意是消除 27–37.5 s 的错位区间；降之前行 2 尚有 27000–37500 的活窗，降之后归零。

两席给出**方向相反**的修法，故升级为 T0 裁决问题（见第五节）：
- 席 A：要么删掉「置位则行 2 不命中」并改用能区分两假设的独立证据，要么显式废行 2 并同步删 ⑪ U8，接受 D7 超窗不再是运行时 fail。
- 席 B：只有存在**独立于该段 HiLog 尾部**的 post-D7 进度证据时才允许保护压过行 2，否则 D7 超窗必须保持 fail-closed。

席 A 的底层论断（主会话认为这是本轮最重要的洞察）：「D7 真超窗」与「P8 真死 + 尾 marker 全丢」在现有证据基础上**观测不可分**，任何阈值选择只是在决定把错误放在哪一侧。

### BL-2 `_T`/`_C` 只能夹住调用，不能证明调用瞬间
**席 B B-02。**

两个独立构造：
- (i) `_T` 已发、`destroy()` 已进入平台层，但进程在返回 Promise 前 terminal → `_C` 缺。正文与 gate 10 ⑨ 一律判 `destroy-never-called` → 行 4(ii) 也不立 → **一次成功的 destroy terminal 被判 fail**。
- (ii) destroy 实际调用于 1001 ms，worker 因它在 1003 ms 返回，主线程 1004 ms 读时钟发 `_C`。因 `1003 ≤ 1004`，判定表误判 `pre-destroy-ready`（残留/外来），真实的 destroy 归因被丢弃。

修法（席 B）：把 `_T` 到 `_C` 冻结为 `destroy-order-indeterminate` 区间并增加独立 call-entry 证据；不得把 `_C` 时间戳等同于实际调用时刻。

主会话注：r7 X4 新造 `_C`、r8 Y1 校准其发射点，两轮都在这个夹缝上做文章，但**两轮都没意识到夹缝本身不可消除**——marker 只能在调用前后发射，不可能在调用瞬间发射。这是设计层面的限制，不是落笔错误。

### BL-3 未知 revents 位与 `dw_return_class` 优先级表冲突
**席 B B-03。**

构造：`ret=1`、`revents=72`（未知位 64 + `POLLERR` 8）、`elapsed_ms=1000`、`at_mono_ms > destroy_call_mono_ms`、`drain=eagain`、`_C` 在。
未知位条款要求归 `other-revents`；判定表行 7 先命中 `fd-event-like`（唯一可归因 destroy 的类）→ **错误解锁路径①**。
现有 selftest 只测纯未知位 64，未覆盖 known+unknown 混合位。

修法：把「含任意未知位」设为进入分类表**之前**的最高优先级门，并补混合位用例。

### BL-4 poll 合法域门未约束 `elapsed_ms >= 0`
**席 B B-04。**

构造：`elapsed_ms = -1`、`ret=1`、`revents=POLLERR`、`at_mono_ms > destroy_call_mono_ms`、`drain=eagain` → 行 7 `fd-event-like` → 路径①解锁。
单调钟「结束 − 开始」不可能为负，只能来自探针计算缺陷、溢出或解析缺陷，却被洗成有效发现事实。

修法：分类前冻结所有单调时钟字段的非负性与必要顺序约束，违反即 fail，并加 `-1` 边界用例。

### BL-5 合法 skip 路径的 POST 字段域未闭合
**席 B B-05（席 A B-02 同族）。**

构造：五个 create 全部被正常 rejected（**合法的平台负面结果**，正是本 campaign 要发现的事实之一）→ 进 `no-live-fd` → 发 PRE、SKIP、POST。此时 D6S1–S7 与 D-W 均未执行。
- 省略 POST 子项 → 命中「字段缺项 → fail」；
- 填 `unobservable(no-live-fd)` → 落在 `dw_return_class`/`dw_join_result` 的冻结枚举**之外** → 触发 criteria-gap → fail。

**两条路都 fail。一个合法的发现结果无法被记录为 pass。**

修法：为 POST 每个 D6/D-W 子项冻结显式的 `result | skipped(cause) | unobservable(cause)` 三选一编码，同步扩展字段域与 selftest。

### BL-6 行 4(ii) 洗白 `_C` 之后的探针崩溃
**席 B B-06。**

构造：PRE 在、`_C` 在、destroy 已 resolve、`D6S1_B` 在；随后探针在执行 D6S1 时崩溃；本次 faultlogger **未产生条目**；positive baseline 后 `PidOfVpn` absent；POST 缺。
→ 行 1/2/3 不命中，行 4(ii)+(iv) 命中 → `platform-termination` → pre-only 可 pass。**真实 probe-fault 被洗白。**

修法：不得把「`_C` 后消失」单独作为平台终止的充分条件；须有独立平台终止证据，否则归 fail-closed 类。

### BL-7 dup-failed 路径调用 destroy 却不发 `_C`
**席 A B-02。主会话已核实：skip 表 `:634` 规定该路径 D-W 全 skip，而 `_C` 的发射只在 D-W 主线序列 `:505` 规定。**

构造：dup-failed、PRE 已发、D7 已跑、skip D-W、destroy 已调用、进程随 destroy terminal、无 faultlogger 条目、POST 缺、`PidOfVpn` absent。
→ 行 1–3 否、行 4(i)(iii) 否、行 4(ii) 因无 `_C` 否、行 5 否 → 行 6 `unattributed` → **fail**。
**一次成功执行白烧一个不可复用 ID**——正是行 4(ii) 存在的目的所要挡住的结构。

修法：凡实际调用 destroy 的路径（含 dup-failed）都必须发 `_C`；或行 4(ii)/V2 另认 P9 执行证据。

### BL-8 路径①条件 3 同一句内自相矛盾
**席 C B-01 + 席 A M-01 收敛（席 C 判 B，席 A 判 M）。**

`:576` 同一句：前半句「两侧均有 marker 锚点：`N1BDISC_DW_RETURN` 与 `N1BDISC_DW_DESTROY_T`」是 Y5 之前的旧叙述，后半句 Y5 定义式却规定 `destroy_call_mono_ms` 取 `_C`。
实现者读前半句取 `_T` → `_T` 早于调用语句 → 把 `[_T, _C]` 窗内的 poll 返回误判为「晚于 destroy」→ 假阳性支撑 `fd-event-like` 与路径①。

席 A 数值构造：`_T=1000`、`_C=1003`、poll `at=1002`。取 `_T`：`1002>1000` 条件 3 真；取 `_C`：`1002>1003` 假。

修法：把该处「两侧锚点」改为 `DW_RETURN` + `DW_DESTROY_C`，删除该处对 `_T` 的锚点指称。

责任归属：**主会话 r8 Y5 引入**——补定义式时没改它推翻的那句旧叙述。

### BL-9 阈值标签张冠李戴（四处）
**席 C B-02。主会话已核实四处（`:90` `:483` `:807` `:1138`）。**

四处把 `20000 + 5000` 标注为「行 2 的 elapsed_proxy 阈值」。行 2 真实阈值 = `20000 + 5000 + 2000（代理容差）= 27000`（`:869`）。`20000 + 5000` 是 **D7 合法时长**。
`:807` 甚至在同一句内既写「= 行 2 的 elapsed_proxy 阈值 20000 + 5000」又写「必须 < 行 2 判定窗 27000 ms」。
数值 25000 本身正确，坏的是标签：读者按标签重算得 27000，`25000 < 27000` 的严格序当场崩掉。

责任归属：**主会话 r8 Y3 引入。**

## 三、Major（5 条）

| 编号 | 席 | 内容 |
|---|---|---|
| MA-1 | B M-03 | **A5 只核对 marker 字面集，没有源码顺序静态断言。** Y1 修的是 `_T → destroy() → _C → await` 的执行序，但 A1–A8 与 gate 10 ⑨ 都不检查源码顺序——实现仍把 `_C` 放在 destroy 之前，marker 字面集、PRE/POST 真值表、现有模拟 trace 全部可通过。**r7 那条 blocker 的修复没有任何机器保证。** |
| ~~MA-2~~ | B M-01 | **经核实不成立，已撤销，r9 不予执行。** 详见第六之七节。 |
| MA-3 | B M-02 | 正文强制的 ledger 状态机 selftest（双实例、close 先于 create、重复 close、域外 `by`、`process-exit`/`host-forcestop`）未逐项同步进声称「集中列举全部用例」的 gate 10 ⑨。 |
| MA-4 | A M-02 | 「保护窗自 D7 合法时长结束时即开启」是闭区间叙事（含 25000），谓词却是严格 `>`（Y4(b) 冻结 25000 不置位）。开闭不同步。 |
| MA-5 | C M-01 | 自陈 11(b) 写「20 s × 1.5 = 30 s」，r8 各处防误引注称旧值为「25 s × 1.5 = 37.5 s」。同一「×1.5 旧阈值」两个数并存（r6 挂 D7 的 20 s 盒 → r7 挂 P6 的 25 s 盒的历史遗留）。 |

## 四、Minor（2 条）

| 编号 | 席 | 内容 |
|---|---|---|
| MI-1 | A m-01 | 置位条件通式用无下标 `T_uncertainty`，却只冻结了 `T_uncertainty(P6) = 25000`。非 P6 位点该谓词无阈值、不可求值——不是全函数。修法：写明仅位点判 P6 时求值，其余位点记不适用。 |
| MI-2 | A m-02 | 自陈 12(c) 把「行 2 让位」写成「方向偏保守（多 fail）」，**极性写反**：让位是把 probe-fault-fail 降为行 4/5/6，fail 面变窄不是变宽。 |

## 五、升级为 T0 裁决的架构问题（已派出，待回）

BL-1、BL-6、BL-7 共享一个结构：**死因分类表试图从一组观测上不可分的证据里「归因」，并把某些归因映射到 `fail`。**

而本 campaign 的治理前提（`native-nx-n1b-adjudication.md` §4.2）是：`verdict` 只评价基础设施与完整性、不评价平台行为；平台行为「不如预期」永远不是 fail；事实按三态记录。

已向三席派出裁决问题（非审查，允许改变各自原立场）：

> 死因分类表是否应停止为 `verdict` 目的做归因，改成 (i) 死亡事实一律三态记录、不再映射 fail；(ii) `verdict` 只在窄的正向探针崩溃签名上 fail（`CPPCRASH`/`JSRAWERROR`/致命信号/短时步集内 `APPFREEZE`），其余一律记录不 fail？

配套需回答：能否给出区分「D7 真超窗」与「P8 尾丢失」的具体独立证据（且不得依赖本 tuple 上从未实测的平台产物格式）；改后 fail-open 残余是什么、能否由 DryRun 门兜住；行 6 `unattributed → fail` 是否一并废除；是否需要新 T0 决议。

## 六、主会话对本轮的自我评估

**r8 是一次「5 项小修」，引入了 3 条 blocker（BL-1、BL-8、BL-9）。** 其中 BL-8、BL-9 是纯机械疏忽（补规则时不核对被推翻的旧叙述、不核对所引数量的真实取值），BL-1 是为消除一个装饰性错位而改阈值造成的结构性回归。

同型落笔错误累计 **18 次**（r8 贡献 4 次：Y4 首稿算术方向、BL-8、BL-9，及自陈 12(c) 极性）。模式始终一致：批量补写规则时不逐一核对 (a) 所引数量在所有路径上是否存在及其真实取值、(b) 叙述改动是否传播到表格与 selftest、(c) 新规则与既有条款如何相互作用。

**关于「收敛」的判断更正**：r7 呈现 2 B / 2 B，我据此向用户报告「收敛是真实的」。r8 出现 6 B / 2 B / 2 B。原因有二，须分开记：
1. 我引入了 3 条；
2. 本轮首次在派发中加入「起草方可信度校准」段，并要求按最坏解读判定，席 B 因此探入从未探过的区域（revents 混合位、负 elapsed、skip 路径 POST 字段域），BL-2 至 BL-6 **全部是前八轮漏掉的既存缺陷，不是 r8 的回归**。

**因此前几轮的 blocker 数下降主要反映审查投入的波动，不反映文档质量的提升。**「迭代到 0 blocker」这个判据本身不可靠——它测的是审查者当轮挖了多深。这一点须在向用户交付时如实说明，不得用「r7 只有 2 条」这类数字暗示接近完成。

## 六之二、机械交叉引用审计（执行层 glm-5.3-flash，非评审席）

与三席并行派出的纯提取比对任务，不占评审独立性（该家族已是本文档起草执行方）。六项审计全部完成，逐条附行号与逐字原文。**新增 2 条 major、2 条 minor，均为三席未覆盖的机械缺陷。**

### 新增 MA-6 A5 静态断言在字面上不成立
A5（`:713`）逐字要求「marker 字面集与本文冻结集逐字一致；**冻结集 = 本文全部 `N1BDISC_*` 字面**」。实际提取：全文 57 个不同字面，冻结集枚举（`:724-726`）55 个，差 2：
- `N1BDISC_D2_REJTEXT`（`:352` 仍在「每条目落盘」规则句中使用），而 `:727` 已写明「r4 第二趟 S4 更正：本集删除 `D2_REJTEXT`」。`:352` 自带「无独立 REJTEXT 专用字面」的澄清，但残留标签未加防误引注。
- `N1BDISC_RESULT`（`:49` `:281` `:1090` `:1101` 四处，全为修订登记/防误引注语境，非活规则引用）。

后果：A5 若按字面实现为「扫描全文 `N1BDISC_*` 字面 == 冻结集」，则**静态断言必失败**，gate 过不去。属可实现性缺陷。

### 新增 MA-7 行 2 阈值分解式的量名在两处错位
- `:839`（死因表行 2 本体）逐字：`> 20000 ms + 5000 ms 冻结时长 + 2000 ms 代理容差`——「冻结时长」紧贴 **5000 ms**。
- `:869`（墙钟代理节）逐字：`elapsed_proxy > 20000 ms 冻结时长 + 5000 ms 宽限 + 2000 ms 代理容差 = 27000`——「冻结时长」属 **20000 ms**、5000 ms 是「宽限」。

总和同为 27000，判定值无冲突，但同一分解式量名错位，`:839` 处易读成「冻结时长 = 5000 ms」。与 BL-9 同族（阈值标签张冠李戴），须一并修。

### 新增 MI-3 `unobservable` 的两种拼法全文混用
`cause=` 包装形式 47 处 / 裸形式 30 处并存，**6 组同一 cause 两种拼法**，最典型是同一 D4 节内：`:379` 写 `unobservable(cause=short-or-zero-io)`、`:384` 写 `unobservable(short-or-zero-io)`。另 `no-live-vpn` / `create-indeterminate` / `matrix-terminated-on-create-timeout` / `protocol-first-accept-lock` 四个只有裸形式、从无 `cause=` 形式。而 criteria-gap 节（`:881`）定义合法域时用的句式是「逐字写明的 `unobservable(cause=…)` 清单」——**两种包装并存且无统一规则说明，合法域判定可能因此收错**。

### 新增 MI-4 selftest 圈码 ①③⑦⑧ 从未被正文单独引用
①–⑪ 定义连续无跳号、无坏引用（这是好消息）；但 ①③⑦⑧ 只出现在定义块与范围引用中，正文从未点名。另记：①② 字形同时被用作 selftest 编号与「路径①/路径②」，现有引用均带前缀消歧，但检索易混。

### 审计的阴性结论（同样重要）
- **跨文件行号引用 94 处逐条核验：88 相符、0 不符、1 条目标文件（`boringtun-0.7.1/src/ffi/mod.rs`）不在仓库无法核验，其余为范围端点空行。** 我此前担心的行号漂移问题在**跨文件**引用上并不存在；漂移只发生在**文件内**自引用（`:70` 已自带说明）。BL-9 那类错误是标签写错，不是行号漂移。
- `dw_return_class` 12 类：全部提取、全部被表外引用、计数自洽，**无问题**。
- 55 个冻结 marker 无孤儿（每个都在协议位点出现）。
- 无「同一个量在两处取不同活值」的冲突；全部新旧值更替均带废弃/防误引注标记。
- 同值不同量属设计常态并已备查：`25000`（D7 合法上界 / `T_uncertainty(P6)`）、`5 s`（T_dw poll 盒 / drain 盒 / D7 宽限）、`10 s`（≥7 个独立时间盒）、`60 s`（3 个）、`500 ms`（3 个相位）、`2 s`（2 个）。

## 六之三、T0 裁决票（陆续回收）

裁决问题见第五节。三席可自由改变各自在审查阶段的立场。

### 席 C（deepseek-v4-pro）—— 已回票

| 问题 | 裁决 |
|---|---|
| Q1 停止为 verdict 目的归因？ | **支持** |
| Q2 失去哪些捕获？ | 三类：未知词根崩溃、无痕崩溃（faultlogger 无条目）、长步 hang |
| Q3 废行 6 `unattributed → fail`？ | **是，且行 7（显式 else → fail）应一并废** |
| Q4 需要新 T0 决议？ | **不需要** |

关键论据（主会话认为最有力的一条）：**决议 §4.2 已经把答案写死了。** `:115` 规定 `verdict` 只对基础设施与完整性求值，`fail` 的四个触发面是「探针未完成预注册采集 / 破坏冻结顺序 / 未增量落盘 / 封签失败」；`:116` 规定平台行为「不如预期」永远不是 fail；`:117` 规定全部平台事实三态登记。**死因分类表把死亡原因归因并映射到 fail，是决议没有要求、且与 `:116` 直接冲突的构造。**

Q4 的核验方式（可复核）：席 C 报告在 `native-nx-n1b-adjudication.md` 全文 184 行中对「死因 / 归因 / probe-fault / platform-termination / unattributed」**零命中**，§4.3 的九条强制条件无一要求建立死因分类表。故本改动是「判据向既有决议收敛」，不是新的治理决策，走既有流程（跨厂商隔离独立审查 → 0 blocker → 用户显式授权）即可。

席 C 对现表的诊断：**双向都错**——`unattributed → fail` 把平台行为烧成 fail（fail 面过宽），行 4(ii) → 不 fail 把探针崩溃洗成 pass（fail 面过窄）。改为「只在正向签名上 fail」后两个方向同时归位。

席 C 提出的唯一边界条件：改动显著收窄 fail 面，按判据自身的自陈纪律须在修订登记与自陈节**逐条登记**删掉了哪些 fail 映射、各自失去的探针缺陷捕获是什么、以及为何这些损失是 `:116` 所要求的。**这是登记义务，不是新 T0 义务。**

### 席 A（grok-4.6）—— 已回票（**改变了审查阶段的立场**）

审查阶段立场是「二选一」（删保护条款改用独立证据 / 显式废行 2）。裁决阶段明确收敛为后者，并强化为「**归因表修不好，不要再修**」。

| 问题 | 裁决 |
|---|---|
| Q1 停止为 verdict 目的归因？ | **支持采用 (i)+(ii)** |
| Q2 失去哪些捕获？ | 四类（见下），且**明确否定 DryRun 可兜底** |
| Q3 废行 6？ | **是，且行 5 从 fail 触发器拿掉、行 7 在 PRE 已在时不得 fail** |
| Q4 需要新 T0 决议？ | **不需要** |

核心论断：BL-1 / BL-7 / BL-6 不是三处落笔错误，是**同一条结构墙**——capture 里的「最后可见 marker + 进程消失 ± APPFREEZE」**对互斥死因不是单射**。凡把其中某些像映射到 `verdict=fail` 的规则，要么把探针缺陷洗成 pass（BL-6、Y3 让位后的行 2），要么把预期成功终态烧成 fail（BL-7、BL-2 的 `_T` 在 `_C` 缺）。**继续在表内打补丁，只会再制造下一轮不可分对。**

**席 A 对候选区分器做了穷尽否定**（这是本轮最有价值的论证，直接回应席 B 审查阶段主张的「独立 post-D7 进度证据」）：

| 候选 | 为何不够 |
|---|---|
| `D7_END` 是否在 capture | Y4 构造就是它丢了；真超窗也从不发它。同观测。 |
| D7 心跳 marker | 从 `D7_BEGIN` 起的连续丢尾会把心跳一并吃掉；能抓到心跳的情形，`D7_END` 本来就能把位点移出 P6，行 2 本就不适用。 |
| 窗内周期性 `PidOfVpn` | 25 s 仍活、30 s 已死：D7 挂到 30 s 被杀，与 D7 跑完再在 P8 死，**时间线相同**。且超出白名单两个使用位点。 |
| `Fault_Type` / `Signal` 形态差 | S1 已登记本 tuple **从未**见过真实 faultlogger 字面。禁止当区分器。 |
| 第二日志通道 / 跨进程 `/proc` | 不在 syscall 面与 HDC 白名单；等于新实验。 |

**要在判据里写死的解释句（席 A 给出，主会话采纳为 r9 落笔内容）**：

> 「未完成预注册采集」限 PRE 缺失、顺序破坏、增量落盘/字段域缺口、封签失败、以及窄崩溃签名；**PRE 封口之后的进程消失，不论能否归因，不构成 fail。**

**席 A 明确列出「不修」清单**：不要再给行 2 找区分器；不要收紧行 4(ii) 来堵 BL-6。

**席 A 对 BL-7 的重新定级**：降为事实质量问题（凡真正调用 destroy 的路径仍应发 `_C`，否则 OB-04 输入是假的「从未调用」），但缺 `_C` **不得再经行 6 烧 ID**——即 verdict 面的 blocker 由本裁定消解，事实面的义务保留。

## 六之四、主会话在本轮裁决中的程序失误（登记）

**失误 #19（形状与前 18 次不同，是程序失误而非文档失误）**：我在向三席同时发出的裁决问题里写了「这些残余是否可由 DryRun 门（Live 之前有一次完整 DryRun）兜住」，**把 DryRun 当作残余 fail-open 风险的兜底，而未先核实 DryRun 门的能力边界**。

席 A 抓住并否掉：`n1b-disc-gate-plan.md:947` 门 11 明写 `is_evidence=false`、**HDC0**（host-only 假 HDC），`g0-go-arm64-physical-probe.md:186` 同文。**DryRun 看不见真机 watchdog、真 faultlogger 字面、真 hilog 丢尾、真 destroy terminal**，因此兜不住第 1–3 类损失。席 A 原话：「**禁止用『还有一次 DryRun』给死因 fail-open 背书。**」

席 C 未质疑该前提即绕过（它用 §4.1 的结局不对称设计作答），席 B 的票尚未回收——**该假前提可能已污染席 C 的票，并可能污染席 B 的票**。

处置：
1. 本条已登记，r9 落笔不得引用「DryRun 兜底」作为接受残余的理由。
2. 残余的**正确**接受理由（席 A 给出，主会话核验成立）：DISC `pass` 不得被引用为平台行为结论（§4.2 硬约束，`evidence-schema.md:90` 同文）；N1b 正式门仍须在本元组复测；发现 campaign 的损失函数是「烧掉不可复用 ID」，不是「漏掉一次可能的探针挂死」。
3. 席 B 回票后须检查其是否依赖了 DryRun 前提；若依赖，须就该点单独重问。

**教训**：向多席同时发出的裁决问题，其中每一条事实性前提都必须先自行核实。同时派发放大了单个假前提的影响——三票同源污染比一次落笔错误更难被发现，因为它表现为「三席一致」。

### 席 B（gpt-5.6-sol）—— 已回票（**也改变了审查阶段的立场**）

审查阶段主张「保住行 2 fail-closed，只在有独立 post-D7 进度证据时才让保护压过它」。裁决阶段自行推翻：现有证据面无法修好归因表——**D7 真超窗**与**D7 已完成但 P7/P8 HiLog 尾部全丢**可产生**完全相同的观测向量**（PRE、`D7_BEGIN`、无 `D7_END` 及后续 marker、同一 APPFREEZE / 死亡时刻）；不存在独立于 HiLog 的已验证进度通道，新增文件/IPC/额外 HDC/平台产物都会引入本 tuple 未实测依赖。

| 问题 | 裁决 |
|---|---|
| Q1 停止为 verdict 目的归因？ | **支持采用 (i)+(ii)** |
| Q2 残余？ | 明确承认并命名，且**独立否定 DryRun 兜底** |
| Q3 废行 6？ | **是；行 5、行 7 也不得因「无法排除 probe fault」而 fail** |
| Q4 需要新 T0 决议？ | **不需要** |

**席 B 独有的建设性贡献（r9 的设计输入）：**

**(1) 用证据向量取代互斥因果类。** 各分量各自三态：`process_death`、`last_visible_site`、`fault_type`、`signal`、`destroy_call_state`、`tail_uncertainty`。这是对「归因表拆掉之后拿什么替代」的正面回答——不再强行合成一个「原因」，而是把各维证据平行落盘。

**(2) `_T`/`_C` 的完整建模（解决 BL-2 与 BL-8，不引入任何新平台依赖）：**

*destroy 调用状态（`destroy_call_state`）*

| 观测 | 判定 |
|---|---|
| 协议显式发出 `SKIP destroy` | `not-called`（**唯一可证「未调用」的途径**） |
| `_T` 缺、`_C` 缺 | 按此前协议位点决定 `not-reached`；**不得仅凭缺失推断未调用** |
| `_T` 在、`_C` 缺 | `unobservable(cause=call-boundary-incomplete)`——既可能尚未调用，也可能已进入调用但未返回。**不得写 `never-called`** |
| `_T` 在、`_C` 在 | 只能证明**调用表达式已返回完成凭据**，不能证明平台效果已发生 |
| `_C` 在、`_T` 缺 | marker 顺序/完整性错误 → **fail**（这是完整性轴，不是死因轴） |

*worker 返回时序（**因字段只记毫秒，等值不能证明先后**，故两侧均为严格不等号、闭区间整体归歧义）*

| 观测 | 判定 |
|---|---|
| `at_mono_ms` **<** `T_mono_ms` | `definitely-pre-invocation` |
| `at_mono_ms` **>** `C_mono_ms` | `definitely-post-invocation` |
| `T_mono_ms` **≤** `at_mono_ms` **≤** `C_mono_ms` | `unobservable(cause=invocation-window-ambiguous)` |
| `_C` 缺 | **不得作 post-invocation 归因** |

路径① **只接受严格的 `at_mono_ms > C_mono_ms`**。BL-8 那句「前半句锚点为 `_T`、后半句取 `_C`」的冲突必须**删除**，不能让实现者自行选边。

*命名*：席 B 建议把 `unattributed` 改名为不含因果暗示的 `unobservable(cause=death-cause-indeterminate)`。

*窄 fail 集（席 B 版，比席 A 版多两项）*：`CPPCRASH`/`JSRAWERROR`；`SIGSEGV`/`SIGABRT`/`SIGBUS`/`SIGFPE`；冻结短时步集内**且 marker 序列无矛盾**的 `APPFREEZE`；**进程被正向确认仍存活但总协议窗到期仍未完成**；既有字段/顺序/解析/封签/哈希/白名单完整性失败。

*证据向量分量（席 B 完整命名）*：`process_death_observed`、`last_visible_site`、`fault_type_observed`、`signal_observed`、`destroy_call_state`、`marker_tail_state`、`probe_crash_signature_observed`。

**(3) 对 BL-7 的加重提醒**：dup-failed 必须使用**独立于 D-W 的共享 destroy 子协议**发 `_T`/call/`_C`，**不能因取消归因表而掩盖该 marker 缺口**（与席 A「事实面义务保留」一致）。

**(4) 对残余的诚实命名**（比我的提问更准确）：新方案接受一种残余 fail-open——没有正向签名的真实 probe fault 可能以 `death_cause=unobservable` 收口并使 pre-only pass。以 BL-6 构造为例，新方案下记 `process_death=observed-true`、`probe_crash_signature=unobservable`、`death_cause=unobservable`，**不再伪称 `platform-termination`**，但 verdict 仍可能 pass——「语义上不再错误归因，操作上仍是未检出的 probe fault」。席 B 同时指出静态审查与故障注入 selftest **只能降风险、不能证明消除**。

## 六之五、裁决结论：三席一致（3/3）

| | 席 A grok-4.6 | 席 B gpt-5.6-sol | 席 C deepseek-v4-pro |
|---|---|---|---|
| Q1 停止归因驱动 verdict | 支持 | 支持 | 支持 |
| Q3 废行 6（及行 5、行 7 的 fail 映射） | 支持 | 支持 | 支持 |
| Q4 需要新 T0 决议 | 不需要 | 不需要 | 不需要 |
| 审查阶段→裁决阶段是否改变立场 | **是** | **是** | 否 |
| 是否独立否定我的 DryRun 假前提 | **是** | **是** | 否（未质疑即绕过） |

三席给出的同一根因表述（措辞不同、结构一致）：
- 席 A：「capture 里的『最后可见 marker + 进程消失 ± APPFREEZE』**对互斥死因不是单射**。」
- 席 B：「D7 真超窗与 D7 已完成但尾部全丢**可产生完全相同的观测向量**。」
- 席 C：「三席 blocker 全部来自**归因**这一步，不是来自**记录**这一步。」

三席一致认定死因→verdict 映射是**判据超出决议自造的一层**（席 A 用词：「判据在违宪」「死因→verdict 是判据私货」；席 C 核验：决议全文 184 行对「死因/归因/probe-fault/platform-termination/unattributed」**零命中**）。故 r9 的性质是**判据向既有决议 §4.2 收敛**，不是新的治理决策。

**两票独立改变立场**这一点值得单独记：席 A 与席 B 在审查阶段给出的是**方向相反**的修法，在拿到完整证据后各自向同一结论收敛，且都不是被我说服的（我的提问是中立列举，且含一个被它们否掉的假前提）。这比「三席从一开始就同意」更有说服力。

## 六之六、r9 的性质与范围（据裁决重估）

r9 **不是一次修订，是一次结构性改造**。范围：

**A. 归因停机（三席裁决落地）**
1. 行 2、行 5、行 6、行 7 的 fail 映射删除；死因表降为**观察分类**，从 finally 步骤 4/9 与 D-W 终态路径的 fail 触发器上拆掉。
2. 按席 B 方案改为证据向量（各分量三态）。
3. 冻结席 A 给出的解释句：「未完成预注册采集」限 PRE 缺失、顺序破坏、增量落盘/字段域缺口、封签失败、以及窄崩溃签名；**PRE 封口之后的进程消失，不论能否归因，不构成 fail**。
4. 按席 C 的登记义务：逐条登记删掉了哪些 fail 映射、各自失去的探针缺陷捕获、以及为何该损失是决议 `:116` 所要求的。**不得引用「DryRun 兜底」作为理由**（失误 #19）。

**B. 与裁决正交、仍须修的缺陷**
5. BL-2 + BL-8：按席 B 五态模型重写 `_T`/`_C` 判定；路径①只接受 `at > C`。
6. BL-3：「含任意未知位」提升为进入分类表**之前**的最高优先级门 + 混合位 selftest。
7. BL-4：分类前冻结全部单调钟字段非负性与顺序约束 + `-1` 边界用例。
8. BL-5：为 POST 每个 D6/D-W 子项冻结 `result | skipped(cause) | unobservable(cause)` 三选一编码。
9. BL-7 事实面：凡真正调用 destroy 的路径（含 dup-failed）走共享 destroy 子协议发 `_T`/call/`_C`。
10. BL-9 + MA-7：阈值标签与量名错位四处 + 两处。
11. MA-1：新增源码顺序静态断言（现有 A1–A8 与 ⑨ 均不检查执行序）。
12. MA-2：P8 预算 9 s → 15 s，总上界 467 → 473。
13. MA-3：ledger 状态机 selftest 逐项同步进 ⑨。
14. MA-4、MA-5、MA-6、MI-1、MI-2、MI-3、MI-4。

**C. 因归因停机而自动消解、不需单独修的**
- BL-1（行 2 死代码）：行 2 的 fail 映射删除后不再存在；⑪ U8「27001 命中行 2」同步删除。
- BL-6（行 4(ii) 洗白）：不再收紧行 4(ii)（席 A 明确「不修」），改由证据向量如实记录。
- `T_uncertainty` 与行 2 的阈值耦合（MA-4、MI-1 的一部分）：行 2 不再驱动 verdict 后，该耦合消失。

**执行策略（据历史失败数据）**：r9 规模远超 r6（r6 是 14 项 / 1041 行，glm-5.3-flash 连续三次空输出，主会话接手）。**不得作为单次派发。** 须切成多个有界工作包，每包 6–11 项、每包独立验证后再进下一包。切分依据是**判断密度**而非模型能力（见第一节席 C 的实证）。

## 六之七、审查席发现的核实结果（**席 B M-01 无效**）

r9 开工时对「先核实再动手」的纪律做了一次实际检验，结果推翻了一条 T0 席的定量发现。

### 席 B M-01（P8 时间预算）：**核实不成立，撤销**

席 B 主张：P8 的 `barrier-never-observed` 路径实际上界是 `7 s + 8 s = 15 s`，而时间盒表写 `9 s`，故总上界应由 `467` 改 `473`、裕量 `58` 改 `52`。

**核实结果（主会话已独立复核原文）**：那 8 s **不属于 P8**。`n1b-disc-gate-plan.md` 逐字：

> barrier 等待盒（7 s）到期仍未见 marker → destroy **顺延**：主线程继续以同一 10 ms 有界轮询等待 marker，**直至 worker 终态轮询盒到期（自 barrier 盒到期起算 ≤8 s）**；届时仍无 marker → 视为 worker 异常……destroy 不再执行

该 8 s 明写是 **P10 的「worker 终态轮询 8 s（10 ms 间隔）」盒**，推导式中 P10(8) 已单列。且该路径上 **destroy 不执行**，P9 的 10 s 盒完全不被消费。

算术核对（两种读法均不越预算）：
- 按原文归属：该路径 P8+P9+P10 = 7 + 0 + 8 = **15 s** ≤ 预算 9 + 10 + 8 = **27 s**；
- 即使按席 B 读法把 8 s 记入 P8：15 + 0 + 8 = **23 s** ≤ 27 s。

另有 P8 并发说明佐证（`r3` 冻结）：drain 由 worker 线程执行、barrier 等待由主线程执行，**二者并行**，故 P8 串行上界取二者最大值再加 in-wait 采集，**不是相加**——「drain 5 + barrier 7 + in-wait 2 = 14」正是 r3 已经更正过的旧算式。

**结论**：席 B 把 P10 的时间盒误记到 P8 名下。`467 / 58 / 525` 全部成立，**一字未动**。

### 由此得出的处理纪律更正（重要）

此前我把「三席发现」当作已验证输入直接排进 r9 施工清单。**这是错的**——审查席的定量主张同样需要核实。若这条未经核实即执行，我会把一个正确的推导式改错，并把冻结值的裕量凭空削掉 6 s。

**已核实成立**：BL-1（主会话独立复核推导链）、BL-7（skip 表与 `_C` 发射点原文）、BL-8、BL-9、MA-1、MA-3、MA-6、MA-7。
**已核实不成立**：MA-2（本节）。
**逻辑性主张、经检视成立**（无数值可验，靠推理判定）：BL-2（marker 只能夹住调用）、BL-6（表删除后自动消解）。
**已核实成立（专项核实，见下）**：BL-3、BL-4、BL-5。

### BL-3 / BL-4 / BL-5 专项核实结果：**三条全部成立**

**BL-3（未知 revents 位）成立，且比席 B 的表述更精确。** 冻结掩码全集 `{0x001, 0x002, 0x004, 0x008, 0x010, 0x020}`，按位或 = 63；`64` 确在全集外、`8` 确为 `POLLERR`。关键证据：
- 合法域门明文带「**先于下方判定表执行**」；「未知位处置（冻结）」条款**没有任何次序保护措辞**。
- 判定表程序是「自上而下，首个命中即定类并终止匹配」；行 7 条件「含 `POLLHUP` 或 `POLLERR`」按 S2 冻结读法 = 按位与非零，`72 & 0x008 ≠ 0` 为真 → 构造命中行 7 `fd-event-like`。
- 而未知位条款自称「归判定表优先级 11 `other-revents` 类」——但**行 11 自身的条件是「不匹配以上任一行」，对该输入为假**。故「归 11」在表内程序下**不可达**。
- **文档对同一输入存在两处互相矛盾的归类。**
- selftest ④ 的域明文界定为 64 子集（0..63）+ 仅纯未知位 64 用例，**确无混合位覆盖**。

修法：把未知位处置**升格为前置门**（「先于判定表执行、直接定类 `other-revents`、不进入表内匹配」）+ 补 `72`/`65` 等混合位用例。

**BL-4（单调钟非负性）成立，且缺口比席 B 指出的更广。** 合法域门逐字仅四条件：`ret ∈ {-1, 0, 1}`；`ret == 0` 必须 revents 空；`ret == 1` 必须 revents 非空；`ret == -1` 必须携带 errno。**无任何 `elapsed_ms` 非负或取值范围约束**，矛盾输入示例清单亦不含；全文无 `elapsed_ms` 的符号检查或计算式定义。构造 `elapsed = -1` 通过门后命中行 7（`-1 < 4500` 为真），无门拦截。

**核实者扩大的范围（席 B 未提）**：`at_mono_ms`、`DW_DESTROY_T`/`_C` 的 `mono_ms`、drain 的 `elapsed_ms`——**同类缺口普遍存在**，全都没有绝对域检查。故 r9 的修法应是**对全部单调钟字段统一冻结非负性与必要顺序约束**，不是只补 `elapsed_ms` 一处。

**BL-5（skip 路径 POST 字段域）成立，且核实者已穷尽搜索过出口。**
- `dw_return_class` 域 = 判定表 12 值；`dw_join_result` 域 = 5 值（`joined`/`join-timeout`/`join-blocked-observed`/`ESRCH`/`other+errno`）。**两者域内均无 `unobservable`。**
- skip 表 D-W/P10 行却明文规定「`dw_*` 全 `unobservable(no-live-fd)`」/「`dw_* = unobservable(dup-failed)`」。
- 全 rejected 终局走 `no-live-fd` 分支，主线仍「→ P11 → P12」，**POST 照发**。
- POST 字段集逐字冻结且「缺任一冻结字段 → fail（沿 MJ-7）」；`d6_items` 要求「`D6S1..S7` 七子项逐字 ret/errno/reuse 结果全列」、**无 skip 编码**（`D6S5` 占位明写「不得空置」）。

**对比发现（核实者补充）**：PRE 有 `skip_summary` 字段，**POST 无对应物**——这是结构性不对称，正是缺口来源。

**穷尽搜索出口的结论**：唯一疑似出口是 `dw_watchdog_killed` 条目内的 S7 注「skip 主线由此有唯一字段值，RESULT 不因域外值 fail」，但修订登记明文「S7 `dw_watchdog_killed` 删除 skip 判 `observed-false` 分支」——**仅限该一字段**（其自身域本就含 skip 具名清单），不构成 `dw_return_class`/`dw_join_result`/`d6_items` 的出口。域外值出口「`value-outside-frozen-domain` 不 fail」仅适用**探针采集到的有效平台新值**，不适用**协议指派的 skip 值**。

**结论：两条路确实都 fail，一个合法的平台负面结果（全部 create 被拒）无法被记为 pass。**

修法：把 `unobservable(cause=no-live-fd/dup-failed)` 逐字纳入两字段域，并为 `d6_items` 补 skip 编码（或仿 PRE 增设 POST 的 skip 收口字段）+ gate 10 ⑨ 补用例。

### 核实纪律的净效果

本批四条定量/结构主张，**三条成立、一条（MA-2）不成立**。若不核实全盘照做，会引入一处错误修改；若因 MA-2 错了就怀疑全部，会漏掉三条真实缺陷。**逐条核实是唯一正确的处理方式**，成本远低于两种误判的任一种。

**纪律**：r9 每一条施工项在动手前必须处于「已核实成立」状态。来源是 T0 席不构成豁免。

## 六之八、WP-mech 执行结果（r9 第一个工作包，已完成）

四项独立机械修正，执行层 glm-5.3-flash，`docs/n1b-disc-gate-plan.md` +6/−3：

| 项 | 判定 | 处置 |
|---|---|---|
| MA-1 源码执行序静态断言 | 前提成立 | **新增 A9**：源码层核对 D-W 主线四步序（`_T` 发射 → `destroy()` 调用 → `_C` 发射 → resolve 等待）同一控制流依次出现、无分支跳过 `_C`；并核对 `destroy()` 全源码恰一个调用点。反例用例补入 gate 10 ①（经确认 ① 是承载静态断言用例的条目）。 |
| MA-2 P8 预算 | **不成立** | 未改（见上节） |
| MA-3 ledger selftest 同步 | 成立 | 正文「selftest 须含」的 6 类 ledger 状态机用例在圈码 ①–⑪ 中**零命中**，已逐字抄入承载 ledger 的 ⑨ |
| MA-6 A5 字面集可实现性 | 成立 | A5 比对口径自「本文全部 `N1BDISC_*` 字面」收窄为「活规则中使用的字面」+ 显式豁免集 `{D2_REJTEXT, RESULT}`；冻结集注新增 r9 条目（57 vs 55 计数与逐字面豁免理由）；`D2_REJTEXT` 唯一使用点补防误引注 |

**与 r9 后续工作包的衔接注**：A9 括注写的是「`destroy()` 有且仅有一个调用点（P9 位点那次，D6 与 D-W 共用）」。这与 r9 规范第四节 4.3 的「共享 destroy 子协议」**互相加强**（一个调用点、多路径调用）；但 WP4 落地后该括注须扩写以覆盖 `no-live-fd` / `dup-failed` 等 skip 路径也经该调用点。**WP4 施工时必须回头改这一处**，否则 A9 会与新的 skip 路径规定不同步。

## 七、关联文件

- 被审对象：`docs/n1b-disc-gate-plan.md`
- 授权决议：`docs/native-nx-n1b-adjudication.md`（`ADJ-T0-N1B-20260831-0001`）
- 未决义务台账：`docs/open-obligations-ledger.md`
- 证据/门 schema：`docs/evidence-schema.md`

## 九、第十轮审查（r9 版）——三席结果与去重登记

被审对象：`docs/n1b-disc-gate-plan.md` @ commit `5dd5261`（状态 `criteria-r9-pending-independent-review`）。

### 9.1 席位与投票

| 席 | 模型 | 范围 | 结论 | 计数 |
|---|---|---|---|---|
| A | grok-4.6 | 全文，四层次 | fail | 2 B / 7 M / 5 m |
| B | gpt-5.6-sol | 全文，四层次 + S2-S5 修复确认 | fail | 9 B / 4 M / 1 m |
| C | deepseek-v4-pro | 收窄至四个新规则块 | **pass** | 0 B / 1 M |

席 C 连续第二次在收窄范围后正常返回并给出高质量结果；其唯一 M（F1 索引漏限定）与席 A M-02、席 B B-01 **三席收敛到同一条**。

**九项主会话裁量：席 A 全部维持（每项附验证内容）；席 B 维持 8 项、对 ② 提出落地层挑战（正文/闭集索引非单值、complete 登记载体不存在），不挑战方向。**

### 9.2 两席全文席的互补盲区（重要观察）

席 A 抓到的 pre-only 闭域缺口（其 B-01/B-02）席 B 没看到；席 B 抓到的字段域/载体/聚合缺口（其 B-03/B-04/B-07/B-08/B-09）席 A 没看到。**两席发现几乎不重叠。**再次印证第九轮结论：blocker 数量反映审查深度分布，不反映文档质量趋势。

### 9.3 去重后 blocker 清单（全部经主会话逐条核实成立）

1. **F1 闭集索引漏 `protocol != complete`**（三席收敛；`:814` vs `:801` 权威定义）。
2. **pre-only 的 dw_\* 闭域缺口**（A B-01）：`unobservable(cause=post-destroy-unobservable)` 是 r5 时代旧 cause，仅 `:839` 一处，不在 r9 新闭的 15/7 值域 → E3 预期成功 destroy terminal 落 F8 烧 ID。R4 只修了 POST 在的 skip 编码，没修 POST 缺的。**complete 主线通、pre-only 断。**
3. **V2 (1b) 只认一个 cause**（A B-02）：`:844` 只认 `cause=barrier-never-observed`，skip 表 destroy 行 `:683` 发的是 `cause=no-live-connection` → 「五 create 全拒 + 死于 POST 前」四支无落点 → F8。
4. **revents marker 字段域与未知位门冲突**（B B-03）：`DW_RETURN` 字段域冻结 `0..63`（`:526`、`:601`），未知位门却要求 `revents=72` 这类合法平台观测可落盘不 fail——marker schema 禁止了门要求必须可记录的值。
5. **D8b 窗口钟无 capture 载体**（B B-04）：`:463` 落盘清单与 `:581` 钟域门都引用 `window_start/end_monotonic`，但 `D8_STORM_BEGIN` 裸、`D8_STORM_END` 只带 5 字段——**没有任何 marker 承载这两枚钟**，合法完成 D8b 必然字段缺项 fail。
6. **七分量非全函数**（B B-05 ⊇ A M-07）：`marker_tail_state` 对「POST 缺 ∧ 无死亡证据」（F9 路径）无值；`process_death_observed` 无 observed-false 完整分支；多处分量只写省略号 cause、无具名可校验字面。
7. **五态表第五行无分量值**（B B-06）：`_C` 在 `_T` 缺 → F3 fail 但 `destroy_call_state` 无值，违反「七分量任何路径各自有值」；SKIP 与 `_T/_C` 同现的矛盾输入无域门。
8. **fault 条目无聚合规则**（B B-07）：`:884` 契约逐条目解析，`fault_type_observed` 单值；同窗多条目（如 `APPFREEZE`+`CPPCRASH`）取哪个不定义 → verdict 非单值。
9. **D7 早退矛盾不 fail**（B B-08）：伪码只在 `now >= deadline` 时退出，`D7_END.elapsed_ms < 20000` 是机械不可能的正向探针控制流缺陷（文档自认「合法值不可能 < 20000」），现仅记观察不 fail → 明确 fail-open。
10. **`process_death_observed` 证据源缺陷**（B B-09 ⊇ A m-01）：`:905` 析取仍含「进程退出记录」而 `:853` 明文已删；faultlogger 条目证明冻结/崩溃事件、不单独证明进程终止。
11. **#6 构造的签名值自相矛盾**（B B-02）：selftest ⑩ 写 `observed-false`、自陈 14(c) 写 `unobservable`。须按「输入分量任一不可求值 → 签名不可求值；全部可求值 → 按三支定真假」的真值表冻结（与 A M-06 同一修法），并修自陈示例值。
12. **complete + 崩溃签名的登记载体不存在**（B M-04）：「在 POST 登记」——POST 冻结字段集无该字段、且崩溃常发生在 POST 之后。应登记入 runner evidence 记录。

### 9.4 Major / minor（去重）

- 门 3 仍写 A1-A8、A9 不进 freeze 清单（A M-04 = B M-01，两席收敛）。
- P0 operator-ready 单调时刻被钟域门排除（B M-02）。
- `T_tail` 标签「单个最长合法阶段的合法时长上界」错误——P2 的 create 盒是 60 s（B M-03）。
- D6 节 U4 旧句「未执行 → observed-false」与 V2 冲突（A M-01）。
- **F1 第 3 支结构性空集**（A M-03）：短时步集（P1 + D2 2.1-2.7）全在 P5T 之前——PRE 在则 `last_visible_site ≥ P5T` 永不在短时步集；PRE 缺则 F2 已 fail。第 3 支**不能独立改变任何 verdict**；selftest ⑩ Ⅲ「钉死第 3 支独立成立」做不到（它自承同时命中 F2）。**主会话裁定**：保留第 3 支作为**事实记录**（对 N1b 的诊断信息），但明文登记其 fail 效力恒与 F2 叠加、修 ⑩ Ⅲ 措辞——删除该支会丢失诊断事实而不改变任何 verdict。
- 三支闭集缺「不可求值」操作化谓词（A M-06，与 9.3 #11 同修）。
- 自陈 14(e) 称 `_C` 在 `_T` 缺规则不存在、五态第 5 行已落地之（A M-05）。
- m：旧自陈裸拼法 2 处（A m-04 = B m-01）；「F1-F9」未排除 F7 的活规则措辞（A m-02）；F4/F8 论域重叠（A m-03）；「fail 只来自 PRE 缺失」过窄（A m-05）。

### 9.5 席 B 对 S2-S5 的修复确认（r8 四条 blocker）

- S5（skip POST 编码）：**已修**，三选一/15/7/全拒→pass 用例正文表格 selftest 同步。
- S2（五态+三分带）：修得不完整——第五行无分量值、SKIP 同现未定义（见 9.3 #7）。
- S3（未知位门）：修得不完整——marker 字段域未同步（见 9.3 #4）。
- S4（单调钟域门）：修得不完整——D8b 载体缺失、P0 排除、自测覆盖不足（见 9.3 #5、B M-02）。

### 9.6 席 A 的 16 点全函数性抽查确认（r9 主修法已落地）

归因停机与窄 F1、未知位门、负钟 F8、三分带与判定表相容、S8 收紧、barrier SKIP、complete 路径 skip 编码、观测窗算术（467/525 独立复核成立，P10 的 8 s 不在 P8——**第三次独立确认第九轮 MA-2 无效**）——这些在活规则上落地。挡住冻结的是 pre-only 收口与新闭域/V2 的不同步。

## 九之二、第十一轮审查（r10 版）——结果与去重登记

被审对象：`docs/n1b-disc-gate-plan.md` @ commit `da20408`（状态 `criteria-r10-pending-independent-review`）。

### 席位与投票

| 席 | 模型 | 范围 | 结论 | 计数 |
|---|---|---|---|---|
| A | grok-4.6 | 全文四层次 | fail | 3 B / 6 M / 3 m |
| B | gpt-5.6-sol | 全文四层次 | fail | 7 B / 2 M / 0 m |
| C | deepseek-v4-pro | 收窄两块 | **两次无消息失败，attempt-not-counted**（该席累计第七次失败） |

**主会话在派出审查的同时自查立案两条**（详见下），其中第 1 条与席 A B-01、席 B B-01**三路收敛**。

### 席 B（sol）终稿补充的独有 blocker（均经主会话核实成立）

- **B-03（sol）= `dw_watchdog_killed` 的 APPFREEZE 死亡证据**：`:625` 四合取的第四项「独立死亡证据 = faultlogger 条目含 `APPFREEZE`/watchdog 类签名」与 r10「事件条目不证明进程消失」两轴分立**正面冲突**——r10 修 `process_death_observed` 时没有同步这个同源字段。pidof 仍非空时该字段虚构「waiter 被杀」。修法：正向支另合取 `process_death_observed=observed-true`；APPFREEZE 只证 watchdog 事件。
- **B-04（sol）= D8b BEGIN-only 无收口**（= 席 A M-02 加深为 B）：storm 中途被杀（BEGIN|ws 在、END 缺、有死亡证据）是文档明知的合法平台终态，但 `window_end` 与 END 的 5 个字段无输入、无预注册 cause → F4 fail。r10 只修了完成态。修法：为 BEGIN-only+死亡证据冻结每字段具名 unobservable 落值。
- **B-06（sol）= selftest ⑥/⑩ 两组断言错误**：`:1156`「SIGKILL 条目**含与任意 Fault_Type 组合** → 签名 observed-false」——「任意」包含 CPPCRASH（届时第 1 支必真）；`:1159`「Ⅰ/Ⅱ/Ⅲ 任一夹具另发 POST」对 Ⅲ 结构不可达（POST 在则位点 ≥ P12）。修法：SIGKILL 反例限定 Fault_Type 不命中第 1/3 支；complete 用例只取 Ⅰ/Ⅱ、删 Ⅲ+POST。
- **B-07（sol）= `dw_destroy_distinguishable_from_timeout` 裸 `unobservable`**：`:633`「其余 → `unobservable`，逐字穷尽列举」不带 cause，违反 r10 冻结的 `cause=` 逐字比对规则——complete 主线（destroy resolve + `pre-destroy-ready`）落「其余」时 POST 无合法单值。修法：为每一类逐字冻结输出 cause。
- **M-01（sol）= `post-destroy-unobservable` 单一 cause 语义错盖**：死于 D7（destroy 未达）的 `dw_*` 输入不可得也记「post-destroy」——事实语义错误。修法：按 `destroy_call_state` 拆 `not-reached`（pre-destroy）与 `post-destroy` 两个 cause。
- **M-02（sol）= 聚合序未定义**（= 席 A M-01 = 主会话自查立案 2，三路收敛）。修法：冻结排序键（文件名字节序，与 FaultRecv 处理序解耦）。

### 两席对八项裁量的裁决对照

| 裁量 | 席 A（grok） | 席 B（sol） |
|---|---|---|
| ① 删 blanket+折域 | 维持 | **挑战**（单一 cause 错盖 destroy 前死亡 → 其 M-01 拆 cause） |
| ② D8b 载体 | 挑战（半状态无收口→M-02） | 维持（载体选择对；BEGIN-only 收口另立 = 其 B-04） |
| ③ no-death-evidence | 维持 | 维持 |
| ④ (b) 限 SIGKILL/SIGTERM | 挑战（B-03 非单值） | **挑战**（收窄方向对、但未关联 `:vpn` 且 SIGTERM 可捕获——不能单独证死） |
| ⑤ 无条目=false | 维持 | 维持 |
| ⑥ 第 3 支保留 | 方向维持、落地挑战（B-02） | 维持（须修 B-05/B-06 自测） |
| ⑦ 混合挂 unobservable | **挑战**（F1 上没收到东西） | 维持（组件内对；跨组件须三值 OR 修 B-01） |
| ⑧ P0 只非负 | 维持 | 维持（增设跨域顺序约束反而伪精确） |

席 B 对其九条 r10 修复的核对：B-01/03/06/08 已修；B-02 已修构造但新真值表另有 B-01；B-04 完成支已修、BEGIN-only 是新 B；B-05 修得不完整；B-07 组件内对、跨组件反向覆盖；B-09 幽灵已删、(b) 未关联 + watchdog 字段未同步。

### 第十一轮去重 blocker 终账（8 条，全部核实成立）

1. 真值表行 2/行 5 冲突（三路收敛；含「不可解析」双口径单值化）
2. `dw_watchdog_killed` APPFREEZE 死亡证据（sol B-03；两轴分立未同步到同源字段）
3. 死亡证据 (b) 支非单值（grok B-03 + sol B-02；`:858`/`:768`/分量三处谓词打架；glob 不绑 `:vpn` 已核实）
4. D8b BEGIN-only 无收口（sol B-04 = grok M-02 加深；合法平台终态被判 fail）
5. selftest ⑩ 守卫反例「不 fail」vs F2/F3（两路收敛）
6. selftest ⑥「任意 Fault_Type」+ ⑩「Ⅲ+POST」不可达（sol B-06）
7. `dw_destroy_distinguishable` 裸 `unobservable` 无 cause（sol B-07；成功主线不能稳定 pass）
8. 聚合序「首个/第一条」未定义（三路收敛；M 级——多条正向无论取哪个 verdict 相同，但事实记录非确定）

r12 修订按判断密度切包：真值表按支求值重写（1+8 合并）、死亡证据谓词统一（2+3）、D8b BEGIN-only 收口（4）、selftest 断言修正（5+6）、裸 cause 具名化（7）+ M 级若干。

## 十、硬边界状态（本轮未越界）

### 席 A 对上轮 12 条 blocker 的修复落地核对

10 条已修；1 条「完成态已修、中途死未赋值」（blocker 5 → 其 M-02）；1 条「方向已修、selftest 未同步」（blocker 6 关联 → 其 B-02）。观测窗算术第四次独立复核（467/525、P10 的 8 s 不在 P8）。

### 去重 blocker（全部经主会话逐条核实成立）

1. **真值表行 2/行 5 冲突——正向签名被旁路 unobservable 稀释**（主会话自查 + 席 A B-01 + 席 B 中间报告，三路收敛；B 级）。
   行 2「任一输入分量 unobservable → 签名 unobservable」先于行 5「任一支命中 → observed-true」求值。席 A 构造 C2 最狠：**单条目**（`Fault_Type=CPPCRASH` 可解析 + `Signal` 字段不可解析）即触发——支 1 字面已成立、签名却被洗成 unobservable → F1 不命中 → pre-only **pass**。探针崩溃 fail-open。
   席 A 另指出「不可解析」双口径：证据规则 5/混合支/⑥(d) 说「unobservable 不 fail」、判别方法 (2) 说「解析失败 F8 fail」——两条读法下 ⑥(d) 的「不 fail」期望都错，verdict 非单值。
   **修法（主会话已裁）**：改按支求值——支 1 只看 `fault_type`、支 2 只看 `signal`、支 3 看 `fault_type ∧ last_visible_site ∧ 无矛盾`；**任一支所需输入齐备且命中 → observed-true（行 5 先于「不可求值」）**；「不可求值 → unobservable」仅当无一支可判 true 且至少一支之必要输入 unobservable；行 1（FaultRecv 整体失败）仍最先。双口径单值化：判别方法 (2) 豁免 Fault_Type/Signal 字段（F8 面限时间戳等其余字段）或一律 F8——取豁免路线（§4.2 不可解析词根本身是未实测平台产物，fail 违反发现语义），写明豁免边界。
2. **selftest ⑩ 守卫反例断言与 F2/F3 冲突**（席 B 中间报告 + 席 A B-02，两路收敛；B 级）。
   「第 3 支守卫反例：marker 序列自相矛盾 → 第 3 支不成立 → observed-false → **不 fail**」——但短时步位点多半在 P5T 前（PRE 未发 → F2）且「marker 矛盾」本身独立命中 F3。正文规则 vs 用例断言非单值。
   **修法（主会话已裁）**：与 Ⅲ 同构——`observed-false` 为事实记录验证，verdict 由 F2/F3 承载（矛盾时叠加 F3），删「不 fail」。
3. **死亡证据 (b) 支与「仍存活 → F9」非单值**（席 A B-03；B 级；对主会话 r10 裁量④的正向挑战）。
   (b) 做成不经 PidOfVpn 的独立充分条件后：PRE 在、pidof 仍非空、同窗有本 bundle SIGKILL 条目（FaultProbe glob `*cn.alfadb.netbird.n1bdisc*` 不绑 `:vpn`，UI 进程同 bundle）→ `:858`「或平台终止」读 pre-only pass vs 观测窗到点「仍存活」读 F9 fail，两读都可达。
   **修法（主会话已裁）**：(b) 降为辅助——SIGKILL/SIGTERM 条目**不得在 pidof 非空时单独证死亡**；死亡证据以 `process_death_observed` 为唯一谓词源，`:858` 的「或平台终止」改为引用该分量而非自立谓词；`:768`「仍存活」谓词同步挂到该分量。glob 收窄到 `:vpn` 或条目绑 pid 属探针实现细节，登记为设计注记不强制。

### 席 A 的 M 级（待席 B 终稿合并去重后一并排 r11）

M-01 聚合序未定义（= 主会话自查立案 2，两路收敛；修法：FaultRecv 取回序 + 文件名字典序 tie-break）；M-02 D8b 中途死亡 `we` 无载体（BEGIN 在 END 缺 → `unobservable(cause=post-destroy-unobservable)` 或具名 cause，selftest「缺项 fail」限定为 END 在而 we 空）；M-03 混合支把可解析 SIGKILL/SIGTERM 也稀释掉（signal 分量「可解析平台终止段」优先于混合 unobservable）；M-04 行 2 多 cause 并存无唯一解（冻结合取序）；M-05 D2/D6 默认支省略号 cause 残留；M-06 POST 节 skip 编码计数未同步 16/8。

### 席 A 对八项裁量的裁决

维持 ①③⑤⑧；挑战 ②（D8b 半状态无收口→M-02）、④（B-03）、⑦（「挂 unobservable 不挂 false」在 F1 上没收到任何东西——unobservable 与 false 同为不命中，只改诊断标签；真问题是行 2 的 OR 拦截）；⑥ 方向维持、落地层挑战（B-02）。r9 九项裁量无改判。

### 对席 C（deepseek）失败的处置记录

r11 两派均无消息失败（第六、七次）。收窄策略在该席上的成功率不稳定（r9 一成一败、r10 成、r11 两败）。本轮记 attempt-not-counted；席 A 的四层次全文审查已覆盖其收窄范围（真值表、聚合规则均在席 A 的 B-01 内详查），覆盖不受影响。

## 十、硬边界状态（本轮未越界）

判据未冻结；未分配 AUTH/pair 与 evidence ID；未请求也未执行任何物理 campaign；未写 N1b r2（须依 DISC 实测事实冻结）。

## 九之三、第十三轮审查（r12 版）——结果与登记

被审对象：`docs/n1b-disc-gate-plan.md` @ commit `da20408` 后（r12 五包 + 深索 M 采纳，至 `8e69ad3`）。

| 席 | 结论 | 计数 |
|---|---|---|
| C deepseek-v4-pro（收窄：U1 两步求值，27 格机械枚举） | **pass** | 0 B / 2 M（已采纳修正 `8e69ad3`） |
| A grok-4.6（全文四层次） | fail | 1 B / 3 M / 3 m |
| B gpt-5.6-sol（全文四层次） | fail（终稿待收） | 已确认 ≥3 B |

**收敛是真实的**：grok 上轮 3B / sol 上轮 7B 的修复核对全部确认已修；grok 本轮仅 1 B（u7 窄窗——主会话裁量⑤留给挑战、被证实）；sol 与 grok 在 u7 窄窗独立收敛。deepseek 席第三次收窄尝试成功（27 格组合表形态），其 pass 票覆盖 r12 最复杂新规则（U1 两步求值）的全函数验证。

### 已核实的 blocker（终稿后补全去重）

1. **u7 的 P5T–P6 窄窗**（两席收敛；grok B-01 = sol B1）：PRE 已发、D7_BEGIN 未发的合法平台死亡——u7 两支前件均假（END 支要求 BEGIN 后静默、marker-gap 支要求无死亡证据、V4 要求位点 P6 而实为 P5T）→ 无落点 → F4 烧掉冻结解释句明文保护的平台终态。与 r12 已收口的 D8b 阶段未达同构。修法（两席同向）：u7 加 `unobservable(cause=stage-not-reached)` 支（死亡分量 true ∧ D7_BEGIN 缺 ∧ 位点 ≤ P5T），覆盖 u7 全部派生字段，selftest 补正例。
2. **`dw_watchdog_killed` true/unobservable 重叠**（grok M-02 升格评估中 + sol B2）：物理上最真的「waiter 在 poll 中被杀」（SPAWN 在、EXIT 缺、`_T`/`_C` 均缺）同时命中 true 支四合取与「`_C` 缺无论 `_T` 在否不得 true/false」支。修法（grok）：废除「`_C` 缺一律不得 true」——`_T`/`_C` 均缺 + 四合取齐 = 允许 true；`_C` 在 + pidof absent 仍不得 true（destroy terminal）；⑪ 正例写明双缺。
3. **`dw_destroy_distinguishable` 17+2 正交相加**（sol B3）：17 个 class 值与 destroy 超时/reject 2 结局是正交输入却按互斥枚举相加——「class 有值 + destroy 未 resolve」组合落两行。修法（主会话）：改为求值序声明——先判 class 是否命中 fd-event-like/timeout-like 两判定行；「其余」内部再按 destroy resolve 与否分流（2 结局是「其余」的内部分流、非平级枚举）。

### sol 终稿（4B/2M/1m）与终账

sol 独有 B-01（per-file 部分失败）核实成立：:931 快照差分取回文件名集合、:952 契约逐条解析，但 :1040 前置只建模全局 FaultRecv 失败——部分文件取回成功部分失败时两读都错（任一失败读=全局 unknown 稀释已见正向，重现 fail-open；全部失败读=失败文件无落值）。修法（sol）：FaultRecv 可用性按文件建模——已取回条目照常求支，失败文件贡献 faultrecv-unavailable unknown，聚合仍 true>unknown>false。

**第十三轮终账（r14 记账更正，sol m-02）：4 条 blocker（两席去重，全部核实成立）**——原计「5 条」把第 5 项「登记块 16+2 笔误」计入，该项自标 m 级非 blocker；轨迹应为 12 → 8 → 4。原「5 major/2 minor 全处置」表述同样过宽（pre-PRE 裸 cause 系裁量⑥维持未修，非处置）。
1. u7 P5T–P6 窄窗（两席收敛）
2. dw_watchdog_killed true/unobservable 重叠（两席收敛；sol 另指 SIGKILL 非 watchdog 专属证据——修法须处理）
3. dw_destroy_distinguishable 17+2 正交相加（两席收敛；sol 修法：先按 destroy 结局分区——timeout/reject 对所有可并存 class 统一 destroy-unresolved，仅 resolved 再按 17 class 求值）
4. FaultRecv per-file 部分失败（sol 独有）
5. （主会话登记块笔误 16+2 计为 m 级，随 r13 修）

M 级（去重）：grok M-01 混合支先于 APPFREEZE 未写明；grok M-03/sol 无 = false 支残留合取；sol M-01 call-boundary-incomplete 被并入 post-destroy（透传原 cause）；sol M-02/grok m-03 = pre-PRE 裸 cause（裁量⑥族，两席均不升 B）；m-01/m-02 标签与计数。

修复核对：两席对上轮各自 3B/7B 全部确认已修（sol 2 条「部分修」的部分处均为本轮新立案的同缝，非回归）。

### grok 的 M/m（已并入上节）

M-01 混合支未写明先于 APPFREEZE（⑥(d) 依赖）；M-03 `process_death_observed` false 支残留「且无任何上述死亡证据」（UI-SIGTERM + pidof 非空时三态无值——修法：删该合取）；m-01 登记块「16+2」过期（应 17+2）；m-02 追溯标签 P3/P4 打架；m-03 u6/D6 省略号 cause（裁量⑥族，维持不修）。

### 裁量裁决（grok）：①②③④⑥⑦ 维持；⑤ 挑战成立（= B-01）。


## 九之四、第十四轮审查（r13 版 @ e3f33ec）——终账

| 席 | 结论 | 计数 |
|---|---|---|
| deepseek-v4-pro（收窄：两表机械枚举） | **pass** | 0 B / 2 M（已采纳；其一的「不可达」注后被两席证伪——采纳失察见失误 #29） |
| grok-4.6（全文） | fail | 1 B / 2 M / 2 m（上轮 1B/3M/3m 全确认已修无回归） |
| sol（全文） | fail | 3 B / 2 M / 2 m（上轮 4B 全确认已修；其一 m-02 揭出本 register 记账错误） |

**程序事件**：deepseek 先回、其 2M 被主会话当场采纳落盘（ec57b03/d0af06e/1016fd1），违反「审查期间不动文件」纪律；两席重绑 e3f33ec 出票（grok 作废混版票重发）。记为失误 #28（版本纪律）。其中采纳的「② 组合不可达」注未独立核实、被 sol+grok 独立证伪（EXIT 可早于 `_T`——inwait 竞态段明文）——失误 #29（采纳审查席数学断言未核实）。

### 去重终账（4 条实质 + 记账）

1. **二维分区 0/0b 映射前提缺口**（grok B-01 = sol B-03，两席收敛）：「有 RETURN、无 `_C`、无 resolve」的合法平台死亡上 `dw_destroy_distinguishable_from_timeout` 无落点 → F8 烧 ID；死亡收口透传行与第一维括注在 E3 方向两读。修法（grok 四步有序互斥，sol 同向）：(0) skip2∪死亡收口3 → 透传；(1) `_C` 在且无 resolve → `destroy-unresolved`（「已发起」钉死为 `_C` 在）；(2) 已 resolved → 第二维逐值表；(3) 类 0/0b 单列直接映射（0b → `destroy-call-unobserved`，0 → SKIP 位点字面），不入「仅 resolved」第二维。
2. **watchdog ② EXIT 缺限定**（grok M-01 = sol B-02，两席收敛）：「EXIT 早于 `_T`」可达（inwait 竞态）→ ② 截胡 ④，waiter 已正常退出却记 call-boundary-incomplete。修法：② 加「且 EXIT 缺」；删 r14 错误「不可达」注。
3. **watchdog ③ 因果包装**（sol B-01 + grok 裁量①挑战；严重度两席分歧、修法同向）：③ 的 true 不含任何 watchdog 专属条目（drain 期死亡也命中）。修法（两席同向）：③ 合取 `DW_BARRIER` 或 `dw_inwait_confirmed=observed-true`（poll 进入证据）；结论句改「waiter 在 destroy 未及窗内死亡**且已确认进入等待**」；字段名 `dw_watchdog_killed` 保留为历史名、加语义注防 N1b 误引（改名波及 POST 冻结字段集，登记为后续裁量）。
4. **FaultRecv 混合支「正向段」按支定义**（sol M-01）：「可解析正向」现只认 CPPCRASH/JSRAWERROR，可解析 APPFREEZE@短时步+无矛盾（第 3 支完整前件）会被混合支压成 unknown。修法：「无可解析正向」= 无任何三支完整前件为真（含第 3 支）。
5. **M/m**：⑪ 反例补 EXIT 缺 + EXIT 在对照（grok M-02）；u7 skip-first 无夹具（grok m-02）；`dw_waiter_spawned`/`no-worker` 两项 r14 已修（sol 确认不计票）；版本闭合（r13 状态下正文有 r14 字样——随 r14 修订自然解决）。

### 轨迹（更正后）：12 → 8 → 4 → ?

## 九之五、第十五轮审查（r14 版 @ a43d0c4）——终账

| 席 | 结论 | 计数 |
|---|---|---|
| deepseek-v4-pro（两块机械枚举） | **pass** | 0 B / 3 M |
| grok-4.6（全文） | **pass** | 0 B / 2 M / 4 m（八点高风险逐格核） |
| sol（全文） | **fail** | 3 B / 1 M / 2 m |

**冻结未达成**：两席 pass 票被 sol 的三条 blocker 推翻——deepseek/grok 验证了主会话不可达性注的内部逻辑（0b 优先级确在行 1-11 前）但未查未知位前置门的旁路（sol 独捉）；B-02（marker class 未来依赖）历轮无人发现。三席互补盲区的又一次兑现——也是「0 blocker 才可冻结」门槛的价值证明。

### 三条 blocker（全部经主会话核实成立）

1. **B-01 未知位前置门绕过 0b，四步表在「other-revents + `_C` 缺 + 无 resolve」无落点**（sol；主会话不可达性注的主论证错误——「0b 截获一切无 `_C` 输入」只对进表输入成立，前置门路由的输入不进表）。构造：`ret=1,revents=64` + worker RETURN/EXIT + 死于 `_T` 前 → (0)-(3) 全不命中 → F8 烧合法平台死亡。修法：派生序冻结「合法域 → 0/0b → 未知位 → 普通 1-11」，0b 的 SKIP/`_C` 门提到未知位门前。
2. **B-02 即时 `DW_RETURN` marker 冻结 `class=<c>`，但最终分类依赖未来知识**（sol；最深一条）：class 取决于 SKIP、`_T`/`_C` 最终是否出现、`at` 相对 T/C——worker 发 RETURN 时这些均未发生；同一前缀可通向不同终局，即时 marker 无论填什么都至少错一支。修法：从 marker 删 `class`，只发 raw（ret/errno/revents/elapsed/at）；runner 在收齐 SKIP/T/C/终态后唯一派生最终 class；明确 raw 与派生无双源。
3. **B-03 watchdog ③ 的 BARRIER 析取支不足 + `:680` 因果定义残留**（sol B-03 + grok M-02 收敛）：`:668` 明文 BARRIER 只证到达 poll 调用点、可被抢占、单独不证进入等待——「BARRIER∨inwait」在合法 trace 上退化为 BARRIER、名实矛盾（BARRIER→poll 间隙死亡被错记「已确认进入等待」的 true）。修法：③ 只认 `dw_inwait_confirmed=observed-true`，BARRIER-only 归 ⑤；`:680` 头句改历史字段名声明。

### M/m 汇总（三席去重约 10 条）
grok M-01（② 的 EXIT 缺写进了 `→` 后括注而非前件槽——最坏读仍截胡 ④）/ grok M-02 = sol B-03 后半（:680/:684 残留）/ sol M-01（FaultRecv「正向段按支定义」循环——正向读聚合字段而聚合字段待正向定义；修法：raw 级三谓词先于聚合）/ sol m-01（skip 透传理由对 dup-failed 不实——它执行 destroy）/ sol m-02（r13 旧块计数未原位标注）/ grok m-01 = deepseek M-03（⑪「四证据」计数滞后）/ deepseek M-01 = grok m-02（不可达注次论证行 4 不依赖 `_C`——随 B-01 重写一并消解）/ deepseek M-02（③「五证据」vs ∧链 6 项计数）/ grok m-03（活规则残「二维/第一维/第二维」索引）/ grok m-04（FaultRecv 仅支 3 命中时 signal 侧未定义——与 sol M-01 同修）。

### 轨迹：12 → 8 → 4 → 3。B-02 是 r9 以来第一条全新类型（marker 发射时序 vs 终局分类的架构性矛盾），非同构收口问题。

## 九之六、第十六轮审查（r15 版 @ ad37cd9）——终账

| 席 | 结论 | 计数 |
|---|---|---|
| deepseek-v4-pro（派生序+四步表机械枚举） | fail | 1 B / 1 M |
| grok-4.6（全文） | fail | 1 B / 1 M / 3 m |
| sol（全文） | fail | 4 B / 1 M / 1 m |

**去重 5 条 blocker（全部经主会话核实成立）：**
1. **派生序传播缺口**（grok B-01）：sol B-01 的修法只写进锚句（:626-630）与一条补格（:1263），未知位门操作句（:656-660）、r9 两道门先后、:689/:653/:944、④ 无 `_C` 钉（:1253）仍按旧范围——实现者按操作句实现则原构造仍 F8。**上轮「验证论证的适用范围」教训在修复者（主会话）身上原样重演**。
2. **`item=D-W` 笔误**（三席收敛）：0/0b 前置检查的 SKIP 支应写 `SKIP|item=destroy`，主会话 P1 手工落盘写成 `item=D-W`（另一枚 marker——D-W 整体 skip 无 RETURN、走步 (0) 编码）。附带 sol 的分域精化：无 RETURN 只走 skip/death 编码、有 RETURN 才进 13 类；`item=D-W` 与 RETURN 同现 → F3 矛盾输入。
3. **POST class 死锁**（sol B-01）：B-02 修复未传播到 POST——:357 冻结字段集仍要求 `dw_outcome` 含 `dw_return_class`，而 :603 声明 runner 收齐终态（含 POST）后唯一派生。complete 成功态不可实现。**主会话裁定采 sol 方案 (b)**：探针主线程于 P12 发射 POST 前唯一派生（P12 时全部输入已发生——B-02 的洞是 worker 即时 marker 的未来依赖，P12 无此问题）、runner 从 raw 独立重建比对（沿 E2 ledger digest 模式）、pre-only 死亡收口由 runner 派生；登记为裁量供下轮挑战。
4. **inwait 字段族死亡收口**（sol B-03）：BARRIER→poll 间隙死亡时 `DW_INWAIT` 未发，src/conf/samples/errno 四字段无预注册 cause → F4。pre-only 死亡 cause 域只扩了 return_class/join_result。
5. **inwait state=S 假阳性**（sol B-04）：`/proc/syscall` 不可读时仅 state=S 即 true——poll 返回后 worker 在其他可中断睡眠（HiLog/锁/写）同样呈 S。**主会话裁定**：syscall 不可读一律不得 true（宁缺勿误路线；poll-active 原子窗属探针实现变更、更侵入，不采）。

M/m：④ 18 类句两读（sol M-01：先要求 18 值过步 (3) 又说明 skip/death 走 (0)/(1)）；未知位门输入域措辞（deepseek M-01：「`_C` 在或 SKIP」→「无 SKIP 且 `_C` 在」）；A5/① 缺 legacy class 负例回归钉（sol m-01）；grok m-01/m-02/m-03（:668 行号漂移、:359「现按二维」残留、raw 谓词 (3) 缺归一化）。

**轨迹：12→8→4→3→5。** 回升但性质明确：5 条中 4 条是 r15 修复自身引入的缝（主会话 P1 手工 2 条、P2/P3 各 1 条）、1 条是收紧暴露的既有缺口。修复面即新审查面。上轮三席票型（两 pass 一 fail）与本轮（三 fail）的翻转说明 pass 票的可靠性依赖第三席对「适用范围」的独立检验——本轮 grok 的任务书校准（先找旁路再采信）在其票里被实际执行并抓到传播缺口。

**主会话同型错误累计 31 次**（第 31 次 = `item=D-W` 字面笔误，落在 r15-P1 手工落盘段）。

## 九之七、第十七轮审查（r16 版 @ 3cef412）——终账

| 席 | 结论 | 计数 |
|---|---|---|
| deepseek-v4-pro（派生序+分域 12 格 / inwait 四关键格） | fail | 1 B / 0 M |
| grok-4.6（全文） | fail | 3 B / 4 M / 3 m |
| sol（全文） | fail | 1 B / 1 M / 1 m |

**去重 3 条 blocker（三席高度收敛，全部核实成立）：**
1. **「无 RETURN」假穷尽分划**（三席收敛，deepseek 格 9 / grok B-01 双形态 / sol B-01）：r16 分域声明「无 RETURN 只走 skip 编码 ∪ pre-only 死亡收口」漏了两个合法形态——(a) **barrier-never-observed + pre-only 死亡**（worker 已 spawn、barrier 未观测、destroy 顺延 SKIP、进程死亡：五态 not-called 但 not-called 支只路由 no-live-fd/dup-failed）；(b) **complete + P10 join-timeout + 无 RETURN**（终态标志只在 RETURN 后置位 → 该路径必然无 RETURN；协议明文允许 timeout 后继续 D6b→P11→P12 走 complete；POST 强制 class 非空 → F4/F8/F9 烧掉预注册的「waiter 卡住」观察事实）。修法：无 RETURN 分域补类 0 支（仅 item=destroy SKIP、不要求 RETURN——其两条真实协议路径本就无 RETURN）+ join-timeout complete 收口具名 cause（grok 建议 `poll-never-returned`）+ D6b 位次冲突裁定 + worker raw 交接声明。
2. **inwait 域扩传播不全**（grok B-02）：`inwait-marker-unobserved` 只扩了 source/samples 域，`dw_inwait_confirmed` 活域漏扩——⑪(d) 夹具声称 pass 与域缺口 F4 非单值。r16-P2 又一次「部分字段传播」。
3. **P12 派生比对未挂闭集**（grok B-03 = sol M-01）：比对失败写「沿 E2 模式 fail」但 F5/F8 字面均不含——按局部句 fail、按闭集实现漏判（fail-open）；且 P12 输入清单漏 poll/drain raw（13 类真正输入在 worker，主线程派生依赖未冻结的跨线程通道）。**两席均驳回主会话裁定①**（方向可、必须补挂载+raw 快照）；**均维持裁定②**（syscall 三合一，正样本时序可达性经两席独立验证）。

M/m 去重：F3 轴错（item=D-W+RETURN 应挂 F8(2) 而非 F3——grok M-01）；errno 第四字段无落盘名（M-02）；`:987`/`:1331` 残「照 13 类判定表」（M-03）；POST 其余派生字段无所有权声明（M-04/sol 同）；r16 登记 5 minor vs 实 4（m-01/sol m-01）；⑪(d) 未钉 barrier 无 RETURN class（m-03）。

**轨迹：12→8→4→3→5→3。** 三席对 r16 五条 blocker 的修复核对：传播对齐、state=S 收紧、inwait 操作句、item=destroy 字面**全部确认落地**（sol 明确「旧 B-02 逐格正确」「旧 B-03 四字段均落值 pass」）；新缝集中在 r16 自己的两个新声明（假分划、P12 裁定未挂闭集）。**主会话裁定①被两席驳回**——裁定方向未被推翻（P12 派生本身可行），但落地不完整（闭集挂载 + raw 交接缺失）即构成 fail-open，须补。

## 九之八、第十八轮审查（r17 版 @ 2806e41）——终账

| 席 | 结论 | 计数 |
|---|---|---|
| deepseek-v4-pro（分域 14 格 + 快照） | fail | 1 B / 3 M |
| grok-4.6（全文） | fail | 2 B / 3 M / 4 m |
| sol（全文） | fail | 2 B / 1 M / 2 m |

**去重 2 条 blocker（三席收敛，全部核实成立）：**
1. **D6b 位次裁定反事实**（grok B-01 = sol B-02；主会话裁定③被正面驳回）：worker drain/poll 对象就是 `fd_dup`（:616），裁定句「非 worker 持有的原 fd」前提错误；abandoned worker 仍阻塞在 poll(fd_dup) 上，主线程 D6b 对同一 fd read/close——close 唤醒与否使 (d) 的「RETURN 缺」前件非单值（晚到 RETURN vs (d) 成立）、D6b 偷走 destroy 本应留下的 POLLIN/HUP 污染核心发现目标；:811 归因洁净句被裁定自相矛盾。迟到 worker 无 timeout cutoff，可在 D6b/P11/P12 间写快照发 RETURN。修法（grok）：join-timeout 形态禁止 D6b 对仍可能被 poll 的 fd_dup 做 read/close（close 交进程退出）；(d) 的 RETURN 缺检验须在「主线程不再触碰 fd_dup」之后。
2. **共享快照不构成冻结的 P12 输入面**（sol B-01 = grok B-02 ⊇ deepseek B-01/M-01，三席三个角度）：(a) 快照只冻 poll 五 raw、漏 drain 终态（class 行 7 的第四前件依赖它——同一 poll raw + drain=eagain/zero-read 应落不同类，P12 无输入区分）；(b) 无 release/acquire 发布边界（松散发布可读到初始化值，raw 分叉可同 class 漏检 = fail-open）；(c) 同值无机制（两次写，单次读未钉死）；(d)「RETURN 缺 ⟹ 无 raw」被「发射前写入」证伪（⑪(e) 构造窗：poll 返回→写快照→RETURN 前阻塞）；(e) 第五形态（join 成功 + capture 丢 RETURN marker——标志置位而 capture 无，= capture 完整性矛盾）无收口。修法（grok 主案）：终态标志为快照可读门（标志置位后 P12 才可读）；join-timeout∧标志未置 → 无视快照一律 (d)；标志已置∧capture 无 RETURN → 单独具名 `return-marker-missing`（F8(2) capture 完整性矛盾，非 poll-never-returned 冒充）；同值机制 = 单次读钟/errno 同源两写。

M/m 去重：`dw_drain_end` 交接缺失（sol B-01 内）；`dw_outcome` 全字段所有权的 EXIT 小窗（sol M-01）；④ 段落错位（grok M-01——complete 夹具收口句插在反例 A 钉后）；`:734` confirmed 仍标三态（grok M-02）；criteria-gap (2) 本体未扩 P12≠runner（grok M-03）；:1313/:1314 计数 5/6 冲突（grok m-01 = sol m-01 两席收敛）；「13 类」描述残留（grok m-02）；登记计数口径（sol m-02）。

**轨迹：12→8→4→3→5→3→2。** 三席对 r17 修复核对：Z1 (c) 支、Z2 域扩、Z3 比对挂载、Z5 M/m 全部确认落地（三席各自构造复走通过）；两条 B 全部打在 r17 的两个新声明（D6b 裁定③、快照机制）上。**裁定①二度驳回**（方向仍可成立，落地缺 drain 交接/发布边界/cutoff/raw 同值四处）；裁定②三度维持。

**主会话失误 #32**：裁定③（D6b「非 worker 持有的原 fd」）——判断性最强的动作里未回读 :616 核对 fd 身份。

## 九之九、第十八轮终账（修订版——sol 终稿补全后）

九之八已登记的三席结果与去重终账经 sol 终稿（2B/1M/2m）确认，无出入。补充两点：
1. sol 的快照五点缺陷中最深的一条：**「同 class 等价类内 raw 分叉漏检」**——marker `ret=0,revents=0,elapsed=100` vs 主线程读到初始化 `elapsed=0`，两边同落 `spurious-early`，F8(2) 只比派生 class 则 pass——fail-open 的具体机制。
2. 裁定①三席合计四度驳回（r17 两度 + r18 两席）；裁定②三度维持。

**轨迹：12→8→4→3→5→3→2。** r18 修复两包已并行派出（P1 D6b 整段 skip + 归因洁净恢复 + A10；P2 快照单源化：单次读同源两写 + 终态标志门 + drain 入快照 + 第五形态 F8(2) 收口 + EXIT 序对调）。

## 九之十、第十九轮审查（r18 版 @ cf7b084）——终账

| 席 | 结论 | 计数 |
|---|---|---|
| deepseek-v4-pro（分域 (a)-(e) + D6b skip 16 格机械枚举） | **pass** | 0 B / 4 M |
| grok-4.6（全文） | fail | 1 B / 3 M / 4 m |
| sol（全文；首次连接中断后重派完成） | fail（终稿待收，中断前已确认竞态 blocker 与 grok 同根） | ≥1 B |

### 已核实的 blocker（grok B-01 = sol 中断前报告收敛——标志门边界竞态）

**边界形态 T**（P10 盒到期时标志未置 → JT 登记 + abandoned + D6b skip → P11 → 此后 P12 前迟到 worker 完成序列）：
- **T-RETURN 在**（JT=1, F=0→1, R=1）：(d) 四前件全真（非 skip∧活到 P12∧JT∧R 缺——**R 缺假**）→ 非 (d) → 读快照 13 类 ✓；但 **F8(2) 对 `dw_join_result` 无重建规则**——runner 侧若无「D6b skip 在 → join_result 重建为 join-timeout」的写死规则，最坏以 EXIT⇒joined 比对 P12 的 join-timeout → **F8(2) 烧掉合法迟到完成 complete**（fail-closed 误伤）。
- **T-RETURN 缺**（JT=1, F=0→1, R=0）：(e) 标题命中（标志已置∧capture 无 RETURN）→ F8(2) ✓；**但 (d) 四前件在此也全真**（R 缺为真）——(d) 列在 (e) 前，只实现四合取的读者会 **(d) 截胡 (e)** → poll-never-returned + complete pass = **fail-open**（快照已证明 poll 返回）。
- **sol 中断前报告的格**（JT=1, F=0, R=1——标志未置而 RETURN 已入 capture）：标志门说「一律 (d)」、(d) 前件「capture 为准」失败、(e) 要求标志已置——无收口。deepseek 16 格表的第 7 格（JT=1,F=0）默认 R=0 未枚举此格；grok 走查确认了 T 两个子格但此第三格未显式覆盖。**三格同根：标志（门）与 RETURN 入 capture（capture 前件）不是同一事件的两面——标志在序列末尾。**

grok 修法（四点）：(d) 改五前件（+标志未置）；join_result runner 重建规则写死（D6b skip → join-timeout，EXIT 不喂 join 轴）；联合形态合法性声明（JT∧13 类∧EXIT 同现 = 合法）；④ 补两钉。

deepseek pass 票说明：其 16 格对 (a)-(e) 与 D6b skip 的穷尽性验证成立（第 8/10/11/12 格特别结论均正确），但第 7 格默认 R=0——sol 的 F=0∧R=1 格与 grok 的 T-RETURN 缺格（F=1∧R=0 被 (d) 四前件截胡）未在其枚举维度内。pass 票在其范围内有效。

M/m 去重：grok M-01（skip 表实体未收编）/M-02（(e) 「join 成功」与标题不等价 = deepseek M-03 同）/M-03（A10 强度）/M-04（两 cause 字面 = deepseek M-04 同）；deepseek M-01（「四子域」标题 = grok m-01 同）/M-02（类 0 路径陈旧 + (b)/(c) 求值序）；grok m-02/m-03/m-04。

**轨迹：12→8→4→3→5→3→2→1。** r18 两条 B 的修复（D6b 重裁 + 快照单源化）经三席确认落地（grok 明确「原 B-01 唤醒分叉不可达」「发布窗封闭」「第五形态有落点」）；本轮唯一 blocker 全部集中在标志门边界竞态（三格同根）——修复面即新审查面的模式继续，但每轮新缝的根因在收窄（本轮只剩一个时序门的边界）。

### sol 终稿（绑定 cf7b084，r19 草案未计入）——fail 1B/4M/3m，与 grok B-01 同根确认

sol 终稿确认三格同根的 B-01（与 grok 收敛），并给出**不同的修法方向**：
- **grok/我已落地的 r19-P1**：五前件 (d) + 1000 ms 局部盒 cutoff + `flag-race-window-expired` 具名收口；
- **sol 建议**：不等待 flag——冻结 P12 cut 状态写入 POST（`worker_terminal_published_at_p12`）；(d) 改为 `JT∧F=0`（删除 R 缺前件、F=0 优先于 capture R）、cause 改 `worker-output-incomplete-at-cut`；F=0 时 R 仅保 raw 不喂 class；F=1∧R=0 唯一走 (e)；F=1∧R=1 走普通类；join_result sticky JT。
- **对 r19-P1 cutoff 的批评**（sol 审查期间看到草案后指出）：「禁止用无新局部上界的 spin-wait 修补，worker 可永久活挂」——r19-P1 已采纳（1000 ms 局部盒 + 具名收口 + 禁止拿 525 s 当上界），但 sol 的 cut-state 方案更根本（不等待任何未来事件、在 P12 时点冻结观测状态）。

sol M-01~M-04 与 grok M 大面积收敛（skip 表实体/A10 强度/(e) P12 可实现性/F8 背书过强——M-04 为独有且深刻：「F8 比的是派生值不是六 raw、runner 无法发现自身漏比，一致性保障来自源码约束+release/acquire 而非 F8」）。m-03 为独有（「读单调钟」vs「各读一次」措辞两读）。

**第十九轮最终账：deepseek pass 0B/4M、grok fail 1B/3M/4m、sol fail 1B/4M/3m——去重 1 blocker（标志门边界竞态，三席三角度收敛：grok 边界形态构造、sol 程序序论证、deepseek 16 格表的第 7 格边界）。r19-P1 已按 grok 方案+sol cutoff 建议落地；sol 的 cut-state 替代方案登记为裁量供第二十轮挑战。**

### 第二十轮中期（deepseek 已回、sol 中间+新增、grok 在跑）

| 席 | 结论 | 计数 |
|---|---|---|
| deepseek（14 格机械枚举 + 竞态窗深查） | fail | 1 B / 3 M |
| sol（中间 + 新增两点，终稿在跑） | fail | ≥3 B |
| grok | 在跑 | — |

**三席已收敛的 blocker（同一根，全部核实成立）**：
`flag-race-window-expired` 盒到期支三重不自洽（我 r19-P1 落地 cutoff 变体时未走查完的格——派发书里给三席埋的疑点全部命中）：
1. **F8(2) 机械矛盾**（deepseek B-01 = sol 第一点，逐字验证最完整）：`:698` P12 写 cause / runner 从 R=1 capture 重建必得 13 类 → 比对必 fail，与 complete pass 矛盾；`:699` 的「一致」断言句内自相矛盾（「runner 正常重建」⟹ 13 类 ⟹ 与 cause 不一致，却断言一致）。双读法死锁：读 A（同 cause）⟹ F8 fail；读 B（class 一致）⟹ P12 须写 13 类但标志门禁读、物理不可能。
2. **P12 不可判**（sol 第二点）：`:684`「以 capture 为准、P12 检验」——设备侧 P12 看不到 host capture，F=0 时无法区分 R=0/R=1；统一等 1000 ms 则 R=0 误落 flag-race、不等则 R=1 误落 poll-never。
3. **域未注册**（sol 第三点）：`flag-race-window-expired` 不在 19 值域声明/poll raw 域/四步表步 (0) 映射——未预注册 cause → F4，与 pass 矛盾。

deepseek 修法（二选一）：(a) 该支 F8(2) 豁免（非对称输入形态，「同一事实两算」前提不成立）；(b) runner 增 capture 可见签名重建同一 cause。sol 修法方向：cut-state 方案（P12 只依赖本地 F、只记录 cut 状态、不与 runner 重建同轴比对）。

deepseek M-01（1000 ms 容器归属错——应在 P12 ≤5 s 内非 58 s 收尾）、M-02（「相邻语句」论证窗口不对齐）、M-03（:620 位点枚举未纳入 P12）。

### 第二十轮终账（三席齐：deepseek fail 1B/3M、grok fail 2B/2M/4m、sol 终稿在跑但其三点已全部经主会话核实）

**去重 2 条 blocker（三席高度收敛，全部核实成立）：**
1. **`flag-race-window-expired` 盒到期支三重不自洽**（三席三角度收敛：deepseek F8(2) 机械矛盾/双读法死锁证明、sol P12 不可判 + 域未注册、grok F8(2) 假一致 + 19 值未扩 + 反 blanket）：r19-P1 cutoff 变体（主会话自作主张部分）首轮被证伪——与 r14-r18 每轮新声明同构。**grok 修法**（sticky 例外——与 join sticky 同构）：runner 增 class 轴 sticky 重建例外（`SKIP|item=D6b` ∧ capture 有 RETURN ∧ 盒到期标志未置 → runner 重建 `flag-race-window-expired`、禁止走 13 类）；闭域 19→20、四步 (0) 6→7 同步；废除 :699 假断言。**deepseek 方案 (b)**（capture 可见签名重建同 cause）与 grok 修法同构。sol cut-state 为替代方案。
2. **`:640`/`:691` 旧活路由未废**（grok B-02）：r18 冻结「JT∧F=0 → 一律 (d)」与「非 (a)-(e) 且有 R → 13 类」两条早于 r19 求值序的活路由未随竞态窗改写——第三格整格被 :640 截走（盒内置位成功路也 F8(2) 烧 ID）或被 :691 绕过等待（无 acquire 读快照）。修法：:640 改「JT∧F=0∧R 缺 → (d)；JT∧F=0∧R 在 → 竞态窗」；:691 补「非竞态窗待决、标志已置」。

M 去重：A11 六字段不可实现（drain_end 在 poll 前写——grok M-01，改为五 raw+钟、drain_end 单列）；1000 ms 上界论证偷换前提（EXIT 相邻≠R 相邻——grok M-02 = deepseek M-02；容器归属错 58s→P12≤5s——deepseek M-01）；:620 位点枚举（deepseek M-03）；grok m-01~m-04（gate 3 括注漏 A11、五子域标题 vs 七步、⑪(e) 钉形态覆盖不足、cause 描述漏 EXIT 后子窗）。

**轨迹：12→8→4→3→5→3→2→1→2。** 回升 1——全部集中在 r19-P1 的 cutoff 变体（主会话在 grok 四点方案之外自作主张的部分）+ 两条旧路由未随新求值序改写（传播缺口老病）。grok 方案本体（五前件/重建规则/联合形态）三席全部确认有效（格 1 complete pass、格 2 (e) fail 落地成立）。

**裁定①四裁：方向维持、落地仍 fail（格 3 两条出路均未闭成总函数）。**

### sol 终稿补充（M 核对，与 grok/deepseek 收敛确认）

sol 补充三点 M 核对全部与 grok/deepseek 收敛：M1 预算归属（= deepseek M-01）、M2 相邻语句偷换前提（= grok M-02 = deepseek M-02）、M3 :620 位点枚举（= deepseek M-03）；另登记 A11 六字段字面两读（= grok M-01——drain_end 不来自 poll-return 五 raw、不在 DW_RETURN，意图可实现但字面应分开）。sol 的 B 层结论（三点）已在中期登记中核实并入终账第 1 条。**第二十轮三席终账完整：deepseek fail 1B/3M、grok fail 2B/2M/4m、sol fail ≥3B（三点同根）——去重 2B。**

### sol 正式终稿（绑定 f321c77，fail 3B/4M/0m）

sol 终稿正式定级 3B（= 中间三点独立定级）+ 4M。**关键增量：八格等价性对照表**——r19 cutoff 方案与 sol cut-state 方案在 `JT×F×R` 八格中**不等价**：不等价格正是 JT=1,F=0,R=1（cutoff 进 1000ms 盒、cut-state 立即按本地 F=0 落 `worker-output-incomplete-at-cut`）；JT=1,F=0,R=0 的 cause 字面也不同。cut-state 只依赖 P12 本地 F、不把 host capture R 喂给 probe class——**挑战结论成立**（裁定①六裁：继续挑战；「box/cut-state 现裁：挑战 box 变体，cut-state 更根本——消除未来事件等待与 P12 对 R 的依赖」）。

sol 明确：本票不构成 cut-state 的预批准（其正式字面仍需独立审查）。

## 九之十一、第二十一轮审查（r20 版 @ bfa112e）

| 席 | 结论 | 计数 |
|---|---|---|
| deepseek-v4-pro（14 格 + sticky 循环性 + 蕴含链） | **pass** | 0 B / 3 M |
| grok-4.6 | 在跑 | — |
| sol | 中间 ≥2B（最终稿在跑） | — |

**deepseek pass 票 + sol 中间预警的分歧聚焦在两个格：**
1. **sticky 循环性**：deepseek 判 M（有界循环——RETURN 在 capture 是独立闸、无 verdict 级后果）；sol 预警 B（自认证——runner 用被检者的输出做输入）。两者对事实（第三前件 = POST cause 存在性）的认定一致，分歧在等级。
2. **SIG=1∧R=0 格**（sol 独有——deepseek 蕴含链验证只走了 SIG=0⟹R=0 方向，未覆盖 SIG=1∧R=0 可达）：SIG 置于 RETURN 发射**前**——worker 写完快照+置 SIG 后、emit RETURN **前**被永久抢占 = **合法调度**（SIG=1, R=0, FLAG=0）。P12 走竞态窗→盒到期→flag-race；runner R=0→sticky 不命中→「正常重建」但**无 RETURN raw 可重建**。`:713` 残余格登记的蕴含方向**错误**（「信号⟹快照已写⟹RETURN 应已发射」——SIG 先于发射不蕴含发射必然发生）。该格是合法平台调度（宁缺勿误适用）却落 fail——**B 级成立**（sol 预警确认，主会话验证蕴含方向）。
3. `:646` R 分岔旧句未同步纯本地化入口（sol 第三点 = deepseek M-03 收敛）。

**deepseek 的贡献**（即使 pass 票被 sol 的 B 推翻）：14 格表 + 三蕴含链（SIG=0⟹R=0 / FLAG=1⟹SIG=1 / JT=0⟹FLAG=1）的验证不依赖 capture 保序（只依赖程序序+原子一致性）——这些结论对 SIG=0 侧成立且不可被 sol 的 SIG=1 反例推翻；M-02/M-03 与 sol 收敛。

### grok 终稿（fail 2B/4M/4m）+ 第二十一轮终账

**三席齐：deepseek pass 0B/3M、grok fail 2B/4M/4m、sol 终稿在跑（中间 ≥2B）——grok 与 sol 的核心发现双向收敛。**

**去重 2 blocker（全部核实成立）：**
1. **SIG=1∧R=0 本征窗**（grok B-01 = sol 预警第二点，构造完全同构）：SIG 置于 RETURN 发射前——worker 置 SIG 后卡在 emit RETURN（HiLog 背压/调度抢占）= 合法调度。盒到期 → P12 落 flag-race → POST 即停 → R=0 永久 → sticky 第三合取（RETURN 在）不成立 → runner 重建 poll-never vs POST flag-race → F8(2) **fail**。`:708`「HiLog 背压超盒 complete pass」与 `:713`「预期 fail」**同一物理迹互斥**。`:713` 的蕴含方向（「信号⟹RETURN 应已发射」）被 worker 冻结序证伪——S 只蕴含「下一步是 emit RETURN」不蕴含「已发射/已入 capture」。**deepseek 的 14 格蕴含链只走了 SIG=0 方向；SIG=1 方向的可达格三席中只有 grok/sol 覆盖。**
2. **:646/:703 仍以 R 为 P12 活路由**（grok B-02 = sol 预警第三点）：r20 本地化入口只写了 :693/:707，:646 的「主线程 R 前件分岔（capture 为准）」与 :703 的「前件 JT∧F=0∧R=1」仍并列活着——sol B-02「P12 不判 R」**未完全落地**（回归风险）。

grok M-01（信号原子无规格化——标识符/初值/sticky/release-acquire 全缺，可见性问题可烧 (d) complete——与 r18 快照门缺发布边界同族）；M-02（sticky 签名两读：「标志仍未置」vs「cause 在 POST」——runner 看不到 worker 标志）；M-03（域扩传播不全：poll raw 域/路径①清单/:831 加法仍 19）；M-04（上界论证再偷换——S 入口后剩余是 RETURN+EXIT+标志三步）。

**轨迹：12→8→4→3→5→3→2→1→2→2。** r20 修复了第二十轮的 2B（sticky 例外闭了 R=1 盒到期的假一致、:640 分岔闭了一律截走），但 SIG 分岔门**把入口从 R 前移到 RETURN 之前**——新开的 S=1∧R=0 窗没有单一 verdict。**修复面即新审查面**的规律在第二十/二十一轮连续兑现——但每轮新缝的根因在急剧收窄：从 r14 的归因停机架构 → r18 的 D6b fd 身份 → r20 的 sticky 循环 → r21 的一个信号位置。**裁定①五裁：方向维持、落地仍 fail。**

### sol 正式终稿（fail 3B/2M/0m）——与 grok 双向收敛 + sticky 循环 B 级定论

sol 正式定级 3B（sticky 自认证循环 / SIG 蕴含方向 / :646 R 活路由）——其中 B-02/B-03 与 grok 完全同构；**B-01（sticky 循环）为三席分歧点：deepseek 判 M（有界循环无 verdict 后果）、grok 判 M-02（签名两读）、sol 判 B（fail-open——P12 错写 flag-race 时 runner 照抄 → 完整性错误可 pass，与 :653/:1066/:1462 的「P12 错写须 F8 fail」冲突）。主会话核实：sol 的 fail-open 构造成立（盒内置位却写 cause 的探针缺陷——正是 F8(2) 该抓的「探针派生值与 runner 重建值不一致」——但 sticky 使 runner 跟随 P12 的错误值 → 比对一致 → 漏检）。deepseek 的「无 verdict 级后果」论断只覆盖「诚实 P12」威胁模型，未覆盖探针逻辑 bug。B 级成立。**

sol 八格等价性表（第二十轮表 + S 隐藏维扩展）：关键发现——**SIG 维把 JT=1,F=0,R=0 格拆成 pass（S=0）/fail（S=1∧抢占）**——八格不再是总函数。裁定①七裁：继续挑战。裁定②：box+本地信号变体不构成 cut-state 等价替代。

**第二十一轮终账（三席齐）：deepseek pass 0B/3M、grok fail 2B/4M/4m、sol fail 3B/2M/0m——去重 3 blocker：① SIG=1∧R=0 本征窗（三席收敛）；② sticky 自认证循环 fail-open（sol B + grok/deepseek M——主会话裁决 B）；③ :646/:703 R 活路由残留（三席收敛）。**

## 九之十二、第二十二轮审查（r21 版 @ 8763260）——中期

| 席 | 结论 | 计数 |
|---|---|---|
| deepseek | 在跑 | — |
| grok | 在跑 | — |
| sol | 中间 ≥2B（终稿在跑） | — |

**sol 中间预警（全部经主会话核实）：**
1. **RACEWIN 同源自认证边界**（B 候选）：RACEWIN 与 POST class 同出 P12 的一个 F 分支——分支级缺陷（F=1 却同支错发 RACEWIN+写 cause / F=0 漏发 RACEWIN+写 13 类）使两者一致地错，F8(2) 抓不住（r21 的去循环只覆盖值级缺陷）。主会话识别修法方向：runner 侧签名加 capture 可见的 EXIT 交叉检验（F=1⟹EXIT 在 capture、F=0⟹EXIT 缺——「RACEWIN∧EXIT」与「POST 13 类∧JT∧EXIT 缺」两矛盾组合均 capture 可见）。
2. **`:713` 残留活句**（B 候选，第二十轮 B-03 原位残留）：「(d) 的机器门是「标志未置」、capture 前件在 P12 检验」以现行结论语气与 r21「P12 不判 R」冲突，未标废止。
3. **`:719` A11 误引**（M）：SIG 单调性声称「由 A11 背书」，A11 实际只核 poll raw 同源两写、不含两个原子信号。

### sol 第二条构造（HiLog 同通道阻塞）——主会话预裁定记录

sol 构造核实：worker 卡在 emit RETURN（通道背压）→ P12 的 RACEWIN/POST 发射在同一阻塞通道上 → 最坏永久阻塞 → 525 s 窗到期、PRE 在、进程活、POST 缺 → F9 fail。`:718`「即使 HiLog 背压……不构成 fail 面」为过度声明。

**主会话预裁定（供终账对质）**：该格的 F9 落点**不是 r21 新挖的洞**——r21 之前该形态（通道阻塞主线程 POST 发射）同样落 F9（POST 一直在同一通道上）；F9 的预注册措辞「存活未完成不是平台事实」本就宣告了该形态为 fail。RACEWIN 只是给同一通道**多发一枚 marker**，没有改变 F9 落点的存在性。**真正的问题是 `:718` 的字面过度声明**（「不构成 fail 面」与 F9 矛盾）——属措辞修正（M 级）而非结构洞（B 级）。若三席终稿以 B 定级、并给出「r21 前该形态不落 F9 而落 pass」的反例，则推翻本预裁定。

### sol 第三条构造（(d) 支 TOCTOU）——核实成立，B 级

**构造**（全部合法调度、实现完全遵守 r21）：JT=1 → P12 读 S=0/F=0 → 决定 poll-never-returned → **主线程在 POST 发射前被抢占** → worker 此时完成（写快照→S=1→emit RETURN→EXIT→F）全部入 capture → 主线程恢复、按**已做的决定**写 POST poll-never → capture 有 RETURN/EXIT、POST 说 poll-never → runner 重建 13 类 → F8(2) fail。烧掉合法平台调度。

**根因**：`:702-703`「S=0 ⟹ RETURN 必未发射 ⟹ capture 无 RETURN」是**点时读取对未来状态的非法全称**——seq_cst 排除读到未来值/回退，但不把 false load 变成对未来的承诺。决策时点与 POST 发射时点之间的窗口内 worker 可完成。**这是与竞态窗同族的第三个 TOCTOU 格**（r19 格 3、r20 SIG 窗、本轮 (d) 支窗）——共同根因：**P12 的点时本地观测与 runner 的最终 capture 重建之间的任何比较，在决策与 POST 发射之间的无界窗口上本质是竞态的**。等待只能缩窗不能关窗。

**方向定论**：r20/r21/r22 连续三轮的 blocker 全部落在这个边界族上；cut-state 方案（POST 记录 cut 状态、runner 比较按 cut 而非终态）历经每轮挑战后仍然是从根上消除该族的唯一方案。第二十二轮修复应正面采纳 cut-state 或其等价物（cut-aware 比对）。

### grok 终稿（fail 1B/5M/3m）——第二十二轮

**B-01（RACEWIN 相对 POST 的发射序未冻结）**：盒到期支写「发 RACEWIN ∧ 落 cause」但**没有冻结 `RACEWIN → POST` 的顺序**——`:959` 冻结全序 P12 动作只有「发射 POST」（RACEWIN 不在全序）、HilogStream「POST 出现即停」。最坏合法实现先发 POST 再发 RACEWIN → RACEWIN 被即停截掉 → sticky 不命中 → T'（SIG=1∧R=0 本征窗）与 r20 R=1 第三格**双双 fail**（本征窗复活 + r20 sticky 刚闭的假一致重开）。**修法最小集：冻结 P12 序 `发射 RACEWIN → 落 cause → 发射 POST`、同步 :959/:1492、selftest 钉 RACEWIN 行次先于 POST、补 T'（无 RETURN）complete pass 夹具。**

grok 对 sol B-01（sticky 去循环）的落地判定：**已落地**（签名读 RACEWIN 存在性、独立错写双方向可被抓）——「在 RACEWIN 实际进入 capture 的前提下循环已破；B-01 使该前提对 Live 非永真，sol 修法的运行时效力被同一洞掏空」。B-02（R 活路由）确认落地（`:654` 本地分岔唯一、无一律 (d) 活句）。

grok M-01（F8 轴号错挂 F8(3)/F8(2) + A11 伪背书——与 sol M 收敛）；M-02（`:721` 仍用「RETURN 在」刻画 flag-race——r19 形态说明未删净）；M-03（行号全面漂移——5 处活引用指向错误活句）；M-04（内存序双规格：seq_cst 与 release/acquire 并存）；M-05（T' 无独立夹具）。裁定①六裁：方向维持、落地仍 fail（新声明第一处时序缝）。

### sol 正式终稿（fail 4B/3M/1m）——第二十二轮

4B 全部经主会话核实：B-01 (d) 支 TOCTOU（S=0 点时读对未来的非法全称）；B-02 HiLog 同通道阻塞（F9 落点 + :718 过度声明——主会话预裁定 M 级、sol 维持 B 级，**分歧待 deepseek/终账裁**：sol 补充论据「A4 的零无界阻塞调用保证未覆盖该 emit」）；B-03 RACEWIN 同源分支联错（EXIT 交叉检验为主会话识别的修法方向）；B-04 :713 残留活句。

**grok vs sol 的 RACEWIN 视角互补**：grok B-01（POST-first 实现自由度：顺序未冻结 → 即停截掉）与 sol B-02（通道阻塞：RACEWIN 到不了 POST）——修法同向（冻结 `发射 RACEWIN → 落 cause → 发射 POST` 序 + :718 按 F9 预注册口径修正）。sol 明确指出顺序流中 POST 即停不会反向截掉已消费 marker——grok 的截断只发生在 POST-first 实现，冻结序即消除。

sol 八格+S 维表：JT=1,F=0,R=1,**S=0** 可达（TOCTOU 洞格）。裁定①八裁：继续挑战。裁定②：r21 变体仍不构成 cut-state 等价替代（`JT=1,F_cut=0,S_cut=0,R_final=1` 在 cut-state 下可 pass、r21 却 fail——**这正是 cut-state 方案的决胜格**）。

## 九之十三、第二十二轮终账（三席齐：grok 1B/5M/3m、sol 4B/3M/1m、deepseek 1B/3M）

**去重 5 blocker（+1 项主会话 M 裁定带分歧记录）：**
1. **(d) 支 TOCTOU**（sol B-01）：S=0 点时读对未来的非法全称——决策与 POST 发射之间 worker 可完成，`JT=1,F_cut=0,S_cut=0,R_final=1` 可达 → F8(2) 烧合法调度。**cut-state 的决胜格**。
2. **RACEWIN 发射序未冻结**（grok B-01；deepseek 判定「结构上已蕴含但未字面冻结」——实现自由度真实存在）：冻结 P12 序 `发射 RACEWIN → 落 cause → 发射 POST` + 同步 :959/:1492 + selftest 钉行次。
3. **RACEWIN 同源分支联错**（sol B-03）：值级缺陷可抓、分支级缺陷（marker+class 一致地错）F8(2) 抓不住——「只消除了对 POST 的直接读取、未建立信任边界上的独立性」。
4. **`:713` 残留活句**（sol B-04）：「capture 前件在 P12 检验」以现行结论语气与本地入口冲突，未标废止。
5. **F vs SIG 措辞互置**（deepseek B-01）：`:717`/`:661` 把「区分窗口关闭原因」错挂 SIG 再读（实为 FLAG 分支检查）——与 `:719` F8(3) 支表面对立。
6. **HiLog 同通道阻塞**（sol B-02；**主会话裁定 M 级、sol 维持 B 级——分歧记录**）：F9 落点在 r21 前就存在（POST 一直在该通道）、F9 预注册措辞本宣告该形态为 fail；真正问题是 `:718`「不构成 fail 面」的过度声明。修法同向（按 F9 口径修正声明）。

**三席对 r21 修复的确认**：sol B-01（去循环值级）格 9/10 验收通过（deepseek 独立复验）；sol B-02（R 活路由）落地；两窗（SIG=1∧R∈{0,1} 盒到期）在 RACEWIN 成功发射下 pass；B-04/SIG 窗口收口意图闭环。**:713 残留是第二十轮 B-02 的原位残留（行号平移导致漏改）。**

**轨迹：12→8→4→3→5→3→2→1→2→3→5。** r19-r22 四轮的全部 blocker 落在同一族：**P12 点时本地观测 vs runner 终态 capture 重建的比较在决策与 POST 之间的窗口上本质竞态**。裁定②（sol 四度主张）：cut-state 在决胜格 `JT=1,F_cut=0,S_cut=0,R_final=1` 上可 pass 而 r21 fail——**r22 修复正面采纳 cut-state**。

**r22 修复设计（主会话定稿）**：POST 新增 `worker_terminal_at_p12`（cut 记录）；runner 对 JT=1 格的规则改 cut-aware：(i) RACEWIN 在 ⟺ cut 记录 FLAG=false（分支一致性，违 → F8(2)）；(ii) class=13 类 ⟹ cut FLAG=true ∧ EXIT 在 capture（违 → F8(2)——抓方向 2）；(iii) class∈{poll-never, flag-race} ⟹ cut FLAG=false、**迟到 RETURN/EXIT 非矛盾**（cut 语义——关 TOCTOU）；(iv) flag-race ⟹ RACEWIN 在（违 → F8(2)）；(e) 格保持（capture 完整性轴）。冻结 P12 序 RACEWIN→cause→POST。

## 九之十四、第二十三轮审查（r22 版 @ cfda2c1）——sol 中断前预警登记

sol（c045842a）连接中断，中断前两点预警（主会话初步验证）：
1. **静态断言缺口**：A11 只保 poll raw 同源两写——无断言把 `worker_terminal_at_p12` 绑定到那一次 `dw_worker_terminal` 读取、无断言把 cut/class/RACEWIN 三输出绑定为相互独立的控制流证据（三输出联错的机器面缺口——与 grok/sol 任务书中的「分支一致性只验证内部一致性」疑点同根）。
2. **`:715` 残留冲突**：旧 (d) 补注仍写「迟到 RETURN 入 capture 则本前件失败」（r18 归因洁净补注语境）——与 r22 cut-state (B)(iii)「迟到 RETURN 非矛盾」正面相反。主会话初步分析：`:715` 的「前件」指旧 capture 前件（`:713` 已注明废除），但**句子本身以活规则语气陈述**且未标 r22 语境更新——最坏读法使决胜格双判（(d) 前件失败 vs (B)(iii) pass）。**:714 的「SIG 未置 ⟹ capture 无 RETURN」点时全称同样是 r22 已废论证的残留陈述**（r21 曾在 :724 更正过竞态窗侧、但 :714 在 (d) 支内未被同步）。sol 终稿待重派。

### sol 席三次连接失败——按「最多 2 次后接手」纪律由主会话完成其终稿判定（局限声明：非隔离采样，判定基于其中断前预警 + 主会话亲验；第二十四轮审查时 sol 席应对本判定复核）

**预警 1（A 系列断言缺口）——主会话判定 M 级**：核实 A10（fd_dup 零操作）/A11（poll raw 同源两写）均不含 cut/class/RACEWIN 三输出的独立性绑定（A 系列确无该断言）。但定级参照系：r18/r19 各轮「静态断言缺口即 B」的先例（A9/A10/A11 的诞生）针对的是**成功终态的 pass 可达性**系于无机器保证的规则（fail-open/fail-closed 方向）；三输出联写是**探针逻辑 bug 威胁模型**（分支一致性验证内部一致性而非真实性——其固有边界在 r22 设计段已声明「探针逻辑 bug 的捕获归 freeze bytes/静审/selftest、非运行时比对」），pass 效力已被 §4.2 封死。**判 M（补 A12 或 A11 扩展——静态核对 cut/class/RACEWIN 三输出与 F 分支控制流的绑定）**。若 grok/deepseek 终稿以「同源联错 B 判例」对质（r19-r22 marker+class 两输出联错判 B），则该判例的适用差异（两输出联错 = 记录器完整性威胁[marker 是事实源]；三输出联错 = 探针逻辑威胁[cut/class/RW 同为 P12 派生输出、无一为独立事实源]）需要在终账正面论证，不能默认。

**预警 2（:714/:715 残留）——主会话判定 B 级**：亲验成立。`:714`「SIG 未置 ⟹ RETURN 必未发射 ⟹ capture 无 RETURN」与 `:715`「迟到 RETURN 入 capture 则本前件失败」均以**活规则语气**陈述已被 cut-state (B)(iii) 废弃的论证（「迟到 RETURN 非矛盾」）——r21 在竞态窗侧（:724 区）更正过同类蕴含、但 :714/:715 在 (d) 支内未同步。决胜格（JT=1,S_cut=0,F_cut=0,class=poll-never,R_final=1）按 :715 读法 (d) 前件失败、按 (B)(iii) pass——**同一格两读**，r22 的核心修复被残留句架空。修法：:714/:715 加 r22 语境注（「本蕴含为点时全称、r22 已由 (B)(iii) 取代——(d) 的 P12 侧判定只依赖本地证据链，迟到 RETURN 的 capture 存在性不再是前件」）。

**三深格主会话判定（供第二十四轮挑战）**：三输出联写 = M（威胁模型划分，见预警 1）；SIG 维 (B) 缺失 = 非缝（点时 SIG=0 不推断终态——sol r22 B-01 自己的论证——(iii) 对 SIG=0 格不是错误宽免）；冻结序 ①③间死亡 = 待 grok/deepseek 覆盖（RACEWIN 在 ∧ POST 缺 → pre-only (b) 收口对 RACEWIN 的忽略——若 (b) 不认 RACEWIN 则其为 capture 孤儿 marker，无规则处理）。

### deepseek 终稿（cut-state 16 格机械枚举）——pass 0B/4M

**pass 票的核心结论**：16 格全穷尽无矛盾；格 6 (A)+(e) 双入口同挂一致；格 7 (E=0) 为 (A) 独有收口（F=1⟹EXIT 已发射）正确；**格 9 SIG 维非缝**（SIG 进程内原子、runner 无通道——(B) 靠 class+RW 校验 P12 解析连贯性——与主会话接手判定一致）；**格 13 cut 错写 false 被 (B)(iii)+「cut 是事实源」显式接受为 TOCTOU 固有不对称、格 14 cut 错写 true 正确 fail（fails closed）——宁缺勿误成立**（对主会话预警 1 的 M 判定提供独立支撑——「固有边界」在规则文本中有显式依据）；格 15 死亡窗 RACEWIN 未被忽略（sticky 签名与 POST 无关、pre-only 收口正确重建——主会话初判的「孤儿 marker」疑虑被打消）；格 16 spurious 已落。冻结序三处一致（行号偏移为任务稿滞后）。4M 均澄清级。

## 九之十五、第二十三轮终账（三席齐：deepseek pass 0B/4M、grok fail 1B/4M/4m、sol 三次失败由主会话接手[预警2 判 B]）

**去重 1 blocker（grok B-01 = 主会话接手判定的预警 2，双向收敛且 grok 深挖出兄弟轴）**：

**cut-aware 例外域不完备——(B)(iii) 只关了 class 轴**。grok 构造 T*（= sol 决胜格的全轴版）：JT=1, P12 读 S=0/F=0 走 (d) 写 poll-never + cut=false + **poll raw=unobservable + watchdog=⑤**，POST 前抢占，worker 完成（RETURN+EXIT 入 capture）——
- class 轴：(B)(iii) 迟到非矛盾 → pass ✓（r22 核心修复有效）；
- **watchdog 轴**：`:665` F8(2) 比对全部 dw_outcome 字段——P12 写 ⑤（EXIT 缺时点）、runner 见迟到 EXIT 走 ④ → 不一致 → **fail**；
- **poll raw 轴**：P12 写 unobservable、runner 从迟到 RETURN 重建数值 → 不一致 → **fail**；
- `:714`「SIG 未置⟹capture 无 RETURN」/`:715`「迟到 RETURN 入 capture 则前件失败」活语气残留（主会话预警 2 原文）。
**净结果：决胜格 fail-closed，与 selftest :1447 的 complete pass 矛盾。** r19-r22 同族 blocker 的第五次出现——「点时观测 vs 终态 capture」的比较 r22 只在 class 一轴关闭。

**grok 修法最小集**：(B) 的「不从终态 capture 重建」扩到一切会被迟到完成分叉的 dw_outcome 字段（class/poll raw/watchdog——即 cut=false 格对这些字段的 runner 侧全部走分支一致性或透传校验而非终态重建）；:714/:715 废止点时全称；路径① :894/四步 (0) :876 的「poll-never⟹R 缺」旁证同步删。

**三席对照的关键信息**：deepseek 16 格 pass 票只走 class 轴（grok 明确点名「不采信其 16 格——未走兄弟轴」）；grok 的 T* 恰好补上这一维。**单轴 vs 全轴的教训与 r16 的「同一钉两处落盘」同型——修复的传播必须在全部受影响字段上闭合。**

grok M-01（行号锚未改净）/M-02（T' 无 R=0 夹具、RACEWIN 行次无钉）/M-03（A12 缺口——与主会话预警 1 判定 M 收敛）/M-04（「双向可抓」过称——cut 不诚实时与 T* 不可分、固有不对称）；m-01（方向 1 钉误引 (B)(i)——cut=true 时 (B) 不适用）/m-02/m-03/m-04。

**轨迹：12→8→4→3→5→3→2→1→2→3→5→1。** grok 对 5B 修复的落地核对：发射序/方向 1/方向 2/:713/F-SIG/M-02/M-04 全部落地；TOCTOU「class 轴是/全轴否」。裁定①（grok 票内）：r22 方向对、新声明首轮证伪点仍是同一句。

## 九之十六、第二十四轮审查（r23 版 @ 422b27c）——sol 先回

**sol 终稿：fail 2B/0M/1m**（三席中第一个成功完成且未中断的 sol 终稿；同时完成接手判定复核）。

**B-01（`:672` 残留无例外全称——主会话派发时埋的疑点被证实）**：`:672`「dw_outcome 的全部派生字段……runner 重建比对、不一致 F8(2)」**无「除 (B)」字样、无优先级声明**（主会话 grep 亲验：该行 cut/例外引用 = 0）——T\* 格按 :672 重建 fail、按 (B) 透传 pass——**同一合法轨迹双 verdict**。九之十五的「扩全轴」未真正传播闭合（r16/r23 单轴教训第三次出现：这次是字段轴传播到了、规则位传播没到）。

**B-02（watchdog 允许集无机器定义）**：`:756` 只给示例「watchdog 应为 ⑤ 或 unobservable」——⑤ 本身就是 unobservable(cause=marker-gap-indeterminate)，字段合法域还含 ①②及 skip 的多个具名 unobservable——「watchdog 错写 unobservable(cause=no-live-fd)」宽实现接受 pass、严实现拒绝 F8——既 fail-open 又非全函数。**须逐 class 冻结精确允许集/真值表**（poll-never/flag-race 各列允许字面、其余 F8）。

**接手判定复核（sol 全部接受）**：预警 1 判 M 接受（A12 已关闭——旧判例有独立 capture raw 事实源、三输出同源 F_cut 终态无法反证——固有不对称由 A12 静态防线承担）；预警 2 判 B 接受（与 grok 同向）；三深格初判全部确认（SIG 维非缝/死亡窗非缝/三输出联错 M）。

**八格意图终表（sol）**：F_cut=0 两格 R_final 不改 verdict（100/101 均 pass——决胜格 + TOCTOU）；实际 F=1 三输出联错按记录 cut 落 pass——**明确接受的不对称**。裁定①十裁：方向维持、落地仍 fail。裁定②终裁：cut-state 语义本体维持采纳、r23 意图在八格 verdict 上等价、字面因 2B 不等价。

### deepseek 终稿——fail 1B/2M（与 sol B-01 精确收敛 + 独有溯源细节）

deepseek 的 grep 取证比 sol 更细：`:665` 区的 cut-state 字样只在 `:676` 的 class sticky 括注（仅覆盖 class 轴）；`:677`「同例外」模糊且只及 poll raw；**watchdog 轴在所有权块内零 carve-out**（`:673`/`:678` 的 EXIT 喂 ④ 语义恰是催生两读的来源）；(B) 段 `:754` **自证**「runner 见迟到 EXIT 走 ④ → :665 全字段比对 fail」却未回写 :665。M-01 与 sol B-02 同根（允许集示例化）+ 补充「flag-race 的 watchdog 允许集只字未提」「裸 unobservable 会把 skip 具名清单误纳入非 skip 格」。M-02（gate 3 括注缺 r23 纳入 A12——纯登记 nit）。其块 1 其余/块 2 与 sol 复核结论一致（join/派生无张力、EXIT 窗诚实观测、A12/selftest 一致）。
