# 任务：为自研 NetBird 客户端采集「官方客户端中继行为 oracle」（只读采集，不改线上配置）

## 背景（一句话）

我们正在开发一个 **HarmonyOS 平台的自研 NetBird 客户端**（不是官方客户端）。它的 P2P 路径（ICE + WireGuard）已在真机跑通，现在要补 **中继（relay）** 路径。按我们项目的治理要求，实现前必须先在**同一套生产实例**上采集**官方客户端的可观察行为**作为对照基准（oracle）——否则只有协议自洽、没有行为对照，测量结论无效。

**这次只需要你们做两件事：跑一次官方客户端 + 抓包。** 不改任何线上 NetBird 配置。

## 需要交付的东西（清单）

| # | 交付物 | 用途 |
|---|---|---|
| 1 | **A 拓扑 pcap**：P2P 可用时（默认状态） | 建立"直连基线" |
| 2 | **B 拓扑 pcap**：故意阻断 UDP 后（强制走中继） | 建立"中继基线" |
| 3 | **两种拓扑下的 `netbird status` 输出**（含连接类型 P2P/Relayed） | 官方客户端自己声明的路径 |
| 4 | **客户端 debug 日志**（两种拓扑各一份） | 中继 URL、Auth/OpenConn/Transport/HealthCheck 时序、token 刷新 |
| 5 | **元数据**：客户端版本、management/signal/relay 版本、抓包时间（含时区）、抓包接口与过滤表达式、实际生效的中继 URL | 证据可复现 |
| 6 | （可选，很有价值）**中继主机侧的同时段抓包** | 证明中继侧确实收到该会话 |

## 执行步骤

### 0. 前置

- 在一台**你能抓到包、也能跑客户端**的机器上做（k8s 节点最合适；需要 root 或 `CAP_NET_ADMIN` + `/dev/net/tun`）。
- 需要 `tcpdump` 与 `iptables`。
- **一次性 setup key**（只能用 1 次，注册 1 台设备）：

  ```text
  <ONE-OFF-SETUP-KEY — 由用户单独提供，仓库内不落凭据>
  ```

  > 你也可以在 dashboard 自己新建一把（等同）。**用完请 revoke**——按官方语义，revoke 不会踢掉已注册设备。
  > 本文件在仓库中**已脱敏**：实际 key 不进入版本库（凭据纪律）。
- 建议**不要**用生产节点名，注册时把主机名设为可识别的测试名，例如 `oracle-official-1`。

### 1. 跑官方客户端

容器方式（推荐，最省事）：

```bash
docker run -d --name oracle-netbird --restart=no \
  --cap-add=NET_ADMIN --cap-add=SYS_ADMIN --device=/dev/net/tun \
  -e NB_SETUP_KEY=<SETUP-KEY> \
  -e NB_MANAGEMENT_URL=https://api.netcenter.alfadb.cn \
  netbirdio/netbird:latest
```

或直接在宿主机跑官方二进制（同样需要 root）：

```bash
netbird up --setup-key <SETUP-KEY> \
  --management-url https://api.netcenter.alfadb.cn --log-level debug
```

**记录版本**：`netbird version`（容器内 `docker exec oracle-netbird netbird version`）。

### 2. 拓扑 A：P2P 可用（默认）

1. 抓包（**请用 `-s 128` 截断**，只取头部，避免把载荷/令牌落进 pcap；如你愿意也可另外补一份 `-s 0` 的短窗口）：

   ```bash
   # 把 <CLIENT_IP> 换成跑官方客户端那台机器的 IP；<PEER_IP> 换成对端公网 IP
   tcpdump -i any -s 128 -w /tmp/oracle-A.pcap \
     '(host <CLIENT_IP> and (tcp port 443 or tcp port 28443 or udp)) or host <PEER_IP>' &
   ```

2. 等 P2P 建起来，采集：

   ```bash
   netbird status -d            # 容器：docker exec oracle-netbird netbird status -d
   netbird status --json        # 若有该选项，一并给出
   ```

3. 让 overlay 跑一点流量（例如 ping 对端 overlay IP 20 次），再抓 60 秒。

4. 停抓包：`pkill -INT tcpdump`，保存 `status` 输出与客户端日志。

### 3. 拓扑 B：阻断 UDP，强制走中继

1. 在客户端机器上阻断出向 UDP（**保留 DNS**）：

   ```bash
   iptables -I OUTPUT -p udp --dport 53 -j ACCEPT
   iptables -I OUTPUT -p udp -j DROP
   ```

2. 重启客户端让它重新协商：`docker restart oracle-netbird`（或 `netbird down && netbird up`）。

3. 同样抓包（换个文件名 `/tmp/oracle-B.pcap`），并再次采集 `netbird status -d` 与 debug 日志。

4. **务必恢复**：`iptables -D OUTPUT -p udp -j DROP; iptables -D OUTPUT -p udp --dport 53 -j ACCEPT`，确认 `iptables -L OUTPUT -n` 已无残留。

### 4. 收尾

- 客户端下线：`docker stop oracle-netbird && docker rm oracle-netbird`（或 `netbird down`）。
- 到 dashboard **revoke 那把一次性 key**；如果要彻底清理，把该 peer 也删掉。

## 交付方式

把以下内容打包（或直接贴关键片段）给我们：

1. `oracle-A.pcap` / `oracle-B.pcap`
2. 两种拓扑的 `netbird status -d` 文本（**特别标注连接类型：P2P 还是 Relayed**）
3. 客户端 debug 日志（两种拓扑各一份；如含敏感信息可先脱敏）
4. 元数据（版本 / 时间与时区 / 接口与过滤表达式 / 实际中继 URL）
5. 可选：中继主机侧同时段抓包

## 重要约束

- **只读采集**：除上面那两条临时 `iptables` 规则（且已要求恢复）与注册一台测试客户端外，**不修改任何线上 NetBird 配置**。
- pcap 建议 `-s 128` 截断；如提供全量包，请注明（其中可能含中继令牌，我们会按敏感材料处理并只做行为分析）。
- setup key 是一次性凭据，**用完 revoke**；不要在其它环境复用。
- 若 `home.alfadb.cn` 在这一时段发生中继切换（failover 到 `relay.netcenter.alfadb.cn`），**请如实记录切换时刻**——这本身就是我们需要的观测之一。

## 备注（我们这边会做什么，不需要你们配合）

- 我们只把这些材料作为"官方客户端行为基准"，用于对照自研客户端的 relay 路径；
- 我们不会据此对你们的部署做任何变更；
- 如果你们更希望**由我们在自己的 pod 里跑官方客户端、你们只负责在节点/中继主机抓包**，也可以——告诉我们，我们给出开始/结束的时间戳与客户端 IP，你们按同一过滤表达式抓即可。
