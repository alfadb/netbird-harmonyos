# 索取：NetBird 既有部署的接入信息（中继 + management）

> **背景**：我们在做 HarmonyOS 平台上的 NetBird 客户端（自研 Rust 核心，非官方客户端）。
> P2P 路径（ICE + WireGuard）已在真机跑通；现在要接**中继**，以覆盖双方都在 NAT 后的场景。
>
> **性质**：以下 **全部为只读确认**，不需要改任何线上配置。唯一需要你们"给"的是一个**测试用 setup key**（可撤销、短有效期）。
> 我们**不需要**任何密钥明文（详见文末"不需要的东西"）。

---

## 一、management 服务

| # | 需要的信息 | 说明 |
|---|---|---|
| 1 | **对外可访问的 management 地址**（完整 URL 含端口，如 `https://netbird.example.com` / `http://IP:33080`） | ⚠️ 必须**同时**对我们的测试 pod（`10.98.0.194`，k8s 内网）**和**手机（局域网 `192.168.50.199`）可达 |
| 2 | **一个测试用 setup key** | 可复用、短有效期即可；我们会用它注册一台测试设备，用完请随时撤销 |
| 3 | 版本号 | `netbird-server --version` 或容器 image tag |
| 4 | **只读确认**：`management.json` 是否有 `Relay` 段 | 只需回答"有/没有"；若有，说明 `Addresses` 的值与 `CredentialsTTL`（**`Secret` 不用给我们**，只需确认"已配置且非空"） |

## 二、relay 服务

| # | 需要的信息 | 说明 |
|---|---|---|
| 5 | **relay 对外地址** | 必须与 relay 容器的 `NB_EXPOSED_ADDRESS` 一致，形如 `rel://域名:端口` 或 `rels://域名:端口` |
| 6 | **传输方式与端口** | WebSocket/TCP 还是 QUIC/UDP（或两者都开）？对外端口是多少（常见 `443`；quickstart 默认 `33080`） |
| 7 | **是否启用 TLS** | `rel://`（明文 WS）/ `rels://`（WSS）/ QUIC（强制 TLS）。若启用：证书是自签还是公共 CA？QUIC 的 ALPN 是什么？ |
| 8 | **只读核验**（麻烦贴一下输出） | `docker ps \| grep netbirdio/relay`<br>`curl -s http://<relay-host>:9000/health`<br>`curl -s http://<relay-host>:9090/metrics \| head` |
| 9 | **一致性确认（最容易出错的一点）** | management 的 `Relay.Secret` 与 relay 容器的 `NB_AUTH_SECRET` **必须是同一个值** —— 请确认二者一致（**不用告诉我们值**） |

## 三、网络可达性

| # | 需要的信息 | 说明 |
|---|---|---|
| 10 | 从我们的 pod（`10.98.0.194`）与手机所在网段（`192.168.50.0/24`）到 relay 地址，**需要放通的端口/协议清单** | 只需 **客户端 → relay 单向**可达，不需要反向 |
| 11 | relay 是否有 IP 白名单/限流策略需要我们报备 | 若有，请给出我们的源地址（pod egress IP / 局域网出口） |

## 四、我们**不需要**的东西

- ❌ relay 的 `NB_AUTH_SECRET` 明文 —— 客户端使用的是 **management 在登录/同步响应里下发的 HMAC 时间戳令牌**，不接触共享密钥
- ❌ TURN 用户名/密码 —— NetBird 默认中继**不是 TURN**（若你们另外单独部署了 TURN，请在**第 6 条**注明）
- ❌ 任何用户账号密码 —— 只要一个测试 setup key

## 五、如果你们的 relay/management 只在内网可达

这种情况我们的 pod 和手机可能都连不上。更实际的做法是请你们在**一台我们双方都能访问的机器**上临时起一套：

- 一个 `netbirdio/relay` 容器（`NB_EXPOSED_ADDRESS` 指向该机器可达地址）
- 一个测试 management（`management.json` 的 `Relay.Addresses` 指向上面这个 relay，`Relay.Secret` 与容器的 `NB_AUTH_SECRET` 同值）

然后按第一~三条给我们信息即可。**这条也请一并确认是否可行。**

---

### 附：我们拿到信息后的步骤（供参考，不需要你们配合）

1. 先做**零代码确认**：让我们现有客户端打印登录/同步回包里的 `NetbirdConfig.relay` 字段，确认 management 确实在下发中继信息；
2. relay 属于我们项目冻结协议栈的范围外，会先经内部评审确认范围，再动代码；
3. 实现最小集（基于上游源码结论）：WebSocket 拨号 + Upgrade → Auth 帧 → OpenConn → Transport 帧（38 字节头）→ 接入现有 WireGuard 收发；**ICE 逻辑无需改动**（relay 不是 ICE 候选类型，而是独立承载路径）。

---

## 结果（2026-09-14：运维已交付，零代码确认通过）

运维交付说明（只读核验、可达性矩阵、TLS/ALPN、令牌语义、未验证项）已收到，关键事实：

| 项 | 值 |
|---|---|
| management | `https://api.netcenter.alfadb.cn`（TCP 443，公共 CA，Caddy 终止 TLS） |
| signal | `https://signal.netcenter.alfadb.cn` |
| **当前广告的中继** | **`rels://home.alfadb.cn:28443`**（纯 WSS/TCP，**无 QUIC**；TLS 由 Caddy 终止，公共 CA） |
| 备用中继（running 但不广告） | `rels://relay.netcenter.alfadb.cn:443` |
| 令牌 TTL | 24h（`Relay.CredentialsTTL=24h`）——客户端需能重新 Sync 刷新 |
| 版本基线 | 控制面 0.77.0 / 中继 0.76.3 |
| 出口 | pod 与手机同为 `175.10.223.50`；三源实测全部可达 |

**pod 侧复核**（与运维矩阵一致，全部 `verify=0`）：api 200 / signal 302 / cloud-relay healthz 200 / home-relay healthz 200 / `/relay` 426。

**客户端实测（零代码确认，通过）**：

```
[n1b] connector: login ok (peer address assigned)                      ← 生产 management TLS 登录成功
[n1b] connector: netbird-config relay urls=1 token_present=true
          (uris=rels://home.alfadb.cn:28443)                          ← 中继已下发（令牌不落日志）
[milestone] signal 'signal.netcenter.alfadb.cn:443' -> 8.139.6.47:443
[milestone] network-map applied (peers=3, signal='signal.netcenter.alfadb.cn:443')
```

即：既有客户端**无需改协议栈**即可对接生产实例；`NetbirdConfig.relay` 早已解析（`network_map.rs::RelayServers`），缺的是 relay 客户端实现。

**本轮暴露的一个缺口**：设备（ArkTS 壳）侧**没有 CA 注入通道**（`ca_pem` 只存在于连接器配置 JSON）——接生产实例前需补（或改为使用平台信任库，属设计选择）。

**证据落盘**：`~/harmonyos-signing/netbird-n1bdisc/records/prod-management-relay-probe-20260914.md`（+`.sha256`，已扫描确认不含任何密钥/令牌值，`is_evidence:false`）。

**范围与顺序**：relay 属 `§二.5` R0 必选功能「direct+relay 双路径」，但涉及新增常驻出站连接与 WebSocket 实现，按 `§二.10` 已派 T0 席裁定范围/判据/重验义务；线格式规格由上游源码提取中。

