# Self-Hosted Interop Plan — 主机侧联调 CLI（N9）

SPDX-License-Identifier: AGPL-3.0-or-later
Copyright (C) 2026 NetBird HarmonyOS contributors

状态：**方案 + 工具已就绪；本文档只描述配方，本文写作时未启动任何服务端/容器。**
目标：在 **不用真机** 的前提下，验证「我们的栈（`client/core`）↔ 真实 NetBird
服务端」的互操作：management 登录 → Sync 网络图 → signal 注册 → ICE 选路 →
WireGuard 握手 → 双向测试包。

工具：`nbinterop` —— `client/core` 包内的 **host-only** CLI
（`client/core/src/bin/nbinterop.rs`，复用 `client/core/src/host_sockets.rs`
的主机侧 socket 提供者）。它只做主机联调，**不是设备路径**：设备上依旧由
ArkTS 壳创建并 `VpnConnection.protect(fd)` 后喂 fd；库的 fail-closed 不变量
没有被放宽（`tests/host_interop_n9.rs` 中有断言：链接了 host 模块之后，
默认 `connector_start` 仍然拒绝无 socket 启动）。

---

## 1. 主机模式如何替代壳侧 fd（隔离原理，先读这个）

设备路径上，所有 socket 由壳创建 + protect，native 只收 **fd 号**（dup-only
消费）。主机上没有 VpnExtension/protect，因此 `host_sockets.rs`
（标注 **主机联调专用，非设备路径**）扮演壳的角色：

| 壳在设备上做什么 | `nbinterop` 在主机上做什么 |
| --- | --- |
| DNS 解析管理/signal 地址 | `std::net::ToSocketAddrs`（IPv4） |
| `mgmt_socket_open()` 建 TCP（未连接） | **同一个** `mgmt_socket_open()` 助手 |
| `VpnConnection.protect(fd)` | **无**（主机无此 API —— 已知差异，联调的是协议行为而非内核路由） |
| 把 fd + 地址喂给 `connector_start_with_socket` | 完全相同的生产入口 |
| `connector_ice_socket_feed`（未绑定 UDP） | 相同 seam，主机自建未绑定 UDP |
| `connector_signal_socket_feed(fd, addr)` | 相同 seam |
| `wg_fwd_open()`（固定端口 47010）UDP | 主机变体绑定 **临时端口**（同机双实例必须端口不同） |
| `connector_tun_fd_feed`（平台 TUN fd） | **socketpair 替身**（不创建内核 TUN；泵对 dup 的裸读写对数据报 socketpair 同样成立） |

隔离保证（三层）：

1. **库默认路径未动**：`connector_start`（无保护入口）仍默认拒绝
   （`management-socket-required`）；喂 fd 的各个 seam 仍然逐 fd dup 探针校验、
   dup-only 消费、borrowed 语义不变。
2. **设备路径不引用本模块**：`host_sockets` 无任何生产调用方；cdylib 导出面
   不变（`build.sh` 的 14 个冻结符号检查照常通过；产物大小与 HEAD 基线同量级）。
3. **fd 合同测试仍在**：`FdBag` 以"提供者持有 fd"语义保存已喂 fd，Drop 时逐个
   关闭；喂入的 fd 号在被消费前必须保持打开（与壳侧行为一致）。

CLI 构建（host 三元组，离线）：

```bash
cd client/core
export RUSTUP_HOME=/home/worker/rust/rustup CARGO_HOME=/home/worker/rust/cargo
export PATH="$CARGO_HOME/bin:$PATH"
cargo build --offline --locked --features cli --bin nbinterop
# 产物: target/debug/nbinterop （release 同参数加 --release）
```

`cli` 是 feature-gated 的 `[[bin]]`：`cargo build` / `cargo test` / OHOS 交叉
构建（`client/core/build.sh`）都 **不会** 触碰它。无新增依赖（参数解析手写），
`Cargo.lock` 未变。

---

## 2. 准备自托管 NetBird 服务端（方案，未执行）

官方来源（访问日期 **2026-09-13**）：

- 自托管 5 分钟 Quickstart（安装脚本、端口要求、生成的文件）:
  <https://docs.netbird.io/selfhosted/selfhosted-quickstart>
- 进阶自托管指南（自定义 IdP、分步配置）:
  <https://docs.netbird.io/selfhosted/selfhosted-guide>
- Setup Key 创建/使用（管理端操作）:
  <https://docs.netbird.io/manage/peers/register-machines-using-setup-keys>
- docker compose 模板（上游仓库 `infrastructure_files/`）:
  <https://github.com/netbirdio/netbird/blob/main/infrastructure_files/docker-compose.yml.tmpl.traefik>
- 组合配置示例（management/signal/relay/STUN 的 `config.yaml`）:
  <https://github.com/netbirdio/netbird/blob/main/combined/config.yaml.example>

Quickstart 要点（详见上方官方页）：一台有公网 IP 的 Linux VM（≥1C2G），放行
**TCP 80/443、UDP 3478**（STUN）；一个解析到该 IP 的域名；装 Docker +
docker compose v2、jq、curl；运行官方 `getting-started-with-zitadel.sh`
（quickstart 页内脚本）生成 `docker-compose.yml` + `config.yaml` + 反代配置；
首次访问 `https://<域名>/setup` 创建管理员。

最小 docker compose 形态（要点式，**只写方案**）：`management`、`signal`、
`dashboard`、`relay`、`stun`(coturn) 五个服务 + Traefik/Caddy 反代终止 TLS；
`management` 对内监听 gRPC `:80`（经反代映射 `:443`，即我们的
`management_url` 端口）、暴露 `:33073` 直连形态也可（配合自签/自有 CA）；
`signal` 对内 `:80`（`HostConfig.signal` 形如 `signal.<域名>:443`）；
**relay(TURN) 可选** —— 我们的栈目前显式不支持 `turn:`/relay 候选（N5a 边界），
同机/同网双端联调不需要它。

两个 setup key / 两个 peer 身份：

1. 管理台登录 → **Settings → Setup Keys → Create Setup Key**（官方页见上）。
2. 创建 **两个** 可重用（reusable）key：`interop-key-a`、`interop-key-b`
   （同一下设置不同的 peer 名即得到两个身份；reusable 便于反复重跑联调）。
3. 每个 CLI 实例用一个独立的 **私钥文件**（见 §3 配置），`hostname`
   分别填 `interop-a` / `interop-b`，服务端即视为两台独立机器。
4. 联调结束后 **撤销（revoke）两个 key**（用完即废）。

---

## 3. CLI 用法与配置文件

配置文件 = `ConnectorConfig` 的 JSON 文档 + 三个 CLI 便利字段。凭据
（管理 URL/私钥/setup key/CA）**只来自文件或环境变量**；命令行传秘密会被
**拒绝**（exit 2）；输出（plan/status）只含公钥与长度，不含秘密。

示例 `~/n9/interop-a.json`（**下面的 key 是合成的示例值，不是真实凭据**）：

```json
{
  "management_url": "https://netbird.example.com:443",
  "ca_pem_file": "/home/me/nb-ca.pem",
  "server_name": "netbird.example.com",
  "private_key": "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=",
  "hostname": "interop-a",
  "setup_key": "DED1BC68-0000-0000-0000-9E3E33B7D6F5"
}
```

- `ca_pem`（内联 PEM 或数组）与 `ca_pem_file`（路径，展开为 `ca_pem`）二选一；
  `https://` 必须注入 CA（无系统信任库）。`http://` 为明文（仅本机测试形态）。
- 环境变量覆盖（优先于文件，便于秘密完全不入文件）：`NETBIRD_SETUP_KEY`、
  `NETBIRD_JWT`、`NETBIRD_MANAGEMENT_URL`、`NETBIRD_CA_PEM`。
- `chmod 600` 配置文件：组/其他人可读会打 **warning**；不可读会得到明确报错
  （"permission denied; fix with chmod 600 …"）。

子命令与退出码（`--help` 同文）：

```text
nbinterop selftest                              # 离线自检（0 全过 / 1 失败）
nbinterop connect --config F [flags]            # 全流程，持续打印状态
nbinterop peer    --config F [flags]            # 对端最小模式：首个 WG 会话建立即 exit 0
flags: --dry-run --interval <ms> --timeout <s>
       --probe-dst <ip> --probe-interval <ms>
       --exit-on-terminal --verbose
退出码: 0 ok | 1 selftest 失败 | 2 用法 | 3 配置 | 4 凭据
       | 10 network | 11 timeout | 12 auth | 13 request | 14 server | 15 parse
       | 16 unsupported_url | 17 超时未达目标 | 18 peer 未建立会话 | 19 未知类别
```

`connect` 在 stdout **逐行** 打印 `connector_status()` 结构的状态 JSON；
`peer` 与其共享同一引擎，区别仅在退出条件（对端使命完成即成功退出）。
里程碑/喂食日志走 stderr（`--verbose` 另转发 HiLog 标记）。

---

## 4. 同机双实例联调（逐条命令）

前提：服务端已就绪（§2）；两个 setup key；两个配置文件
`~/n9/interop-a.json`、`~/n9/interop-b.json`（内容同 §3，`hostname`/私钥/
key 不同）。同机跑两个实例的地址在服务端分配的网段（如 `100.64.0.x/16`）。

```bash
cd client/core

# 0) 离线自检（不联网；两分钟内应全绿）
cargo build --offline --locked --features cli --bin nbinterop
./target/debug/nbinterop selftest

# 0') dry-run：只解析配置 + 打印计划；无解析、无 socket、无连接（exit 0）
./target/debug/nbinterop connect --config ~/n9/interop-a.json --dry-run

# 1) 起对端 B（先起 B，让 A 上线时立刻有可连对象）
./target/debug/nbinterop peer --config ~/n9/interop-b.json \
    --probe-dst 100.64.0.1 --probe-interval 1000 --interval 2000 --verbose \
    > /tmp/nb-b.status.jsonl 2> /tmp/nb-b.log

# 2) 起本端 A（持续运行；Ctrl-C 结束）
./target/debug/nbinterop connect --config ~/n9/interop-a.json \
    --probe-dst 100.64.0.2 --probe-interval 1000 --interval 2000 --verbose \
    > /tmp/nb-a.status.jsonl 2> /tmp/nb-a.log
```

期望看到的关键日志（stderr 里程碑顺序）与状态字段变化（stdout JSONL）：

| 阶段 | stderr 里程碑 / 关键日志 | `connector_status()` 字段 |
| --- | --- | --- |
| 登录 | `connector started (state=connecting)`；HiLog `connector: started over protected management socket` | `running:true`, `state:connecting→connected` |
| Sync | `network-map applied (peers=1, signal='…')` | `peer_count:1`, `route_count` 增长 |
| signal | `signal '<uri>' -> <ip:port>`；`signal socket fed`；`signal registered` | `signal.registered:true` |
| ICE | `ice connected peers = 1` | `ice.connected:1`（`checking` 先短暂变化） |
| WG | `wg device up`；`wg tunnel ready` | `wg.device_up:true`, `wg.ready:true`, `wg.handshakes≥1` |
| 双向包 | （探针注入，无专门日志） | A/B 两端 `wg.tx_packets`/`wg.rx_packets` 均增长；`peers_with_session:1` |

`peer`（B）在 `wg.peers_with_session >= 1` 时打印最终状态并 **exit 0**；
`/tmp/nb-*.status.jsonl` 的最后一行即两端互通的证据。测试包走的是真实 WG
数据面（探针是手工注入 TUN 替身 socketpair 的合法 IPv4/UDP 帧）。

---

## 5. 失败诊断清单

- **TLS/CA**：`auth`/`parse` 类错误 + `x509` 字样 → `ca_pem(_file)` 是否为服务端
  证书链的根；`server_name`/SNI 是否与证书一致；反代是否终止 TLS。我们的栈
  **不使用系统信任库**，必须显式注入 CA。
- **DNS**：`management host '…' resolve failed`（exit 10）→ 用
  `dig +short <host>` 核对；容器形态注意 `netbird.<域名>` 与
  `signal.<域名>`（或 `:443` 端口直连形态）的解析差异。
- **端口/防火墙**：TCP 443（管理+signal，反代形态）或 33073/10000（直连形态）；
  UDP 3478（STUN）。ICE 全失败（`ice.failed` 增长）多半是 UDP 不通或 NAT 对称。
- **NAT/直连**：同机联调通常 `host` 候选直连成功；跨网段需要 srflx（3478 可达）。
  无 relay（`turn:` 显式不支持）—— 两端均不可直连时 `ice.connected` 恒为 0。
- **setup key**：`auth` 类（status 16/7 等）→ key 失效/被撤销/用尽；重修 key 或
  换 `NETBIRD_SETUP_KEY`。
- **fd/喂食**：`socket-fd-missing` / `socket-fd-invalid` → 提供者侧 socket
  创建失败（进程 fd 上限？）；stderr `[feeder]` 行给出细节。
- **日志位置**：CLI 自身日志 = stderr（`/tmp/nb-*.log`）；状态流 = stdout
  （`/tmp/nb-*.status.jsonl`）；服务端 = `docker compose logs management|signal`
  （quickstart 生成的 compose 目录内）。

## 6. 安全说明（务必遵守）

- 管理 URL/私钥/setup key/CA **只放配置文件（chmod 600）或环境变量**；不要贴进
  聊天、issue、截图或提交进仓库；CLI 在设计上拒绝命令行传秘密并隐去一切秘密
  输出（公钥与长度除外）。
- 联调用的私钥/setup key **用完即废**：管理台 revoke 两个 key，删除
  `~/n9/*.json` 与私钥文件。
- `--dry-run` 与 `selftest` 之外的一切都会发起真实网络连接 —— 确认目标服务端
  是你自己搭建的靶子，不要对公网托管服务（如 netbird.io 云）跑本工具。
- 本文档中的全部 key/域名均为示例占位，不是真实凭据。
