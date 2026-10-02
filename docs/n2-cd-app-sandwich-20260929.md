# N2 第一批 C/D 列：应用侧切网夹心 + 应用内证据环形文件（2026-09-29）

> **诊断实现文档，不是设备授权，也不是任何门结论。** 本文描述的产品侧机制全部默认
> 关闭、仅在已接受 `n2_evidence` 票据的武装会话内生效，每条输出行都以
> `|diag=true|is_evidence=false` 结尾。它**不**签发 AUTH、**不**授权连接设备、抓包、
> 改管理端或修改服务器，也**没有**做过真机测量。正式执行仍须按
> [`n2-device-evidence-preregistration-draft-v2-20260928.md`](n2-device-evidence-preregistration-draft-v2-20260928.md)
> （冻结 v4.1）§6 分别签发两份 AUTH 并经用户回读确认，再按 §7 T7a 做工具/配置独立
> 符合性审查。**冻结文本仍是权威**；本文中的任何措辞都不改变冻结判据、阈值、oracle
> 或适用集。
>
> 本版对齐 2026-09-29 的**重写实现**：编排按
> [`N2-CD-RING-T0-REVIEW.md`](../../../n2h-kit/status-20260927/N2-CD-RING-T0-REVIEW.md)
> §3「最小修正」第 1–7 条落地（切网后门控重馈 + 新网络前控、D 列 prepare/ready、
> 回执改身份判据、Wb 补控件、证据环持久 seq、承载同句柄实测）。该实现**尚未**经过
> 新一版 T0 复核，AUTH 不得据本文签发。
>
> 2026-09-30 增补（同上审查 §3 第 1 条的收口，不改变任何判据）：**正式切换的生产恢复门**
> 从「只跳过本次直接重馈」扩成真正的门——它绑定计划 + 切换 epoch，覆盖**所有**恢复/新连接
> 供给入口（含在途恢复换 epoch 后的追逐、DNS/管理即席供给、ICE 补给与 recreate-finally
> 重试），核心 poll 命名该计划与该 epoch 回答 `step=recover` 是**唯一**放行点；正式切换落
> `unsat` 后门进入 `held`，直到本次取证被显式终止。完整生命周期见 §3.4，验证记录见 §7，
> 剩余契约缺口（进程内无「中途解除取证」通道）见 §3.4 末条。

## 1. 问题

第一批 C 列（Wi‑Fi → 蜂窝）与 D 列（蜂窝 → Wi‑Fi）有同一个结构性障碍：

* 主机经 Wi‑Fi（hdc 目标 `192.168.50.199`）控制手机；
* 一旦设备切到蜂窝，主机**没有任何实时通道**下发命令（`hdc` 链路在 Wi‑Fi 上），
  也无法实时导出 hilog——蜂窝阶段 HiLog 缓冲可能轮转或流控丢行，丢的可能是 `N2_*`
  证据行甚至泄漏告警本身；
* 而冻结 §3 要求**每个生产事件**都有「前控 → 生产包 → 后控」，且两个正控都必须在
  vpn-tun `service_tun` 读循环的 from-OS tee 命中（核心吞掉、不外发），并由
  `N2_TUN_TEE_STATS` / `N2_TUN_GAP` 证明该区间覆盖连续。

用户已决定：**由应用自己在蜂窝阶段夹对照包**，日志留在设备上，回到 Wi‑Fi 后主机统一
取回。本文即该决定的实现设计。

## 2. 机制总览

三条互相独立的机制，都是「默认关闭、仅武装时启用、未武装零行为」：

| 机制 | 私有通道 | 谁执行 | 产出 |
| --- | --- | --- | --- |
| **切网夹心计划** | `--ps n2_cd_plan '{"column":"C"\|"D","kind":"M\|S\|R\|D\|I\|Wb\|Ws","rep":n,"id":"…"}'` | 应用（已武装、已 attach 的会话） | `N2_CD_SANDWICH` 逐阶段行 + 壳侧 `N2_CD_SANDWICH_SHELL` 行；控制 marker 仍由核心 `N2_CONTROL_SENT/HIT/MISS` 记；生产回执仍由 `N2_SOCK` 记 |
| **应用内证据环形文件** | （无通道；武装时自动开启） | 核心（`hilog::emit` 的 tee，`client/core/src/hilog.rs:64`）+ 壳侧转发 | 应用私有目录下的一个环形文件，逐字复制 `N2_*` / `N7_WG_FEED` / `VPN_NETWORK_*` / `VPN_WG_DATAPLANE_FED` / `N2_TUN_*` 行；每条记录带 `N2R\|sess=…\|seq=…`（`n2_ring.rs:1110-1113`），配 `N2_RING_*` 计数行 |
| **环形文件取回（兜底）** | `--ps n2_ring_dump '{"from_seq":0,"to_seq":0,"count":64,"session":"<arm token>"}'` | 应用 | 先要 `N2_RING_MANIFEST`，再把**按持久 seq 分页**的有界窗口重发到 hilog（`N2_RING_CHUNK` 逐条 + `N2_RING_DUMP` 汇总）；**不推进 ACK** |
| **显式主机 ACK** | `--ps n2_ring_ack '{"session":"<arm token>","up_to_seq":<已持久化的最大持久 seq>}'` | 应用（转发）+ 核心（绑定当前 arm 后判定） | 核心把该 arm 的水位推进到 `up_to_seq`，壳侧 `N2_RING_ACK_SHELL` 行；**只有它**能放行轮转删除 |

四者都复用既有的非机密 want 参数机制：`EntryAbility` 把 payload **逐字**写到
`<filesDir>/<basename>`（文件名 + UTF‑8 字节数入日志，payload 从不落日志），已运行的
扩展在下一个 watcher tick（≤ `STATUS_POLL_MS` = 5 s）读取并**恰好消费一次**（读后即
删）。「一次启动 = 一个待处理请求」的组合规则现在覆盖**六个**通道
（`n2_stimulus`、`n2_control`、`n2_a_event`、`n2_cd_plan`、`n2_ring_dump`、
`n2_ring_ack`）：一次启动带多于一个就**一个都不写**并清掉全部残留。

桥导出清单（`client/core/src/napi.rs:138-152`）：

```
n2_cd_plan(8 参)  n2_cd_prepare  n2_cd_ready  n2_cd_pre
n2_cd_switch_identified  n2_cd_identity  n2_cd_poll  n2_cd_post
n2_ring_configure  n2_ring_append  n2_ring_disable  n2_ring_status
n2_ring_dump  n2_ring_manifest  n2_ring_ack
```

旧的五参 `n2_cd_switch` 桥导出**已删除**（`n2_cd.rs:687-689` 仅保留一个 fail-closed
的 Rust 内部函数，且没有任何桥/C 调用点会走到它）。

## 3. 切网夹心计划的生命周期

主机在**还有实时链路时**（Wi‑Fi 阶段）下发计划：

```bash
aa start -b cn.alfadb.netbird -a EntryAbility \
  --ps n2h_arm_options '<the same arm ticket that armed this session>' \
  --ps n2_cd_plan '{"column":"C","kind":"M","rep":1,"id":"cd-C-M-1"}'
```

应用随后**独自**完成（每个阶段一行 `N2_CD_SANDWICH`；受理路径
`ext.ets:2577-2665`，状态机 `n2_cd.rs:479-1034`）：

1. **`phase=plan`**（`n2_cd.rs:479-535`）：壳侧先实测当前物理承载（`bearerTypes` +
   link address），纯模块给出起始网络判定；核心校验 `column`/`kind`/`rep`/`id`/
   `target`，登记计划，派生两个控制 marker（`<id>-pre` / `<id>-post`），并把
   `target`/`proto` 写进**每一行**（冻结:L42 的预注册）。
   两列都在 **Wi‑Fi 上受理**：`net_before != "wifi"` → `start-bearer-not-wifi`；
   `start_verdict` 非空（承载不可测/不是 Wi‑Fi）→ 以该 token 具名拒绝。
   `C` 受理后即在位（`Phase::Ready`）；`D` 受理为**预置**
   （`Phase::AwaitStartBearer`，`reason=await-start-bearer`）。
   拒绝一律具名：`invalid-column` / `invalid-kind` / `invalid-rep` / `invalid-id` /
   `invalid-target` / `start-bearer-not-wifi` / `plan-already-active` /
   `plan-already-used` / `no-target` / `bearer-unknown` / `start-network-mismatch`。
   **受理只登记：不发任何控件**（`ext.ets:2650-2656`）。
2. **`phase=prepare` / `result=prepared|reason=not-a-d-measurement`**（仅 D 列，
   `n2_cd.rs:540-569`）：操作员切到蜂窝。`n2CdTransitionStep` 判定这是一次
   **准备跳**（`NetBirdN2CdPlan.ets:225-233`），壳调 `n2_cd_prepare` 记录它：
   **不计 C 事件、不发控件、不开重馈门**。随后蜂窝侧 `VPN_NETWORK_RECOVERED` 出现，
   壳调 `n2_cd_ready`（`n2_cd.rs:574-593`，`ext.ets:2772-2793`），`result=ready`
   `reason=preparation-recovered`，计划才「已就位」。两个上限分别是
   `PREPARE_WAIT_MS`（受理→准备）与 `PREPARE_WAIT_MS`（准备→恢复）。
3. **`phase=switch`**（`n2_cd.rs:608-680`）：正式切网。`VPN_NETWORK_EVENT` 观察者在
   判定 `switched=true` 后按**固定次序**走完这一拍（`ext.ets:914`）：
   ① **同步建立生产恢复门**（`n2CdGateArm`，`ext.ets:2942`）——在任何 await 之前，
   门一建立就禁止向核心供给本次新网的生产连接；
   ② 调 `n2_cd_identity(kind)`（`napi.rs:853-856`，`connector.rs:5598-5612`）取**旧网络**
   上该 kind 的 live 身份三元组 `(fd, dup_fd, protect_generation)`，随 `net_id_after`
   一起交给 `n2_cd_switch_identified`（`napi.rs:841-846`）；取不到（`ok=false`）或任一字段
   缺失时壳传 `-1`，核心**具名 fail-closed**：`switch-identity-required` /
   `switch-identity-missing` / `switch-net-id-unmeasured` /
   `switch-generation-unmeasured`；D 列跳过 prepare/ready 直接报切换 →
   `prepare-hop-required`；准备跳未落地就报切换 → `switch-before-preparation-complete`；
   D 列还要求准备承载上的最小驻留 `DWELL_MIN_MS = 3_000`（更快的返回是抖动，不是 D 事件）；
   ③ **才**调 `connectorPhysicalNetworkChanged('')`（核心侧旧连接清理，同步、不延后）；
   ④ 只有在③之后才向核心要一步（`pollN2CdStep`）——前控因此一定发在**新网络**上，
   且晚于核心的旧连接清理。方向由纯模块按实测承载判定（列 C 要求 wifi→cellular，
   列 D 要求 cellular→wifi）：不符即 `unsat|direction-mismatch`，任一侧承载不可测即
   `unsat|bearer-unknown`（不猜）。
4. **`phase=pre`**（`n2_cd.rs:725-780`，`ext.ets:3114`）：切换报成 `switched` 后，
   壳在**核心清理之后**立刻（不等下一个 watcher tick）向核心要一步：核心答 `step=pre`，
   壳用该 kind 的**生产 opener**（M/S/R → `mgmtSocketOpen`（tcp），D/I/Wb/Ws →
   `iceSocketOpen`（udp），**不 protect、不绑网络句柄、不走任何供给入口**）在**新网络**上
   发前控 marker `<id>-pre`。目标取冻结 capture 集合中该 kind 的目的（I 优先
   `udp/…:3478`、D 优先 `udp/…:53`，否则该协议的第一条；没有该协议的目的就 `no-target`）。
   壳在核心自己的 3 s 控制窗内等 HIT；`result=hit` **必须**由核心 tee 观察到，否则
   `unsat|pre-hit-unobserved`。**前控 HIT 是唯一开门条件**：在核心自己的 poll 命名
   **本计划 + 本切换 epoch** 回答 `step=recover` 之前，`recoverPhysicalNetwork()` 以及
   所有其它供给入口都不向核心交出本次新网的生产连接（见 §3.4）；旧网络上的前控被核心
   以 `pre-before-switch` 拒绝，永远开不了门（`n2_cd.rs:743-748`）。
5. **`phase=receipt`**（`n2_cd.rs:787-857`）：核心等该 kind 在**新网络**上的新身份
   回执，逐条判据：
   * `generation` 必须异于切换基线 → 否则 `generation-unchanged`；
   * `(fd, dup_fd)` 必须异于切换基线 → 否则 `identity-unchanged`；
   * `net_id` 必须等于切换时实测的新网络 → 否则 `net-id-mismatch`（`0` 即
     `net-id-unmeasured`）；
   * `protect` 必须成功 → 否则 `protect-failed`；
   * `io_mono_ms` 必须**晚于**新网络前控 HIT → 否则 `io-before-pre-hit`；
   * `M`/`S`/`R` 必须是真实的 `phase=subsequent_io` 业务写（connect 完成不算）→ 否则
     `connect-only`；
   * `Wb` 的回执是**新代的** `protect_settle|status=success`，由核心在**真正接管**
     该已 protect 的 base fd 时喂入（`n2_socket_trace.rs::adopted`，调用点是
     `wg_device.rs` 的 `adopt`/`reattach_socket`，即 dup 成功且 `getsockname` 校验
     通过之后）→ 否则 `wb-protect-settle-missing`。回执绑定的身份是**已接管的**
     core-owned `dup_fd`（不是壳自己的 fd 号），回执时刻是**该次 protect settle**
     的核心时钟，不是接管时刻：切换前 protect、切换后才接管的 socket 仍被
     `io-before-pre-hit` 拒绝。
   每一次拒绝都是**非终态** `rejected`：计划继续等有效回执，直到
   `RECEIPT_WAIT_MS = 90_000` 到点落 `unsat|receipt-wait-exceeded`。
   `Wb` 不再豁免：它照样有前后控，回执就是 protect settle，成功落
   `result=exempted|reason=wb-protect-settle-success`。
6. **`phase=post`**（`n2_cd.rs:996-1034`）：核心给壳 `step=receipt` 后，壳发出后控
   `<id>-post`（同一 opener）。核心要求：tee 观察到该 marker 的 HIT，且其 `mono_ms`
   **晚于**回执。`result=observed`；否则 `unsat|post-hit-unobserved` /
   `post-before-receipt` / `post-control-miss` / `post-wait-exceeded`。

写明的等待上限（全部是源码常量，调用点不可调）：

| 常量 | 值 | 位置 | 含义 |
| --- | --- | --- | --- |
| `PREPARE_WAIT_MS` | 300_000 | `n2_cd.rs:119` | D：受理→准备、准备→恢复，各一次 |
| `DWELL_MIN_MS` | 3_000 | `n2_cd.rs:123` | D：准备承载上的最小驻留 |
| `PRE_WAIT_MS` | 30_000 | `n2_cd.rs:127` | `switched` 之后等前控 HIT 的上限 |
| `RECEIPT_WAIT_MS` | 90_000 | `n2_cd.rs:130` | 等新网络回执的上限 |
| `POST_WAIT_MS` | 90_000 | `n2_cd.rs:133` | 交出 `step=receipt` 到后控回来的上限 |
| `SWITCH_WAIT_MS` | 300_000 | `n2_cd.rs:136` | 就位后等操作员切网的上限 |

### 3.1 行格式

状态机实际发出的行是 `format_line_ext`（`n2_cd.rs:421-432`）：

```
N2_CD_SANDWICH|id=…|column=C|D|kind=…|rep=…|phase=plan|prepare|switch|pre|receipt|post|marker=…|result=…|reason=…|net_before=…|net_after=…|local_before=…|local_after=…|fd=…|dup_fd=…|remote=…|wait_ms=…|mono_ms=…|net_id=…|protect_generation=…|identity=fd:dup_fd:generation|event_mono_ms=…|local=…|request_id=…|target=…|proto=…|detail=…|diag=true|is_evidence=false
```

`format_line`（`n2_cd.rs:405-414`）是**兼容/基线**形态：字段序列相同，只是到
`mono_ms` 为止（没有身份块），`client/core/tests/n2_log_samples.rs` 用它渲染冻结
夹具，形状不可移动。字段、取值集合与单位见
[`n2-log-formats-20260928.md`](n2-log-formats-20260928.md) §8。

`marker` 是**该阶段自己的** marker（plan/switch/receipt 为 `none`，pre 为
`<id>-pre`，post 为 `<id>-post`）；`fd` 是壳侧原始 fd、`dup_fd` 是核心自有 fd，就是
`N2_SOCK` 报的那一对；`identity` 是 `fd:dup_fd:generation`（没有就是 `none`）。
`net_before`/`net_after` 是实测承载
（`wifi|cellular|ethernet|bluetooth|unknown`），`local_before`/`local_after` 是该网络
自己的 link address（拿不到就是 `unknown`，永不编造）。

壳侧还有自己的投递/阶段回执 `N2_CD_SANDWICH_SHELL|requestId=…|id=…|column=…|kind=…|rep=…|phase=…|marker=…|result=…|reason=…|diag=true|is_evidence=false`
（核心行仍是判定权威）。

### 3.2 latch 与终态

壳侧 latch（`ext.ets:2897-2913`）**只跟随核心的终态**：

* `observed`（后控腿的终态）→ **清 latch 并复位门**（夹心完成，产品从此正常恢复）；
* `unsat`（核心 `close()` 的每个终态）→ 清 latch 并**把门锁在 `held`**：正式切换失败后
  绝不允许「计划没了 ⇒ 立刻恢复」——那会用新网供给污染仍采集的失败 epoch；
* 非终态 `rejected`（`pre-already-reported` / `switch-already-reported` /
  `invalid-result` 等）→ **保留** latch，因为核心的计划仍活着；
* `step=closed`（核心已发终态行）→ 按该行的终态做上面的 `observed`/`held` 处理并清 latch；
* `step=idle`（核心无计划）→ 清 latch，并把已建立的门**锁在 `held`**（计划消失永远不是
  放行理由）；
* `n2_cd_post` 调用**抛异常**时保留 latch：核心计划可能仍活着，只有它自己的 poll
  能收尾（`post-wait-exceeded`）；核心**答了**（无论 `observed` 还是具名拒绝）时，
  `observed` 复位门、其余一律 `held`。

### 3.3 与现有行的关联

* 前/后控本身仍由核心的 `N2_CONTROL_SENT` / `N2_CONTROL_HIT` / `N2_CONTROL_MISS` 记，
  `id=` 就是 `<id>-pre` / `<id>-post`；tee 的 HIT 经 `n2_cd::control_hit`
  （`wg_device.rs:498`）进状态机，壳侧自述不能替代；
* 生产回执就是该 kind 的 `N2_SOCK` 行：`phase=first_io`（I/D/Ws）或
  `phase=subsequent_io`（M/S/R 的 `post_connect_write`，`n2_socket_trace.rs:442-479`），
  Wb 则是核心在**接管**该 base fd 时喂入的 `protect_settle`（`n2_socket_trace.rs::adopted`
  → `n2_cd::receipt`，`dup_fd` 是已接管的 core-owned 描述符、回执时刻是 protect settle）；
  新 fd / 新 `dup_fd` / 新 `protect_generation` 都要变；
* 同一次接管还（**不依赖是否存在本计划**）打一条核心自己的
  `N2_SOCK|phase=protect_settle|kind=Wb` 行：`fd` = 壳原 fd、`dup_fd` = 已验证的
  core-owned 描述符、`proto=17`、`protect`/`status` 按该次保护实际结果、
  `protect_mono_ms` 取该次 protect 判决时刻（不是接管时刻）、**没有 wall 字段**、
  `detail=core-adopt`（2026-09-30；格式与语义见
  [`n2-log-formats-20260928.md`](n2-log-formats-20260928.md) §6/§11）。它是冻结 E1
  替代证据读的那条行，与喂给状态机的回执同源但彼此独立；
* 覆盖连续仍由 `N2_TUN_TEE_STATS`（`frames = decode_fail + dropped + Σ N2_TUN_DEST.frames`）
  与 `N2_TUN_GAP` 负责；本实现路径上的切网只做重解析 / 重 protect / 重馈 fd
  （`recoverPhysicalNetwork` 不 destroy/create），因此 tee 不卸载、`before-tun-attach`
  之后的窗口连续。**注意**：若执行时仍出现 recreate（例如 routes/peer 变化），按冻结
  第一批判据该格 `UNSAT` 停格，不能靠本机制补做通过。

### 3.4 生产恢复门（formal-switch production-recovery gate）

一个门、一个生命周期（`ext.ets:2925-2986`），四态：

| 状态 | 含义 | 谁写 |
| --- | --- | --- |
| `''`（无门） | 未武装、无活动计划、D 列的**准备跳**、或从未报过正式切换 | 受理/准备跳/`n2CdGateReset` |
| `blocked` | 正式切换已报，核心尚未放行：**所有 epoch** 的供给入口一律拒绝 | `n2CdGateArm`（切换拍内同步） |
| `released` | 核心 poll 命名**本计划 + 本 epoch** 回答 `step=recover`：**只**放行该 epoch | `n2CdGateRelease`（唯一放行点） |
| `held` | 正式切换（或计划）落到 `unsat`：**所有 epoch** 继续拒绝，直到本次取证被显式终止 | `n2CdGateHold` |

* **绑定**：门的 epoch 是切换拍内的 `networkEpoch`，还记着计划 id；`n2CdGateRelease`
  要求状态为 `blocked`、`n2CdGatePlan === id`、`networkEpoch === n2CdGateEpoch` 三者同时
  成立。异步回调（迟到的 `step=recover`、换网后的常驻放行）因此**放行不了另一个 epoch**；
  同一计划的第二次切换也不会重新武装一个已放行的门（那会放行更新的 epoch）。
* **覆盖的供给入口**（全部走同一个纯谓词 `n2CdProductionBlocked`）：
  `recoverPhysicalNetwork` 的**入口**、**每一轮追逐**（`do…while` 条件，旧 epoch 的
  在途尝试因此不会在 DNS await 之后 `continue` 追进被门挡住的新 epoch）、每个 await 之后
  的 `continue` 前置检查与实际 core feed 之前；`fulfillSocketRequest`
  （`SocketSupplyGate.productionGateBlocked` → `reason=production-gate`，**先于**
  `networkDirty` 规则且**不**享受 `isDns && networkRecoveryActive` 豁免）、
  `feedSpareProtectedSocket`、`feedOneIceSocket`、`topUpIceBatch`、
  `recoverPhysicalNetwork` 的 finally pending retry、以及 `performControlledRecreate`
  的 finally retry。
* **对照包不受影响**：前/后控由壳自己的 opener（`mgmtSocketOpen`/`iceSocketOpen` +
  `n2_control_send`）发出，**从不经过任何供给入口**，所以门关着也能发出前控并等到 HIT —
  这正是门能被打开的前提。
* **D 列准备跳正常恢复**：`n2CdTransitionStep` 判为 `prepare` 时不武装门，蜂窝侧的普通
  恢复照常重馈并在落地后报 `n2_cd_ready`；只有正式切换才建门。
* **未武装/拆除**：`''` 态是默认；`readDiagSwitches()`（下一次 start flow，即本次取证的
  显式终止）复位门；`onDestroy`/`destroyOnce` 先把 `networkRecoveryReady` 置 false，拆除
  路径**不经过门**，供给拒因是 `destroy`。
* **剩余契约缺口**：进程内**没有**「中途解除本次取证」的私有通道（不存在
  `n2_cd_abort` 之类），所以一次正式切换 `unsat` 之后，本次武装会话内的后续计划都会被同一
  个 `held` 门挡住（新计划不会重新武装门）。要恢复产品必须**终止本次取证**：停止/重启
  VPN 会话（`onDestroy`）或在下一次 start flow 不带 `n2_evidence` 票据（`readDiagSwitches`
  复位门）。这是**有意的 fail-closed**：不自行放宽冻结判据；若将来要在会话内解除，须先
  设计并评审该显式通道。

## 4. 与冻结 §3 的关系（实现后的口径）

T0 审查（2026-09-29）§3 已判定：下面第 1–5 条属**实现缺口的修复**，不构成判据变更；
§4 的五个冻结解释问题也有了书面口径。本实现按此落地：

1. **前控在「同一当前网络」，且紧后于切换**：受理不再发前控；切换拍内**先同步建门**、
   捕获旧身份并报切换，**再**做核心的旧连接清理，**最后**才向核心要步——`step=pre`
   因此一定在新网络上、且晚于核心清理。前控 HIT 之后核心 poll 才会答 `step=recover`，
   那是**唯一**放行点：在此之前**所有**恢复/新连接供给入口都不向核心交出本次新网的生产
   连接（§3.4，`ext.ets:2925-2986`）。前控与生产事件的间隔为秒级（`PRE_WAIT_MS = 30 s`
   是计划级保险丝，不是判据）。
2. **D 列可达**：D 在 Wi‑Fi 上预置受理，经 `prepare`/`ready` 走到蜂窝，再在回到 Wi‑Fi
   时正式测量（§3 第 2、3 步）。主机全程不必在蜂窝下发命令。
3. **回执是「新网络上的新身份真实发送」**：generation + fd/dup 必须变、`net_id` 匹配、
   protect 成功、I/O 晚于前控 HIT，M/S/R 必须是业务写（§3 第 5 步）。进程全局登记序号
   已从判据中**删除**（模块文档仍具名说明该旧路径，但代码里没有任何 `live_seq`
   读取）。
4. **Wb 补控件**：Wb 同样发前控 + 后控，其回执是**新代** `protect_settle`
   （§3 第 5、6 步），由核心在**接管**该已 protect 的 base fd 时喂入并绑定已接管的
   `dup_fd`/代际与**实际 protect settle 时刻**（`n2_socket_trace.rs::adopted`：壳阶段
   的裸 protect 不产生回执，dup/feed/adopt 失败也不产生）。冻结:L39「每格至少三个
   可归属的正控 marker 且 3/3 命中」在 Wb×C/D 同样适用。
5. **证据环可定位、可对账**：持久 `seq` + 会话 `sess`、轮转记录被丢弃的 seq 区间、
   ACK 前不删未取回段、manifest 兜底（§5）。

**仍然不构成「已符合冻结」的剩余项（须真机与复核）：**

* 目标 ROM 上 `bearerTypes`/link address 在 `trustedApplications=[本 App]` 下的**实际
  可读性**未验证（B5 已改为与 `physicalDefaultNetId()` 同一句柄，见
  §9；修完仍不可读则整列 `UNSAT`，见 T0 审查 Q4‑3）；
* D 方向两张物理网同时在线时 `physicalDefaultNetId()` 是否因多候选返回 `0`；
* 「紧」的数值上限若将来要写进判据，属新增判据，须测前 T0 + 用户批准（Q4‑1）；
* ±2000 ms 只是**跨侧配对的候选检索窗**，不是夹心间隔（Q4‑5）：C/D 事件与独立网络侧
  （Caddy/coturn/抓包/NAT 源）的配对仍受它约束，离线窗口的时钟漂移要计入 2000 ms 上界。

在上述项核实、并有一版针对**重写后**实现的 T0 复核之前，C/D 列的正式 AUTH **不应**把本
机制当作已符合冻结的实现。

## 5. 应用内证据环形文件

### 5.1 记什么

武装会话内，以下行**逐字**（与 hilog 同文）追加到应用私有目录的一个文件，并各带一条
`N2R|sess=…|seq=…|mono_ms=…|wall_ms=…|line=<原件>` 记录
（`n2_ring.rs:1110-1113`）：

* 核心侧：**全部** `N2_*`、`N7_WG_FEED`、`N2_TUN_*`——tee 就在 `hilog::emit` 尾部
  （`hilog.rs:64`，实现为 `tee_append` → `n2_ring::append`），凡是核心发出的行都自动
  入环；
* 壳侧（同一调用点、同一字符串，转发给核心的 `n2_ring_append`，`ext.ets:2486-2510`）：
  `VPN_NETWORK_BASELINE`（起始锚点，`ext.ets:816`）、`VPN_NETWORK_EVENT`（切网证据）、
  `VPN_NETWORK_RECOVERED`（重馈完成，`ext.ets:985-988`）、`VPN_NETWORK_RECOVERY_FAIL`
  （`ext.ets:899-903`）、`VPN_WG_DATAPLANE_FED`（attach/重馈锚点）、
  `N2_EVIDENCE_ARMED` / `N2_EVIDENCE_REFUSED` / `N2_EVIDENCE_ROUTES` /
  `N2_EVIDENCE_RECREATE_DETECTED`、`N2_TUN_DEST_CONFIG_FAILED`、`N2_PROC_NET*`、
  `N2_SOCK` 的壳侧三个阶段（`phase=open` / `protect_begin` / `protect_settle`）、
  全部 `N2_*_SHELL` 回执、`N2_RING_*`、以及各私有通道的 `N2_*_READ_FAILED` /
  `_CONSUME_FAILED` 失败行。

**明确不在环内（并说明原因）：**

* `EntryAbility` 在**另一个进程**（UI 进程）里写的启动期行（`N2_*_FILE_WRITTEN` /
  `_REMOVED`、`N2H_ARM_TICKET_*`、`N2_CONTROL_COMBO_REJECTED`）：环形文件由扩展进程
  的核心持有，UI 进程的库实例没有配置它；这些行在启动瞬间仍可从实时 hilog 取回。若
  后续要求纳入，须另设计跨进程汇聚（本次不做）。
* 未武装会话的一切行：环本身**只在**接受 `n2_evidence` 票据的会话里配置。

### 5.2 上限、轮转、失败计数与清单

* 单文件上限 `max_bytes`（默认 **4 MiB**，`n2_ring.rs:108`），保留 `segments`（默认
  **3**，`n2_ring.rs:110`）个文件：`<file>`、`<file>.1`、`<file>.2`，最旧优先淘汰。
  最坏磁盘占用 ≈ 12 MiB。
* 轮转公告是**新段的第一个记录**（`rotated_span_line`，`n2_ring.rs:1186-1189`），带
  `dropped_from_seq` / `dropped_to_seq` / `dropped_records` / `acked_seq`——被删掉的
  头部与「本来就从这里开始」因此可以区分。
* **取回前不删**：`can_rotate`（`n2_ring.rs:428-441`）要求目标段的最高 seq 已被 ACK；
  未确认的段不会被删，容量不足时改为计一次溢出（`N2_RING_OVERFLOW|dropped_records=…|from_seq=…|to_seq=…|acked_seq=…`）
  或写失败（`N2_RING_LOST|from_seq=…|to_seq=…|records=…|reason=…|code=…`），两者都
  带 seq 区间。
* 每次写入失败**计数**并公告**第一次**（`N2_RING_WRITE_FAILED|failures=…|code=errno-<n>`，
  只给 errno 型 code，不给路径/系统消息）；恢复后的**第一次成功**公告
  `N2_RING_RECOVERED|failures=…`，并且该恢复行**本身入环**（此时写入是好的）。公告路径
  有重入防线（`IN_APPEND`），不会递归回环。
* `N2_RING_STATS|file=…|enabled=…|bytes=…|max_bytes=…|segments=…|writes=…|failures=…|rotations=…|last_error=…|mono_ms=…`
  （`n2_ring.rs:1164-1168`）：
  * 武装时随 `n2_ring_configure` 落 `N2_RING_ARMED`；
  * 武装期间**每 60 秒一条**（`N2_RING_STATS_MS = 60000`，`ext.ets:225`；定时器由
    被接受的 arm 启动，`ext.ets:2544-2554`），行由核心自己格式化，因此进程被杀时环尾
    的计数最多落后一分钟；
  * teardown 时在 `onDestroy` 里报一次，然后 `n2_ring_disable()` 关闭——**文件保留**，
    等操作员取回；顺序是 `connectorStop()` → `destroyOnce()` → `n2_ring_status()` →
    `n2_ring_disable()`（`ext.ets:5029-5066`），这样关闭期的行（TUN gap、destroy 关掉
    的 fd 上的 `N2_SOCK` / `N2_FD_SEND`）也进环。
* `N2_RING_MANIFEST|file=…|sess=…|enabled=…|…|first_seq=…|last_seq=…|frozen_tail_seq=…|next_seq=…|acked_seq=…|…|gaps=…|gap_from=…|gap_to=…|malformed=…|tail_open=…|last_error=…|mono_ms=…`
  （`n2_ring.rs:1208-1217`）是头/尾/缺口/失败的一次性清单；取回时先要它
  （`n2_ring_manifest`，`ext.ets:3037`），清单行由核心自己发出并入环。

### 5.2.1 诊断用的段大小缩小开关（2026-10-01）

0048–0052 的真机记录里 `rotations=0`：默认 4 MiB 的段在试跑窗口内写不满，
「旧日志前缀被删除时有记录」这一证明点因此不可考。为让**诊断试跑**真的走到轮转，
arm 票据的 `n2_evidence` 对象新增**可选**键 `ring_segment_bytes`（该对象正是被接受的
证据票据，也是唯一配置环的路径）：

* 键**缺失** ⇒ 与改动前**逐字节相同**的 4 MiB 默认（不新增任何行；`N2_RING_ARMED`
  的 `max_bytes` 仍是 `4194304`，环的第一个记录仍只有 `N2_RING_ARMED`）。
* 键**存在** ⇒ 必须是整数字节数，范围 `[64 KiB, 4 MiB]`（**只能缩小，不能放大**）。
  越界 / 非整数 / 非数字 ⇒ **整张证据票据按名拒绝**：`parseN2Evidence` 返回
  `invalid-ring-segment-bytes`，start flow 在建立 VPN 之前返回，日志里是
  `N2_EVIDENCE_REFUSED|requestId=…|reason=invalid-ring-segment-bytes|diag=true|is_evidence=false`。
  这与既有票据风格一致（`NetBirdN2Evidence.ets` 的其它 `invalid-*` 错误：present but
  malformed request must not start a VPN）。核心 `n2_ring_configure` 用同一个下界
  （`n2_ring.rs` 的 `MIN_MAX_BYTES = 64 KiB`，壳侧常量 `N2_RING_MIN_BYTES`）再校验一次。
* 生效的段大小由**新增的一行**显式记录——冻结文档 `docs/n2-log-formats-20260928.md` §9
  的 `N2_RING_ARMED` 字段表**不改**，因此不往那一行追加字段（`N2_RING_ARMED` 的
  `max_bytes` 本来就等于生效的段大小，新行是显式标记）：

  `N2_RING_SEGMENT_BYTES|segment_bytes=65536|default_bytes=4194304|mono_ms=…|diag=true|is_evidence=false`

  该行**只在段大小小于默认值时**发出（默认 arm 一行都不多发），紧随 `N2_RING_ARMED`
  之后，是环的第 2 条记录。它**不在**冻结文档 §9 的 `N2_RING_*` 清单内，格式以本文件
  与 `client/core/tests/fixtures/n2-log-samples.txt` 为准（核心格式化函数
  `n2_ring::segment_line`）。

诊断触发方式（`n2_evidence` 里带该键）：

```
aa start -b cn.alfadb.netbird -a EntryAbility --ps n2h_arm_options \
  '{"endpoint_exclusions_enabled":false,"n2_evidence":{"trusted_self":true,
   "capture_32":[…],"outer_destinations":[…],"hdc_control_ip":"…",
   "allow_hdc_overlap":false,"ring_segment_bytes":65536}}'
```

**正式取证不得使用该键。** 正式执行器 `n2-campaign/run_campaign.py::arm_ticket()`
生成的 `n2_evidence` 对象**不含** `ring_segment_bytes`（恒缺失 ⇒ 正式路径行为不变）；
要在诊断模式中启用，须由执行器在生成票据时显式加入该键——属 kit 侧改动，不在本次范围。

### 5.3 与 hilog 失败/流控的关系

环的写入**不依赖** `OH_LOG_Print` 的返回值：tee 在 hilog 打印之后无条件执行，平台
返回码不进任何判断；hilog 流控丢行不会少写环，环写失败也不会改变 hilog 的输出。
两者唯一的耦合是「同一函数里的顺序执行」。

### 5.4 取回方式（只读调研的结论与依据）

**结论：主路径是厂商文档给出的、带包名的命令行沙箱访问；`hdc shell cat` 不可行；若
`-b` 在目标 ROM/签名形态上不可用，用 `n2_ring_dump` 私有通道按持久 seq 分页把窗口重发
到实时 hilog。**

依据（均为仓内既有材料，本次**未**做任何设备操作）：

1. **`hdc shell cat` 读不到应用沙箱。** 应用沙箱路径（`/data/app/el2/<user>/base/<bundle>/…`，
   应用内的 `filesDir`）对 `shell` 域不可读——仓内设计文档已登记「hdc **读不到** App
   沙箱（shell 域无权限）」[`n2h-device-collector-design-20260917.md:97`](n2h-device-collector-design-20260917.md)；
   同一文档第 98 行与厂商答复第 4.2 条把「应用沙箱」与「/data/local/tmp」明确区分开。
2. **应用写不了 `/data/local/tmp`。** 厂商第 4.1 条：`/data/local/tmp` 不属于应用可见
   范围；仓内 0027 的实测结论是应用 `accessSync` 对该路径答 `false` 而 shell 能列出
   [`n2h-huawei-support-replies-20260923.md:68-69,101`](n2h-huawei-support-replies-20260923.md)，
   另有 [`n2h-arm-private-channel-fix-20260918.md:21-23`](n2h-arm-private-channel-fix-20260918.md)
   记录该否定结论并据此把票据通道迁进 `filesDir`。因此「把环复制到 /data/local/tmp
   等 hdc 可读路径」在本平台上**没有**应用可写、shell 可读的交集。
3. **官方交付路径是 `hdc file send/recv -b {bundleName}`。** 厂商第 4.2 条给出的正是
   应用沙箱的命令行访问方式（`hdc file send -b {bundleName} … ./data/storage/el2/base`、
   `hdc file recv -b {bundleName} ./data/storage/el2/base/… <host>`）
   [`n2h-huawei-support-replies-20260923.md:71-74`](n2h-huawei-support-replies-20260923.md)；
   同文件 §2 的 4.2 行记录了 OpenHarmony hdc 文档的前置条件与失败码（**2026-09-23 实读**）：
   目标 App 必须**用调试证书签名且已在设备上启动**，否则 `E003001`；远端路径越出应用
   数据目录报 `E005102`；SDK/ROM 不支持 `-b` 报 `E005004`。仓内**从未**用过 `-b`，
   AUTH 0029 第三项即为此项验证（该 AUTH 处于草案/未授权状态）。
   因此本次实现取回的**主命令**是：

   ```bash
   hdc file recv -b cn.alfadb.netbird \
     ./data/storage/el2/base/files/netbird-n2-evidence-ring.log <host>
   # 轮转过的旧段（最旧在前）：
   hdc file recv -b cn.alfadb.netbird \
     ./data/storage/el2/base/files/netbird-n2-evidence-ring.log.2 <host>
   ```

   前置条件与失败码必须在正式 AUTH 里逐条登记；`-b` 未经验证前不得作为唯一取回路径。
4. **兜底：`n2_ring_dump` 私有通道（按持久 seq 分页）。** 回到 Wi‑Fi 后（hilog 可实时
   读），先要一次清单，再按 seq 取窗口：

   ```bash
   # 1) 清单（核心自己发 N2_RING_MANIFEST，含 first_seq/last_seq/frozen_tail_seq/…）
   aa start -b cn.alfadb.netbird -a EntryAbility --ps n2_ring_dump '{}'
   # 2) 按 seq 取一页；to_seq 取清单里的 last_seq 冻结尾部，count ≤ 256
   aa start -b cn.alfadb.netbird -a EntryAbility \
     --ps n2_ring_dump '{"from_seq":0,"to_seq":1234,"count":64,"session":"<arm token>"}'
   ```

   请求文档**只认** `{"from_seq":…,"to_seq":…,"count":…,"session":…}`（
   `NetBirdN2Ring.ets:99-154`，核心侧 `n2_ring.rs:766-802`）。含 `start` 的
   **旧式按行号请求一律拒绝**：`legacy-start-index-unsupported`——拼接段上的行下标映射
   不到持久 seq（轮转会平移），假装服务它会静默重复或漏行。其余具名拒绝：
   `empty-request` / `invalid-json` / `invalid-request` / `invalid-count` /
   `invalid-session` / `request-too-long`（壳与核心同名同判）。
   扩展把核心答的 `N2_RING_CHUNK|seq=<持久 seq>|of=<count>|line=<原始行逐字>`
   （`line=` 是最后一个字段，其内容可含 `|`）和一条
   `N2_RING_DUMP|from_seq=…|to_seq=…|count=…|emitted=…|frozen_tail_seq=…|…|acked_seq=…|result=dumped|rejected|reason=…|diag=true|is_evidence=false`
   （`n2_ring.rs:1226-1231`）**只打到 hilog**；两者都**不写回环**（chunk 是环内容的
   副本，汇总行根本不是环内容）。壳自己另发一条
   `N2_RING_DUMP_SHELL|requestId=…|from_seq=…|to_seq=…|count=…|emitted=…|ack_watermark=…|result=…|reason=…|diag=true|is_evidence=false`
   （`NetBirdN2Ring.ets::formatN2RingDumpShellLine`）**这一条**才写回环——它是一次取回事件。
   **dump 绝不推进 ACK**：回执行里的 `ack_watermark=` 只是核心**当时**的水位（被拒的页
   记 `-1`），重发一页只证明应用**读到了**，不证明主机**持久化并核验**了它。分页推进用
   `from_seq = 上次最后 seq + 1` 重复执行即可取完整环。
   **该通道需要运行中的扩展**（它由 watcher tick 消费）；teardown 之后扩展已停，dump
   文件无人消费——要用就得按 AUTH 白名单重新起一次 VPN 会话。

   取回之后、只有主机**已把环字节持久化并核验窗口完整**时，才发显式 ACK
   （`ack_seq`，`n2_ring.rs`）：

   ```bash
   # 3) 显式 ACK：session 取 N2_RING_MANIFEST 的 sess=，up_to_seq 取已核验的最大持久 seq
   aa start -b cn.alfadb.netbird -a EntryAbility \
     --ps n2h_arm_options '<the same arm ticket that armed this session>' \
     --ps n2_ring_ack '{"session":"<sess= from N2_RING_MANIFEST>","up_to_seq":1234}'
   ```

   请求文档**两个键都必需**（`{"session":…,"up_to_seq":…}`；核心侧
   `n2_ring::parse_ack_request`，壳侧 `NetBirdN2Ring.ets::parseN2RingAck`，壳与核心同名
   同判：`empty-request` / `request-too-long` / `invalid-json` / `invalid-request` /
   `invalid-session`）。壳把文档**逐字**转发给 `n2_ring_ack`，绑定关系由**核心**在环自己的
   锁内判定，壳不会「读一次 session 再无条件 ACK」：

   * `not-configured`（本进程从未配置过环）/ `not-armed`（环未武装——ACK 只对武装中的环生效）/
     `session-mismatch`（旧 arm 或不属于本环的 token）/ `seq-beyond-tail`（越界）一律拒绝，
     水位**不动**；
   * 同 session 的重复 ACK（`up_to_seq` 不大于当前水位）**幂等**：答 `ok`、
     `advanced=false`、水位不变；水位单调（`max`），永不回退。

   回执行 `N2_RING_ACK_SHELL|requestId=…|sess=…|up_to_seq=…|acked_seq=…|advanced=…|result=acked|rejected|reason=…|diag=true|is_evidence=false`
   是**唯一**报告水位推进的行（`advanced=true`）；文档没通过壳的解析时是
   `…|sess=-|up_to_seq=-1|acked_seq=-1|advanced=false|result=rejected|reason=…`，且**不调核心**。
   ACK 是轮转的放行条件：不确认，未取回的段不会被删（容量不足时记一次溢出
   `N2_RING_OVERFLOW`，绝不覆盖）。

## 6. 与冻结文本的关系（再次强调）

* 本机制不改变冻结的任何判据、阈值、oracle、适用集或停止条件；
* 它**不**自证任何格达标；`N2_CD_SANDWICH` 与环内行全部 `is_evidence=false`；
* 是否把某个 `N2_CD_SANDWICH` 区间当作冻结意义上的「逐事件夹心」，取决于针对重写后
  实现的 T0 复核；复核前相关格保持 `UNSAT`。

## 7. 主机验证（本次做过的，以及没做的）

做过（无设备、无网络、`cargo --offline`）：

* `client/core` 单元测试：`n2_cd.rs`（计划校验与全部具名拒绝、C 的受理即就位、D 的
  `await-start-bearer`→prepared→ready、最小驻留、identity 缺失的各类 fail-closed、
  前控→切换→回执→后控顺序、回执/切换/后控超时的 `unsat` 语义、Wb 的 protect-settle
  回执、未武装静默）与 `n2_ring.rs`（逐字追加、持久 seq、上限与 ACK 门控轮转、溢出/
  丢失的 seq 区间、失败计数与恢复公告、递归防线、清单、格式化器）；
* 集成测试 `client/core/tests/n2_cd.rs`：用**真实** `N2DestinationTee::consume_control`
  产生前/后控 HIT、用**真实** `n2_socket_trace` 的 protect/first_io/post_connect_write
  产生新身份回执，走完整 JSON 面（`n2_cd_plan_json` → `n2_cd_identity_json` →
  `n2_cd_switch_identified_json` → `n2_cd_poll_json` → `n2_cd_post_json`）；另有
  `tests/n2_cd_state_revision.rs`（重写状态机的修订面）与
  `tests/n2_ring_integrity.rs`（持久 seq / ACK / 轮转完整性；显式 ACK 的
  session 绑定、越界、幂等与「读路径不动水位」）；
* 日志格式基线：`client/core/tests/n2_log_samples.rs` + `tests/fixtures/n2-log-samples.txt`
  覆盖 `N2_CD_SANDWICH`（基线形态）全阶段与 `N2_RING_*`；
* 壳侧自检 16 套 `client/entry/selfcheck/run*.mts` 全绿，其中 `run-n2-cd.mts`
  （纯模块 + 源码钉 + 门的 G1–G7 生命周期钉）、`run-physical-network.mts`（纯策略：
  门先于 `networkDirty` 且不被 DNS 豁免穿透）、`run-exclusion-generation.mts` 的
  N16–N30（真 `EntryAbility` 六通道规则、真 `pollN2CdPlan` / 切换钩子 + 身份 / 步进轮询 /
  环转发 / 按 seq 取回 + manifest、dump 不推进 ACK、显式 ACK 通道、以及 **N26–N30 的门
  行为驱动**：合法 D 的 prepare→ready(+DWELL)→在途恢复卡 DNS await→正式切换→核心清理→
  新网前控→释放旧 await→`step=recover` 前零供给、之后恰好一次重馈；切换早于准备完成落
  `unsat` 后门 `held` 且不自放行；recreate-finally 与 pending-retry 都被门挡住；disarm/
  teardown 清理；异步 `step=recover` 的计划/epoch 绑定）。计数器直接读**核心接缝**的
  `__NB_SUPPLY_*`（`exclusion-generation-hooks.mjs`），不是文本断言；
* `DEVECO_SDK_HOME=/home/worker/harmonyos/command-line-tools/current bash client/build.sh`
  端到端成功（core 交叉编译 → `.so` 拷贝 → `hvigorw assembleHap` release 未签名 →
  HAP 内 `.so` sha256/ELF/apiVersion=26 校验），即 ArkTS 编译面（`CompileArkTS`）实际执行
  并通过。

**没做**：任何真机操作、任何 AUTH、任何抓包或管理端改动。因此：

* 目标 ROM 上 `bearerTypes`/link address 的**实际可读性**未验证——这是 C/D 列能否执行
  的前置条件（见 §4）；
* `hdc file recv -b` 的**实际可用性**未验证（E003001/E005004/E005102 三种失败形态都还
  只是文档契约）；
* ±2000 ms 配对、tee 连续性、物理/独立网络侧 oracle 等冻结判据仍须按正式 campaign
  取证。本机制只提供设备侧的一半。

## 8. 操作员速查（正式 AUTH 内使用）

```bash
# 0) 武装（既有票据；本机制不新增权限）
aa start -b cn.alfadb.netbird -a EntryAbility \
  --ps n2h_arm_options '<arm ticket with n2_evidence>'

# 1) Wi-Fi 阶段：下发一列/一行/一次的切网夹心计划（每次一个全新的 id）
aa start -b cn.alfadb.netbird -a EntryAbility \
  --ps n2_cd_plan '{"column":"C","kind":"M","rep":1,"id":"cd-C-M-1"}'
# 观察 N2_CD_SANDWICH|…|phase=plan|result=planned（受理只登记，不发控件）

# 2) 操作员按该列方向切网。
#    C 列：直接是 phase=switch；D 列先看到 phase=prepare|result=prepared，
#    蜂窝侧重馈完成后 phase=prepare|result=ready。

# 3) 切换报成 switched 后：核心先做旧连接清理，然后 phase=pre|result=hit
#    （前控在新网络、所有生产供给被门挡住），核心 poll 随后给 step=recover，
#    重馈才发生；再 phase=receipt|result=receipt，最后 phase=post|result=observed。
#    （90 s 内没有新身份回执即 unsat|receipt-wait-exceeded）
#    三次独立事件用三个不同的 id（rep=1/2/3）。
#    ⚠ 一次正式切换落 unsat 后，门进入 held：本次武装会话内的后续计划都会被挡住。
#    要测下一个事件，必须先终止本次取证——停/重启 VPN 会话（onDestroy），或让下一次
#    start flow 不带 n2_evidence 票据（readDiagSwitches 复位门）。见 §3.4。

# 4) 回到 Wi-Fi 后取回环形文件（主路径）
hdc file recv -b cn.alfadb.netbird \
  ./data/storage/el2/base/files/netbird-n2-evidence-ring.log <host>
#    （另取 .1 / .2；不可用时用 n2_ring_dump 按 seq 分页兜底，见 §5.4）
```

## 9. 代码位置

| 关注点 | 位置 |
| --- | --- |
| 计划状态机、行格式、等待上限 | `client/core/src/n2_cd.rs`（`plan` :479、`prepare` :540、`ready` :574、`switch_identified` :608、`receipt` :787、`poll` :914、`post` :996） |
| 环形文件、持久 seq、ACK 门控轮转、清单、显式 ACK | `client/core/src/n2_ring.rs`（`parse_dump_request` :766、`dump_page` :855、`parse_ack_request`、`ack_seq`（`(up_to_seq, session)`，环锁内绑定当前 arm）、`manifest_line_now` :701、`rotated_span_line` :1186） |
| tee 挂钩（核心行入环） | `client/core/src/hilog.rs:64`（`tee_append` → `n2_ring::append`） |
| 身份三元组（回执/切换基线） | `client/core/src/n2_socket_trace.rs`（`live_identity`、`adopted`（Wb 的接管登记 + protect-settle 回执，由 `wg_device.rs` 的 `adopt`/`reattach_socket` 调用）、`post_connect_write` :442-479） |
| HIT 钩子（tee → 状态机） | `client/core/src/wg_device.rs:498`（`n2_cd::control_hit`） |
| NAPI 桥导出 | `client/core/src/napi.rs:138-152`；JSON 包装 `client/core/src/connector.rs`（`n2_cd_plan_json` :5521、`n2_cd_switch_identified_json` :5573、`n2_cd_identity_json` :5598、`n2_ring_manifest_json` :5693、`n2_ring_dump_json` :5713、`n2_ring_ack_json`（请求文档 `{"session":…,"up_to_seq":…}`）） |
| 六个通道的组合规则 | `client/entry/src/main/ets/entryability/EntryAbility.ets`（`N2_CD_PLAN_PARAM` :93、`N2_RING_DUMP_PARAM` :99、`N2_RING_ACK_PARAM` :102） |
| 应用侧编排（受理/准备/就位/切换/建门/前控/门控重馈/回执/后控/环/取回/显式 ACK） | `client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets`（`pollN2CdPlan` :2650、`n2CdOnSwitch` :2762、`n2CdReadyAfterRecovery` :2861、`n2CdLatchAfter` :2897、`n2CdClearLatch` :2908、`pollN2CdStep` :3004、`n2CdSendPreControl` :3114、`n2CdSendPostControl` :3142、`pollN2RingDump`、`pollN2RingAck`、`n2RingConfigure` :2595、`n2RingStartStats` :2617、`onDestroy` :5324） |
| 生产恢复门（唯一放行点、绑定 plan+epoch、终态 held） | 同上：`n2CdProductionBlocked` :2925、`n2CdGateArm` :2942、`n2CdGateRelease` :2958、`n2CdGateHold` :2972、`n2CdGateReset` :2981；供给入口的调用点：`physicalNetworkAvailable` :914（建门→身份→核心清理→前控）、`recoverPhysicalNetwork` :963（入口 + 每轮 + while + finally retry）、`fulfillSocketRequest` :4420、`feedSpareProtectedSocket` :4507、`topUpIceBatch` :5008、`feedOneIceSocket` :5086、`performControlledRecreate` 的 finally :4138 |
| 门纯策略（`productionGateBlocked` → `reason=production-gate`，先于 `networkDirty`） | `client/entry/src/main/ets/vpnextensionability/NetBirdPhysicalNetworkPolicy.ets`（`SocketSupplyGate`、`socketSupplyRejectionReason`） |
| 纯模块：计划解析、目标选择、承载分类、起始/方向/准备判定与阶段判定、回执格式 | `client/entry/src/main/ets/vpnextensionability/NetBirdN2CdPlan.ets`（`n2CdStartVerdict` :194、`n2CdDirectionVerdict` :203、`n2CdTransitionStep` :225、`n2CdPrepareVerdict` :238） |
| 纯模块：按 seq 分页的 dump 解析、显式 ACK 文档解析、壳侧回执 | `client/entry/src/main/ets/vpnextensionability/NetBirdN2Ring.ets`（常量、`parseN2RingDump`、`formatN2RingDumpShellLine`、`formatN2RingDumpShellReject`、`parseN2RingAck`、`formatN2RingAckShellLine`、`formatN2RingAckShellReject`） |
| 平台事实读取（bearerTypes / linkAddresses / 同一句柄的 netId） | `client/entry/src/main/ets/vpnextensionability/NetBirdConnector.ets::physicalDefaultBearerFacts`（:898-935，句柄来自 `physicalDefaultNetId` :937-966） |
