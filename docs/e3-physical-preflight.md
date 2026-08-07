# E3-PHYS-PREFLIGHT 物理设备预检计划与证据模板

最后核验：2026-08-07

本文定义 `E3-PHYS-PREFLIGHT`，即 E8 `OPEN` 前唯一允许的物理设备执行例外。它只验证一个冻结的 HarmonyOS 7 / API 26 arm64 具名设备目标上的 E3 可达性，不是产品测试、R 阶段退出或 E4-E7 完整验证。历史 initial live 曾以 HarmonyOS 6.1 / API 23 元组执行并因 build drift 停止，见 [`EV-E3-PHYS1API23-20260806-0001`](evidence/e3-physical-preflight-2026-08-06.md)；当前可执行目标元组以 `ADJ-20260806-0003` 冻结的 HarmonyOS 7 / API 26 为准。预检记录同时达到 `record_status: reviewed-pass` 和 `verdict: pass` 是 E8 `OPEN` 的必要但非充分条件；预检为 `blocked`、`fail` 或 `invalid` 时 E8 必须保持 `CLOSED`，预检通过也不自动开放 E8。

## 当前状态

`plan_status: blocked-awaiting-device-authorization`。此处 **不是** 设备执行授权，也 **不** 授权 auto retry、新 campaign/evidence ID 或任何 HDC/设备命令（用户显式设备授权 + fresh device confirmation 完成前）。`ADJ-20260807-0003` 已由用户直接批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径：S3/S7 优先 callback terminal + post-destroy fd snapshot，缺失时严格 fallback 到白名单 `PidOf`/`BundleDump` 连续 absent 观察（≥2 次、间隔 ≥3s；S3 需 bundle present；`FD_STILL_OPEN` 不可覆盖；S3 fallback 需 S5 fresh create 作为 clean reactivation proof，否则 overall blocked）；S5 改为先 fresh A create/open、Settings>VPN 页仅 observation（不影响结果）、随后人工 Settings>应用信息>A>强制停止并单独截图 + `SETTINGS-APP-INFO-FORCE-STOP-CAPTURED` 确认，HDC 只观察不 ForceStop（**HDC force-stop 明确 cleanup-only**：仅 finally 残留清理，Reason 限 `exception-cleanup`/`final-cleanup`）。runner/freeze example/selftest 已随执行 commit `e3fe0c642c28b8a332c0f70db2217787884334e9`（parent `c6acae7`，M1/M3 probe fixes）更新；Windows 签名/构建主机的 runner、freeze 与 selftest 快照已重建并登记为 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)（host selftest `HDC_PROCESSES=0`、独立审查 0 B/0 M；candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`；DryRun `is_evidence: false`/HDC0/integrity empty；旧 20260807 candidate `INVALID-TIMELINE` 不可用）。`ADJ-20260807-0002` 批准的中文完整 1–7 重跑已 Live 并登记为 [`EV-E3-PHYS1API26-20260807-0002`](evidence/e3-physical-preflight-api26-0002-2026-08-07.md)（`reviewed-pass/blocked`，`consumed-blocked`；S1/S4 pass，S2/S3/S5/S6/S7 blocked；cleanup `verified-clean`；双审查 0 B/5 M）。prior 0001 保留。本登记期间 **禁止 HDC**。E3 未关闭，E8 仍为 `CLOSED`。

`ADJ-20260806-0003` 曾分配的准备 ID `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001` 从未 Live、未占用，经 `ADJ-20260807-0001` 标为 `superseded-unexecuted`。`ADJ-20260807-0001` 随后编号的 `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 已 Live 并 operator-aborted 登记为 [`EV-E3-PHYS1API26-20260807-0001`](evidence/e3-physical-preflight-api26-2026-08-07.md)（`reviewed-pass/blocked`，`consumed-blocked`，保留）：operator 误确认场景 5 Settings 事实并直接关窗；recovery cleanup `verified_absent`；独立 seal 审查 0 B/M；**禁止局部 scenario5 重放**。`ADJ-20260807-0002` 批准中文完整场景 1–7 重跑（非设备行为 retry）后，该身份 `E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` 已 Live 并 `consumed-blocked`。

历史 API23 initial live 已于 2026-08-06 执行并登记为 [`EV-E3-PHYS1API23-20260806-0001`](evidence/e3-physical-preflight-2026-08-06.md)：`record_status: reviewed-pass`、`verdict: blocked`、`execution: live`、`attempt: initial`。`reviewed-pass` 只表示独立证据审查完成且为 0 blocker/0 major，不是 E3 pass。runner 在连续 capture、staging 与 install 前发现 live build 脱敏投影的可见 suffix 与冻结 build 不同，按预定输入漂移停止；`campaign_started=false`，A/B 未安装、未运行。旧 campaign/evidence 不可复用，无 `infrastructure_reason`，不授权 infra retry。

2026-07-18 的一次最小只读设备元组发现仍保留为历史完成记录，六条白名单设备 `shell` 均曾成功：distribution 为 `HarmonyOS`；model 为 `PLA-AL10`；完整 software/build string 为 `PLA-AL10 6.1.0.117(SP6C00E115R7P7)`；API 为 `23`；kernel arch 为 `aarch64`；app ABI 为 `arm64-v8a`。真实 HDC endpoint/target 继续只在仓外受控映射为 `PHYS-1`，不得写入仓库、普通证据或日志。唯一 signing enrollment 命令随后由授权用户在批准边界内执行一次，例外已经消耗；UDID 未留存、未回传，命令不得重跑。live model 复核匹配 `PLA-AL10`；live build 仅投影为 `PLA-AL10 <REDACTED_IPV4>(SP8C00E32R7P2)`，只据可见 suffix 确认 build drift，不猜完整版本或漂移原因。

`ADJ-20260806-0002` 授权的 HarmonyOS 7 最小只读 rebind discovery 已执行并登记为 [`EV-E3-PHYS1REBIND7-20260806-0001`](evidence/e3-physical-rebind7-2026-08-06.md)：`record_status: reviewed-pass`、`verdict: pass`（**严格只表示**三条 rebind 完成，不是 E3/campaign pass）；投影结果 API `26` / `aarch64` / `arm64-v8a`；Settings 人工报告 `7.0.0.100 (SP8C00E32R7P2patch09)`；HDC binding 候选 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`（旧 live 投影 + 人工值，未重查 dist/model/build）。`ADJ-20260806-0003` 已冻结该新元组，批准复用原 FINAL HAP hashes（兼容性仅作实测、不证明成功），并曾分配准备 campaign ID `E3-PHYS-PREFLIGHT-20260806-0002` / evidence ID `EV-E3-PHYS1API26-20260806-0001`（从未 Live、未占用；`ADJ-20260807-0001` 标 `superseded-unexecuted`）。host reverify 已 PASS：public manifest SHA-256 `66a70a52c92b927d4b23e528ae6eaf1b52169e504291c6ff0e7efa4c7ffee010`；FINAL HAP / signature / profile / member-list hashes 与历史登记一致、未变；**不**主张设备安装兼容性。`ADJ-20260806-0004` 授权的单条 build 确认已登记为 [`EV-E3-PHYS1BUILD7-20260806-0001`](evidence/e3-physical-build7-confirm-2026-08-06.md)：`record_status: reviewed-pass`、`verdict: pass`（**严格只表示** build-confirm，不是 E3/campaign pass）；`const.product.software.version` 精确投影 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` 与冻结 HDC binding 逐字匹配，消除合成 build 风险；API `26` 仍由前一 rebind 实测，**不**从 build 推断。`ADJ-20260807-0001` 曾将未执行 API26 campaign 跨日重新编号为 `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001`；该组 ID 已 Live 并 `consumed-blocked`（见上，保留）。`ADJ-20260807-0002` 分配的 `E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002`（同 tuple/HAP；runner 中文提示变更；prior blocked 显式绑定 0001）已 Live 并 `consumed-blocked`（见 [`EV-E3-PHYS1API26-20260807-0002`](evidence/e3-physical-preflight-api26-0002-2026-08-07.md)）。继续执行须先取得新的路线裁决；当前 **无** auto retry/新 ID/设备命令授权。

隔离目录 `spikes/e3-vpn-extension-physical-preflight-hap/` 已完成 API 23 受限适配、签名和最终输入审计；历史 `spikes/e3-vpn-extension-hap`、其现有 HAP 和 raw evidence 均未修改。A/B bundle 分别为 `cn.alfadb.netbird.e3physvpna` 与 `cn.alfadb.netbird.e3physvpnb`。登记 HAP 仍为 compile API `24`、target/compatible API `23`；设备冻结 API 为 `26`，安装/运行兼容性仅以实测为准，不得由 compile/target 值推导。冻结构建链为 DevEco Studio `6.1.1.290`（Build `243.24978.46.36.611290`）、SDK `6.1.1.125` / API `24`，target/compatible 均为 API `23`；HDC `3.2.0d`，可执行文件 SHA-256 为 `fff02abf2e61603e491e896aa6195e78db0c1779a6d7b992b89757a9a3c72116`。

FINAL signed HAP A SHA-256 为 `3a98ad68bfc6253fe10b37a262e0051267b3b3083e6943f8207d68482d00c244`、size `106210`；B 为 `1adfa9664e59e7a9dc3da6650a59972517f5262a9b8160f4b3f6416770da3c26`、size `106212`。A/B profile SHA-256 分别为 `a3abfc6ac351cf06f5639b31f108c80edcdcd96080f43ccfd48ce12a07325b05` 与 `f09af0f314773c53d61d90804332605317ec6a61316add0df3672067da99a16e`；公开证书文件 SHA-256 为 `c13847ecd674a330acb1dfb9df027eb68b21ccadd90eca6e21ebd5a515d6d7fc`。四项 `verify-profile`/`verify-app` 均 exit `0`、人工核对 `pass`，且 HAP 内嵌 profile 与对应外部 profile byte-equal。signed 内容审计确认：仅 `ohos.permission.INTERNET`；VPN Extension `exported=false`；API `23`；debug 普通开发签名；唯一 native payload 为 arm64-v8a 纯 C `libfdprobe.so`，它只用 `fcntl(F_GETFD)` 读取 fd 快照，平台 `VpnConnection.destroy` 是唯一关闭责任。A/B signed member-list SHA-256 分别为 `216acabcd1f1c0efdc2ed6fbf89b4d88a1dd064bf5d508d4f692447a9b0f0166` 与 `4177f5c11d291bb20730ff45543b2ed5fcda9b8a349dbbe568ee01c89cdc82c2`。

冻结 build-source archive SHA-256 为 `e9aa2360df2027bfbd0a84f89a926439cf7bcfb50ddbb0c4977804373fb5da36`，source manifest 为 `e5ca08160003aeb621220bf0666a7cc8f20ab2cef3241d692814798c758e1b50`，SDK map 为 `f3ed4f374f1c877c14fdce99adf6f601595de4cc9d531bded7cc111fb14130b3`。API26 0001 blocked campaign 所用 runner SHA-256 为 `19fc1a76e49b9dca66a8a0352cc6bc8291f2888e66b3ad72cdc8a91ed97312e7`（仅绑定 `EV-E3-PHYS1API26-20260807-0001`）。API26 0002 live 绑定 runner SHA-256 `2fb2d3e99585a53adec82ea3b51ae2ea29c8f021d46e24b0828faa5415d38194`、`code_sha: e8eb1b67a48603c55d3f55d2be686bae0dbd15e1`、freeze `d6334c2d8d0d1bf11a2a9e26f65039ee0a1a98e377fbf644cef557ff02c55a1a`（仅绑定 `EV-E3-PHYS1API26-20260807-0002`；freeze 目录已写 `CONSUMED-BLOCKED.txt`，JSON 不改）。host-only selftest 历史 SHA-256 `f7156314031eaf3649dfc6e5e245b9fbba75944724ed03fa62b87a465188b39d` 仅作历史参考。`ADJ-20260807-0003` 已变更 runner（host process terminal probe、`Get-VpnFinalState`、S5 强制停止路径与冻结决策字段）；后续新 freeze 必须绑定含本变更的新 commit 与新 runner SHA-256，历史 runner/freeze hash 只绑定各自记录。历史 API23 initial live 所用 runner `749be7f8dd7c561f0728e90220fa703f12ccc33e7eb7a22e30af482511e4a770` 仅保留在旧 evidence 中。Windows 准备基线为 `f44be17331e5bc67a5eff702badba41cbd7a195f`；API23 initial live freeze 已绑定 `code_sha: 82ebc400de89a9de691a8c9d1bd629c9845999e8`；API26 0001 blocked live 绑定 `code_sha: 5ef532d099fcb3f4cd42fd8daab2864b6a6779a8`。signed HAP/profile/certificate 须保留至 E8 审查结束。

旧 physical-preflight unsigned HAP hash 继续作为历史准备阶段记录保留：A `5712541de9095e6eb99cfd2d72582b150adf2d78a14cc23375d887b298ece7ed`，B `9c4ae9206b8ac6843f4317645a2ebdb656610575c0220c58c6091a23e16687c0`。它们不是当前签名制品、campaign 输入或最终 hash，不得与上述 FINAL signed HAP 混用。

## 唯一例外边界

- 例外名只有 `E3-PHYS-PREFLIGHT`，只准进行一个 campaign，并绑定一台在执行前具名且冻结目标元组的 HarmonyOS 7 / API 26 arm64 物理设备。历史 initial 曾绑定 HarmonyOS 6.1 / API 23 元组且已消费，不得复用。相近型号、第二台设备、其他 build、API、架构或签名 profile 不得复用本例外。
- 只准复用或最小适配 `spikes/e3-vpn-extension-hap` 的普通第三方 A/B 公共 `VpnExtension` 探针。实现只能使用 ArkTS 和必要的纯 C；不得加入 Go、NetBird、WireGuard、私有 fork 或产品代码。
- 只准使用公开 `vpnExtension`、`VpnExtensionAbility` 和 `VpnConnection` API，以及普通开发签名。禁止 `MANAGE_VPN`、system/debug/enterprise 权限、root、隐藏服务、权限授予命令、策略修改或设备类型伪装。
- 只验证 allow、deny、`onCreate`、`VpnConnection.create` 返回 fd、active stop、Settings revoke、第二 VPN 冲突和最终清理。不验证 `protect`、流量、路由/DNS 正确性、Go/NetBird 数据面、产品生命周期或发布能力。
- 本例外不能扩展为一般真机许可。除本计划列出的单次预检外，E8 `OPEN` 前的 ABI、Go、NetBird、E4-E7、网络、性能、能耗、长稳、渠道和产品测试仍禁止在物理设备执行。E4-E7 的完整义务未被免除；因 Emulator 授权前置缺失，它们移交到 E8 `OPEN` 后该具名物理设备的 R2/R3 门执行。

## 执行前输入门

### 最小只读设备元组发现边界

2026-07-18 最小只读发现已完成一次并冻结当时的发行版、型号、完整 build、API、kernel arch 和 ABI；该六条历史白名单不得作为当前元组原样重跑或扩展。发现不是 campaign。真实 endpoint 和 HDC target 仅在仓外受控映射；仓内只可使用 `PHYS-1`。以下六项保留为已执行白名单的可审计记录；除 `ADJ-20260806-0002` 另授的三条 rebind 外，设备端命令只能通过仓外变量 `PHYS_1_TARGET` 使用：

```sh
$HDC -t "$PHYS_1_TARGET" shell param get const.product.os.dist.name
$HDC -t "$PHYS_1_TARGET" shell param get const.product.model
$HDC -t "$PHYS_1_TARGET" shell param get const.product.software.version
$HDC -t "$PHYS_1_TARGET" shell param get const.ohos.apiversion
$HDC -t "$PHYS_1_TARGET" shell uname -m
$HDC -t "$PHYS_1_TARGET" shell param get const.product.cpu.abilist
```

这六项只用于当时冻结发行版、型号、build、API、kernel arch 和 ABI，不能用于识别个人、扩展 campaign 或替代任何其他输入门。在设备元组 discovery 内，禁止 `param dump`、`uname -a`、`ohos.boot.sn`、`const.ohos.serial`、`bm get -u`、`bm dump -a`、`bm dump -d`、`hidumper`，以及任何序列号、UDID、应用清单或全量状态读取。六条历史 discovery 不得重跑；`ADJ-20260806-0002` 仅额外授权一次三条只读 rebind（`const.ohos.apiversion`、`uname -m`、`const.product.cpu.abilist`，evidence `EV-E3-PHYS1REBIND7-20260806-0001`）；`ADJ-20260806-0004` 另授一次单条只读 build 确认（`const.product.software.version`，evidence `EV-E3-PHYS1BUILD7-20260806-0001`，精确 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`）。上述额外授权均已消费，禁止再查 dist/model/build/API/arch/ABI 与任何 campaign 动作。仅下节列出的单次长选项 enrollment 例外曾可读取 UDID（已消耗），其他设备 `shell` 命令仍不得执行。

### Signing enrollment 唯一例外

此例外只用于为 `PHYS-1` 注册普通开发签名 profile，不是 campaign、不是 evidence。它已由授权用户在批准边界内执行一次并消耗，不扩展 campaign，亦不新增动态调整记录。以下命令只作为已履行边界的审计记录，禁止重跑；无线连接与本机验签记录见 [Windows + DevEco Studio 开发交接](windows-development-handoff.md)：

```text
hdc shell bm get --udid
```

长选项 `--udid` 是唯一获批过的 UDID 读取，短选项 `bm get -u` 仍禁止。命令 stdout 已仅用于人工录入 AGC，未重定向、留存或回传。这个已消耗例外不授权任何其他 `shell`，也不授权 `install`、`send`、`start`、`stop`、运行 VPN 或 campaign；所有其他禁止保持不变。

历史 campaign 输入中设备、系统、API 与架构、HDC 曾按 API 23 / `PLA-AL10 6.1.0.117(SP6C00E115R7P7)` 冻结，但 initial live 已因 build drift 停止；该历史冻结与 ID 不得再作为可执行 campaign 输入。`ADJ-20260806-0003` 已冻结新元组；host reverify 已 PASS；`ADJ-20260806-0004` 已以单条 `software.version` 实测确认 HDC build 逐字匹配。当时 host 侧曾一度 `plan_status: ready`（仅 host 输入 ready）；现治理已为 `blocked-awaiting-device-authorization`（0001/0002 均 `consumed-blocked`；`ADJ-20260807-0003` 已批准 host process terminal probe 与 Settings 应用信息强制停止撤销路径，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)）。秘密和本机 HDC 标识保存在仓库外，只在仓库证据中使用稳定别名。

| 输入 | 必须值与证据 |
| --- | --- |
| 设备 | 新冻结：`PLA-AL10`（`ADJ-20260806-0003`）；历史 API23 冻结记录保留不改写 |
| 系统 | 新冻结：distribution `HarmonyOS`；HDC binding build `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`（`EV-E3-PHYS1BUILD7-20260806-0001` 单条 `software.version` 实测确认，逐字匹配；非合成候选）；Settings 人工观测 `7.0.0.100 (SP8C00E32R7P2patch09)` |
| API 与架构 | 新冻结：API `26`；kernel arch `aarch64`；app ABI `arm64-v8a`（均由 rebind `EV-E3-PHYS1REBIND7-20260806-0001` 实测；**不**从 build 字符串推断） |
| HDC | 唯一 target 在仓外受控映射中绑定为 `PHYS-1`；真实 endpoint、序列号、USB 标识或网络地址不得入库 |
| 签名 | 普通 debug 开发签名与 profile/certificate hash 经 host reverify 确认与历史登记一致、未变 |
| A/B 制品 | `ADJ-20260806-0003` 批准复用原 FINAL signed HAP A/B hashes；host reverify PASS（HAP/signature/profile/member hashes 未变）；兼容性仅作实测、**不**证明 API 26 安装/运行成功。旧 unsigned hash 只绑定历史准备阶段 |
| 源码与 SDK | build-source archive、source manifest、SDK map 及其 SHA-256 经 host reverify 确认；public manifest SHA-256 `66a70a52c92b927d4b23e528ae6eaf1b52169e504291c6ff0e7efa4c7ffee010`；Live 前按仓外 commit-bound freeze 重核 |
| 清理基线 | initial live 与 rebind 均未安装 A/B；Live 前须重新确认清理基线 |
| 采集准备 | `controlled external EvidenceRoot/RawRoot`；仓内只接收脱敏 manifest/projection/判定 |
| 审查 | rebind reviewer=`isolated kimi-coding/k3`（0 blocker/0 major）；build-confirm reviewer=`isolated kimi-coding/k3`（0 blocker/0 major，`EV-E3-PHYS1BUILD7-20260806-0001`）；新 campaign Live 审查角色须在 freeze 中重新绑定 |
| Campaign | 历史：`PHYS1API23` / `E3-PHYS-PREFLIGHT-20260806-0001` / `EV-E3-PHYS1API23-20260806-0001` 已消费且 blocked，不可复用。历史准备（`superseded-unexecuted`）：`E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001`（从未 Live、未占用；见 `ADJ-20260807-0001`）。历史 API26 live（`consumed-blocked`，保留）：`E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001`（operator-aborted；禁止局部 scenario5 重放）。历史 API26 live（`consumed-blocked`）：`E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002`（完整 1–7；`reviewed-pass/blocked`；双审查 0 B/5 M；见 [`0002 证据`](evidence/e3-physical-preflight-api26-0002-2026-08-07.md)）。当前：candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`、未 Live；`plan_status: blocked-awaiting-device-authorization`（`ADJ-20260807-0003` runner 变更完成，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)；用户显式设备授权 + fresh device confirmation 前无 auto retry/新 ID/设备命令授权；candidate IDs 保留，`ready` freeze 可绑定同候选身份但仅在明确授权后按治理决定） |
| Settings re-allow | 预测路径冻结为 `direct-system-activation`；路径偏差只作预注册观测，不因偏差本身 blocked；无 Settings 入口或无法重新激活仍 blocked。`ADJ-20260807-0003` 后：S5 撤销机制为 `settings-app-info-force-stop`（人工 Settings>应用信息>A>强制停止，单独截图 + `SETTINGS-APP-INFO-FORCE-STOP-CAPTURED` 确认）；Settings>VPN 页面仅 observation（`settings_vpn_page_policy=observation-only`，截图/字段记录，不影响结果）；HDC 只 `PidOf`/`BundleDump` 观察，绝不 ForceStop（HDC force-stop cleanup-only） |

原复合输入门曾冻结为 `plan_status: ready`；API23 initial live 已实际消费该授权并因 drift 变为 `blocked`。rebind、`ADJ-20260806-0003`、host reverify、`ADJ-20260806-0004` build 确认、`ADJ-20260807-0001` 跨日重新编号、API26 0001 live `consumed-blocked`、`ADJ-20260807-0002` 中文完整 1–7 重跑（0002 live `consumed-blocked`）与 `ADJ-20260807-0003`（host process terminal probe 与 Settings 应用信息强制停止撤销路径，runner 变更完成，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)）后，当前 `plan_status: blocked-awaiting-device-authorization`（禁 HDC；E8 `CLOSED`；用户显式设备授权 + fresh device confirmation 前无 auto retry/新 ID/设备命令授权；candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`，`ready` freeze 可绑定同候选身份但仅在明确授权后按治理决定）。2026-07-18 六条最小设备发现不得作为当前元组重跑；rebind 与 build 确认授权均已消耗且不得重跑；单次 signing enrollment 已消耗且不得重跑；旧 campaign/evidence ID 不得复用，亦不授权 infra retry 或局部 scenario5 重放。继续执行须先取得用户显式设备授权 + fresh device confirmation。

## Campaign 与重试纪律

一次 campaign 是在同一冻结输入元组和一个 campaign ID 下，从清理基线开始、按固定顺序执行全部场景、完成最终清理并提交独立审查的完整活动。首次安装原定为场景执行起点；本次 runner 在更早的目标绑定预检即停止，`campaign_started=false`，但 live initial 记录已经形成并占用 evidence ID，不能把“未安装”解释为 initial 未消费。预检授权不允许拆成多次选择性运行，也不允许把不同尝试的正面子结果拼接成 `pass`。

只有初次执行因纯基础设施原因得到 overall `blocked` 时，才可在相同冻结元组下进行一次记录在案的完整重试。基础设施原因仅包括 HDC/USB 中断、采集存储故障或与被测 VPN 行为无关的 runner/宿主故障。本次记录没有 `infrastructure_reason`，build drift 不属于允许原因，因此 `infrastructure-blocked-retry-1` 不获授权。任何继续都必须先取得新的路线决策、冻结完整新 build，并使用新的 campaign/evidence ID。

## 现有探针适用性与 API 23 隔离适配

2026-07-18 对历史 `spikes/e3-vpn-extension-hap` 的源码、构建配置和现存 HAP 完成只读核查；它仍固定 HarmonyOS `6.1.1(24)`、API 24 和 `phone`，A/B bundle 为 `cn.alfadb.netbird.e3vpna` 与 `cn.alfadb.netbird.e3vpnb`。历史源码只含 ArkTS 与资源，manifest 仅请求 `ohos.permission.INTERNET`，Extension 为非导出普通 `type: vpn`，且没有 Go、NetBird、native library、`MANAGE_VPN` 或签名配置。历史实现只调用公开 start/stop 并记录 `onCreate`/`onDestroy`，没有创建 `VpnConnection`，故不能覆盖 fd、active stop、Settings revoke 或真实冲突结果。历史 A/B HAP SHA-256 分别为 `6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c` 和 `c1d57d2544a93e4c4f172ee3ecb6ff2659adb7650558957e5b0cfb7aa69ae21e`；它们均为 `isSigned:false` 的 unsigned debug 研究制品，包内无 `libs/` 或 `.so`，不表示可安装到 arm64 真机。

适用性结论已落实为独立的 API 23 适配，而非改写历史树：`spikes/e3-vpn-extension-physical-preflight-hap/` 使用 `6.1.1(24)` 构建、target/compatible `6.1.0(23)`，A/B 为 `cn.alfadb.netbird.e3physvpna`/`cn.alfadb.netbird.e3physvpnb`。适配通过公开 API 最小化观测 `VpnConnection.create` 和 fd 生命周期；完整审计确认它仅有 `INTERNET`、非导出 `type: vpn`，无 Go、NetBird、WireGuard、`protect`、特权能力或外部 endpoint。唯一 arm64-v8a 纯 C `libfdprobe.so` 只用 `fcntl(F_GETFD)` 读取 fd 状态；它不关闭、复制、读取或写入 fd，`VpnConnection.destroy` 保持唯一 close 责任。历史 Emulator HAP、源码归档及 raw evidence 不得覆盖、改写或作为本适配的 campaign 输入。

## 受限适配要求

- 在 `VpnExtensionAbility.onCreate` 中用 Extension context 创建 `VpnConnection`，提交最小、确定且不承载业务流量的配置，并记录 `create` resolve/reject、错误码和返回 fd。
- fd 只用于证明公开 API 返回了有效描述符；不得交给 Go、NetBird、WireGuard 或产品模块。唯一 native `libfdprobe.so` 只能以 `fcntl(F_GETFD)` 采集只读快照，严禁 `close`、`dup`、`read` 或 `write`；`VpnConnection.destroy` 是唯一关闭责任，必须防止重复关闭并记录异常清理。
- start、stop、create、destroy、`onCreate` 和 `onDestroy` 都使用可关联 request ID 的 HiLog marker。A/B 保持独立普通 bundle，禁止共享身份制造冲突结果。
- 不加入 `protect`、外部 endpoint、数据泵、后台服务、TestRunner 或自动授权。系统授权、Settings 撤销（`ADJ-20260807-0003` 后为人工 Settings>应用信息>A>强制停止 + 截图确认）和冲突操作必须由普通用户可见 UI 完成并截图；Settings>VPN 页面只作 observation 截图/字段，不影响结果。
- 唯一物理设备 runner 为 `spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-campaign.ps1`，与 Emulator 历史 runner 分离；不得修改历史 raw evidence 或把 Emulator 判定重写为真机结果。
- runner 是设备命令唯一白名单：只允许两条 model/build target-binding 复核（零新增身份信息），定向 A/B bundle/PID/install/start/cleanup、单一连续 `E3PhysVpn` HiLog、A/B fault、screen/layout 采集，以及 S3/S5/S7 的 `PidOf`/`BundleDump` host process terminal probe（仅观察）。**HDC `ForceStop` 明确 cleanup-only**：仅 finally 残留清理，Reason 限 `exception-cleanup`/`final-cleanup`，永不用于 S5 撤销或任何场景判定。禁止全量查询、UDID、serial、`hidumper`、`uiInput` 与任何特权命令；真实 target 只从仓外 `PHYS_1_TARGET` 注入。

## 场景与通过条件

每个场景都必须在执行前冻结预期、决定性动作和独立的 60 秒有界观察窗口。窗口从该场景的决定性动作开始：场景 1 为首次清理基线查询，场景 2 为选择 allow，场景 3 为发出 stop，场景 4 为选择 deny，场景 5 为发起冻结路径的 re-allow，场景 6 为从 B 发起 start，场景 7 为发出最终 stop/destroy。决定性动作前的基线和 UI 截图仍须保留，但不计入观察窗；超时不得继续等待并把迟到结果计入通过。窗口内无法满足下述明确判据时记 `blocked`，并保留全部原始材料。

1. **清理基线**：确认 A/B 未安装、无 A/B 进程、无任何活动 VPN 或暂存文件，且其他 VPN 不参与场景，然后安装冻结的已签名 A/B HAP；全部确认和安装结果须在该场景 60 秒窗口内完成。
2. **Allow 与 fd**：从 A 的普通 Entry UI 发起 start，在系统授权 UI 选择 allow；窗口内必须观察授权 UI、A 的 `onCreate`、`VpnConnection.create` resolve 和有效 fd。
3. **Active stop**：A 保持活动时从普通 UI stop；窗口内必须观察 stop settlement、`onDestroy` 与对应 terminal/post-destroy fd snapshot（callback 优先）。`ADJ-20260807-0003` 后：当 callback terminal + post-destroy fd snapshot 缺失时（该元组上 marker 时序不可靠），允许 `destroy_terminal_policy=callback-or-strict-process-boundary` 的严格 fallback——窗口内唯一合法 stop（同 bundle/request）+ `UI_STOP` 或 `STOP_PROMISE_RESOLVED` + `VPN_ONDESTROY` + `VPN_DESTROY_BEGIN` 或 pre-destroy snapshot + 连续 absent host process probes（≥2 次、间隔 ≥3s，`PidOf`/`BundleDump` 观察）；S3 额外要求 bundle present；`FD_STILL_OPEN` 为硬 fail 不可 fallback；`VPN_DESTROY_ISSUED` 绝不算。S3 以 strict-process-boundary 通过时，overall pass 额外依赖 S5 同 bundle 后续 fresh request `CREATE_ACCEPTED` + post-create open（`clean_reactivation_proof`），缺失则 overall blocked。
4. **Deny**：以新鲜 B 授权请求在系统 UI 选择 deny。只有以下任一结果可判为 `pass`：窗口内出现可观察的 reject/error；或保留明确的系统拒绝截图，并且从拒绝动作开始的完整 60 秒窗口内 B 没有 `onCreate`、没有 `VpnConnection.create`。若 B 成功 create 或成为活动 VPN，则为 `fail`；缺拒绝截图、观察窗口不完整或既无可观察 reject/error 又不能满足“拒绝截图 + 60 秒无 onCreate/create”时为 `blocked`。
5. **Settings revoke**：预测路径为 `direct-system-activation`，路径偏差只记录为预注册观测，不因偏差本身 blocked。`ADJ-20260807-0003` 后撤销机制为 `settings-app-info-force-stop`：先 fresh A Start/create accepted/open（clean reactivation 基础）；Settings>VPN 页面仅 observation 截图/字段（`settings_vpn_page_observation_only=true`，不影响结果；该页面 capture 失败只写 `CaptureArtifacts status=degraded` 与独立 `observation_only_degraded` 诊断，**绝不**进入全局 `capture_degraded`、不阻断 S5/overall）；随后人工 Settings>应用信息>A>强制停止，单独截图 + `SETTINGS-APP-INFO-FORCE-STOP-CAPTURED` 确认；HDC 只 `PidOf`/`BundleDump` 观察，绝不 ForceStop（HDC force-stop 明确 cleanup-only，仅 finally 残留清理，Reason 限 `exception-cleanup`/`final-cleanup`）。pass 需人工确认、fresh create/open、bundle still present、主/vpn bundle process 连续 absent（≥2 次、间隔 ≥3s）；无 `UI_STOP` 正常；缺人工确认、缺 fresh create、bundle absent 或进程未连续 absent 均 `blocked`。
6. **第二 VPN 冲突**：再次激活 A，再从 B 发起 start；窗口内记录系统可见冲突、拒绝或替换语义，必须证明系统没有同时保留两个活动 VPN，并分别关联 A/B 生命周期与 create 结果。若 B 替换 A，还必须观察 A 对应 destroy terminal 与 post-destroy fd cleanup；缺任一项为 `blocked`。S6 双 active 判定为 operator 三态确认：先确认 `NO-DUAL-ACTIVE-CAPTURED`（最终可见状态未同时出现 A/B 两个 active VPN）；仅当其 false 时再独立确认 `DUAL-ACTIVE-CAPTURED`（只在明确看到 A/B 同时 active 时 true）。仅 `dual_active_confirmed=true` 且 `no_dual_active_confirmed=false` 为 `fail`（`dual-active-observed`）；`noDual=true` 且 `dual=false` 为正常；双 false 为 `blocked`（`dual-active-observation-unresolved`）；双 true 为 `blocked`（`inconsistent-operator-confirmation`）；留空/false 单独绝不直接 fail。record 含 `no_dual_active_confirmed`/`dual_active_confirmed`/`operator_state`。
7. **最终清理**：先通过普通 stop/destroy 清除活动连接；`ADJ-20260807-0003` 后，S7 先进行 pre-uninstall 连续 probe（callback terminal 缺失时作为严格 fallback 终态证据），场景 terminal 评估完成（callback pass 或 strict-process-boundary pass）后才允许现有 uninstall cleanup；随后卸载 A/B、删除暂存材料，并在窗口内采集 post-cleanup snapshot，确认无 A/B bundle、无 A/B 进程、无活动 VPN 和无测试配置残留；finally/uninstall 后的 absent 不得回填 terminal probes，也不得声称在进程已随卸载消失后直接查询其 fd。

逐场景聚合规则固定为：任一场景 `fail`，overall 为 `fail`；无 `fail` 但至少一个场景 `blocked`，overall 为 `blocked`；所有场景均为 `pass`，overall 才为 `pass`；`ADJ-20260807-0003` 后，若 S3 以 `strict-process-boundary` fallback 通过且无 S5 同 bundle 的 `clean_reactivation_proof`（fresh request `CREATE_ACCEPTED` + post-create open），即使全部场景 pass，overall 仍为 `blocked`。证据污染、hash 不一致、场景顺序破坏或跨 attempt 拼接使 overall 为 `invalid`。逐场景结果和 overall 结果都可供独立审查引用，但逐场景 `pass`、overall `pass` 或 `reviewed-pass` 均不得升格为 E4-E7、R 阶段、VPN 数据面或产品通过结论。

`ADJ-20260807-0003` **不新增任何退出标准**：它只修正 S3/S5/S7 的终态优先级（callback terminal + post-destroy fd snapshot 优先，`FD_STILL_OPEN` 硬 fail 不可 fallback，否则严格 process-boundary fallback）与 S5 撤销机制（人工 Settings>应用信息>强制停止）。S2/S4/S6 在 capture/window degradation 下保持既有 `fail` > `blocked` 规则不变：显式功能 fail（create rejected/invalid fd、deny 后 create、替换 destroy fail、operator 确认的双 active 可见）始终优先于 capture/window/operator 退化且绝不被降级为 `blocked`；证据缺失在退化下保持 `blocked`，绝不被升格为 `fail`。S6 双 active 为 operator 三态确认（`NO-DUAL-ACTIVE-CAPTURED` 优先，false 时再独立 `DUAL-ACTIVE-CAPTURED`）：仅 `dual=true` 且 `noDual=false` 为 fail，`noDual=true` 且 `dual=false` 为正常，双 false 为 `blocked dual-active-observation-unresolved`，双 true 为 `blocked inconsistent-operator-confirmation`，留空/false 单独绝不直接 fail。S5 Settings>VPN 页面 capture 为 observation-only：失败仍写 `CaptureArtifacts status=degraded` 与独立 `observation_only_degraded` 诊断，但绝不调用 `Add-CaptureDegradation`、不进入全局 `capture_degraded`/final overall block；决定性 `scenario-5-app-info-force-stop` capture 失败仍 `blocked`。freeze 决策字段为 `process_absent_probe_spacing_seconds`（旧 `spacing` 作为未知/缺新字段被拒绝，不兼容复用）。

## 判定影响

- 只有 `record_status: reviewed-pass` 与 `verdict: pass` 同时成立，才满足 E8 `OPEN` 的预检必要条件；它只表示冻结的 `PHYS-1` 目标元组上 E3 可达，不关闭 E1，不启动或完成 E4-E7，不证明 `protect`、流量、Go、NetBird、产品或发布能力，也不自动把 E8 置为 `OPEN`。
- `fail` 只否定该具名设备、完整 build、API、arm64、签名 profile、A/B 源码/SDK/HAP 组合上的预检路径，不得外推其他设备、build、API、架构、发行版或 Emulator；E8 必须保持 `CLOSED`，继续执行须先取得新路线决策。
- `blocked` 和 `invalid` 不形成正面或负面平台结论，均使 E8 保持 `CLOSED`，也不得用第二台设备绕过；除同一冻结元组上的一次基础设施性 blocked 重试外，继续执行必须先取得新的路线决策。
- 后续 E8 独立聚合审查只能引用本记录的精确 E3 可达性结论；还必须单独核验当前R0正式基线（现v0.74.7）的 E1 官方 Go loader/runtime、全部目标元组与哈希、Emulator blocked-exception 成员及其他聚合条件，再显式决定是否 `OPEN`。E8 `OPEN` 只许可后续具名物理设备投入，不表示 VPN 或数据面已通过。

## 原始证据要求

每个场景必须保留未筛选原始 HiLog、脱敏结构化 transcript projection、系统授权/Settings/冲突/结果截图、必要布局或状态快照、定向 A/B fault list、开始/结束时间和 SHA-256 manifest。`ADJ-20260807-0003` 后，S3/S5/S7 的 host process terminal probe（`PidOf`/`BundleDump` 观察，time/status/连续计数）全部进入场景 record 与 transcript，任何 finally/uninstall 后的 absent 不得回填；HDC force-stop 仅 finally 残留清理（cleanup-only），永不用于撤销。S5 Settings>VPN 页 capture 为 observation-only：失败仍保留 degraded artifact 与独立 `observation_only_degraded` 诊断，不进入全局 `capture_degraded`、不阻断 S5/overall。证据还必须绑定完整目标元组、稳定设备别名、源码归档、source manifest、SDK、签名验证结果、A/B HAP 和 runner hash。

证据只引用抽象的 `controlled external EvidenceRoot/RawRoot`。EvidenceRoot 保存脱敏 structured projection、manifest 与判定；独立 RawRoot 仅在实际产生时保存未筛选 HiLog、截图、布局和定向 fault 原始材料。raw 仓外受控保留至少 90 天；存在争议时保留至争议关闭。日志进入仓库前必须扫描秘密。HDC target、序列号、USB 标识、网络地址、UDID、签名私钥、证书口令、profile 内部设备标识、账号和 token 不得入库；含 UDID 的 profile 验证原始输出不得归档。signed HAP/profile/certificate 保留至 E8 审查结束。本次在连续采集前预定停止，故 raw HiLog、截图、布局与 fault 未产生；这不是缺失或篡改。独立 reviewer 已核对脱敏记录、哈希、停止边界与最终清理，并将记录审查为 `reviewed-pass/blocked`。

## 专用证据模板

target code 为 `PHYS1API26`。campaign ID `E3-PHYS-PREFLIGHT-20260807-0002` / evidence ID `EV-E3-PHYS1API26-20260807-0002` 已 Live 并 `consumed-blocked`（见[API26 0002 证据](evidence/e3-physical-preflight-api26-0002-2026-08-07.md)）。历史：`ADJ-20260806-0003` 曾分配 `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001`（`superseded-unexecuted`）；`ADJ-20260807-0001` 曾分配 `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001`（`consumed-blocked`，保留，见[API26 0001 证据](evidence/e3-physical-preflight-api26-2026-08-07.md)）。当前 candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`、未 Live；`plan_status: blocked-awaiting-device-authorization`（`ADJ-20260807-0003` runner 变更完成，host 侧已重建并登记 [`EV-E3-PHYS1HOST-20260808-0001`](evidence/e3-physical-preflight-host-remediation-2026-08-08.md)；用户显式设备授权 + fresh device confirmation 完成前 **无** auto retry/新 ID/设备命令授权；candidate IDs 保留，`ready` freeze 可绑定同候选身份但仅在明确授权后按治理决定）。HDC binding 已由 `EV-E3-PHYS1BUILD7-20260806-0001` 实测确认；API `26` 仍由 rebind 实测、不从 build 推断。本模板不授权 Live 或新 campaign。历史 API23 initial live 见[已审查证据记录](evidence/e3-physical-preflight-2026-08-06.md)。登记 HAP 仍 compile API `24`、target/compatible API `23`；设备冻结 API 为 `26`，兼容性仅实测。

```yaml
evidence_id: EV-E3-PHYS1API26-20260807-0002 # consumed-blocked; reviewed-pass/blocked; plan_status blocked-awaiting-device-authorization; no auto retry/new ID/device command until explicit user device authorization + fresh device confirmation
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: draft | collected | reviewed-pass | reviewed-fail | blocked | invalidated | superseded
stage_or_gate: E3
related_stages_or_gates: [E8]
target_tuple:
  distribution: HarmonyOS
  device_model: PLA-AL10
  device_alias: PHYS-1
  full_system_build: PLA-AL10 7.0.0.100(SP8C00E32R7P2) # HDC binding; Settings manual 7.0.0.100 (SP8C00E32R7P2patch09)
  api: "26" # device freeze API; HAP remains compile 24 / target+compatible 23
  architecture: arm64
  kernel_arch: aarch64
  app_abi: arm64-v8a
  sdk_api_syscap: HarmonyOS SDK 6.1.1.125 / compile API 24; target and compatible API 23; public VPN Extension API only
  channel: ordinary-development-signing-only
hdc_target_reference: <out-of-repo-controlled-reference>
signing:
  type: ordinary-development
  device_in_profile: true
  public_fingerprint: <non-secret-value-or-N/A-with-reason>
code_sha: <runner-binds-current-clean-HEAD-out-of-repository-immediately-before-live>
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime allowed
source_archive_sha256: <sha256>
source_manifest_sha256: <sha256>
sdk_sha256: <sha256-map>
runner_sha256: <sha256>
artifact_sha256:
  hap_a: <sha256>
  hap_b: <sha256>
preflight_inputs_frozen_at: <ISO-8601-with-zone>
campaign_id: E3-PHYS-PREFLIGHT-20260807-0002
prior_blocked_binding: EV-E3-PHYS1API26-20260807-0001 # consumed-blocked; no partial scenario5 replay; not infrastructure retry
attempt: initial | infrastructure-blocked-retry-1
retry_basis: N/A-for-initial - ADJ-20260807-0002 protocol usability correction full 1-7 rerun; not device-behavior or infrastructure retry
scenario_window_seconds: 60
settings_reallow_expected_path: direct-system-activation
settings_reallow_path_policy: observation-only
settings_revoke_mechanism: settings-app-info-force-stop # ADJ-20260807-0003; manual Settings app-info force-stop with confirmation and screenshot; HDC never force-stops for revoke
settings_vpn_page_policy: observation-only # Settings>VPN page is screenshot/fields observation only, never a pass gate
settings_vpn_page_observation_only: true
destroy_terminal_policy: callback-or-strict-process-boundary # callback terminal + post-destroy fd snapshot first; strict fallback = unique stop + onDestroy + begin/pre snapshot + consecutive absent host process probes
process_absent_required_count: 2
process_absent_probe_spacing_seconds: 3 # freeze field (ADJ-20260807-0003); legacy `spacing` is rejected as unknown/missing for every mode
operator: authorized user
orchestrator: main agent
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
    terminal_mode: callback-post-fd | strict-process-boundary # strict fallback needs unique stop/onDestroy/begin + >=2 absent probes >=3s apart + bundle present; FD_STILL_OPEN is a hard fail
    clean_reactivation_proof: true | false # S3 strict-process-boundary pass requires S5 same-bundle fresh CREATE_ACCEPTED + post-create open; missing => overall blocked
  scenario_4_deny:
    deny: pass | fail | blocked
  scenario_5_settings_revoke:
    settings_revoke: pass | fail | blocked
    terminal_mode: settings-app-info-force-stop
    force_stop_confirmed: true | false
    settings_vpn_page_observation_only: true
    settings_vpn_page_capture: <name/status/visible> # observation-only; degraded never blocks S5/overall, recorded in observation_only_degraded
    bundle_present_during_probe: true | false
    process_final_state_probes: <time/status/consecutive-absent list> # pre-cleanup host probes; finally-absent never backfills
  scenario_6_second_vpn_conflict:
    second_vpn_conflict: pass | fail | blocked
    no_dual_active_confirmed: true | false # NO-DUAL-ACTIVE-CAPTURED operator confirmation
    dual_active_confirmed: true | false # DUAL-ACTIVE-CAPTURED; only asked when no_dual_active_confirmed is false (live); simulation may pre-set both
    operator_state: normal | dual-active-observed | dual-active-observation-unresolved | inconsistent-operator-confirmation # only dual=true && noDual=false fails; both false => blocked unresolved; both true => blocked inconsistent; empty/false alone never fails
  scenario_7_final_cleanup:
    final_cleanup: pass | fail | blocked
    terminal_mode: callback-post-fd | strict-process-boundary # uninstall cleanup allowed only after the terminal assessment completes; pre-uninstall probes; finally-absent never backfills
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
observation_only_degraded: <independent diagnostic list> # observation-only captures (Settings>VPN page) that degraded; never enters capture_degraded, never blocks scenario/overall
forbidden_capabilities_audit: <no-Go-NetBird-private-fork-MANAGE_VPN-or-privileged-bypass>
actual: <bounded-observation-summary>
verdict: pass | fail | blocked | invalid
scope_statement: <exact-target-only-no-extrapolation>
cleanup_result: <pre-uninstall-in-process-fd-snapshot-and-post-uninstall-no-bundle-process-active-VPN-result>
reviewer: <independent-isolated-session-bound-at-freeze> | pending
reviewed_at: <ISO-8601-with-zone-or-pending>
review_record: <id-or-pending>
```
