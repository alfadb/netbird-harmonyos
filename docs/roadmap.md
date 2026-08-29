# 双目标实施路线图

最后核验：2026-08-15（仅表示文档状态核验）

本文定义 `netbird-harmonyos` 从当前研究阶段到两个具名平台目标发布及持续运维的证据门路线图。路线图中的平台能力、接口、性能和发行路径均须由对应阶段的 SDK、代码、制品、渠道和具名真机证据确认；在证据形成前，只作为待验证假设。

## 当前起点

### 当前实测

- **E3 S5 production-layout 现行规则（2026-08-15）**：[`0004 sealed production layout 登记`](evidence/e3-physical-preflight-production-layout-2026-08-15.md) 已取代 `ADJ-20260808-0003` 当时的“无可信样本 / 无 ExpectedBundle / 不要求 `app-info-structure`”保守 matcher。当前 S5 step3 必须在 Settings owner 下唯一可见 `Setting.AppDetail` 子树内，匹配 expected A/B 的 distinct label 与同子树 force-stop control；历史 ADJ 保持当时事实、不改写。step4 post-force 页面形状或一般 capture 退化是 observation-only，不独立改变 verdict；连续 HiLog stream degraded 仍依全局安全规则 blocked。
- **N0 原生客户端可行性门已双轴验收 `reviewed-pass/pass`（2026-08-10）**：N0(a) 固定 NetBird v0.76.3/f65f7b34 协议/行为/许可 inventory 与 compat oracle 已定义（[N0 决议](n0-native-client-feasibility.md)）；N0(b) 由 [`EV-N0-EMU24-20260810-0002`](evidence/n0-native-core-emulator-reviewed-pass-2026-08-10.md) 在真实官方 API 24 x86_64 phone Emulator 上实测单一 native WG core（BoringTun 0.7.1 `ffi-bindings`）加载/冒烟 PASS，`record_status: reviewed-pass`、`verdict: pass`（终审 `REV-N0-EMU24-20260810-0002`：`openai/gpt-5.6-sol` + `xai/grok-4.5` 双路只读独立复算 0 blocker/0 major，分别 2/6 minor，均为非阻塞记录改进建议、不扩范围修）。能力接受使用固定正式发布与真实 official Emulator runtime 数据，不是合成 fixture；不证明 VPN fd/TUN/protect/management/ICE/relay/UI/arm64 load/physical/product；下一步不是继续实现（仅未来用户显式授权后 `E3-PHYS-PREFLIGHT` 第一物理动作，N0+E3 pass 后才新 ADJ/T0）；E8 仍 `CLOSED`。
- 仓库已增加短生命周期 `spikes/r1-api24-hap` 研究探针并实际构建 unsigned API 24 HAP 和双 ABI 普通 Node-API 库；`EV-E0-EMU24-20260717-0001` 已以 `reviewed-pass/pass` 关闭 E0。`EV-E1-EMU24-20260717-0005` 又在三个不同普通 `EntryAbility` PID 中各完成 10 轮 C-only ArkTS/native/fd probe，现为 `reviewed-pass/pass`；独立审查确认 0 blocker/major、5 minor，且不改变 measured artifact。`EV-E2-EMU24-20260717-0002` 随后在三个新 PID 中各完成 E1 完整回归和 10 轮纯 C TCP/UDP loopback、Pod 本机受控 endpoint、确定性 DNS/错误及资源恢复，并保留三张可见 E2 PASS 页面；该 E2 记录现为 `record_status: reviewed-pass`、`verdict: pass`，E2 已关闭。研究探针不是产品应用工程、产品测试套件或持续集成配置。
- 独立 `spikes/e3-vpn-extension-hap` 已 clean-build 两个普通 bundle，并由正常 Entry UI 实测公开 VPN Extension start/stop。`EV-E3-EMU24-20260717-0003` 及补充 `0004` 均保持 `reviewed-pass/blocked`，历史 evidence 与 raw 判定不改写。精确 API 24 x86_64 phone image 缺少授权前置组件，记录的公开 runtime 路径不可继续，不能产生 `pass`；E4-E7 因此前置依赖未启动。
- 新隔离目录 `spikes/e3-vpn-extension-physical-preflight-hap/` 已完成 API 23 受限 A/B 适配、普通 debug 开发签名、四项验签与 signed 内容审计，未修改历史 E3 树或 raw。FINAL A/B HAP、profile/certificate、源码/SDK map、runner 与 campaign 复合输入曾冻结；A/B 仅 `INTERNET`、VPN `exported=false`、唯一 arm64 native 成员为纯 C `libfdprobe.so`。唯一 initial live 已登记为 `EV-E3-PHYS1API23-20260806-0001`（`reviewed-pass/blocked`），因 build drift 在 continuous capture/install 前停止。`EV-E3-PHYS1REBIND7-20260806-0001` 已完成三条只读 rebind（`reviewed-pass/pass`，仅 rebind 范围）；`ADJ-20260806-0003` 冻结 HarmonyOS 7 新元组并曾批准准备 campaign `E3-PHYS-PREFLIGHT-20260806-0002` / evidence `EV-E3-PHYS1API26-20260806-0001`（从未 Live、未占用；`ADJ-20260807-0001` 标 `superseded-unexecuted`）；host reverify 已 PASS（public manifest `66a70a52c92b927d4b23e528ae6eaf1b52169e504291c6ff0e7efa4c7ffee010`；HAP/signature/profile/member hashes 未变；不主张安装兼容性）。`ADJ-20260806-0004` 授权的单条 build 确认 `EV-E3-PHYS1BUILD7-20260806-0001` 已登记（`reviewed-pass/pass`，仅 build-confirm）：`const.product.software.version` 精确投影 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` 与冻结 binding 逐字匹配，消除合成 build 风险；API `26` 仍由前一 rebind 实测，不从 build 推断。`ADJ-20260807-0001` 曾将未执行 API26 campaign 跨日重新编号为 `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001`；该组 ID 已 Live 并 operator-aborted 登记为 [`EV-E3-PHYS1API26-20260807-0001`](evidence/e3-physical-preflight-api26-2026-08-07.md)（`reviewed-pass/blocked`，`consumed-blocked`；禁止局部 scenario5 重放；保留）。`ADJ-20260807-0002` 批准的中文完整重跑已 Live 并登记为 [`EV-E3-PHYS1API26-20260807-0002`](evidence/e3-physical-preflight-api26-0002-2026-08-07.md)（`reviewed-pass/blocked`，`consumed-blocked`；S1/S4 pass，S2/S3/S5/S6/S7 blocked；cleanup `verified-clean`；双审查 0 B/5 M；opus timeout attempt-not-counted）。历史（2026-08-10 前）`plan_status: blocked-awaiting-device-authorization`（`ADJ-20260807-0003` 已批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径；runner/freeze/selftest 随执行 commit `e3fe0c6` 更新，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)；candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`；用户显式设备授权 + fresh device confirmation 完成前无 auto retry/新 ID/设备命令授权；本登记禁 HDC）；旧 unsigned hash 只绑定历史准备阶段。2026-08-10 用户（直接人类决策者）已显式授权**新**完整白名单 campaign（[`AUTH-E3-PHYS1API26-20260810-0002`](evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)：`authorization_status=granted`、`device_readiness=user-attested-ready`（用户就绪声明**不**替代机器 fresh confirmation）、`machine_fresh_confirmation=pending`、`plan_status=authorized-awaiting-windows-ready-freeze`；**取代**已消耗的 [`AUTH-E3-PHYS1API26-20260810-0001`](evidence/e3-physical-preflight-authorization-2026-08-10.md)，新候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`（旧 pair `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 因 sealed blocked 已消费不得复用；audit-1/audit-2 均 hash 记录）；新增**一次 host-prep `hdc list targets` 内存级窄例外**（恰一 token、设 process-scope `PHYS_1_TARGET`、不输出/持久化、不扩大 runner HDC 白名单）；attempt=initial/retry N/A、任一 gate 失败停止、不重试不换 ID；Live 前允许 runner+治理变更审查后提交/推送，campaign evidence 仍不提前提交；允许范围仅现行 runner 白名单，禁止任何额外 discovery/UDID/serial/`hidumper`/root/privileged/Go/NetBird/product）。当前 Windows signing/build host preflight 复测 `ready_for_commit_and_ordered_preflight`（COMPUTERNAME=ALFADB-V-WIN、pwsh 7.6.4、HDC binary 定位+文件 hash 未执行、历史 freeze/签名 HAP/profile/cert 齐备、EvidenceRoot/RawRoot 存在、HDC 进程 0 未发设备命令；尚待未提交 clean final commit、新 pair audit-1/audit-2、全新 blocked/ready freeze、confirmation/review records、新 EvidenceRoot/RawRoot、process-scope `PHYS_1_TARGET` 映射；非全部 Live-ready）→ 按序执行：同步 trusted refs/bundle → ID 审计①（hash 记录）→ blocked confirmation freeze 静态审查（`independent_review_ready=true` 仅表示契约/角色静态就绪）→ host-prep `hdc list targets` 映射（一次，内存级，恰一 token，设 process-scope `PHYS_1_TARGET`，不输出/持久化）→ `-TargetBindingConfirm`（互斥模式，3 条白名单 target-binding 命令产出仓外双文件 confirmation record + `.sha256` companion，不建 evidence roots、不设 `is_evidence`、不消费 campaign/evidence ID；固定候选 pair 与 `attempt=initial`（retry N/A；generic retry 分支不进入本路径）；drift → blocked + 非 0，无 cleanup 查询）→ 全新仓外 `ready` freeze draft 绑定 record（`machine_fresh_confirmation.status=pass` + `authorization_id` + `record_path` + `record_sha256`，绑定 Windows 同步后精确 clean HEAD 与重建后的仓内 runner/freeze example/selftest SHA-256（以最终 commit 为准）及完整外部 hash，不得原地改旧 blocked freeze）→ 独立审查 record（`e3-ready-freeze-review`）→ 最终 `ready` freeze 绑定 review → ID 审计②（hash 记录）→ selftest `HDC_PROCESSES=0` → 对同一最终 `ready` freeze DryRun `is_evidence: false`/HDC0/integrity empty（`ready` DryRun 同样验证 confirmation + review 双绑定，`blocked` DryRun 允许 `pending`（review 同规则：声明 `status=pass` 时同样完整校验，machine `pending`/缺省 而 review 声明 `pass` → 明确拒绝，review `pending` 允许））→ 审查 DryRun 且 freeze 字节不变 → 单次 Live；`blocked`/`fail`/`invalid`/no seal 后不得自动重跑（当前 AUTH 固定 initial，任何 retry 须新治理）；E3 未关闭，E8 保持 `CLOSED`。
- [E3 API 24 Emulator 矩阵审查](evidence/e3-vpn-extension-api24-emulator-matrix-2026-07-17.md)已统一登记官方 API 24 x86_64 phone、2in1 与 Tablet。phone 记录覆盖公开 runtime 与注册前置边界；2in1、Tablet 记录只覆盖 registration-layer 前置核查，按停止条件未安装 HAP，不能扩写为完整 runtime 不可执行结论。记录不互相替代、不外推，也不要求为 E8 伪造 pass。
- HarmonyOS 命令行工具链、SDK、Linux Emulator、镜像和基础恢复入口已经准备完成。
- Emulator 曾在运行约 25 分钟后出现 HDC target 仍显示 `Connected`、但 shell RPC 连续超时的退化；该观测要求 E7 使用有界短循环并保留故障证据，不把 25 分钟以上长稳列为 Emulator 总门必过项，也不缩减 E0-E8 中可在 Emulator 客观执行的 VPN 验证。

### 尚未验证

- 任一目标 ABI 上的 NetBird Go 核心、WireGuard 数据面、ArkTS/native/Go 桥接和完整应用生命周期。
- 普通第三方应用在任一具名量产设备上的 VPN 授权、虚拟接口、socket 保护和真实业务流量。
- 任一渠道的正式签名、审核、重签、安装、升级、发布和回滚闭环。

### R1研究进度

2026-07-17，[R1 Go ABI 预探针与 API 24 HAP 构建证据](evidence/r1-go-abi-preflight-2026-07-16.md)已记录固定 NetBird/Go/SDK 的编译和静态风险，并用 CLI 6.1.1.290、API 24、Hvigor 6.24.3 实际构建 unsigned Stage 应用/测试 HAP 及 arm64-v8a/x86_64 `libprobe.so`；后续 API 24 Emulator 研究当时确认可见 pixelMap、普通 `EntryAbility` 被 `10106102` 阻塞，而短生命周期 TestRunner 可直接完成 Node-API 初始化、`ping=pong` 和版本断言；后来的 `EV-E0-EMU24-20260717-0001` 已用正确解锁序列消除该运行阻塞，并已作为 E0 `reviewed-pass` 关闭。0006 的 Go 1.25.12 Linux/amd64 c-shared 直接 `dlopen` 在 initial-exec TLS 处受控失败；0007 进一步证明无 Go、无 `res_search`、无 `NODELETE`/`SYMBOLIC` 的普通 initial-exec TLS so 同样被拒绝，且正确 `DT_NEEDED` wrapper 的 runtime 传递加载也在依赖 Go so 处被拒绝。`ADJ-20260717-0001` 后的 0008 确认公开 Native Child API 对 API 14 及以后只支持 PC/2in1、Tablet，因而在实现前暂停 API 24 phone B 族。第二个六席 T0 共识 `ADJ-20260717-0002` 随后批准单次 Tier1 纯 C 动态 TLS loader 探针；0009 用最终 ELF 实证确认 IE 为 `PT_TLS+TPOFF64+STATIC_TLS`、classic GD 为 `DTPMOD64+DTPOFF64+__tls_get_addr`、TLSDESC gnu2 为 `R_X86_64_TLSDESC`，并附加 local-dynamic，所有 link 均禁用 relax。API 24 x86_64 TestRunner baseline 先通过，IE 按 0007 类别拒绝且无环境漂移，classic GD、TLSDESC 和 local-dynamic 均成功加载；每个动态模型的主线程、加载前等待线程和加载后新线程分别以不同值完成初值42、100轮 set/read、reset42，0 错误、0 crash，Tier1 为 `PASS`，不触发停止该 x86_64 元组。该正面只证明精确 C loader 模型，不证明 Go 1.25.12 会生成或支持这些模型；Go `#71953`、CL `644975`、CL `696635`、PR `75048` 均未合并、未发布，只作下一道独立 Tier2 门参考。arm64、华为商用 HarmonyOS 具名真机和其他设备/loader/toolchain 保持 provisional，补丁数仍为 0；R0、R1、R2 均未退出，也没有新增产品证据。

## 当前基线与总门状态

当前R0正式基线（现v0.76.3）固定为 commit `f65f7b347ee4e7de6d98c488d3d894cd018b02b6`，`go.mod` 声明 `go 1.25.5` 与 `toolchain go1.25.12`；官方 Release run [`31256677326`](https://github.com/netbirdio/netbird/actions/runs/31256677326) 成功，固定 URL 的访问日期为 2026-08-09。相对 v0.74.7 前进 140 个 commit；`netbirdio/wireguard-go` replace 回到 `2834bebf6c1aea76bd217f31ea91c99f75e4a20a`（与 v0.74.6 相同）。安全公告 GHSA-qcpp-8vwj-hhwr（high，影响 `>=0.5.0,<0.76.0`，`0.76.0` patched）为本次升级动因之一。固定源码研究见 [Tailscale-OHOS VPN 数据通路审计与 NetBird 映射](tailscale-ohos-netbird-port-audit.md)（已同步 v0.76.3）。未发布分支、patch set、PS4 和 Go 1.26.5 均不是当前门输入。

v0.74.6、commit `3a2f773d655d88d16ed953fc2a114a4e690a1b08` 与 v0.74.7、commit `a1c9427d8004576e2cbb9e546d409847fa9df318` 继续作为所有既有 E/R evidence 的历史实际输入。采用 v0.76.3 不静默替换历史版本、源码、制品 hash 或判定；后续受影响门必须用 v0.76.3 新记录重跑。

API 24 x86_64 phone Emulator 总门当前为 `CLOSED`：E0、E1-C 和 E2 为 `reviewed-pass/pass`。v0.74.6 历史 loader 负面证据保持原绑定；当前R0正式基线（现v0.76.3）的官方 Go 1.25.12 loader/runtime 由 `EV-E1-EMU24-20260809-0003` 实测 `reviewed-pass/blocked`（[证据](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md)），仍无 `reviewed-pass/pass`，E1 overall Go 未关闭。E3-E7 以 reviewed dependency-blocked aggregation exception 进入后续聚合，不是 `pass` 或 `N/A`；E4-E7 完整义务移交 E8 `OPEN` 后的具名物理设备 R2/R3 门。

**N0 状态（2026-08-10）**：N0 原生客户端可行性门已双轴验收 `reviewed-pass/pass`（[`EV-N0-EMU24-20260810-0002`](evidence/n0-native-core-emulator-reviewed-pass-2026-08-10.md)，终审 `REV-N0-EMU24-20260810-0002` 0 blocker/0 major）。N0 pass 只接受固定正式发布（NetBird v0.76.3/f65f7b34、BoringTun 0.7.1）与真实 official Emulator runtime 数据的单一 native WG core C ABI 加载/冒烟，不是合成 fixture；不证明 VPN fd/TUN/protect/management/ICE/relay/UI/arm64 load/physical/product。下一步不是继续实现：仅未来用户显式授权后 `E3-PHYS-PREFLIGHT` 第一物理动作，N0+E3 都 pass 后才提交新 ADJ/T0 治理定义 native N1-Nx 门；治理生效前 E8 保持 `CLOSED`，不得以 native 静默替代 Go E1，不得开启产品实现。

E8 前唯一 `E3-PHYS-PREFLIGHT` initial live 已于 2026-08-06 消费。冻结目标为 `PLA-AL10` / `PLA-AL10 6.1.0.117(SP6C00E115R7P7)` / API `23` / arm64；live model 匹配，但 live build 仅投影为 `PLA-AL10 <REDACTED_IPV4>(SP8C00E32R7P2)`，可见 suffix 发生 drift。runner 在 continuous capture、staging 与 install 前停止，`campaign_started=false`，A/B 未安装或运行，finally 清理为 `verified-clean`，integrity violations 为空。

证据 `EV-E3-PHYS1API23-20260806-0001` 为 `record_status: reviewed-pass`、`verdict: blocked`；独立审查 0 blocker/0 major。`reviewed-pass` 不是 E3 pass。记录无 `infrastructure_reason`，build drift 不授权 `infrastructure-blocked-retry-1`；旧 campaign/evidence 不可复用，无 infra retry。`ADJ-20260806-0002` 授权的 rebind discovery 已形成 `EV-E3-PHYS1REBIND7-20260806-0001`（`reviewed-pass/pass`，严格只表示三条 rebind 完成，不是 E3/campaign pass）：API `26` / `aarch64` / `arm64-v8a`。`ADJ-20260806-0003` 随后冻结新元组 `HarmonyOS` / `PLA-AL10` / HDC build `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` / Settings `7.0.0.100 (SP8C00E32R7P2patch09)` / API `26` / `aarch64` / `arm64-v8a`，批准复用原 FINAL HAP hashes（兼容性仅作实测、不证明成功），并曾批准准备 campaign `E3-PHYS-PREFLIGHT-20260806-0002` / evidence `EV-E3-PHYS1API26-20260806-0001`（从未 Live、未占用；`ADJ-20260807-0001` 标 `superseded-unexecuted`）；host reverify 已 PASS（public manifest `66a70a52c92b927d4b23e528ae6eaf1b52169e504291c6ff0e7efa4c7ffee010`；HAP/signature/profile/member hashes 未变；不主张安装兼容性）。`ADJ-20260806-0004` / `EV-E3-PHYS1BUILD7-20260806-0001` 以单条 `software.version` 实测确认 HDC build 与冻结 binding 逐字匹配（`reviewed-pass/pass`，仅 build-confirm，不是 E3/campaign pass），消除合成 build 风险；API `26`/`aarch64`/`arm64-v8a` 仍由 rebind 实测、不从 build 推断。`ADJ-20260807-0001` 曾将未执行 API26 campaign 跨日重新编号为 `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001`；该组 ID 已 Live 并 operator-aborted 为 `consumed-blocked`（[`EV-E3-PHYS1API26-20260807-0001`](evidence/e3-physical-preflight-api26-2026-08-07.md)：`reviewed-pass/blocked`；scenario5 Settings 误确认并直接关窗；recovery cleanup verified_absent；独立 seal 审查 0 B/M；禁止局部 scenario5 重放；保留）。`ADJ-20260807-0002` 批准的中文完整 1–7 重跑已 Live 并登记为 [`EV-E3-PHYS1API26-20260807-0002`](evidence/e3-physical-preflight-api26-0002-2026-08-07.md)（`reviewed-pass/blocked`，`consumed-blocked`；S1/S4 pass，S2/S3/S5/S6/S7 blocked；cleanup `verified-clean`；双审查 0 B/5 M）。历史（2026-08-10 前）`plan_status: blocked-awaiting-device-authorization`（`ADJ-20260807-0003` 已批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)；candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`；用户显式设备授权 + fresh device confirmation 完成前无 auto retry/新 ID/设备命令授权；本登记禁 HDC），E3 未关闭，E8 保持 `CLOSED`。2026-08-10 用户已显式授权**新**完整白名单 campaign（[`AUTH-E3-PHYS1API26-20260810-0002`](evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)：`authorization_status=granted`、`device_readiness=user-attested-ready`、`machine_fresh_confirmation=pending`、`plan_status=authorized-awaiting-windows-ready-freeze`；**取代**已消耗的 [`AUTH-E3-PHYS1API26-20260810-0001`](evidence/e3-physical-preflight-authorization-2026-08-10.md)，新候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`（旧 pair `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 因 sealed blocked 已消费不得复用；audit-1/audit-2 均 hash 记录）；新增一次 host-prep `hdc list targets` 内存级窄例外（恰一 token、设 process-scope `PHYS_1_TARGET`、不输出/持久化、不扩大 runner HDC 白名单）；attempt=initial/retry N/A、任一 gate 失败停止、不重试不换 ID；Live 前允许 runner+治理变更审查后提交/推送，campaign evidence 仍不提前提交；当前 Windows signing/build host `ready_for_commit_and_ordered_preflight`（工具/签名输入齐备；尚待未提交 clean final commit、新 pair audit-1/audit-2、全新 blocked/ready freeze、confirmation/review records、新 EvidenceRoot/RawRoot、process-scope `PHYS_1_TARGET` 映射；非全部 Live-ready），按序执行：同步 trusted refs/bundle → ID 审计①（hash 记录）→ blocked confirmation freeze 静态审查 → host-prep `hdc list targets` 映射（一次，内存级，恰一 token，设 process-scope `PHYS_1_TARGET`）→ `-TargetBindingConfirm` 机器 fresh 确认（3 条白名单命令、仓外双文件 record + `.sha256` companion）→ 全新仓外 `ready` freeze draft 绑定 record hash → 独立审查 record（`e3-ready-freeze-review`）→ 最终 `ready` freeze 绑定 review → ID 审计②（hash 记录）→ selftest → 同一 `ready` freeze DryRun → 审查且 freeze 字节不变 → 单次 Live；用户就绪声明不替代机器 fresh confirmation；`blocked`/`fail`/`invalid`/no seal 后不得自动重跑（当前 AUTH 固定 initial，任何 retry 须新治理））。完整记录见 [API23 物理预检证据](evidence/e3-physical-preflight-2026-08-06.md)、[API26 0001 blocked 证据](evidence/e3-physical-preflight-api26-2026-08-07.md)、[API26 0002 blocked 证据](evidence/e3-physical-preflight-api26-0002-2026-08-07.md)、[host remediation 证据](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)、[rebind 证据](evidence/e3-physical-rebind7-2026-08-06.md)、[build 确认证据](evidence/e3-physical-build7-confirm-2026-08-06.md) 与 [2026-08-10 授权登记 0002](evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)。

## T0 共识记录

2026-07-16，OpenAI、Anthropic、DeepSeek、Moonshot、MiniMax、Zhipu 六个厂商参与了四轮 T0 讨论，并最终形成一致结论：采用单一主执行者，以证据门串行推进首目标，再独立验证和发布第二目标；模拟器不作为产品证据，支持范围必须绑定具名发行版、设备、完整系统版本、架构和渠道。

2026-07-17，第二次路线级 T0 也取得同一六个厂商席位一致批准：在不修改 Go、NetBird 或 SDK 的前提下，以一个工作日、一次实现完成 Tier1 纯 C 动态 TLS loader 门；只有验明的 classic GD 或 TLSDESC 至少一个通过加载前/加载后线程隔离测试才可进入下一道独立决策门，两者都失败时只停止 API 24 x86_64 元组，不外推 arm64 或真机。

本文只记录参与厂商和最终共识，不记录讨论过程或模型商业信息。

2026-08-09，第三次路线级 T0（T0+/T0 五席：`anthropic/claude-fable-5`、`openai/gpt-5.6-sol`、`kimi-coding/k3`、`minimax/MiniMax-M3`、`xai/grok-4.5`）经三轮讨论，Round 3 十条决议逐条 `SIGNED`、最终 `ROUND3 FINAL BALLOT: UNANIMOUS-SIGN`，用户批准后形成 **N0 现行研究门**：当前立即且唯一无物理设备门，固定 NetBird v0.76.3 协议/行为/许可清单与 compat oracle，只选一个许可清晰的 native WireGuard core 完成 OHOS x86_64 Emulator 加载与 arm64 交叉构建最小 compile-only 证据（arm64 不声称加载）；不实现 management/ICE/relay/UI，不授权物理设备 HDC，不给工期。完整决议、A-E 处置、双轴验收、停止条件与后续 ADJ 条件见 [N0 原生客户端可行性门](n0-native-client-feasibility.md)。**E8 保持 `CLOSED` 不变**：N0 与物理 E3 都 pass 后须先提交新 ADJ/T0 治理定义 native N1-Nx 门并处理 E8 的 Go 专属前提，治理生效前不得以 native 静默替代 Go E1 或开启产品实现；既有 E 门与历史 ADJ 记录不改写。

**N0 后续结果（2026-08-10）**：按 N0 决议执行，N0(b) 已由 [`EV-N0-EMU24-20260810-0002`](evidence/n0-native-core-emulator-reviewed-pass-2026-08-10.md) 实测 `reviewed-pass/pass`（终审 `REV-N0-EMU24-20260810-0002`，0 blocker/0 major），N0 overall 双轴验收通过；N0(a) inventory+oracle 已定义。N0 pass 不改变 E8 `CLOSED`：下一步不是继续实现，仅未来用户显式授权 + fresh confirmation 后执行既有 `E3-PHYS-PREFLIGHT` 第一物理动作；N0 与物理 E3 都 pass 后才提交新 ADJ/T0 治理。

## 动态调整机制

本项目为研究型项目，持续存在不确定性。路线图是基于当前证据的 living plan，不是冻结计划；执行中新证据可以调整尚未开始阶段的范围、顺序、实现路线、退出标准和版本映射。

- 总目标及信息状态纪律保持稳定：已证实、待验证假设和未证实主张必须继续依据相应证据明确区分。
- 每次调整须记录提出角色、日期与时区、触发证据、调整原因、受影响阶段、已评估替代方案、R0/SLO/补丁预算影响、重跑范围、生效条件、回退条件和审查状态。
- 已完成阶段的原始证据不得删除或改写；后续结论应通过新增记录说明其适用范围。
- 新证据若推翻已通过阶段的关键前提，须重新打开该阶段及其依赖阶段，并重新满足相应退出标准。
- 人类直接决策者对重大技术方向具有优先权。有明确的人类直接决定时，该决定可在记录所限定的这一次调整中替代内部 T0 触发，但调整记录必须明确决定范围、已评估替代方案、生效条件、回退条件和审查状态；不得把该替代写成已进行或已达成 T0。
- 没有明确人类直接决定时，涉及首目标、核心数据面、跨语言边界、VPN 能力门、发布门、支持声明，或放宽 R0 已锁定的质量、安全、性能、隐私 SLO 或错误预算阈值的重大调整，仍须按既有规则重新进行 T0 讨论。用户明确要求“T0”时，无论是否存在可适用的人类优先规则，均按 T0 协议执行并记录真实结果。
- 局部且可逆的实现调整可按本机制正常修订；不得为维持原路线而忽略反证，也不得因轻微实现差异频繁重排全局路线图。
- 调整后须更新阶段依赖摘要、版本映射和完成定义，确保本文仍然自洽。

### 2026-07-17 人类投入顺序决定

项目投入顺序先关闭 API 24 x86_64 phone Emulator 上所有客观可执行项。Emulator 的 PASS、FAIL 与 blocked 均不得外推到 arm64 或物理设备；arm64 ABI、真实硬件、物理网络、硬件密钥、能耗、渠道和长稳仍是 E8 `OPEN` 后的真机专属项。2026-07-18 的路线调整只为 E3 可达性增加一个严格受限例外，不开放其他物理设备工作。

### ADJ-20260806-0001：Settings re-allow 路径偏差判定与 campaign 冻结

- **提出与批准角色**：用户（直接人类决策者）；于 2026-08-06 直接批准。本记录按“人类直接决策者优先”规则替代这一次内部 T0 触发，不声称执行了 T0。
- **日期与时区**：`2026-08-06`，`Asia/Shanghai (+08:00)`；未虚构未提供的秒级批准时间。
- **触发原因**：执行前对 Settings 场景的最佳预测是系统直接激活 A，但系统实际可能再次展示普通授权 UI。旧规则要求实际路径严格等于冻结预测，可能把不影响功能结论的 UI 路径差异误判为 blocked。
- **受影响阶段**：只影响 `E3-PHYS-PREFLIGHT` 场景 5 的路径偏差分类、专用计划与 E8 预检输入解释；不改变 E1、E4-E7、R0-R10 的既有退出标准。
- **调整内容**：`settings_reallow_expected_path` 仍冻结为 `direct-system-activation`，实际路径偏差作为预注册观测保留，不因偏差本身 blocked。此记录替代 `ADJ-20260718-0001` 中“路径必须严格相等”的判定规则，仅替代该一项，不放宽功能判据。
- **功能判据**：场景 5 仍必须重新激活 A 并取得有效 fd、从普通 Settings 入口撤销、观察匹配的 destroy terminal 与 post-destroy fd cleanup。无 Settings 入口、无法重新激活、缺 destroy terminal 或缺 post snapshot 仍为 blocked。场景 3/6/7 同样按适用分支要求 destroy terminal 与 post-destroy snapshot。
- **已评估替代方案**：继续严格路径相等会把 UI 表现差异混入功能结论；把预测路径删除会失去预注册观测；允许 force-stop/uninstall 替代 Settings 会破坏普通用户路径证据。采用“保留预测并观测偏差、功能判据不变”。
- **R0/SLO/补丁预算影响**：不改变首目标候选、必选功能、任何质量/安全/性能/隐私 SLO、错误预算或补丁上限，不授权代码、SDK、Go、NetBird 或 WireGuard 补丁；当前补丁计数不变。
- **重跑范围（批准时事实）**：本调整批准时 campaign 尚未开始，无既有设备证据需要重写或重跑；首次且唯一执行使用本记录。后续 initial live 已因输入漂移按回退条件停止，不能局部重跑场景 5。
- **冻结身份**：target code `PHYS1API23`；campaign ID `E3-PHYS-PREFLIGHT-20260806-0001`；evidence ID `EV-E3-PHYS1API23-20260806-0001`。该组 ID 已由 initial live 消费，不得复用。
- **执行与审查角色**：operator=`authorized user`；orchestrator=`main agent`；reviewer=`独立 deepseek/deepseek-v4-pro 隔离会话`。
- **证据存储**：只引用 `controlled external EvidenceRoot/RawRoot`；EvidenceRoot 存脱敏 manifest/projection/判定，RawRoot 仅在实际产生时保存 raw，仓外受控保存至少 90 天，争议时保存至争议关闭。signed HAP/profile/certificate 保留至 E8 审查结束。
- **命令白名单**：唯一 runner 为 `spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-campaign.ps1`。只准两条 model/build target-binding 复核（零新增身份信息）、定向 A/B bundle/PID/install/start/cleanup、单一连续 `E3PhysVpn` HiLog、A/B fault、screen/layout；禁止全量、UDID、serial、`hidumper`、`uiInput` 与特权命令。
- **生效与状态**：initial live 已执行并形成 `EV-E3-PHYS1API23-20260806-0001`。live build 投影与冻结 build 可见 suffix drift，runner 在 continuous capture/install 前停止；`campaign_started=false`、cleanup `verified-clean`、`plan_status: blocked`。记录为 `reviewed-pass/blocked`，E3 未关闭，E8 保持 `CLOSED`。本 ADJ 的 Settings 路径判据仍有效；后续新元组冻结与新 campaign 准备见 `ADJ-20260806-0003`。
- **回退条件**：本次冻结输入已发生 build drift并触发停止。没有 `infrastructure_reason`，initial 已消费且不授权 retry；任何继续必须先取得新路线决策、冻结完整新 build、分配新 campaign/evidence ID，不得通过复用旧 ID 或改写路径判据绕过。
- **审查状态**：路线调整已由用户直接批准；campaign 证据已由 isolated `deepseek/deepseek-v4-pro` 审查为 0 blocker/0 major，`reviewed-pass/blocked`；E8 聚合审查未通过，E8 `CLOSED`。历史状态后续见 `ADJ-20260806-0003`。

### ADJ-20260806-0002：HarmonyOS 7 最小只读元组重绑定授权

- **提出与批准角色**：用户（直接人类决策者）；于 2026-08-06 直接批准。本记录按“人类直接决策者优先”规则替代这一次内部 T0 触发，不声称执行了 T0。
- **日期与时区**：`2026-08-06`，`Asia/Shanghai (+08:00)`；未虚构未提供的秒级批准时间。
- **触发证据**：`EV-E3-PHYS1API23-20260806-0001` 的 live build 脱敏投影 `PLA-AL10 <REDACTED_IPV4>(SP8C00E32R7P2)` 与冻结 `PLA-AL10 6.1.0.117(SP6C00E115R7P7)` 可见 suffix drift，导致 initial live `reviewed-pass/blocked`、`plan_status: blocked`；用户 Settings 页人工报告 `7.0.0.100 (SP8C00E32R7P2patch09)`。
- **调整原因**：旧冻结元组与 live/人工观测已不再可对齐；在未经新只读结果确认 API/arch/ABI 前，不得复用旧 campaign/evidence，也不得直接发起新 campaign 或重建/复用 HAP 的路线决定。
- **受影响阶段**：仅 `E3-PHYS-PREFLIGHT` 的设备元组输入与后续路线决策前置；不改变 E1、E4-E7、R0-R10 既有退出标准，不放宽任何 SLO 或补丁预算，不授权代码/SDK/Go/NetBird/WireGuard 补丁。
- **人工与旧 live 事实**：Settings 人工报告 `7.0.0.100 (SP8C00E32R7P2patch09)`；旧 live HDC 投影可见为 `PLA-AL10 <REDACTED_IPV4>(SP8C00E32R7P2)`。只据二者将 HDC binding 候选记为 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`；在新只读重绑定完成前，该候选**不得**作为已冻结结论，也不得回写旧 evidence。
- **授权范围（仅 discovery，非 campaign）**：分配 discovery evidence ID `EV-E3-PHYS1REBIND7-20260806-0001`；**不**分配新 campaign ID。用户明确且仅授权以下三条只读 `shell`（经仓外 `PHYS_1_TARGET`）：
  1. `param get const.ohos.apiversion`
  2. `uname -m`
  3. `param get const.product.cpu.abilist`
- **明确禁止**：不得重查 dist/model/build（已有本次 live 投影 + Settings 人工值）；不得读取序列号/UDID、应用清单；不得 install/start/stop、VPN、全量查询或任何 campaign 动作；不得复用 `E3-PHYS-PREFLIGHT-20260806-0001` / `EV-E3-PHYS1API23-20260806-0001`；不得以 build drift 主张 `infrastructure-blocked-retry-1`。
- **证据存储**：三条命令的原始输出仅仓外受控保存；仓内只可登记脱敏 projection 与 hash。本调整本身不创建 evidence 文件、不改 runner、不改旧 evidence 文档。
- **后续门控**：仅当上述三条结果齐备后，才可另行决定：既有 A/B HAP 是否可复用或须重建，以及是否发起新 campaign 的路线决定。在此之前当前 `plan_status` 仍为 `blocked`。
- **已评估替代方案**：直接以 Settings 人工值冻结新 build 会把 UI 文本升格为 HDC 绑定结论；重跑六条旧 discovery 或重查 dist/model/build 超出用户本次授权且重复既有事实；直接开新 campaign 或 infra retry 违反既有纪律。故仅授权三条缺口只读。
- **R0/SLO/补丁预算影响**：不改变首目标候选、必选功能、任何质量/安全/性能/隐私 SLO、错误预算或补丁上限；当前补丁计数不变。R0 checklist 中 2026-07-18 旧六条 discovery 的历史完成状态保留；HarmonyOS 7 tuple rebind 随后已完成（见 `EV-E3-PHYS1REBIND7-20260806-0001` 与 `ADJ-20260806-0003`）。
- **重跑范围**：本记录只授权一次只读 rebind discovery；不重跑旧 campaign，不重写旧判定，不自动开启 E3 或 E8。
- **生效条件**：用户直接批准后立即约束后续执行边界；设备端三条命令须在执行前再次确认仍仅限白名单，且不得扩展。
- **回退条件**：用户撤销授权、任一禁止项被执行、原始输出入库、把候选 build 当作已冻结、复用旧 campaign/evidence ID，或在三条结果未齐备时发起 HAP/campaign 决定时，立即停止并以新的动态调整记录重新界定。
- **审查状态**：路线授权由用户于 2026-08-06 直接批准；discovery 随后已执行并形成 `EV-E3-PHYS1REBIND7-20260806-0001`（`reviewed-pass/pass`，仅 rebind 范围；isolated `kimi-coding/k3`，0 blocker/0 major）；后续冻结与新 campaign 准备见 `ADJ-20260806-0003`。E3 未关闭，E8 保持 `CLOSED`。

### ADJ-20260806-0003：冻结 HarmonyOS 7 元组并准备新物理预检 campaign

- **提出与批准角色**：用户（直接人类决策者）；于 2026-08-06 直接批准。本记录按“人类直接决策者优先”规则替代这一次内部 T0 触发，不声称执行了 T0。
- **日期与时区**：`2026-08-06`，`Asia/Shanghai (+08:00)`；未虚构未提供的秒级批准时间。
- **触发证据**：`EV-E3-PHYS1REBIND7-20260806-0001` 完成 ADJ-0002 三条只读 rebind（`reviewed-pass/pass`，仅 rebind 范围）：API `26`、kernel `aarch64`、app ABI `arm64-v8a`；Settings 人工报告 `7.0.0.100 (SP8C00E32R7P2patch09)`；HDC build 候选仍据旧 live 投影 + 人工值记为 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`。
- **调整原因**：rebind 三条结果已齐备，可在不复用旧 campaign/evidence ID、不主张 infra retry 的前提下，冻结新目标元组并准备下一 campaign 的 host 输入；但设备执行仍须再次确认，且原 FINAL HAP 复用须 host reverify。
- **受影响阶段**：仅 `E3-PHYS-PREFLIGHT` 的目标元组冻结、HAP 输入准备与新 campaign 身份分配；不改变 E1、E4-E7、R0-R10 既有退出标准，不放宽任何 SLO 或补丁预算，不授权代码/SDK/Go/NetBird/WireGuard 补丁。
- **冻结元组**：`distribution=HarmonyOS`；`device_model=PLA-AL10`；HDC binding build=`PLA-AL10 7.0.0.100(SP8C00E32R7P2)`；Settings 人工观测=`7.0.0.100 (SP8C00E32R7P2patch09)`（`patch09` 不并入 HDC binding 字符串）；`api=26`；`kernel_architecture=aarch64`；`app_abi=arm64-v8a`；设备别名仍为 `PHYS-1`。
- **HAP 复用**：批准复用原 FINAL signed HAP hashes A=`3a98ad68bfc6253fe10b37a262e0051267b3b3083e6943f8207d68482d00c244`、B=`1adfa9664e59e7a9dc3da6650a59972517f5262a9b8160f4b3f6416770da3c26` 作为新 campaign 候选输入。兼容性仅作为后续实测对象，本批准**不**证明 API 26 安装/运行成功。host reverify 随后已 PASS（见下“当前状态”）。
- **新 campaign 身份（准备，非执行；历史原始分配）**：campaign ID `E3-PHYS-PREFLIGHT-20260806-0002`；evidence ID `EV-E3-PHYS1API26-20260806-0001`；target code 预期 `PHYS1API26`。旧 `E3-PHYS-PREFLIGHT-20260806-0001` / `EV-E3-PHYS1API23-20260806-0001` 历史判定不改写且不可复用。该组 20260806 准备 ID 从未 Live、未占用；跨日重新编号见 `ADJ-20260807-0001`（标 `superseded-unexecuted`；当时编号 `E3-PHYS-PREFLIGHT-20260807-0001` 后已 `consumed-blocked`）；`ADJ-20260807-0002` 的 `E3-PHYS-PREFLIGHT-20260807-0002` 亦已 Live 并 `consumed-blocked`。
- **明确禁止**：不得在未经再次设备执行确认时启动 Live campaign；不得复用旧 campaign/evidence ID；不得以 build drift 主张 `infrastructure-blocked-retry-1`；不得把 rebind `verdict: pass` 或 host reverify PASS 写成 E3/campaign pass；不得把 `plan_status: ready` 解释为设备执行已授权。
- **当前状态**：host reverify 已 PASS：public manifest SHA-256 `66a70a52c92b927d4b23e528ae6eaf1b52169e504291c6ff0e7efa4c7ffee010`；FINAL HAP / signature / profile / member-list hashes 与历史登记一致、未变；**不**主张设备安装兼容性。登记 HAP 仍 compile API `24`、target/compatible API `23`，设备冻结 API `26`。当前 runner SHA-256 `19fc1a76e49b9dca66a8a0352cc6bc8291f2888e66b3ad72cdc8a91ed97312e7`，selftest `f7156314031eaf3649dfc6e5e245b9fbba75944724ed03fa62b87a465188b39d`；selftest/dry-run/livesim HDC process count `0`。`plan_status: ready` **只表示**新 campaign host 输入 ready；仓外 commit-bound freeze 在提交后生成；设备执行仍须明确再确认。E3 未关闭，E8 保持 `CLOSED`。
- **已评估替代方案**：继续停在 rebind 后不冻结会让后续 host 准备无权威输入；强制重建 HAP 在兼容性未测前成本过高且非用户本次批准；在 host reverify 完成前直接 `ready`/Live 会跳过输入核验与设备再确认。故采用“冻结元组 + 批准复用候选 + 准备新 ID + host reverify 后 ready（仅 host）”。
- **R0/SLO/补丁预算影响**：不改变首目标候选、必选功能、任何质量/安全/性能/隐私 SLO、错误预算或补丁上限；当前补丁计数不变。R0 checklist 中 rebind 与 host 输入准备可记完成；“执行 campaign 并取得 `reviewed-pass/pass`”仍未完成。
- **重跑范围**：不重跑旧 campaign，不改写旧判定；新 campaign 若启动必须使用新 ID 与本记录冻结元组，并在 Live 前取得设备执行再确认与仓外 commit-bound freeze。
- **生效条件**：用户直接批准后立即约束文档与 host 准备边界；host reverify 完成后 `plan_status` 可为 `ready`（仅 host）；Live 仍须 clean worktree、完整仓外 freeze 与设备再确认。
- **回退条件**：用户撤销授权、host 输入再次漂移且无替代制品、设备再确认否定当前元组、禁止能力出现、复用旧 ID，或把 rebind/host reverify/本准备批准写成 E3 pass 时，立即停止并以新的动态调整记录重新界定。
- **审查状态**：路线授权由用户于 2026-08-06 直接批准；rebind 证据已由 isolated `kimi-coding/k3` 审查为 0 blocker/0 major；host reverify 已 PASS；新 campaign 尚未执行，E8 聚合审查未通过，E8 `CLOSED`。后续单条 build 实测确认见 `ADJ-20260806-0004`。后续未执行 campaign 跨日重新编号见 `ADJ-20260807-0001`。

### ADJ-20260806-0004：用户授权单条 software.version build 确认

- **提出与批准角色**：用户（直接人类决策者）；于 2026-08-06 按最终 freeze reviewer 建议直接批准。本记录按“人类直接决策者优先”规则替代这一次内部 T0 触发，不声称执行了 T0。
- **日期与时区**：`2026-08-06`，`Asia/Shanghai (+08:00)`；未虚构未提供的秒级批准时间。
- **触发证据**：`ADJ-20260806-0003` 冻结的 HDC binding build `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` 当时仍依据旧 live 脱敏投影可见 suffix + Settings 人工值合成，而非单条 `const.product.software.version` 实测；最终 freeze reviewer 建议在 Live 前以一次只读确认消除合成 build 风险。
- **调整原因**：在不新增 campaign、不改变既有 campaign/evidence ID、不重查 dist/model/API/arch/ABI 的前提下，用用户授权的单条只读 `param get const.product.software.version` 取得精确 HDC build 字符串，验证与冻结 binding 逐字匹配。
- **受影响阶段**：仅 `E3-PHYS-PREFLIGHT` 的 HDC build 输入确证强度；不改变 E1、E4-E7、R0-R10 既有退出标准，不放宽任何 SLO 或补丁预算，不授权代码/SDK/Go/NetBird/WireGuard 补丁。
- **授权范围（仅 build-confirm discovery，非 campaign；历史原始边界）**：分配 discovery evidence ID `EV-E3-PHYS1BUILD7-20260806-0001`；**不**分配新 campaign ID，**不**改变当时准备 ID `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001`。该组准备 ID 从未 Live、未占用；跨日重新编号见 `ADJ-20260807-0001`。用户明确且仅授权以下一条只读 `shell`（经仓外 `PHYS_1_TARGET`）：
  1. `param get const.product.software.version`
- **实测结果**：精确投影 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`，与 `ADJ-20260806-0003` 冻结 HDC binding **逐字匹配**；合成 build 候选风险消除。API `26` / `aarch64` / `arm64-v8a` 继续绑定 `EV-E3-PHYS1REBIND7-20260806-0001` 实测，**不得**从本 build 字符串推断。
- **明确禁止**：不得重查 dist/model/API/arch/ABI；不得读取序列号/UDID、应用清单；不得 install/start/stop、VPN、全量查询或任何 campaign 动作；不得复用旧 campaign/evidence ID；不得以本确认写成 E3/campaign pass；不得扩展为第二次 build 查询或其他 HDC 操作。
- **证据存储**：单条命令的原始输出仅仓外受控保存；仓内只可登记脱敏 projection 与 hash。登记见 [`EV-E3-PHYS1BUILD7-20260806-0001`](evidence/e3-physical-build7-confirm-2026-08-06.md)（`reviewed-pass/pass`，严格只表示 build-confirm）。
- **当前状态**：`plan_status: ready` **仍只表示**新 campaign host 输入 ready；最终 commit-bound freeze 须基于包含本登记的提交重生；设备执行仍须明确再确认。E3 未关闭，E8 保持 `CLOSED`。不新增 campaign，不改变既有 IDs。
- **已评估替代方案**：继续以合成候选作为 Live 输入会保留 reviewer 指出的残余风险；重跑六条旧 discovery 或重查 API/arch/ABI 超出本次授权且重复既有实测；直接开新 campaign 或改 ID 违反“不新增 campaign/不改变 IDs”。故仅授权一条 software.version 确认。
- **R0/SLO/补丁预算影响**：不改变首目标候选、必选功能、任何质量/安全/性能/隐私 SLO、错误预算或补丁上限；当前补丁计数不变。R0 checklist 中 host 输入准备与元组冻结状态保留；“执行 campaign 并取得 `reviewed-pass/pass`”仍未完成。
- **重跑范围**：本记录只授权一次只读 build 确认；不重跑旧 campaign，不改写旧判定，不自动开启 E3 或 E8。
- **生效条件**：用户直接批准后立即约束后续文档与 Live 输入解释；设备端该命令须在执行前再次确认仍仅限白名单单条，且不得扩展。
- **回退条件**：用户撤销授权、任一禁止项被执行、原始输出入库、把本确认写成 E3/campaign pass、复用旧 campaign/evidence ID，或在未重生 commit-bound freeze 前把旧 freeze 当作最终 Live 输入时，立即停止并以新的动态调整记录重新界定。
- **审查状态**：路线授权由用户于 2026-08-06 直接批准；build 确认证据已由 isolated `kimi-coding/k3` 审查为 0 blocker/0 major，`reviewed_at: 2026-08-06T20:46:00+08:00`；`reviewed-pass/pass` 仅 build-confirm 范围。新 campaign 尚未执行，E8 聚合审查未通过，E8 `CLOSED`。后续未执行 campaign 跨日重新编号见 `ADJ-20260807-0001`。

### ADJ-20260807-0001：未执行 API26 campaign 跨日重新编号

- **提出与批准角色**：用户（直接人类决策者）；于 2026-08-07 直接批准。本记录按“人类直接决策者优先”规则替代这一次内部 T0 触发，不声称执行了 T0。
- **日期与时区**：`2026-08-07`，`Asia/Shanghai (+08:00)`；未虚构未提供的秒级批准时间。
- **触发证据**：`ADJ-20260806-0003` 分配的准备身份 `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001` 跨日仍未 Live、未占用；文档 commit 与仓外 commit-bound freeze 将随本登记重生，不宜继续以 20260806 准备日期作为当前可执行身份。
- **调整原因**：在不改冻结元组、不改 HAP/runner/rules、不主张任何设备执行、不运行 HDC 的前提下，将从未执行的 API26 campaign/evidence 准备身份跨日重新编号为当日当前 ID，使日期语义与未占用状态一致，并明确旧准备 ID 为 `superseded-unexecuted`。
- **受影响阶段**：仅 `E3-PHYS-PREFLIGHT` 的当前准备 campaign/evidence 身份编号与文档同步；不改变 E1、E4-E7、R0-R10 既有退出标准，不放宽任何 SLO 或补丁预算，不授权代码/SDK/Go/NetBird/WireGuard 补丁，不授权任何 HDC/设备命令。
- **历史原始 ID（保留，不改写 ADJ-0003/0004 当时分配事实）**：`E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001`。状态：`superseded-unexecuted`（从未 Live、未占用；不可复用为当前执行身份）。
- **当时准备身份（历史；后已 Live 并 consumed-blocked）**：campaign ID `E3-PHYS-PREFLIGHT-20260807-0001`；evidence ID `EV-E3-PHYS1API26-20260807-0001`；target code 仍为 `PHYS1API26`。该组 ID 随后 Live 并因 operator-aborted 记为 `consumed-blocked`；见 `ADJ-20260807-0002` 与 [`EV-E3-PHYS1API26-20260807-0001`](evidence/e3-physical-preflight-api26-2026-08-07.md)。
- **不变项**：`ADJ-20260806-0003` 冻结元组、原 FINAL HAP hashes 复用批准与 host reverify 结果、Settings re-allow 预测路径、rebind/build-confirm 历史 measured 记录均不变；历史 evidence measured YAML 不改写。
- **明确禁止**：不得执行 HDC 或任何设备端命令；不得在未经再次设备执行确认时启动 Live campaign；不得复用已消费的 API23 ID 或本记录标为 `superseded-unexecuted` 的 20260806 准备 ID；不得以本重新编号写成 E3/campaign pass；不得把 `plan_status: ready` 解释为设备执行已授权。
- **历史状态（本记录生效时）**：`plan_status: ready` **只表示**新编号 campaign 的 host 输入 ready；最终 commit-bound freeze 须基于含本登记的提交重生；因文档 commit/freeze 重生，设备执行仍须明确再确认。E3 未关闭，E8 保持 `CLOSED`。tuple/HAP/runner/rules 当时 unchanged。
- **已评估替代方案**：继续沿用 20260806 未执行准备 ID 会使日期与当前文档/freeze 重生脱节；改写 ADJ-0003/0004 历史分配或 measured evidence 会破坏可追溯性；借重新编号顺带改 tuple/HAP/runner 或执行 HDC 会越权。故仅做未执行身份的跨日重新编号与文档同步。
- **R0/SLO/补丁预算影响**：不改变首目标候选、必选功能、任何质量/安全/性能/隐私 SLO、错误预算或补丁上限；当前补丁计数不变。R0 checklist 中“执行 campaign 并取得 `reviewed-pass/pass`”仍未完成。
- **重跑范围**：不重跑任何 discovery 或 campaign；不改写历史判定与 measured YAML；仅更新当时准备 ID 与相关文档指针。
- **生效条件**：用户直接批准后立即约束文档与 host 准备身份；Live 仍须 clean worktree、完整仓外 freeze 与设备再确认。
- **回退条件**：用户撤销授权、把 `superseded-unexecuted` ID 当可执行身份、改写历史 measured、执行未授权 HDC、或把本重新编号写成 E3 pass 时，立即停止并以新的动态调整记录重新界定。
- **审查状态**：路线授权由用户于 2026-08-07 直接批准；本记录仅为身份重新编号与文档同步。后续 live 与 operator-aborted seal 见 `ADJ-20260807-0002`。E8 聚合审查未通过，E8 `CLOSED`。

### ADJ-20260807-0002：中文完整重跑（protocol usability correction）与 prior blocked 绑定

- **提出与批准角色**：用户（直接人类决策者）；于 2026-08-07 直接批准中文完整重跑。本记录按“人类直接决策者优先”规则替代这一次内部 T0 触发，不声称执行了 T0。
- **日期与时区**：`2026-08-07`，`Asia/Shanghai (+08:00)`；未虚构未提供的秒级批准时间。
- **触发证据**：`E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 已 Live 并 operator-aborted sealed 为 `consumed-blocked`（见 [`EV-E3-PHYS1API26-20260807-0001`](evidence/e3-physical-preflight-api26-2026-08-07.md)）。operator 将场景 5 `SETTINGS-REVOKE-CAPTURED` 误确认为真，且未操作普通系统设置即直接关窗；recovery cleanup `verified_absent=true`；独立 operator-aborted seal 审查（isolated `anthropic/claude-sonnet-5`）0 blocker / 0 major。公开 hash：scenario-results `b29e97cd0643eb59447120d727dd4706a919d9cf9f71a5dad3e99c3d18b4b537`；hash-manifest `1326e60b7ea14d3ee40e6580686af4a2c611b328448ccad28305c0e03d17d541`；campaign-seal `720a119d39d2028baef956b9cccceefea6bd68414144257a7574d3a6992d98ad`；operator-correction `2d0c465fe08581233235eb9763ea80c8817a4ceb21abdf3a9ec3ae9d600d9029`；transcript chain_head `7cf8d72be6ea1231854e906f1459d1f0ceed22050f477881654433693ce04795`；cleanup verified true。
- **调整原因**：**protocol usability correction**（runner 中文人工提示，降低 operator 误确认风险），**不是**设备行为 retry，也不是 `infrastructure-blocked-retry-1`。prior blocked campaign 明确绑定为前置事实；完整场景 1–7 必须用新 ID 重跑。
- **受影响阶段**：仅 `E3-PHYS-PREFLIGHT` 的当前准备 campaign/evidence 身份、runner 人工提示可用性与 host 准备状态；不改变 E1、E4-E7、R0-R10 既有退出标准，不放宽任何 SLO 或补丁预算，不授权代码/SDK/Go/NetBird/WireGuard 补丁；**本登记期间禁止任何 HDC/设备命令**。
- **历史 consumed-blocked ID（保留，不改写 measured）**：`E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001`。状态：`consumed-blocked`；禁止复用；**禁止局部 scenario5 重放**。
- **当时准备身份（历史；后已 Live 并 consumed-blocked）**：campaign ID `E3-PHYS-PREFLIGHT-20260807-0002`；evidence ID `EV-E3-PHYS1API26-20260807-0002`；target code 仍为 `PHYS1API26`。完整场景 1–7；prior blocked 显式绑定 `EV-E3-PHYS1API26-20260807-0001`。
- **不变项**：`ADJ-20260806-0003` 冻结元组、原 FINAL HAP hashes 复用批准与 host reverify 结果、Settings re-allow 预测路径、rebind/build-confirm 历史 measured 记录均不变；历史 evidence measured YAML 不改写。同 tuple/HAP。
- **变更项**：runner 人工提示中文化（protocol usability）；已 commit 为 `e8eb1b67a48603c55d3f55d2be686bae0dbd15e1` 并完成仓外 freeze / dry-run / Live。
- **明确禁止**：不得执行 HDC 或任何设备端命令（本登记与当前治理均禁 HDC）；不得从场景 5 局部重放 prior blocked campaign；不得复用任一 `consumed-blocked` ID 或 `superseded-unexecuted` 的 20260806 准备 ID；不得以 prior blocked 或 0002 blocked 主张 `infrastructure-blocked-retry-1`；不得 auto retry / 自动分配新 ID；不得把本授权或 0002 `reviewed-pass` 写成 E3/campaign pass 或设备行为 retry。
- **当前状态（登记时）**：`plan_status: blocked-awaiting-device-authorization`。0002 已 Live 并登记为 [`EV-E3-PHYS1API26-20260807-0002`](evidence/e3-physical-preflight-api26-0002-2026-08-07.md)（`reviewed-pass/blocked`，`consumed-blocked`；S1/S4 pass，S2/S3/S5/S6/S7 blocked；cleanup `verified-clean`；双审查 0 B/5 M；opus timeout attempt-not-counted）。`ADJ-20260807-0003` 已批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径（runner 变更完成，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)）；E3 未关闭，E8 保持 `CLOSED`。用户显式设备授权 + fresh device confirmation 完成前无 auto retry/新 ID/设备命令授权。（本行为该 ADJ 登记时点（2026-08-07 后）的状态快照；后续治理状态见下方 ADJ-20260718-0001 当前状态。）
- **已评估替代方案**：局部 scenario5 重放会破坏 campaign 完整性且被 seal 明确禁止；把 prior blocked 当 infra retry 会错误放宽纪律；改写 prior measured 会破坏可追溯性；在 runner 中文修正未 commit/freeze 前直接 Live 会跳过输入门。故当时采用“prior blocked 公开登记 + 新 ID 完整 1–7 + 中文 usability 修正”。0002 现已消费为 blocked，继续须新裁决。
- **R0/SLO/补丁预算影响**：不改变首目标候选、必选功能、任何质量/安全/性能/隐私 SLO、错误预算或补丁上限；当前补丁计数不变。R0 checklist 中“执行 campaign 并取得 `reviewed-pass/pass`”仍未完成。
- **重跑范围**：当时授权完整场景 1–7 的新 campaign（新 ID）；不改写 prior blocked measured；不重跑 rebind/build-confirm discovery。0002 已执行完毕，**不再**自动授权另一次重跑。
- **生效条件**：用户直接批准后立即约束文档与 host 准备身份；Live 曾须 clean worktree、含中文 runner 的 commit、完整仓外 freeze、dry-run 与设备再确认（均已履行后执行 0002）。
- **回退条件**：用户撤销授权、复用 consumed-blocked ID、局部 scenario5 重放、未授权 HDC、改写 prior measured、或把本记录/0002 写成 E3 pass / 设备行为 retry 时，立即停止并以新的动态调整记录重新界定。
- **审查状态**：路线授权由用户于 2026-08-07 直接批准；prior blocked seal 已由 isolated `anthropic/claude-sonnet-5` 审查为 0 blocker/0 major；0002 双审查 isolated `kimi-coding/k3` + `anthropic/claude-sonnet-5` 均为 0 blocker / 5 major；E8 聚合审查未通过，E8 `CLOSED`。

### ADJ-20260807-0003：host process terminal probe 与 Settings 应用信息强制停止撤销路径（用户直接裁决）

- **提出与批准角色**：用户（直接人类决策者）；于 2026-08-07 直接批准。本记录按“人类直接决策者优先”规则替代这一次内部 T0 触发，不声称执行了 T0。
- **日期与时区**：`2026-08-07`，`Asia/Shanghai (+08:00)`；未虚构未提供的秒级批准时间。
- **触发证据**：`EV-E3-PHYS1API26-20260807-0002`（`reviewed-pass/blocked`，`consumed-blocked`）中 S3/S7 均因缺 destroy terminal / post-destroy fd snapshot 判 `blocked`，但事件里存在 Stop / `VPN_ONDESTROY` / pre-destroy 证据（M4：S7 runner reason `requestId-missing` 与事件事实不符）；S5 的系统 VPN 页为空（“VPN / 没有VPN / 添加VPN网络”）、无普通 VPN 管理入口可撤销，`settings_revoke_captured=false`。设备正面上 S2/S5 的 `VpnConnection.create` 均返回有效 fd（fd=32/33 open accepted），`onDestroy` 与 pre-destroy 事件存在，说明回调 terminal 缺失很可能是该 HarmonyOS 7 / API 26 元组上的 marker 时序/进程冷启动问题，而非 VPN 生命周期本身失败。官方 OpenHarmony VPN 公开实现（`communication_netmanager_ext` 仓，`OpenHarmony-6.0-Release` 分支；访问日 2026-08-07；OpenHarmony 公开仓库无 6.1 标签，最新 6.x 分支为 6.0-Release；HarmonyOS 7 私有实现不可引，仅引官方公开实现作背景）：
  - <https://gitee.com/openharmony/communication_netmanager_ext/blob/OpenHarmony-6.0-Release/frameworks/js/napi/vpn/src/vpn_connection.cpp>（`VpnConnection` NAPI `Destroy`）
  - <https://gitee.com/openharmony/communication_netmanager_ext/blob/OpenHarmony-6.0-Release/frameworks/js/napi/vpnext/src/vpn_connection_ext.cpp>
  - <https://gitee.com/openharmony/communication_netmanager_ext/blob/OpenHarmony-6.0-Release/frameworks/js/napi/extensionability/vpn_extension_ability.js>
  - <https://gitee.com/openharmony/communication_netmanager_ext/blob/OpenHarmony-6.0-Release/services/vpnmanager/src/net_vpn_impl.cpp>
  - <https://gitee.com/openharmony/communication_netmanager_ext/blob/OpenHarmony-6.0-Release/services/vpnmanager/src/multi_vpn_helper.cpp>
- **调整原因**：回调 terminal + post-destroy fd snapshot 仍是优先证据，但该元组上 marker 时序不可靠时，需要不依赖该 marker 的 host 侧进程终态证据（白名单 `PidOf`/`BundleDump` 观察）作为严格 fallback；S5 需要可操作的普通撤销机制（Settings 应用信息强制停止）替代该元组上不存在的 Settings VPN 管理入口。
- **受影响阶段**：仅 `E3-PHYS-PREFLIGHT` 的 S3/S5/S7 终态判定、S5 撤销机制与 runner/冻结输入；S2/S4/S6 的 capture/window degradation 判定与既有 `fail` > `blocked` 规则**不在本裁决范围内、保持不变**（显式功能 fail 优先于退化、绝不降级为 blocked；证据缺失保持 blocked、绝不升格为 fail）。不改变 E1、E4-E7、R0-R10 既有退出标准，不放宽任何 SLO 或补丁预算，不授权代码/SDK/Go/NetBird/WireGuard 补丁；本登记期间禁止任何 HDC/设备命令（host 重建完成前）。
- **调整内容**（runner 决策字段与判定规则）：
  - 本裁决**不新增任何退出标准**，只修正 S3/S5/S7 的终态优先级（callback terminal + post-destroy fd snapshot 优先，`FD_STILL_OPEN` 硬 fail 不可 fallback，否则严格 process-boundary fallback）与 S5 撤销机制；S2/S4/S6 判定不变。替代范围仅限上述终态判据与 S5 撤销机制，不放宽 S1/S2/S4/S6 的任何功能判据。
  - 新增 host process terminal probe：复用白名单 `PidOf` + `BundleDump`，仅观察；至少连续 2 次 absent 且相邻间隔 ≥3s 才构成终态；任何 present/unknown/error 重置连续计数，unknown/error 使该序列整体 blocked；所有 probe 进入场景 record 与 transcript；finally/uninstall 后的 absent 不得回填。
  - `Get-VpnFinalState`：优先现有 callback terminal + post-destroy fd snapshot；显式 `FD_STILL_OPEN` fail 不可 fallback；否则 S3/S7 严格 fallback 需当前窗口唯一合法 stop（同 bundle/request）、`UI_STOP` 或 `STOP_PROMISE_RESOLVED`、`VPN_ONDESTROY`、`VPN_DESTROY_BEGIN` 或 pre-destroy snapshot、连续 absent probes；S3 额外要求 bundle present；`VPN_DESTROY_ISSUED` 绝不算。
  - S3 fallback pass 的 aggregation 额外依赖：S5 同 bundle 后续 fresh request `CREATE_ACCEPTED` + post-create open（`clean_reactivation_proof`）；缺失则 overall blocked。
  - S5 新路径：先 fresh A Start/create accepted/open；Settings>VPN 页面仅 observation 截图/字段（`settings_vpn_page_observation_only=true`，不影响结果）；随后人工 Settings>应用信息>A>强制停止，单独截图 + `ConfirmVisibleFact SETTINGS-APP-INFO-FORCE-STOP-CAPTURED`；HDC 只 `PidOf`/`BundleDump` 观察，绝不 ForceStop（**HDC force-stop 明确 cleanup-only**：仅 finally 残留清理，Reason 限 `exception-cleanup`/`final-cleanup`，永不用于撤销）；pass 需人工确认、fresh create/open、bundle still present、主/vpn bundle process 连续 absent；无 `UI_STOP` 正常；路径观察保留但不阻塞。
  - S7：pre-uninstall 连续 probe；场景 terminal 评估完成后才允许现有 uninstall cleanup；finally absent 不得回填。
  - 字段：`terminal_mode` = `callback-post-fd|strict-process-boundary|settings-app-info-force-stop`；`process_final_state_probes`（time、status、consecutive count）；`bundle_present_during_probe`；`settings_vpn_page_observation_only=true`；`app_info_force_stop_capture`；`assertions.force_stop`；`process_absent_evidence`（met/reason/required/measured spacing）；`clean_reactivation_proof`。错误/unknown 一律 blocked；所有 HDC 输出仍走脱敏/raw governance。
  - 冻结决策字段（freeze manifest 必填、`Assert-FreezeManifest` 严格校验）：`settings_revoke_mechanism=settings-app-info-force-stop`、`settings_vpn_page_policy=observation-only`、`destroy_terminal_policy=callback-or-strict-process-boundary`、`process_absent_required_count=2`、`process_absent_probe_spacing_seconds=3`（旧 `spacing` 字段作为未知/缺新字段被拒绝，不兼容复用）。旧 freeze 仅历史、不可用于新 runner live；DryRun 明确 blocked。
- **已评估替代方案**：把 0002 的 blocked 直接重判为设备行为 retry 违反纪律且无新证据；坚持仅 callback terminal 会让该元组在 marker 时序问题上永久 blocked；用 HDC `aa force-stop` 执行 S5 撤销会把它变成 runner 驱动的设备命令而非普通用户可见路径；用 uninstall 冒充撤销破坏清理语义。故采用“callback 优先 + host 进程 probe 严格 fallback + Settings 应用信息强制停止（人工、可见、截图确认）”。本裁决只替代 S3/S5/S7 终态判据与 S5 撤销机制，不替代 S2/S4/S6 的 capture/window degradation 判定，也不新增任何退出标准。
- **R0/SLO/补丁预算影响**：不改变首目标候选、必选功能、任何质量/安全/性能/隐私 SLO、错误预算或补丁上限；当前补丁计数不变。
- **重跑范围**：runner/freeze example/selftest 与相关计划文档按本裁决更新；**不**改写 0001/0002 历史 measured 判定；新 Live 仍须新 campaign/evidence ID、host 重建后完整仓外 freeze、dry-run 与设备再确认。
- **生效条件**：用户直接批准后立即约束 runner 与文档；Live 前须 host rebuild 完成（Windows 签名/构建主机侧的 runner、freeze 与 selftest 快照随本提交变更后重生）、clean worktree、含本登记的 commit、完整仓外 freeze 与设备再确认。
- **回退条件**：用户撤销授权、回调 terminal 在新运行中可用且判定与 probe fallback 冲突、probe 观察到与回调矛盾的结果、禁止能力出现（如 HDC force-stop 被用于 S5 撤销而非 cleanup-only）、旧 freeze 被用于新 Live、改写历史 measured，或把本裁决写成 E3 pass / 设备行为 retry 时，立即停止并以新的动态调整记录重新界定。
- **审查状态**：路线授权由用户于 2026-08-07 直接批准；runner 变更后 host-only selftest（`-SelfTest` + 完整 selftest，`HDC_PROCESSES=0`）、bash -n/PS parser、JSON 与 diffcheck 均通过；本提交追加 S7 cleanup readiness 单一 `Get-VpnFinalState` evaluator 重构与冻结字段 `process_absent_probe_spacing_seconds` 重命名（旧 `spacing` 拒绝）后复跑同样全部通过；host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)（host selftest `HDC_PROCESSES=0`、独立审查 0 B/0 M；candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`；DryRun `is_evidence: false`/HDC0/integrity empty；旧 20260807 candidate `INVALID-TIMELINE` 不可用）；登记时 `plan_status: blocked-awaiting-device-authorization`（禁 HDC/设备命令；用户显式设备授权 + fresh device confirmation 完成前无 auto retry/新 ID/设备命令授权），E3 未关闭，E8 保持 `CLOSED`。

### ADJ-20260809-0001：正式采用 NetBird v0.76.3

- **提出与批准角色**：用户（直接人类决策者）；于 2026-08-09 直接批准。本记录按“人类直接决策者优先”规则替代这一次内部 T0 触发，不声称执行了 T0。
- **日期与时区**：`2026-08-09`，`Asia/Shanghai (+08:00)`；未虚构未提供的秒级批准时间。
- **触发证据**：NetBird 正式非 prerelease tag `v0.76.3`（published `2026-08-08T12:11:41Z`）、固定 tag commit `f65f7b347ee4e7de6d98c488d3d894cd018b02b6`、官方 Release run [`31256677326`](https://github.com/netbirdio/netbird/actions/runs/31256677326) 成功（固定 URL 访问日期 2026-08-09），以及安全公告 GHSA-qcpp-8vwj-hhwr（high；影响 `>=0.5.0,<0.76.0`，`0.76.0` patched）。
- **调整原因**：R0 规则要求后续门跟随并显式固定最新正式 NetBird release；v0.76.3 已发布且版本输入可复核；同时 GHSA-qcpp-8vwj-hhwr（high）影响 `>=0.5.0,<0.76.0`，v0.74.7 在受影响范围内，升级到 v0.76.3 可消除该公告影响。继续仅登记而不作采用决定会使后续门的正式基线不明确且保留已知 high 漏洞输入。
- **调整内容**：自本记录生效起，当前R0正式基线采用 NetBird `v0.76.3`。后续新运行使用该版本；所有实际使用 v0.74.6/v0.74.7 的历史 evidence 继续绑定原 tag、commit、制品 hash 和判定，不改写、不重判，也不因新基线自动失效或通过。
- **受影响阶段**：R0 版本锁定、E1 官方 Go loader/runtime、E8 聚合输入，以及后续实际消费 NetBird 客户端或服务端基线的 R2-R8 与 R10 依赖维护；不改变任何阶段当前退出状态，不改变 E3 的 HDC/设备授权状态（`plan_status: blocked-awaiting-device-authorization` 保持，禁 HDC）。
- **版本与发布核对**：tag `v0.76.3`；commit `f65f7b347ee4e7de6d98c488d3d894cd018b02b6`；published `2026-08-08T12:11:41Z`；`go 1.25.5`；`toolchain go1.25.12`；官方 Release run [`31256677326`](https://github.com/netbirdio/netbird/actions/runs/31256677326) 成功；固定 run URL 访问日期 `2026-08-09`；相对 v0.74.7 前进 140 个 commit。
- **wireguard-go 差异**：NetBird `go.mod` 的 `netbirdio/wireguard-go` replace 从 v0.74.7 的 `8ec1ad32882fab0432317d027b0189371782ad01` 回到 `2834bebf6c1aea76bd217f31ea91c99f75e4a20a`（与 v0.74.6 相同）；该变化须进入新基线依赖、SBOM、许可证和功能回归输入，不能复用 v0.74.7 的完成结论。
- **安全动因**：GHSA-qcpp-8vwj-hhwr 严重级别 high，影响 `>=0.5.0,<0.76.0`，`0.76.0` patched；v0.74.7 在受影响范围内，v0.76.3 已包含修复。本记录只登记影响范围与升级动因，不展开漏洞细节。
- **已评估替代方案**：继续以 v0.74.7 作为新运行基线会偏离“最新正式 release”规则且保留已知 high 漏洞输入；只登记 v0.76.3 而延后采用会让 E1 和后续门输入悬而未决；静默替换历史 evidence 会破坏可追溯性。故采用 v0.76.3 作为后续基线，同时完整保留 v0.74.6/v0.74.7 历史绑定。
- **R0/SLO/补丁预算影响**：更新 R0 的 NetBird client/server 版本输入，不改变首目标候选、功能范围、任何质量、安全、性能、隐私 SLO、错误预算阈值或分阶段补丁预算；不授权任何 Go、NetBird、wireguard-go 或 SDK 补丁，当前补丁计数仍为 0。
- **重跑范围**：首先以 v0.76.3 重跑 E1 官方 Go loader/runtime；在 E8 聚合前重核相关上游 SHA 和全部输入 hash。进入相应阶段前，重新生成或复核 v0.76.3 客户端/服务端及 wireguard-go 变化涉及的依赖清单、SBOM、许可证与 AGPL 分析，并对 R2-R8/R10 中实际消费该基线的门执行风险对应回归。v0.76.3 的 E1 当前尚未重跑，未达到 `reviewed-pass/pass`；SBOM/许可证重核亦未完成。
- **T0 判定与决策依据**：版本采用属于重大技术输入调整，本记录按本节“人类直接决策者优先”规则，以用户明确决定替代这一次内部 T0 触发；未执行或虚构 T0。若用户明确要求 T0，仍按 T0 协议执行。
- **生效条件**：上述 tag、commit、published 时间、Go/toolchain、成功 Release run 和固定访问日均可复核，且文档同步保留 v0.74.6/v0.74.7 历史边界后立即生效；生效只改变后续正式输入，不赋予任何 E/R 门通过状态。
- **回退条件**：tag/commit 或 Release run 核对被推翻、v0.76.3 被上游撤回、出现阻断级安全/许可证问题，或重跑证明其无法在既有 R0 预算内继续时，停止新基线运行并由新的动态调整记录决定回退或前进版本；不得通过改写 v0.74.6/v0.74.7 历史 evidence 实施回退。
- **审查状态**：用户于 2026-08-09 明确批准正式采用；文档层调整已进入当前未提交 diff。E1、SBOM、依赖、许可证和 AGPL 的新基线证据审查尚未完成，本记录不把这些事项写成通过。E3 未关闭，E8 保持 `CLOSED`。
- **后续结果（2026-08-09）**：按本记录重跑范围，E1 已执行 `EV-E1-EMU24-20260809-0003`（[measured blocked 证据](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md)）：完整模式在 API 24 x86_64 phone Emulator 上测得 stock Go 1.25.12 loader 精确拒绝（`initial-exec TLS resolves to dynamic definition`），`record_status: reviewed-pass`、`verdict: blocked`（终审 `REV-E1-EMU24-20260809-0003`，0 blocker/0 major；formal raw 内 `RECORD_STATUS=collected` 为 runner sealed 的 pre-review 不可变事实）；仍无 `reviewed-pass/pass`，E1 overall Go 未关闭，E8 保持 `CLOSED`。SBOM/许可证重核仍未完成。

### ADJ-20260808-0003：强可靠执行细化（真实 layout 校准 / prompt-time 事件窗口 / 空档 UI action guard / infra capture 分类 / S5 去除非决定性 VPN 页步骤 / process-target verify / C6 真实 shape 与连续 capture infra 传播）

- **提出角色**：主会话编排 + 执行子代理（`ADJ-20260808-0002` 强可靠模式的执行细化；runner/selftest 已落地）。完整登记见 [`e3-physical-preflight-operator-trust-2026-08-08`](evidence/e3-physical-preflight-operator-trust-2026-08-08.md)。
- **触发证据**：`ADJ-20260808-0002` 强可靠模式落地后，机器 layout 档案 / 事件窗口 / 额外 UI action / 采集失败分类 / S5 VPN 页步骤 / process-target 校验仍需精确化；Settings app-info 页真实结构尚未采样（这是当时历史事实；现行 matcher 已由 [`2026-08-15 production layout`](evidence/e3-physical-preflight-production-layout-2026-08-15.md) 取代）。
- **调整原因**：强可靠模式要求机器判定 request/事件/layout/进程；需要真实 layout 校准、prompt-time 事件窗口、空档 UI action guard、infra capture 分类，去除非决定性 Settings>VPN 页步骤，并校验 `<bundle>:vpn` 命名元组。
- **调整内容**：1) 机器 layout 档案按 sealed raw layout facts **真实校准（C6 修订：真实 attributes/children shape）**——API26 sealed raw 与仓库 EMU 样本均为顶层数组、每节点 `{attributes:{bundleName,type,id,key,text,...},children:[...]}`，`Test-CapturedLayoutProfile` 用通用精确 fact 正则匹配任意深度 `attributes.<field>`（不依赖自造 `window`/`resourceId`）；`authorization`（任一 `attributes.bundleName=vpndialog` + 任一 `attributes.type=Dialog` + 标题/问题含 `允许`+`VPN` + Allow/允许 + Cancel/Deny/取消/拒绝）与 `settings-vpn`（任一 `attributes.bundleName=settings` + `attributes.id/key=vpn_settings`；页已非决定性；禁止仅凭文本 `VPN` 匹配）；`entry` 仍要求 ExpectedBundle + 按钮 `id/key` `start-vpn`/`stop-vpn`；settings-app-info 不要求 ExpectedBundle（A correctness 由 A process effect gate 证明）；request ownership 由 UI_START/事件门证明，绝不要求授权页含 bundle。2) **prompt-time 事件窗口**：`relative_to_prompt` 事件以 prompt 时间为下界打戳，慢操作员不把设备事件推到回车之后。3) **空档 UI action guard**：不属于当前机械步骤的 UI_START/STOP/STOP_SKIPPED → 立即 scenario invalid。4) **infra capture 分类（C6 修订：连续 capture infra 传播）**：ScreenCap/DumpLayout/Receive 124/125/超时/HDC transport → infrastructure blocked + retry 授权，绝不 scenario invalid；`Invoke-Capture` 遇 `CampaignCapture.Degraded` 按 raw-hilog 条目 `category`/`infrastructure_reason` 分类传播，`Wait-MachineCondition`/`Invoke-MechanicalStep`/直接 capture 判断均区分 `LastCaptureInfrastructure`。5) **S5 去除非决定性 VPN 页步骤**（`settings_vpn_page_capture=not-required`），改为 fresh A → 机器校验 Settings A app-info → 强制停止 → `:vpn` probes。6) **process-target verify**：CREATE_ACCEPTED 后精确 `pidof <bundle>:vpn` present 校验，S3/S5/S7 只消费已校验 checkpoint。7) **step 超时分类**：`Wait-MachineCondition` pending 超时时，期望 UI action 缺失 → invalid `mechanical-action-missing`；正确 action 后平台 marker 缺失 → blocked `platform-marker-missing`（绝不 invalid）。8) **S6 A 可选 reauthorization**（同 S2/S5）：A Start 后立即 capture/layout 判断 entry/authorization；若 authorization → 机械单步点 Allow（after capture、无额外 UI guard）再等待 A create terminal；纯 `START_PROMISE_REJECTED`/无 `VPN_ONCREATE` 的授权层结果 → blocked `authorization-outcome-unclassified`、S7 `not-run-after-platform-blocked`（绝不 fail/invalid）；仅 `VPN_ONCREATE` 后 `VPN_CREATE_REJECTED`/`INVALID_FD` → 功能 fail。9) **S6 非冻结 B 拒绝码**：完成 observation、记录 blocked `B-conflict-code-not-frozen:<code>`、S7 `not-run-after-platform-blocked`、绝不 invalid。App Info 真实结构尚未采样及其保守 `settings-app-info` profile（无 ExpectedBundle、无 `app-info-structure` id/key）是 ADJ 当时的历史事实；已由 [`2026-08-15 production layout`](evidence/e3-physical-preflight-production-layout-2026-08-15.md) 取代。当前规则要求 Settings owner 下唯一可见 `Setting.AppDetail`，并在同一子树匹配 expected A/B distinct label 与 force-stop control。
- **生效条件**：只适用下一新 campaign/evidence；`EV-E3-PHYS1API26-20260808-0001` 与全部历史 sealed evidence **不改字节、不重判**，不回溯 0001；保留 `ADJ-20260808-0001`/`0002` 底层修复。
- **审查状态**：host selftest `HDC_PROCESSES=0`、runner `-SelfTest`、parser 通过；登记时 `plan_status: blocked-awaiting-device-authorization`（禁 HDC），E3 未关闭，E8 `CLOSED`。

### ADJ-20260808-0002：强可靠操作员信任模型（mechanical-action-only-machine-verified-v1 / scenario invalid 停止 / S6 机器冲突 / 无人工三态）

- **提出角色**：用户明确选择强可靠模式；主会话编排 + 执行子代理落地。完整登记见 [`e3-physical-preflight-operator-trust-2026-08-08`](evidence/e3-physical-preflight-operator-trust-2026-08-08.md)。
- **触发证据**：旧 READY/ACK nonce、operator 三态与 `Confirm-VisibleFact` 把 S4/S5/S6/S7 语义交给人工，无法强复核；`ADJ-20260808-0001` 已修 process-target 等底层但未独立登记 trust model。
- **调整原因**：需要单步机械操作 + 机器判定 request/事件/layout/进程；首个 scenario invalid 停止后续；overall 优先级 integrity invalid > scenario invalid > fail > blocked > pass；finally cleanup+seal 不改变 overall。
- **调整内容**：1) freeze 必填 `operator_trust_model=mechanical-action-only-machine-verified-v1`、`scenario_invalid_policy=stop-and-finally-cleanup-seal`、`layout_verification_profile=deterministic-layout-v1`、`vpn_conflict_rejection_codes=[2203002]`。2) Live 主路径仅 `Invoke-StrongLiveCampaign`；操作员只见“现在只做 X，完成后按回车”；删除 READY/ACK/Confirm-VisibleFact 主路径。3) S2/S4 点击 Allow/Deny 前机器 layout gate；S3 额外 Start/STOP_SKIPPED/错误 requestId → invalid；S5 原子步骤机器 gate + `:vpn` absent；S6 唯一 A CREATE_ACCEPTED + 唯一 B CREATE_REJECTED(2203002)，删除 no_dual/dual 判据；S7 绑定 S6 A request，无 FINAL-CLEANUP。4) 未执行场景 `not-run-due-to-invalid`；`operator-wait-state` schema v2 绑定 manifest；attestation 仅机械完成时间。
- **历史标注**：operator 三态 / READY-ACK / 人工可见事实确认仅 historical，不得作为新 campaign 当前规则。
- **生效条件**：只适用下一新 campaign/evidence；`EV-E3-PHYS1API26-20260808-0001` 与全部历史 sealed evidence **不改字节、不重判**；保留 `ADJ-20260808-0001` 底层修复。
- **审查状态**：host selftest `HDC_PROCESSES=0`、runner `-SelfTest`、parser 通过；登记时 `plan_status: blocked-awaiting-device-authorization`（禁 HDC），E3 未关闭，E8 `CLOSED`。

### ADJ-20260808-0001：进程边界前瞻修复（严格进程边界探针目标 / UI last-known / S4 预截图 / S5 探针间隔 / operator-wait-state / aggregation 三态）

- **提出角色**：主会话编排 + 执行子代理（本次 E3 sealed blocked 前瞻修复任务）；host 侧实现登记，非设备证据。完整登记见 [`e3-physical-preflight-process-target-2026-08-08`](evidence/e3-physical-preflight-process-target-2026-08-08.md)。
- **触发证据**：`EV-E3-PHYS1API26-20260808-0001`（sealed blocked，保留不改写）；历史 0002 双审查 0 B/5 M 中的 S3/S5/S7 终态缺口；`ADJ-20260807-0003` 确立的 host process terminal probe 路径。
- **调整原因**：旧 bundle 级 `PidOf` 在**正常 Stop 且 App UI 可见**时物理不可满足（Entry UI 进程不随 Extension 终止退出，`pidof <bundle>` 永远非空），strict-process-boundary fallback 的连续 absent 在合法路径上无法成立；另修复 S3 首次 `UI_STOP_SKIPPED` 与人为二次 Start 的 requestId 错配、S4 deny 截图后置无法证明拒绝画面、S5 `LastProbeAt` `[string]` 转换丢亚秒使实际间隔可低于冻结 3.0s、`scenario_aggregation.s3_clean_reactivation_proof` 对 ordered dictionary 读取恒 false、缺少主会话可轮询的等待状态。
- **调整内容**：1) `PidOf` 目标改为 `<bundle>:vpn` Extension 能力进程（定向精确名；禁止宽泛 process list）；`BundleDump` 继续证明 bundle/主 App 安装存在；探针与 S3/S5/S7 记录新增 `process_target`；freeze 新增必填字段 `process_probe_target`（进入 freeze contract，旧 freeze 一律拒绝）。2) UI last-known request id：pending release 不清、Stop 在 active 为空时以 `basis=last-known-request` 使用、新 Start 覆盖、Stop 成功/terminal rejection（含 16000001）清理；resolve 文案改为 `Start request resolved` + “VPN create follows extension events”。3) S4 在用户点 Deny 前确认授权/拒绝界面可见并立即 capture（`deny_screen_capture`），随后 Deny + ACK + 完整 60s 观察；预截图 + 人工确认 + 无 B create 构成 deny proof。4) S5 探针调度 round-trip 精确 `DateTimeOffset`（+0.1s 裕量），实际间隔 ≥ 冻结 3.0s（阈值不降、无事后容差）；连续 absent 计数达到但间隔不足时继续探。5) 新增 `operator-wait-state.json`（EvidenceRoot 内稳定路径，主会话无需设备命令即可发现；每次 READY/ACTION ACK/visible fact 落盘 scenario/phase/kind/expected/nonce/capture_done/response_valid/updated_at，完成时 complete；禁止 target/UDID/HAP 路径/endpoint/secret；经 manifest + record 引用封签）。（**历史字段**：`ADJ-20260808-0002`/`0003` 后为 schema v2 字段 `scenario/step_index/step_id/expected_action/phase/capture_before/capture_after/machine_precondition/machine_postcondition/updated_at/complete/completed_at/history`，不再写 READY/nonce。）6) `scenario_aggregation.s3_clean_reactivation_proof` 三态化（S3 实测 true/false、未探测 null）。
- **已评估替代方案**：保留 bundle 级 pidof + 放宽 absent 判据（会伪造通过，不可行）；`hidumper`/全量进程列表（违反白名单）；跨进程文件桥/完整 UI 持久化（超出最小修复）。
- **生效条件**：冻结新 freeze（含 `process_probe_target`）并绑定本执行 commit 后对下一新 campaign 生效；只适用下一新 campaign/evidence，不回溯修改 `EV-E3-PHYS1API26-20260808-0001` 或任何历史 evidence 字节。
- **回退条件**：`<bundle>:vpn` 精确名在目标元组上不可观测时 S3/S5/S7 strict fallback 记 blocked，不得退回 bundle 级 pidof；由新路线决策决定下一步。
- **审查状态**：host selftest `HDC_PROCESSES=0`、audit 源码约束复核、markdownlint 通过；独立审查角色待 freeze 重新绑定；登记时 `plan_status: blocked-awaiting-device-authorization`（禁 HDC），E3 未关闭，E8 `CLOSED`。

### ADJ-20260718-0001：E3 不可执行边界与唯一物理预检

- **提出角色**：用户（直接人类决策者）。
- **日期与时区**：`2026-07-18T10:31:00+08:00`。
- **触发证据**：[E3 Emulator 矩阵审查](evidence/e3-vpn-extension-api24-emulator-matrix-2026-07-17.md)聚合 phone 0003/0004、2in1 0001/0002 和 Tablet 0001。phone 的公开 runtime 与注册前置均 blocked；2in1、Tablet 只在 registration-layer 前置边界确认组件缺失并停止，未安装 HAP。
- **调整原因**：现有官方 image 无法产出 Emulator E3 pass；继续要求 E3-E7 全部 pass 会形成不可执行依赖，但把这些项写成 `N/A` 又会错误免除完整 VPN 义务。需要一个严格有界的物理 E3 可达性输入，同时维持 E1、哈希和独立聚合门。
- **受影响阶段**：R0 的投入顺序、E3-E8 的聚合与开放条件，以及 E8 `OPEN` 后由 R2/R3 承接的 E4-E7 完整义务；不改变 R1 退出状态或 R4-R10 的既有退出标准。
- **授权依据**：用户于 2026-07-18 作出直接人类决策，批准一次具名 HarmonyOS 6.1 arm64、纯 ArkTS/C、普通开发签名、公共 VPN API 的 `E3-PHYS-PREFLIGHT`。该决定按本节“人类直接决策者优先”规则，仅在精确例外范围内替代这一次内部 T0 触发和 2026-07-17 的原物理设备禁令，不开放其他物理设备工作。
- **调整内容**：E3-E7 在 Emulator 聚合中统一记为 reviewed dependency-blocked aggregation exception，不是 `pass` 或 `N/A`。E4-E7 完整义务移交 E8 `OPEN` 后同一具名设备的 R2/R3 门。E8 `OPEN` 只许可后续物理投入，不表示 VPN 或数据面通过。
- **唯一例外**：只允许一个 campaign；必须冻结设备型号、完整 build、API、arm64、HDC 映射、普通开发签名/profile、A/B HAP、源码/SDK/hash、清理基线、每场景 60 秒窗口和 Settings re-allow 预期。只覆盖 allow、deny、`onCreate`、`VpnConnection.create` fd、active stop、Settings revoke、第二 VPN 冲突和清理。
- **重试与禁止项**：同一冻结元组仅纯基础设施性 overall blocked 可有一次记录在案的完整重试；功能 fail、invalid、非基础设施 blocked 或输入变化必须先取得新路线决策。禁止 Go、NetBird、WireGuard、私有 fork、产品代码、`MANAGE_VPN`、system/debug/enterprise/root、隐藏服务、权限授予和策略绕过。
- **证据与 E8 条件**：必须保留未筛选 HiLog、完整 transcript、截图、状态/布局、fault list 和 hash manifest，并由独立角色审查。只有预检同时为 `record_status: reviewed-pass`、`verdict: pass` 才满足 E8 的预检必要条件；它仍非充分条件。`blocked`、`fail` 或 `invalid` 均保持 E8 `CLOSED`。
- **已评估替代方案**：继续等待含组件的官方 image，风险最低但无法判断已具名物理目标的 E3 可达性；把 E3-E7 记为 `N/A` 会错误免除义务；扩大到多设备、Go/NetBird 或特权探针会突破投入与权限边界；私有 image 或私有 API 无法形成普通第三方公共路径证据。因此只采用一次受限预检。
- **R0/SLO/补丁预算影响**：修改 R0 的物理投入顺序和 E8 必要条件，不改变首目标候选、功能范围、任何 R0 SLO、统计口径或分阶段补丁上限；不授权上游补丁，当前补丁计数仍为 0。
- **T0 状态**：本记录适用本节“人类直接决策者优先”规则，只替代这一次内部 T0 触发；它不是 T0 结论，未执行或声称新的 T0 共识。若用户明确要求 T0，仍须按 T0 协议执行。
- **生效条件**：路线规则立即生效；设备端 campaign 仅在全部输入冻结、执行/独立审查角色分离且专用计划可执行时生效。E8 状态只有在预检 `reviewed-pass/pass`、当前R0正式基线（现v0.74.7）的 E1 `reviewed-pass/pass`、全部目标元组/哈希一致及独立聚合审查显式批准后才可改为 `OPEN`。（**历史条件标注**：本记录生效时（2026-07-18）的 E1 基线条件为 v0.74.7；该历史条件已被 `ADJ-20260809-0001` 正式采用 v0.76.3 取代，当前 E1 基线条件为 v0.76.3，见下文 ADJ-20260809-0001。）
- **回退条件**：用户撤销授权、范围越界、禁止能力出现、输入漂移、证据污染、campaign/重试纪律破坏，或新官方 image/build 含所需组件时，停止例外并回到物理设备禁止状态；新 image/build 必须从 E3 重验。功能 fail、invalid 或非允许重试的 blocked 继续保持 E8 `CLOSED`，后续须新路线决策。
- **审查状态**：路线授权由用户于 `2026-07-18T10:31:00+08:00` 批准；initial live 证据现已完成独立审查并为 `reviewed-pass/blocked`，E8 聚合审查仍未通过。本文不把路线批准或 `reviewed-pass` 写成 E3 pass。
- **当前状态**：六条只读 discovery 与唯一 signing enrollment 均已消费。API 23 纯 ArkTS/C A/B、普通开发签名、FINAL HAP/profile/certificate、源码/SDK/hash、runner 与角色输入曾完整冻结；API23 initial live 发现 build drift 后在 continuous capture/install 前停止，`campaign_started=false`。旧 evidence ID 已占用且不可复用，无基础设施 retry 授权。后续 rebind / `ADJ-20260806-0003` / host reverify / `ADJ-20260806-0004` build 确认 / `ADJ-20260807-0001` 跨日重新编号 / API26 live `EV-E3-PHYS1API26-20260807-0001`（`consumed-blocked`，保留）/ `ADJ-20260807-0002` 中文完整重跑与 `EV-E3-PHYS1API26-20260807-0002`（`consumed-blocked`）/ `ADJ-20260807-0003`（host process terminal probe 与 Settings 应用信息强制停止撤销路径，runner 变更完成，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)）见对应记录；历史（2026-08-10 前）`plan_status: blocked-awaiting-device-authorization`（candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`；该旧 pair 因外部 sealed blocked evidence `EV-E3-PHYS1API26-20260808-0001` 已消费不得复用）。**历史（2026-08-10 · 0002）**：用户曾显式授权**新**完整白名单 campaign（[`AUTH-E3-PHYS1API26-20260810-0002`](evidence/e3-physical-preflight-authorization-2026-08-10-0002.md)，取代已消耗的 [`AUTH-E3-PHYS1API26-20260810-0001`](evidence/e3-physical-preflight-authorization-2026-08-10.md)；新候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`；新增一次 host-prep `hdc list targets` 内存级窄例外；attempt=initial/retry N/A、任一 gate 失败停止、不重试不换 ID），`plan_status: authorized-awaiting-windows-ready-freeze`（audit-1/audit-2 均 hash 记录；Live 前允许 runner+治理变更审查后提交/推送，campaign evidence 仍不提前提交）。
- **当前覆盖（2026-08-29 · 0001）**：[`AUTH-E3-PHYS1API26-20260829-0001`](evidence/e3-physical-preflight-authorization-2026-08-29-0001.md) 绑定新 pair `E3-PHYS-PREFLIGHT-20260829-0001` / `EV-E3-PHYS1API26-20260829-0001`，attempt initial / retry N/A；当前 `blocked-awaiting-full-gates`、not ready，20260829-0001 machine confirmation/review/freeze/DryRun/Live 均 pending。**S7 前置校准**（依据 20260828 Live 双处观察：B 的 `:vpn` 进程在冲突拒绝后存活为平台行为，S7 前置只门控 A active、B observed-only）。S5/S6、reviewer、source/HAP 逻辑不改；20260828-0001 已 Live consumed-blocked（S1-S6 首次全 pass、S7 前置模型与平台行为不符 blocked，证据 `reviewed-pass/blocked`，不得复用）；20260825-0001 已因 gate 5 TargetBindingConfirm 实测设备 OTA build 与冻结 build 漂移以 `governance-tuple-drift-retired` 退役（blocked confirmation record，非流程违规），gate 6-13 未运行、未 DryRun/Live、未 consumed；20260817-0002 已在 gate 4（用户 host-prep）未完成时提前执行 gate 5 TargetBindingConfirm 产生 blocked confirmation record 以 `governance-order-invalid-retired` 退役，gate 6-13 未运行、未 DryRun/Live、未 consumed；20260817-0001 已在 gate 9 因未授权设备进程枚举以 `governance-operation-invalid-retired` 退役，audit-2 文件不构成 gate pass，gate 10-13 未运行，未 DryRun/Live、未 consumed。历史 AUTH/pair 均不得复用；E8 `CLOSED`。

### ADJ-20260718-0002：正式采用 NetBird v0.74.7

- **提出角色**：用户（直接人类决策者）。
- **日期与时区**：`2026-07-18T10:50:00+08:00`。
- **触发证据**：NetBird 正式非 prerelease tag `v0.74.7`、固定 tag commit、官方 Release run，以及 [Tailscale-OHOS VPN 数据通路审计与 NetBird 映射](tailscale-ohos-netbird-port-audit.md)对 v0.74.6 至 v0.74.7 的固定源码差异核对；官方 URL 的访问日期为 2026-07-18。
- **调整原因**：R0 规则要求后续门跟随并显式固定最新正式 NetBird release；v0.74.7 已发布且版本输入可复核，继续仅登记而不作采用决定会使后续门的正式基线不明确。
- **调整内容**：自本记录生效起，当前R0正式基线采用 NetBird `v0.74.7`。后续新运行使用该版本；所有实际使用 v0.74.6 的历史 evidence 继续绑定原 tag、commit、制品 hash 和判定，不改写、不重判，也不因新基线自动失效或通过。
- **受影响阶段**：R0 版本锁定、E1 官方 Go loader/runtime、E8 聚合输入，以及后续实际消费 NetBird 客户端或服务端基线的 R2-R8 与 R10 依赖维护；不改变任何阶段当前退出状态。
- **版本与发布核对**：tag `v0.74.7`；commit `a1c9427d8004576e2cbb9e546d409847fa9df318`；`go 1.25.5`；`toolchain go1.25.12`；官方 Release run [`29587548629`](https://github.com/netbirdio/netbird/actions/runs/29587548629) 成功；固定 run URL 访问日期 `2026-07-18`。
- **wireguard-go 差异**：NetBird `go.mod` 的 `netbirdio/wireguard-go` replace 从 v0.74.6 的 `2834bebf6c1aea76bd217f31ea91c99f75e4a20a` 更新到 v0.74.7 的 `8ec1ad32882fab0432317d027b0189371782ad01`；该变化须进入新基线依赖、SBOM、许可证和功能回归输入，不能复用 v0.74.6 的完成结论。
- **已评估替代方案**：继续以 v0.74.6 作为新运行基线会偏离“最新正式 release”规则；只登记 v0.74.7 而延后采用会让 E1 和后续门输入悬而未决；静默替换历史 evidence 会破坏可追溯性。故采用 v0.74.7 作为后续基线，同时完整保留 v0.74.6 历史绑定。
- **R0/SLO/补丁预算影响**：更新 R0 的 NetBird client/server 版本输入，不改变首目标候选、功能范围、任何质量、安全、性能、隐私 SLO、错误预算阈值或分阶段补丁预算；不授权任何 Go、NetBird、wireguard-go 或 SDK 补丁，当前补丁计数仍为 0。
- **重跑范围**：首先以 v0.74.7 重跑 E1 官方 Go loader/runtime；在 E8 聚合前重核相关上游 SHA 和全部输入 hash。进入相应阶段前，重新生成或复核 v0.74.7 客户端/服务端及 wireguard-go 变化涉及的依赖清单、SBOM、许可证与 AGPL 分析，并对 R2-R8/R10 中实际消费该基线的门执行风险对应回归。v0.74.7 的 E1 当前尚未重跑，未达到 `reviewed-pass/pass`。
- **T0 判定与决策依据**：版本采用属于重大技术输入调整，本记录按本节“人类直接决策者优先”规则，以用户明确决定替代这一次内部 T0 触发；未执行或虚构 T0。若用户明确要求 T0，仍按 T0 协议执行。
- **生效条件**：上述 tag、commit、Go/toolchain、成功 Release run 和固定访问日均可复核，且文档同步保留 v0.74.6 历史边界后立即生效；生效只改变后续正式输入，不赋予任何 E/R 门通过状态。
- **回退条件**：tag/commit 或 Release run 核对被推翻、v0.74.7 被上游撤回、出现阻断级安全/许可证问题，或重跑证明其无法在既有 R0 预算内继续时，停止新基线运行并由新的动态调整记录决定回退或前进版本；不得通过改写 v0.74.6 历史 evidence 实施回退。
- **审查状态**：用户于 `2026-07-18T10:50:00+08:00` 明确批准正式采用；文档层调整已进入当前未提交 diff。E1、SBOM、依赖、许可证和 AGPL 的新基线证据审查尚未完成，本记录不把这些事项写成通过。

### ADJ-20260717-0001：Go 跨语言替代路线 T0 一致结论

`EV-R1-EMU24-20260717-0007` 触发的 T0 路线讨论已达成一致，`EV-R1-EMU24-20260717-0008` 已执行其首个公开能力门；本调整只决定研究顺序、停止条件和证据边界，不改变首目标候选、R0 SLO、补丁预算或阶段退出标准。

- **C1 启动边界**：普通 HAP 没有应用可控制的进程启动期 `DT_NEEDED` 注入点，因此不再把 runtime wrapper、加载顺序或其他进程内 late-load 变体描述成 startup linkage。
- **C2 公开能力门**：B 族只能使用普通第三方应用公开 SDK；必须先由公开文档与目标 SDK 共同确认 Native Child Process 的目标设备支持，且不依赖 debug、system、企业权限或系统策略修改，头文件存在本身不等于 phone 可用。
- **C3 B0/B1 串行门**：公开能力门通过后才构建 API 24 x86_64 Emulator 的 B0 无 TLS C child baseline；B0 通过后才运行含真实 initial-exec TLS 且预期 `GetTLS=42` 的 B1 C child，B0 失败则不运行 B1，B1 失败则不运行 child Go `dlopen`。
- **C4 回传与失败纪律**：B0/B1 必须通过真实公开入口、签名一致的 HAP、可收集 PID/entry/result、异步等待和完整 HiLog 得出结论；平台或设备形态不支持时记录精确错误码与枚举，不使用 fork/exec、debug/system 能力、设备类型伪装或策略开关规避。
- **C5 负面范围**：所有当前负面只绑定已记录的 API 24 x86_64 phone Emulator、进程模型和 ELF 输入；arm64、具名真机、PC/2in1、Tablet、其他 loader 与工具链在各自证据形成前保持 provisional，不从 x86_64 外推。
- **C6 B 族后继**：只有 B0 可用且通过、B1 失败时，下一步才是验证普通应用公开 exec/PIE 能力并运行固定 Go executable；该条件未成立时不得跳到 executable、Go runtime、netpoll 或 NetBird。
- **C7 A 族门**：A 工具链 spike 只能在无补丁 B 族按上述门关闭后，由另一次 T0 先确定 timebox、固定输入、退出标准、补丁预算与长期维护阈值；不得把 toolchain/runtime fork 当成默认后继。
- **C8 非产品承诺**：B 族或 A 族任一路线成功都只允许进入下一研究证据门，不自动形成产品架构承诺、平台支持声明、VPN 可行性、发布承诺或阶段退出。
- **C9 当前状态**：0008 已确认公开 API 对 API 14 及以后仅支持 PC/2in1、Tablet，phone 作为非 PC/2in1/Tablet 的其他设备类型映射 `NCP_ERR_NOT_SUPPORTED(801)`，故 API 24 phone B0/B1 在实现前暂停，补丁数为 0；该暂停可由华为商用 HarmonyOS 具名真机行为或公开 SDK 设备范围变化重开，并非全局永久关闭。Tablet/2in1 仍为公开文档支持范围，但不属于当前 phone 目标；R0、R1、R2 均保持未退出，A 工具链 spike 仍须新 T0 决定 timebox。

### ADJ-20260717-0002：动态 TLS loader Tier1 六席一致结论

`EV-R1-EMU24-20260717-0008` 关闭公开 phone Native Child 入口后，第二次六席 T0 一致批准先把 loader 是否支持动态 TLS 与 Go 是否能生成该模型拆成两个串行门；本调整只授权 Tier1 纯 C 探针，不授权 Go/runtime/toolchain patch，不改变 R0 SLO、阶段退出标准或补丁预算。

- **D1 timebox 与实现次数**：Tier1 限 1 工作日/8 工程小时/1 次实现，使用现有 `native-probes`、Node-API 和 TestRunner，0 Go/NetBird/SDK patch，且不提交。
- **D2 最小输入**：必须实际生成可独立命名的 IE 对照、classic GD 和 TLSDESC gnu2；local-dynamic 只有在同一实现自然可得时附加且不改变 PASS 门，每份库都导出确定的 `GetTLS`、`SetTLS`、`ResetTLS` C ABI。
- **D3 最终验模**：模型身份只接受最终 `readelf`/`objdump`；IE 必须有 `PT_TLS`、`TPOFF`、`STATIC_TLS`，classic GD 必须有 `DTPMOD64/DTPOFF64` 或明确 classic `__tls_get_addr` 重定位且无 `TPOFF/STATIC_TLS`，TLSDESC 必须有 `R_X86_64_TLSDESC` 且无 `TPOFF/STATIC_TLS`；允许用标准 linker `--no-relax` 防止模型塌缩，不修改 SDK。
- **D4 IE 漂移门**：普通 Node-API baseline 必须先 PASS；IE 必须在 0007 同一 TestRunner 和 `RTLD_NOW|RTLD_LOCAL` late-load 边界复现 initial-exec 拒绝，若意外加载则标环境漂移并立即停止。
- **D5 动态线程门**：classic GD 与 TLSDESC 各自在 `dlopen` 前建立等待线程，加载后让它解析并调用 ABI 100 轮，再建立加载后新线程调用 100 轮；主线程与两线程必须用不同值并分别验证初值42、set/read 不串扰和 reset42，任何错误或 crash 都使该模型 FAIL。
- **D6 判定范围**：Tier1 `PASS` 等于 classic GD 或 TLSDESC 至少一个全过；两者都失败只 `STOP` API 24 x86_64 元组，arm64 与具名真机保持 provisional；任何结果都不退出或回退 R0、R1、R2，也不自动形成 Go、NetBird、VPN 或产品结论。
- **D7 Tier1 实测**：0009 最终验明 IE、classic GD、TLSDESC 和附加 local-dynamic 均未塌缩；IE 对照按预期拒绝，GD、TLSDESC、LD 的主线程/加载前线程/加载后线程各 100 轮全部通过，无串扰或 crash，因此 Tier1 `PASS`，该 x86_64 元组不停止，补丁数为 0。
- **D8 下一门**：Go issue `#71953`、Go CL `644975`、Go CL `696635`、Go PR `75048` 当前均 open 或 `NEW`、未合并、未发布，只能作为下一道独立授权 Tier2 Go/toolchain 可行性门参考；Tier2 必须另行固定已发布输入、timebox、退出标准、维护阈值和补丁预算，0009 不自动授权实现。
- **D9 预定门**：在记录的 T0 和人工批准下，Tier2 限 `16h/2d`、最多两个不可变输入和两次迭代。迭代1只允许直接重建干净 PS4 `5f5911fabb3af7b5662ebc17ff7fa4f881df903a`，不改 Go 源码；候选必须先以 x86_64 c-shared 最终 ELF 的 `TLSDESC`、无 `TPOFF`/`STATIC_TLS` 通过身份门，再在 API 24 x86_64 TestRunner 中十个不同且退出的 PID 上完成 late `dlopen`、加载前/后 C 线程 `dlsym`、`Hello=42`、goroutine/channel/timer/allocation 和 loopback `net.Dial`，每次 `ResultCode 0`、无 crash/hang。迭代2只允许把原始 PS4 binary diff 对官方 Go 1.25.12 执行一次机械 `git apply --3way`；任一冲突、缺失路径或需要语义解决即为预定 `STOP: high-maintenance`，禁止第三次迭代。
- **D9 事后结果**：原始第一次全量 app buffer 未在停机前持久化，故不作为证据。`EV-R1-EMU24-20260717-0010` 的同制品可复算重放已用同一 APP/TEST/member 哈希完成上述十次运行：十个新 PID 的框架结果均为 0，完整无筛选 app buffer 保留了十条 suite PASS、十条 pre PASS 和十条 post PASS，并在停机前记录 fault/crash 清单及进程退出。迭代2随后在五个文件冲突和缺少 `runtime/cgo/gcc_unix.c` 时达到预定 high-maintenance STOP；没有语义解决、官方 Go 构建或 target run。补丁数仍为 0，CL696635 仍为 `NEW`、未发布，R0/R1/R2 不退出。

### ADJ-20260717-0003：API 24 x86_64 phone Emulator 投入总门

E0-E8 是物理设备产品投入前的聚合总门。所有客观可执行 Emulator 项必须形成 `record_status: reviewed-pass`、`verdict: pass` 并保持目标元组和 hash 一致；经审查证明因平台前置组件缺失而 blocked 的 E3-E7 必须保留原判定，并显式登记为 reviewed dependency-blocked aggregation exception，不能伪造 pass 或写成 `N/A`。E8 的任何状态变化都须独立聚合审查；新官方 image/build 若含所需组件，旧 blocked-exception 不再适用，必须重验。

- **E0 普通应用**：普通第三方 phone 应用完成构建、安装、普通 `EntryAbility` 启动、可观察运行、停止、卸载和清理；`10106102` 未解决时本项失败。
- **E1 ArkTS/native/fd ownership**：验证 ArkTS 与 native 双向调用、异步线程回调，以及 fd 的创建、复制、移交、关闭、重复关闭防护和异常清理所有权；还必须使用当前R0正式基线（现v0.76.3）声明的 Go/toolchain 输入验证官方 Go 制品可加载并运行该边界，C-only 结果只能作为子证据。v0.74.6 历史 loader 负面保持原绑定；v0.76.3 由 `EV-E1-EMU24-20260809-0003` 实测 `reviewed-pass/blocked`（[证据](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md)），仍无 pass。
- **E2 C 网络**：以不依赖 Go 的 C native 路径验证 TCP/UDP、DNS、loopback 与外部测试端点的可判定网络收发和错误传播。
- **E3 VPN Extension 授权**：普通第三方应用通过公开 VPN Extension 路径验证授权、拒绝、撤销和冲突状态，不使用特权绕过。phone 记录的公开 runtime 路径 blocked；2in1、Tablet 只在 registration-layer 前置边界 blocked，未形成完整 runtime 结论。E8 前只允许按 `E3-PHYS-PREFLIGHT` 判断一个精确物理目标的 E3 可达性。
- **E4 `setUp`/TUN 配置**：实际调用 `setUp` 建立虚拟接口并核验 fd、地址、路由、DNS、MTU、IPv4 及声明范围内 IPv6 的配置与清理。
- **E5 `protect` 真实绕行**：对真实外层 TCP/UDP socket 在正确时点调用 `protect`，以可观察流量证明绕过隧道而非仅验证 API 返回值。
- **E6 C native 双向泵**：用纯 C native 数据泵在 TUN 与真实业务端点之间传输双向流量，验证背压、部分读写、fd 关闭和异常清理。
- **E7 lifecycle/故障短循环**：在 Emulator 的可靠时间窗内执行有界短循环，覆盖重复启停、撤权、断网、进程退出和故障清理；25 分钟以上 soak、能耗和硬件相关长稳不列为本总门必过项。
- **E8 聚合**：核验所有客观可执行项的 `reviewed-pass/pass`、E3-E7 的 reviewed dependency-blocked aggregation exception、目标元组和全部 hash；当前R0正式基线（现v0.76.3）的官方 Go loader/runtime 必须形成 E1 `reviewed-pass/pass`，`E3-PHYS-PREFLIGHT` 也必须同时为 `reviewed-pass/pass`，最后由独立聚合审查显式决定是否 `OPEN`。任一条件缺失或预检为 `blocked/fail/invalid` 都保持 `CLOSED`；`OPEN` 只是物理投入许可，不是 VPN/数据面通过。

## 实施原则

- **投入总门优先**：所有 API 24 x86_64 phone Emulator 上客观可执行项优先完成；E8 未通过时只允许一个严格受限的 `E3-PHYS-PREFLIGHT` campaign，其他物理设备执行仍禁止。
- **单一主执行者**：任何时点只有一个主执行者负责当前首目标实现流、集成决策、证据归档和阶段退出判断。
- **串行风险门**：首目标的 `R0` 至 `R8` 按顺序推进。前一阶段未满足退出标准时，不以日历进度替代证据进入下一阶段。
- **受控并行**：许可证、威胁模型、目标设备与渠道调查、测试设计和构建自动化等横切研究可以提前，但不得形成竞争实现流，也不得绕过当前风险门合并产品实现。
- **支持声明有界**：每项支持声明必须同时绑定发行版、具名设备、完整系统版本、架构和分发渠道；缺少任一维度时不得扩展为平台级支持。
- **物理设备证据有界**：E8 前的唯一预检只判断精确目标 E3 可达性；E8 `OPEN` 只允许进入具名物理设备 R2/R3 等后续门，E4-E7、arm64、硬件、渠道、能耗、长稳和发布支持仍须分别形成完整证据。
- **双向不外推**：API 24 x86_64 phone Emulator 的 PASS 与 FAIL 都只覆盖该目标元组，不外推 arm64、具名真机或华为商用 HarmonyOS。
- **设备形态独立**：phone、2in1、Tablet 及其他 Emulator 形态分别使用独立 image、实例、端口、输入和证据记录；补充形态记录不进入 phone E8 聚合，也不能替代任一其他形态或真机证据。
- **API 主张待验证**：未经目标 SDK 声明与头文件、最小样例编译、项目代码和具名真机共同证明的 API 或桥接主张，只作为待验证假设。
- **失败范围明确**：失败结论只覆盖其证据所绑定的目标、版本、设备、架构、渠道和实现路径，不无依据推广到其他目标。

## 版本与证据门

版本号表示已通过的证据门，不承诺固定总周数：

| 版本 | 对应证据门 |
| --- | --- |
| `0.1` | `R2` Go 生存门 |
| `0.2` | `R3` VPN 切片 |
| `0.3` | `R4` NetBird E2E |
| `0.4` | `R5` 产品 alpha |
| `0.5` | `R6` 硬化基线 |
| `0.6` | `R6` 完整支持矩阵及安全、稳定、性能证据 |
| `0.7` | `R7` 发布工程 |
| `0.8` | `R8` dogfood 与封闭 beta |
| `0.9` | `R8` RC 与渠道最终制品回归 |
| 每目标独立 `1.0.0` | `R8` 目标限定 GA |

`R8` 横跨 `0.8` 至每目标独立的 `1.0.0`：其阶段内依次包括 dogfood、封闭 beta、RC、渠道最终制品真机回归和目标限定 GA。RC 不是独立证据门。

阶段推进以退出标准为准。版本不得用于替代支持矩阵、测试报告或尚未满足的证据门。

## R0：任务章程与首目标确认

### R0 目标

建立可审计的范围、责任、质量阈值和证据规则。首目标暂定 HarmonyOS，但最终由最早同时具备普通第三方 VPN 路径、可执行签名流程、明确分发渠道和具名量产设备的目标确认。

### R0 关键交付物与验证

- 锁定首目标的发行版、具名量产设备、完整系统版本、架构、SDK/SysCap 依据和渠道，并锁定该真机的 HDC 安装、启动、日志与卸载闭环。
- 锁定 NetBird client/server 版本，定义认证方式、必须功能、明确排除项、服务端兼容范围和补丁预算。
- 明确开发、测试和发布签名责任，以及渠道账号、审核、市场重签和最终制品获取责任。
- 定义连接成功率、重连、吞吐、时延、CPU、RSS、能耗、稳定性和隐私 SLO 及错误预算；阈值由本阶段确定，后续阶段不得自行放宽。
- 定义证据与脱敏 schema，包括目标矩阵、工具与依赖版本、命令、测试输入、预期结果、实际结果、时间戳、制品哈希、日志字段、敏感字段过滤和保留期。
- 完成许可证清单、初始威胁模型、具名真机准备、渠道可达性和维护责任确认。
- 保留人工 bootstrap 工具链流程；持续集成只校验固定版本、校验和并归档合规允许的制品与证据，不尝试替代授权人员首次获取或协议处理。

### R0 退出标准

首目标及全部支持维度已具名，设备与 HDC 可供后续验证，范围、版本、阈值、排除项、证据 schema、许可证边界、威胁模型、签名责任和渠道责任均有可执行记录；普通第三方 VPN 路径仍明确标记为待验证假设。

### R0 依赖

依赖现有环境、工具链和平台策略文档，以及目标厂商、设备、渠道和签名责任人的可核验信息。

### R0 停止或回退条件

若暂定目标无法同时提供普通第三方 VPN、签名、渠道和具名量产设备条件，停止该目标的实现投入并重新进行 T0 选择。若无目标满足条件，路线图停留在研究阶段，不建立竞争实现流。

## R1：ABI 预探针与最小 HAP 闭环

### R1 目标

以不超过两个工作日的预探针尽早识别目标 ABI/libc 对 Go、syscall 和固定依赖的硬阻断，同时建立短生命周期测试 HAP 的最小开发闭环。

### R1 关键交付物与验证

- 针对目标 ABI/libc 执行 Go 工具链、关键 syscall、链接方式和固定依赖子集的最小预探针，保留成功与失败日志及可复现命令。
- 构建短生命周期测试 HAP，验证 CLI 构建、debug 签名、安装、Ability 启动、普通 native 库加载、日志采集和卸载。
- 在 API 24 x86_64 phone Emulator 上执行并关闭全部客观可执行项；E8 未通过时，除唯一 `E3-PHYS-PREFLIGHT` 外不得执行具名物理设备 HDC 操作，arm64 ABI、硬件、渠道、能耗与长稳仍留作后续真机专属证据。
- 将预探针失败分为工具链、ABI/libc、syscall、依赖、签名、安装或设备连接问题。

### R1 退出标准

E8 已先通过并允许真机投入，两工作日内形成可重复的预探针结论，随后在具名真机完成最小 HAP 闭环。正面预探针只允许进入 `R2`，不构成 NetBird、VPN 或产品可行性证据。

### R1 依赖

依赖 `R0` 锁定的目标、SDK、架构、真机、HDC、debug 签名责任和证据 schema。

### R1 停止或回退条件

ABI/libc、Go 或关键 syscall 的负面结果可以触发 T0 首目标重选。若失败仅属于可修复工具或签名准备问题，则回退修复 `R0` 准备项后重跑；不得用 Emulator 成功替代真机 HDC 失败。

## R2：Go、依赖与实际桥接机制递增验证

### R2 目标

在 HAP 应用进程内递增证明 Go runtime、cgo、线程与所有权、网络基础、固定 NetBird 依赖子集、wireguard-go 和实际 SDK 桥接机制可共同工作。

### R2 关键交付物与验证

- 依次验证 Go runtime/cgo、线程创建与回调、调用方和被调用方所有权、网络/DNS/socket、固定 NetBird 依赖子集及 wireguard-go。
- 桥接机制不预设为某一技术；只接受目标 SDK 声明、头文件、官方或随 SDK 样例、实际编译结果和 R0 具名真机运行结果组成的证据链。
- 完成 `ArkTS -> native -> Go` 同步调用和 `Go/native -> ArkTS` 异步线程回调。
- 明确并测试字符串、字节缓冲区和 fd 的创建、复制、移交、释放与关闭所有权。
- 验证多个动态依赖同时加载、重复启动与停止，以及 panic/crash 后由系统回收进程并能再次启动。
- 在 arm64 具名真机复现完整结果；仅证明 `.so` 可加载不满足退出要求。
- `client` 或 `embed` 只作为候选集成入口评估，不因选择候选入口而删除认证、management、signal、peer、route、DNS、重连等控制面验收范围。

### R2 退出标准

全部桥接方向、异步线程、所有权、多动态依赖、重复启停和异常回收用例在 arm64 具名真机可重复通过，并形成固定依赖与补丁清单。此时可标记该目标 `0.1` Go 门通过，但不表示 VPN 可行。

### R2 依赖

依赖 `R1` 的最小 HAP、目标 ABI 结论、具名真机 HDC 闭环，以及 `R0` 的版本和证据规则。

### R2 停止或回退条件

将失败分类为目标平台限制、ABI/libc、Go runtime、依赖、桥接、线程、所有权或工程实现问题。无法在补丁预算内解决，或实际 SDK 与真机否定所需能力时，停止当前实现并重新进行 T0 讨论；不得只保留 `.so` 加载结果继续推进。

## R3：普通第三方 VPN 数据通路

### R3 目标

先以最小 native 数据泵证明普通第三方应用 VPN 路径，再接入 Go/WireGuard，避免将数据面问题与语言运行时问题混合判断。

### R3 关键交付物与验证

- 在具名真机验证第三方 VPN 授权、拒绝和撤销，以及与其他 VPN 冲突时的行为。
- 取得虚拟接口 fd，验证地址、路由、DNS、MTU、IPv4 和明确声明范围内的 IPv6 行为。
- 为 fd 建立复制、移交、关闭顺序和异常清理契约，并通过 native 数据泵验证双向真实业务流量。
- 识别所有外层连接及重连创建的 socket，在建连所需时点逐一验证目标 SDK 所证明的 `protect` 机制；不得只覆盖首次连接或单一 socket 类型。
- 在 native 数据泵通过后接入 Go/WireGuard，重复双向业务流量、重连和清理测试。
- 验证授权拒绝、授权撤销、VPN 冲突、crash、进程 kill 和重复启停后接口、fd、路由与连接状态得到清理。

### R3 退出标准

具名真机上的 native 数据泵和 Go/WireGuard 路径均可稳定传输双向真实业务流量，所有外层及重连 socket 均有保护证据，拒绝、撤销、冲突和异常退出清理可重复通过。此时可标记该目标 `0.2` VPN 门通过。

### R3 依赖

依赖 `R2` 的 Go 与桥接证据、`R0` 的普通第三方 VPN 假设、目标 SDK 和具名真机。

### R3 停止或回退条件

若 SDK、权限策略或具名真机证明普通第三方应用无法建立所需数据通路，只否决该具名目标组合的普通第三方应用路线，并触发 T0 讨论；不得扩大为所有 HarmonyOS 或 OpenHarmony 目标均不可行。若仅 Go/WireGuard 失败，则回退到已通过的 native 数据泵定位。

## R4：固定测试网络的 NetBird 端到端

### R4 目标

在固定版本、可重放的真实测试网络中证明 NetBird 控制面和数据面的完整端到端行为。

### R4 关键交付物与验证

- 固定 NetBird client、management、signal、relay 和测试 peer 版本及配置，记录拓扑与制品哈希。
- 验证认证和续期、management/signal 连接、direct/relay、多 peer、routes、DNS、IPv4 及明确声明范围内的 IPv6、MTU 和双向业务流量。
- 验证断网、Wi-Fi/蜂窝切换、服务端不可达后的退避与重连，以及凭据轮换。
- 产出脱敏、可重放的配置、步骤、时间线、网络条件、日志和预期/实际结果，不包含长期凭据或节点私钥。

### R4 退出标准

固定测试网络的全部必选场景在具名真机达到 `R0` 阈值，失败场景可诊断、可恢复且证据可重放。此时可标记该目标 `0.3` 端到端门通过。

### R4 依赖

依赖 `R3` 的 VPN 数据通路、`R0` 锁定的 NetBird 版本与认证范围，以及可控的真实测试服务端和 peer。

### R4 停止或回退条件

若失败源于客户端集成，回退到 `R2` 或 `R3` 的对应边界；若源于固定服务端、协议版本或测试拓扑，保持客户端门关闭并修复测试网络。不得通过删除 `R0` 必选控制面能力获得通过结论。

## R5：最小完整产品与真机生命周期

### R5 目标

构建最小但完整的 ArkUI 产品界面和真实连接状态机，并证明授权、错误处理、系统生命周期和网络变化下的行为。

### R5 关键交付物与验证

- 提供登录或配置、连接、断开、状态、注销和诊断同意等必要 ArkUI 流程；状态来自真实状态机，不以静态界面代替。
- 对 VPN 授权拒绝、认证失败、服务不可达、配置错误和系统限制提供可行动错误及可恢复路径。
- 在具名真机验证前后台、冻结与休眠、Extension 与应用进程终止、系统回收、重启、Wi-Fi/蜂窝/飞行模式切换和配置变化。
- 验证注销后凭据、节点状态、VPN 配置、fd、路由、DNS 和日志的清理边界。
- 完成关键流程的无障碍检查，并让诊断采集由用户明确同意。

### R5 退出标准

最小完整产品在具名真机通过生命周期和网络变化矩阵，状态与实际隧道一致，拒绝和错误均可恢复，注销清理及无障碍达到 `R0` 要求。此时可标记该目标 `0.4` alpha；平台限制无法消除时必须缩减并公布支持声明。

### R5 依赖

依赖 `R4` 的真实端到端状态、`R0` 的功能与隐私范围，以及目标平台实际生命周期证据。

### R5 停止或回退条件

若平台生命周期限制使既定 SLO 或关键功能无法达到，停止扩大功能并回退 `R0` 缩减支持范围；若状态机与系统 VPN 状态无法一致恢复，则回退修复，不进入 beta。

## R6：质量、安全、性能与合规加固

### R6 目标

建立覆盖代码、ABI、桥接、网络、系统生命周期、安全、隐私、供应链和具名设备性能的分层质量证据。

### R6 关键交付物与验证

- 建立单元、集成、ABI/桥接、模糊、故障注入、端到端和具名真机测试，并维护支持矩阵。
- 执行至少 24 小时 soak、重复连接、重复切网和异常恢复，持续观测 fd、线程、内存与资源回收。
- 测量连接与重连、吞吐、时延、CPU、RSS 和能耗，并按 `R0` 阈值判定，不在本阶段临时创造性能事实。
- 复核最小权限和威胁模型。长期主凭据与节点私钥必须由具名真机验证的平台安全存储或硬件派生密钥保护，禁止明文持久化；其他持久敏感材料须加密；短期密钥只存在于受控内存并在生命周期结束时清除。
- 验证日志脱敏、诊断明确选择加入、数据最小化和保留期执行。
- 为每个制品生成并归档 SBOM、许可证与商标审查结果、漏洞扫描结果及处置记录。

### R6 退出标准

所有 `R0` 质量、安全、性能、能耗、稳定性和隐私阈值均由具名真机证据满足；24 小时以上 soak 和重复切换无未解释的 fd、线程或内存增长；每个候选制品具备完整合规与供应链记录。

### R6 依赖

依赖 `R5` 的完整状态机和生命周期、`R0` 的阈值与威胁模型，以及可执行的真机测试和制品归档环境。

### R6 停止或回退条件

出现凭据或私钥明文持久化、日志泄露、不可接受漏洞、资源持续泄漏或任何 SLO 超限时，停止晋级并回退对应实现阶段。无法由平台安全能力满足长期秘密保护要求时，停止该目标发布并重新评估目标或产品范围。

## R7：可复现构建、签名与发布工程

### R7 目标

在人工 bootstrap 完成后建立无人值守、锁定、可审计且签名职责隔离的构建与候选制品验证流程。

### R7 关键交付物与验证

- 锁定工具链、SDK、Go、NetBird、WireGuard 和应用依赖；自动校验来源允许保存的工具制品、版本和校验和。
- 在签名前生成可复现产物与 provenance，并使用隔离 runner；归档构建输入、依赖锁、SBOM、测试结果和制品哈希。
- 分离 debug、test 和 release 签名；每个目标使用独立 KMS/HSM 边界，执行最小权限、双人控制、备份、轮换和吊销演练。
- 取得市场重签后的最终制品，并在具名真机重新执行安装、升级、VPN 授权和联网回归。
- 建立发布停止和恢复流程。回滚定义为停止放量、维持服务端兼容、使用可选开关、渠道撤回和前向修复，不假设渠道或系统允许强制降级。

### R7 退出标准

人工 bootstrap 后可无人值守重建同一签名前产物；provenance、隔离 runner 和签名控制通过审查；市场重签最终制品在具名真机完成安装、升级、VPN 与联网回归；吊销和发布停止演练可执行。

### R7 依赖

依赖 `R6` 的测试与合规证据、`R0` 的签名和渠道责任，以及组织可用的隔离 runner 与 KMS/HSM。

### R7 停止或回退条件

依赖漂移、不可解释的不可复现差异、provenance 缺失、签名权限越界、密钥恢复或吊销演练失败、市场重签制品回归失败时停止发布。修复后从签名前构建和最终制品回归重新验证，不复用先前结论。

## R8：分批发布与首目标 GA

### R8 目标

以逐级放量和最终渠道制品真机证据发布首目标限定范围的 `1.0.0`。

### R8 关键交付物与验证

- 按 dogfood、closed beta、RC、渠道最终制品真机回归、目标限定 GA 的顺序推进。
- 每级使用 `R0` 阈值、错误预算、支持矩阵和退出标准判定，不以参与人数或日期单独判定。
- 在放量前演练事故识别、停止放量、可选开关、渠道撤回、沟通和前向修复。
- 发布说明明确首目标发行版、具名设备、完整系统版本、架构、渠道、功能范围、限制和已知问题。

### R8 退出标准

RC 和渠道最终制品在支持矩阵内的具名真机通过回归，分批阶段均满足阈值且事故停止演练完成，首目标以明确目标与支持矩阵发布独立 `1.0.0`。该版本不得暗示第二目标已经支持。

### R8 依赖

依赖 `R7` 的最终制品与发布工程、`R6` 的质量阈值，以及 dogfood、closed beta 和渠道回归资源。

### R8 停止或回退条件

任一级错误预算耗尽、关键 SLO 失败、安全或隐私事件、最终制品与候选制品行为不一致时停止晋级，按 `R7` 流程停止放量或撤回并前向修复。不得以未经验证的强制降级作为恢复前提。

## R9：第二目标独立验证与发布

### R9 目标

在首目标 GA 后，以共享核心和已获得的知识为基础，对第二目标独立重跑完整证据门并发布其独立 `1.0.0`。

### R9 关键交付物与验证

- 只有在首目标已 GA，且第二目标的具名发行版、具名量产设备、完整系统版本、SDK、SysCap、架构、签名信任根、分发渠道和维护资源齐备后才启动。
- 可共享协议核心、通用测试思想、证据 schema 和工程知识，但平台应用壳、配置、签名、制品、渠道结论和设备证据保持独立。
- 对第二目标独立重跑 `R0` 至 `R8`，包括平台与范围确认、ABI/libc、Go 与桥接、普通第三方 VPN、NetBird 端到端、生命周期、性能能耗、安全、签名、市场重签、渠道和具名真机验证。
- 第二目标使用独立版本序列和支持矩阵，达到目标限定 GA 后发布自己的 `1.0.0`。

### R9 退出标准

第二目标独立满足 `R0` 至 `R8` 全部退出标准并完成限定范围 GA。第二目标 GA 后，双目标的交付阶段完成；完整路线图是否完成还须满足 `R10` 的运维闭环定义。

### R9 依赖

依赖首目标 GA，以及第二目标全部具名条件和长期维护资源就绪。首目标证据只可复用方法和共享核心结果，不可替代第二目标平台、设备、签名或渠道证据。

### R9 停止或回退条件

任一启动条件缺失时不启动第二实现流。第二目标任一风险门失败时按其独立 `R0` 至 `R8` 规则停止、回退或重新进行 T0 讨论，不降低首目标已发布支持范围，也不把首目标结论直接移植到第二目标。

## R10：发布前准备与发布后持续运维

### R10 目标

在每个目标发布前建立运维能力，并在发布后持续管理可靠性、安全、密钥、依赖、设备再认证、支持范围和生命周期终止。

### R10 关键交付物与验证

- 发布前明确 SLO、错误预算、监控信号、值班与升级路径，并演练事故响应、漏洞接收与披露、渠道停止和用户通知。
- 跟踪证书与密钥到期，定期验证备份、轮换和吊销；确保目标之间的签名和信任边界独立。
- 持续评估 NetBird、WireGuard、Go、目标 SDK、系统版本和设备升级，并对每次支持范围变化执行风险对应的回归。
- 对支持矩阵中的具名设备和完整系统版本定期再认证；新增设备、版本、架构或渠道必须先取得相同维度的证据。
- 维护漏洞、许可证、SBOM、日志保留和诊断同意机制，按错误预算决定停止变更或修复优先级。
- 定义 EOL、渠道下架、服务端兼容终止和用户通知流程，并保留执行证据。

### R10 退出标准

发布前运维演练通过；发布后 SLO 与错误预算、事故和漏洞披露、证书密钥、依赖升级、具名设备再认证、支持矩阵及 EOL 流程均有责任人、周期、记录和可执行闭环。持续运维本身没有一次性终点。

### R10 依赖

发布前准备从 `R0` 起随证据成熟持续推进；实际生产监测和再认证依赖对应目标 GA、渠道数据和维护资源。

### R10 停止或回退条件

错误预算耗尽、重大安全或隐私事件、密钥或证书失效、关键依赖无法维护、具名设备再认证失败或维护资源不足时，停止放量或支持扩展，并按影响采取可选开关、渠道撤回、前向修复、缩减支持矩阵或 EOL。不得保留已失去证据的支持声明。

## 阶段依赖与可并行工作摘要

主依赖链如下：

```text
N0 研究门：N0(a) inventory+oracle 已定义 + N0(b) EV-N0-EMU24-20260810-0002 reviewed-pass/pass（2026-08-10）-> 下一步不是继续实现：仅未来用户显式授权后 E3-PHYS-PREFLIGHT 第一物理动作；N0+E3 都 pass 后才新 ADJ/T0 治理定义 native N1-Nx 门；治理生效前 E8 CLOSED
Emulator 客观可执行项：E0 pass -> E1 C-only pass/official Go blocked -> E2 pass
Emulator E3：phone 公开 runtime blocked；2in1/Tablet 只在 registration-layer 前置边界 blocked
Emulator E3-E7：reviewed dependency-blocked aggregation exception，不是 pass/N/A；完整义务未免除
唯一物理例外：E3-PHYS-PREFLIGHT API23 initial 已消费；EV-E3-PHYS1API23-20260806-0001 为 reviewed-pass/blocked，build drift 后 pre-install stop，E3 未关闭且无 retry 授权；EV-E3-PHYS1REBIND7-20260806-0001 已完成三条 rebind（reviewed-pass/pass，仅 rebind）；ADJ-20260806-0003 冻结 HarmonyOS7/API26 元组并曾准备 E3-PHYS-PREFLIGHT-20260806-0002 / EV-E3-PHYS1API26-20260806-0001（never Live/unoccupied；ADJ-20260807-0001 标 superseded-unexecuted）；host reverify PASS；ADJ-20260806-0004 / EV-E3-PHYS1BUILD7-20260806-0001 单条 software.version 实测确认 HDC build 逐字匹配（reviewed-pass/pass，仅 build-confirm；API26 仍 rebind 实测）；ADJ-20260807-0001 曾编号 E3-PHYS-PREFLIGHT-20260807-0001 / EV-E3-PHYS1API26-20260807-0001，现已 Live 并 consumed-blocked（operator-aborted；禁止局部 scenario5 重放；保留）；ADJ-20260807-0002 中文完整重跑 ID E3-PHYS-PREFLIGHT-20260807-0002 / EV-E3-PHYS1API26-20260807-0002 已 Live 并 consumed-blocked（reviewed-pass/blocked；S1/S4 pass，S2/S3/S5/S6/S7 blocked；双审查 0 B/5 M）；ADJ-20260807-0003 后 host 侧已重建并登记 EV-E3-PHYS1HOST-20260808-0001（host selftest HDC_PROCESSES=0、独立审查 0 B/0 M；candidate E3-PHYS-PREFLIGHT-20260808-0001 / EV-E3-PHYS1API26-20260808-0001 已准备、freeze plan_status blocked；DryRun is_evidence=false/HDC0/integrity empty；旧 20260807 candidate INVALID-TIMELINE 不可用）；2026-08-10 用户已显式授权**新**完整白名单 campaign（AUTH-E3-PHYS1API26-20260810-0002：authorization_status=granted、device_readiness=user-attested-ready、machine_fresh_confirmation=pending、plan_status=authorized-awaiting-windows-ready-freeze；取代已消耗的 AUTH-E3-PHYS1API26-20260810-0001，新候选 pair E3-PHYS-PREFLIGHT-20260810-0001 / EV-E3-PHYS1API26-20260810-0001（旧 pair E3-PHYS-PREFLIGHT-20260808-0001 / EV-E3-PHYS1API26-20260808-0001 因 sealed blocked 已消费不得复用；audit-1/audit-2 均 hash 记录）；新增一次 host-prep hdc list targets 内存级窄例外（恰一 token、设 process-scope PHYS_1_TARGET、不输出/持久化、不扩大 runner HDC 白名单）；attempt=initial/retry N/A、任一 gate 失败停止、不重试不换 ID；Live 前允许 runner+治理变更审查后提交/推送，campaign evidence 仍不提前提交；当前 Windows signing/build host `ready_for_commit_and_ordered_preflight`（工具/签名输入齐备；尚待未提交 clean final commit、新 pair audit-1/audit-2、全新 blocked/ready freeze、confirmation/review records、新 EvidenceRoot/RawRoot、process-scope `PHYS_1_TARGET` 映射；非全部 Live-ready），按序执行：同步 trusted refs/bundle → ID 审计①（hash 记录）→ blocked confirmation freeze 静态审查 → host-prep hdc list targets 映射（一次，内存级，恰一 token，设 process-scope PHYS_1_TARGET）→ TargetBindingConfirm 机器 fresh 确认（3 条白名单命令、仓外双文件 record+sha companion）→ ready freeze draft 绑定 record hash → 独立审查 record（e3-ready-freeze-review）→ 最终 ready freeze 绑定 review → ID 审计②（hash 记录）→ selftest → 同一 ready freeze DryRun → 审查且 freeze 字节不变 → 单次 Live；用户就绪声明不替代机器 fresh confirmation；blocked/fail/invalid/no seal 后不得自动重跑（当前 AUTH 固定 initial，任何 retry 须新治理）），E8 CLOSED
E8：当前 CLOSED；物理预检缺 reviewed-pass/pass，且还须当前R0正式基线（现v0.76.3）E1 pass、哈希一致及独立聚合审查
E8 OPEN 后：只许可具名物理投入；R2/R3 承接 E4-E7 完整 VPN/数据面义务
首目标正式门：R0 -> R1 -> R2 -> R3 -> R4 -> R5 -> R6 -> R7 -> R8
第二目标：首目标 GA + 第二目标启动条件 -> 独立重跑 R0 -> ... -> R8 -> R9 退出
运维：R0 起准备，随每个目标 GA 进入持续 R10 闭环
```

首目标 `R0` 至 `R8` 是串行风险门。`R9` 不得在首目标 GA 或第二目标启动条件未满足时提前形成第二实现流。

在不改变主依赖链的前提下，可以并行开展以下工作：

- 许可证、商标、威胁模型、隐私和漏洞响应研究。
- 目标设备、完整系统版本、SDK/SysCap、签名信任根和渠道信息收集。
- 证据与脱敏 schema、测试矩阵、故障注入方案和支持矩阵模板设计。
- 人工 bootstrap 后的构建校验、制品归档、隔离 runner、provenance 和密钥流程准备。
- 固定 NetBird 测试网络、监控、SLO、事故响应和 EOL 流程准备。

这些并行项只能提供当前或后续风险门的输入，不得生成竞争应用壳、替代主执行者决策或形成未经当前阶段验证的产品支持声明。

## 完成定义

单个目标发布 `1.0.0` 只表示该目标在其明确支持矩阵内完成限定 GA。首目标 GA 不代表双目标完成，第二目标完成实现但未 GA 也不代表双目标完成。

只有第二目标完成独立 `R0` 至 `R8` 并 GA，且两个目标均已建立并实际执行 `R10` 所定义的运维闭环，才算完整双目标路线图完成。此后持续运维、再认证和 EOL 仍按 `R10` 延续。完成状态及其版本、依赖和退出标准的后续变更，适用“动态调整机制”。
