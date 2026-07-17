# E3 VPN Extension API 24 Tablet Emulator 前置证据

最后核验：2026-07-17

本文记录官方 HarmonyOS API 24 x86_64 Tablet Emulator 对 E3 授权链前置组件的独立只读核验。该记录与 phone E3 0003/0004 及 2in1 0001 完全独立：它使用新下载的 `tablet_x86` image、新实例 `netbird_api24_tablet`、MatePad Pro 13 screen profile、独立 HDC guest port `10002` 和固定 target `127.0.0.1:10002`。任何结果均不外推到 phone、2in1、arm64、具名真机、其他 Emulator image 或华为商用设备。

只读门确认 `com.huawei.hmos.vpndialog` 与 `VpnServiceExtAbility` 均不存在后，执行按预定停止条件结束。归档 E3 0003 A/B HAP 只在 host 核对 SHA-256，没有发送到 guest、没有安装，没有进入授权普通 UI，也没有调用 `create`、`protect` 或任何 system/debug/enterprise 绕过。

## 证据记录

```yaml
evidence_id: EV-E3-TABLETEMU24-20260717-0001
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8, R1, R3]
target_tuple:
  distribution: official HarmonyOS 6.1.1(24) API 24 Tablet Emulator image; tablet_x86 Release
  device: netbird_api24_tablet Tablet Emulator; MatePad Pro 13 screen profile; explicitly not phone, 2in1, or physical device
  full_system_version: instance metadata HarmonyOS 6.1.1(24), software 6.1.0.125; guest software emulator 6.1.0.125(SP9DEVC00E16R1P1)
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: API 24; only guest identity and BMS/Settings registrations measured; VPN runtime SysCap not entered
  channel: N/A; no HAP was sent or installed
code_sha: cd6f71c7ecc71e878c5fddea793d65de5b68f31d; no project source was read by the guest, built, or executed
upstream_sha: N/A; no Go, NetBird, or other upstream runtime code participated
toolchain: Debian GNU/Linux 13 x86_64 Pod; host Linux 7.0.14-4-pve; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e; KVM enabled; noWindow cold boot
working_directory: /home/worker/work/base/netbird-harmonyos
command: official image install; unique Tablet instance create with MatePad Pro 13 profile; noWindow cold boot with hdcPort 10002; every guest command used explicit Beta HDC target 127.0.0.1:10002; exact commands and output are in the transcript
input: official tablet_x86 API 24 image and empty new instance; archived E3 0003 A/B HAP hashes checked on host only, with neither file sent or installed
expected: read guest identity/API/arch/device type/build; use bm help and inspect complete current-user-100 and user-0 bundle lists for vpn/vpndialog; directly dump com.huawei.hmos.vpndialog in default, current-user-100, and user-0 scopes; search Settings registrations; continue to archived-HAP E3 replay only if the bundle exists and declares VpnServiceExtAbility; otherwise stop without HAP installation
actual: guest reported HarmonyOS, API 24, x86_64, device type tablet, and software build emulator 6.1.0.125(SP9DEVC00E16R1P1). bm help limited -u to current user or user 0. bm dump -a -u 100 returned 58 bundles and bm dump -a -u 0 returned 8, with zero vpn/vpndialog matches in each complete list. Direct vpndialog dumps in default, user 100, and user 0 scopes all failed to get bundle information. Settings default and user-100 registration searches matched only two MANAGE_VPN permission strings and no VpnServiceExtAbility or VPN extension registration; the user-0 search had no match. A/B were absent in all three query scopes, so no HAP was sent or installed and no E3 scenario was run.
started_at: 2026-07-17T21:32:44+08:00
ended_at: 2026-07-17T21:45:06+08:00
clock_source: host CLOCK_REALTIME, tmux pane lifecycle timestamps, Emulator.log timestamps, guest command responses, and persistent file mtimes
artifact_sha256:
  raw_artifact_manifest: d43bd4f23070eb1ce313ca0fc5fa541735de2d512fab6b48d6a752e6091b781c
  transcript: 66f312c845b976bb861fbc88f5ac1929fe3865a7f38ac57486204d8d15ba43c2
  complete_user_100_bundle_list: 03dafac5361b3f7a376f72846a82692e5f3d7149b167c1e932bac2c8f80e3f2c
  complete_user_0_bundle_list: b28bf61bba663c102c32c03a668f9a4fa8f8d8e60c2737ac9628e0c276eb7817
  direct_vpndialog_queries: d5b4a1924228a1e99be32419075c501f0d34baa8e17fc51e7e0f5873072b7c6e
  settings_vpn_registration_search: 1a579e8f0380b6143f139e745e336feff10c2eb10a7f28456d1a02a98c6e4ab0
  guest_identity: b535146323b282614f185babc8a9481d0e088b528c880fdd51ce4a4042e4b2cb
  hdc_connect: 6fd37fb3f6fc86ac3efdfbd2eae427c0edf84101fa63bad75156ac82acdbf8f9
  emulator_full_log: 47f6d220b362c15ae2c0f515a1306f18daf950c0b2d2e1ae192c5feb7c6f6a6e
  qemu_full_log: 40837c324116383d6b0889b7d303f68247a20a478df8586b8f4e2241ebdd1251
  final_cleanup: 73e5524a4295580c9e65e22a349d49b3b59cd07bc6164eef6db071e926776e75
  official_image_info_json: fc98e75b35585ab06c268d263e33ac6c6f4435293f8ff2e405cde5430810492e
  official_image_sdk_pkg_json: 99b8904200953662a26ebfc87c8c4c0bd021b6fd092998b783ef1b9f34c795a5
  preserved_instance_config: 9be1008582f732457d91cdcaf8c43356a5288ee622faf558802f579734c36ffe
  archived_candidate_a_not_installed: 6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c
  archived_candidate_b_not_installed: c1d57d2544a93e4c4f172ee3ecb6ff2659adb7650558957e5b0cfb7aa69ae21e
raw_log_reference: repository-controlled raw/EV-E3-TABLETEMU24-20260717-0001-*; manifest lists 16 raw files, three preserved external metadata files, and two host-only archived HAP references; repository access
verdict: blocked
reviewer: anthropic/claude-opus-4-8
reviewed_at: 2026-07-17T21:56:00+08:00
review_record: Independent review verified the authoritative full artifact manifest `d43bd4f23070eb1ce313ca0fc5fa541735de2d512fab6b48d6a752e6091b781c`, transcript `66f312c845b976bb861fbc88f5ac1929fe3865a7f38ac57486204d8d15ba43c2`, complete user-100/user-0 lists, three-scope direct vpndialog and Settings searches, identity, HDC lifecycle, noWindow/KVM logs, HAP noninstallation, and cleanup. 0 blocker/major; three minors retained: (1) the manifest, not narrative excerpts, is the authority for the complete raw-material set; (2) `phone_settings` is a shared internal Settings module name, not a phone-device inference; (3) BMS/Settings are registration-layer observations only, and the stopped boundary means no VPN runtime or HAP result is claimed. Verdict remains blocked and does not close E3.
```

`record_status: reviewed-pass` 表示独立审查已确认材料完整性、范围和 `blocked` 判定；它不表示 E3 通过。`verdict: blocked` 仍表示该精确 Tablet 目标元组缺少进入普通授权 UI 所需的系统组件，不改变 phone 0003/0004、2in1 0001、phone E3 未关闭、E4-E7 dependency blocked 或 E8 `CLOSED` 状态。审查保留三个 minor：artifact manifest 是全量材料权威来源；`phone_settings` 只是共享内部 Settings module 名而非设备推断；BMS/Settings 只覆盖注册层，停止边界不产生 VPN runtime 或 HAP 结论。

## 官方镜像与实例

使用的安装命令未指定代理、`force` 或替代镜像：

```text
/home/worker/harmonyos/command-line-tools/26.0.0.461/bin/Emulator -install -deviceType Tablet -osVersion 'HarmonyOS 6.1.1(24)' -imageRoot /home/worker/harmonyos/emulator-images
```

Emulator 报告下载 `2,002,569,925` bytes 到 `system-image/HarmonyOS-6.1.1/tablet_x86/` 并成功完成。实例创建命令使用 CLI 列出的精确 profile 名 `MatePad Pro 13`；实例元数据记录 `deviceType=tablet`、`PADEMU-FD00`、2880x1920、320 dpi、API 24、x86_64、software 6.1.0.125 与 `isHotBoot=false`。

启动命令使用 `coldboot`、`noWindow` 和独立 `hdcPort 10002`。完整 Emulator 日志在 `2026-07-17 21:35:57.009 +08:00` 明确记录 `KVM is supported and accessible`。首次 Beta HDC 3.2.0e 连接发生在 guest 启动未完成时并显示 `Offline`，第二次连接显示 `Connected`，随后显式 target 的 shell readiness marker 成功；离线与恢复均保留在 transcript，没有把首次离线写成成功。

## 只读门结果

| 查询 | 实际结果 | 判定 |
| --- | --- | --- |
| guest distribution/API/arch/device type/build | `HarmonyOS` / `24` / `x86_64` / `tablet` / `SP9DEVC00E16R1P1` | 目标身份确认 |
| `bm dump -h` | `-u` 仅支持 current user 或 user 0 | 后续查询使用合法范围 |
| `bm dump -a -u 100` | 58 个 bundle；`vpn\|vpndialog` 0 匹配 | current user 100 未注册 VPN dialog bundle |
| `bm dump -a -u 0` | 8 个 bundle；`vpn\|vpndialog` 0 匹配 | user 0 未注册 VPN dialog bundle |
| default direct `vpndialog` dump | `failed to get information` | default query 未找到 bundle |
| 同一 direct dump 加 `-u 100` / `-u 0` | 两者同样失败 | 两个合法显式 user 范围均未找到 bundle |
| Settings default / `-u 100` VPN 搜索 | 各仅两处 `ohos.permission.MANAGE_VPN` | 只有权限文本 |
| Settings user 0 VPN 搜索 | 无匹配 | user 0 无 VPN 注册匹配 |
| Settings ability/extension 搜索 | 无 `VpnServiceExtAbility` 或 VPN extension | 无可进入的授权组件注册 |
| A/B bundle 查询 | default、user 100、user 0 均不存在 | HAP 未安装 |

`MANAGE_VPN` 是 Settings 系统 bundle 的请求权限文本，不是 `VpnServiceExtAbility`，也不授权普通第三方应用使用该权限。本轮没有调用该权限或修改任何权限。

## 停止边界

预定条件要求 `com.huawei.hmos.vpndialog` 存在且其 BMS 信息包含 `VpnServiceExtAbility` 后，才允许把归档 A/B HAP 发送到该独立 target，并通过普通 UI 重放授权允许、拒绝、active stop、普通撤销和双应用冲突。两个条件均未满足，因此：

- 没有 HDC file send、`bm install`、`aa start` 或普通 UI 点击；
- 没有调用 `VpnConnection.create`、`protect` 或任何隐藏/system/debug/enterprise 能力；
- 没有生成测试 HiLog、layout、screenshot 或 fault 材料，因为测试阶段未开始；
- 没有把“组件不存在”改写成授权拒绝、应用失败、phone 或 2in1 结论。

A/B 的 host 归档哈希与 0003 一致，但它们只作为“若前置门通过时的候选输入”列入 manifest，不是本次被安装或运行的制品。

## 清理与保留

`Emulator -stop netbird_api24_tablet` 报告成功，Beta HDC server 随后停止。最终检查确认实例 `isRunning=false`，没有目标 Emulator/QEMU/HDC server 进程，`10002` 和 `8710` 无监听；本轮 `netbird-api24-tablet-install` 与 `netbird-api24-tablet-run` tmux session 已删除。

官方 image 与实例分别保留在：

```text
/home/worker/harmonyos/emulator-images/system-image/HarmonyOS-6.1.1/tablet_x86/
/home/worker/harmonyos/emulator-instances/netbird_api24_tablet
```

## 原始材料

完整清单见 `raw/EV-E3-TABLETEMU24-20260717-0001-artifact-manifest.txt`，其 SHA-256 为 `d43bd4f23070eb1ce313ca0fc5fa541735de2d512fab6b48d6a752e6091b781c`。该 manifest 是全量原始材料的权威清单；关键材料包括完整命令 transcript、两次 HDC 连接与 readiness 原样输出、current user 100 与 user 0 完整 bundle list、三范围 direct vpndialog 查询、三范围 Settings VPN 注册检索、guest identity、完整 Emulator/QEMU logs、HAP 未安装确认和最终清理记录。独立审查已完成，但本记录的 `blocked` verdict 不能用于任何阶段退出。
