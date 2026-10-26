# NetBird HarmonyOS 0.0.58 发布记录

本文记录 0.0.58 这个版本的不可变标识、本版内容、验证证据、已知问题，以及升级与回滚方式。它是**版本记录**，不是上架就绪度门禁；门禁类结论见[0.0.49 发布就绪度门禁](../0.0.49/release-readiness.md)。

> **读之前先看这一条。** 0.0.58 **尚未装机**。代码、构建与两份签名都已完成且可复算，但两端设备在当前网络下不可达（TCP Offline），安装与任何真机功能验证都**没有做**。本文第 3.3 节、第 4.1 节写的都是「待设备恢复后要做的事」，不能读成「已经验过」。

## 1. 版本与产物标识

### 1.1 源码与标签

| 项目 | 值 |
| --- | --- |
| 版本 / versionCode | 0.0.58 / 58 |
| 代码提交 | `a6d4cde`（`feat(core,ets): DNS 可观测性扩展、EPERM reprotect 诊断、快速失败与在线/离线门`） |
| 升版提交 | `30055c7`（`chore(release): 0.0.58`） |
| annotated tag | **未打**（0.0.58 未打 tag、未建发布分支，是否补待定；当前仓库 tag 只有 `v0.0.55`） |
| 包名 / APP ID | `cn.alfadb.netbird` / `6917608953844302724` |
| 目标设备类型 | `phone`、`2in1`（ARM64） |

升版提交 `30055c7` 相对 `a6d4cde` 只改版本号：`AppScope/app.json5`、`build-profile.json5` 的 versionCode/versionName 升到 58 / 0.0.58，以及 selfcheck 的版本断言（`run-keep-alive.mts`、`run-tray.mts`）。写本文前工作区干净（`main` = `30055c7`）。

### 1.2 产物

| 产物 | 字节数 | SHA-256 |
| --- | ---: | --- |
| 未签名 HAP（`entry-default-unsigned.hap`） | 7720722 | `5a77c68cbcf93f7b604217cc6d594255bf672f0b35e2bb639f0af291e5da04bc` |
| PC 签名 HAP（MatebookFold） | 7839375 | `58857437de541c18c52ddeae35c90dae46e86cdbc02544d50823b7842306933b` |
| 手机签名 HAP（ACL） | 7839376 | `4b72100fa465c833f9916fcf92eb68bada730b0355c34fef86458cdf0b5cd566` |

- 未签名 HAP 的 `pack.info` 为 `versionCode 58` / `versionName 0.0.58`，`apiVersion target=26 compatible=26 releaseType=Release`，与 `AppScope/app.json5`、`build-profile.json5` 一致。
- `libs/arm64-v8a/libnetbird_core.so` 在 unsigned、pc-signed、phone-signed 三个包内 SHA-256 相同，均为 `4410929429fb8c28ae6cec98f67ad3caaecea9fbea5491e06107b04f102a4bda`，与构建阶段对 llvm-strip 后 core 产物的校验值一致。
- 两份签名包大小只差 1 字节（7839375 / 7839376），两者各自嵌入不同的 profile；其中的 `libnetbird_core.so` 与未签名包同一份（sha256 相同，见上条）。

### 1.3 签名材料

- 沿用 0.0.55 / 0.0.57 同一套 E3 debug 签名流水线（`hap-sign-tool` + `HapSignLauncher`，签名器本体在 `signApp` 前先过 `sha256sum -c` 校验）。
- 两份 profile 各绑定一台设备，**不能互换**：PC 用 `NetBird HarmonyOS Debug MatebookFold.p7b`，手机用 `NetBird HarmonyOS Debug ACL.p7b`。两份均为 `type: debug`、bundle-name `cn.alfadb.netbird`、app-identifier `6917608953844302724`、`device-id-type=udid`、`allowed-acls=[]`，各带 1 个设备。
- 验签：两份 `verify-app` 均 rc=0，日志含 `Digest verify result: true, DigestAlgorithm: SHA-256` 与 `verify-app success`（codesign、权限签名、证书链均通过）；两份 `unzip -t` 均无错（11 个成员）。
- 内嵌 profile 与源 `.p7b` 逐字节一致（`cmp` 两份均 IDENTICAL），`verify-profile` 复核 `verifiedPassed=true`；签名前预检与签名后核对均确认 UDID 命中各自目标设备（PC / 手机各 1 个，本文不列出 UDID 全文）。
- 这是**设备调试签名**，不是应用商店发布签名；签名结论只覆盖本机安装与 QA 使用。

### 1.4 已做 / 未做一览

| 事项 | 状态 |
| --- | --- |
| 代码提交（`a6d4cde`）与升版提交（`30055c7`） | 已完成，在 `main` |
| 未签名 HAP 构建 + 产物摘要 | 已完成（3.1） |
| 两份设备签名 + 验签 + profile 核对 | 已完成（3.2） |
| client/core 全量 `cargo test` | 已完成，两轮 1179 passed / 0 failed（3.4） |
| 仓库根 selfcheck 57 套件 | 已完成，两轮 57/57、0 失败（3.4） |
| ArkTS 编译（hvigorw CompileArkTS） | 已完成，BUILD SUCCESSFUL（3.4） |
| 双端装机（`install -r`） | **未做**：两端 TCP Offline，待设备恢复后安装（3.3） |
| 真机连接/路由/DNS 功能验收 | **未做**（4.1） |
| R1 / R2 / R3 的现场验证 | **未做**（4.1） |
| 打 tag / 建发布分支 | **未做**（待定，4.4） |
| 回滚路径实测 | **未做**（5.2） |

## 2. 本版改了什么

四块改动：2.1 补齐 0.0.57 只做了 core 侧的 R0；2.2–2.3 是 DNS 链路的两个新行为（R1 诊断、R2 快速失败）；2.4 是核心侧的在线/离线门（R3）。

> **编号说明。** 0.0.57 的「R1 / R2」指 relay 线（relay 可用判定、relay 提前启动），0.0.58 的「R1 / R2」指 DNS 线（reprotect 诊断、快速失败）。两版编号撞名但不是同一件事，读旧记录时不要混。

### 2.1 R0 ets 半：诊断行能对上、标记行能落盘（`a6d4cde`）

三件事：

1. socket 供给回复携带 `net_id` / `protect_generation` / `protect_ms` 三个值。NAPI 上是**可选尾参**：旧壳只传 3 个参数时行为不变，不会因为版本不齐而失败。
2. `eperm` 行与 `supply` 行按 id 关联同一组值，故障期可以逐 socket 对齐「谁被拒、第几代、什么时刻」。
3. `VPN_PROTECT_MARK` 追加 `diagMilestone` 持久化。

**为什么**：0.0.57 只做了 core 侧（DNS EPERM 行补 id/fd、socket-supply result 行补 epoch），壳侧没有回传，两边日志只能各看各的，对不上同一条链路。第 3 点是实测倒逼的：标记行走 hilog I 级，日志不保留，实测 **0 行**落盘——诊断字段再多，不落盘就等于没有。

### 2.2 DNS R1：EPERM 后先试一次 reprotect（`a6d4cde`）

EPERM 后每条链只做一次 reprotect：新增 `poll` / `reply` 两个 NAPI，壳层等 200 ms（box），核心侧上限 250 ms；`ok` 就在**同一个 socket 上重发一次**，非 `ok` 不改既有降级顺序。

**为什么**：EPERM 有两种成因，处置完全不同——「保护标记丢了、可以重打」（重打一次就能救回这条链），和「受保护路径整体不通」（再等也没用）。此前没有手段区分，只能一律走降级。先花一次 reprotect 把两者分开，才能决定下一步是重发还是快速失败。

### 2.3 DNS R2：判定受保护路径不可用时快速失败（`a6d4cde`）

R1 判非 `ok`（受保护路径不可用）后：

- DNS 请求快速拒绝，30 s 探针窗口内不再逐条等超时；
- OS 段等待从 3 s 降到 500 ms；
- `mgmt` / `signal` / `relay` 不受影响；
- 恢复只走既有的网络变更与探针路径，**不新增通道**。

**为什么**：故障期实测 794 次 DNS 请求，每次纯等 3 s——即使按串行这种最保守的算法也接近 40 分钟，而且完全掩盖了「路径不通」这个事实。快速失败把这段纯等待压成一次明确结论，同时用探针窗口留出恢复入口。

### 2.4 R3：在线/离线门（`a6d4cde`）

- 新增 `connector_network_available` / `connector_host_resumed` 两个入口。
- 离线时 guard 不发起、也不消费重试预算（对齐上游 `guard.go:123-125`）。
- ets 侧接 `netLost` / `netAvailable` 事件，并补 `>15s tick-gap` 判定。

**为什么**：此前设备离线时仍在按固定间隔空耗重试预算，等网络回来时预算已经用光，反而拖慢恢复；挂起/恢复也没有显式事件，只能靠下一次定时器猜。离线不消费预算 + 显式恢复事件，是让重试预算花在「有可能成功」的时刻上。

## 3. 验证证据

### 3.1 构建与产物

| 检查 | 对象 / 命令 | 结果 |
| --- | --- | --- |
| HAP 打包 | `DEVECO_SDK_HOME=… bash client/build.sh`（release、unsigned） | rc=0；`hvigor BUILD SUCCESSFUL in 16 s 166 ms`（33 tasks，27 executed / 6 up-to-date） |
| 原生库校验 | `libs/arm64-v8a/libnetbird_core.so` | `VERIFY OK` 三项：与 strip 后 core 产物 sha256 一致、ELF64 ET_DYN AArch64、sha256 `4410929…04bda` |
| 版本一致性 | `pack.info` vs `build-profile.json5` / `AppScope/app.json5` | `VERIFY OK: version code=58 name=0.0.58`、`target=26 compatible=26 releaseType=Release` |
| HAP 摘要 | 7720722 字节 | `5a77c68c…5da04bc`；`unzip -t` 无错（10 个成员） |

构建日志：`/tmp/build-0058.log`（本机临时文件，未入库）。

### 3.2 签名

| 检查 | PC | 手机 |
| --- | --- | --- |
| profile | `NetBird HarmonyOS Debug MatebookFold.p7b` | `NetBird HarmonyOS Debug ACL.p7b` |
| `signApp` | `errCode=SUCCESS_CODE` / `sign app success` | 同左 |
| `verify-app` | rc=0；`Digest verify result: true, DigestAlgorithm: SHA-256`；`verify-app success` | rc=0；同左 |
| `unzip -t` | 无错（11 成员） | 无错（11 成员） |
| 内嵌 profile | 与源 p7b `cmp` IDENTICAL；`verifiedPassed=true`、`type=debug`、bundle-name `cn.alfadb.netbird` | 同左 |
| 产物 | 7839375 字节，`58857437…06933b` | 7839376 字节，`4b72100f…0b5cd566` |

验证日志：`artifacts/build-20261027-0058/signed/logs/sign-pc.log`、`sign-phone.log`、`verify/{pc,phone}/verify-app.log`。

### 3.3 设备：本版尚未装机

| 设备 | 序列 | 状态 |
| --- | --- | --- |
| PC（SLM-W32 MateBook Fold） | `192.168.50.183:43833` | `hdc list targets -v` 显示 **TCP Offline**；`hdc tconn` `[Fail]Connect failed` |
| 手机 | `192.168.50.199:37193` | 同上，**TCP Offline** |

可达性排查（2026-10-27 02:29 起）：两端 `ping -c 2` 均 100% 丢包；TCP 直连报 `No route to host`；本机是 K8s Pod（`eth0 10.98.0.254/24`），网关 `10.98.0.1` 可达、外网通，但**路由表里没有 `192.168.50.0/24`**，本机也没有可借道的 tun/wg 接口。对照昨晚的 `artifacts/build-20261026-0057/BUILD-RECORD.md`，同一目录下两端 `install -r` 均 rc=0——说明昨晚可达、今晨不可达，阻塞在网络链路而不是包本身。

因此本版**没有**做：安装、启动、`bm dump` 版本核对、进程存活、任何功能验收。安装步骤与成功判据见 5.1，**待设备恢复后执行**；在此之前不要把本文读成「已装机」或「装机验证通过」。

### 3.4 测试

| 套件 | 命令 | 结果 |
| --- | --- | --- |
| Rust 全量 | `cargo test --offline --locked`（client/core） | **1179 passed / 0 failed，两轮**（61 个 test 二进制） |
| selfcheck 全量 | 仓库根 57 个 `.mts` 套件逐套件执行 | **57/57、0 失败，两轮** |
| ArkTS 编译 | `hvigorw assembleHap`（经 `client/build.sh` 调用，任务 `entry:default@CompileArkTS`） | BUILD SUCCESSFUL（CompileArkTS 10 s 291 ms，总 16 s 166 ms） |

本版新增/扩展的用例（随 `a6d4cde` 提交）：`dns_r1_reprotect`（5 例）、`dns_r2_fastfail`（8 例）、`network_availability`（1 例）、`dns_r0_observability` 扩展、`obs_contracts` 更新。

`cargo test` 两轮的原始输出已归档到 `artifacts/build-20261027-0058/cargo-test-full-round1.log` 与 `cargo-test-full-round2.log`（两轮均为 1179 passed / 0 failed / 61 个 test 二进制）；selfcheck 逐套件输出未归档，需要时重跑。

### 3.5 这轮验证没有覆盖什么

- 设备侧**零覆盖**：装、启、跑、功能全没做（见 3.3）。
- R1 / R2 / R3 只有单测与静态校验，**没有**真机现场结论（见 4.1）。
- 没有与真实官方 NetBird 服务端做端到端联调。
- 没有实测回滚路径（见 5.2）。

## 4. 已知问题与风险

### 4.1 未装机：本版真机行为未验证

R1 / R2 / R3 都有只能在真机上判定的项，单测只能证明「逻辑分支按代码走」，不能证明「平台接受这个调用、现场不会误伤」：

1. **同一 dup fd 二次 protect 是否被平台接受**（R1）——若被拒，reprotect 结果恒非 `ok`，R1 会退化成「多等一次超时」。
2. **重发是否改变 egress**（R1）——同 socket 重发依赖「socket 仍绑定原路由」的假设，未验证。
3. **快速失败会不会误伤正常 DNS**（R2）——30 s 探针窗口内如果路径其实已恢复，DNS 可能被误拒。
4. **门禁 liveness 误拒率**（R3）——在线门负责判断网络是否可用，liveness（判活）指靠事件与计时确认链路仍活着；判定过严会把可用网络判成离线，导致不发起连接。
5. **`netLost` 事件顺序**（R3）——与 VPN 生命周期事件的先后顺序未验证。
6. **离线位可能卡死**（R3）——离线位就是记录「当前处于离线」的那个状态；若「唤醒后零回调」（既无 `netAvailable` 也无 tick），可能一直没有事件来复位它。这一条是本版最需要现场盯的风险。

另外，R0 ets 半的效果（三条诊断行能否对齐、`VPN_PROTECT_MARK` 是否真的落盘）同样只能在真机日志上确认——「编译通过 + 单测绿」不构成落盘证据。

### 4.2 R1 没有诊断开关，EPERM 风暴期有额外开销

R1 的 reprotect 每次都真的走一遍（壳层 200 ms box + 核心 250 ms 上限），没有开关可以关掉。平静期无影响；但 EPERM 风暴期每条链会多出 **≤500 ms**，链路多时这会叠加。当前按「先拿到现场数据」取舍，后续若要长期开启，应补开关或采样。

### 4.3 R2 的 EPERM 判定是字符串匹配（fail-open）

R2 仍用 `errno=1 (` 这个**字符串匹配**来认定 EPERM。平台若改措辞（或换错误包装），匹配会静默失效：判定不到 EPERM 就不触发快速失败，行为退回 0.0.57 的旧路径，且**没有任何日志或告警会提示这一点**。方向是 fail-open（不会误拒，只会失去本版收益），但意味着「本版有效」这件事在措辞变化后会悄悄不成立。

### 4.4 未打 tag、未建发布分支

0.0.58 目前只有 `main` 上的两笔提交，**没有** tag，**没有** `rel/0.0.58` 分支。是否补、补在哪个提交上，待定。仓库现有 tag 只有 `v0.0.55`（0.0.56、0.0.57 也没有 tag），因此「tag 指向哪个提交」目前不能作为 0.0.58 的标识来源；可复算的标识是本文 1.2 的三个 SHA-256。

### 4.5 其他

- **debug 签名**：两份产物都是 debug profile 签名，只用于设备 QA；商店发布需要另走正式签名与渠道流程。
- **设备功能未验收**：见 3.3、3.5；「构建绿 + 签名过」不等于「组网可用」。
- **回滚未实测**：见 5.2。

## 5. 升级与回滚

### 5.1 原地升级（`install -r`）

设备恢复可达后，用 `install -r` 原地覆盖，保留配置与登录态：

```bash
hdc -t <serial> install -r <签名 HAP>
hdc -t <serial> shell aa start -a EntryAbility -b cn.alfadb.netbird
```

- PC 与手机必须用各自 profile 签出的包（见 1.3），**不能互换**。
- 成功判据是输出的字面量 `install bundle successfully.`，不是 hdc 退出码。
- 升级后可用 `bm dump -n cn.alfadb.netbird` 核对 `versionCode` / `versionName` 为 58 / 0.0.58，并确认 `firstInstallTime` 未变（说明是覆盖安装而非卸载重装）。这一判据来自 0.0.55、0.0.57 的实测行为；**0.0.58 本轮没有实测**，因为设备不可达。
- 装机后请先按 4.1 的六项做现场核对，再进入功能验收。

### 5.2 回滚

回滚 = 重装 0.0.57。产物已在本机工作区：

- PC：`artifacts/build-20261026-0057/signed/netbird-0.0.57-pc-signed.hap`
- 手机：`artifacts/build-20261026-0057/signed/netbird-0.0.57-phone-signed.hap`

注意与限制：

- 系统对 versionCode 回退有限制，带 `-r` 的降级安装通常会被拒绝；实际回滚多半要先卸载再安装。
- 卸载重装会清除应用内的配置、登录态与设备凭据，需要重新配置管理地址并重新登录。升级前如需保留退路，应先自行导出/记录配置。
- 本轮**未实测**回滚（设备不可达），也没有验证「卸载重装后能否恢复既有配置」，所以 5.2 只能作为方向，不能当已验证步骤。

## 6. 与上一版（0.0.57）的差异

0.0.57 只包含 relay 线的 R1（relay 可用判定）/ R2（relay 提前启动）与 R0 的 **core 侧**；0.0.58 补齐 R0 的 **ets 半**，并加入 DNS 线的 R1 / R2 与核心侧的 R3。

| 工作项 | 0.0.57 | 0.0.58 |
| --- | --- | --- |
| R0 core 侧（EPERM 行补 id/fd、socket-supply 行补 epoch） | 有 | 有（另扩展 `net_id` / `protect_generation` / `protect_ms` 关联） |
| R0 ets 半（可选尾参回传、eperm 与 supply 按 id 关联、`VPN_PROTECT_MARK` 持久化） | 无 | 有（2.1） |
| relay 可用判定 / relay 提前启动（0.0.57 的 R1 / R2） | 有 | 保留，本版未改动 |
| DNS R1（EPERM 后一次 reprotect） | 无 | 有（2.2） |
| DNS R2（受保护路径不可用时快速失败） | 无 | 有（2.3） |
| R3（在线/离线门、`netLost`/`netAvailable`、`>15s tick-gap`） | 无 | 有（2.4） |

**对读者意味着什么**：从 0.0.57 升上来只需看 2.1–2.4；从更早版本升上来的读者，还应先看[0.0.55 发布记录](../0.0.55/RELEASE.md)（0.0.49 之后六轮的合并与三项行为修复）。0.0.56 没有独立发布记录（`docs/release/` 下没有该目录，只有提交信息）。

## 7. 证据文件索引

`artifacts/build-20261027-0058/`（本机工作区产物，未入库）：

| 文件 | 用途 |
| --- | --- |
| `BUILD-RECORD.md` | 本轮构建 / 签名 / 设备可达性阻塞的原始记录（安装、版本核实、进程三节为空，标注「待设备可达后补」） |
| `signed/netbird-0.0.58-pc-signed.hap` | PC 签名产物（7839375 B，`58857437…06933b`） |
| `signed/netbird-0.0.58-phone-signed.hap` | 手机签名产物（7839376 B，`4b72100f…0b5cd566`） |
| `signed/logs/sign-pc.log`、`sign-phone.log` | 两次 `signApp` |
| `verify/pc/verify-app.log`、`verify/phone/verify-app.log` | 两次 `verify-app`（含 Digest verify 结果） |
| `verify/{pc,phone}/profile.p7b`、`certchain.cer` | 内嵌 profile 与证书链 |

未签名 HAP 不在该目录：构建输出在 `client/entry/build/default/outputs/default/entry-default-unsigned.hap`，摘要见 1.2。构建日志在 `/tmp/build-0058.log`，为临时文件，未归档。

### 未在本文中记录的内容

以下内容本文没有写，因为没有可用证据，写进去就是编造：真机安装/启动结果、`bm dump` 版本与 `firstInstallTime` 核对、进程与功能验收、R1/R2/R3 的现场验证结论、真实服务器联调结果、回滚实测结果、tag / 发布分支结论。
