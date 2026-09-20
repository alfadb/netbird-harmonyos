# posctl 发送 socket 只读上下文诊断（2026-09-20）

## 已证事实（0020 run，host+设备日志）

- 同一 a2b `probe_id`（nonce 严格匹配）被 host 收到：`src=100.108.171.38:46003`，
  该地址对应 `192.168.50.9`（运营者确认；.9 是家宽 DMZ 暴露 `home.alfadb.cn` 的机器）。
- 设备自身 overlay 地址为 165.x；设备 TUN 全程未见该 marker。
- 但设备内核 `/proc/net/route` 却有 `100.108/16 dev vpn-tun`；设备无 `ip` 命令；
  host 侧看到的 UDP src 字段是 `0.0.0.0` 通配（未真正观察 socket）。
- 纯 WG relay 只转发密文，不会自行改写内层源 IP——因此该接收的来源路径
  （路由选择/NAT/默认网络）**仍未证实**。本轮增量不回答这个问题，只把
  「发送 socket 实际携带了什么」从猜测变成 fd 级读数。

## 变更（`client/core/src/sys.rs` + `core/src/n2h_collector.rs`）

- `send_posctl` 在**同一个** a2b UDP socket 的 `send_to` 前后，各做一次只读采样：
  `getuid`/`geteuid`、`getsockopt(SO_MARK)`、`getsockopt(SO_BINDTODEVICE)`、mono 时刻。
  发送结果（长度/errno，取自拥有的 `io::Error`）先保存，再取 after 采样，
  诊断读数永远不会覆盖发送自身的错误或改变 Ok/Err 语义。
- bind/目的地址/发送字节/调用次数/生命周期/成功失败文案逐字不变；
  不 connect、不 setsockopt、不绑设备、不重发；bind/local_addr 失败的早退路径无诊断行。
- 输出（仅元数据，无 payload/私钥/异常文案，`diag=true|is_evidence=false`）：

  `N2H_POSCTL_SOCKET_CONTEXT|probe_id=<合法 payload marker 提取>|phase=before|after|fd=…|uid=…|euid=…|mark=…|mark_errno=…|bound_device=…|bound_device_errno=…|mono_ms=…[|send_len=…|send_errno=…]|diag=true|is_evidence=false`

- 字段语义：`mark` 输出原始 32 位十六进制（`0x…`）；读失败写 `unavailable` + 真实 errno。
  `bound_device=`（空）= 内核回答「无显式设备绑定」，**不等于**选择了物理网卡。
  只在 `send_posctl` 触发，普通网络连接无探针；不进 Evidence.counters/PosctlProbe，不改 judge。

## 只读边界与 mark 解读纪律

- 新增 sys 面（sys.rs 只读诊断节）：`getuid`/`geteuid`/`getsockopt`，仅此三者，无任何写路径。
- **mark=0 是有效的 unmarked 读数**，绝不解读为「不支持」；读失败必须带真实 errno（如 EBADF=9、ENOTSOCK=88）。
- fwmark 位布局未知：**只输出原始十六进制/数值**，不解码成 netId，也不套 Android 布局；
  任何「mark 说明走了某条路」的结论都必须先有布局依据。
- before/after 采样的是同一个 fd（测试钉死）；closed fd 读数保留内核真实 errno。

## 测试与验证（host，全离线）

- `cargo test --offline --locked` 全量（核心 lib，默认 parallel）**首次运行失败 1 项**：
  `ledger::tests::digest_is_deterministic_for_a_fixed_state`（319 passed / 1 failed，
  日志 /tmp/n2h-posctl-ctx-full-test.log）。ledger 的 TEST_LOCK 仅模块内生效、
  其它测试可修改全局 entries，属已知非隔离机制，但本次失败的唯一因果未证实；
  未据此修改 ledger。随后完整全量复跑 **555 passed / 0 failed**
  （/tmp/n2h-posctl-ctx-full-test2.log）。
- 共享测试状态隔离条件下的串行回归 `cargo test --offline --locked -- --test-threads=1`：
  **555 passed / 0 failed**（2026-09-20 合并验收，/tmp/n2h-socket-network-validation/serial-full-test.log）。
  其中 collector 47 项（42 既有 + 5 新增）全绿。
- 新增 5 测：Linux host mark=0 记为有效读数；before/after 同一 fd 且字段契约冻结；
  非socket fd（/dev/null）读失败保留 ENOTSOCK=88 且 send 结果不变（可注入 fd seam，仅 cfg(test)）；
  closed fd 保留 EBADF=9；wildcard bind 与原 UDP 收发路径端到端仍有效。
- 既有 I6 wire-shape 测试继续通过（发送语义逐字等价的回归证据）。

## 真机待测与残余风险

- aarch64 OHOS libc 符号（getuid/geteuid/getsockopt）已按 SDK 头核对（SO_MARK=36、
  SO_BINDTODEVICE=25、socklen_t=u32），但**真机读数待统一构建后上机确认**——本增量不承诺设备侧行为。
- 观测期间的并行 ETS probe 改动（entry/*）与本增量无交集；本轮不做 HAP 构建、不 commit。
- 未触范围：路由/NAT/默认网络结论仍开放，等待真机 fd 读数与后续 run 的证据链。
