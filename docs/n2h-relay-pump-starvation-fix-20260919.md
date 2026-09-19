# n2h relay 泵饿死修复 — 0018 会话无法建立的分析与修复（2026-09-19）

## 源代码缺陷（已静态确认 + 离线复现）

`client/core/src/connector.rs` 生产泵 `relay_carrier_pump` 每轮在进入 select/pump
之前**先顺序 await 完整轮 `ensure_lanes()`**：对 peers 表逐个 `ensure_lane().await`，
内部 `RelayClient::open_conn().await` 停在 §4.1 等待直到对端上线或
`OPEN_CONN_TIMEOUT=30s`（relay_client.rs，会话注入时钟判定）。任何一个从未 online
的 peer 都把整条泵停满 30s/轮；多个离线 peer 时更久。已到达的入向 Transport 帧
（client inbound 队列）在此期间完全不被 ingest → WG 无数据 → 永不握手 → session
无法建立。修前红测试证明：帧已到达 client（transport_rx_bytes 增长）但 2s 有界窗内
frames_in 恒 0，直到虚拟时钟跨过 30s 才被处理。

## 0018 佐证（与症状一致）

- host `transport_bytes=10456` ≈ `wg.carrier_tx_bytes=6068` + 收向 ≈4388B
  Transport：**server→client 方向确有数据到达 relay client**（reader 独立于泵）。
- `wg.rx_packets=0` 且 `decrypt_errors=0`：`Wg.handle_carrier` 一入即 RX++ 或
  unknown_peer_drop++，二者皆 0 ⇒ 数据根本没进 WG（泵没 ingest），**不是**映射
  miss 后被丢弃，也**不是**双方 ready 死锁（tick 有 path 会主动握手）。
- 6 peers 中 3 个未 online：泵每轮先被离线 peer 的 open 挡住，与症状时序吻合。

**仍未排除**：若泵修复后真机仍不通，剩下的候选是 sender→peer key 映射全 miss
（例如对端 relay peer id 与本端 `PeerId::from_wg_pubkey_string` 推导不一致，或
0018 对端实际未向本机发数据）。新增的一次性日志（见下）即为区分这两者而设。

## 调度方案（最小实现，全在 connector.rs `RelayCarrier`）

- 泵循环 `Ready` 分支由 `ensure_lanes().await` 改为 `spawn_pending_lanes()`（同步、
  不 await）：对每个需要开 lane 的 peer 最多 spawn 一个 `ensure_lane` 任务
  （`pending_opens` 单飞）。**任何 open_conn await 都不在泵路径上**，锁均为短临界区，
  泵 select/pump 每轮照常跑；失败仍走既有 `LANE_OPEN_RETRY=5s` backoff 与原 30s 超时
  ⇒ 无每 tick 取消重发/风暴。
- **commit 原子性（T0 复审项 1）**：`commit_lane_open` 把「有效性复查 + WG attach +
  attached 表插入」放进同一个 `lifecycle: Mutex<bool>` 临界区，与全部失效迁移
  （`attach_client` 换 client / `set_peers` 移除 / `shutdown`）互斥：迟到的 open
  要么看到失效状态丢弃，要么先提交、随后失效方在同一锁内 detach——已删除/已停止
  的 carrier 不可能被迟到 open 复活。不持 lifecycle 跨 await；锁序固定
  lifecycle → 单个叶锁（client/peers/attached/pending/retry/stats），无叶锁持有者
  反取 lifecycle（无 ABBA）；seam 回调不回环进 RelayCarrier。
- **pending 代际簿记（T0 复审项 2）**：pending 条目为 `(gen, JoinHandle)`
  （`AtomicU64` 计数器），任务收尾 `finish_open(key, gen)` 只删自己代次的条目——
  修复裸 key 尾删的 ABA（旧任务被 drain+abort 后若已过末次 await 不可取消，其尾删
  会误删同 key 新任务的注册，使其脱离 abort/shutdown 管控并破坏单飞）；spawn 即时
  完成无害：finish no-op，残留 finished handle 由每 tick reap 清理。
- **登记原子性（T0 复审残留收口）**：`spawn_pending_lanes` 整个无 await 过程持
  lifecycle guard（未武装直接返回——穿越 teardown 的旧泵同步段无法在 shutdown 后
  登记孤儿任务）；retry/attached 叶锁检查各自完成后，对 pending_opens 一次加锁，
  同一临界区内 vacant 检查 → tokio::spawn → insert——双泵 check-miss 相互覆盖
  handle 不可能；spawn 不 await/不轮询 child，持 std Mutex 覆盖安全；叶锁不交叉
  嵌套，finish_open 代际保留。
- 取消/清理：`set_peers` 移除 peer 时 abort 其 pending 并清 backoff 条目；
  `attach_client` 换 client、`shutdown()`、泵退出统一 `abort_pending_lanes()`；
  每 tick `reap_finished_lanes()` 兜底收尸（panic 残留）。
- 未改：PeerId 算法、wire 协议、crypto/握手阈值、ready 闸、TUN tee、
  冻结 connector_status/relay JSON 形状、信任边界（unknown sender 照旧只计数）。
- 诊断日志（有界一次性/受 backoff 限速，peer 仅公开 `sha-` 形式）：lane open
  `waiting|peer=` / `failed|… class=` / `resolved late, dropped`，一次性
  `first inbound frame matched|peer=` 与 `first unknown-sender frame dropped|
  sender=`——离线日志区分「泵未运行」与「映射 miss」。

## 测试（真实泵 + 假 server + 注入时钟，非简化复制品）

`client/core/src/connector.rs` 内部测试：生产 `ensure_pump`/`relay_carrier_pump` +
真实 `RelayClient` + `crate::relay_testserver`，VirtualClock 冻结 §4.1 预算：

1. `relay_pump_ingests_inbound_while_another_lane_open_is_still_pending`：在线 peer
   lane 附着后、离线 peer open 挂起期间注入入向帧，断言有界实时内 ingest 到 WG
   （键=发送方 pubkey）、open 仍挂起；时钟跨 30s 后 typed 超时 + backoff；泵继续
   收帧；未知 sender 只计数不进 WG。修前红/修后绿。
2. `relay_pump_pending_lane_open_ends_on_peer_removal_and_on_stop`：pending open
   在 peer 移除与 shutdown 后被终止，对端上线也不产生迟到 attach。修前红/修后绿。
3. `relay_lane_open_commit_is_atomic_against_shutdown` / `..._against_client_swap`：
   seam 的 `attach_carrier` 停住「已过 await、正在提交」的任务，另一线程调
   `shutdown`/`attach_client`，断言互斥阻塞、放闸后 **active lane 余额**（HashSet，
   非 attach 历史）为空、lane_count==0、无未受管 pending；swap 另断言新 client
   生效。修前红（teardown 提前完成、lane 复活）/修后绿。4. `pending_open_unregister_is_generational`：先登记 gen2 再 `finish_open(k, gen1)`
   ⇒ gen2 仍在且仍受 shutdown 管控；匹配代次才自删（白盒）。
5. `pending_open_registration_is_atomic_and_gated`：同 key 连续两轮 spawn 只有一个
   pending、gen 不被替换；未武装与 shutdown 后的 spawn 均登记为空（白盒）。

输出存档 `/tmp/n2h-0018-*.log`：red、commitrace-red（修前各 2 failed）；green-newtests、commitrace-green、registrn-green（修后全绿）；full-after-r3（终态全量）。

## 回归

`cargo test --offline --locked`（环境约定同上）：34 个目标 **550 passed / 0 failed，
rc=0**（lib 315 = 309 原有 + 6 新；relay 套件 relay_client_e2e 12 /
relay_connector_e2e 6 / relay_e2e 12 / relay_presence_refresh 6 /
relay_advertised_urls 4 / wg_over_relay_n13 5 全绿）。`cargo check --features cli
--bin nbinterop` rc=0。工作树仅改 `client/core/src/connector.rs`；未 commit；未做
HAP 构建（按约定代码验收后统一构建冻结）。

## 真机待验

- 0018 场景重放：预期日志序列 `lane open waiting`（离线 peers）与
  `first inbound frame matched` 并存，`wg.rx_packets` 开始增长，握手完成、
  session 置位。
- 若仍不通：看是否出现 `first unknown-sender frame dropped`（sender 映射问题）
  或全无入向标记（对端/链路问题），沿此二分支排查。本修复不声称已证 0018 唯一
  根因——离线回归只证明「串行 open 等待可饿死已到达入向」该缺陷真实存在且已修复。
