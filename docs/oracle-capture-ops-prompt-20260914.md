# 任务：为自研 NetBird 客户端采集「官方客户端中继行为 oracle」（只读采集，尽量零改动）

## 背景（一句话）

我们在开发一个 **HarmonyOS 平台的自研 NetBird 客户端**（非官方客户端）。P2P 路径（ICE + WireGuard）已在真机跑通，现在要补 **中继（relay）** 路径。按项目治理要求，实现前必须先在**同一套生产实例**上采集**官方客户端的可观察行为**作为对照基准（oracle），否则只有协议自洽、没有行为对照，测量结论无效。

**不需要新增任何客户端**：请从你们网络里**已有官方客户端的设备中挑一台**（下面有挑选建议）。

## 挑选设备的原则

优先级从高到低：

1. **本身就是"Relayed"的 peer** —— 在任意一台设备上跑 `netbird status -d`，看每个 peer 的连接类型；若有 peer 显示 **Relayed**（NAT 后的设备通常如此），**直接选它**：这种情况**完全不需要改任何配置**（零改动）。
2. **能抓到包的 Linux 设备**（有 root/sudo 与 `tcpdump`）—— 能同时看到"到 peer 的 WG UDP"和"到中继的 WSS TCP"，是最理想的采集点。
3. 若设备本身抓不了包，但**它的上行网关/出口**能抓 —— 也可以，但请告诉我们采集点在哪一跳。

⚠️ 尽量选**测试/开发设备**；如果是生产设备，请选低峰窗口，且只做第 2 步那条可撤销的临时规则。

## 需要交付的东西

| # | 交付物 | 用途 |
|---|---|---|
| 1 | **该设备 `netbird status -d` 全文** | 官方客户端自己声明的连接类型（P2P / Relayed），以及它看到的中继 |
| 2 | **A 拓扑 pcap**：该设备处于 P2P 时的抓包 | 直连基线 |
| 3 | **B 拓扑 pcap**：该设备处于 **Relayed** 时的抓包 | 中继基线（若已有 Relayed peer，A/B 可能来自两台不同设备，请分别标注） |
| 4 | **客户端日志**（debug 级，含中继 URL、Auth/OpenConn/Transport/HealthCheck 时序） | 协议时序对照 |
| 5 | **元数据**：设备名/系统、官方客户端版本、management/signal/relay 版本、抓包时间（含时区）、抓包接口与过滤表达式、实际生效的中继 URL、对端 peer 名 | 证据可复现 |
| 6 | （可选，很有价值）**中继主机侧同时段抓包** | 证明中继侧确实收到该会话 |
| 7 | （可选）若期间发生过中继切换/failover | **请记录切换时刻**——这本身是我们需要的观测之一 |

## 执行步骤

### 1. 先看现状（零改动）

在若干台设备上（或结合 dashboard 的 peer 列表）执行：

```bash
netbird status -d
```

**记录每个 peer 的连接类型**。若发现已有的 Relayed peer，直接进入第 3 步（跳过第 2 步的 UDP 阻断）。

### 2. （仅当网络里没有 Relayed peer 时）制造 Relayed 条件

在**选定设备**上临时阻断出向 UDP（保留 DNS），迫使该设备回退到中继：

```bash
# 记录现状，便于复原核对
sudo iptables -S OUTPUT > /tmp/oracle-iptables-before.txt

sudo iptables -I OUTPUT -p udp --dport 53 -j ACCEPT
sudo iptables -I OUTPUT -p udp -j DROP

# 让客户端重新协商
sudo netbird down && sudo netbird up        # 或 systemctl restart netbird
```

> 说明：中继走 **WSS/TCP**，阻断 UDP 不会切断中继本身，只会切断直连 WG。
> **如该设备是生产设备、或你无法在设备上改防火墙**：请跳过本步，只做第 1/3 步，并在交付里注明"无 Relayed 样本"。我们据此调整采集方案。

### 3. 抓包

在**能同时看到该设备出向流量**的位置抓（设备本机最佳）：

```bash
# <DEV_IP> = 选定设备 IP；若直接在设备本机抓，可省略 host 过滤
sudo tcpdump -i any -s 128 -w /tmp/oracle-relayed.pcap \
  '(host <DEV_IP> and (tcp port 443 or tcp port 28443 or udp))' &
```

- 抓 **≥60 秒**，期间让 overlay 跑点流量（例如 ping 对端 overlay IP 20 次、或访问一个内网服务）。
- `-s 128` 是**有意截断**：只取头部，避免把中继令牌/载荷落进 pcap。
- 同一台设备在 P2P 状态下重复一次，文件名 `oracle-p2p.pcap`。

### 4. 采集日志

```bash
journalctl -u netbird --since "-10 min" > /tmp/oracle-netbird.log     # 容器则 docker logs
```

若日志级别不是 debug，可临时提高后再重跑一次（记得改回）：`sudo netbird up --log-level debug`。

### 5. 恢复（**务必执行**）

```bash
sudo iptables -D OUTPUT -p udp -j DROP
sudo iptables -D OUTPUT -p udp --dport 53 -j ACCEPT
diff <(sudo iptables -S OUTPUT) /tmp/oracle-iptables-before.txt && echo "OUTPUT 规则已复原"
```

## 交付方式

把 pcap、`netbird status -d` 全文、日志、元数据打包（或贴关键片段）给我们即可。**不需要提供任何 setup key**（本次不新增设备）。

## 重要约束

- **不新增客户端、不新建 setup key**（若手上有一把为本次准备的一次性 key，请**直接 revoke，未使用**）。
- **不改线上 NetBird 配置**（management / signal / relay 一律不动）。
- 唯一允许的临时改动是第 2 步那条 UDP 阻断规则，**必须在第 5 步复原**；生产设备上请勿执行第 2 步。
- pcap 已用 `-s 128` 截断；若提供全量包请注明（可能含中继令牌，我们会按敏感材料处理，只做行为分析）。
- 若选定设备属于他人/生产用途，请先确认影响面再操作。

## 备注（我们这边会做什么）

我们只把这些材料作为"官方客户端行为基准"，用于对照自研客户端的中继路径；不会据此对你们的部署做任何变更。如果你们更希望**由我们在自己的 pod 里跑官方客户端、你们只负责在节点/中继主机抓包**，也可以——告诉我们，我们给出开始/结束时间戳与客户端 IP，你们按同一过滤表达式抓即可。
