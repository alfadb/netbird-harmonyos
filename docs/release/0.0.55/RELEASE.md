# NetBird HarmonyOS 0.0.55 发布记录

本文记录 0.0.55 这个已发布版本的不可变标识、本版内容、验证证据、已知问题，以及升级与回滚方式。它是**版本记录**，不是上架就绪度门禁；门禁类结论见[0.0.49 发布就绪度门禁](../0.0.49/release-readiness.md)。

> **读之前先看这一条。** 0.0.49–0.0.54 六轮发布线此前没有独立发布记录，0.0.55 是第一次把这条发布线整体合回 `main`。所以第 2 节的「本版内容」是相对「上一次已合入 main 的版本」而言：从 0.0.54 升上来的读者只看第 2 节；从 0.0.49 或更早版本升上来的读者还要看第 6 节的六轮清单。

## 1. 版本与产物标识

### 1.1 源码与标签

| 项目 | 值 |
| --- | --- |
| 版本 / versionCode | 0.0.55 / 55 |
| 源码提交 | `2a07290d2469631e07fcf74d7fec52dbf12f7d02`（`chore(release): 0.0.55`） |
| annotated tag | `v0.0.55`，tag 对象 `6e67c6a29c61eab4b81b82dcbcd33c2c481bef0e`，已推送到 origin |
| 分支 | `main` 与 `rel/0.0.55` 同点，均为 `2a07290` |
| 包名 / APP ID | `cn.alfadb.netbird` / `6917608953844302724` |
| 目标设备类型 | `phone`、`2in1`（ARM64） |

`2a07290` 本身只做两件事：`AppScope/app.json5` 与 `build-profile.json5` 的 versionCode/versionName 升到 55 / 0.0.55，以及 selfcheck 的版本断言同步（`run-keep-alive.mts`、`run-tray.mts`）。

### 1.2 产物

| 产物 | 字节数 | SHA-256 |
| --- | ---: | --- |
| 未签名 HAP（`entry-default-unsigned.hap`） | 7414318 | `49041c3c63db7748db7032dc169d5f5d4ed651f9a300076c35bbf767927b7058` |
| PC 签名 HAP（`artifacts/n2h-055/pc-signed.hap`） | 7515035 | `3056c78c98fe71eae1bb1857c377d5b982fafe5fba189e0062a477bf123d7554` |
| 手机签名 HAP（`artifacts/n2h-055/phone-signed.hap`） | 7515032 | `7ee707c49fe9417f235d60110a7199e854fb37dda08f6ebe109956f1017fdfc0` |

未签名 HAP 的 `pack.info` 为 `apiVersion target=26 compatible=26 releaseType=Release`，版本 55 / 0.0.55 与 `AppScope/app.json5`、`build-profile.json5` 一致。

### 1.3 签名材料

- 工具链：SDK 26.0.0.821 的 `hap-sign-tool`（`/home/worker/harmonyos/command-line-tools/26.0.0.821/…`），E3 debug 材料。
- 两份 profile 各绑定一台设备，不能互换：PC 用 `NetBird HarmonyOS Debug MatebookFold.p7b`，手机用 `NetBird HarmonyOS Debug ACL.p7b`。两份均为 `type: debug`、bundle-name `cn.alfadb.netbird`、app-identifier `6917608953844302724`、profile 版本 2.0.0 / 2、issuer `app_gallery`，各自只带 1 个设备 UDID。
- 验签：两份 `verify-app` 均为 success；内嵌 profile 与输入 `.p7b` 逐字节一致；`verify-profile` 均 success（`verifiedPassed: true`、`type: debug`）。
- 这是**设备调试签名**，不是应用商店发布签名；签名结论只覆盖本机安装与 QA 使用。
- 注意版本号：签名工具链的 SDK 是 26.0.0.821，而设备端 `bm dump` 里另有 `compileSdkVersion 26.0.0.105`（构建产物自带字段）。两个版本号不是同一件事，不要互相印证。

## 2. 本版改了什么

四条改动：2.1–2.2 是「把历史补齐」，2.3（内含三项修复）与 2.4 是行为修复。

### 2.1 六轮发布线首次合回 main（`0413865`）

`main` 自 `rel/0.0.28` 之后没有再合并过发布线，0.0.49–0.0.54 的改动（peer-meta 上报、VPN 恢复与退避收敛、视觉改版、许可与发布文档等）此前只存在于发布分支。本次以 `33703ae` 为共同祖先合并，冲突只有 `THIRD-PARTY-NOTICES.md` 一处，按发布线已有的声明形式解决，并把过时的上游基线事实改成当前 pin。

**为什么**：不合并的话 `main` 读不出实际发布内容，后续任何从 main 出发的工作都会丢掉这六轮的修复。

### 2.2 上游 pin 从 `7914010` 对齐到 NetBird `v0.81.4`（`4001d94`）

协议副本由旧 pin `7914010`（2026-09-12）对齐到 NetBird tag `v0.81.4`（commit `7e791d3…`）。本轮实际换字节的只有 `management.proto`：

| 文件 | 本轮处理 | 与上游关系 | SHA-256 |
| --- | --- | --- | --- |
| `client/core/proto/management.proto` | 换成该 tag 的逐字副本 | 与上游一致（原带一处追加字段，现已删除） | `8916e1256cc32c586f2a8618cd1d2a0dcf79d8f736df3d8ebc45cb32cbb3a207` |
| `client/core/proto/signalexchange.proto` | 未改动 | 逐字节相同 | `9374413168551f7ded6c009dda2bfaa55e111bf138f2fe61780752a05c566ab9` |

**为什么**：旧 pin 落后于实际对端版本，且旧副本对 `management.proto` 做过一处本地追加（`PKCEAuthorizationFlowRequest.SessionExtend`，当初取自 commit `67c2041`）。对齐到 `v0.81.4` 后该字段由上游该 tag 原生提供，本地补丁可以整段删掉，副本重新回到「与上游逐字一致」。新增的 `certificateProofs` / `certificateChallenge` 字段只是协议面补齐，本仓没有对应业务实现（见 4.1）。

### 2.3 三个行为修复（`d9144fe`）

三项都先有能红的用例、再有修复，分别对应上游同形问题：

1. **Signal 注册等待加 10 s 上界**（对应上游 #8175）。此前服务端接受流却不发注册确认头时，注册会永久等待。现在等确认头的等待有 10 s 上界，超时走正常的重注册路径。
2. **relay pump 三条终态退出统一卸承载**（对应上游 #7490 同域）。三条终态退出统一调 `finish_transport_unavailable()`（transport down + `detach_all` + abort pending）。**为什么**：此前 carrier 会留在 WG seam 上，guard 误判为 `PartiallyConnected`，重试间隔从激进退化成每小时一次，表现出来就是「一直转圈但不再恢复」。顺带覆盖了 `relay_start` 失败后仍持有旧 stopped client 的路径。
3. **per-peer ICE sessionId**（对应上游 #7814）。`Body.sessionId` 5 字节，首次建会话时生成，只有本地 `drop_session` 才换新；远端换 ID 时重建一次且本机 ID 不变；远端不带 ID（旧版本对端）时保留原来的凭证变化判据。**为什么**：官方对端对不带 `sessionId` 的帧按「重复 offer」处理、直接跳过，所以本机本地重启对官方对端**不可见**（不重建 agent、不重发候选）；同时只看凭证变化也无法区分「对端重建了会话」和「换了对端」。

### 2.4 `commit_lane_open` 增加终态校验（`d9144fe`）

lane 提交时校验目标 client 是否已进入终态。**为什么**：pump 已终态退出后迟到的 lane 此前会 attach 到已 `Dead` 的 client 上，一直残留到下一次 `attach_client` 或 teardown。

## 3. 验证证据

### 3.1 构建与产物

| 检查 | 对象 / 命令 | 结果 | 证据 |
| --- | --- | --- | --- |
| Rust core 构建 | `cargo build --release --target aarch64-unknown-linux-ohos --offline --locked` | exit 0；4 条 warning（`unused_assignments`、`dead_code` ×2、`dropping_references`） | `artifacts/n2h-055/build.log` |
| 原生库校验 | `libnetbird_core.so` | ELF64 / AArch64 / DYN；`NEEDED` 含 `libc.so` + `libhilog_ndk.z.so`、无 `libc.so.6`；14/14 个 BoringTun ffi 冻结符号（T，已定义）；`.init_array` 构造器存在 | 同上 |
| BoringTun 供应链 | `boringtun-0.7.1.crate` | 包与 `Cargo.lock` 校验和一致（`15dd6a8a…`） | 同上 |
| HAP 打包 | `hvigorw assembleHap`（release、unsigned） | BUILD SUCCESSFUL；版本号、`pack.info`、`.so` 一致性三项 VERIFY OK | 同上 |
| HAP 摘要 | 7414318 字节 | `49041c3c…` | 同上 |

### 3.2 签名

| 检查 | PC | 手机 |
| --- | --- | --- |
| profile | `NetBird HarmonyOS Debug MatebookFold.p7b` | `NetBird HarmonyOS Debug ACL.p7b` |
| `signApp` | SUCCESS_CODE / sign app success | SUCCESS_CODE / sign app success |
| `verify-app` | success（codesign、权限签名、证书链均通过） | success（同上） |
| 内嵌 profile | 与输入 p7b 逐字节一致 | 与输入 p7b 逐字节一致 |
| `verify-profile` | success；`verifiedPassed: true`、`type: debug` | success；同左 |
| 产物 | 7515035 字节，`3056c78c…` | 7515032 字节，`7ee707c4…` |

日志：`artifacts/n2h-055/sign-pc.log`、`sign-phone.log`、`verify-pc.log`、`verify-phone.log`、`verify-profile-pc.log`、`verify-profile-phone.log`。

### 3.3 设备

| 设备 | 序列 | 升级路径 | 安装 | 启动 | 进程 |
| --- | --- | --- | --- | --- | --- |
| PC（SLM-W32 MateBook Fold） | `192.168.50.183:43833` | 0.0.54 / 54 → 0.0.55 / 55 | `install -r` 一次，输出 `install bundle successfully.` | `start ability successfully.` | `cn.alfadb.netbird`，PID 30721 |
| 手机 | `192.168.50.199:37193` | 0.0.54 / 54 → 0.0.55 / 55 | `install -r` 一次，输出 `install bundle successfully.` | `start ability successfully.` | `cn.alfadb.netbird` PID 59483，另有 `cn.alfadb.netbird:vpn` PID 59668 |

`bm dump` 前后对比：两端 `versionCode` / `versionName` 由 54 / 0.0.54 变为 55 / 0.0.55；`firstInstallTime` 未变（PC `1791723572583`、手机 `1789309788737`），只有 `updateTime` 前进（PC `1792580234754` → `1792858287749`，手机 `1792580276238` → `1792858303386`），说明是原地覆盖安装而非卸载重装。证据：`artifacts/n2h-055/{pc,phone}-bm-{before,after}.txt`、`install-*.log`、`start-*.log`、`ps-*.log`。

### 3.4 测试

| 套件 | 命令 | 结果 |
| --- | --- | --- |
| Rust 全量 | `cargo test --offline --locked` | 合并后 exit 0 全绿（`0413865` 记录）；修复轮记录最好一次 1124 passed / 0 failed（`d9144fe`） |
| selfcheck 主套件 | `run.mts` | 81 passed, 0 failed |
| keep-alive | `run-keep-alive.mts` | 200 passed, 0 failed |
| tray | `run-tray.mts` | 61 passed, 0 failed |

三个 selfcheck 的日志在 `artifacts/n2h-055/`（`selfcheck.log`、`run-keep-alive.mts.log`、`run-tray.mts.log`）。`cargo test` 的原始输出**没有**归档到 `artifacts/n2h-055/`，其结论来自提交记录与本轮执行记录；需要独立复核时只能重跑。全量套件本身不稳定，见 4.3。

### 3.5 这轮验证没有覆盖什么

- 设备侧只验到「装上、能启动、进程在」，**没有**做连接、路由、DNS、托盘或 UI 功能验收。
- 没有与真实官方 NetBird 服务端做端到端联调（见 4.2）。
- 没有实测回滚路径（见 5.2）。

## 4. 已知问题与风险

### 4.1 证书姿态检查未实现（功能缺口，会导致策略恒失败）

客户端不上报 `certificateProofs`。若管理端对某账户启用 Certificate 姿态策略（Posture Check），本客户端会**恒失败**。proto 字段已在副本中，但没有对应业务实现。官方 Android / iOS 客户端同样不支持这一姿态类型，因此该缺口是「与上游同形」而非本仓独有，但对启用了该策略的组织来说就是硬性不可用。

### 4.2 per-peer ICE sessionId 未与真实上游对端联调（互操作风险）

sessionId 的 wire 兼容目前只由「本仓 proto 副本 + 信封往返单测」保证，**没有**与真实官方 NetBird 0.81.x 对端做过互通验证；与旧版本（不带 sessionId）对端混部时，本机重启后的恢复路径也未验证。这一条不能读成「已兼容上游」。

### 4.3 client/core 全量测试套件在并行负载下不稳定（根因未证实）

多次运行全量 `cargo test`，每次各挂 1 个**互不相同**的用例：`wg_dns_seam`、`ice_mux`、`n2_ring`、`peer_conn::black_holed_stun`、`phone_log`；其中一次 lib 测试二进制以 SIGSEGV 退出。所有失败用例单独跑都能通过。根因**未证实**，正在进行排查。这条影响的是测试可信度：不能把「某次全量全绿」当作稳定信号，也不能把某次单点失败直接归因于当轮改动。

### 4.4 其他风险与待办

- **回滚路径未验证**：本轮只做了升级，没有实测降级安装（见 5.2）。
- **debug 签名**：两份产物都是 debug profile 签名，只用于设备 QA；商店发布需要另走正式签名与渠道流程。
- **构建 warning**：core 构建 4 条 warning（`stun_uris` 未读赋值、`ack_applied_route_set*` 未使用方法、`wg.rs` 的 `name` 字段未读、对引用调 `drop`），本轮未处理，不阻塞构建，但属于待清理项。
- **设备功能未验收**：见 3.5；「进程存活」不等于「组网可用」。

## 5. 升级与回滚

### 5.1 原地升级（`install -r`）

两台设备本轮都是原地覆盖安装，保留配置与登录态：

```bash
hdc -t <serial> install -r <签名 HAP>
hdc -t <serial> shell aa start -a EntryAbility -b cn.alfadb.netbird
```

- 成功判据是输出的字面量 `install bundle successfully.`，不是 hdc 退出码。
- PC 与手机必须用各自 profile 签出的包（见 1.3），**不能互换**。
- 升级后可用 `bm dump -n cn.alfadb.netbird` 核对 `versionCode` / `versionName`，并确认 `firstInstallTime` 未变（是覆盖安装而非卸载重装）。

### 5.2 回滚

回滚只能重装低版本 HAP，而系统对 versionCode 回退有限制：带 `-r` 的降级安装通常会被拒绝。实际回滚多半需要先卸载再安装，这会清除应用内的配置、登录态与设备凭据，需要重新配置管理地址并重新登录。本轮**未实测**回滚路径，也没有验证「卸载重装后能否恢复既有配置」，因此回滚方案只能作为方向，不能当已验证步骤。升级前如需保留退路，应先自行导出/记录配置。

## 6. 与上一版的差异范围（重要）

0.0.50–0.0.54 五轮没有独立发布记录（只有 `rel/0.0.5x` 分支），0.0.49 有一份发布就绪度门禁文档但不是版本记录。0.0.55 首次把这几轮合回 `main`，所以本记录实际覆盖的是 **0.0.49 之后六轮**的内容，而不是只有第 2 节那四条。

| 轮次 | 提交 | 主题 | 一句话内容 |
| --- | --- | --- | --- |
| 0.0.49 | `d1c481b`、`05b87f6` 等 | peer-meta 与发布资料 | 上报真实应用版本与 HarmonyOS OS 标识，补发布就绪度/设备验收/隐私与商店文案草案，记录上游 dashboard 请求 |
| 0.0.50 | `18810ab` | VPN 恢复收敛 | 给恢复等待加上界、跳过 EPERM 重试、回退到最近一次已知管理地址 |
| 0.0.51 | `f46c0db` | 应用交互修复 | 退出确认、紧凑返回控件、子页单一标题 |
| 0.0.52 | `933be32` | 视觉改版 P0 | 大标题、分组设置列表、搜索、计数 chip |
| 0.0.53 | `87ef99b` | 视觉改版 P1/P2 | 对比度、焦点、动效、空态、1:2 分栏 |
| 0.0.54 | `727d292` | 数据面恢复 | connector 仍报 connected 时，睡眠/唤醒后恢复数据面 |

因此：

- 从 **0.0.54** 升上来：只多了 2.1–2.4 四条（发布线合并、上游 pin 对齐、三个行为修复与 lane 终态校验），功能行为与 0.0.54 基本一致。
- 从 **0.0.49 或更早**升上来：还包含上表 0.0.50–0.0.54 五轮的 VPN 恢复修复、交互与视觉改版；这些没有单独的版本记录，上表是它们目前的唯一摘要。
- 0.0.49 的[发布就绪度门禁](../0.0.49/release-readiness.md)是上架前的检查清单（草案状态），不能当作 0.0.50–0.0.54 的变更说明。

## 7. 证据文件索引

`artifacts/n2h-055/` 下与本版对应的文件（均为本机工作区产物，未入库）：

| 文件 | 用途 |
| --- | --- |
| `build.log` | core 构建 + 原生库校验 + HAP 打包与摘要 |
| `sign-pc.log`、`sign-phone.log` | 两次 `signApp` |
| `verify-pc.log`、`verify-phone.log` | 两次 `verify-app` |
| `verify-profile-pc.log`、`verify-profile-phone.log` | 两次 `verify-profile` |
| `pc-profile.p7b` / `.json`、`phone-profile.p7b` / `.json` | 输入 profile 及其校验结果 |
| `pc-certchain.cer`、`phone-certchain.cer` | verify-app 导出的证书链 |
| `install-pc.log`、`install-phone.log` | 安装输出 |
| `start-pc.log`、`start-phone.log`、`ps-pc.log`、`ps-phone.log` | 启动与进程 |
| `{pc,phone}-bm-{before,after}.txt` | 升级前后 bundle 信息 |
| `selfcheck.log`、`run-keep-alive.mts.log`、`run-tray.mts.log` | 三个 selfcheck 套件 |
| `pc-signed.hap`、`phone-signed.hap` | 两个签名产物 |

未签名 HAP 本身不在 `artifacts/n2h-055/` 中，构建输出位于 `client/entry/build/default/outputs/default/entry-default-unsigned.hap`，摘要见 1.2。

### 未在本文中记录的内容

以下内容本文没有写，因为没有可用证据，写进去就是编造：真实服务器的连接/路由/DNS 验收结果、真实上游 0.81.x 对端的互通结果、回滚实测结果、商店发布签名与渠道结论、以及 0.0.50–0.0.54 每轮的完整变更明细（目前只有提交主题级摘要，见第 6 节）。
