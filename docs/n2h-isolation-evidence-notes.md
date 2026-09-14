# N2-H 隔离证据工具（route-exclusion isolation evidence）— 实现说明

状态：主机侧可开发、可测试、已离线验证；**不改变治理条款**（见 §6）。
工具入口：`nbinterop isolation-check`；核心模块：`client/core/src/n2h.rs`；
测试：`client/core/tests/n2h_isolation.rs` + `n2h.rs` 内单元测试。

## 0. 背景与依据

- 真机已证实 `VpnConnection.protect(fd)` 与 `protectProcessNet()` 均被
  `ohos.permission.MANAGE_VPN` 权限校验挡住，且该权限不可申请、声明即安装失败
  （9568289）→ 原治理条款 §二.4（逐 socket protect + fail-closed）在本平台
  **不可满足**，记录为 UNSAT（见证据文档固定字段 `unsat_note`）。
- 跨厂商 T0 裁定（2026-09-14，`gpt-5.6-sol` 席）准许"隔离的路由排除测量"，
  并给出草案条款 **N2-H** 与七条证据要求；本工具即该证据要求的可执行实现。
  **N2-H 正式采用须用户批准并修订治理条款**；在此之前 `n2h-pass` 不是任何
  治理判据的 pass。

## 1. T0 证据要求 → 工具字段（判据映射）

| # | T0 要求 | 工具实现 / 证据字段 |
| --- | --- | --- |
| 1 | 每类协议首连/重连唯一五元组+唯一载荷探针 | `frozen_endpoints`（连接前冻结，7 类：management/signal/stun/turn/relay/wg_peer/dns，各含 kind/host/port/proto/source/resolved/frozen_at）；`probes[]`（每端点一条，`probe_id = N2H-<kind>-<nonce>`，nonce 取自 /dev/urandom；`tuple.proto/src/dst`——每条探针从**独立临时 socket** 发出，五元组唯一；`payload_marker`（ASCII probe_id 或 STUN 12B txid，hex 呈现）；`expected_path:"outer"`）。重连/撤销后复验 = 重新执行本工具（每个证据文档只覆盖一个会话窗口） |
| 2 | TUN 包级负证据 | `tun_negative`：解析 TUN 帧（IPv4 version/IHL/proto/5-tuple + 载荷），对每帧做探针唯一载荷的**全帧逐字节子串匹配**；`hit:false` + `frames_scanned` + `sides` + `method`/`criteria` 说明随文档走。任一 outer 探针载荷命中 → `n2h-fail` |
| 3 | 隧道正控 | `tunnel_positive_control`：发往对端 **overlay IP** 的探针（`expected_path:"tunnel"`，双向 a2b/b2a），必须在对端 TUN 侧观测到（`observed_in_tunnel` + 每条 `observed_in_tunnel` 命中帧的五元组/长度/side）。**正控缺失/未观测 → 永不 pass**（`n2h-inconclusive`） |
| 4 | 端点侧证据 | `endpoint_side[]`：每条 sent 探针的接收证明。三种方法：`sink-record`（UDP/TCP mock/运维 sink 记录载荷+`recvfrom`/`peer_addr` 物理源址）；`stun-binding-response`（txid 匹配的 Binding 响应，XOR-MAPPED-ADDRESS = 端点报告的物理源址，复用 `crate::stun` 真 codec）；`peer-device-offpath-drop`（对端 WG 设备外层 socket 的 `unknown_peer_drops` 增量——旁路数据报在对端 demux 处被计数）。`physical_src_source` 明确标注源址来自端点报告还是探针 socket getsockname |
| 5 | 计数对账 | `counters[]`：隔离窗口前后各一次接口快照（before/after/delta）。设备侧 = 真 `WgDeviceStats`（tx/rx 包数字节、握手、各类 drop）；TUN 侧 = 采集器独立清点的注入帧数/交付帧数/交付字节。**对账恒等式：`delta.wg.rx_bytes_to_tun == delta.tun.delivered_bytes`**（设备"解密写入 TUN 字节"vs 采集器"TUN 接口实际收到字节"，两条独立口径）；不等 → `n2h-inconclusive`。为使 `connector_status()` 也能读取字节计数，本次给 `WgDataplaneStatus` 增补了 `tx_bytes`/`rx_bytes_to_tun`（§5） |
| 6 | 撤销后复验 | 工具按"单会话窗口"产出证据；VPN 撤销/重连后**再次执行本工具**即复验（`residual_scope` 中明确声明本证据不跨窗口） |
| 7 | 路由表/配置回读只作辅助 | 工具**不读取、不产出**路由表/`/proc/net/route`/API 返回值作为判据；接口总计数也只作为对账辅助（§1#5 的恒等式须与 #2/#3/#4 同时成立），单独不足 |

记录禁词：证据文档不含 `protect pass`、`N2 pass`、`等同逐 socket protect`、
`waived`、`N/A`（单元测试与集成测试断言）；原义务固定记
`unsat_note = "逐 socket protect: UNSAT/未满足（MANAGE_VPN 受限 + promise 不 settle）；替代: route-exclusion；非原 N2 判据 pass"`。

## 2. 用法与最小复现命令

```bash
# 构建 CLI（host-only）
cd client/core
export RUSTUP_HOME=/home/worker/rust/rustup CARGO_HOME=/home/worker/rust/cargo
export PATH=$CARGO_HOME/bin:$PATH
cargo build --offline --locked --features cli --bin nbinterop

# ① 正常路径（默认离线双实例 loopback 会话；合成固定测试密钥，仅 loopback sink）
target/debug/nbinterop isolation-check
#    → exit 0，写 ./n2h-isolation-evidence.json（stdout 同步输出 JSON）

# ② 反例 A（外层探针注入隧道 → 必须 fail）
target/debug/nbinterop isolation-check --fault-leak --out /tmp/n2h-leak.json

# ③ 反例 B（正控发往 allowed_ips 之外 → 必须 inconclusive）
target/debug/nbinterop isolation-check --fault-no-posctl --out /tmp/n2h-nopos.json

# ④ 端点枚举缺失（signal 未取到 → 必须 inconclusive，fail-closed 不建连）
target/debug/nbinterop isolation-check --fault-omit-endpoint signal --out /tmp/n2h-omitsig.json

# ⑤ 真实部署的建连前冻结检查（读 config 的 management_url + CLI 补充端点；不读密钥）
target/debug/nbinterop isolation-check --config deploy.json \
    --outer-endpoint stun:stun.example:3478 --outer-endpoint dns:10.0.0.1:53
#    （无隧道会话 → verdict 恒为 n2h-inconclusive；STUN 响应可作为端点侧证据）
```

测试：`cd client/core && cargo test --offline --locked`（新增
`tests/n2h_isolation.rs` 四场景 + `n2h.rs` 11 项单元测试 +
`host_sockets.rs` 帧边界回归测试）。

## 3. verdict 语义

| verdict | 语义 | 退出码 |
| --- | --- | --- |
| `n2h-pass` | 全部证据腿成立：冻结完整、探针全发、TUN 负证据零命中、正控全部命中、端点侧收执全部观测、计数对账全部一致 | 0 |
| `n2h-fail` | **直接证伪**：任一 outer 探针唯一载荷出现在任何 TUN 帧（路由排除泄漏）。证伪优先于一切 inconclusive | 1 |
| `n2h-inconclusive` | 无法确立也无法证伪：冻结缺口（必需类缺失/可选类无理由缺席）、探针未发出、正控未发或未观测、端点侧收执缺失、计数对账不平 | 20 |

判定顺序：命中 → fail；否则任一缺口 → inconclusive（全部缺口列进 `reasons`）。
必需端点类 = management/signal/stun/wg_peer/dns（NetBird 会话必存在）；
turn/relay 为可选类——缺席时必须给出 `absent_reason`（如"本部署未配置"），
无理由缺席同样记缺口。

## 4. residual_scope（固定随文档输出）

- 域名/CDN：按冻结时刻解析结果枚举；重解析/换址后的新端点不在证据内
- DNS 上游：DNS 查询的上游递归路径不受观测
- 动态候选：srflx/relayed（STUN/TURN 派生）候选及其外层映射未在主机侧覆盖
- 重连换址：单会话窗口证据；撤销后复验/重连须重新执行
- 非 LAN/NAT 拓扑：主机侧 loopback 不代表 NAT/多宿主物理路径
- 宽前缀旁路：仅证明被枚举端点（/32 冻结）零命中；更宽前缀旁路不覆盖
- 端点侧日志：真实服务端投递日志须运维侧提供；主机侧只覆盖 sink/对端设备计数
- IPv6：TUN 帧解析与端点枚举均为 IPv4-only

## 5. 顺带修复的既有缺陷（本次变更内的最小修复）

1. **TUN 替身帧边界缺陷（实质缺陷，影响一切包级测量）**：
   `host_sockets::open_tun_standby_pair` 原用 `UnixStream::pair()`
   （SOCK_STREAM）——背靠背两次 write 会被**合并成一次 read**，设备把两帧当
   一帧（实测：80B+63B 合并为 143B 的"帧"）。真内核 TUN 是包保真的，包级
   负证据因此可能漏检/误检。修复：改用 `AF_UNIX SOCK_SEQPACKET`
   socketpair（一次写 = 一个数据报 = 一次读，与真 TUN 契约一致）；
   回归测试 `host_sockets::tests::tun_standby_pair_preserves_frame_boundaries`，
   反例 A 集成测试同时长期守护该行为。
2. **接口字节计数未上抛**：`WgDataplaneStatus`（`connector_status().wg`）原只
   暴露 tx/rx 包数，无字节数——N2-H 计数对账（按字节）在设备路径上无从读取。
   修复：增补 `tx_bytes` / `rx_bytes_to_tun` 两个直通字段并更新
   `to_json`、`dataplane_status()` 与相关测试断言（connector.rs 两处、
   wg_device.rs 一处）。

除上述两处外未改动任何既有生产逻辑；`client/entry/**`、`spikes/**`、治理文档
零改动。

## 6. 治理声明（绑定）

- 本工具**不改变治理条款**。原 §二.4 逐 socket protect 义务在本平台为
  **UNSAT/未满足**，本工具与任何输出都不构成对该义务的豁免（"waived" 一词
  被测试禁止出现在证据中）。
- `n2h-pass` **不是**原 N2 判据的 pass；它只是"路由排除隔离"这一替代机制的
  单会话观测证据，且带 `residual_scope` 明示未覆盖面。
- N2-H 要生效，必须：用户显式批准 + 正式修订 `docs/native-nx-governance.md`
  等治理文件。在此之前，本工具的定位是"为该讨论提供可执行证据"，仅此而已。
- 凭据纪律：loopback 模式只用合成固定测试密钥；`--config` 模式只以普通
  JSON 读取 `management_url`，不读 setup key/JWT/私钥/CA。
