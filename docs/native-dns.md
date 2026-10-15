# Native DNS 维护说明

适用范围：`feat/native-dns` 工作树中已落盘的原生 DNS 实现。
本文记录实现边界与已有证据；最终完整 host 回归与 PC 包构建/签名/验签已通过，真实新实现 DNS 验证尚待设备连接恢复。
官方语义对照固定为 NetBird `67c2041fb5534c19d7b2320617cfc3aaab8e7d3d`、Android `147bb4f`，不宣称最新 release。
参考：[NetBird 固定提交](https://github.com/netbirdio/netbird/tree/67c2041fb5534c19d7b2320617cfc3aaab8e7d3d)、[Android 固定提交](https://github.com/netbirdio/android-client/tree/147bb4f)。

## 代码入口与配置模型

- [network_map.rs](../client/core/src/network_map.rs)：管理配置模型与转换。
- [dns_resolver.rs](../client/core/src/dns_resolver.rs)：不可变配置快照、记录解析、选路与 UDP fitting。
- [dns_transport.rs](../client/core/src/dns_transport.rs)：smoltcp 本机 DNS IPv4 UDP/TCP transport。
- [dns_upstream.rs](../client/core/src/dns_upstream.rs)：单个上游的异步 wire 交换及 socket 保护。
- [dns_runtime.rs](../client/core/src/dns_runtime.rs)：worker、队列、快照更新和解析计划执行。
- [connector.rs](../client/core/src/connector.rs)、[wg_device.rs](../client/core/src/wg_device.rs)：生命周期及 TUN 接缝。
- [NetBirdVpnConfig.ets](../client/entry/src/main/ets/vpnextensionability/NetBirdVpnConfig.ets)：系统 DNS、search 与本机服务路由。

管理端 DNS 模型保留完整 CustomZones / NameServerGroups 内容，不压平成 DNS IP 列表：

| 层级 | 字段 |
| --- | --- |
| DnsConfig | ServiceEnable、NameServerGroups、CustomZones |
| CustomZone | Domain、Records、SearchDomainDisabled、NonAuthoritative |
| SimpleRecord | Name、Type、Class、TTL、RData |
| NameServerGroup | NameServers、Primary、Domains、SearchDomainsEnabled |
| NameServer | IP（可含 IPv6）、NSType、Port |

Type、TTL、NSType 保留有符号值；Port 转换约束为 u16。
当前 resolver 接受 `NSType=1`，保留合法服务器的地址、端口及管理组边界。
废弃 forwarder port 当前未消费；保留模型不代表每个协议类型均可执行。

## 地址与传输边界

虚拟 resolver IPv4 使用官方 WG prefix 算法：网络最后地址减一，监听 `:53`。
例如 `100.100.0.0/16` 对应 `100.100.255.254:53`，由 `runtime_ip_from_cidr` 计算。
smoltcp 只服务该本机 IPv4 `/32` 的 UDP/TCP DNS，不承担其它 overlay 上游网络栈。
AAAA 是 DNS 记录类型，可以经 IPv4 UDP/TCP 查询；这不表示已提供 IPv6 DNS endpoint。
当前不接收 IPv4 fragments，也没有完整 IPv6 本机 DNS endpoint。
DNS 文本记录与 wire 编解码使用 Hickory；TCP 使用两字节长度 framing。
transport 有界：16 个 TCP 槽、128 个待答请求、每方向 64 帧；过期由 poll 驱动。

## 解析与上游策略

1. CustomZones 本地记录优先；区域按域名具体程度选择。
2. 需要转发时，选择最具体匹配分域；primary 作为 root 边界。
3. 同一匹配域保留多个管理组：组间并发，组内按服务器顺序尝试。
4. SERVFAIL / REFUSED 可继续尝试，NXDOMAIN 是有效答复；私域组失败不改查 root 或物理 DNS。
5. 没有匹配上游边界时，才使用 SocketSupply 当前物理 DNS 列表，并剔除 resolver 自身。

external CNAME 已落盘为通过同一 snapshot 对目标再次 `resolve`，完成后合并原始问题与 CNAME 链。
因此目标可命中本地记录、私域组或 primary；只有目标无匹配组时才走物理 DNS。
该修正的最终 runtime 集成 18 项及 lib 单元 3 项已通过。
续查保留查询所属旧 snapshot；更新后的新查询使用新 snapshot。

仅当上游精确匹配当前实际物理 IPv4 DNS 且端口为 53，才申请受保护 socket。
非 53 端口、其它 IPv4 和 IPv6 上游使用普通 socket，由 OS 路由决定路径。
物理列表不是控制面组的别名，也不能用公共 DNS 配置推断物理网络实际 DNS。
单上游交换共享 4 秒预算；UDP 收到匹配且 TC=1 的响应才重试 TCP。
TCP 客户端获得完整 wire；UDP 答案在 EDNS / 默认 512 与 MTU−28 的预算内原样透传。
超预算才按完整 RRset 重编码、置 TC；上游 UDP TC → TCP 后仍需执行客户端预算 fitting。

## 配置容忍与有意差异

Android permanent 语义下，`ServiceEnable=false` 仍应用实际下发的 records、groups、search。
该标记不等于删除规则；真正的 `None` 或配置内容为空才删除对应内容。
永久 resolver 可继续运行并处理物理 fallback，不能凭 ready 推断仍有本地记录。
单条坏记录局部 skip；坏 zone、组域元素、IP / 不支持类型或自上游按对应粒度跳过并计数。
合法兄弟项保留；只接受字面 IN，TTL 需落入 u32，TTL=0 可用。

空 root 经服务器过滤后不注册阻断边界，允许物理 fallback。
私域原有服务器经过滤为空时保留 fail-closed 边界，是有意的私域隔离差异。
原本 NameServers 为空的组直接跳过，不应混同于上述“过滤后为空”。
不可变快照按配置顺序返回完整 RRset，不实现官方有状态 RR round-robin。
单个 CustomZone 即使 NonAuthoritative=true 仍按官方兼容语义强制权威。
搜索域小写、末点 FQDN 去重；search flags 不改变查询路由，壳收到 ASCII / punycode。

## worker、WG 与系统接线

worker 独占 transport 与单线程 Tokio runtime，不持有 TUN fd，也不获取 WG/device 锁。
入口为 Tokio `mpsc(128)` + `try_send`；匹配 DNS 包即使队满、停止或代际过期也按已消费处理并计 drop。
独立 reset epoch 清 transport / 旧任务 / 旧输入，避免复用 generation 时旧数据复活。
WgDevice 是唯一 TUN I/O owner：拦截 DNS，poll wake fd，排出答案并在写 TUN 前核对 generation。
poll 计划持有 handle 的 Arc 以保证 wake fd 寿命；worker 不直接写 TUN。
停服先 detach，再在设备/feed/inner 锁外 `stop_and_join`；Drop 仅作防泄漏后备。

Connector 在发布壳快照前准备真实 runtime，ready 按 worker running 导出。
规则或上游更新复用 worker；地址 / search 变化进入既有受控 VPN recreate 与 ACK 流程。
IP / MTU 变化可能替换 worker；准备失败保留上一可用配置及安全错误摘要。
壳仅在 ready=true 且地址有效时声明虚拟 DNS，设置 search，并添加 `source=local-dns` 的 `/32` 路由。
该 `/32` 属本机服务，不是额外 managed peer route；ready=false 保留基础 DNS。
source config 日志仅记录数量摘要，不输出记录值、完整配置或密钥；ready/IP/错误阶段用于运行诊断。

## 已有实证与缺口

壳 ACK 携带实际 create 快照的 `applied_dns`，core 按回执确认平台配置，create → ACK 竞态修复已完成。
定向验证为 native DNS 9 项、route apply 60 项及真实 Ability 390 项通过。

以下为外部本地验收记录；本次提交整理未重跑测试、构建或设备操作：

| 范围 | 已有记录 |
| --- | --- |
| ArkTS selfcheck 最终轮 | 53 个 run-*.mts + run.mts，共 54 组全部 exit 0；不合并断言总数 |
| pure resolver | 23 通过 |
| Connector | 最终定向 native DNS 9、connector 集成 22 通过 |
| WG | 单元 36、wg_e2e 11、recreate 6 通过 |
| runtime 最终 CNAME | 集成 18、lib 单元 3 通过 |
| 完整 Rust 首次回归 | 1110 passed / 1 failed，exit 101；保留历史 |
| 完整 Rust 最终一轮 | exit 0，53 个结果单元、1114 passed / 0 failed / 0 ignored |
| 最终输入一致性 | 241 文件起终逐文件 SHA256 一致；metadata / diffcheck 均 exit 0 |
| OHOS 最终包 | aarch64 build / sign / verify 均 exit 0；x86_64 OHOS cargo check exit 0 |

[wg_dns_seam.rs](../client/core/tests/wg_dns_seam.rs) include 了 runtime 集成测试；上述完整回归计数含跨 target 重复执行，不是 unique 测试数。
唯一失败为 `relay_client_e2e::auth_rejection_is_typed_and_retry_stays_on_the_exact_backoff`，首次在 `round 1 dialed` 触发内部 60s fuse；exact 隔离复跑一次通过。
整个 relay target 在 DNS 树和 clean main `71a2212` 各一次 16/16 通过；相关 test/relay 文件无改，bare TcpDialer 不经 DNS / SocketSupply。
虚拟时钟同步竞态仅为源码候选，未动态复现，实际根因未定；最终完整一轮 relay 16/16 通过，不能抹掉首次失败或将候选写成已证根因。
最终 1114 为跨 target 执行次数，含 `wg_dns_seam` include 的 runtime 18 项重复执行，不是 unique tests 数。

最终交付候选为 [PC signed HAP](</home/worker/n2h-kit/native-dns-validation/20261015T152517.035553Z-final-pc-package/netbird-native-dns-pc-signed.hap>)，7493396 B，0.0.48 / 48。
SHA256：`fdc9ac8d8291e1d88d3eedf50c2dd2b9dc7474306d8c4441bd641d50bff03a65`。
N2H_BUILD_SHA256：`cb8fb023713a43faf470004c803dfcf331d59a5169d6bc1bd1b8038bec88e7e4`。
构建时 HEAD 为 model commit `0d4cd69912c5f6a0ad60025b1fd89c20251fb79f` 且 dirty，含本次功能源码；本地功能提交不代表按新 commit 重建。
中间 `2cec1c…` 包已过时，不作为交付。
[本轮设备报告](</home/worker/n2h-kit/native-dns-validation/20261015T153311Z-pc-final/REPORT-device.md>) 记录列表见 PC，但 echo 探测 15 秒超时，未安装最终包。
真实新实现 DNS 验证尚待设备连接恢复；后续重连结果尚未纳入本文，不据此推断成功或失败。

以下均为外部本地证据，不随 repo 发布：
- [最终完整测试报告](</home/worker/n2h-kit/native-dns-validation/20261015T152508Z-final-tests/REPORT-final-tests.md>)、[最终包报告](</home/worker/n2h-kit/native-dns-validation/20261015T152517.035553Z-final-pc-package/REPORT-final-package.md>)。
- [完整 host 回归报告](</home/worker/n2h-kit/native-dns-validation/host-tests-Hjrr9lSY/REPORT-host-tests.md>)、[relay 限定因果报告](</home/worker/n2h-kit/native-dns-validation/relay-causal-20261015/REPORT-relay-failure.md>)。
- [OHOS 首轮构建报告](</home/worker/n2h-kit/native-dns-validation/20261015T150407.971387Z-ohos-build/REPORT-build.md>)、[首轮签名报告](</home/worker/n2h-kit/native-dns-validation/20261015T151121.308743Z-signed-pc/REPORT-sign.md>)。
- [历史 selfcheck 报告](</home/worker/n2h-kit/native-dns-validation/20261015T143238.736316Z-native-dns-selfcheck-2853641-report.json>)、[PC 配置报告](</home/worker/n2h-kit/native-dns-validation/20261015T145320Z-pc-dns-config/REPORT.md>)。
真实设备 DNS、路由与生命周期行为仍待验证。

## 当前用户场景

用户报告：此前断开 VPN 时 .6 → .9 NetBird DNS 链路可解析 netcenter.local，连接 VPN 后失败。
PC 0.0.48 当前 baseline core 的 App state=connected，配置 DNS 为公网 223.5.5.5 / 114.114.114.114，interface_dns=null；不是新 DNS HAP 的设备验收。
尚未取得 OS 最终生效 DNS 值，也没有物理 Wi-Fi DNS 的直接证据，不能据此裁定根因。
此前 ROM ping 私域报 `Name does not resolve`，公网 example 可解析；当时 VPN 状态未证，不能当作本次切网 A/B。
自带 ELF 临时 probe 被 ROM `Permission denied`，已清理；后续使用 App、已有系统工具或 browser 验收。
未改 .6/.9 生产配置；未实现另一个 fakeIP 分类网关项目。

## 可复制的 host 验证命令

从仓库根目录执行；以下命令是建议复验，执行结果需另行记录。
优先使用通用 cargo；离线前需已缓存锁定依赖，工具链路径按本机既有环境设置。
如需自定义环境，保留已有 `RUSTUP_HOME` / `CARGO_HOME`，不要把其它主机路径直接照搬。

```sh
cd client/core
cargo test --offline --locked --lib dns_resolver::tests
cargo test --offline --locked --lib dns_runtime::tests
cargo test --offline --locked --lib connector::tests::native_dns
cargo test --offline --locked --lib wg_device::tests
cargo test --offline --locked --test dns_upstream --test dns_transport_memory --test dns_runtime
cargo test --offline --locked --test connector --test wg_e2e --test wg_recreate_n8
cargo test --offline --locked --test wg_dns_seam wg_seam::
```

## 建议真机验收清单

- 记录版本、网络、VPN 状态、实际 create DNS/search 参数、OS 生效 DNS 和实际物理 DNS，建立同条件断开/连接 A/B。
- 分别验证 private/public、A/AAAA、UDP/TCP，并区分系统解析入口与显式 resolver 查询。
- 验证本地记录、最具体分域、primary、无匹配物理 fallback，以及私域失败不泄漏到物理 DNS。
- 验证跨私域 / primary 的 external CNAME、链路错误及 UDP TC → TCP → fitting。
- 热更新 records/upstreams 不重建；地址/search 更新按受控 recreate 生效；None/空删除、false 保留下发规则。
- stop/restart 与 TUN 替换后验证 ready、旧 generation 丢弃、TCP 会话复位及无迟到答案。
- 保留原 .6 → .9 路径作对照，记录到 .6 的实际选路/exclusion；分别验证直连和中继路径。
- 用 App、已有系统工具或 browser 复现业务访问；保存脱敏证据后再认定设备验收通过。
