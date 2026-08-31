# native N1-Nx 门治理决议：路线 A 与 E8 Go 前提处置（T0 2026-08-30）

最后核验：2026-08-30

本文持久化 2026-08-30 跨厂商 T0 委员会（`anthropic/claude-opus-5`、`xai/grok-4.6`、`openai/gpt-5.6-sol` 三席；主会话只编排不投票）对 [T0 材料包](t0-native-nx-gates-materials.md) 六问的表决结果与用户批准的最终决议。这是 N0 决议（第 5-10 条）预注册的后续治理，其触发条件（N0 pass + 物理 E3 pass）已满足且证据强于预期（追加 G0 物理实测 blocked 与 Go 1.27 对照）。审议材料包两处事实错误已更正（API 版本归属、N1/N2 环境列），材料包降级为历史审议材料；冲突时以本文为准。

## 决议元数据

```yaml
decision_id: ADJ-T0-NATIVE-NX-20260830-0001
date: 2026-08-30
timezone: Asia/Shanghai (+08:00)
seats: [anthropic/claude-opus-5, xai/grok-4.6, openai/gpt-5.6-sol]
ballot: Q1-Q4/Q6 全票 SIGNED；Q5 原案否决（grok 拒签）、按三席调和文本通过
user_approval: granted 2026-08-30
supersedes: E8 OPEN 必要条件中的 E1 Go 专属前提（见 §3）
status: active
```

## 一、路线决议（Q1，3/3 签署 + 修正案 A1）

**路线 A（ArkTS 系统壳 + C++ NAPI 薄桥 + 单一 native core）为唯一分阶段可行性路线。**

- 本签批准的是 N0 方向 B 的**分阶段可行性验证架构**，不是产品实现授权、不是全面原生重写开闸；R0 章程与 N0 决议第 8-10 条（fork 禁令、不给工期、不实现第二协议面）不变。
- "唯一可行性路线"≠"已证可行"：避开的是**已实测的 stock Go c-shared IE-TLS 阻塞**（E1/G0 双元组同错误族 + Go 1.26/1.27 形态未变）；native gRPC/QUIC/ICE、OHOS arm64 加载与平台 API 仍须逐门验证。
- **路线 B（进程外 Go 可执行）与 E 方向关闭**：两席独立交叉取证（2026-08-30）一致确认 phone 无公开机制——华为官方 FAQ 明文"当前禁止三方应用在手机设备上 Fork 进程"；子进程/fd 传递 API（startChildProcess/startArkChildProcess/startNativeChildProcess、NativeChildProcess_FdList≤16）设备门控全部排除 phone（801/16000061/NCP_ERR_NOT_SUPPORTED）；SELinux neverallow 应用沙箱文件执行；HNP exec 被开发者模式门控且 vpn_isolate_hap 隔离沙箱不含其挂载。
  - B 的关闭为**时点取证结论**，登记重开触发器：官方文档将上述子进程/fd API 的设备支持扩展到 phone，或出现普通三方公开进程启动+fd 传递机制。
  - 与 `EV-R1-EMU24-20260717-0008`（公开 Native Child API 面身份 PASS）对账：公开 SDK 面存在 ≠ phone 设备支持，二者不矛盾。
  - 关闭即不开 exec 探针（N0 第 6 条）。

## 二、N1-Nx 门骨架（Q2，3/3 签署 + 绑定修正案 A2）

七门分解、双轴 `reviewed-pass/pass`、Emulator/物理双向不外推、N0 停止条件全盘沿用。以下为**绑定条款**（"开门前细化"只能细化证据方法，不得删改或削弱）：

1. **N1 拆分**：N1a（Emulator 可执行：BoringTun + tun.Device 等效数据泵 + 非 VPN 回环 fd，验证吞吐/背压/资源）与 N1b（物理冻结元组：VpnExtension 真实 fd 集成 + 握手 + 双向真实报文）。理由：`EV-E3-API24-EMU-MATRIX-20260717-0001` 实测三形态 Emulator 均缺 VPN 授权注册组件，VpnExtension fd 在 Emulator 不可执行；若未来官方 image 补齐组件，须按既有纪律从 E3 重验后才能移回 Emulator。
2. **N1 fd 合同**（N1b 实测建立，不得继承）：平台原始 fd 由 `VpnConnection.destroy()` 唯一关闭；native 只消费 dup 副本（优先 `F_DUPFD_CLOEXEC`）；禁止把原始 fd 交给 BoringTun/tun.Device。须登记所有权表并实测：dup 语义与 CLOEXEC、单一 close 责任、`O_NONBLOCK` 对共享 open-file description 的副作用、destroy 后 fd 有效期与重复 close、EAGAIN/部分写/背压/shutdown unblock、EBADF/复用/泄漏。E3 探针契约（`fcntl(F_GETFD)` 只读、禁 dup）与 Tailscale-OHOS（dup 副本契约）冲突，不可混引；Tailscale-OHOS 整体定性为"不可复现、不可引用"，仅作设计候选。
3. **N2 拆分**：N2a（壳侧路由/DNS 声明与 API 面）与 N2b（物理：protect 实证 + 撤销复验）。**前置取证门**：先确证目标 API 存在普通第三方可用的逐 socket protect/bypass 公开 API，及其线程/同步语义能否满足 NetBird `ControlProtectSocket`（`RawConn.Control` 内 connect/bind 前同步返回）等价契约。
4. **N2 判据**：建连前逐 socket protect + 可观察流量证明未入隧道（枚举 management、signal、relay QUIC/TCP/UDP、ICE/STUN/TURN、WG peer、DNS 外层 socket × 首次连接与每次重连）；protect 失败 fail-closed；API 返回值不构成证据。路由/DNS 由壳声明，VPN 撤销后复验。
5. **N6 必须逐条覆盖 R0 必选功能十项**：setup key 注册+注销/本地状态清理、management+signal 维持、direct+relay 双路径、多 peer（>2）、IPv4、NetBird routes、NetBird DNS、MTU、断网/切网/重连、凭据（setup key/token/节点身份）轮换。"子集"仅适用于 N5 ICE 行为子集与非 R0 必选的 ACL；真机安全存储验证显式指派 N7。行为 oracle 为真实自托管 v0.76.3 + 官方客户端可观察行为（不得只用 IDL 自洽）。
6. **N7**：沿用 R0 SLO 数值；R3"native 数据泵与 Go/WireGuard 路径均有双向真实流量"改为"native 单核双向真实流量"（Go 路径已证不可达）；不得放宽连接/重连/吞吐/资源阈值。
7. **判据预注册**：每门判据、oracle、停止条件、不外推声明须在测量开始前书面登记并经独立审查确认，测量后不得追认或修改。
8. **物理前置**：N2b 物理前须有冻结元组 arm64 同核心加载证据（补 N0 compile-only）；E4（TUN 配置/地址/MTU/IPv4/清理）与 E6（双向泵）映射到 N1a/N1b，E5→N2b，E7→N2/N6，SLO→N7，映射不免除义务。
9. **语言/栈**：N3 开门前冻结（tonic+prost+quinn 为当前最优信号，非本决议冻结；材料包 §2.3 外部事实须补来源 URL 与访问日期后才可作栈选型输入）。
10. 门范围、顺序、阈值、SLO 或补丁预算变化必须回 T0。

## 三、E8 Go 前提处置（Q3，3/3 签署 + 修正案 A3/A7-A9）

**选项一**：E8 OPEN 的 Go/E1 前提替换为 **N6 `reviewed-pass/pass`**；E1 转休眠门。其余必要条件不变（哈希一致、E3-PHYS-PREFLIGHT 已 `reviewed-pass/pass`、独立聚合审查显式 OPEN）。

- 不构成标准降低：N6（物理冻结元组端到端、双 peer、direct+relay、真实自托管 oracle、R0 必选功能全文）在覆盖面与证明强度上严格强于 E1（loader 冒烟）。
- **pre-E8 native 物理例外（消除门序倒置）**：N1a-N7 的物理工作在 E8 `CLOSED` 期间按**受治理例外**进行——每次物理 campaign 独立 AUTH/pair、冻结元组与输入哈希、白名单 HDC、单次执行不重试不换 ID、双向不外推、禁止性能/长稳/渠道/产品外扩（沿用 E3-PHYS-PREFLIGHT 与 G0 既有实践）。E8 OPEN 的语义相应变为 **N6 之后的产品/R 门投入许可**——这是明示治理，不是静默替代 E1。N6 未 pass 前不得开启产品实现。
- **E1 dormant 登记**：E1 从"所有客观可执行 Emulator 项均为 reviewed-pass/pass"集合中显式排除（状态记 dormant，不得写成 pass/N/A/waived）；`EV-E1-EMU24-20260809-0003` 与 `EV-G0PHYS1API26-20260830-0001` 既有判定与绑定不改写。
- R0 章程与 roadmap 的 E8 条款随本决议同步修订（见 §六）。

## 四、许可法律评估（Q4，3/3 签署 + 修正案 A4/M10）

**N3 硬前置**：在任何 N3 IDL codegen、复制/转译参考实现或协议实现提交之前，取得**书面、可执行**的专业法律结论（"已委托评估"不满足）。评估对象：

1. `shared/` BSD-3 声明映射的效力（.proto IDL 与客户端参考实现按仓库声明位于 BSD-3 侧；AGPL 例外仅顶层 `management/`、`signal/`、`relay/`、`combined/` 服务端目录）；
2. 根 LICENSE 例外文本与 `LICENSES/REUSE.toml` 映射冲突时的优先级（`combined/` 差异即此类）；
3. 从 BSD-3 侧 .proto 与参考实现派生代码的义务、署名与 NOTICE 要求；
4. 以 `shared/relay` 为 oracle 的再实现边界；分发形态（应用市场/HAP）下的归属与 SBOM 义务。

评估完成前的不变项：全路线禁止复制 AGPL 目录代码、禁止以 AGPL 目录源码为实现参考（可观察行为与 `shared/` BSD 侧源码除外）；结论为义务触发或不确定时立即返回 T0，执行者不得自行判定。净室不作默认，仅评估结论要求时启用。在取得书面结论前，不得把"担忧已收窄"写成已合规。

## 五、bundle 排除 fallback（Q5 原案否决，三席调和文本）

**零预授权**：N2 pass 必须是逐 socket protect + 流量 oracle。若 N2b 实测 protect 不可达（取证门结论为无公开 API，或实测不满足同步契约，且经独立审查），**停止并返回 T0**，不得自动 fallback。任何 bundle 排除**测量**须新 T0 决定；将 bundle 排除**采用**为 E5 等效判据须新 T0 + 用户批准，且证据中必须把 E5 原义务登记为 deviation/未满足，不得写成 pass。own-bundle 排除仅可作 protect 的纵深附加，不得单独构成 N2 pass。若官方 SDK 后续出现逐 socket 机制，优先回到逐 socket 路径。

## 六、E1 休眠触发器（Q6，3/3 签署 + 修正案 A6/M12 全文）

E1（stock Go loader/runtime 门）转为 dormant，既有判定保持原绑定。触发重开的充分条件（满足任一即可）：

- (a) Go 官方**正式 release**（非 beta/tip/未合入 CL 或 proposal）合入 general dynamic TLS（golang/go#71953 或等价），且该形态变化可由 stock 工具链复现、实测 c-shared 不再呈现目标 loader 所拒的 initial-exec TLS 形态；
- (b) NetBird 官方正式采用新工具链或构建方式（不以本项目需维护的 fork 为前提），且其 c-shared 产物的 TLS 重定位形态改变并有可复核产物依据；
- (c) **平台 loader 侧**：冻结元组或其后继官方 build 的动态 loader 接受 initial-exec TLS 解析到 dynamic definition（拒绝方为设备 loader，独立于 Go 的重开路径）。

触发仅授权"提交新 ADJ/新 AUTH + 新 evidence ID + 新冻结元组下的一次重测"，不自动改变 E1、E8 或任何门状态；已对照仍为 IE TLS 的版本（含 1.26.x/1.27.0）不构成触发；禁止以私有 fork、未合入补丁或偏离 stock 构建作为触发。

## 七、后续治理备注（非阻塞，席位关切）

- native 再实现不计入 R0 补丁数（补丁定义针对上游偏离）；约束转为 compat oracle 符合性与 fork 禁令。须防止把本应计补丁的上游偏离改称"再实现"规避预算；行为偏离登记与上限机制留待后续 T0。
- N0 决议矩阵中 management/signal 权威路径标注与实际位置（`shared/` 侧）不符——以本次逐路径纠正为准，历史文档不改写。
- BoringTun 0.7.1 crate 缺 LICENSE 文件（N0 已记）：SBOM/NOTICE 待办，不阻塞。
- 本决议不改变：N0 决议全部条款、R0 章程（除 §三 所述 E8 条款同步修订）、历史 evidence 与判定、E8 其余必要条件。
