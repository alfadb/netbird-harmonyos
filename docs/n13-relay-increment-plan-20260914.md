# N13 增量计划：relay 客户端最小路径（测量前登记，2026-09-14）

> 本文件按 `docs/native-nx-governance.md` **§二.7「判据预注册」** 编写：每门判据、oracle、停止条件、
> 不外推声明在测量开始前书面登记并经独立审查确认。**独立审查 = 跨厂商 T0 席 `xai/grok-4.6` 的 relay 范围裁定**
> （2026-09-14；原文见下 §2 摘要，完整回复经主会话转录）。测量后不得追认或修改本文件。
>
> **本增量不得被读作 N2-H pass 或 N6 pass**；主机侧测通不等于任何门结论。

## 1. 增量定位

- **编号**：N13（承接 N12a advertised candidate）。
- **性质**：补齐 `§二.5` 已列的 **R0 必选功能「direct+relay 双路径」**，不是范围扩张。
- **范围**：在 `client/core` 内实现**最小 relay 客户端**（离线手写 WSS）→ Auth/OpenConn/Transport/HealthCheck 帧 → 把 relay 流当作一条**独立 WG 外层承载**（等价上游 wgProxy）接入现有 WireGuard 收发。**ICE 逻辑不改**（relay 非 ICE 候选类型）。
- **不做**：QUIC 路径（生产实例无 QUIC；非 `rel://`/`rels://` scheme **fail-closed**）；ICE-TURN（上游亦非同层）；多中继聚合。
- **依赖纪律**：**不新增 crate**（base64/data-encoding/hmac/httparse/tokio/rustls/ring 已在锁内）。引入 `tokio-tungstenite` 等将**翻转** T0 的 Q1 结论为「栈变更、须回 T0」。

## 2. T0 裁定摘要（`xai/grok-4.6`，2026-09-14）

- **Q1**：本增量**不构成 §二.9 冻结栈变更**（冻结的是编译层选型；手写 WSS 与已手写 ICE/STUN 同类）。**不需要**新 T0 改条款；**需要**工程增量编号（本文）+ §二.7 判据预注册（本文）。门范围/顺序/阈值未变，不触发 §二.10。
- **Q5**：离线手写 WS 可接受；QUIC 不必同期；冻结表里的 quinn 行保留不动（**不得**把"本期只做 WSS"写成否决 quinn）。
- **Q7（给决策者）**：批准在冻结栈内手写 WSS relay 补齐 R0「direct+relay」；但须**先采官方客户端 oracle**，**默认路由启用前把 relay TCP 端点纳入 N2-H**，且本增量不得写成 N2-H 或 N6 pass。

## 3. 判据与 oracle（登记项）

### 3.1 Oracle
**同一生产实例上官方客户端的可观察行为**（§二.5：不得只用 IDL 自洽）。版本必须显式登记，不得默认同构：

| 对象 | 版本 |
|---|---|
| 治理文本写作时的基线 | 自托管 v0.76.3 |
| 实际对接实例 | 控制面 **0.77.0** / 中继 **0.76.3** / dashboard v2.91.1 |
| 我们的调研 clone | **0.78.1**（commit `791401060d2b…`） |
| 官方客户端（待采） | **待登记**（本增量硬前置） |

### 3.2 对照试验（官方客户端先、本客户端后，同实例）
- **(A) P2P 可达**：ICE 选中 host 候选，WG 外层指向 peer 的 host/srflx；**relay Transport 字节 ≈ 0**。
- **(B) 阻断 peer 间 UDP**：overlay 仍通；本端 WG endpoint = 本地 relay 代理地址（**非** peer 候选）；对端公网侧**无 WG UDP**；本端与中继的 Transport 帧/字节随 overlay 流量同步增长。

### 3.3 路径正证据（缺一不可）
1. WSS 五元组 ∈ 冻结阶段解析出的 `rels://` 结果；
2. Transport 帧计数（本端）；
3. 中继侧或对端侧的会话/投递记录。
> 仅"直连 WG 消失"**不足**（可能是丢包而非走中继）。

### 3.4 N2-H 外层义务（明文要求枚举 relay）
建连前**冻结 relay 的 TCP host:port**；全部排除后才允许建立 WSS；解析失败 / 换址 / 观测不符 → **fail-closed**。API 返回值不作证据。
**已修**：`n2h.rs` 原把 `EndpointKind::Relay` 归为 `udp`（与生产 WSS/TCP 不符，会让冻结集合无法证伪 hairpin）——已改为 `tcp` 并加断言（commit `820f665`），回环 harness 的 relay sink 同步改为 `TcpSink`。

### 3.5 TUN 包级负证据
外层 WSS 探针的唯一载荷**不得**出现在任何 TUN 帧（命中 = `n2h-fail`）；overlay 正控必须在 TUN 被观测（缺失 = `n2h-inconclusive`）。
负证据只证明"外层未入隧道"，**不单独证明"走了 relay"**——后者靠 §3.3。

### 3.6 停止条件
出现以下任一即停并上报：默认路由下 WSS 未先排除 relay TCP；无 §3.3 正证据却报 relay 成功；ICE Failed 被当作可达；令牌过期静默假通；destroy/注销后仍有 WSS 出站连接。

## 4. 重验义务（新增常驻出站 WSS 连接所触及的既有结论）

| 既有结论 | 必须重验 |
|---|---|
| **N1 fd 合同** | WSS 为 native 自建 TCP（类 management）：**禁止** close TUN 原号；TUN 原号仍仅由 `VpnConnection.destroy()` 关闭。补所有权表；destroy/注销/本地清理必须拆掉该连接；若涉及壳 fd，仍只消费 `F_DUPFD_CLOEXEC` 副本 |
| **默认路由闸 / 数据面就绪** | "ICE 提名 socket = WG endpoint"**不得外推**；ready = ICE 提名 **或** relay OpenConn + 本地代理已起；ICE Failed **不得**静默当可达；双路径并存时 relay 字节不得计入 ICE 成功 |
| **N2-H** | 冻结集合必须含 relay **TCP** 端点并重跑 TUN 负证据 + 端点侧投递；failover/重解析 = 新端点 = **重冻** |
| **MTU** | 38B 帧头 + WS + TLS + TCP；`MaxMessageSize=8820`；既有 ICE 路径 MTU 结论不得外推 |
| **凭据 24h / 切址** | 属 R0「凭据轮换」「断网/切网/重连」；过期未刷新必须 fail-closed |
| **撤销清理** | 注销/destroy 后 WSS 必须消失，不得留出站连接 |
| **ICE 直连** | P2P 可达时仍须成立（优先级 `Relay < ICE-TURN < ICE-P2P`） |

## 5. 顺序与硬前置

```
[编码开工] 无硬前置
     ↓
[默认路由下启用 WSS] 硬前置：relay TCP 端点已纳入 N2-H 冻结并排除
     ↓
[N13 验收测量] 硬前置：官方客户端 oracle 已落盘（§二.7）
```

## 6. 官方客户端 oracle 采集清单（硬前置）与**已知可行性约束**

T0 要求的最小采集（官方客户端 × 同一实例）：
1. P2P 开 / P2P 断 两拓扑；
2. 抓包：WG-to-peer vs WSS-to-relay；
3. URI、`/relay`、Auth/OpenConn/Transport/HealthCheck 时序；
4. token 出现与 Sync 刷新；
5. 切址 / 杀主（若 urls>1 或可操作）；
6. overlay MTU 上限 vs 8820；
7. 拆除与重连；
8. 服务端/客户端版本 vs 治理 v0.76.3。

**可行性约束（如实登记，须在采集前确定替代方案）**：
- 我们的 pod **无 `CAP_NET_RAW`**（`tcpdump` 与 `AF_PACKET` 均被拒，2026-09-14 实测）→ 第 2 项**无法在本 pod 内完成**。
- 替代候选（须 T0 或用户确认后登记）：
  (a) 由运维在**中继主机或 k8s 节点**侧抓包（最接近 T0 原意）；
  (b) 用官方客户端**自身可观察输出**（`netbird status` 的连接类型 P2P/Relayed）+ 本端 `ss` 观测 WSS 五元组 + 我方 `isolation-check` 的 Transport 计数作为第 2 项的等效证据，并在登记中明示这是"等效但不同源"的证据；
  (c) 在一台我们可抓包的机器上运行官方客户端。
- 官方客户端需**另一把 setup key**（现有一次性 key 已被 2026-09-14 的零代码确认消耗）。

## 7. 可接受残余（须书面登记，不得假通）

- 只做 WSS：生产若 failover 到纯 QUIC 则 **fail-closed**（日志 + 不可用，不得假通）；
- 不做 ICE-TURN（与上游"relay 非 ICE 候选"一致）；
- 当前实例 `urls=1`、无多中继聚合；
- 主机侧测通 ≠ N6/N2-H pass；
- 0.76.3 与 0.78.1 的版本差（须在证据中显式标注）。

## 8. 证据与登记

- T0 裁定原文：经主会话转录；本文 §2 为其摘要，完整文本随会话交付。
- 生产实例对接实测（零代码确认）：`~/harmonyos-signing/netbird-n1bdisc/records/prod-management-relay-probe-20260914.md`（+`.sha256`，`is_evidence:false`）。
- 线格式规格：`docs/relay-client-spec-20260914.md`（上游 `file:line` 依据）。
- 本增量的设备侧证据将写入 `diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260914-0002/`（现 AUTH 有效期至 2026-09-15T14:29+08:00）。

## 附录：测试偶发（flake）治理记录（2026-09-14）

### A1. 发现方式与标准变更

- N13 推进期间的一次 **100 轮全量压测**发现 8/100 轮失败，涉及 5 个不同测试；而同样的测试**单测隔离 220 次全部通过**。隔离通过 + 并行失败，判定依据就是**跨测试并行干扰**（fd 号复用、瞬态状态、时序假设），不是测试自身逻辑错。
- 由此修订"已验证"标准：**3 连跑不足以支撑"已验证"**；改用 `flake-check.sh` 压测 **≥30 轮**，且任何失败必须带**测试名与断言原文**（只有"failed"计数不算证据）。
- 反向推论同样成立：**单测隔离复现失败 ≠ 无 bug**——隔离通过只证明无自身逻辑错，不能排除并行干扰。

### A2. 工具：`client/core/flake-check.sh`

- 用法：`bash client/core/flake-check.sh N`（N 为轮数，默认 20）。每轮循环跑一次全量 `cargo test --offline --locked --color never`。
- 失败轮的完整日志按**每轮唯一时间戳**保留在 `target/flake-check-<时间戳>-round-<N>.log`，**不覆盖**（重跑不再毁证据，此改进属 `57be0da`）；脚本就地抓出失败测试名与 panic 断言原文。
- 汇总"N 轮中有几轮失败"；只要有失败轮即**非零退出码**，可直接作验收门。

### A3. 两轮修复的事实清单

**第一轮 `ebd9d30`（fd 号复用 + 熔断过紧 + 瞬态断言）**

1. `host_sockets::fd_bag_closes_every_kept_fd`（3/100）：断言依赖 fd 号，Drop 关闭后并行测试可能复用同号。修法是 `F_DUPFD_CLOEXEC` **自适应高位**复制：候选 `1<<20 / 1<<18 / 1<<16 / 1<<14` 从高到低，选中后仍断言 floor ≥ `1<<12`，全失败则 panic 并打印 ulimit，绝不退回低号。本环境实测 `ulimit -Sn` / `ulimit -Hn` 均为 524288（soft=hard）→ `1<<20` 正确落空（超上限）、**实际选中 `1<<18`**。Drop 后的 EBADF 断言强度不变。
2. 熔断加固 5s→60s：relay_client_e2e / relay_e2e / relay_connector_e2e 的 FUSE，connector / mgmt_socket / signal_session / sync_stream 的 `wait_for`，signal_channel 的 `wait_accept` / `wait_dead`，以及 tests/management_grpc.rs 的测试局部 CONNECT/REQUEST_TIMEOUT。这些是**挂起保护**而非时延断言——放宽它们不改变任何通过条件；生产侧 `connector.rs` 的 `DEFAULT_CONNECT_TIMEOUT=5s` / `DEFAULT_REQUEST_TIMEOUT=10s` **未动**（可 grep 复核）。
3. 两处 relay_client_e2e 瞬态断言纠正（语义变更）：`server_disconnect` 谓词补 `dial_attempts==2` 并等待 server accepted==2；`auth_rejection` 旧断言 `[2s,4s]` 会在第三条 8s backoff 落盘的微秒瞬态失效（push 发生在每轮 arm 时刻，`src/relay_client.rs` 约 1672 行附近），改为等第三条落盘后断言 `[2,4,8]`——既确定又更强。
4. 同轮新增 `flake-check.sh`（见 A2）。

**第二轮 `57be0da`（A–E 五项残余）**

- **A** `wg_e2e::endpoint_change_switches_the_path_and_keeps_traffic_flowing`（原 1/30）：真根因是**真实 fd 双重 close → 号复用误杀**。ICE 测试里 `fed_a.raws[0]` 被 `Node::adopt`（`src/wg_device.rs` 的 `WgDevice::adopt`，fd 归 Node、由其 drop 关闭）与测试本地的 `FedSocks::drop`（`tests/wg_e2e.rs`）**双重所有**；第二次 close 落在已被内核回收复用的 fd 号上，杀掉并行测试刚 bind 的 socket——探针实测 `sendto` 返回 EBADF errno=9。（提交信息曾写的"断言假设/端口复用"原假设被实证推翻：`assert_ne!(new_port, old_port)` 在 500+ 观察轮从未命中，旧 socket 未关时内核不复用该端口。）修法：`raws.remove(0)` 所有权移交，不变量"每 fd 恰一 owner"；未放宽任何断言。复现需 4 进程并发的整二进制压力：修复前 320 轮 2 失败 → 修复后 320 轮 0 失败。
- **B/C** `connector::napi_global_start_status_stop_roundtrip` 与 `mgmt_socket::start_with_socket_refusal_gate_and_feed`：瞬态状态断言改为**可达集不变量** `state ∈ {connecting, reconnecting}`（首拨构造性拒连，Connecting→Lost→Reconnecting 合法随时发生；该窗口内 Connected 需 login 成功、Failed 需退避预算耗尽，均不可达）。
- **D** `signal_channel`：`wait_dead`（及 `recv`）的每次阻塞读包进 `timeout(FUSE=60s)`——原 deadline 只约束"有帧继续到达"的循环，对静默阻塞的读永不触发，曾导致一次 22 分钟静默挂起。注释写明：guard 触发即应视为真实缺陷上报。
- **E** `tun_fd_contract::backpressure_partial_write_budget_then_drain_completes`：判定为**测试时序假设**而非生产缺陷——读者退出条件 `seen >= total` 把 phase-1 残留（约 212,992B）计入，读者可在写者还剩尾部时退出，残留 + truesize 开销使 POLLOUT 在预算内永不就绪（实测 `Backpressure{written:292352}` 与模型自洽）。修法：读者改为 drain 到 `grand_total`（两次写之和），断言改 `assert_eq!(seen, grand_total)` 的**字节守恒**；生产侧行为不变（对端停读 → 预算耗尽返回）。⚠️ `57be0da` 的提交信息在此处被截断，本段的收尾结论系依据提交 diff（tun_fd_contract.rs 的 `grand_total` 改动与其注释）重建。
- 同轮 `flake-check.sh` 补日志保留改动（每轮唯一时间戳，见 A2）。

### A4. 验收数字与复核命令

以下数字均应能用仓库内命令复核；未标注"本机复跑"的来自两个提交的记录（`git log --format=%B -n 2 ebd9d30`、`git log --format=%B -n 1 57be0da`）。

| 数字 | 含义 | 复核命令 |
|---|---|---|
| 8/100 轮失败、5 个测试 | 治理前的压测发现 | 提交记录；可用 `bash client/core/flake-check.sh 100` 复现量级 |
| 隔离 220 次全过 | 证明非自身逻辑错 | 提交记录 |
| fd_bag 3/100、endpoint_change 1/30 等 | 各项修复前的失败率 | 提交记录 |
| 修复前 320 轮 2 失败 → 修复后 320 轮 0 | A 项修复的压力对照（4 进程并发） | 提交记录；重跑需同型并发压测 |
| 修复后 flake-check 0/30 | 两轮修复后的验收 | `bash client/core/flake-check.sh 30`（本节写入时未重跑 30 轮，数字取自提交记录） |
| 29 个测试目标、455 passed / 0 failed | 当前全量基线 | `cargo test --offline --locked`（**2026-09-14 本机复跑确认**） |
| ulimit soft=hard=524288、选中 `1<<18` | fd 高位复制的环境依据 | `ulimit -Sn; ulimit -Hn`（已实测）；选中值可读 `src/host_sockets.rs` 候选表推算 |
| 生产超时 5s/10s 未动 | 挂起保护非时延断言 | `grep -n DEFAULT_CONNECT_TIMEOUT client/core/src/connector.rs` |

### A5. 已知残余与存疑（如实保留）

1. **endpoint_change 的每轮具体断言路径属推断**：EBADF（errno=9）有探针实测，修复后压测零复现，但"哪一轮哪个断言被误杀"的逐轮路径没有逐轮日志可指证，属合理推断。
2. **`tun_fd_contract::close_once_double_close_and_use_after_close`**：仅在 4 倍超压下 2/100 失败（ledger diff 挑错 inst 行），修复另行进行。**截至本记录写入（HEAD=`57be0da`，工作区干净）该修复尚未提交**——`git log -S close_once_double_close_and_use_after_close -- client/core` 仅命中其创建提交 `5ca19dd`；若读到此记录时已有新提交，以 git 历史为准。
3. **生产侧是否有真实竞态**：无可指证证据。两轮修复全部落在测试侧（除 A 项所有权不变量本身是真实代码合同的澄清），不据此断言生产代码有或无竞态。

### A6. 结论

flake 是**证据质量问题**，不是"测试琐事"：8% 的失败率意味着任何一次"通过"都可能是运气，基于它的验收、回归与 T0 裁决全部失去意义。压测轮数、失败证据（测试名+断言原文）与所有权/可达集级别的根因修复，是把"通过"重新变回证据的最低配置。
