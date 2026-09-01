# 会话交接文档（2026-09-01）：T0 决议落地 + N1BDISC 判据三轮审查

最后核验：2026-09-01

承接 [2026-08-31 交接](session-handoff-2026-08-31.md)。本次会话完成两件事：**N1b 治理决议的表决与落地**，以及 **N1BDISC 发现 campaign 判据的起草与三轮跨厂商审查**。

## 一、已完成并已提交

| 产出 | 文件 | 状态 |
| --- | --- | --- |
| N1b T0 决议 | [`native-nx-n1b-adjudication.md`](native-nx-n1b-adjudication.md) | `ADJ-T0-N1B-20260831-0001`，用户已批准 |
| 开放义务台账 | [`open-obligations-ledger.md`](open-obligations-ledger.md) | OB-01..OB-09 |
| schema 扩展 | [`evidence-schema.md`](evidence-schema.md) | N/G 门代码追认 + 发现型 campaign 记录形态 |
| T0 材料包 | [`t0-n1b-discovery-materials.md`](t0-n1b-discovery-materials.md) | 表决完成，含 8 处事实更正与 5 处偏向登记 |
| N3 法律简报 | [`n3-legal-brief.md`](n3-legal-brief.md) | 自 `/tmp` 迁入（字节未变），待用户提交外部法律意见 |

提交：`9c83c3c`（决议）、`16f3970`（法律简报）。已推送 `origin/main`。

## 二、进行中：N1BDISC 判据

**文件**：[`n1b-disc-gate-plan.md`](n1b-disc-gate-plan.md)（563 行，状态 `criteria-r2-pending-independent-review`，**未提交**）

**判据未冻结、不得测量、不得分配 AUTH/pair 或 evidence ID。**

### 审查历史

| 轮次 | grok-4.6 | gpt-5.6-sol | deepseek-v4-pro |
| --- | --- | --- | --- |
| r0 | fail 6 B | fail 10 B | pass 0 B |
| r1 | fail 5 B | fail 11 B | fail 2 B（**加了针对性提示**） |
| r2 | fail 2 B | fail 11 B | pass 0 B（**撤掉提示**） |

起草人：`zai-coding-cn/glm-5.3`（r0/r1）→ `glm-5.3-flash`（r2 四趟）。**claude 家族因主会话撰写决议而不任审查席；glm 家族因起草而占用。**

### r2 的四趟修订（共 33 项）

F1–F11 事实常量 → A1–A6 D-W 集群 → B1–B7 verdict 集群 → C1–C9 冻结项与生命周期。四趟由四个独立执行过程顺序完成，**趟间接缝是 r2 新缺陷的主要来源**。

## 三、r3 待修清单（三席合并，按优先级）

### 致命（两席独立收敛，主会话已核实）

1. **死因分类优先级使成功路径必 fail**（grok B1 = sol B5）。全序 `probe-fault > unattributed > platform-termination`，而 `unattributed` 的条件（`:vpn` absent + capture 静默 + 无新增 faultlogger）**恰好被 P9 destroy 后的正常静默退出满足**，于是优先级 3 的「预期终态必须可 pass」永远轮不到。修法：改序为 `probe-fault > platform-termination > unattributed`，或从 `unattributed` 显式排除「已发出 `DW_DESTROY_T`」。**同时**解决 C8 回退（「不得因此判 fail」）与 `:379`（「无死亡证据且 RESULT 缺 → fail」）的正面冲突——只能留一条出口。

2. **APPFREEZE 位点约束过粗**（grok B2；**主会话规格错误**）。r2 规格把 D1/D2/D4/D5/D8 一律列为「短时步」，但 **P2 每条 create 有 60 s 盒、P7 storm 有 10 s**。平台在合法长 create 上触发 watchdog 会被判 probe-fault → fail，违反「平台行为永远不是 fail」。修法：短时步集合只保留真正短步；P2/P7 要么纳入暴露窗，要么 APPFREEZE 只登记观察、不 fail。

### 跨趟接缝（两席收敛）

3. **路径① 条件 4 语义漂移**（grok M1 = sol B2）。A1 趟写「drain 已到 EAGAIN（`dw_drain_timeout == false`）」，C6 趟把 drain 定义成五类返回，`zero-read` 与 `errno-N` 同样使该布尔为 false。修法：钉死为 `dw_drain_end == eagain`，`fd-event-like` 同步收紧。

4. **`destroy` 从未执行时派生字段不可求值**（grok M2 = sol B4）。`barrier-never-observed` 分支已预注册（`:246`），但 `dw_return_class` 第 4/5/7/8 类依赖 `destroy_call_mono_ms`，B6 的「穷尽」枚举没有这一输入。修法：新增 `destroy-never-called` 类或固定 cause，不归因、不解锁路径①。

5. **A4 静态断言与 A5(b) join 裁定冲突**（grok M5 = sol B1）。A4 写「零无界阻塞」，A5(b) 又给 `pthread_join` 开例外。修法：A4 改为「除 A5(b) 已登记的 join-blocked 外零无界」。

### 单席但成立

6. **poll 合同域未校验**（sol B3）：`ret=0 + POLLHUP` 会命中 `fd-event-like`。分类前加合法域门（单 fd 时 `ret∈{-1,0,1}`，`ret=0` 必空 revents，`ret=1` 必非空），矛盾输入 fail + raw。
7. **post-mortem 在实现上不可达**（sol B9）：RESULT 必须带 fd-ledger digest，但无增量 ledger marker、无 canonical 序列化，平台终止时进程来不及发、runner 无法合法代算。修法：冻结 canonical 编码 + 每次 create/close 发增量 transition marker。
8. **chunk 无逻辑键**（sol B8）：只有 index/count/hash/payload，多条 detail 无法唯一重组。修法：加冻结的 `stream=<literal>|item=<id>`。
9. **U3 枚举无优先级**（sol B10）：同一帧可同时命中 `tun_pi-like`/`no-prefix`/`ambiguous`；`off=both` 时长度关系用哪个 offset 未定。
10. **D7 `elapsed` 区间不互斥**（grok m3 = sol B11）：`>=20000` 判 true 与 `>25000` 判异常重叠；正文与自陈边界也不一致。**平台调度延迟不得触发 fail。**
11. **readiness 仍在 `StartEntry` 之后求值**（sol B7）：B3 裁定 (a) 的文字改了，但计时锚点没动，Live 已开始却仍称「未消费」。
12. **finally 缺 `PidOfVpn` 采样步**（grok M4）：死因分类以 PidOfVpn 为输入，但 finally 序列在 ForceStop 前没有该步。
13. **criteria-gap 兜底仍可洗白**（grok M3）：删除 criteria-gap 作为运行时 unobservable cause。

### 机械

14. `:208` 引 `native-nx-n1b-adjudication.md:66` → 应为 **`:65`**（**主会话错误**：r2 规格照搬了 grok r1 轮的错误更正，把原本正确的 `:65` 改错）。
15. 自陈 6(g) 已被 C5 取代（argv 已逐字入正文），文字未回写。
16. 观测窗推导把并发的 drain（worker）与 barrier 等待（主线程）串行相加，上界应为 **467** 非 472；冻结值 525 不变，方向保守不影响安全。

## 四、主会话在本轮犯的错（供后续校准）

| # | 错误 | 谁抓到 |
| --- | --- | --- |
| 1 | 路径① 三条件缺「destroy 可与自身 timeout 区分」，因果缺口 | 三席全中 |
| 2 | `{73, 414}` 基于未核实前提加严（414 只在 32-bit 块内） | 两席 |
| 3 | 差点越权砍掉 D-W（决议 §三.3 写死为强制项，§二.10 要求回 T0） | 自查决议时抓到 |
| 4 | 照搬 grok 的 `:66` 错误更正，把正确引用改错 | deepseek + grok 后续自纠 |
| 5 | APPFREEZE 位点约束过粗，把 60 s create 当短时步 | grok |

**共同形状**：1、4、5 都发生在把大量审查发现压缩成单份规格时——按「审查席说了什么」转写，而未对每条独立判断边界条件。**判断密度超阈值则质量下降，对主会话与子代理同样成立。**

## 五、流程约束（重要，不重新发现一遍）

- **派发规模**：`glm-5.3` 在整体重写规格上**两次空产出**；`glm-5.3-flash` 在 6–11 项边界清晰的子任务上**四战四胜**。按「可拆成多少边界清晰的子项」定规模，不按任务复杂度选高档模型。
- **评审席占用**：能改稿的执行层中，`deepseek-v4-flash` / `gpt-5.6-terra` / `grok-4.6` 家族**全部占着本文档的评审席**，让它们改稿即损失一个跨厂商席位。可用的只有 glm 家族（起草人家族，已占用）。
- **deepseek 席需要校准**：三轮里它两次判 0 blocker、一次（加了针对性提示后）判 2 blocker 并给出硬发现。**r3 必须恢复该提示**——提示词的形式可比性不如对已知系统性偏差的校准重要。
- **不得中途改稿**：审查席读的是当前字节，修订必须等三席全部归队。

## 六、待决策

1. **A/B：是否砍掉 D-W**。D-W 拿关键路径事实（U4，驱动 C10 三分支出口）去赌一个已有预授权替代去向的 OB-03，且是两轮 blocker 主产地。但决议 §三.3 把 D-W 写成**强制项**，§二.10 要求范围变更**回 T0**——**主会话无权自行决定**。建议：等 r3 审查结果，若 D-W 仍是 blocker 主产地则以实测证据提 T0。
2. **N3 法律评估**：简报已就位，**只有用户能做**，与本条线完全并行，越早启动越从容。
3. **物理执行授权**：判据冻结且审查 0 blocker 后，须用户显式授予新 AUTH。决议已写死拆分**保证消耗两个** AUTH/pair 与两个 evidence ID。

## 七、下一步

1. r3 修订（上述 16 项，**拆两趟**派 `glm-5.3-flash`）
2. 主会话自查接缝
3. 第四轮三席审查（**恢复 deepseek 校准提示**）
4. 0 blocker 后方可请用户授权
