# DNS 回执路径①接线：`dns-nxdomain-echo`（v4 生效稿 §3 / §6.2 项 2）

日期：2026-09-20 ｜ 代码：`client/core/src/n2h_collector.rs` ｜ 状态：实现落盘，**真机未验**

## 依据与开关

- v4 生效稿 §3 路径①（2026-09-20 已决）：启用 `dns-nxdomain-echo` 并在回执链中
  **消费** `echo_received`——qname 回含 marker ⇒ 该 dns 探针收执成立；miss 仍记
  缺口、永不单独构成通过。§6.2 项 2：发送接线＋判定链消费，真机重新验证。
- 新增 arm 选项 `dns_nxdomain_echo`（`ArmOptions`，JSON 键同名）。
  **默认 `false`**＝既有行为逐字不变：裸 ASCII `N2H-dns-<nonce>` 载荷、不等待
  回执、`endpoint_side[]` 记 `method:"none"`＋operator-side 缺口。`true`＝证据轮
  显式启用；非布尔值拒绝 arm（fail-closed）。启用时 arm 写 notes 行
  `dns-nxdomain-echo=enabled`（缺席＝未启用，可机械核对）。
- 壳侧已接线转发（2026-09-20）：`N2hArmFileOptions` / `N2hArmOptions` 各加可选
  `dns_nxdomain_echo?: boolean`，在 `peer_overlay_ip` 同款转发布点按键存在原样转发
  （缺键不注入 `false`、不默认、不类型转换，严格布尔校验留在 core）——键面属
  v4 §6.2 实现项。

## 发送形态变更（仅 dns kind 且开关开启时）

- 载荷：裸 ASCII → 标准 DNS A 查询（RD、QDCOUNT=1、无 EDNS），qname=
  `<probe id>.n2h-invalid`；**probe id 仍逐字在 qname 首标签**，TUN 扫描的
  `WireMarker::Ascii` 匹配不受影响。数据报数不变（每窗口 1 个）、socket 形态
  不变（每探针独立 wildcard `0.0.0.0:0`）、目标不变（冻结 resolved 端点）。
- 新增一次**有界回读等待**：发送后同 socket 等待至多 `probe_wait_ms` 并逐包做
  回执判定（与 STUN 发送内等待同形；窗口 deadline 检查仍逐探针先行）。
- 其它 kind 及开关关闭时的 dns 发送形态逐字不变（有测试断言）。

## 回执链消费点（`initial_receipt`，发送后首个 `endpoint_side[]` 条目）

- `kind==dns && dns_nxdomain_echo` 时消费 `sent.echo_received`：
  - **命中**（QR 置位＋query-id 匹配＋线格式 qname 逐字回含；RCODE 不要求，
    §4.0）⇒ `observed:true`、`method:"dns-nxdomain-echo"`，qname 逐字回含即证据；
    **据实改记（§3 口径）**：本路径**不填** `physical_src` / `physical_src_source`
    ——DNS 回执只证明投递，响应来自 resolver，地址不是端点观测到的探针源址；
    v4 §3 的五字段契约是给**外部回执 bundle**（mgmt/signal/relay）的，DNS 属
    不同回执形态，故源址字段留空，**不借用** `endpoint-reported` /
    `probe-socket-getsockname`（两 token 均未使用）；源址证据仍需运维日志。
    探针 socket 的 getsockname 读数只留在 `probes[].tuple.src`，不进入任何
    `physical_src*` 字段位。新增一行 notes：
    `dns-echo-receipt-physical-src=not-provided|no endpoint-observed physical
    source on this path (a DNS echo proves delivery only); source-address evidence
    still requires the operator log — a different receipt form from the external
    receipt bundle`。
  - **未收到/不匹配/截断/超时** ⇒ `observed:false`＋miss 缺口，永不单独通过。
- 窗口 notes 行（§6.2 项 2 完成判据面）：`dns-echo-receipt=hit|probe_id=<id>`
  或 `dns-echo-receipt=miss|probe_id=<id>|gap`。
- 其余判定链原样：stun 自证、wg_peer 对端计数、mgmt/signal/relay operator-side
  缺口、tun_negative/C4、`probe_receipt` seam；未新增设备命令或权限。

## 测试（离线、真实代码路径，未连设备）

新增 5 单测：开关默认 OFF/非布尔拒绝；OFF 线字节＝probe id 逐字（回归）＋ON=
A 查询含 id＋ON 不外推 wg_peer；错名/截断/乱码/超时 ⇒ 缺口；命中⇒observed＋
具名 method（`physical_src`/`physical_src_source` 均空，输出不含
`endpoint-reported`/`probe-socket-getsockname` 任一 token，且无
`physical_src_source=<token>` 赋值——经 `n2h::Evidence::to_json` 真实渲染
断言）、miss⇒缺口、relay 不受影响、OFF 时 dns 条目与既有 `method:"none"` 缺口
逐字相等、stun miss 原样。全量 `cargo test --offline --locked --no-fail-fast`
（client/core）：**566 通过 / 0 失败**（基线 561＋新增 5；首轮无 `--no-fail-fast`
时 `connector::tests::relay_lane_open_commit_is_atomic_against_client_swap`
在并发负载下偶发失败，单独重跑与该轮重跑均通过，与本变更无关）。

## 仍未验证（真机待验）

- **真实 resolver 行为**：需真机窗口出现 echo 命中行与 miss 缺口行各一（真实
  resolver 返回 NXDOMAIN 且逐字回含 qname），方可按 §6.2 项 2 判读；本实现不
  声称该回执链在真机成立。
- 真实网络中间盒对 `*.n2h-invalid` 的处理（劫持/改写/缓存 ANSWER）未测；命中
  语义以「回含我们的 qname」为准，RCODE 不判。
- host CLI 采集路径（`n2h.rs:1347` 探针闭包）不在本变更范围（§3/§6.2 指设备
  采集器），其 dns 探针仍为 ASCII 形态。
