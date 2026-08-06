# R0 任务章程

最后核验：2026-08-06（仅表示文档状态核验）

本文是 `netbird-harmonyos` 的 R0 唯一决策源。路线图继续定义阶段顺序和门语义；涉及 R0 状态、首目标候选、版本、范围、阈值、责任、例外和未满足项时，以本文为准。

## 状态

| 项目 | 当前状态 |
| --- | --- |
| R0 状态 | 进行中 |
| R0 退出 | 未退出 |
| 首目标 | 候选为 HarmonyOS API 24；唯一 `E3-PHYS-PREFLIGHT` 的精确 API 23 设备元组、普通开发签名/profile、FINAL A/B HAP、源码/SDK/hash、runner、清理/采集/审查和 campaign 复合输入已冻结，`plan_status: ready`；这不等于完成 R0 全部支持维度或目标锁定，campaign 尚未开始且无 evidence record/verdict |
| 普通第三方 VPN 路径 | 尚未验证 |
| API 24 x86_64 phone Emulator 总门 | `CLOSED`；E0、E1-C、E2 已完成；E1 loader 负面只绑定 v0.74.6，当前R0正式基线（现v0.74.7）尚未重跑且无 pass；E3 0003/0004 证明授权前置组件缺失，精确 phone 目标不可执行/`blocked`；E4-E7 为 reviewed dependency-blocked aggregation exception |
| API 24 x86_64 2in1 Emulator 矩阵记录 | 0001/0002 保持 `reviewed-pass/blocked`；只在 registration-layer 前置边界确认授权组件缺失并停止，未安装 HAP，不形成完整 runtime 不可执行结论，也不替代其他形态 |
| API 24 x86_64 Tablet Emulator 矩阵记录 | 0001 保持 `reviewed-pass/blocked`；只在 registration-layer 前置边界确认授权组件缺失并停止，未安装 HAP，不形成完整 runtime 不可执行结论，也不替代其他形态 |
| 物理设备执行 | E8 前只允许一个 `E3-PHYS-PREFLIGHT` campaign。六条 discovery 已完成；唯一 signing enrollment 命令由授权用户执行一次且例外已消耗，UDID 未留存/回传。FINAL signed HAP/profile/cert、源码/SDK、runner、采集/审查与 campaign 输入已冻结，计划为 `ready`。截至 2026-08-06 campaign 从未 install/run/start/stop；planned evidence ID 尚未正式占用，没有 record status 或 verdict，E8 保持 `CLOSED`，NetBird 与其他物理执行仍禁止 |

R0 未退出意味着任何研究结果都不能表述为产品可行性、真机支持或发布承诺，也不能据此跳过后续证据门。

## 当前技术基线

除明确标记为尚未验证的项目外，下表是当前执行使用的固定基线；基线变化必须进入动态调整记录，并按路线图“动态调整机制”处理人类直接决定或 T0 触发。

| 项目 | R0 决定 | 信息状态 |
| --- | --- | --- |
| 首目标候选 | HarmonyOS API 24 | 方案建议；待具名真机确认 |
| 稳定构建链路 | DevEco Studio 6.1.1.290（Build 243.24978.46.36.611290），SDK 6.1.1.125 / API 24，target/compatible API 23，HDC 3.2.0d | 当前实测并作为唯一预检复合输入冻结 |
| Emulator 链路 | Beta Command Line Tools 26.0.0.461，HDC 3.2.0e | 当前实测 |
| Emulator 镜像 | `HarmonyOS 6.1.1(24)`，API 24；phone `phone_all_x86`、独立 2in1 `pc_all_x86` 与独立 Tablet `tablet_x86` image | 当前实测；三种设备形态不互相替代 |
| NetBird | 正式采用 `v0.74.7`，commit `a1c9427d8004576e2cbb9e546d409847fa9df318` | 当前R0正式基线（现v0.74.7）；所有 v0.74.6 历史 evidence 保持原绑定 |
| NetBird Go 声明 | `go 1.25.5`、`toolchain go1.25.12`；官方 Release run [`29587548629`](https://github.com/netbirdio/netbird/actions/runs/29587548629) 成功 | 官方 Actions URL，访问日期 2026-07-18 |
| 当前 Go 工具链 | 1.25.12 | 与 NetBird 声明一致；目标 ABI 构建尚未验证 |
| 测试服务端 | 自托管 management、signal、relay，均采用 NetBird v0.74.7 | 方案建议；部署与兼容性尚未验证 |
| 初始认证 | 测试网络专用 setup key | 方案建议；不得在仓库或证据中记录 key 值 |
| GA 渠道 | 华为应用市场 | R0 固定；账号、签名、审核、市场重签和最终制品闭环尚未验证 |

自 2026-07-18 起，当前R0正式基线（现v0.74.7）固定为 NetBird commit
`a1c9427d8004576e2cbb9e546d409847fa9df318`。正式非 prerelease release、tag/commit、
`go 1.25.5`、`toolchain go1.25.12` 和成功 Release run
[`29587548629`](https://github.com/netbirdio/netbird/actions/runs/29587548629) 已核对；
该固定 GitHub Actions URL 的访问日期为 2026-07-18。
[Tailscale-OHOS 审计](tailscale-ohos-netbird-port-audit.md)已固定检查相对 v0.74.6
前进的 7 个 commit，并核对 wireguard-go replace 从 `2834bebf...` 更新到
`8ec1ad32...`。所有既有 E/R evidence 继续绑定其实际使用的 v0.74.6、commit
`3a2f773d655d88d16ed953fc2a114a4e690a1b08` 和当时制品，不重写、不重判。采用
当前R0正式基线（现v0.74.7）不会使历史记录自动失效或通过；后续运行只使用
v0.74.7，并按动态调整机制重跑受影响门。

设备、华为账号、签名材料、渠道权限和测试网络等外部资源由用户本人负责准备和控制。仓库文档、源码、测试数据、日志和证据体系不得记录任何账号凭据、setup key、token、私钥、Cookie、恢复码或可复用临时下载地址。

Go 1.26.5 不属于当前R0正式基线（现v0.74.7），PS4 尚未发布；未发布的 Go 分支、patch set、提案或实验候选不得作为当前 E0-E8 或正式 R 门输入。每次采用新 NetBird 正式 release 前必须重新记录 tag、commit、`go` directive、`toolchain` directive 和官方 release run 结果，并按动态调整机制更新受影响门。

## Emulator 投入总门与唯一物理预检例外

API 24 x86_64 phone Emulator 上所有客观可执行项仍须优先完成。Emulator 上的 PASS、FAIL 与 blocked 都只覆盖记录的 x86_64 目标元组，不得外推到 arm64 或物理设备。除下述一个例外外，E8 `OPEN` 前仍禁止物理设备上的 HDC 设备操作、ABI、Go、NetBird、E4-E7、网络、性能、能耗、长稳、渠道和产品测试。

[E3 API 24 Emulator 矩阵审查](evidence/e3-vpn-extension-api24-emulator-matrix-2026-07-17.md)保持所有历史记录和 raw evidence 原样。phone 记录在公开 runtime 与注册前置边界均为 `blocked`；2in1、Tablet 只在 registration-layer 前置边界确认授权组件缺失并按停止条件未安装 HAP，不能扩写为完整 runtime 不可执行结论。三种记录均不外推其他 image、build、架构、物理设备或发行版。E3 与因其前置缺失未启动的 E4-E7 在 E8 中统一登记为 reviewed dependency-blocked aggregation exception，不是 `pass` 或 `N/A`。

E4-E7 的完整义务没有免除；它们移交到 E8 `OPEN` 后同一具名物理设备的 R2/R3 门执行。E8 `OPEN` 只是物理设备投入许可，不表示 VPN、TUN、`protect` 或数据面通过。若新官方 image/build 包含所需授权或注册组件，旧 blocked 边界不再适用于当前聚合；历史记录保留，但必须从 E3 开始重验并按可达性执行后继项。

E8 `OPEN` 的必要条件必须全部满足：所有客观可执行 Emulator 项均为 `reviewed-pass/pass`；当前R0正式基线（现v0.74.7）的 E1 官方 Go loader/runtime 形成 `reviewed-pass/pass`；全部目标元组与代码、上游、制品和输入哈希一致；`E3-PHYS-PREFLIGHT` 同时为 `record_status: reviewed-pass`、`verdict: pass`；独立聚合审查显式决定 `OPEN`。现有 loader 负面证据只绑定 v0.74.6，v0.74.7 尚未重跑且无 pass。预检复合输入虽已冻结且计划为 `ready`，campaign 仍从未执行、没有 record 或 verdict，因此 E8 继续 `CLOSED`。

### `E3-PHYS-PREFLIGHT`

E8 前只定义这一个物理设备 campaign。它只准在输入冻结后的一台具名 HarmonyOS 6.1 arm64 设备上，复用或最小适配现有普通第三方纯 ArkTS/C 公共 VPN Extension A/B 探针，验证 allow、deny、`onCreate`、`VpnConnection.create` fd、active stop、Settings revoke、第二 VPN 冲突和清理。禁止 Go、NetBird、WireGuard、私有 fork、产品代码、`MANAGE_VPN`、system/debug/enterprise、root、隐藏服务、权限授予或策略绕过。

设备型号 `PLA-AL10`、完整 build `PLA-AL10 6.1.0.117(SP6C00E115R7P7)`、API `23`、kernel `aarch64`、app ABI `arm64-v8a` 与仓外 `PHYS-1` 映射已冻结；任一漂移必须停止。隔离目录 `spikes/e3-vpn-extension-physical-preflight-hap/` 已完成 API 23 适配、普通 debug 开发签名和内容审计：仅 `INTERNET`、非导出 `type: vpn`、无 Go/NetBird/WireGuard/`protect`/特权/外部 endpoint；唯一 arm64-v8a 纯 C `libfdprobe.so` 只用 `fcntl(F_GETFD)` 读取 fd，平台 `VpnConnection.destroy` 为唯一 close 责任。FINAL A/B HAP SHA-256 为 `3a98ad68bfc6253fe10b37a262e0051267b3b3083e6943f8207d68482d00c244` / `1adfa9664e59e7a9dc3da6650a59972517f5262a9b8160f4b3f6416770da3c26`；profile 为 `a3abfc6ac351cf06f5639b31f108c80edcdcd96080f43ccfd48ce12a07325b05` / `f09af0f314773c53d61d90804332605317ec6a61316add0df3672067da99a16e`，certificate file 为 `c13847ecd674a330acb1dfb9df027eb68b21ccadd90eca6e21ebd5a515d6d7fc`。四项验证 exit `0`、人工核对 pass、内嵌 profile byte-equal。源码归档、manifest、SDK map、runner 和角色/采集输入也已冻结，完整值见专用计划。唯一 enrollment 已执行一次且例外消耗，UDID 未留存/回传。

计划 target code 为 `PHYS1API23`，campaign ID 为 `E3-PHYS-PREFLIGHT-20260806-0001`，planned evidence ID 为 `EV-E3-PHYS1API23-20260806-0001`；后者只在首次设备端动作时正式占用，跨日期未启动须重新决策。operator=`authorized user`、orchestrator=`main agent`、reviewer=`独立 deepseek/deepseek-v4-pro 隔离会话`。计划现为 `ready`，但截至 2026-08-06 从未 install/run/start/stop，没有 evidence record 或 verdict。完整计划、runner 白名单、仓外 raw 保留与证据模板见 [E3-PHYS-PREFLIGHT](e3-physical-preflight.md)，已完成 Windows 回传见 [Windows + DevEco Studio 开发交接](windows-development-handoff.md)。

只有预检同时达到 `record_status: reviewed-pass`、`verdict: pass`，才满足 E8 `OPEN` 的预检必要条件，但仍不充分且不自动开放 E8。预检 `fail`、`blocked` 或 `invalid` 均使 E8 保持 `CLOSED`；结果只决定该精确物理目标上的 E3 可达性，失败范围不得外推。一次 campaign 及唯一基础设施性 blocked 重试的纪律以专用计划为准；功能 fail、输入变化、第二台设备或其他越界继续执行都必须先取得新的路线决策。

## 必选功能

首个 0.x 实现与固定测试网络必须覆盖以下能力，不得为通过后续门而静默删除：

- 使用专用 setup key 完成节点注册，并支持注销及本地状态清理。
- 建立并维持 management 与 signal 连接。
- 验证 direct 与 relay 两种 peer 数据路径。
- 同时处理多个 peer。
- 支持 IPv4 数据流量。
- 支持 NetBird routes。
- 支持 NetBird DNS 行为。
- 正确配置并验证 MTU。
- 覆盖断网、切网和重连。
- 覆盖 setup key、token 或节点身份相关凭据的轮换流程。

IPv6 在首轮实现中必须探测并记录能力与失败边界，但不作为首个 0.x 的强制功能。若拟在 GA 中声明 IPv6 支持，必须通过动态调整另行锁定范围、目标元组、测试矩阵和 SLO，并取得对应真机与渠道最终制品证据。

## 明确排除项

首个 0.x 不包含以下能力：

- OIDC 或 SSO。
- exit node。
- system VPN 或 always-on VPN。
- 第二平台目标或第二实现流。
- 完整企业功能集。

排除项不得被解释为底层实现可以破坏未来兼容性；任何纳入决定仍须按动态调整机制评估阶段、证据和 T0 影响。

## 补丁预算

补丁是项目为使固定 NetBird、WireGuard、Go 或相关依赖在目标平台构建和运行而维护的上游偏离；纯应用壳代码和已被固定上游版本包含的变更不计入补丁数。每个补丁必须在证据体系中记录原因、范围、维护风险、上游状态、替代方案和移除条件。

| 门范围 | 累计补丁上限 |
| --- | ---: |
| R2 退出前 | 5 |
| R3 退出前 | 8 |
| R4 至 R5 退出前 | 10 |
| R6 至 R10 | 不得新增适配补丁；全项目累计上限仍为 10 |
| 全项目固定总上限 | 10 |

任一补丁被评估为高维护风险，或任一阶段累计补丁数超过对应上限，均立即触发 T0 讨论；不得通过拆分、重命名或把补丁移入构建脚本规避预算。bug fix 不自动豁免高维护风险或超预算的 T0 规则；若修复需要新增适配补丁、扩大既有补丁范围，或使累计补丁数超过当时门范围上限，仍须立即触发 T0 讨论。

## 初始 SLO

下表是 R0 的初始阈值，均待具名真机确认。真机到位后只能依据可审查证据收紧阈值；任何放宽、删除或改变统计口径的提议都必须重新进行 T0 讨论。

| 证据门 | 初始且待真机确认的阈值 |
| --- | --- |
| R3 功能证明 | 完成 10 次连接/断开且无 crash；native 数据泵与 Go/WireGuard 路径均有双向真实流量；观测期间 fd 与线程无单调增长 |
| R6 加固基线 | 完成至少 24 小时 soak、100 次连接和 50 次切网；连接成功率不低于 95%；重连成功率不低于 90% 且重连 P95 不高于 30 秒；吞吐不低于同设备、同网络条件非 VPN 基线的 70%；无明显 RSS、fd 或线程增长；隐私与安全事件零容忍 |
| R8 RC 与 GA 前回归 | RC 阶段无新增 crash；连接成功率不低于 99%；重连成功率不低于 98% 且重连 P95 不高于 20 秒；华为应用市场最终制品完成支持矩阵内完整回归；隐私与安全事件零容忍 |

连接、重连、吞吐和资源指标必须按固定目标元组、网络条件、样本数、起止时间、失败分类和统计方法记录。当前尚未具名的时延、CPU、能耗和错误预算判据仍是 R0 未满足项，不能由上述阈值推导为已锁定或已通过。

## 责任矩阵

责任只按角色记录，不在本文保存姓名、账号标识或联系方式。

| 角色 | 责任 |
| --- | --- |
| 用户 | 准备和控制具名设备、华为账号、开发与发布签名、华为应用市场渠道权限及测试网络；处理必须由授权人员完成的协议和外部操作 |
| 执行代理 | 实施工程、维护固定基线、执行测试、采集与脱敏证据、报告失败和动态调整触发条件；不保管外部账号或长期秘密 |
| 独立审查 | 审查证据完整性、复现性、信息状态、门阈值和例外边界，并作出阶段门验收意见 |

## 当前未满足项

- `E3-PHYS-PREFLIGHT` 的复合输入已冻结且计划为 `ready`，包括具名 API 23 设备元组、SDK/SysCap 依据、签名/profile、FINAL signed HAP、源码/SDK/hash、runner、清理、采集/审查和 campaign 输入；R0 的渠道及其他完整支持维度仍未锁定，不能从预检输入冻结推导 R0 退出。
- API 24 x86_64 phone Emulator 总门为 `CLOSED`：E0、E1-C、E2 已为 `reviewed-pass/pass`；现有官方 Go 1.25.12 loader 负面绑定 v0.74.6，当前R0正式基线（现v0.74.7）尚未重跑且没有 pass，E1 overall Go 未关闭。
- 官方 API 24 x86_64 phone、2in1、Tablet 的历史矩阵记录均保持 `reviewed-pass/blocked`。phone 的 blocked 覆盖所记录公开 runtime；2in1、Tablet 的“不可继续执行”只限 registration-layer 前置边界，未安装 HAP，不能扩写为完整 runtime 结论。历史 evidence 和 raw 判定不改写，范围不外推。
- E3-E7 在当前聚合中为 reviewed dependency-blocked aggregation exception，不是 `N/A` 或 pass；E4-E7 完整义务移交 E8 `OPEN` 后的具名物理设备 R2/R3 门。当前仍因 E1、预检和完整独立聚合审查均未满足而 `CLOSED`。
- 唯一 `E3-PHYS-PREFLIGHT` 的设备、签名制品、源码/SDK/hash、runner、清理/采集/审查、角色和 campaign 复合输入均已冻结，计划状态为 `ready`。六条 discovery 已执行；唯一 signing enrollment 由授权用户执行一次且例外已消耗，UDID 未留存/回传。除此之外尚未执行 campaign 的 install/run/start/stop。planned evidence ID 尚未因首次设备端动作而正式占用，没有 record status 或 verdict；若跨日期未启动必须重新决策 ID。
- 除该唯一受限预检外，E8 `OPEN` 前仍禁止物理设备执行；真机 HDC 安装、启动、日志、卸载的完整闭环尚未建立。
- 普通第三方 VPN、SysCap、虚拟接口 fd、`protect`、后台生命周期和真实流量尚无产品或阶段门证据；预检即使通过也只证明精确物理目标上的 E3 可达。
- 当前R0正式基线（现v0.74.7）已固定；所有 v0.74.6 历史 evidence 保持原版本、commit、输入 hash 和判定，后续受影响门尚未基于 v0.74.7 重跑。
- 自托管同版本 management、signal、relay 及专用 setup key 流程尚未形成可重放证据。
- 开发、测试、发布签名及华为应用市场审核、重签、最终制品获取与回归闭环尚未跑通。
- 时延、CPU、能耗及完整错误预算阈值尚未由具名真机基线锁定。
- 初始威胁模型和许可证基线已建档，但 SBOM、依赖锁定后的许可证确认、漏洞处置和真机安全存储验证尚未完成。
- R0 门退出证据尚未经过独立审查。

## R0 Checklist

- [x] 建立 R0 唯一决策源并明确“进行中/未退出”。
- [x] 固定当前工具链、Emulator、NetBird、Go、服务端、初始认证和 GA 渠道基线。
- [x] 锁定必选功能、IPv6 首轮边界和明确排除项。
- [x] 锁定分阶段补丁预算及 T0 触发条件。
- [x] 建立初始且待真机确认的 R3、R6、R8 SLO。
- [x] 建立证据 schema、初始威胁模型、许可证基线和角色责任矩阵。
- [x] 记录 Emulator 客观可执行项投入总门、双向不外推纪律和 E8 前唯一 `E3-PHYS-PREFLIGHT` 例外。
- [x] 把 API 24 x86_64 2in1 与 Tablet 补充探测分别作为独立 image/实例/端口/evidence 记录，不互相替代。
- [x] 正式采用 NetBird v0.74.7 基线，并保留所有 v0.74.6 历史 evidence 的原绑定和判定。
- [ ] 以当前R0正式基线（现v0.74.7）关闭全部客观可执行 Emulator 项，并对 E3-E7 的 reviewed dependency-blocked aggregation exception 完成 E8 独立聚合审查。
- [x] 以唯一一次最小只读发现冻结预检设备的型号、完整 build、API、kernel arch、app ABI 和仓外 HDC `PHYS-1` 映射；任一漂移停止。
- [x] 锁定普通开发签名/profile、FINAL signed A/B HAP、冻结源码/SDK/final hash、runner、清理基线、采集/审查准备、角色、campaign ID、60 秒窗口和 Settings 预测路径；`plan_status: ready` 只表示唯一 campaign 输入 ready。
- [ ] 仅按 `E3-PHYS-PREFLIGHT` 在该具名设备执行一个 campaign，并取得 `reviewed-pass/pass`；其他结果保持 E8 `CLOSED`。
- [ ] 仅在 E1、预检、哈希一致性及独立聚合审查等全部必要条件满足并由 E8 显式 `OPEN` 后，于具名物理设备完成预检范围外的 HDC 闭环和 R2/R3 E4-E7 完整义务。
- [ ] 用具名真机证据补齐并收紧全部质量、性能、能耗、稳定性、隐私和错误预算阈值。
- [ ] 验证普通第三方 VPN 路径仍可作为后续工程假设推进。
- [ ] 完成开发、测试、发布签名和华为应用市场责任所需的外部资源准备与可执行流程确认。
- [ ] 完成依赖锁定后的许可证复核、安全存储方案确认和初始供应链审查。
- [ ] 由独立审查确认全部 R0 退出条件满足。
