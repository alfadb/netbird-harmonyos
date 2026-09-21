# N2-H iface 字节 API 窗口两沿配对读取（2026-09-20，diag-only）

## 规范依据（v4 §4 前置②(ii)）

- v4 生效稿 §4 前置②(ii)「同源标定未完成」：两侧读数从未同刻重叠（API 打点早于
  /proc 3.5–4.1 s > 单帧周期 ~1 s），每轮数百字节间隙无独立解释；判据要求**一次
  同刻对**：两侧间隔 <1 s（最好 API 在 /proc 之后）。0028 实测单次 ARM 读数与紧随
  /proc 读数相隔 3.4 s、差值恰各一帧——形态同源但**不满足 <1 s**，故单沿不可用，
  需窗口两沿配对。本实现只落地"配对读数面"；同源标定判据本身（A→B 注入已知
  n×80 B 单向帧）仍需真机实验。
- v4 §4 影响与边界 + §5-4：statistics 读数**只用于 C7 计数对账**，不作路径/隔离
  正证据；本实现不写入 `Evidence.counters/notes`、不进 judge、不派生包数。
  §6.2-4 的判据侧采集落点未落盘：同源标定未完成前该窗口 C7 仍按现行缺口口径。

## 两沿定义与"下一次 watcher tick"的理由

- **开沿**：沿用既有 ARM 成功分支（`N2H_COLLECTOR_ARMED` 之后）对 `vpn-tun` 的既
  有读取，既有 `N2H_IFACE_BYTES_PROBE` 行逐字不变；**同一个 `IfaceSample`** 再渲染
  一行 `N2H_IFACE_BYTES_WINDOW|phase=open`（不二次读数）。
- **关沿**：扩展在**既有 status watcher 的下一拍**（ARM 之后第一拍）对 `vpn-tun` 做
  一次新读取，同形行 `phase=close`。
- 字段（两行同形）：`requestId|phase|interface|rxBytes|txBytes|rxError|txError|`
  `pairRequestId|startedUnixMs|finishedUnixMs|diag=true|is_evidence=false`；
  `pairRequestId`＝同一 requestId，即两次读数的配对标识；字节缺失记 `unavailable`，
  错误只记 BusinessError code（`none`/`no-code`/`invalid-value`）；epoch-ms 走
  `%{public}s`。**不打印异常 message、不打印 payload**。
- 为什么"下一次 watcher tick"：**不新增定时器/循环**——开沿在 async probe 内发出，
  其 open-latch 回调只在 `phase=open` 行之后置位，此刻 arm 拍同步体已退栈，故关沿
  结构上不可能落在 arm 拍；关沿挂在既有 `maybeArmN2hCollector` 函数开头（一次性早
  返回之前），复用其 tick 可达性，后续 tick 仍会到达。

## 一次性与失败语义

- `n2hIfaceWindowOpenSeen` / `n2hIfaceWindowCloseDone` 两个 boolean：关沿在 issue 时
  即消费，之后任何 tick 不再读、不再打行；**未获开沿（ARM 未成功）则零关沿**。
- 读取 fire-and-forget：arm 拍不加 await、不阻塞 arm、不改变 ARM/latch/WAITING 语义；
  API 拒绝只记方向级 code + `unavailable`，last-resort catch 兜底（只打 code/name），
  不影响既有 probe/诊断流程。
- 治理：两行 `is_evidence=false`，只作 C7 对账输入，不写 `Evidence.counters/notes`、
  不进 judge。

## 自检与工具链证据

- `run-exclusion-generation.mts` 新增 W1-W6：开沿/关沿各恰一次且 requestId/pair 相同；
  开沿与 PROBE 行是**同一次采样**（数值逐字相同、无二次读）；关沿在下一拍且
  `close.started >= open.finished`、恰两次 vpn-tun 调用；拒绝时 unavailable+code 且
  流程继续、后续 tick 静默；三种未 ARM 成功形态零关沿；源码 pin（窗口路径无 timer/
  loop、无 await、关沿 hook 在早返回之前、读取仅在 open latch 之后、latch 回调只写
  一个 boolean 且在 ok 分支内）。P6 同步（首拍=关沿，再拍静默）。
- 七套 shell 自检真实运行：run 81 / generation 143 / addr-shape 25 / relay-exclusion 49
  / relay-flag 9 / relay-status-log 3 / route-validation 49，共 **359 passed / 0 failed**。
- 两改动 ETS（NetBirdInterfaceStatsProbe / NetBirdVpnExtensionAbility）codelinter
  6.0.240：`No defects found in your code.`（exit 0）。

## 真机待验点

- 关沿是否真落在 ARM 后第一拍：需真机日志按 `pairRequestId` 配对 open/close 两行，
  并与同窗口 /proc 读数比对间隔是否 <1 s（同源标定判据另见 §4 前置②(ii)，须注入已知
  n×80 B 单向帧）。
- 若 ARM 后 status poll 先遇 terminal/recreate（该路径在 arm hook 之前 return），关沿
  不触发，只有开沿——真机须如实记录该缺沿形态，不得以开沿单点代替配对。
- 两沿只解决"同刻"问题；statistics 与 /proc 的计数基/刷新周期差异不因两沿而消失，
  同源结论仍以真机同刻对判据为准。

## 构建状态

- 按协调未跑 HAP build（等待并行 core 改动合并后由合并验收构建统一执行）；无设备
  操作、无 commit；core 侧未提交改动（DNS 回执接线等）未被触碰。
