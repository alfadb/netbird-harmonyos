# host n2h-peer 逐 probe_id 回执日志（2026-09-20）

## 问题：`a2b_seen` 不是「某个 nonce 被收到」的证明

`nbinterop n2h-peer` 的 `a2b_seen` 只统计**形状合法**的 posctl 帧：
`n2h_peer_drain_and_echo` 用 `nc::posctl_marker` 判定，而该 helper 只认
`N2H-posctl-<24 hex>|positive-control`，不绑定本次 run 公告的 marker，也不比对
src/dst。因此 `a2b_seen=1` 既可能来自被测设备，也可能来自别的 peer 的同类探针、
甚至本机 b2a 从 WG 回环回来的帧；`last_a2b_from` 只有源地址，没有 id。

0019 即此情形：host 记到 `a2b_seen=1/echoes_sent=1/last_a2b_from=100.108.171.38:42563`，
但没有记录收到的 marker，无法把这次接收归因到设备 a2b 的 probe_id。

## 变更（仅 `client/core/src/bin/nbinterop.rs`）

- `n2h_peer_drain_and_echo`：`posctl_marker` 的成功结果绑定为**实际收到的 marker**，
  每个合法 posctl 帧最多发**一条**结构化回执（普通流量不输出）。
- 行格式（沿用 `nbinterop: <token>|k=v` house style）：

  `nbinterop: n2h-peer-receipt|probe_id=<收到的 marker>|proto=17|src=<ip:port>|dst=<ip:port>|echo=written|self_marker=false|unix_ms=<host SystemTime 毫秒>`

  - `echo` ∈ `written` | `no-own`（无 overlay 地址）| `write-failed(errno=N)`（只 errno）。
  - `unix_ms` 取真实 `SystemTime`；时钟早于 epoch 时写 `unavailable`，不伪造 0。
  - `self_marker` 只表示「收到的 id == 本 run 公告的 marker」，**不是**设备归因；
    其它 id 一律如实写自己的值。
- 只输出 `probe_id`（校验过的 ASCII marker）+ 五元组 + 结果；**绝不输出整 payload、
  私钥或 setup key**。
- 既有转发行为与计数语义不变（同一写回、同一 `a2b_seen`/`echoes_*` 增量，旧人读
  `[n2h-peer]` 行保留）。
- `N2hPeerState.receipts`：最近 64 条回执的进程内环，供 `--loopback` 自检回读；
  stderr 已逐行输出，不构成第二条投递路径。

## 用途

把 host 回执的 `probe_id` 与设备 evidence 的
`tunnel_positive_control.probes[].probe_id`（a2b/b2a）一一匹配；匹配上才算该 nonce
真的到达 host。`a2b_seen` 仅作形状级计数参考。

## 验证（host-only，全离线）

- `cargo build --offline --locked --features cli --bin nbinterop` → rc=0。
- `cargo test --offline --locked --features cli --bin nbinterop` → 17 passed, rc=0。
- 新增/扩展测试：外来 marker（cd…）回执写 cd… 而非配置 b2a id；
  `--loopback` 真收包回执 == 随机 a2b payload id 且 != FIXED、src/dst 正确；
  收到本机 b2a marker 写自身 id 且 `self_marker=true`、计数与 echo 不变；
  非 posctl 不生成回执。
- `--loopback` 实测（合成 ID）：公告 `b2a_probe_id=N2H-posctl-c15d…`，回执
  `probe_id=N2H-posctl-dfbc…|echo=written|self_marker=false|unix_ms=1789886108063`。

## 未触范围

设备侧 ETS/collector、core 转发、路由、协议、既有统计字段语义均未改；无设备操作，
未重建/签名 HAP，冻结设备包不动。
