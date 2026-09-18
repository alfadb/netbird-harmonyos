# N2-H arm 私有通道修复（2026-09-18）

一句话：arm 票据从应用不可见的 `/data/local/tmp/netbird-n2h-arm` 迁入应用自身
filesDir，复用凭据既有的跨进程 filesDir 机制；默认不 arm，普通启动清票据。
纯 ArkTS 两文件改动，core/Rust 零改动，无新权限、无 UI、无新增路径参数。

## 设备事实（0012 continuation，19:49:24–19:50）

- 手动/agent 布局定位 click 成功，UI state connected；ENTRY 凭据沿用 filesDir 登录
  成功；扩展 onCreate 与 watcher PATH 均在案；点击 used 1/3（remaining 2）。
- relayRequired=true / failed=0；当轮仅有 4 条 /32 路由，缺 hostname→IP 映射，
  relay /32 不能声称已证（与本修复无关，仅记录边界）。
- arm 检查输出 `N2H_COLLECTOR_ARM_SKIPPED|reason=arm-file-absent|path=
  /data/local/tmp/netbird-n2h-arm|accessError=none|attempted=true`；同刻 shell `ls`
  可列出该 100 B 文件。

## 根因边界（准确表述，不扩大）

- `accessError=none` = `fs.accessSync` 返回布尔 false，**不是**已证明的 ENOENT
  （13900002），权限/挂载视图过滤均未排除。
- 已证的仅有：应用沙箱对 `/data/local/tmp/netbird-n2h-arm` 的 access 探测为否，
  而同一文件 shell 可见。SELinux/挂载具体根因**未判定**。
- 修复不裁决根因：彻底退出对该共享路径的依赖（凭据通道已真机证明 filesDir 可用）。

## 新非秘密启动命令（仅诊断字段，无 setup-key/private_key 入 argv）

```bash
aa start -b cn.alfadb.netbird -a EntryAbility \
  --ps n2h_arm_options '{"posctl_port":41000,"probe_wait_ms":900,"peer_b2a_probe_id":"N2H-posctl-<hex>"}'
```

- EntryAbility 把该 JSON 原样写 `<filesDir>/netbird-n2h-arm`（与凭据 writeFile 同风格）；
  VpnExtensionAbility 用 `context.filesDir + '/netbird-n2h-arm'` 探测/读取，
  basename 与目录机制与凭据一致；文件存在仍是 arm 请求，既有 arm 选项
  （posctl_port/probe_wait_ms/peer_b2a_probe_id + shell_exclusions/gen/dns）原样送 native。
- 无效 JSON：**沿旧行为**——扩展记 `ARM_OPTS_IGNORED` 后按 §5.1 defaults arm，
  **并非拒绝**；这是保留的既有约定（default fallback），写入侧不做 JSON 校验。
- 新日志只含文件名/字节数/错误码，永不打印 payload；`bytes=` 为真实 UTF-8 字节数
  （平台 TextEncoder 计算，不用 UTF-16 code-unit 的 `text.length` 冒充）：
  `N2H_ARM_TICKET_WRITTEN|file|bytes` / `N2H_ARM_TICKET_WRITE_FAILED|file|code` /
  `N2H_ARM_TICKET_REMOVED|file|note` / `N2H_ARM_TICKET_REMOVE_FAILED|file|code|note` /
  `N2H_ARM_TICKET_CLOSE_FAILED|file|code`。

## 默认关闭与清理行为

- 不带 `n2h_arm_options` 的每次启动（onCreate/onNewWant）删除遗留票据，
  普通启动不会误开上一轮的诊断窗（`N2H_ARM_TICKET_REMOVED` 可 grep）。
- 写失败同样不留可用票据：open 失败本就无文件；open(TRUNC) 成功后 write 抛错
  或**短写**（fs.writeSync 返回值 ≠ TextEncoder 计得的 UTF-8 字节数——TRUNC 已把
  旧票清空，残留空/半票会被扩展按 defaults arm）→ 关闭 fd 后删除**仅这一个**
  诊断票据（note=failed-write-cleanup）；清理自身失败如实记
  `REMOVE_FAILED|code`，绝不伪称已删除。凭据/config/deviation 文件两种路径
  都不触碰。
- 每 ability 实例至多一次 native arm 尝试语义不变（失败不重试）。
  已知时序点（非冲突，如实记录）：若 VPN 已在跑且该实例已消费 arm 尝试，
  后补的票据不会武装该实例；需 stop→重新 connect 生成新扩展实例。
  票据不被扩展消费后删除：扩展实例重启仍可 arm（保持旧通道语义）。

## 改动与测试（host-only，全绿）

- 代码（2 文件）：`client/entry/src/main/ets/entryability/EntryAbility.ets`
  （新增 param/file 常量、writeArmTicketFile（含短写检测+失败清理）、
  removeArmTicket(note)、persistCredentialOverrides 重构）；
  `.../vpnextensionability/NetBirdVpnExtensionAbility.ets`
  （`N2H_ARM_FILE` 绝对路径 → `N2H_ARM_FILE_NAME` + `n2hArmFilePath()`，
  probe/read/skip 行用解析路径）。凭据文件读写流程逐字未改；core/gate/verdict/
  冻结 status JSON 未触碰。
- selfcheck：`run-exclusion-generation.mts` 新增 C1–C7（真 EntryAbility→filesDir
  票据→真扩展读同文件→native 选项；无参数清票+凭据逐字不动；无效 JSON 沿
  ARM_OPTS_IGNORED+defaults 旧约定；open 失败与 TRUNC 后写失败均不留可用票且
  清理独立记日志（C5/C5b）；清理失败如实 REMOVE_FAILED（C5c）；UTF-8 字节数
  反 UTF-16 冒充（C1b）；同实例第二 tick 零调用（C6）；源码 pin：扩展源无该
  绝对路径/凭据 dev-override 未动）；A/B 组字面量同步到 filesDir 路径，
  A3/B3 用 accessError=none（真机布尔 false 形态）。
  全套件 302/0（run 81、addr-shape 25、exclusion-generation 86、relay-exclusion 49、
  relay-flag 9、relay-status-log 3、route-validation 49）。
- HAP：`bash client/build.sh` 实测 exit 0（BUILD SUCCESSFUL；CompileArkTS 仅
  Index.ets 既有 getContext deprecation WARN，与本改动无关）；
  `VERIFY OK: libnetbird_core.so sha256 identical` / `pack.info apiVersion 26`；
  产物 `client/entry/build/default/outputs/default/entry-default-unsigned.hap`
  （4236055 B，sha256 6c7054c7ee4e4e2c…51267aac）。

## 真机待验（本轮不执行）

1. 上述 `aa start --ps n2h_arm_options` 后 connect：应出现
   `N2H_COLLECTOR_ARMED|...|posctlPort=...|probeWaitMs=...`（不再 arm-file-absent）。
2. 普通（无参数）启动后 connect：`N2H_COLLECTOR_ARM_SKIPPED|reason=arm-file-absent|
   path=<filesDir>/netbird-n2h-arm` 且无 ARMED（默认关验证）。
3. 诊断轮之后普通冷启动：日志含 `N2H_ARM_TICKET_REMOVED`，凭据登录不受影响。

## 0016 真机事实与等待/attempt 时序修复（2026-09-18 晚，ee9f3b3 基线）

- 0016 轮（21:42:45–21:42:57）：EntryAbility 输出
  `N2H_ARM_TICKET_WRITTEN|file=netbird-n2h-arm|bytes=99`，凭据登录成功；扩展
  21:42:54 输出 `N2H_COLLECTOR_NOT_ARMED|requestId=ui-…|reason=session-not-ready|
  deviceUp=true|ready=false|diag=true|is_evidence=false`；无 ARM_SKIPPED，清理
  成功。私有票据 probe 已过（该行在 readText 之前，不能声称已读取/解析票据）；
  但 WG 未 ready 时首个 watcher tick 即把该实例唯一的 arm 尝试消费掉——即使
  该会话此后变为 ready，该实例也没有 arm 窗口（0016 在 NOT_ARMED 后即收尾，
  未观测到后续状态，此处为条件式推断）。
- 时序修复（仅扩展侧 `maybeArmN2hCollector`）：文件缺失/不可读仍一次性
  SKIPPED（attempted=true，普通启动默认关）；文件存在而
  `!(wg.device_up && wg.ready)` 时改为**等待**——记一条去重的
  `N2H_COLLECTOR_WAITING|requestId=…|reason=session-not-ready|deviceUp=…|
  ready=…|attempted=false|diag=true|is_evidence=false`（NOT_ARMED 退役），
  不置 `n2hArmAttempted`、不调 native，沿既有 watcher 后续 tick 再检查；gate
  满足的首个 tick 才在读票据/调 native 前置位 latch。无新 timer/retry loop，
  `wg.device_up && wg.ready` 闸未降。
- native once 语义不变：每实例至多一次 native arm 尝试、失败不重试；变的是
  消费时点（gate 满足时才消费，而非首个 tick）。
- 该 WAITING→(ready)→ARMED 时序本轮仅 host selfcheck（A 组时序用例）覆盖，
  未真机验证；ARMED/窗口仍以「真机待验」为准。
