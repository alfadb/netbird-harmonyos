# posctl 探针 socket 显式绑定 VPN 网络（2026-09-20，仅诊断）

## 为什么需要绑定

- 已证（0021）：a2b 探针 socket mark=0x00000000、无 SO_BINDTODEVICE 绑定、属应用
  UID 20020329、send 成功 52 字节；App defaultNetId=143(wlan0)、appNetId=0、
  VPN 网络 netId=147(vpn-tun) 且其 routes 不含 100.108.0.0/16；设备 TUN 未见
  a2b，而 b2a 在 to-os 可见；对端以 100.108.171.38（属 192.168.50.9）看到同一 nonce。
- **推断边界**：以上与「探针走默认网络而非 VPN 网络」一致，但策略路由表从未被观测，
  机制仍是推断。本增量不回答机制，只把问题变成实验：给探针 socket 显式绑定
  VPN 网络，再看 TUN/对端/errno 是否随之改变。
- 范围：仅 a2b 正控探针这一个 socket。产品流量 socket 不动，不改路由配置，
  不调 setAppNet，不新增权限。

## 接口来源（SDK 实读，26.0.0.821 sysroot）

- `network/netmanager/net_connection.h:279`：
  `int32_t OH_NetConn_BindSocket(int32_t socketFd, NetConn_NetHandle *netHandle)`，
  since 12；`@library libnet_connection.so`；syscap NetManager.Core。
- **该函数未标注 @permission**（同头文件的 GetDefaultNet/GetAllNets 标
  `ohos.permission.GET_NETWORK_INFO`，BindSocket 没有）；401/2100002/2100003 为其错误面。
- `net_connection_type.h:226`：`typedef struct { int32_t netId; } NetConn_NetHandle;` 单字段。
- 措辞为 "Binds a socket to the specified network…Socket constructed by the user"，
  **未限定 socket 类型**（未见仅 TCP 的字样）；对 UDP fd 的实际效果待真机确认。
- 加载方式：dlopen/dlsym 运行时解析（ice.rs 的 `libc.so` 惯用法），不新增链接依赖；
  句柄按本 crate 惯例保留（不 dlclose）。

## 变更

- ETS（`NetBirdVpnExtensionAbility.ets`）：`maybeArmN2hCollector` 在调 native arm
  **之前**，用官方只读 getter（`getAllNetsSync` + `getConnectionPropertiesSync`，
  经 `NetBirdInterfaceStatsProbe.ets` 新增的 `resolveVpnTunNetId`）找 interfaceName
  恰为 `vpn-tun` 的 NetHandle；找到 ⇒ 非秘密整数 `probe_net_id` 进 opts JSON；
  0 命中/重名/异常 ⇒ 键省略（探针行为与现状逐字一致）+ 一行
  `N2H_POSCTL_PROBE_NET_ID_UNAVAILABLE|…|reason=…|code=…`（只记 code token）。
  getter 语义与既有 NETCTX 快照一致（GET_NETWORK_INFO，均 Sync）。
- native（`client/core/src/n2h_collector.rs`）：`ArmOptions.probe_net_id: Option<u32>`
  严格校验（正整数、≤i32::MAX，0 是「无网络」哨兵；负数/小数在文档解析层即拒），
  非法 ⇒ `invalid-opts` 拒绝 arm。`send_posctl` 创建 socket 后、`send_to` 前，
  仅当该键存在才对**该 fd** 尝试 `OH_NetConn_BindSocket`；绑定发生在
  before 采样之后，故 before/after 采样对恰好把绑定夹在中间。
- 新 metadata 行（每条 a2b 发送恰一行；不进 Evidence/judge/PosctlProbe 形状）：

  `N2H_POSCTL_BIND_NET|probe_id=…|requested_net_id=…|result=bound|no-lib|no-symbol|failed|not-requested|errno_or_code=…|diag=true|is_evidence=false`

## 失败降级与纪律

- 库缺失/符号缺失/调用失败：**如实记录、发送照旧**——send 语义（bind/dst/字节/次数/
  生命周期/成功失败文案）逐字不变，send 结果按原样返回，从不被绑定读数覆盖。
- `errno_or_code` 仅在 `result=failed` 时携带 BindSocket 的真实返回码；
  dlopen/dlsym 无数字契约，其失败记 0，**dlerror 文本绝不打印**。
- **不预设 mark 一定改变**：mark 仍输出原始十六进制，不解码位布局；绑定后
  mark/bound_device 是否变化由 after 采样如实回答。
- 无 `probe_net_id` 时不加载库、不尝试绑定（测试用 host 上「尝试必得 no-lib」的
  确定性证明了这一点）。

## 测试（host，全离线，不连设备）

- core 新增 6 测：字段契约冻结；无键 ⇒ 零绑定尝试 + not-requested 行 + ctx 两行不变；
  有键且符号可用 ⇒ 恰一次且只对探针 fd（与 ctx before 行的 fd= 相等）；
  no-lib/no-symbol/failed 三种降级 ⇒ 如实记录且 send 仍成功到达；
  host 真实 dlopen 路径 ⇒ no-lib 且发送成功；ArmOptions 非法值全部拒绝。
- ETS selfcheck（run-exclusion-generation.mts）新增 PB1-PB6：找到 vpn-tun ⇒ opts
  含 `probe_net_id`；找不到/list 拒绝/重名/props 拒绝 ⇒ 键不存在 + 恰一行 marker；
  源码 pin：extension 无 @ohos.net.connection import、解析在 native arm 之前、
  module.json5 无新权限；stub 无 bindSocket/setAppNet 可调（调即 TypeError）。
- 7 套 shell 自检合计 342 项全绿；`cargo test --offline --locked` 全量结果见
  本轮 run 记录。本轮不做 HAP build。

## 仍需真机验证（未证点）

- 绑定后 a2b 是否出现在本机 TUN（from-os 入帧）、对端是否改以 overlay 地址可见；
- 绑定后 send 是否 ENOTSOCK/ENETUNREACH 等失败、BindSocket 返回码实际取值；
- mark=0x00000000 是否因绑定改变（布局未知，不预设）；
- libnet_connection.so 在 VPN Extension 进程的 dlopen 可达性（no-lib 是否真的不发生）。
- 未证：0021 中「探针走默认网络」的机制本身；本轮只是让下一轮 run 能对它证伪。
