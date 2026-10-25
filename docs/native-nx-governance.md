# native N1-Nx 门治理决议：路线 A 与 E8 Go 前提处置（T0 2026-08-30）

最后核验：2026-08-31

> **本决议已被 [`ADJ-T0-N1B-20260831-0001`](native-nx-n1b-adjudication.md) 修正**：§二.5（N6 增列 IPv6 观察类探测项）与 §二.8（E4 义务清单以 `roadmap.md:348` 原义为准，本文括注仅为映射标签不得作删减依据；N1a 不承担 E4；剩余面具名指派）。该决议另按 §二.10 授权门代码 `N1BDISC` 前置发现 campaign，并新设[开放义务台账](open-obligations-ledger.md)。冲突时以该决议为准；本文其余条款不变。

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

## 追加登记：诊断授权类别（2026-09-13，只追加不改写）

> 本节 2026-09-13 追加：只追加、不改写上文任何内容。本文 §一至 §五（含 §二 绑定条款第 10 条、§三 pre-E8 native 物理例外与「N6 未 pass 前不得开启产品实现」、§四 N3 书面法律硬前置）的既有文字一字未动；本节只是登记一个新授权类别的设立，不修改、不削弱上述任何绑定条款的效力。

- 诊断授权类别（diagnostic authorization）**已设立**：作为 `docs/evidence-schema.md`「E8 前唯一物理设备例外」段之外的**第二个显式例外**（「例外二（2026-09-13 追加）」），覆盖不消费任何 campaign/pair/evidence ID、不产生任何门结论的开发性设备操作；完整边界（适用范围/允许白名单/禁止/时限/消费语义四字段/签发纪律/证据去处）以该例外条款为准。
- 依据：跨厂商 T0 裁定 Q4（`~/harmonyos-signing/netbird-n1bdisc/reviews/t0-auth-boundary-ruling-20260912.md`，sha256 `080d4f44356d0fda8398c68c5fc8af8d4003e6669870c93a1d35efeb8490869a`，2026-09-13 现算）；事实依据 `~/harmonyos-signing/netbird-n1bdisc/records/retrospective-device-activity-20260912.json`（sha256 `90feab1f8b7af8f475a00f807e5e41246fa29ef76163a537b6152a52e98aef85`，2026-09-13 现算）；提案 `~/harmonyos-signing/netbird-n1bdisc/proposals/diagnostic-authorization-class-20260912.md`（sha256 `abb8eb499312d27653d857b2ab54b955f608af31e1d18092e8000c4e1dacdb9c`，2026-09-13 现算）**已获用户 2026-09-13 批准并落地**——提案文件按其自身边界声明保持原样、未改写，本条即为该提案状态的登记。
- AUTH 模板：`~/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-TEMPLATE-diagnostic-20260913.json`（sha256 `93330d783a937f8998f35b302159ca0920b191ee09394f3cb99ce9539b30a044`，sidecar `.sha256` 就地 `sha256sum -c` 可校验）。模板是占位符骨架，**不是授权**；实际签发须人类逐条确认，agent 不得自分配 AUTH ID，口述指示须先落盘回读确认后方可执行。
- 本类别**不放宽**本文 §三（N6 未 pass 前不得开启产品实现）与 §四（N3 书面法律前置），也不改写 `docs/n1b-disc-gate-plan.md` 任何冻结判据；campaign 类物理执行仍按既有 pair AUTH 路径须用户逐项显式授权。

## §四 修订（2026-09-13，唯一现行）：许可路线改为 AGPL

> 本节 2026-09-13 依用户决定文末追加，未改写上文任何文字；在许可路线范围内，本节是本文**唯一现行**的许可条款，与其冲突的既有表述以本节为准。本节登记用户决定与机械合规义务，**不构成法律意见**。

1. **取代声明**：本条**全文取代** §四 原文「许可法律评估（Q4，3/3 签署 + 修正案 A4/M10）」（:57-66，N3 编码前书面法律结论前置）。§四 原文自本条起仅作历史保留，**不再现行**，其「评估完成前的不变项」（禁止以 AGPL 目录源码为参考等）一并失效。为此前路线产出的缩范围提案 `~/harmonyos-signing/netbird-n1bdisc/proposals/cc-amendment-section4-cleanroom-20260913.md` 与 T0 裁定 `~/harmonyos-signing/netbird-n1bdisc/reviews/t0-section4-amendment-review-20260913.md` **保留为历史记录，不删除、不改写**；该裁定所附 C1-C6 条件**不再落地**——其全部前提（保持 MIT 洁净）已随本项目改用 AGPL 而消失。
2. **新许可基线**：本项目整体以 **AGPL-3.0** 发布（根 `LICENSE`）；历史版本按其各自当时的许可声明（MIT）授权，**不回溯改变**。
3. **上游使用范围**：可阅读、参考上游 NetBird 任意部分（含原列为例外的 `management/`、`signal/`、`relay/`、`combined/` 服务端目录）；据此产出或派生的代码受 AGPL 约束；**必须保留**上游 BSD-3 部分的版权与归属声明，不得移除；不得移除上游许可文本与署名。
4. **机械合规（取代净室机器）**：每次**上游 bump** 必须完成——拉取新 commit、跑许可扫描（REUSE/SPDX 或等效；无工具则逐文件映射人工登记）、把 bump commit 与扫描工具/结果登记到当前项目版本控制记录（`refs/netbird` gitlink 变更、`client/core/proto/README.md` 来源说明及 `THIRD-PARTY-NOTICES.md` 许可证记录）；原仓B 已退役，不再作为当前操作入口，既有历史登记保留。**净室纪律（clean-room log、commit 门检 trailer、独立席相似度抽检）自本条起不适用**，其规范与模板随本条一并作废（仓内 `docs/cleanroom-log-spec-20260913.md` 与 `scripts/check-cleanroom-env.sh` 已删除；仓B `cleanroom/cleanroom-log-template-20260913.jsonl(+.sha256)` 与 `records/cleanroom-env-baseline-20260913.json(+.sha256)` 保留为历史，随净室路线一并作废）。
5. **发布门**（「发布前」定义逐字沿用上述 T0 裁定 C2）：「「发布前」必须显式包含：(a) 向公开 remote 推送含 N3 产物（含 `alfadb/netbird-harmonyos`）；(b) tag/release；(c) HAP/内测/公开下载。」每次「发布前」必须完成：NOTICE/第三方清单刷新、SBOM 生成、AGPL 要求的 **Corresponding Source 获取方式**说明、**目标分发渠道（应用市场）条款核对**。
6. **回退条款**：若日后决定改回宽松许可，必须先移除或重写一切 AGPL 派生内容，并回到 T0 重新裁定；已发布版本不可撤回。
7. **未验证声明**：应用市场条款与 HAP 分发下的源码提供机制**尚未核对**；上游 v0.76.3 的逐文件许可映射与 SBOM **尚未重做**——本节及任何登记不得被读作已完成上述合规。
8. **不放宽其它绑定条款**：§二（含其第 10 条：门范围、顺序、阈值、SLO 或补丁预算变化回到 T0）、§三（N6 前不得开启产品实现）、§五 均未被本条修改；`docs/n1b-disc-gate-plan.md` 冻结判据不受影响。

## 追加登记：N2-H 判据设立与生效（2026-09-14，只追加不改写）

> 本节 2026-09-14 追加：只追加、不改写上文任何内容。§二 第 3/4 条与 §五「零预授权」段的既有文字**一字未动**；本节登记一个新判据（N2-H）的**设立与生效**，其适用条件严格受限（见「适用边界」），不修改、不削弱上述任何绑定条款的效力。

- **依据（可校验）**：
  - **用户批准**：2026-09-14 用户直接指示「批准」（本条即该决定的登记；提案文本见下，与 T0 草案逐字一致）。
  - **跨厂商 T0 裁定**：`~/harmonyos-signing/netbird-n1bdisc/reviews/t0-n2-protect-substitution-ruling-20260914.md`，sha256 `54c88d2713273bf1b4c0a56a7f6088699d9322eaf70a4da9b386cd2228f025b9`（sidecar 就地 `sha256sum -c` 可校验）。
  - **事实依据**（真机实测，AUTH `AUTH-DIAG-DEVICE-VALIDATION-20260913-0001`，均在仓B `diagnostics/<auth-id>/`）：
    `device-run-3.hilog.log` sha256 `e72e10553d6cbc333b8feea17193aeb34fa696d7b1bc10aaa0345d7703b28997`、
    `device-run-6.hilog.log` sha256 `00a001f11007d27b4b3f560d591297acc60814816469075e3bd66d046ea4eb7b`、
    `device-run-7.hilog.log` sha256 `8bbca9e6b2c1fd499815f30bb1b06a409e8179dbd6cacc118d787f903ac46781`。
    关键事实：`VpnConnection.protect(fd)` 与 `protectProcessNet()` **均被平台内部 `ohos.permission.MANAGE_VPN` 校验拦住**（`Caller is not allowed, need sys permissive` / `permission check failed`），且两个 API 的 promise **不 settle**；该权限在 AGC「ACL权限」列表中**不存在**；manifest 声明它会使**安装失败**（`code:9568289 grant request permissions failed`）。
    即 §二 第 3 条的**前置取证门在本平台结论为「无普通第三方可用的逐 socket bypass API」**。

- **判据 N2-H（本平台替代判据，逐字采用 T0 草案）**：
  「N2-H：普通第三方公开逐 socket bypass 不可用时，可采用端点派生的最小主机路由排除。VPN 创建及每次连接/重连前，完整冻结并枚举 management、signal、relay、ICE/STUN/TURN、WG peer、DNS 外层端点；全部排除后方可建连，解析失败、端点变化或观测不符均 fail-closed。以 TUN 包级负证据、隧道正控及端点侧投递/源地址证明实际旁路；API 返回值不作证据；撤销后复验。原 E5 登记 deviation，结论仅为 N2-H pass。」

- **适用边界（不得外推）**：
  1. 仅在「逐 socket bypass 无公开 API」结论成立的目标平台上适用；官方 SDK 一旦出现逐 socket 机制，**优先回到 §二 第 4 条原路径**。
  2. 结论只可写 **`N2-H pass`**；**禁止**出现 `protect pass`、`N2 pass`、`等同逐 socket protect`、`waived`、`N/A`。
  3. 证据与登记中必须把**逐 socket protect 义务记为 `UNSAT/未满足`**，并写明原因、替代机制、适用端点/前缀/拓扑/时间窗、oracle 结果与残余风险；不得以任何措辞掩盖。
  4. 残余范围必须明示（至少）：域名/CDN、DNS 上游、动态候选、重连换址、非 LAN/NAT 拓扑、宽前缀旁路、IPv6。
  5. 本判据**不改变** §五 两条：「任何 bundle 排除**测量**须新 T0 决定」「own-bundle 排除仅可作纵深附加，不得单独构成 pass」。

- **工具与执行**：N2-H 证据由 `nbinterop isolation-check`（`client/core/src/n2h.rs`）产出，字段含 `frozen_endpoints`/`probes`/`tun_negative`/`tunnel_positive_control`/`endpoint_side`/`counters`/`verdict`/`residual_scope`/`unsat_note`，并自带反例（注入泄漏必须判 `fail`、正控缺失必须判 `inconclusive`）。
- **未完成声明（不得被读作已达成）**：**设备侧 N2-H 证据尚未产出**；在产出并经独立审查前，本平台 N2 相关结论一律为 `UNSAT + 替代测量进行中`，且**开发期偏离**（`--ps protectDeviation route-exclusion`，日志带 `is_evidence=false|not_a_gate_pass=true`）不构成任何门证据。

## 追加登记：授权窗口时钟硬前置（2026-09-15，只追加不改写）

> 本节 2026-09-15 追加：只追加、不改写上文任何内容。本文既有条款（含 §二 绑定条款、§三 pre-E8 native 物理例外、诊断授权类别登记、N2-H 判据登记）一字未动；本节为诊断授权类别及一切设备操作授权增设**时钟比对硬前置**，自 T0 批准登记时刻起**只向前生效**，不追溯。
>
> 依据 T0 复核（2026-09-15，`anthropic/claude-opus-5` 席）与人类批准（2026-09-15 会话内）追加。

1. **硬门前置（三时钟比对）**：任何设备操作——包括但不限于清理、卸载、撤销、force-stop、安装、点击（uitest）、抓取日志（hilog）——**开始之前**，执行者必须记录并比对：**主机当前时间、设备当前时间、该次 AUTH 的 `valid_from`/`valid_until`**；确认主机时刻与设备时刻**均**落于窗口内，方可执行第一个设备命令。比对结果必须写入当次运行记录，含主机与设备各自的具体时刻、时区、AUTH 的 `valid_from`/`valid_until` 原值与窗口内/外结论；**无比对记录即视为未比对**。
2. **派发方义务（未见比对结果不得开始）**：派发真机任务的执行者/主会话，必须在**派发稿**中写明当次 AUTH 的 `auth_id` 与 `valid_from`/`valid_until` 原文，并显式要求执行者**先回报时钟比对结果、再动手**；执行者未回报比对结果之前，任何设备命令不得发出。派发稿中关于「是否在窗口内」的一切非引用性声称不构成依据，以执行者回报的实测比对结果为准。
3. **过期即失效（窗口外产物不构成证据）**：窗口外执行产生的一切设备产物**一律不得作为证据**，必须标注 `out_of_authorization_window: true`、`is_evidence: false`、`not_a_gate_pass: true`；**不得**被任何后续 AUTH 追认，**不得**被后续测量重新引用为证据。补签的 AUTH **只对其签发时刻之后**的行为生效，不追溯覆盖签发前的越界操作；越界事实本身须如实登记，不得因补签而消失。
4. **不得顺延（依据只有两样）**：任何「口头认为还在窗口内」「距过期应该还有余量」「上次检查时还在」均**不构成依据**；唯一依据是 **AUTH 文件中的 `valid_from`/`valid_until`** 与**本次实测时钟比对结果**。AUTH `validity_rule` 的「过期即失效、逾期操作须新签、不得顺延」按字面执行。
5. **失败即停**：出现下列任一情况，一律立即停止全部设备操作并上报，**不得重试、不得绕过、不得改用其它通道**：(a) hdc 不可达；(b) 设备处于锁定/需要解锁状态；(c) 主机与设备时钟偏差超过阈值（已由 T0 裁定为 **60 秒**）。时钟偏差超阈值时，窗口比对结果不可信，属**停止条件**而非告警。
6. 比对有效期与到期即停：单次比对有效期不超过 15 分钟，超期须重比对。任何设备操作序列的预计结束时刻必须仍在窗口内；剩余窗口不足以覆盖预计耗时，不得开始。窗口在操作中途到点，立即停止并登记中断点，不得完成"手头这一步"。
7. 消费状态同为硬门：比对时须一并读取该 AUTH 的 consumed / reusable / retry_allowed；consumed:true，或 reusable:false 且已消费的，与窗口外同等处理。
8. 不可测即停止：设备时间不可读、时区未知、AUTH 文件与其 .sha256 sidecar 校验不通过、同一 AUTH 存在多份内容不一致副本——任一成立即停止上报，按窗口外处理。执行硬门所需的只读时间查询命令必须在该 AUTH 的 allowed 白名单内显式列出；未列出即不得执行设备操作（读时间不得作为"未授权但必要"的例外）。
9. 绝对时刻与单调性：一切比较在带时区的绝对时刻（epoch）上进行，禁止墙钟字符串比较。本次比对记录的主机时刻不得早于同一 AUTH 上一次已登记的 checked_at；出现倒退即视为时钟不可信，按停止条件处理。禁止为通过比对而修改主机或设备系统时间。
10. 并行与单会话：同一 AUTH 同一时刻只允许一个执行者持有设备操作权；并行执行、跨会话续用须新签。

## 追加登记：N2-H 休眠与 N2 原路径恢复（2026-09-27，只追加不改写）

> 本节 2026-09-27 追加：上文 §二、§五及 N2-H 原登记一字未改。此处仅登记事实更正、适用状态与后续取证边界；既有诊断观察不追认为门证据。

1. **旧事实与适用前提**：N2-H 登记第 118–123 行所载「`protect(fd)` / `protectProcessNet()` 均被 `MANAGE_VPN` 拦住、Promise 不 settle、声明权限安装失败，故无普通第三方逐 socket bypass API」不再是现行事实。主要依据是 [`n2h-device-round-20260926-findings.md`](n2h-device-round-20260926-findings.md) 第 61–67 行的 09-26 独立盲审：`protect(fd)` **40/40 resolved**；旧超时来自应用计时器未清；线程卡死、安装失败均无留存原始日志。补充观察是仓B `diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260926-0037/verdict.md` 第 7–24、88–92 行：在该设备该固件上一次 `protect-bypass-effective` 观测。两者均 `is_evidence=false`，**不构成 N2、N2-H 或任何 N2 门结论**。因此原 N2-H 第 126 行「普通第三方公开逐 socket bypass 不可用时」与第 129 行「仅在逐 socket bypass 无公开 API 结论成立的平台上适用；官方 SDK 一旦出现逐 socket 机制，优先回到 §二 第 4 条原路径」的适用前提在当前目标平台不成立。
2. **N2-H 休眠**：N2-H 材料与结论冻结存档，不再沿用其替代判据取证。仅遇以下任一情形才可提出重开，且须 **T0 + 用户批准**：(a) 某类外层流量由系统解析器等本进程无法持有的 socket 发出，且无法改为受保护 socket（主要是 DNS）；(b) 证实 `protect` 在 `VpnConfig.create` 之前、recreate 之后或切网之后不能持续生效；(c) 在目标冻结元组上 `protect` 绕开效果被证伪。触发不自动重开、不改变门状态。
3. **回到 N2 原路径**：按 §二 第 4 条逐 socket `protect`、流量 oracle 与 fail-closed 判据取证。混合方案以 `protect` 为主；路由排除等仅可作 §五 第 70 行规定的纵深附加，**N2 证据必须在关闭附加机制时取得**。某类别若只能依赖 `protectProcessNet` 或路由排除，登记该类别 `deviation/UNSAT`，返回 **T0 + 用户批准**，不得写 `N2 pass`。
4. **前置取证待办**：§二 第 3 条前置取证门的后一半尚未书面闭合：须论证壳侧异步「open → protect resolve → 才 feed 给 native」顺序，与上游 `ControlProtectSocket` 在 `RawConn.Control` 内 connect/bind 前同步返回的语义等价，并经独立审查；调用成功及上述诊断观察不能代替该论证。
5. **评审与批准记录**：2026-09-27 T0 评审（`anthropic/claude-opus-5-5`）有条件同意，七项条件为：①完成本登记；②书面闭合上述等价论证并独立审查；③relay 改用受保护 fd、DNS 提出 N2 可证方案，不预设混合判据；④N2 证据轮关闭端点排除、`protectProcessNet`、`blockedApplications` 与开发偏离开关；⑤实现重连与切网的按需受保护 socket 补给；⑥N2 预注册覆盖全部外层流量类别 × 首连/重连/切网/recreate/撤销/fail-closed 注入，先审后测，使用 campaign 类授权；⑦N6 与 N2 合并实现、分门判定，默认路由放开前全部外层 socket 已 `protect`。DeepSeek 第二席两次派发均失败、无输出；经用户同意不补第二席，不记为第二席评审意见。用户在 DSH Web GUI（2026-09-27）原文批准：「全部同意你的意见，但是我现在出门了，设备等我回来再测试」；对「第二席不补、按任务顺序开工」回复：「1. 可以 2. 同意」。本登记不授权上机，设备测试待用户回来后另按授权流程执行。

## 追加登记：真机授权连接、期限与重试规则（2026-09-27，只追加不改写）

> 本节登记用户 2026-09-27 在 DSH Web GUI 明确同意的后续真机授权规则，并与 [`n2h-device-round-20260926-findings.md`](n2h-device-round-20260926-findings.md) §7.4 对账。仅适用于 2026-09-27 之后签发的真机 AUTH；已消费的旧 AUTH、既有计数和证据不追溯改写。本节不签发 AUTH、不授权立即上机；执行器规则尚须在下次上机前实现并用假设备验证。

1. **连接与暂停**：执行器先查询设备是否在线；不在线则自动以 AUTH 白名单内的 `hdc tconn` 尝试连接，每次连接事件最多 10 次，尝试之间留间隔。10 次均失败即停止设备操作并告知用户，记录失败；AUTH 不因此作废、不计为已消费。用户携设备回来后可在同一 AUTH 下重新连接并继续，仍须履行上机前回读确认和各项前置校验。连接尝试及其只读在线查询按授权计数登记。
2. **中途传输中断**：保留已采数据，将该行登记为传输中断；依上条重连后该行最多重跑一次，原尝试与重跑尝试均如实登记，不覆盖旧证据。只有断线或传输故障可触发此例外；结果不理想不得重跑，也不得换证据 ID 掩盖原尝试。此项仅对 §三 pre-E8 物理例外中的「单次执行不重试」开出传输中断例外，其余边界不变。
3. **期限与确认**：取消固定 `valid_from`/`valid_until` 窗口和到期即失效规则；新 AUTH 自签发起有效，直到按授权限额用完或用户明确作废。签发后、任何上机动作前，主会话仍须回读 AUTH 并取得用户确认；确认前不得发出设备操作命令，包括连接尝试。制品哈希核对、AUTH 本体与 `.sha256` sidecar 校验仍须执行。签发前或未经确认的操作不因事后补签、确认而追认为授权证据。
4. **时钟与固件**：记录主机与设备的实测时间、时区及比对结果，主机与设备时钟偏差超过 60 秒即停止；设备断连导致时间暂不可读时先按第 1 条连接，不将其视为窗口外，连接成功后再读取设备时间并检查偏差，未完成检查不得进行后续设备操作。固件仅小版本号更新不使 AUTH 作废，在证据中登记实测版本；大版本变化则停止并询问用户，不自行沿用。
5. **跨尝试总计数**：签发 AUTH 时把重连及一次允许的重跑连带增加的全部计数（含设备时间读取、制品哈希核对等）纳入跨尝试总额并留足上限。执行器按同一 AUTH 的累计值守限，不得把每次尝试的限额重新起算；限额耗尽按授权消费规则停止。

**与「2026-09-15 授权窗口时钟硬前置」的替代关系（仅限上述新 AUTH）**：

- 第 1 条（主机/设备时间及 `valid_from`/`valid_until` 均须在窗口内才可发出首个设备命令）：取代固定窗口与窗口内结论前置；保留两钟实测、偏差检查及记录，离线连接按本节第 1、4 条办理。
- 第 2 条（派发稿写入窗口原值、执行者先回报窗口比对结果、未回报前任何设备命令不得发出）：取代窗口字段和窗口结论要求；回读确认在连接前完成，连接后回报实测时钟结果，再进行其它设备操作。
- 第 3 条（窗口外产物一律非证据、过期不得追认或补签）：仅取代以已取消的窗口判定产物的部分；无 AUTH、未确认或超出授权范围的行为仍不得事后追认为证据。
- 第 4 条（唯一依据为 AUTH `valid_from`/`valid_until` 与实测窗口比对、过期须新签且不得顺延）：取代固定期限和过期新签要求，以本节第 3 条的持续有效与明确作废规则为准。
- 第 5 条 (a)（`hdc` 不可达即停且不得重试）：取代为最多 10 次有间隔的自动 `tconn`，失败暂停而非作废；(b) 设备锁定/须解锁即停、(c) 时钟差超 60 秒即停继续有效。
- 第 6 条（15 分钟重比对、预计结束须在窗口内及到点即停）：取消与窗口倒计时、到点停止有关的要求；15 分钟的时钟重比对周期继续执行，不再作窗口内/外结论。
- 第 8 条（设备时间不可读即停止且按窗口外处理）：仅当断连造成暂时不可读时以本节第 1、4 条的重连后再读取替代；连接后仍不可读、时区未知、AUTH/sidecar 校验失败或副本不一致仍停止上报。只读时间查询及连接命令仍须列入 AUTH 白名单。
- 第 10 条（跨会话续用须新签）：取代此项；暂停后跨会话可续用同一未作废且未用完的 AUTH，续用前须重新回读确认及完成前置校验。同一 AUTH 同时只准一个执行者持有设备操作权的限制不变。

原节第 7 条消费状态核对、第 9 条绝对时刻/单调性/不得改系统时间，以及第 5、8、10 条未在上文取代的部分继续有效；AUTH 与 sidecar 校验、同一 AUTH 单执行者、设备锁定停止等均不放宽。既有门判据和证据纪律不因本登记改变。

## 追加登记：N2 第一阶段标签定义（随 N2 预注册 v4 冻结，2026-09-28）

> 本节随 [`n2-device-evidence-preregistration-draft-v2-20260928.md`](n2-device-evidence-preregistration-draft-v2-20260928.md) v4 于 `2026-09-28T19:58:12+08:00` 测前冻结，只追加不改写旧治理条款。登记定义不等于授予标签，不签发 AUTH，也不追认任何诊断记录为门证据。

`N2 第一阶段通过`仅可在已冻结的第一批 A/B/C/D **28 格全体**（测前经 T0 + 用户批准并逐格登记的具体豁免除外）满足逐 socket protect、全 campaign TUN 负证据与逐事件夹心正控、物理出口及独立网络侧 oracle，经第一批工具和原件独立复核、T0 + 用户批准后授予。适用格中任何 `not exercised`、`invalid/UNSAT` 或 `UNSAT` 均阻止授予；不因未实现或未产生可选事件缩减范围。第二批 E/F/G 的 recreate、撤销和 fail-closed 尚未因此通过；此标签是阶段记录，**不是 N2 门结论，不得作为 E8/N6 或任何其他门的输入**。`N2 pass/reviewed-pass` 仍须全量条件、独立复核和治理审批；N6 单独判定，判据/豁免变更须测前回 T0 + 用户重新批准并冻结，不得测后追认。0045–0047 诊断均 `is_evidence=false`；其中 0047 名单外浏览器回报后的约 13 秒 TUN 采样空档须修正执行器、正式复测，不得用诊断记录填任何格。

## 追加登记：N2 预注册 v4.1 的 Wb 四格豁免与 Wb/Ws oracle 地点（2026-09-28，只追加不改写）

> 本节随[`n2-device-evidence-preregistration-draft-v2-20260928.md`](n2-device-evidence-preregistration-draft-v2-20260928.md) v4.1 于测前追加，只追加不改写旧治理条款（含本节之前「N2 第一阶段标签定义」一字未动）。登记豁免不等于授予标签、不签发 AUTH、不追认任何诊断记录为门证据。

1. **Wb 四格豁免登记**：第一批 A/B/C/D 中 Wb 行的 **A、B、C、D 四格**登记为逐格豁免（v4:11 机制），豁免「必须有 3 次真实生产发送」这一判据；依据是 base socket 在当前架构下结构性不发包（`peer_conn.rs:1224-1239` 的 fail-closed 结构性堵法，不改动），**唯一例外**是 `try_build` 重放中 egress 重放失败后仍无条件重放 endpoint（`wg_device.rs:2285-2302`）。四格按替代证据 E1–E6（身份/protect 登记、发送点零回执、**按 fd** 发送计数=0、四表**在册**对账、宿主抓包覆盖证明/LAN 零命中·蜂窝来源闭合、fd 全生命周期窗口覆盖）裁决，登记为 `exempted-with-substitute-evidence`，**不得写 pass**。任何 Wb fd 出现发送、或 base fd 未 protect、或 from-OS TUN 命中、**或出现 `N11_WG_FEED|replay-egress-failed`、或 adopted 时 `endpoints>0` 而该 peer 的 egress 未重放成功**，即该格 `fail` 并回 T0 + 用户重新裁决。任一替代证据缺件即 `UNSAT` 并阻止授予。**配套失败关闭小修（egress 未重放成功则不重放该 peer 的 endpoint，只收紧）另行实现与评审**，落地前不得仅凭静态论证依赖「永不发包」。
2. **适用集与豁免格的口径（修订）**：第一批**适用集 = 24 格**（M/S/R/D/I/Ws × A/B/C/D，按 v4 原判据全体达标）；**另有 Wb × A–D 4 个豁免格**，其替代证据齐备且未触发任何 fail 条件，是授予标签的**附加必要条件**（不是「适用集仍为 28 格」）。豁免格不计入「已证明生产外发」，也不得作为任何门结论输入。
3. **引用该标签的注记与主张边界（新增，须逐条遵守）**：① 今后**凡引用该标签**（任何文档、判决、汇报、台账）**必须附注记**「**Wb×A–D 以零发送替代证据豁免**」；② 该标签**不主张 Wb 行有真实生产发送受保护的证据**——Wb 四格只有身份/protect 登记与零发送负证据，任何表述不得暗示 Wb 曾真实出包并被保护；③ 本登记**不影响治理 §二.4 对 N2 门的枚举要求**，且**第二批 Wb 行的判据按 v4 原文**（E/F/G 不因本登记改变，也不因此通过）。
4. **Wb/Ws oracle 地点**：独立网络侧由「peer 自有主机物理外侧」改为 k8s 宿主机 `192.168.50.3`（`alfadb`）限时只读抓包；这是操作员侧 vantage 变更，不是设备授权；过滤（锚定 peer 侧目的）/两个抓包点/不读 conntrack/测前只读冻结/`-s 96`/类型分流/限时/限量/脱敏/原件 sha256 与删除规则/配对规则随 v4.1 冻结。设备物理接口发送元数据仍按 v4:40 独立留存。peer 应用层 `recvfrom` 回执按 Q2 采用为**辅助证据（应用层，非独立网络侧 oracle）**，不替代宿主抓包、不补救抓包缺失的格。宿主机 `setcap` 是持久特权变更：**授予**为 2026-09-28 晚由用户执行 `sudo setcap cap_net_raw,cap_net_admin=eip /usr/bin/tcpdump`（主会话 `getcap` 回读为 `cap_net_admin,cap_net_raw=eip`），**撤销**为 campaign 结束后执行 `sudo setcap -r /usr/bin/tcpdump`；授予/撤销时刻、命令原文与 `getcap` 回读（撤销后应为空）均入当批 AUTH 证据。
5. **评审与批准记录**：T0（`anthropic/claude-opus-5-5` 席，干净上下文，2026-09-28）评审意见：**「有条件」——8 项必改已逐条落实于提案 r2（见该提案文末「修订记录 r2」），主会话已核对**；**条件为用户对最终 v4.1 文本重新批准，该批准已于 2026-09-28T22:07:17+08:00 在本会话 DSH Web GUI 取得（原文 `批准`）**，本节登记即在该批准后落入。用户此前对三项修订**方向**的回复原文：`1. 可以` / `2. 已添加公钥，用户名alfadb` / `3. 可以`，仅是对方向的同意。本登记不授权上机、不授权抓包、不授权登录服务器。

## 追加登记：N2 预注册 v4.2 的 Wb C/D E1 次序窄修订（只追加不改写）

> 本节随 [`n2-device-evidence-preregistration-draft-v2-20260928.md`](n2-device-evidence-preregistration-draft-v2-20260928.md) **v4.2** 追加，只追加不改写旧治理条款（含上一节「N2 预注册 v4.1 的 Wb 四格豁免与 Wb/Ws oracle 地点（2026-09-28，只追加不改写）」一字未动）。登记规则修订不等于授予标签、不签发 AUTH、不追认任何诊断记录或旧 C/D 数据为门证据、不表示实现已验收。

1. **登记对象与批准依据**：登记用户已批准的窄修订 **R1–R4 + F1**（Wb×C / Wb×D 的 E1 次序窄例外、比较时钟精确化与防绕行禁令）。批准对象与文本以被批准的提案快照为准：[`n2-wb-e1-ordering-amendment-proposal.md`](n2-wb-e1-ordering-amendment-proposal.md)（sha256 `758148ddd07f0ca871c11e07427441f3c64d7d425f41134e38a920be0c1cfbab`；该提案文件未被改写一个字节）；批准事实见 [`n2-wb-e1-ordering-approval-record.md`](n2-wb-e1-ordering-approval-record.md)（sha256 `c6c639fd95001602559e106a5386f34f5204bd35076f6944b179e4ef54dda60f`）。用户所选选项原文（主会话 `ask_user_question` 选项文本，由派发子代理转述，逐字引用）：`批准窄修订，继续离线工作（推荐）`。本登记**不**登记用户批准发生的具体时刻，也**不**声称用户签名或授权编号。
2. **登记内容与取代范围（只覆盖相应 C/D E1 子句）**：获批 R1–R4 + F1 的逐字文本登记在冻结件文末「v4.2 追加修订记录」。R1 取代 `:57` Wb 行 **C/D 两格**「同左」（**A 格正文与 B 格「同左」不动**）；R2 取代 `:202`（E1）中 **C/D 后续代**的 before-attach 次序要求，并把初次条件明确为「每一个独立 tee 生命周期内**首次**成功接管登记的 Wb base」（删除「只在该 campaign 首次成功」的错误读法）；R3 取代 `:197`（Wb×C）替代证据列，`:198`（Wb×D）「同 C」引用随之指向 R3 后的 C 列（该行文字本身不动）；R4 精确化比较时钟（一律用核心单调时钟；`protect_mono_ms` 是壳判决/平台完成时刻**上界**，非精确平台完成时刻、非 adopt/接管时刻，不做墙钟换算）；F1 新增防绕行禁令（与 R1–R4 同时生效，不禁止另行授权的原第二批 E 受控 recreate/TUN 替换，该路径按第二批原规则另判）。**A/B 两格、任何新 tee 的首个 Wb 注册、E2–E6、Wb 零生产发送、§3 阈值、§2 配置冻结、本节之前的标签定义、第二批 E/F/G 与第二批判据，以及全部 fail 条件均不变**（任一 Wb fd 实际发送 / 未保护 base 实际被接管或使用 / protect 失败后裸发或真实 TUN 泄漏 / from-OS TUN 命中 / `N11_WG_FEED|replay-egress-failed` / adopted 时 `endpoints>0` 而该 peer egress 未重放成功 → 该格 `fail`）；**`fail` 不得被 `UNSAT` 掩盖**。
3. **禁止追认旧证据**：批准前已有/在跑的 C/D 数据、日志环与 `N2_CD_SANDWICH` 行**一律不得**被追认为符合新条款的证据；严格次序 `pre_hit_ms < protect_mono_ms < post_hit_ms` 必须在**批准并登记之后**的新窗口内实测；不得改写/补写/回填 `protect_mono_ms` / `pre_hit_ms` / `post_hit_ms`，不得用墙钟换算冒充核心单调时钟。
4. **生效前置与未决**：按提案 §7 第 1 步，严格边界（`pre_hit_ms < protect_mono_ms < post_hit_ms`，消除 `post_hit_ms >= receipt_ms` 的同毫秒可通过）须实现并测试，并由**未参与本修订设计与实现**、跨厂商 family 的独立 T0（干净上下文）完成 §8 C1–C5 符合性审查；本登记**不**表示该审查已通过、也**不**构成「实现已验收」。两批 AUTH 前置项（P4–P8、端点排除开关、抓包授权/`setcap` 登记、两抓包点、`-s 96` 等）与第二批 E/F/G 判据一律**不放宽**。本节**不是 AUTH**，不授权任何设备/服务器/抓包操作。
5. **版本哈希**：v4.1（即 v4.2 的完整旧前缀）sha256 `06db804da6ce788ac862b987fe7bc069517ccda93d467b24f4bc15746648bed1`、78511 字节（落盘后以 `head -c 78511` 复算，前缀一致）；**v4.2 整文件 sha256 `80a90cfdfeff37ad9dfa048e3e6e9df44db8b5d3dc8150d5005c189ec3a3e903`、90777 字节**，独立 sidecar 为 `docs/n2-device-evidence-preregistration-draft-v2-20260928.md.sha256`（完整文件落盘后现算）。本节**不**把治理文档自身的 hash 写入本节，避免自引用循环。
6. **kit `n2-campaign` 的 Q-d 冻结文档 pin 影响清单（只读登记；下一任务同步，本任务未改 kit）**：Q-d 规则为 `sha256 = sha256(git show <commit>:<path>)`，**不接受工作区哈希**；本修订尚未提交，故新规则目前**无法**按现 pin 机制引用（不得以工作区哈希冒充）。受影响条目：
   - `n2-campaign/AUTH-TEMPLATE-n2-batch1.json` → `frozen_tuple.preregistration`（commit `47a3793`、path 为冻结件、sha256 `06db804d…`、version `v4.1`、`frozen_at` `2026-09-28T22:07:17+08:00`、`worktree_sha256_at_pinning` `06db804d…`），以及 `frozen_tuple.governance_label_registration` / `frozen_tuple.governance_exemption_registration`（commit `47a3793`、path `docs/native-nx-governance.md`、sha256 `fa51dc37…`、各带 `section`）。
   - `n2-campaign/run_campaign.py:2238-2244` → `FROZEN_PINNED_SHA256` 常量：`preregistration` 必须等于 `06db804d…`、两份治理条目必须等于 `fa51dc37…`；`gate()` 每次实测 `git show` 重算并与该常量比对，不符即拒绝签发。
   - 文档化 pin 与版本字样：`n2-campaign/AUTH-TEMPLATE-n2-batch1.md:10-16`（本批 pin 说明）与 `:66`（Q-d 行）；`n2-campaign/README.md:6-8`（冻结依据头）与 `:451`（治理标签/豁免登记行）；`n2-campaign/judge.py:2`、`n2-campaign/run_campaign.py:2,5`。
   - `n2-campaign/judge.py::judge_pins()` 仅登记 AUTH 元组中的 pin、不据其产生判决（代码内无硬编码哈希）；`n2-campaign/selftest*.py` 引用 `run_campaign.py` 的常量而非硬编码字面量，随常量变更同步即可。
   - `status-20260927/N2-CD-RING-T0-REVIEW.md:30-31` 是历史只读审查记录（非 gate），不因本登记改写。
   - **边界**：把 v4.2 纳入 pin 需要一次提交（产生含新文本的 blob）并按同一规则重新 pin、同步更新 `FROZEN_PINNED_SHA256`；本任务不提交、不改 kit、不改远端。

## 追加登记：N2 预注册 v4.3 的 IPv6 TUN 解码窄修订（只追加不改写）

> 本节随 [`n2-device-evidence-preregistration-draft-v2-20260928.md`](n2-device-evidence-preregistration-draft-v2-20260928.md) **v4.3** 追加，只追加不改写旧治理条款（含上一节「N2 预注册 v4.2 的 Wb C/D E1 次序窄修订（只追加不改写）」一字未动）。登记规则修订不等于授予标签、不签发 AUTH、不追认任何诊断记录（含 0048–0051 与 `is_evidence=false`）为门证据、不表示实现已验收。

1. **登记对象与批准依据**：登记用户已批准的窄修订 **R1–R5 + F1**（IPv6 控制帧解码窄例外）。批准对象与文本以被批准的提案快照为准：[`n2-tun-ipv6-decode-amendment-proposal.md`](n2-tun-ipv6-decode-amendment-proposal.md)（sha256 `b1c9758752fd092d57cae4938b65c2e3fa82e052a045da500244e4442efd4cb7`；该提案文件**未被改写一个字节**，状态行仍逐字为 `DRAFT / UNAPPROVED`）；批准事实见 [`n2-tun-ipv6-decode-approval-record.md`](n2-tun-ipv6-decode-approval-record.md)（sha256 `bb95e6f43f6b8e6993392da1a851a91c5afcd3e56ac77745616e877f44866d64`）。用户回复原文（逐字，由批准记录转述）：`同意你的意见`。本登记**不**登记用户批准发生的具体时刻、**不**声称用户签名或授权编号。
2. **R5 覆盖范围口径注记（本节登记对象之一，只追加不改写）**：`:44` 的「整个 campaign 生产外层 TUN 零命中」与 `:158` 标签定义里的「全 campaign TUN 负证据」，在本修订后解释为**相对冻结的 IPv4 外层目的集合的零命中** ＋ **控制类 IPv6 帧的逐区间登记**（不计为命中，也不计为覆盖）；**负扫描覆盖仍是 IPv4-only，不因本修订扩展**。**豁免帧不构成泄漏路径的证明**，只构成「该帧类不在冻结负扫描的判据范围内」；任何「无 v6 泄漏」的更强表述都超出本修订。`:15`/`:68` 的「候选 IPv6 无对应经预审捕获方案 ⇒ 相应格 `UNSAT`」**不变**，本修订不缩减该项。**本节之前的标签定义文字一字未动。**
3. **登记内容与取代范围（只覆盖相应子句）**：获批 R1–R5 + F1 的逐字文本登记在冻结件文末「v4.3 追加修订记录」。R1/R2 只改**格式基线** `docs/n2-log-formats-20260928.md`（在 `fail_other=…` 之后追加 `ipv6_out_of_scope`/`ipv6_other`/`ipv6_scope`/`ipv6_next`/`first_v6_len`/`first_v6_dst_prefix`/`first_v6_other_{len,dst,next,dport}` 十个尾字段与字段口径；完整性判据由「`decode_fail ≠ 0` ⇒ `invalid/UNSAT`」加**唯一窄例外**（被拒帧全部由控制类 IPv6 解释）并加 (i) 逐行四和自洽 `Σipv6_scope == Σipv6_next == fail_v6 == ipv6_out_of_scope + ipv6_other` 与 (ii) `decode_fail == ipv6_out_of_scope` 两条充分必要条件，字段缺失或任一对账不成立即回到原判据（fail-closed）；既有字段名/顺序/语义与 `:100` 不变式一字未动）；R3 只限定冻结件 `:39` 的「帧丢失/解码失败」子句（`无前/后控`、`窗口空白`、`不能外推其间未知帧` 一字不动）；R4 把非控制类 v6 具名化（该区间 `invalid/UNSAT` + 具名风险事件 `tun_ipv6_other_nonzero` + 按 `:127` 全轮停测 + 回 T0/用户；目的可归属任何冻结/已知外层目的时升该格 `fail`，`fail` 不得被 `UNSAT` 掩盖）；F1 为新增禁令（含禁止把任何 IPv4 命中/非控制类 v6/未登记或对账不符的帧记成 `ipv6_out_of_scope`、禁止测后补写或回填 `ipv6_*`/`first_v6_*`、禁止把旧件 `decode_fail` 事后重算为豁免、禁止把任一 `fail` 降级为 `UNSAT` 或豁免）。**其余全部原规则不变**：`:3` 的变更程序、§3 阈值（3 次独立事件、三证齐全、正控 3/3、`±2000 ms`）与 `:39` 夹心正控、`:44` fail 条件、§2 配置冻结、适用集 24 格 + Wb×A–D 4 个豁免格、`:15`/`:68` 的 IPv6 候选无预审捕获方案 ⇒ `UNSAT`、`:68` 抓包过滤器与两个抓包点、全部既有 fail 条件、`N2_TUN_DEST`/`N2_TUN_FRAMES`/`N2_TUN_GAP` 行格式、`n2h::parse_ipv4_frame` 判据与其单测。
4. **禁止追认旧证据**：批准前已有/在跑的 0048–0051 与任何 `is_evidence=false` 诊断记录**一律不得**被追认为符合新条款的证据；不得测后补写/回填 `ipv6_*`/`first_v6_*` 字段；用于门判定的观测必须是**批准并登记之后**的新窗口。实现与获批文本的偏差按格式文档「实现偏差登记」一节记录（链未走通时 `first_v6_other_next` 记 `-`；短帧目的 hex 只记实际携带字节；扩展头集合按 IANA），字段拼写以实现与 `client/core/tests/fixtures/n2-log-samples.txt` 为准。
5. **生效前置与未决**：按提案 §7 第 1 步，kit 判定器/执行器须同步解析并强制 R1/R2 字段（`decode_fail > 0 且 decode_fail != ipv6_out_of_scope` → 原 problem 码 `tun_decode_fail_nonzero`；新字段缺失 → `tun_tee_stats_fields_missing`；R2(i) 四和不符 → 新增 problem 码 `tun_tee_stats_inconsistent`；`ipv6_other > 0` → `tun_ipv6_other_nonzero`），并由**未参与本修订设计与实现**、跨厂商 family 的独立 T0（干净上下文）逐条核对 R1–R5/F1 与代码/判定器的一一对应、`fail` 未被降级、无隐藏第三路径；本登记**不**表示该审查已通过、也**不**构成「实现已验收」。两批 AUTH 前置项（P4–P8、端点排除开关、抓包授权/`setcap` 登记、两抓包点、`-s 96`、时钟校准等）与第二批 E/F/G 判据一律**不放宽**。本节**不是 AUTH**，不授权任何设备/服务器/抓包操作（含提案 §6 的 `ip -6 addr`/`ip -6 route` 只读发现，须另行纳入 AUTH）。
6. **版本哈希**：v4.2（即 v4.3 的完整旧前缀）sha256 `80a90cfdfeff37ad9dfa048e3e6e9df44db8b5d3dc8150d5005c189ec3a3e903`、90777 字节（落盘后以 `head -c 90777` 复算并与 `git show HEAD:<path>` 逐字节 `cmp` 一致）；**v4.3 整文件 sha256 `1787075596ec5b328afe04cf4c60e0d4595ad1c9613c03b83c55421f6eb4ec94`、104048 字节**；**格式基线（R1/R2 落实后）sha256 `5b94f117f08294a9478e0bd404513a86343f5cb5c2a27286d8ef8ddcbfed2af8`、52684 字节**（旧格式文档 `7812d56162f636b59ee8b24a1e71ed284f94a295aef00b447348af4352e0f8da`、45527 字节）。独立 sidecar 为 `docs/n2-device-evidence-preregistration-draft-v2-20260928.md.sha256`，另有登记记录 [`n2-tun-ipv6-decode-registration-20261001.md`](n2-tun-ipv6-decode-registration-20261001.md)。本节**不**把治理文档自身的 hash 写入本节，避免自引用循环。
7. **kit `n2-campaign` 的 Q-d 冻结文档 pin 影响清单（只读登记；本任务未改 kit）**：Q-d 规则为 `sha256 = sha256(git show <commit>:<path>)`，**不接受工作区哈希**。本修订已由本次本地提交产生含新文本的 blob，须由后续任务按同一规则重新 pin：
   - `n2-campaign/AUTH-TEMPLATE-n2-batch1.json` → `frozen_tuple.preregistration`（path 为冻结件、`version`、`sha256`、`worktree_sha256_at_pinning`）与 `frozen_tuple.governance_label_registration` / `frozen_tuple.governance_exemption_registration`（path `docs/native-nx-governance.md`、各带 `section`）。
   - `n2-campaign/run_campaign.py:2238-2244` → `FROZEN_PINNED_SHA256` 常量（`preregistration` 与两份治理条目）；`gate()` 每次实测 `git show` 重算并与该常量比对，不符即拒绝签发。
   - 文档化 pin 与版本字样：`n2-campaign/AUTH-TEMPLATE-n2-batch1.md:10-16`（本批 pin 说明）与 `:66`（Q-d 行）；`n2-campaign/README.md:6-8`（冻结依据头）与 `:451`（治理标签/豁免登记行）；`n2-campaign/judge.py:2`、`n2-campaign/run_campaign.py:2,5`。
   - `n2-campaign/judge.py::judge_pins()` 仅登记 AUTH 元组中的 pin、不据其产生判决（代码内无硬编码哈希）；`n2-campaign/selftest*.py` 引用 `run_campaign.py` 的常量而非硬编码字面量，随常量变更同步即可。
   - 另需按提案 §5 第 5、6 行同步：`n2-campaign/judge.py`（`:1700-1710`、`:1935-1956`）、`n2-campaign/run_campaign.py:4005-4011`（故障行）、`n2-campaign/selftest_judge.py:179-181,1442`（字段白名单与负例）。
   - **边界**：本任务只读 kit、不 push、不改远端；kit 同步与重新 pin 属后续任务。

## 追加登记：产品开发方向决定——主线转向 App 产品开发，不再以 N2/N6 取证门通过为前提（2026-10-04，只追加不改写）

> 本节 2026-10-04 追加：只追加、不改写上文任何内容。本文既有文字一字未动，含 :53「N6 未 pass 前不得开启产品实现」及 :91、:96、:109 各处重申；本节只登记用户 2026-10-04 作出的新产品开发方向决定，及该决定对上述约束效力的覆盖。

- 决定事实与完整内容见 [`product-direction-decision-20261004.md`](product-direction-decision-20261004.md)：时间 `2026-10-04T21:54:48+08:00`，渠道为本会话 DSH Web GUI 用户消息——用户表示验证已持续一两个月仍无法完成 App 开发，「1. 先做一个自己能用的版本，以后再考虑上架 2. 要求与官方APP相同能力」，主会话提出登记后用户回复「同意」。
- 决定要点：①主线改为完成 App 产品开发，第一目标为自用版本（调试签名、自行安装），上架后续再议；②功能范围以官方 NetBird Android/iOS 客户端为基准对齐，SSO 登录与出口节点纳入范围（此前 `r0-charter.md` 约 :98-104 列为排除项）；③**产品实现不再以 N2/N6 取证门通过为前提**——:53、:91、:96、:109 中「N6 未 pass 前不得开启产品实现」的约束自该决定起不再作为产品主线前置；④N2 正式取证工作暂停，已有成果（本地提交、`/home/worker/n2h-kit` 工具包、授权与证据记录）原样保留，恢复时须先补完工具包未完成项（connect 步骤修复的测试、模板一致性复核、彩排与完整自检）；⑤产品验证方式为构建通过 + 用户真机日常使用，安全边界相关改动做针对性检查。
- 声明：本节及决定记录不修改任何冻结 N2 规则文本，不追认任何诊断记录为证据。
