# Device Validation Plan — 真机平台层验证（S0–S8）

SPDX-License-Identifier: AGPL-3.0-or-later
Copyright (C) 2026 NetBird HarmonyOS contributors

日期：2026-09-13（同日拓扑更正版）。目的：主机侧六项里程碑已对真实
netbird-server 0.78.1 打通（docs/interop-run-1-20260913.md），本计划验证主机侧
永远验不到的**平台层**：HAP 安装/签名、VpnExtension create/destroy、逐 socket
protect、fd 合同、真实 TUN 数据面、撤销复原。**前提**：设备操作须先有例外二项下
人类逐条确认的生效 AUTH（草案见仓外
`diagnostics/AUTH-draft-device-validation-20260913.json`）；本计划本身不是授权。
目标 bundle `cn.alfadb.netbird`；证据去处
`~/harmonyos-signing/netbird-n1bdisc/diagnostics/<auth-id>/`，`is_evidence:false`，
全文件 `.sha256`，凭据一律占位（`${SETUP_KEY_REDACTED}` 等）。
记录纪律：**任何一步未执行就写「未验证」，不得用推断/主机侧结果顶替**。

## 运行拓扑（2026-09-13 主机侧只读核实）

- 本机是 **k8s pod**（eth0 10.98.0.180/24，MTU 1450；serviceaccount 无 RBAC，
  不能用 NodePort/Service 暴露）；pod 有公网出口，可路由到用户局域网
  192.168.50.0/24（实测 ping 网关 192.168.50.1 可达），但**反向大概率不通**。
- 设备 `192.168.50.199`（历史 tconn 地址），hdc 走 TCP：`hdc tconn
  192.168.50.199:37193`；hdc 二进制
  `~/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/toolchains/hdc`。
- **服务端与主机侧 peer 都在 pod 内**；设备侧 management/signal 走端口转发的
  TCP 反向转发（设备 `127.0.0.1:18080` → pod `18080`；hdc 命令中设备→pod 方向
  为 `rport`，`fport` 为 pod→设备，按实测语义使用，两条均在 AUTH 白名单）。
  服务端 `exposedAddress http://127.0.0.1:18080` 因此**保持不变是正确的**：
  两端各自在本地解释该地址（pod 内 peer 直用 127.0.0.1，设备经转发）。

## S0 预检与转发建立

- 目的：固化目标元组，建立连接与转发。
- 人工：让手机与 pod 所在环境网络可达（手机侧无需配置）；确认 hdc 调试授权。
- 命令：`hdc list targets` `[只读]`；未连接则 `hdc tconn 192.168.50.199:37193`
  `[写-连接]`；`hdc shell param get const.product.model`、
  `const.product.software.version`、`const.ohos.apiversion` `[只读]`；
  `hdc rport tcp:18080 tcp:18080`（或实测等价 fport）`[写-连接]`；
  `hdc rport ls` `[只读]`。
- 期望证据：target token；PLA-AL10 / 7.0.0.105(SP10C0E105R7P3) / API 26（漂移即
  记录实际值）；转发清单行；服务端在 pod 内 `[只读]` 存活。
- 失败即停：设备不识别、元组异常且用户未确认继续、转发建立失败。

## S1 签名与安装

- 目的：debug HAP 上机。**当前卡点**：client bundle 为 `cn.alfadb.netbird`，仓内
  仅有 `cn.alfadb.netbird.n1bdisc` 的 debug profile（不得混用，
  docs/windows-development-handoff.md：profile 必须匹配 App ID）。需用户为
  `cn.alfadb.netbird` 签发 debug profile（AGC/DevEco，绑现有证书与设备）。
- 人工：提供新 profile `.p7b` → 仓外 `profiles/`；或明确改走 n1bdisc bundle。
- 命令：`bash client/build.sh`（pod 内 `[写]`，未签名 HAP）→ hap-sign-tool 签名
  （复用 candidate-signing helper）→ `hdc file send` HAP 至 /data/local/tmp
  `[写]` → `hdc install` / `bm install` `[写]` → `hdc shell bm dump -n
  cn.alfadb.netbird` `[只读]`。
- 期望证据：签名 HAP 路径/字节数/sha256；verify-app/verify-profile 日志；
  `bm dump` 的 bundleName/versionCode/installTime。
- 失败即停：签名校验失败、install 报错（不得改系统设置强装）。

## S2 控制面

- 目的：设备侧 login/Sync/signal registered。
- 人工：准备配置文件（见「配置下发」）经 `hdc file send` 推送 `[写]`；确认 S0
  转发在位。
- 命令：`aa start`（带 requestId）`[写]`；hilog 抓取 `[只读]`。
- 期望证据：hilog `VPN_ONCREATE` → `VPN_CONNECTOR_STARTED|mgmtSocket=protected`
  → `VPN_SIGNAL_FED` → status 行 `signalRegistered=true`；服务端日志
  `peer registered [..]`（interop 里程碑①②③同款）。
- 失败即停：`VPN_CONNECTOR_ABORTED`（credentials/dns/protect 类）、授权弹窗外的
  未知系统弹窗。

## S3 protect 与 fd 合同（§二.4）

- 目的：逐 socket protect（mgmt/signal/ICE/WG）+ fail-closed + fd 所有权。
- 命令：S2 启动流内自动发生；另验 fail-closed：pod 内停掉服务端后观察重连 dial
  被拒、无 unprotected 回退 `[只读]`。
- 期望证据：hilog 每个 socket 的 `VPN_PROTECT_BEGIN` → `VPN_PROTECT_RESOLVED`
  （datagramsFlown=none-yet）；failure 路径 `VPN_PROTECT_FAIL_CLOSED` →
  `VPN_CONNECTOR_ABORTED`/destroy；TUN fd `CREATE_ACCEPTED`；native 仅 dup
  （`fd_status` 审计，原始 fd 只由 destroy 关）。
- 失败即停：任一 socket 先连后 protect、或 fail-closed 路径未触发。

## S4 数据面（**尽力验证 + 明确失败判据**）

- 已知约束：STUN/UDP 不能经端口转发 → 设备 srflx 候选收集失败（**无害**，host
  候选仍可用，须记录 srflx 缺失本身）；UDP 反向（手机 → pod 10.98.0.180）大概率
  不可达 → ICE 很可能需 prflx 路径才能建成。
- 判据（逐条记，不许含糊写「数据面未通过」）：
  a) 候选交换：signal 帧 OFFER/answer 中 host 候选双向可见（signal registered
     前提下）；b) 握手尝试：hilog/status `wgReady`、`wgSessions`、`handshakes`；
  c) 若 ICE 失败：精确记录失败点（无候选/候选不可达/握手超时）+ 所需条件，
     例如「需要在 192.168.50.0/24 内的机器上运行主机侧 peer」。
- 探针方式：`hdc shell ping` 类流量会**绕过 VPN**，ICMP 不作主判据；沿用本地
  投递 sink / 用户态探针（host-wg-peer 先例）。
- **回退方案**：把主机侧 peer 二进制与配置打包，交用户在局域网内某台机器
  （k8s 宿主机或工作站）运行——需用户先提供该机器的访问方式；回退启用即记录。
- 期望证据：`VPN_NETCFG_APPLIED`（address/routes/dns）、
  `VPN_DEFAULT_ROUTE_GATE|allowed=false|reason=default-route-held:*`（数据面未
  就绪时 0.0.0.0/0 不得入 VpnConfig）、握手后 `wgReady=true|wgSessions>=1`、
  allowed_ips 双向探针、`VPN_RECREATE_*`（默认路由释放路径）。
- 失败即停：数据面未就绪却出现 default route；黑洞且无法撤销。

## S5 撤销与清理

- 命令：`aa force-stop` / VPN 关闭 `[写]`；`hdc uninstall cn.alfadb.netbird`
  `[写]`；复查 `bm dump`、`pidof` `[只读]`；`hdc rport rm`（或 fport rm）清理
  `[写-连接]`。
- 期望证据：`VPN_ONDESTROY` → `VPN_CONNECTOR_STOPPED` → destroy once；destroy 后
  路由/DNS 复原（设备设置页 VPN 断开）；无 `cn.alfadb.netbird:vpn` 进程残留；
  卸载后 bundle 不存在；转发清单清空。
- 失败即停：进程/路由残留、二次 destroy 报错未收敛。

## S6 证据归档（pod 内）

- hilog 全量（脱敏扫描后）、状态 JSONL（每次操作时间+操作+结果）、服务端日志
  片段 → `~/harmonyos-signing/netbird-n1bdisc/diagnostics/<auth-id>/` +
  `.sha256` + `is_evidence:false` 声明文件；生成 `MANIFEST.sha256`。
- 会话结束或 valid_until 到点：AUTH `consumed:true` 回填，不自动续期。
- 系统侧日志/快照的采集命令与「掉线、被杀、崩溃、网络异常必须当场抓」规则见
  `/home/worker/n2h-kit/tools/README-collect.md` 的「掉线/异常现场取证」一节。

## S7 灭屏保活（screen-off keep-alive）

- 改写说明：原「休眠唤醒后重连」场景在当前设计下测不到——2026-10-26 两轮真机实测
  证明，长时任务保活使系统对 netbird 的 Doze 被豁免（hilog
  `Doze has special:ERR_HAS_CONTINUOUS_TASK`），灭屏期间 VPN 不会被冻结，「休眠后
  重连」这条边根本不会触发；灭屏场景实际验证的是「保活是否让连接保持」。真正打断
  连接的是进程被系统回收，见 S8。
- 执行时机：S4 数据面已通、S5 撤销之前（编号按文末追加，执行顺序在此）；只观察
  灭屏/唤醒对现网连接的影响，不改任何配置。
- 目的：确认灭屏静置期间连接保持（peers 不归零、无 presence-cleared、无进程换代），
  唤醒后状态与基线一致。
- 前置：连接正常（status 行 `peers_online>0`、`vpn-tun` 接口在）；已配置 ≥2 个对端。
- 步骤（可执行、可计时）：
  1. 记录基线：`peers_online`/`peers_relay`、主进程与 `:vpn` pid、`vpn-tun` 收发
     字节；记录设备本地时间 + 主机时间各一，便于与 hilog 对齐。
  2. 锁屏静置 5–10 分钟：按电源键熄屏，不手动断网、不退出 App；记录灭屏时刻与
     唤醒时刻（设备时间 + 主机时间双标）。
  3. 唤醒后 0–10 分钟内每 30 秒记录一次：连接状态、`peers_online`/`peers_relay`、
     是否出现 presence-cleared/reconnecting、是否换代、距唤醒的恢复耗时。
- 判据：
  - 通过：灭屏期间连接保持（peers 不归零、无 presence-cleared、无进程换代），唤醒
    后状态与基线一致。
  - 观察：灭屏期间断开但唤醒后自动恢复；记录恢复耗时与卡在哪一阶段（如
    `stage=signal-resolve`、DNS EPERM）。
  - 失败：断开且未恢复，或需人工干预（重启 App/手动重连/手动切网）才恢复。
- 必须记录（缺一不可，否则本场景不能算通过）：
  - 系统是否对 netbird 豁免 Doze：hilog 搜 `ERR_HAS_CONTINUOUS_TASK`，记录该行
    时刻、进程名与 pid；同窗口被正常 Doze 冻结的应用可作对照。
  - `wall−mono` 漂移：真挂起时该漂移应约等于灭屏时长；若远小于灭屏时长，说明进程
    没有被挂起，本场景只覆盖「灭屏不打断」，不覆盖「挂起后重连」。
  - hilog 最大间隙：灭屏段内相邻日志行的最大间隔；与灭屏时长同量级才说明设备真的
    静默，间隙过小同样说明没挂起。
- 执行记录（2026-10-26 真机已执行；证据在 `artifacts/analysis-20261026-s7-run2/`）：
  - 第二轮（干净窗口，判**通过**）：灭屏 20:39:39.885 → 唤醒 20:47:11.733
    （451.6 s，落在 5–10 分钟内）；Doze 被 `ERR_HAS_CONTINUOUS_TASK` 豁免；
    `wall−mono` 漂移仅 22.773 s（远小于 451.6 s，证明确未挂起）；休眠段 92 条状态
    发布，peers 11/5 → 12/5 → 12/6 → 12/5，无 0/0、无 presence-cleared、`:vpn`
    进程零换代；唤醒前后状态同为 12/5，连接全程保持。
  - 第一轮（无效场景，只能记**观察**）：实际累计挂起 5.49 h（非声称的约 19 h）、
    `:vpn` 21 小时内换代 23 次（全部 `PRIOR_EXIT_UNCLEAN`）、唤醒后 23.656 s 恢复；
    场景有效性不成立，不能据此宣布「休眠唤醒后重连」已验证。
- 失败即停：判失败后仍需继续观察时，先按 S5 撤销并当场留证（hilog 全量 + status
  JSONL）；不得改写判据凑通过。

## S8 进程被系统回收后重连（process killed → auto reconnect）

- 改写说明/理由：白天 23 次换代**全部**是非优雅退出（`PRIOR_EXIT_UNCLEAN`），最后
  一次能确认是系统 LowMemoryKill（hilog `KillHandlerInner LowMemoryKill` /
  `PROCESS_KILL reason=LowMemory`）。进程被系统回收才是实际打断连接的主要路径，与
  S7 的「灭屏不打断」互补。
- 执行时机：S4 数据面已通、S5 撤销之前；S7 之后执行。
- 前置：连接正常（status 行 `peers_online>0`、`vpn-tun` 接口在）；最好能制造低内存
  压力（打开多个大应用）或等系统回收。
- 步骤（可执行、可计时）：
  1. 记录基线：`peers_online`/`peers_relay`、主进程与 `:vpn` pid、`vpn-tun` 收发
     字节；记录设备本地时间 + 主机时间各一，便于与 hilog 对齐。
  2. 触发系统回收：制造内存压力，或等待系统自然回收；记录触发时刻。
  3. 确认进程被杀：hilog 搜 `PROCESS_KILL`/`LowMemoryKill`，或用 pid 变化确认
     `:vpn` 进程已被换掉；记录被杀时刻与新进程 pid。
  4. 观察新 `:vpn` 进程启动与重连：0–10 分钟内记录连接状态、peers、恢复耗时。
- 判据：
  - 通过：回收后 60 s 内自动恢复连接且 peers 回到基线，全程无需用户干预。
  - 观察：60–300 s 恢复，或恢复但 peers 少于基线。
  - 失败：>300 s、需手动打开应用、或未恢复。
- 必查观测（逐条记，缺一不可）：
  - 新 `:vpn` 进程 pid（与基线 pid 不同即确认换代）与主进程 pid 是否变化。
  - `PRIOR_EXIT_UNCLEAN` 是否在重启首报，含 `prev_state` 值。
  - 重连是否走 `initial=1`/`carrier-warmup`（区分冷启动全量重连与复用路径）。
  - DNS EPERM 是否出现，带 generation 号以便定位是哪一代进程。
  - 恢复耗时（相对进程被杀时刻，秒）。
- 参考：0.0.15/0.0.20 的自动重连策略（`NetBirdReconnectPolicy`）在系统回收且 UI 在
  前台/保活时的行为；核对点：系统回收不经过 `onDestroy`，marker 不会被写成
  clean/stopping，下一会话应报 `PRIOR_EXIT_UNCLEAN` 并按策略自动重试，而不是被
  marker 门跳过。
- 失败即停：确认进程已被杀但 10 分钟内未恢复且无自动重试记录时，先按 S5 撤销并当场
  留证（hilog 全量 + status JSONL），不得把该场景记成通过。

## 代码改动（本计划配套，已实现）

- `client/entry/.../NetBirdConnector.ets`：新增 `DEV_CREDENTIAL_DIR`
  （`/data/local/tmp`，注释明示 DEVELOPMENT-ONLY、非安全存储）与
  `readCredentialFileText`：同名文件存在于 dev 目录则读它（打
  `CRED_DEV_OVERRIDE` warn），否则回退 filesDir；两处皆缺 → 抛错 → 启动中止
  （fail-closed 不变量不变：无凭据不建 VPN）。`NetBirdVpnExtensionAbility.ets`
  两处凭据读取改走新函数。core 未改，294 个 Rust 测试全绿。

## 风险与未验证项

- App 沙箱可能不可见 `/data/local/tmp`（accessSync 失败→自动回退 filesDir；若
  filesDir 也写不进 = 阻塞，须用户给出替代下发路径）。
- 手机→pod UDP 反向大概率不可达：S4 按尽力验证执行，失败时按 a/b/c 判据落档，
  回退方案需用户局域网机器配合；在此之前数据面结论只能是 blocked/未验证。
