# N2-H collector peer overlay CIDR 错配修复与 a2b 目标选择（2026-09-18）

- 性质：core 增量（两轮）。仅改 `client/core/src/n2h_collector.rs`、`client/core/tests/n2h_device_collector.rs` 与本文；未动 ETS/AUTH/冻结包/connector；未设备操作、未 commit、未做完整构建。
- 证据：同仓 B diagnostic 目录 Evidence 0017 解码 exit 0，但 a2b `sent=false`，理由 no peer overlay。`wg.ready` = 至少一个 peer `session_established && !expired`，与 carrier/direct 无关；同一 `Arc<WgDeviceFeed>` 与 tee 无实例错配（已核）。

## 1. 已证格式错配（第 1 轮根因）

- 生产端：`connector.rs` `route_network_string` 恒以 `a.b.c.d/p` 渲染 `peers[].vpn_addresses[]`（如 `100.108.126.29/32`），无裸 IP 形态。
- 消费端：peer overlay 循环此前拼 `"{s}:0"` 交 `parse_addr_literal`（SocketAddr），`"ip/prefix:0"` 全失败 → `peer_overlay` 恒空 → a2b 无目的地。旧 fixture 裸 `10.77.0.2` 掩盖了错配。
- 修复：局部 helper `peer_overlay_ipv4`（仅用于 vpn_addresses）：合法 `/0..=32` 取地址、裸 IPv4 兼容；IPv6/坏前缀/主机名/坏八位组一律 `None`。`parse_addr_literal` 与 outer endpoint 解析未放宽；`address`(own_overlay) 未动；理由措辞改为 `no usable IPv4 peer overlay (network-map peers[].vpn_addresses)`。

## 2. a2b 目标选择（第 2 轮：已实测 6-peer 网络，列表顺序不可作选择规则）

- `ArmOptions` 新增可选非秘密 `peer_overlay_ip`：裸点分 IPv4 字符串，arm 时经 `config::parse_ipv4` 严格解析为 `[u8;4]`；CIDR/IPv6/主机名/错类型/坏八位组直接 invalid-opts。证据文档形状未加字段（选择结果由 a2b 实际 `tuple.dst` 呈现）。
- `build_posctl` 经纯函数 `select_posctl_peer_target` 选择：显式 pin 在 freeze facts 可用集合内才用；pin 不在集合 → 有理由缺口（`not among …`），不回退 first；无 pin 且去重后恰一个 → 自动选择（单 peer 旧行为不变）；多个候选 → `ambiguous-peer-target: N usable …` 缺口，不任意选；零个 → 原 no-overlay 缺口。
- 未改：WG ready 闸、direct/carrier 语义、tee、packet 算法；`sent=true` 仍只出现在真实/mock send 成功路径。

## 3. 红/绿测试（实际运行）

- 第 1 轮红：fixture 换 `"10.77.0.2/32"` 后 `process_freeze_facts_map_both_connector_documents` 实跑 `left: []`。绿：同例过；新增 `peer_overlay_vpn_addresses_take_cidr_addresses_and_refuse_non_ipv4`（CIDR/裸 IP/`/0`/空白、`/33`、`/abc`、空前缀、IPv6、主机名、坏八位组、文档级混合列表）。
- 第 2 轮：新增 `posctl_peer_target_is_pinned_or_unique_and_never_list_order`（2-peer fixture 第一项非目标、pin 选第二项；pin 不在集 → Err；无 pin 多候选 → `ambiguous-peer-target` 含数量；单/去重/空集）与 ArmOptions pin 正反例。新增 3 个集成测试：`a2b_pin_selects_the_second_peer_address_and_the_probe_really_leaves`（真实发送、`tuple.dst=127.0.0.1:<port>`、echo 观测）、`a2b_without_a_pin_and_several_candidates_stays_an_ambiguous_gap`、`a2b_pin_outside_the_freeze_facts_stays_a_gap_not_a_fallback`。单 peer 旧行为与 CIDR 生产格式回归由既有用例覆盖。
- 全量：`cargo test --offline --locked --color never` 全绿，第 2 轮实际计数 **544 passed; 0 failed**（lib 309 + 33 个集成/文档二进制；n2h_device_collector 19），exit 0。**未做完整构建**，待与并行 ETS iface 探针合并后统一构建（该统一构建已于 2026-09-18 合并验收完成：`bash client/build.sh` exit 0）。

## 4. 边界与残余风险

- **壳接线**（本文成文时未做；2026-09-18 合并验收时已完成）：ETS 桥接已由 `NetBirdVpnExtensionAbility.ets` 把 arm 选项中的 `peer_overlay_ip` 原样传入 core，`run-exclusion-generation.mts` 以 C8 四条断言钉住（present 逐字转发 / absent 键不出现 / 错误类型不改造成缺省）；桥接落地前多 peer 网络的 a2b 保持 reasoned gap（诚实缺口，不是故障）。
- **数据通道仍待真机验证**：a2b 真帧进出 TUN、echo 返回、b2a 运营中继链路均未在真机确认；本文不宣称 a2b 已闭合。
- 不能由「零 TUN 帧」断言「无全部流量」：无帧也可能是未产生流量或采集面未见，判定须按 §4.3 正控与帧扫描口径综合。
- 当前主机 n2h-peer 已观测 `own_overlay=100.108.126.29`（仅作现场记录；core 未硬编码任何地址，参数/fixture 中的地址均为示例）。
