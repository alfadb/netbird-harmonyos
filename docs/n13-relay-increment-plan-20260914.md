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
