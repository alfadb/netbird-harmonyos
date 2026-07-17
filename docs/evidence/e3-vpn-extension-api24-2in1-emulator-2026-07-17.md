# E3 VPN Extension API 24 2in1 Emulator 前置证据

最后核验：2026-07-17

本文记录官方 HarmonyOS API 24 x86_64 2in1 Emulator 对 E3 授权链前置组件的独立只读核验。该记录与 `netbird_api24_phone` 的 E3 0003/0004 完全独立：它使用新下载的 `pc_all_x86` image、新实例 `netbird_api24_2in1`、MateBook Pro screen profile、独立 HDC guest port `10001` 和固定 target `127.0.0.1:10001`。任何结果均不外推到 phone、arm64、具名真机、其他 Emulator image 或华为商用设备。

只读门确认 `com.huawei.hmos.vpndialog` 与 `VpnServiceExtAbility` 均不存在后，执行按预定停止条件结束。归档 E3 0003 A/B HAP 只核对 SHA-256，没有发送到 guest、没有安装，也没有调用 `create`、`protect` 或任何 system/debug/enterprise 绕过。

## 证据记录

```yaml
evidence_id: EV-E3-2IN1EMU24-20260717-0001
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8, R1, R3]
target_tuple:
  distribution: official HarmonyOS 6.1.1(24) API 24 2in1 Emulator image; pc_all_x86 Release
  device: netbird_api24_2in1 2in1 Emulator; MateBook Pro screen profile; explicitly not phone or physical device
  full_system_version: instance metadata HarmonyOS 6.1.1(24), software 6.1.0.125; guest software emulator 6.1.0.125(SP12DEVC00E47R1P3)
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: API 24; only guest identity and BMS/Settings registrations measured; VPN runtime SysCap not entered
  channel: N/A; no HAP was sent or installed
code_sha: cd6f71c7ecc71e878c5fddea793d65de5b68f31d; no project source was read by the guest, built, or executed
upstream_sha: N/A; no Go, NetBird, or other upstream runtime code participated
toolchain: Debian GNU/Linux 13 x86_64 Pod; host Linux 7.0.14-4-pve; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e; KVM enabled; noWindow cold boot
working_directory: /home/worker/work/base/netbird-harmonyos
command: official image install; unique 2in1 instance create with MateBook Pro profile; noWindow cold boot with hdcPort 10001; every guest command used explicit Beta HDC target 127.0.0.1:10001; exact commands and output are in the transcript
input: official pc_all_x86 API 24 image and empty new instance; archived E3 0003 A/B HAP hashes checked on host only, with neither file sent or installed
expected: read guest identity/API/arch/device type, search bm dump -a for vpn/vpndialog, directly dump com.huawei.hmos.vpndialog, and search Settings registrations; continue to archived-HAP E3 replay only if the bundle exists and declares VpnServiceExtAbility; otherwise stop without HAP installation
actual: guest reported HarmonyOS, API 24, x86_64 and device type 2in1. Bare bm dump -a returned a parameter error; bm help documented user selection, and bm dump -a -u 100 then returned 49 bundles with zero vpn/vpndialog matches. Direct vpndialog dumps with default user and user 100 both failed to get bundle information. Settings registration search matched only two MANAGE_VPN permission strings and no VpnServiceExtAbility or VPN extension registration. A/B remained absent, so no HAP was sent or installed and no E3 scenario was run.
started_at: 2026-07-17T21:04:47+08:00
ended_at: 2026-07-17T21:15:33+08:00
clock_source: host CLOCK_REALTIME, tmux zsh extended-history timestamp, Emulator.log timestamps, guest command responses, and persistent file mtimes
artifact_sha256:
  raw_artifact_manifest: 07dba727f7312de8ba116f5a7976f30bedf4e69d4f576e8e42195a365d37b7f3
  transcript: 5e3c8eea9be1a8248153a74a4104fc7480d44f23e6e06a79ebc4908e277729c7
  complete_user_100_bundle_list: d39a1ed8414c7f7e0cb120ed245a3ebe257a89d88f2aef63697baa6df3a7c7e7
  direct_vpndialog_query: 891a293aea4210c64736dade60000faa0ceab395cada52e2552cdacd96ea7107
  settings_vpn_registration_search: 4d63afb2b2228be4323b97be3fd5cc21c15228ccf71a0cbf80957cee8c7334f5
  guest_identity: e4675f8c7f56861c75b850b67c3890413bacf55934af6ae809fcbd3b3a65bde4
  emulator_console: 7cc2fd970523c0ff353fc29a17ab5af8dc3dc589929c8631fe6e5547d457ba0b
  final_cleanup: 5c0b373dcc7bff9826ad1b72ebb636ee11b9f98cb9b18ba2462cfa8d6326131f
  official_image_info_json: fc98e75b35585ab06c268d263e33ac6c6f4435293f8ff2e405cde5430810492e
  official_image_sdk_pkg_json: 07f1fa601a7eb9df012fc01ca1e06ccb9337975bbf8686c1af59a16eca1c2785
  preserved_instance_config: 517b79e4035c8c15479bbad59b1a5294fd4c7f81d3de77c11be06a3d85902eea
  archived_candidate_a_not_installed: 6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c
  archived_candidate_b_not_installed: c1d57d2544a93e4c4f172ee3ecb6ff2659adb7650558957e5b0cfb7aa69ae21e
raw_log_reference: repository-controlled raw/EV-E3-2IN1EMU24-20260717-0001-*; manifest lists seven raw files, three preserved external metadata files, and two host-only archived HAP references; repository access
verdict: blocked
reviewer: anthropic/claude-opus-4-8 independent 2in1 matrix review
reviewed_at: 2026-07-17T22:27:30+08:00
review_record: Independent 2in1 matrix review verified the authoritative full artifact manifest `07dba727f7312de8ba116f5a7976f30bedf4e69d4f576e8e42195a365d37b7f3`, transcript `5e3c8eea9be1a8248153a74a4104fc7480d44f23e6e06a79ebc4908e277729c7`, user-100 list `d39a1ed8414c7f7e0cb120ed245a3ebe257a89d88f2aef63697baa6df3a7c7e7` (49 bundles), default/user-100 direct vpndialog queries, Settings registration search, host-only A/B HAP hashes with no guest send or installation, and final cleanup. With reviewed supplemental 0002, all three direct scopes (default, user 100, user 0) and both BMS lists are covered. 0B/0M/6 non-blocking minors retained in the review appendix; verdict remains blocked and does not close E3.
```

`record_status: reviewed-pass` 表示独立审查已确认材料完整性、范围和 `blocked` 判定，不表示 E3 通过。`0001` 的 current-user-100 完整列表为 49 bundles；与已审查的 `0002` 合并后，user 0 完整列表为 7 bundles，direct vpndialog 查询覆盖 default、user 100 和 user 0 三个合法范围，Settings 注册查询均无可进入的授权组件。该精确 2in1 目标元组仍为 `verdict: blocked`，不改变 phone 0003/0004 的 `reviewed-pass/blocked`、phone E3 未关闭、E4-E7 dependency blocked 或 E8 `CLOSED` 状态。

## 审查附录

独立审查保留以下 6 项 non-blocking minors：

1. registration-layer 边界必须保持明确；BMS/Settings 注册观察不外推为 VPN runtime 结论。
2. YAML 以对应 artifact manifest 为全量材料权威来源；叙述性摘录不替代 manifest。
3. `0002` transcript 的 `0_bundles` 标签表示 0 个 VPN bundles（0 个 `vpn|vpndialog` 匹配），不是 0 个总 bundles。
4. `0002` 的两次启动失败属于 guest readiness 前的环境轨迹，不是 BMS、Settings 或 VPN runtime 结果。
5. `0001` 的裸 `bm dump -a` 参数错误已按 help 补充当前 user，不作为组件缺失依据。
6. 0001/0002 的范围保持隔离；合并时仅明确覆盖 default、user 100 和 user 0，不外推到其他目标。

user 0 coverage 的原 minor 已由 `0002` 补齐并解决，但审查仍保留以上 6 项 non-blocking minor 记录。

## 官方镜像与实例

使用的安装命令未指定代理、`force` 或替代镜像：

```text
/home/worker/harmonyos/command-line-tools/26.0.0.461/bin/Emulator -install -deviceType 2in1 -osVersion 'HarmonyOS 6.1.1(24)' -imageRoot /home/worker/harmonyos/emulator-images
```

Emulator 报告下载 `2,352,917,078` bytes 到 `system-image/HarmonyOS-6.1.1/pc_all_x86/` 并成功完成。实例创建命令使用帮助列出的精确 profile 名 `MateBook Pro`，实例元数据记录 `deviceType=2in1`、`PCEMU-FD00`、3120x2080、304 dpi、API 24、x86_64、software 6.1.0.125 与 `isHotBoot=false`。

启动命令使用 `coldboot`、`noWindow` 和独立 `hdcPort 10001`。Emulator 日志在 `2026-07-17 21:12:30.483 +08:00` 明确记录 `KVM is supported and accessible`；Beta HDC 3.2.0e 只连接固定 target `127.0.0.1:10001`。

## 只读门结果

| 查询 | 实际结果 | 判定 |
| --- | --- | --- |
| guest distribution/API/arch/device type | `HarmonyOS` / `24` / `x86_64` / `2in1` | 目标身份确认 |
| `bm dump -a` | 参数错误 | 按 help 增补当前 user，不作为组件缺失依据 |
| `bm dump -a -u 100` | 49 个 bundle；`vpn\|vpndialog` 0 匹配 | current user 100 未注册 VPN dialog bundle |
| user 0 all-bundle list | 由 reviewed supplemental `0002` 完整采集；7 个 bundle、`vpn\|vpndialog` 0 匹配 | user 0 未注册 VPN dialog bundle |
| `bm dump -n com.huawei.hmos.vpndialog` | `failed to get information` | default query 未找到 bundle |
| 同一 direct dump 加 `-u 100` | 同样失败 | user 显式化后结论不变 |
| Settings `vpn\|vpndialog` 搜索 | 仅两处 `ohos.permission.MANAGE_VPN` | 只有权限文本 |
| Settings ability/extension 搜索 | 无 `VpnServiceExtAbility` 或 VPN extension | 无可进入的授权组件注册 |
| A/B bundle 查询 | 两者均不存在 | HAP 未安装 |

`MANAGE_VPN` 是 Settings 系统 bundle 的请求权限文本，不是 `VpnServiceExtAbility`，也不授权普通第三方应用使用该权限。本轮没有调用该权限或修改任何权限。

## 停止边界

预定条件要求 `com.huawei.hmos.vpndialog` 存在且其 BMS 信息包含 `VpnServiceExtAbility` 后，才允许把归档 A/B HAP 发送到该独立 target 并重放授权允许、拒绝、active stop、普通撤销和双应用冲突。两个条件均未满足，因此：

- 没有 HDC file send、`bm install`、`aa start` 或普通 UI 点击；
- 没有调用 `VpnConnection.create`、`protect` 或任何隐藏/system/debug/enterprise 能力；
- 没有生成测试 HAP identity、HiLog、layout、screenshot 或 fault 材料，因为测试阶段未开始；
- 没有把“组件不存在”改写成授权拒绝、应用失败或 phone 结论。

A/B 的 host 归档哈希仍与 0003 一致，但它们只作为“若前置门通过时的候选输入”列入 manifest，不是本次被安装或运行的制品。

## 清理与保留

`Emulator -stop netbird_api24_2in1` 报告成功，Beta HDC server 随后停止。最终检查确认该实例 `isRunning=false`，没有目标 Emulator/QEMU/HDC server 进程，`10001` 和 `8710` 无监听；两个本轮 tmux session 已删除。

官方 image 与实例分别保留在：

```text
/home/worker/harmonyos/emulator-images/system-image/HarmonyOS-6.1.1/pc_all_x86/
/home/worker/harmonyos/emulator-instances/netbird_api24_2in1
```

## 原始材料

完整清单见 `raw/EV-E3-2IN1EMU24-20260717-0001-artifact-manifest.txt`，其 SHA-256 为 `07dba727f7312de8ba116f5a7976f30bedf4e69d4f576e8e42195a365d37b7f3`。该 manifest 是本记录全量原始材料的权威清单；关键材料包括完整命令 transcript、current user 100 完整 bundle list、default/current-user direct vpndialog 查询、Settings VPN 注册检索、guest identity、Emulator console 与最终清理记录。default all-bundle query 的参数错误不作为组件缺失依据；已审查的 supplemental `0002` 补齐 user 0 范围。本记录不用于任何阶段退出。

## user 0 补充证据

`EV-E3-2IN1EMU24-20260717-0002` 是对 `0001` 的独立只读 supplemental；两者均已完成同一独立 2in1 矩阵审查，且均为 `reviewed-pass/blocked`。它复用保留的 `pc_all_x86` image 与 `netbird_api24_2in1` instance，并以 `coldboot`、`noWindow`、KVM 和 Beta HDC `3.2.0e` 的固定 target `127.0.0.1:10001` 采集合法 `user 0` 范围。最初两次冷启动在 guest readiness 前分别出现 connect-key 未映射和 `Offline`，均已停止并保留完整 transcript/console/cleanup；它们仅是 readiness 环境轨迹，不是 BMS、Settings 或 VPN runtime 结果。第三次在正常 `emulator-connect` 建链后取得 `Connected`，才执行 guest 只读查询。

```yaml
evidence_id: EV-E3-2IN1EMU24-20260717-0002
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8, R1, R3]
target_tuple:
  distribution: official HarmonyOS 6.1.1(24) API 24 2in1 Emulator image; pc_all_x86 Release
  device: preserved netbird_api24_2in1 2in1 Emulator; MateBook Pro screen profile; explicitly not phone or physical device
  full_system_version: guest emulator 6.1.0.125(SP12DEVC00E47R1P3); instance metadata HarmonyOS 6.1.1(24), software 6.1.0.125
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: API 24; only identity and BMS/Settings registrations measured; VPN runtime SysCap not entered
  channel: N/A; no HAP was sent or installed
code_sha: cd6f71c7ecc71e878c5fddea793d65de5b68f31d; no project source was read by the guest, built, or executed
upstream_sha: N/A; no Go, NetBird, or other upstream runtime code participated
toolchain: Debian GNU/Linux 13 x86_64 Pod; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e; KVM enabled; noWindow cold boot
working_directory: /home/worker/work/base/netbird-harmonyos
command: preserved 2in1 instance coldboot/noWindow with hdcPort 10001; normal Beta-HDC target connection; every guest command used explicit target 127.0.0.1:10001; exact commands and output are in the transcript
input: preserved official pc_all_x86 image and preserved 2in1 instance; no HAP input was sent, installed, or executed
expected: recheck guest identity, collect complete bm dump -a -u 0 and vpn/vpndialog search, directly query vpndialog -u 0, and search Settings registrations -u 0; stop without HAP activity
actual: guest again reported HarmonyOS, API 24, x86_64, device type 2in1 and SP12DEVC00E47R1P3. Complete user-0 BMS list contained 7 bundles and zero vpn/vpndialog matches. Direct com.huawei.hmos.vpndialog -u 0 failed to get information. Settings -u 0 registration search had no VPN, vpndialog, VpnServiceExtAbility, or VPN-extension match. No HAP was sent or installed.
started_at: 2026-07-17T22:04:04+08:00
ended_at: 2026-07-17T22:15:16+08:00
clock_source: host CLOCK_REALTIME, persistent raw-file mtimes, Emulator.log timestamps, and guest command responses
artifact_sha256:
  raw_artifact_manifest: cf0ec86ace0ab559bad22968cb73c099957f99bf4b1020fa82db2e6b554211a5
  successful_transcript: c40b6283c7e88086a9f8b5fe1b9d227cc00614e9146f001b0956e0f46f587815
  complete_user_0_bundle_list: 91dc0a1b8613e2b60f3b54b860a4ca9366e538c01aaedb9865c942d22a4724dd
  user_0_list_count_validation: e2fd48915abf769b3e2d6ba550ae539ebb3e739cc099c95f8810dc39cab3ec2d
  user_0_vpn_search: 1c8e4613213bf7aa21a898686c18e2341485f79a5d2994a3fda920ce979ddbd7
  direct_vpndialog_user_0: 72480072effd48b858e3163d3a7b16f0cdb1f624f4cf62032ac3891fcb45c87a
  settings_user_0_registration_search: 705449361bea5d4d08bec1d8a5b99e65b61d7d9e4fa06b13e7c6030a3c068e85
  guest_identity_recheck: 876b55f81c4c382996274973549f255cc165e7122d2db5510c5960aff01a03f5
  current_boot_emulator_log: 9518e13bd825ab07d0756068c054d382cb964f215a5ae5d0a1a3a98ed8b870ec
  final_cleanup: 6692b1b0dcaf8171977296bac2b6e8e91da0a70395196c595547a28976ba1bfc
raw_log_reference: repository-controlled raw/EV-E3-2IN1EMU24-20260717-0002-*; authoritative manifest lists 16 raw logs, including both pre-readiness startup failures, and three preserved external metadata/config files; repository access
verdict: blocked
reviewer: anthropic/claude-opus-4-8 independent 2in1 matrix review
reviewed_at: 2026-07-17T22:27:30+08:00
review_record: Independent 2in1 matrix review verified the authoritative full artifact manifest `cf0ec86ace0ab559bad22968cb73c099957f99bf4b1020fa82db2e6b554211a5`, successful transcript `c40b6283c7e88086a9f8b5fe1b9d227cc00614e9146f001b0956e0f46f587815`, user-0 list `91dc0a1b8613e2b60f3b54b860a4ca9366e538c01aaedb9865c942d22a4724dd` (7 bundles), count validation, user-0 direct vpndialog and Settings registration searches, identity, startup/readiness logs, HAP noninstallation, and cleanup. The transcript label `0_bundles` means 0 VPN bundles (0 `vpn|vpndialog` matches), not 0 total bundles; the full list contains seven package names. Combined with reviewed 0001, user 100=49 and user 0=7, direct scope is default/user 100/user 0, and HAPs remain uninstalled. Startup failures are readiness environment traces only. The original user-0 coverage minor was resolved by 0002, while the review retains the six non-blocking minor records listed in the appendix. 0B/0M/6 non-blocking minors; verdict remains blocked and does not close E3.
```

该 supplemental 的 `blocked` 仅表示这个精确 2in1 image 在已采集的 registration-layer 前置查询中没有可进入普通 VPN 授权 UI 的组件；不把 BMS/Settings 结果扩展为 VPN runtime 结论，也不替代或改变 phone、Tablet、E3、E8 或真机禁令。`0002` transcript 的 `0_bundles` 标签经审查解释为 0 个 VPN bundles（0 个 `vpn|vpndialog` 匹配），不是 0 个总 bundle；完整 user-0 list 实为 7 个 bundle。两次启动失败只是 guest readiness 前的环境轨迹。清理后 `Emulator -stop` 与 Beta HDC `kill` 均执行，`10001`/`8710` 无监听、目标 Emulator/QEMU/HDC server 无残留，image 与 instance 均保留。
