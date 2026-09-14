# N13 oracle 登记：官方客户端中继行为基准（2026-09-14 落盘）

> 本文件满足 `docs/n13-relay-increment-plan-20260914.md` §5 与 T0 裁定 Q4 的**硬前置**：
> 「N13 验收测量前，官方客户端 oracle 必须已落盘」。本文件即该 oracle 的登记，
> **本身不是任何门 pass**，也不得被读作 N2-H 或 N6 pass。

## 1. 交付与完整性

- **交付包**：`netbird-relay-oracle-delivery-4files.tar.gz`，sha256 `0a16bdf6b504c178c6404e62de6c3b15891943f0bc71688cf7a22f18b6942cb7`（与交付声明一致，主会话独立复核）。
- **内层材料**：`netbird-relay-oracle-20260914.tar.gz` → 22 个文件。
- **归档位置（仓B）**：`~/harmonyos-signing/netbird-n1bdisc/records/oracle-official-client-20260914/`（23 文件，含 `ORACLE-SHA256SUMS`；`sha256sum -c` 通过）。
- **采集性质**：零客户端配置改动、未阻断 UDP、未新增客户端、未创建/撤销 setup key；全部 pcap `-s 128`（仅头部，不含中继令牌/载荷）。
  - 对生产的两处临时改动均已复原并验证：netrpi 临时装/卸 `tcpdump`（dpkg 终态逐字一致）；两台客户端日志级别 `info→debug→info`。
- **交付方自检**：日志敏感扫描 `token|signature|secret|password|bearer|eyJ` **零命中**（主会话对 `netbird-debug-netrpi.log` 独立复扫，同样 0 命中）。

## 2. 主会话独立核验（不只采信交付说明）

| 核验项 | 方法 | 结果 |
|---|---|---|
| 交付包完整性 | `sha256sum` | ✅ 与声明一致 |
| 连接类型声明 | 读 `status-d-netrpi.txt` / `status-d-net-host.txt` | ✅ netrpi 多为 `Relayed`（另有 1 条 `P2P`，对应交付说明中的自然迁移）；net-host 为 `P2P` |
| **直连 vs 中继的四元组对照** | `tcpdump -nr` 汇总会话 | ✅ P2P 基线：`10.0.1.1` ↔ **`175.10.223.50.58433`**（对端 WAN 端点，无中继端口）；Relayed：↔ **`175.10.223.50.28443`**（中继 WSS） |
| **中继侧三点对齐** | 读 `*-relayhost-28443.pcap` | ✅ 客户端 NAT 口 `:49902` → `192.168.50.9:28443` → Caddy `172.18.0.3` → relay 容器 `:80` |
| **帧格式逐字节** | 自写只读 pcap 解析器（`/tmp/relay_pcap_analyze.py`，SLL2/以太网） | ✅ 75 个 `ver=1` 帧；**Transport 帧样本 `01 03 73 68 61 2d …`**（`[ver=1][type=3]["sha-"+peerID]`）；保活帧 `01 05`；WS opcode 观测到 binary/ping/pong/close；客户端帧较服务端大 4 字节（WS 掩码） |
| 日志时序关键行 | `grep` on `netbird-debug-netrpi.log` | ✅ `starting relay client manager with [rels://home.alfadb.cn:28443]`、`open peer connection via permanent server: …`、`relay health check: healthy=true` ×5 |

## 3. 基线事实（登记项）

### 3.1 版本
| 组件 | 版本 |
|---|---|
| 客户端 netrpi（192.168.50.8，wt0 `100.108.162.237`） | **0.77.1** |
| 客户端 net-host（192.168.50.9，wt0 `100.108.171.38`） | **0.78.1** |
| 客户端 netcenter（`10.0.1.1`，wt0 `100.108.227.115`） | **0.78.1** |
| management / signal / relay（容器） | **0.77.0** |
| home relay（独立容器） | **0.76.3** |
| dashboard | v2.91.1 |
| 治理文本写作基线 | v0.76.3（差异已在增量计划 §3.1 登记） |

### 3.2 中继与端点（实测读出，非推断）
- 生效中继 URL：**`rels://home.alfadb.cn:28443`**（管理面当前只广告这一条）。
- P2P 基线端点：netcenter `10.0.1.1:49179` ↔ net-host WAN `175.10.223.50:58433`。
- Relayed 链路：客户端 → `175.10.223.50:28443`（TLS）→ 宿主 `192.168.50.9:28443` → Caddy `172.18.0.3` → relay 容器 `172.18.0.2:80`（明文 WS）。

### 3.3 观测到的状态迁移（对实现有直接价值）
1. **Relayed → P2P 自然回退**（约 15:48:49）：netrpi ↔ netcenter 在未干预下由 Relayed 迁移为 P2P（ICE host/srflx）；
   迁移后的 `oracle-relayed-netrpi-keepalive-only.pcap`（44 包）**证据化地展示了"peer 走 P2P 时中继连接仍在保活"**——即中继连接不随 P2P 建立而拆除。
2. **P2P → Relayed**（客户端重启后）：netrpi 对 netcenter 又回到 Relayed。
3. 全程 **无中继 failover**：`mode: home / home: healthy / cloud: healthy / switch_failures: 0 / degraded: no`。

### 3.4 协议时序（官方客户端日志，netrpi 0.77.1）
| 动作 | 日志对应物 |
|---|---|
| 中继 URL 下发/生效 | `shared/relay/client/manager.go:161: starting relay client manager with [rels://home.alfadb.cn:28443] relay servers` |
| OpenConn | `manager.go:201: open peer connection via permanent server: <公钥指纹>` |
| Transport | `client.go:662: buffered early transport message for peer: sha-…` |
| HealthCheck | `client/internal/engine.go:2333: relay health check: healthy=true` |

## 4. 残余与已知边界（不得外推）

1. **Auth 帧字节未捕获**：两段抓包窗口均在会话中途，Auth 只在建连瞬间出现；Auth 布局仍以**源码级**依据为准（`shared/relay/auth/hmac/v2/token.go` 的 `Marshal()` 与 `messages/id.go` 的 `"sha-"` 前缀，主会话已抽查）。
2. **TLS 段不可见帧结构**：客户端→中继为 WSS，`28443` 抓包只见 TLS 记录层；帧级证据来自 relay 容器 `:80` 的**明文 WS** 段。
3. **本 pod 无 `CAP_NET_RAW`**：我们无法自行抓包（2026-09-14 实测），后续任何 oracle 补充采集仍需运维侧执行（或改用 T0 允许的等效证据并登记）。
4. **单机视角**：抓包均在第三层以上、无交换机镜像级全路径抓包。
5. **一个 pcap 混入第三方会话**：`oracle-p2p-nethost.pcap` 窗口内同端口混入 net-host ↔ admindingzuwei 会话，分析须按四元组筛选（交付方已注明）。
6. 主会话自写解析器对**截断帧会失步**（snaplen 128），故类型统计只取完整帧；已剔除噪声后给出上表结论。

## 5. 结论与下一步

- **T0 Q4 的硬前置「官方客户端 oracle 落盘」= 已满足** ✅
- 另一个硬前置仍未满足：**默认路由下启用 WSS 前**，须把 relay 的 **TCP** 端点纳入 N2-H 冻结并排除（工具侧已就绪：`EndpointKind::Relay.proto()=="tcp"`，回环验证 `n2h-pass`；真机侧待执行）。
- 因此：**N13 编码可以开工**（T0 Q4：编码无硬前置）；**验收测量**须在两端点齐备后进行，且结论只可写 `N13 pass`，不得写成 N2-H/N6 pass。
