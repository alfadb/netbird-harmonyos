# E3-PHYS-PREFLIGHT 物理设备预检计划与证据模板

最后核验：2026-07-18

本文定义 `E3-PHYS-PREFLIGHT`，即 E8 `OPEN` 前唯一允许的物理设备执行例外。它只验证一个冻结的 HarmonyOS 6.1 arm64 具名设备目标上的 E3 可达性，不是产品测试、R 阶段退出或 E4-E7 完整验证。预检记录同时达到 `record_status: reviewed-pass` 和 `verdict: pass` 是 E8 `OPEN` 的必要但非充分条件；预检为 `blocked`、`fail` 或 `invalid` 时 E8 必须保持 `CLOSED`，预检通过也不自动开放 E8。

## 当前状态

`plan_status: blocked`，不得执行 campaign；尚未分配证据 ID，也没有形成 `record_status` 或 `verdict`。2026-07-18，唯一允许的一次最小只读设备元组发现已执行且六条白名单设备 `shell` 均成功：distribution 为 `HarmonyOS`；model 为 `PLA-AL10`；完整 software/build string 为 `PLA-AL10 6.1.0.117(SP6C00E115R7P7)`；API 为 `23`；kernel arch 为 `aarch64`；app ABI 为 `arm64-v8a`。真实 HDC endpoint/target 继续只在仓外受控映射为 `PHYS-1`，不得写入仓库、普通证据或日志。

已在新隔离目录 `spikes/e3-vpn-extension-physical-preflight-hap/` 完成 API 23 受限适配并本地构建 unsigned A/B HAP；历史 `spikes/e3-vpn-extension-hap`、其现有 HAP 和 raw evidence 均未修改。A/B bundle 分别为 `cn.alfadb.netbird.e3physvpna` 与 `cn.alfadb.netbird.e3physvpnb`；构建使用 `6.1.1(24)`，`targetSdkVersion` 和 `compatibleSdkVersion` 均为 `6.1.0(23)`。完整本地审计已通过：仅请求 `ohos.permission.INTERNET`，VPN Extension 为非导出 `type: vpn`，没有 Go、NetBird、WireGuard、`protect`、特权能力或外部 endpoint。唯一 native payload 是 arm64-v8a 纯 C `libfdprobe.so`，只以 `fcntl(F_GETFD)` 对 fd 进行只读快照检查，不执行 `close`、`dup`、`read` 或 `write`；平台 `VpnConnection.destroy` 是唯一关闭责任。

当前 unsigned HAP SHA-256 只记录本地准备快照，非 campaign 冻结制品：A 为 `5712541de9095e6eb99cfd2d72582b150adf2d78a14cc23375d887b298ece7ed`，B 为 `9c4ae9206b8ac6843f4317645a2ebdb656610575c0220c58c6091a23e16687c0`。unsigned 是预期构建警告；Stable SDK 当前不验证 NAPI 模块声明，虽已提供 `.d.ts` 且编译通过，真机 native 加载尚未验证。后续重建或签名必会改变 HAP，因此这些 SHA-256 不得用于签名制品、campaign 输入或最终 hash 的冻结。

设备、系统、API 与架构、HDC 映射输入现已冻结并满足；任一已冻结输入漂移必须停止，不得继续或重新冻结后继续。除这六条白名单发现外，未执行其他设备 `shell`、`install`、`send`、`start` 或 `stop`，campaign 未开始。当前仍缺普通开发签名与设备 profile、已签名 A/B HAP、冻结的源码/SDK/final hash、清理基线、采集准备、独立审查准备及 campaign 输入。任一项缺失时，本预检计划保持 `blocked`；只有全部剩余输入门满足、正式证据 ID 分配并开始 campaign 后，才使用证据 schema 的记录状态与判定。

## 唯一例外边界

- 例外名只有 `E3-PHYS-PREFLIGHT`，只准进行一个 campaign，并绑定一台在执行前具名且冻结目标元组的 HarmonyOS 6.1 arm64 物理设备。相近型号、第二台设备、其他 build、API、架构或签名 profile 不得复用本例外。
- 只准复用或最小适配 `spikes/e3-vpn-extension-hap` 的普通第三方 A/B 公共 `VpnExtension` 探针。实现只能使用 ArkTS 和必要的纯 C；不得加入 Go、NetBird、WireGuard、私有 fork 或产品代码。
- 只准使用公开 `vpnExtension`、`VpnExtensionAbility` 和 `VpnConnection` API，以及普通开发签名。禁止 `MANAGE_VPN`、system/debug/enterprise 权限、root、隐藏服务、权限授予命令、策略修改或设备类型伪装。
- 只验证 allow、deny、`onCreate`、`VpnConnection.create` 返回 fd、active stop、Settings revoke、第二 VPN 冲突和最终清理。不验证 `protect`、流量、路由/DNS 正确性、Go/NetBird 数据面、产品生命周期或发布能力。
- 本例外不能扩展为一般真机许可。除本计划列出的单次预检外，E8 `OPEN` 前的 ABI、Go、NetBird、E4-E7、网络、性能、能耗、长稳、渠道和产品测试仍禁止在物理设备执行。E4-E7 的完整义务未被免除；因 Emulator 授权前置缺失，它们移交到 E8 `OPEN` 后该具名物理设备的 R2/R3 门执行。

## 执行前输入门

### 最小只读设备元组发现边界

最小只读发现已完成一次，已确定并冻结发行版、型号、完整 build、API、kernel arch 和 ABI；不得重复或扩展。发现不是 campaign，不得分配 evidence ID、形成证据记录或改变 `plan_status: blocked`。真实 endpoint 和 HDC target 仅在仓外受控映射；仓内只可使用 `PHYS-1`。以下六项保留为已执行白名单的可审计记录，设备端命令只能通过仓外变量 `PHYS_1_TARGET` 使用：

```sh
$HDC -t "$PHYS_1_TARGET" shell param get const.product.os.dist.name
$HDC -t "$PHYS_1_TARGET" shell param get const.product.model
$HDC -t "$PHYS_1_TARGET" shell param get const.product.software.version
$HDC -t "$PHYS_1_TARGET" shell param get const.ohos.apiversion
$HDC -t "$PHYS_1_TARGET" shell uname -m
$HDC -t "$PHYS_1_TARGET" shell param get const.product.cpu.abilist
```

这六项只用于冻结发行版、型号、build、API、kernel arch 和 ABI，不能用于识别个人、扩展 campaign 或替代任何其他输入门。禁止 `param dump`、`uname -a`、`ohos.boot.sn`、`const.ohos.serial`、`bm get -u`、`bm dump -a`、`bm dump -d`、`hidumper`，以及任何序列号、UDID、应用清单或全量状态读取。除已完成的上述六项外，不得执行任何其他设备 `shell` 命令。

设备、系统、API 与架构、HDC 已冻结；其余输入必须在安装前一次性冻结。秘密和本机 HDC 标识保存在仓库外，只在仓库证据中使用稳定别名。

| 输入 | 必须值与证据 |
| --- | --- |
| 设备 | 已冻结：`PLA-AL10` |
| 系统 | 已冻结：distribution `HarmonyOS`；完整 build `PLA-AL10 6.1.0.117(SP6C00E115R7P7)` |
| API 与架构 | 已冻结：API `23`；kernel arch `aarch64`；app ABI `arm64-v8a` |
| HDC | 已冻结：唯一 target 在仓外受控映射中绑定为 `PHYS-1`；真实 endpoint、序列号、USB 标识或网络地址不得入库 |
| 签名 | 普通开发签名；设备已纳入对应 profile；只记录非秘密的签名类型和必要公开指纹 |
| A/B 制品 | 已签名的两个独立普通 bundle HAP、最终 SHA-256、签名验证结果和包内容清单；现有 unsigned 快照不是冻结制品 |
| 源码与 SDK | 冻结的适配源码归档、source manifest、SDK/API 输入及其版本和 SHA-256；当前本地源码/SDK/final hash 尚未冻结 |
| 清理基线 | A/B 均未安装、无 A/B 进程、无任何活动 VPN、无暂存文件；记录其他已安装 VPN，确保其不参与 A/B 冲突场景 |
| 采集准备 | 原始 HiLog、完整 transcript、截图、布局/状态记录、fault list 和 hash manifest 的受控存储已就绪 |
| 审查 | 执行人与独立审查角色已分离，判定标准在执行前冻结 |
| Campaign | campaign ID、初次执行与唯一允许重试的编号规则、场景顺序，以及每场景固定 60 秒窗口的决定性起点已冻结 |
| Settings re-allow | 执行前冻结预期路径：系统直接激活 A，或再次出现普通系统授权 UI；现场不得在两者间事后改判 |

输入门只允许做必要的本地构建和签名验证；最小设备发现已完成，不得重复。任一已冻结的设备、系统、API 与架构、HDC 映射，或后续冻结的源码、SDK、签名 profile 或 HAP hash 发生变化都必须停止，且须取得新的路线决策；不能在运行中替换或仅重新冻结后继续。

## Campaign 与重试纪律

一次 campaign 是在同一冻结输入元组和一个 campaign ID 下，从清理基线开始、按固定顺序执行全部场景、完成最终清理并提交独立审查的完整活动。首次安装是 campaign 的设备端执行起点；预检授权不允许拆成多次选择性运行，也不允许把不同尝试的正面子结果拼接成 `pass`。

只有初次执行因纯基础设施原因得到 overall `blocked` 时，才可在相同冻结元组下进行一次记录在案的完整重试。基础设施原因仅包括 HDC/USB 中断、采集存储故障或与被测 VPN 行为无关的 runner/宿主故障；重试必须使用同一 campaign ID、独立 attempt 编号、重新确认清理基线，并保留首次 blocked 材料。功能 `fail`、`invalid`、非基础设施 blocked、任一输入或预期变化，以及唯一重试仍 blocked 时，都不得再次执行；继续工作必须先取得新的路线决策。

## 现有探针适用性与 API 23 隔离适配

2026-07-18 对历史 `spikes/e3-vpn-extension-hap` 的源码、构建配置和现存 HAP 完成只读核查；它仍固定 HarmonyOS `6.1.1(24)`、API 24 和 `phone`，A/B bundle 为 `cn.alfadb.netbird.e3vpna` 与 `cn.alfadb.netbird.e3vpnb`。历史源码只含 ArkTS 与资源，manifest 仅请求 `ohos.permission.INTERNET`，Extension 为非导出普通 `type: vpn`，且没有 Go、NetBird、native library、`MANAGE_VPN` 或签名配置。历史实现只调用公开 start/stop 并记录 `onCreate`/`onDestroy`，没有创建 `VpnConnection`，故不能覆盖 fd、active stop、Settings revoke 或真实冲突结果。历史 A/B HAP SHA-256 分别为 `6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c` 和 `c1d57d2544a93e4c4f172ee3ecb6ff2659adb7650558957e5b0cfb7aa69ae21e`；它们均为 `isSigned:false` 的 unsigned debug 研究制品，包内无 `libs/` 或 `.so`，不表示可安装到 arm64 真机。

适用性结论已落实为独立的 API 23 适配，而非改写历史树：`spikes/e3-vpn-extension-physical-preflight-hap/` 使用 `6.1.1(24)` 构建、target/compatible `6.1.0(23)`，A/B 为 `cn.alfadb.netbird.e3physvpna`/`cn.alfadb.netbird.e3physvpnb`。适配通过公开 API 最小化观测 `VpnConnection.create` 和 fd 生命周期；完整审计确认它仅有 `INTERNET`、非导出 `type: vpn`，无 Go、NetBird、WireGuard、`protect`、特权能力或外部 endpoint。唯一 arm64-v8a 纯 C `libfdprobe.so` 只用 `fcntl(F_GETFD)` 读取 fd 状态；它不关闭、复制、读取或写入 fd，`VpnConnection.destroy` 保持唯一 close 责任。历史 Emulator HAP、源码归档及 raw evidence 不得覆盖、改写或作为本适配的 campaign 输入。

## 受限适配要求

- 在 `VpnExtensionAbility.onCreate` 中用 Extension context 创建 `VpnConnection`，提交最小、确定且不承载业务流量的配置，并记录 `create` resolve/reject、错误码和返回 fd。
- fd 只用于证明公开 API 返回了有效描述符；不得交给 Go、NetBird、WireGuard 或产品模块。唯一 native `libfdprobe.so` 只能以 `fcntl(F_GETFD)` 采集只读快照，严禁 `close`、`dup`、`read` 或 `write`；`VpnConnection.destroy` 是唯一关闭责任，必须防止重复关闭并记录异常清理。
- start、stop、create、destroy、`onCreate` 和 `onDestroy` 都使用可关联 request ID 的 HiLog marker。A/B 保持独立普通 bundle，禁止共享身份制造冲突结果。
- 不加入 `protect`、外部 endpoint、数据泵、后台服务、TestRunner 或自动授权。系统授权、Settings 撤销和冲突操作必须由普通用户可见 UI 完成并截图。
- 物理设备 runner 必须新建且与 Emulator 历史 runner 分离；不得修改历史 raw evidence 或把 Emulator 判定重写为真机结果。

## 场景与通过条件

每个场景都必须在执行前冻结预期、决定性动作和独立的 60 秒有界观察窗口。窗口从该场景的决定性动作开始：场景 1 为首次清理基线查询，场景 2 为选择 allow，场景 3 为发出 stop，场景 4 为选择 deny，场景 5 为发起冻结路径的 re-allow，场景 6 为从 B 发起 start，场景 7 为发出最终 stop/destroy。决定性动作前的基线和 UI 截图仍须保留，但不计入观察窗；超时不得继续等待并把迟到结果计入通过。窗口内无法满足下述明确判据时记 `blocked`，并保留全部原始材料。

1. **清理基线**：确认 A/B 未安装、无 A/B 进程、无任何活动 VPN 或暂存文件，且其他 VPN 不参与场景，然后安装冻结的已签名 A/B HAP；全部确认和安装结果须在该场景 60 秒窗口内完成。
2. **Allow 与 fd**：从 A 的普通 Entry UI 发起 start，在系统授权 UI 选择 allow；窗口内必须观察授权 UI、A 的 `onCreate`、`VpnConnection.create` resolve 和有效 fd。
3. **Active stop**：A 保持活动时从普通 UI stop；窗口内必须观察 stop settlement、连接/fd 清理和 `onDestroy`，且系统不再显示 A 为活动 VPN。
4. **Deny**：以新鲜 B 授权请求在系统 UI 选择 deny。只有以下任一结果可判为 `pass`：窗口内出现可观察的 reject/error；或保留明确的系统拒绝截图，并且从拒绝动作开始的完整 60 秒窗口内 B 没有 `onCreate`、没有 `VpnConnection.create`。若 B 成功 create 或成为活动 VPN，则为 `fail`；缺拒绝截图、观察窗口不完整或既无可观察 reject/error 又不能满足“拒绝截图 + 60 秒无 onCreate/create”时为 `blocked`。
5. **Settings revoke**：按冻结的 re-allow 预期重新激活 A 并取得 fd；系统可直接激活 A，也可再次显示普通系统授权 UI，但实际路径必须与执行前冻结值一致，不一致时本场景为 `blocked`。随后从普通 Settings VPN 管理入口撤销，并在窗口内观察 A 的撤销/销毁结果和资源清理；Settings 中无普通 VPN 管理入口时本场景为 `blocked`，不能用 force-stop 或 uninstall 冒充撤权。
6. **第二 VPN 冲突**：再次激活 A，再从 B 发起 start；窗口内记录系统可见冲突、拒绝或替换语义，必须证明系统没有同时保留两个活动 VPN，并分别关联 A/B 生命周期与 create 结果。
7. **最终清理**：先通过普通 stop/destroy 清除活动连接；卸载前在仍存活的探针进程内采集 fd 快照，证明探针所拥有的 VPN fd 已关闭。随后卸载 A/B、删除暂存材料，并在窗口内确认无 A/B bundle、无 A/B 进程、无活动 VPN 和无测试配置残留。不得声称在进程已随卸载消失后直接查询其 fd。

逐场景聚合规则固定为：任一场景 `fail`，overall 为 `fail`；无 `fail` 但至少一个场景 `blocked`，overall 为 `blocked`；所有场景均为 `pass`，overall 才为 `pass`。证据污染、hash 不一致、场景顺序破坏或跨 attempt 拼接使 overall 为 `invalid`。逐场景结果和 overall 结果都可供独立审查引用，但逐场景 `pass`、overall `pass` 或 `reviewed-pass` 均不得升格为 E4-E7、R 阶段、VPN 数据面或产品通过结论。

## 判定影响

- 只有 `record_status: reviewed-pass` 与 `verdict: pass` 同时成立，才满足 E8 `OPEN` 的预检必要条件；它只表示冻结的 `PHYS-1` 目标元组上 E3 可达，不关闭 E1，不启动或完成 E4-E7，不证明 `protect`、流量、Go、NetBird、产品或发布能力，也不自动把 E8 置为 `OPEN`。
- `fail` 只否定该具名设备、完整 build、API、arm64、签名 profile、A/B 源码/SDK/HAP 组合上的预检路径，不得外推其他设备、build、API、架构、发行版或 Emulator；E8 必须保持 `CLOSED`，继续执行须先取得新路线决策。
- `blocked` 和 `invalid` 不形成正面或负面平台结论，均使 E8 保持 `CLOSED`，也不得用第二台设备绕过；除同一冻结元组上的一次基础设施性 blocked 重试外，继续执行必须先取得新的路线决策。
- 后续 E8 独立聚合审查只能引用本记录的精确 E3 可达性结论；还必须单独核验当前R0正式基线（现v0.74.7）的 E1 官方 Go loader/runtime、全部目标元组与哈希、Emulator blocked-exception 成员及其他聚合条件，再显式决定是否 `OPEN`。E8 `OPEN` 只许可后续具名物理设备投入，不表示 VPN 或数据面已通过。

## 原始证据要求

每个场景必须保留未筛选原始 HiLog、完整命令 transcript、系统授权/Settings/冲突/结果截图、必要布局或状态快照、fault list、开始/结束时间和 SHA-256 manifest。证据还必须绑定完整目标元组、稳定设备别名、源码归档、source manifest、SDK、签名验证结果、A/B HAP 和 runner hash。

日志进入仓库或普通证据存储前必须扫描秘密。HDC target、序列号、USB 标识、网络地址、签名私钥、证书口令、profile 内部设备标识、账号和 token 不得入库。独立审查必须核对原始材料完整性、场景顺序、hash、普通权限边界、最终清理和失败范围后，才能把记录从 `collected` 改为 `reviewed-pass` 或 `reviewed-fail`。

## 专用证据模板

设备元组虽已冻结，仍不得在签名、制品、源码/SDK/hash、清理、采集、审查和 campaign 输入齐备前分配正式 target code 或证据 ID。

```yaml
evidence_id: EV-E3-<R0_NAMED_TARGET_CODE>-<YYYYMMDD>-<sequence>
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: draft | collected | reviewed-pass | reviewed-fail | blocked | invalidated | superseded
stage_or_gate: E3
related_stages_or_gates: [E8]
target_tuple:
  distribution: HarmonyOS 6.1
  device_model: <exact-model>
  device_alias: PHYS-1
  full_system_build: <exact-build>
  api: <exact-api>
  architecture: arm64
  sdk_api_syscap: <sdk-version-and-public-vpn-syscap-basis>
  channel: ordinary-development-signing-only
hdc_target_reference: <out-of-repo-controlled-reference>
signing:
  type: ordinary-development
  device_in_profile: true
  public_fingerprint: <non-secret-value-or-N/A-with-reason>
code_sha: <full-repository-sha>
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime allowed
source_archive_sha256: <sha256>
source_manifest_sha256: <sha256>
sdk_sha256: <sha256-map>
runner_sha256: <sha256>
artifact_sha256:
  hap_a: <sha256>
  hap_b: <sha256>
preflight_inputs_frozen_at: <ISO-8601-with-zone>
campaign_id: <single-authorized-campaign-id>
attempt: initial | infrastructure-blocked-retry-1
retry_basis: <N/A-for-initial-or-prior-blocked-record-and-infrastructure-reason>
scenario_window_seconds: 60
settings_reallow_expected_path: direct-system-activation | system-reauthorization-UI
cleanup_baseline: <A/B-absent-no-A/B-process-no-active-VPN-other-VPN-isolated-and-staging-state>
scenarios:
  scenario_1_cleanup_and_install:
    cleanup_and_install: pass | fail | blocked
  scenario_2_allow_and_fd:
    overall: pass | fail | blocked
    assertions:
      allow: pass | fail | blocked
      vpn_on_create: pass | fail | blocked
      vpn_connection_create_fd: pass | fail | blocked
  scenario_3_active_stop:
    active_stop: pass | fail | blocked
  scenario_4_deny:
    deny: pass | fail | blocked
  scenario_5_settings_revoke:
    settings_revoke: pass | fail | blocked
  scenario_6_second_vpn_conflict:
    second_vpn_conflict: pass | fail | blocked
  scenario_7_final_cleanup:
    final_cleanup: pass | fail | blocked
scenario_aggregation:
  mapping: "1=cleanup_and_install; 2=allow_and_fd; 3=active_stop; 4=deny; 5=settings_revoke; 6=second_vpn_conflict; 7=final_cleanup"
  scenario_2_rule: "overall is pass only when allow, vpn_on_create, and vpn_connection_create_fd are all pass; fail dominates blocked"
  overall_rule: "any scenario fail => fail; else any scenario blocked => blocked; all seven scenarios pass => pass; evidence integrity violation => invalid"
  overall: pass | fail | blocked | invalid
started_at: <ISO-8601-with-zone>
ended_at: <ISO-8601-with-zone>
clock_source: <host-and-device-clock-sources>
raw_hilog_reference: <immutable-reference-and-sha256>
transcript_reference: <immutable-reference-and-sha256>
screenshot_reference: <manifest-reference-and-sha256>
layout_state_reference: <manifest-reference-and-sha256>
fault_reference: <immutable-reference-and-sha256>
hash_manifest_reference: <immutable-reference-and-sha256>
forbidden_capabilities_audit: <no-Go-NetBird-private-fork-MANAGE_VPN-or-privileged-bypass>
actual: <bounded-observation-summary>
verdict: pass | fail | blocked | invalid
scope_statement: <exact-target-only-no-extrapolation>
cleanup_result: <pre-uninstall-in-process-fd-snapshot-and-post-uninstall-no-bundle-process-active-VPN-result>
reviewer: <independent-role-or-pending>
reviewed_at: <ISO-8601-with-zone-or-pending>
review_record: <id-or-pending>
```
