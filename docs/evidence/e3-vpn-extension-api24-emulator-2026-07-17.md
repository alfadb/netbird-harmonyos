# E3 VPN Extension API 24 Emulator 证据

最后核验：2026-07-17

本文记录两个普通第三方 bundle 在 API 24 x86_64 phone Emulator 上通过公开 VPN Extension 入口执行 E3 授权门的结果。被测源码只调用 `@ohos.net.vpnExtension.startVpnExtensionAbility`、`stopVpnExtensionAbility` 和 `@ohos.app.ability.VpnExtensionAbility` 生命周期；manifest 只声明 `type: vpn` 与普通权限 `ohos.permission.INTERNET`。

本记录不使用 Go、NetBird、PS4、`MANAGE_VPN`、`VpnConnection.create`、`protect`、`destroy`、API 26 observer、权限授予命令、隐藏服务、系统策略修改、system/debug/enterprise 绕过或真机。UI 输入仅通过 `hdc uitest` 模拟普通用户点击、滑动、按键和截图。

## 证据记录

```yaml
evidence_id: EV-E3-EMU24-20260717-0003
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8, R1, R3]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host; no native library was packaged
  sdk_api_syscap: SDK 6.1.1.125/API 24 ordinary UIAbility, ArkUI, VpnExtensionAbility and vpnExtension start/stop surface; no API 26 observer
  channel: N/A; unsigned debug research HAPs accepted only by this Emulator
code_sha: Git baseline 09fd047cf70f3ecbbb7941f3a2560b025574f799 plus uncommitted measured source archive SHA-256 7ff0be0f8c5382388f69bd95c7ef384dbc3e057ffa64401f1acbcdd6e97267b3
upstream_sha: N/A; no Go, NetBird or other upstream runtime code participated in the measured path
toolchain: Debian GNU/Linux 13 x86_64 Pod; host Linux 7.0.14-4-pve; Command Line Tools 6.1.1.290; SDK 6.1.1.125 API 24; Hvigor 6.24.3; ohpm 6.1.2.285; host Node.js 24.15.0; Beta Emulator 26.0.0.200; Beta HDC 3.2.0e; DISPLAY=:1; KVM enabled
working_directory: /home/worker/work/base/netbird-harmonyos
command: bash spikes/e3-vpn-extension-hap/e3-vpn-extension-emulator-run.sh; executed in tmux session e3-vpn-extension; every guest operation used explicit Beta HDC target 127.0.0.1:10000
input: clean-built unsigned A bundle cn.alfadb.netbird.e3vpna and B bundle cn.alfadb.netbird.e3vpnb; each package had one normal EntryAbility, one non-exported type vpn E3VpnExtensionAbility, only INTERNET permission, no native/test member, and was absent before installation; guest HiLog buffers were set to 16 MiB
expected: A normal UI start must show the system authorization dialog, a simulated ordinary allow click must produce real E3VpnExtensionAbility onCreate, fresh B must show the dialog and support an ordinary deny click with no B onCreate in the bounded window, normal stop of active A must produce onDestroy, Settings VPN management must permit ordinary revocation and a new request, and B start while A remains active must expose a real single-instance result; all five are required for pass
actual: the A UI emitted UI_START in three distinct PIDs 3148, 3322 and 3605, but on every request the system attempted com.huawei.hmos.vpndialog/VpnServiceExtAbility and BMS/AMS reported that the bundle and Extension did not exist; no authorization window appeared, each start promise remained pending for 10 seconds, and onCreate/onDestroy counts were zero. Fresh B PID 3857 reproduced the same result, so no deny button existed. A stop from PID 4307 while no VPN was active emitted UI_STOP, but its promise remained pending for 5 seconds and no onDestroy occurred. Conflict requests from A PID 4826 and B PID 5206 both emitted UI_START, but neither created an active Extension, so single-instance behavior was not adjudicable. The ordinary desktop Settings icon opened the real com.huawei.hmos.settings main page; its complete layout contained WLAN, System, Apps & services and other listed entries but no VPN management or More connections entry. Both bundles were uninstalled and all Emulator/HDC/test-port residue was removed.
started_at: 2026-07-17T20:03:03+08:00
ended_at: 2026-07-17T20:07:23+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, guest HiLog timestamps, HAP mtimes and current qemu boot-complete segment
artifact_sha256:
  application_a_hap: 6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c
  application_b_hap: c1d57d2544a93e4c4f172ee3ecb6ff2659adb7650558957e5b0cfb7aa69ae21e
  measured_source_manifest: 3fe78c5c4c582e09a0d0a206b3947ac0626a1a6339d5737d4f2c563e39190f6e
  measured_source_archive: 7ff0be0f8c5382388f69bd95c7ef384dbc3e057ffa64401f1acbcdd6e97267b3
  api24_vpn_extension_contract: 0566f6ce06b5d913208fa22859197dbe98398ca87db08fe44ccb4fab6c172c3a
  api24_vpn_extension_ability_contract: ead1dfa2ee50f57be83e5ac1c2c3c6907c858194ab0b30e67a2ef2905b094528
  api24_modulecheck_contract: ca59c4d6f99dc61e0b8789ec682b572f2787d0a4cbda34a8ac7d7799260964a8
  raw_artifact_manifest: c330c40e3047857133d623b26709c871a6620da9fac7c177a1c3fc8b6d1a872c
raw_log_reference: repository-controlled raw/EV-E3-EMU24-20260717-0003-*; artifact manifest lists 41 measured files; transcript SHA-256 56a13af9e9db5826b15476666b7c6692afe81bfd34c73d56d2708a89cd35e027, tag HiLog 7ba8acdc80371be2ff3ea647fa0ce429849967126823927037a8fd88f62a0996, fault list fe8544f99a8f7b4a8f5ab004c07aa2fa3209fd3460a4ebb58dccb5b3b6960ac5, Emulator console a32ddd244fce552c4d13e1fc0e3cd0af16682c8346b73649d512e9b9bf4639a3; repository access
verdict: blocked
reviewer: anthropic/claude-opus-4-8 independent E3 evidence review
reviewed_at: 2026-07-17T20:37:00+08:00
review_record: Reviewed three root causes: exact API 24 x86_64 phone Emulator image lacks com.huawei.hmos.vpndialog; ordinary public API has no bypass and remains pending; Settings has no ordinary VPN management entry, so required states are unreachable; hashes, layouts, logs, 0B/0M/minors reviewed; E3 remains blocked and does not close.
```

`EV-E3-EMU24-20260717-0003` 的证据记录已完成独立审查，状态为 `reviewed-pass`，但 `verdict` 仍为 `blocked`，不是 E3 通过记录。授权、拒绝、active stop、普通用户撤销和 active 冲突均没有形成真实可判定正反结果，因此 E3 保持未关闭；E4-E7 均为 dependency blocked，不开始。E0、E1-C 和 E2 是当前不依赖 Go 且合法可达的已完成项；E1 官方 Go 仍 blocked。E8 保持 `CLOSED`，真机执行禁令不变。

## API 与权限边界

目标 SDK 的 API 24 `.d.ts` 同时声明：

- `startVpnExtensionAbility(want): Promise<void>`；
- `stopVpnExtensionAbility(want): Promise<void>`；
- `VpnExtensionAbility.onCreate(want)` 与 `onDestroy()`。

API 24 没有 `createVpnObserver` 或授权结果 observer；该能力在 API 26 才出现。因此本探针只保存 promise 的实际 settle 行为、UI 和生命周期。五次 start 观察与一次 stop 观察均未 resolve、未 reject，故没有错误码可报告；把 pending 写成拒绝或内部错误会伪造结果。

两个最终 HAP 的打包后 `module.json` 都满足：

- `appPrivilegeLevel` 在安装后为 `normal`，`isSystemApp` 为 `false`；
- Extension 安装类型为 `502`，名称为 `E3VpnExtensionAbility`，进程后缀为 `:vpn`；
- 请求权限只有 `ohos.permission.INTERNET`；
- HAP 没有 `libs/`、TestRunner、Go、NetBird 或 PS4 member。

源码和打包审计均确认没有 `MANAGE_VPN`、`createVpnConnection`、`createVpnObserver`、`VpnConnection.create`、`protect` 或 `destroy` 调用。

## 场景结果

| 场景 | PID | 正常 UI 请求 | Promise 实测 | 生命周期 | 系统/UI 实测 | 判定 |
| --- | ---: | --- | --- | --- | --- | --- |
| A allow 尝试 1 | 3148 | 1 个 `UI_START` | 10 秒内 pending | 0 `onCreate` / 0 `onDestroy` | 授权组件缺失，layout/截图无弹窗 | blocked |
| A allow 尝试 2 | 3322 | 1 个 `UI_START` | 10 秒内 pending | 0 / 0 | 同一缺项重现 | blocked |
| A allow 尝试 3 | 3605 | 1 个 `UI_START` | 10 秒内 pending | 0 / 0 | 同一缺项重现 | blocked |
| 新鲜 B deny | 3857 | 1 个 `UI_START` | 10 秒内 pending | 0 / 0 | 无弹窗、无拒绝按钮，未伪造点击 | blocked |
| A stop 观察 | 4307 | 1 个 `UI_STOP` | 5 秒内 pending | 0 / 0 | A 从未 active，不能替代正常 stop | blocked |
| A/B conflict | 4826 / 5206 | A/B 各 1 个 `UI_START` | 均未 settle | 0 / 0 | A 从未 active；E3 未调用 `create` | blocked |
| Settings revoke | N/A | 桌面图标普通点击 | N/A | 无 active A | Settings 主 layout 无 VPN 管理入口 | blocked |

官方流程要求首次 start 由系统显示授权 UI，允许后系统再调用 Extension `onCreate`。本镜像在这一前置步骤即缺少 `com.huawei.hmos.vpndialog`，所以不能用三次普通 start 请求代替“三次 allow 成功”。三 PID 只证明阻塞可重复，不证明授权持久化或 VPN Extension 可用。

## 系统缺项

每个 A/B start 的完整系统 HiLog 都出现同类序列：

```text
ServiceExt: name:com.huawei.hmos.vpndialog VpnServiceExtAbility
BMSCommon: bundle not exist -n com.huawei.hmos.vpndialog
AMS: failed: com.huawei.hmos.vpndialog
BMSQuery: ExplicitQueryAbility no match ... VpnServiceExtAbility
BMSQuery: ExplicitQueryExtension size:0 ... VpnServiceExtAbility
```

与此同时，授权后 layout 仍只有普通 A/B 页面与 `Start requested` 状态，没有 `com.huawei.hmos.vpndialog` window；“`Start requested` 匹配 1”按 layout 节点计，不是授权成功次数。A 三轮和 B 一轮分别保存了点击前 layout、10 秒后 layout、截图及完整无筛选 HiLog。

这只能说明该精确 API 24 Emulator 镜像缺少公开流程所需的系统授权组件，不外推到 arm64、具名真机、华为商用 HarmonyOS 或其他 Emulator 镜像。

## Stop 与冲突边界

A 未获得授权，因而没有 active `VpnExtensionAbility`。普通 Stop VPN 按钮确实记录 `UI_STOP`，但 promise 在 5 秒观察窗内仍 pending，且没有 `onDestroy`。该观察不能满足“正常 stop active A”；force-stop 仅在每个 blocked 场景后用于创建新 PID，日志明确标为 `notUsedAsRevoke=true`。

冲突场景先让 A 正常 UI start 并保持 A 进程存活，再启动独立 B EntryAbility 并点击 B Start VPN。完整 HiLog 中 A/B 各有一个 `UI_START`，两个 promise 都未 settle，且没有任一 `onCreate`。由于 A 从未 active，无法判定平台单实例冲突。E3 按范围没有调用 `VpnConnection.create` 强行制造 create-level conflict；该层留在 E4。

## Settings 撤销入口

runner 先按 Home 键，再从完整桌面 layout 中定位 `clickable=true` 的 Settings 图标容器并模拟点击。以下三项断言全部通过：

- Settings 前后 layout 和截图哈希不同；
- 新 layout 存在可见 `bundleName: com.huawei.hmos.settings` window；
- 新 layout 存在可见标题 `Settings`。

完整 Settings 主列表包含 WLAN、Home screen & style、Display & brightness、Sounds & vibration、Notifications & status bar、System、Apps & services、Accessibility、Storage、Battery 和 Biometrics & screen lock；没有 VPN、VPN management 或 More connections。由于 A 也从未 active，普通用户撤权、`onDestroy`、活动状态消失和再次授权均无法执行。本记录不使用 force-stop 或 uninstall 替代撤权；二者只分别用于场景隔离和最终清理。

## Fault 与清理

fault 目录在首个 VPN 场景前已包含一份 `20:04:12` 的 `sysfreeze-com.ohos.sceneboard`，它发生在 Emulator 启动/解锁阶段、早于首个 `UI_START` 的 `20:04:55`。从 `BEFORE_SCENARIOS` 到三个 A、B、stop、conflict、Settings 和 `AFTER_SCENARIOS` 的九次列表中，该文件及全部历史文件保持相同，没有被测 A/B bundle 的新 fault。

最终清理完成：A/B 均卸载且 `bm dump` 不再返回包信息，staging 目录删除，Emulator 与 HDC 停止，10000/5555/8710 无监听，也没有残留 Emulator、qemu 或 HDC server 进程。`bash -n`、全仓 `git diff --check`、markdownlint、敏感模式扫描和禁止 API 复扫均通过。

## 原始材料

完整 41 文件及每个 SHA-256 见
`raw/EV-E3-EMU24-20260717-0003-artifact-manifest.txt`，该 manifest 自身 SHA-256 为 `c330c40e3047857133d623b26709c871a6620da9fac7c177a1c3fc8b6d1a872c`。关键材料如下：

| 文件 | 大小 | SHA-256 |
| --- | ---: | --- |
| `raw/EV-E3-EMU24-20260717-0003-application-a-hap.bin` | 34574 | `6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c` |
| `raw/EV-E3-EMU24-20260717-0003-application-b-hap.bin` | 34564 | `c1d57d2544a93e4c4f172ee3ecb6ff2659adb7650558957e5b0cfb7aa69ae21e` |
| `raw/EV-E3-EMU24-20260717-0003-source-manifest.txt` | 2308 | `3fe78c5c4c582e09a0d0a206b3947ac0626a1a6339d5737d4f2c563e39190f6e` |
| `raw/EV-E3-EMU24-20260717-0003-source.tar` | 61440 | `7ff0be0f8c5382388f69bd95c7ef384dbc3e057ffa64401f1acbcdd6e97267b3` |
| `raw/EV-E3-EMU24-20260717-0003-transcript.log` | 80962 | `56a13af9e9db5826b15476666b7c6692afe81bfd34c73d56d2708a89cd35e027` |
| `raw/EV-E3-EMU24-20260717-0003-hilog-tag.log` | 3608 | `7ba8acdc80371be2ff3ea647fa0ce429849967126823927037a8fd88f62a0996` |
| `raw/EV-E3-EMU24-20260717-0003-fault-list.log` | 20802 | `fe8544f99a8f7b4a8f5ab004c07aa2fa3209fd3460a4ebb58dccb5b3b6960ac5` |
| `raw/EV-E3-EMU24-20260717-0003-emulator-console.log` | 1862 | `a32ddd244fce552c4d13e1fc0e3cd0af16682c8346b73649d512e9b9bf4639a3` |
| `raw/EV-E3-EMU24-20260717-0003-settings-main-layout.json` | 128437 | `341429cc6ac4d80115ec1ed96ca0328b89ec445645e3670bc568cf922ed64ed2` |
| `raw/EV-E3-EMU24-20260717-0003-settings-main.png` | 296111 | `b1cad41b9d6ed9ad0b2fc0a70c5aabd0534d48356808941f1b6bd49a421077a9` |

## 失败与失效尝试

`EV-E3-EMU24-20260717-0001` 保留为首次 blocked 探索。它在 A PID 3343 中首次确认正常 `UI_START` 后系统授权组件缺失、promise pending 且无 `onCreate`；该尝试没有覆盖完整 E3 场景，不能替代 0003。

`EV-E3-EMU24-20260717-0002` 完成了 VPN 请求场景，但 Settings 自动化点击了不可点击的文字标签。其所谓 Home 与 Settings layout、截图分别具有完全相同哈希，后续审查将该 ID 标为 `invalidated/fail`。原始材料继续保留，失效说明为 `raw/EV-E3-EMU24-20260717-0002-review.log`，SHA-256 `9ae27881afe446129db68347a6d15c171096b4a28e9515b4ff52bd5ecd4bf026`。0003 使用可点击图标容器和 bundle/title/layout-change 三重断言，不复用旧 ID。

## 0004 同制品只读补充确认

`EV-E3-EMU24-20260717-0004` 是对 `0003` 的补充，不替代或删除 `0003`。它没有读取或修改被测源码、没有构建、没有使用 B HAP、没有真机，也没有改变系统属性、系统权限或应用权限。输入 A HAP 是从 `0003` 原始归档逐字节复制得到；两个文件均为 34,574 bytes，SHA-256 均为 `6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c`。

```yaml
evidence_id: EV-E3-EMU24-20260717-0004
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8, R1, R3]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: SDK 6.1.1.125/API 24; ordinary UIAbility and public vpnExtension start surface
  channel: N/A; unsigned debug research HAP accepted only by this Emulator
code_sha: Git baseline 09fd047cf70f3ecbbb7941f3a2560b025574f799; no source was read, changed, or rebuilt for this record
upstream_sha: N/A; no Go, NetBird, or other upstream runtime code participated
toolchain: Debian GNU/Linux 13 x86_64 Pod; Beta Emulator 26.0.0.200; SDK 6.1.1.125 API 24; Beta HDC 3.2.0e; DISPLAY=:1; x86_64 guest
working_directory: /home/worker/work/base/netbird-harmonyos
command: explicit Beta HDC target 127.0.0.1:10000; install archived A HAP; aa start EntryAbility; hdc uitest click visible start-vpn control; observe 60 seconds; all commands and output are in raw/EV-E3-EMU24-20260717-0004-transcript.log
input: archived 0003 A HAP only, copied to 0004 without rebuild; pre-install A absent; normal cold-booted API 24 Emulator; no B HAP, hidden service, property or permission operation
expected: ordinary Start VPN UI click must either settle the API promise and enter E3VpnExtensionAbility onCreate, or retain a fully captured blocked/fail observation through 60 seconds; BMS, Settings and system-HAP inquiries must remain read-only
actual: normal click emitted exactly one UI_START at 20:31:31+08:00. Across the complete 60-second window, START_PROMISE_SETTLED=0, VPN_ONCREATE=0, VPN_ONDESTROY=0, the A process remained present, the post-window layout retained Start requested, and no com.huawei.hmos.vpndialog layout node existed. Full HiLog records the system request for com.huawei.hmos.vpndialog/VpnServiceExtAbility followed by BMS bundle-not-exist, AMS failed, ExplicitQueryAbility no-match, and ExplicitQueryExtension size:0. The ordinary read-only BMS full bundle list had only the installed A as a vpn search match; bm dump -n com.huawei.hmos.vpndialog returned failed to get information. Settings full registration dump had only MANAGE_VPN as a VPN text match and no VPN ability or extension registration. Direct shell reads of /system, /system/app and /system/etc were permission denied; the guest find implementation also rejected -ls, so system HAP filesystem presence is recorded as not observable rather than absent.
started_at: 2026-07-17T20:30:16+08:00
ended_at: 2026-07-17T20:32:54+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, guest HiLog timestamps, and captured fault-list timestamps
artifact_sha256:
  application_a_hap: 6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c
  transcript: 7da7e5b6728eaa7bba5b791532d64ef7939e43970c3539520bb5e571e4a2c894
  full_unfiltered_hilog: 4ed0d2f6ff6ca40d56d4fed571f14f846cdb7a8672e436a84bf96f9a9447e0a2
  after_60_seconds_layout: df192c20694e460719b21e3014f32ecfa73d68482d60ed2027803d8cf144ac13
  after_60_seconds_screenshot: 9a260ae441db6a0eb6850c03fb7e21a8eaa3a3c92dd651b34c763a80082bb087
  raw_artifact_manifest: 9e828c63b4dc6f1498d6ca374d6b2e112e071e31269554930129dea8c5250f27
raw_log_reference: repository-controlled raw/EV-E3-EMU24-20260717-0004-*; 15 raw artifacts plus the manifest; every raw artifact name, size and SHA-256 is listed in raw/EV-E3-EMU24-20260717-0004-artifact-manifest.txt (1,793 bytes, SHA-256 9e828c63b4dc6f1498d6ca374d6b2e112e071e31269554930129dea8c5250f27)
verdict: blocked
reviewer: anthropic/claude-opus-4-8 independent E3 evidence review
reviewed_at: 2026-07-17T20:37:00+08:00
review_record: Reviewed 0004 HAP SHA-256 6b3cb636238188ebf9ebd767f2f0fa0c7beab2bdde6028fa6e4bc2da42084d6c, transcript 7da7e5b6728eaa7bba5b791532d64ef7939e43970c3539520bb5e571e4a2c894, full HiLog 4ed0d2f6ff6ca40d56d4fed571f14f846cdb7a8672e436a84bf96f9a9447e0a2 and 60-second layout df192c20694e460719b21e3014f32ecfa73d68482d60ed2027803d8cf144ac13; 60-second window, BMS/settings read-only checks, post-window and final cleanup reviewed; three root causes remain exact-image missing com.huawei.hmos.vpndialog, no ordinary public-API bypass, and no Settings VPN entry; 0B/0M/minors; E3 remains blocked and does not close.
```

### 60 秒观察结果

从 `20:31:31+08:00` 至 `20:32:31+08:00` 的完整窗口由 transcript 的 `OBSERVATION_WINDOW_BEGIN/END` 锚定。普通 `hdc uitest uiInput click` 点击布局中可见且可点击的 `start-vpn` 控件后，计数为 `UI_START=1`、`START_PROMISE_SETTLED=0`、`VPN_ONCREATE=0`、`VPN_ONDESTROY=0`、`VPN_DIALOG_REFERENCES=10`。截图与完整 post-window layout 分别为 `raw/EV-E3-EMU24-20260717-0004-after-60s.png` 和 `raw/EV-E3-EMU24-20260717-0004-after-60s-layout.json`；后者的 dialog bundle 匹配数为零而 `Start requested` 匹配数为一；该数值按 layout 节点计，不是授权成功次数。

这次延长观察没有把 pending 伪写成 reject。它仅再次确认该目标元组的公开系统授权链在缺少 `com.huawei.hmos.vpndialog` 时不 settle，且没有触发 VPN Extension `onCreate`。因此 `0004` 的实际 verdict 为 `blocked`，`record_status` 为 `reviewed-pass`；这里的 reviewed-pass 只表示证据记录审查完成，不是 E3 通过。E3 仍不能关闭，E4-E7 为 dependency blocked 且不开始；`0003` 的范围、材料和 blocked 判定仍保留。

### 只读系统补充

- `bm dump -a` 的完整输出保存为 `raw/EV-E3-EMU24-20260717-0004-bms-bundle-list-full.log`。安装期间的 VPN 检索只命中 A；`com.huawei.hmos.vpndialog` 不在该普通 BMS 列表中。
- `bm dump -n com.huawei.hmos.vpndialog` 的原样输出保存为 `raw/EV-E3-EMU24-20260717-0004-bm-dump-vpndialog.log`，结果为 `failed to get information`。
- `bm dump -n com.huawei.hmos.settings` 的完整已注册 ability/extension 输出保存为 `raw/EV-E3-EMU24-20260717-0004-settings-bm-dump-full.log`。VPN 检索结果仅为 Settings 自身的 `ohos.permission.MANAGE_VPN`；没有 VPN ability 或 extension 名称。该只说明本镜像的 Settings 注册表观察，不把该系统权限用于任何调用。
- 系统 HAP 目录探查只执行普通 shell `ls`/`find`。`/system`、`/system/app` 和 `/system/etc` 返回 `Permission denied`，部分候选路径返回 `No such file or directory`，而 guest `find` 返回 `bad arg '-ls'`。完整逐字输出位于 `raw/EV-E3-EMU24-20260717-0004-system-hap-vpndialog-search.log`；不以这些权限受限结果断言系统 HAP 中不存在组件。

### 故障与清理

fault list 中唯一当次时间段新增的 `sysfreeze-com.ohos.sceneboard` 时间戳为 `20:30:49`，早于 UI 点击的 `20:31:31`；列表没有 A bundle 的 fault。原始 Emulator console、完整无筛选 HiLog、fault list、布局和截图均在 `0004` 清单中。

A 被卸载并由 `bm dump` 确认为不存在，staging 目录删除，Emulator 停止。`ended_at: 20:32:54+08:00` 覆盖的是采集与测量记录窗口，不包含约 `20:34` 的测后清理；该测后清理已由 `raw/EV-E3-EMU24-20260717-0004-final-cleanup.log` 复核。首次清理后的诊断 `hdc list targets` 意外重新启动了 HDC server 并留下 8710 listener；该状态保留在 transcript。补充的 `raw/EV-E3-EMU24-20260717-0004-final-cleanup.log` 随后先记录该 listener，再执行 HDC 停止并确认 Emulator/QEMU/HDC 无进程，10000/5555/8710 均无 listener。没有提交。
