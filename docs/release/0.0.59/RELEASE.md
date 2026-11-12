# 0.0.59 发布记录（2026-11-12）

## 0. 版本一句话

stale 自动恢复：手机在「无 kill 记录 / kill 记录对不上 marker」且状态文档长期不更新时直接启动扩展；PC 在「connector 显示 connected 但 signal 未注册、peers=0」的僵尸态持续 5 分钟且恢复无效时，进程内重启 connector。两端 UI 在对端未恢复期间如实显示。

## 1. 内容

### 手机（NetBirdReconnectPolicy.ets / pages/Index.ets）
| 项 | 说明 |
|---|---|
| stale-unconfirmed | 无任何 APP_KILLED 且文档连续 ≥180 s 不更新（RECONNECT_STALE_CONFIRM_MS）→ 扩展按已死处理，直接启动（不先 stop）。依据 2026-11-04 19:28 `InvalidKillId` 批量回收 4 h 无恢复（artifacts/diag-20261104/） |
| stale-mismatch（新） | kill 记录存在但 pid 对不上 marker（或 marker pid 不可用）且文档连续 ≥300 s（RECONNECT_STALE_MISMATCH_MS）→ 同样直接启动。依据 2026-11-12 采集：7 次文档丢失 ≥180 s 全被 vpn-pid-mismatch 跳过（artifacts/diag-20261112/phone/） |
| 预算 | 两条 stale 分支共用同一 attempts（3 次）/ 60 s 间隔 / 30 s 验证窗；`requestStart(true, true)` 跳过 stop-first（UI_START_DIRECT） |
| 观测（A2） | `ui_status_lost` 每丢失 episode 一条；`ui_auto_reconnect` 的 skip/failed/limit 由「每进程一条」改为「同 reason 每 10 min 一条」 |

### PC（NetBirdDataPlanePolicy.ets / NetBirdVpnExtensionAbility.ets / Index.ets）
| 项 | 说明 |
|---|---|
| 数据面升级 | 同一 stale episode ≥300 s（DATA_PLANE_RESTART_AFTER_MS）且至少一次看门狗恢复未愈合 → `restartConnectorForDataPlane()`：`connector_stop` → 受保护 mgmt socket → `connector_start_with_socket`（config/setupKey 缓存自 create 腿）→ TUN 重挂 → `recoverPhysicalNetwork()` 补 WG/DNS/ICE-mux/spare/signal 全套喂送 |
| 预算 | 2 次/episode（stale 清零复位）、最小间隔 600 s（DATA_PLANE_RESTART_MIN_GAP_MS）；耗尽后记 `vpn_dataplane_restart reason=restart-budget` 并继续常规恢复 |
| 机制边界 | 不销毁 extension（marker 不写 stopping）、不 ACK route-set（不消耗 core RECREATE_MAX 预算）、不与 N8 recreate 冲突（检查 recreateInFlight）。N8 recreate 不重启 connector，对「signal 注册失败」形态无效——这是选择 connector 重停的原因 |
| UI | `stateText()`：connected 且 peers 非空且全部 offline → 「已连接（对端恢复中…）」 |

## 2. 构建 / 签名 / 装机

- 构建：`bash client/build.sh`，BUILD SUCCESSFUL；unsigned HAP 7,726,334 B，sha256 `0c6ebec506b5f7c22456fec2d499c2ba1ee08d6e6fdb124e6f5b2937c677044f`；pack.info 0.0.59/59。
- 签名（artifacts/diag-20261112/signed/，SIGN-REPORT.md）：
  - 手机 ACL：`netbird-0.0.59-phone-signed.hap` 7,841,956 B，sha256 `254f2cdc4d26a99f24b03d366e80cf8e54df9f0bb4b6f48d5790d2aeb2e040f2`
  - PC MatebookFold：`netbird-0.0.59-pc-signed.hap` 7,841,956 B，sha256 `394a35e17b44558597477e2c6aedb62cbbedd892ec1d0850d8b88ff4813c9185`
  - 两份 verify-app success，内嵌 profile 与源 p7b 逐字节一致、device-id 逐台核对
- 装机：2026-11-12 13:43 双端 `install -r` 成功，bm dump 根级 59 / 0.0.59。
- selfcheck：全量 57 套件 0 失败（run-reconnect-policy 157/0、run-dataplane-policy 61/0、run-keep-alive 200/0 等）。

## 3. 真机验证

### PC：connector 重启升级——命中并完整恢复（2026-11-12 晚）
| 时间（CST） | 事件 |
|---|---|
| ~19:13 | 休眠唤醒进入僵尸态：connected 但 signal 未注册（与 0.0.58 的 2h11m 窗口同形态），旧恢复腿每 60 s 空转 |
| 19:18:48 | `vpn_dataplane_restart age=339 reason=no-signal` → connector-stop ok → connector-restart ok → `state -> reconnecting` |
| 19:45:23 | `login ok` → `connected`，网络地图 applied（peer_count=19） |
| 19:58:21 | 终态：`wg_sessions=12`、`signal_registered=true` ✅ |

全程零人工介入；对照 11-11 同形态在 0.0.58 挂 2h11m 且需手动打开应用。恢复时长 ~45 min 的瓶颈是已知平台侧受保护 DNS EPERM（0.0.49 起在案），不在本修复范围。证据：`artifacts/diag-20261112/verify/hit-PC-20261112-1921/`、`VERIFY-RESULT.md`。

### 手机：已部署待命中
- 装机后自动重连成功（系统补投 kill 记录 → marker 匹配 → `marker-open` 1 s 恢复）。
- `stale-mismatch` / `stale-unconfirmed` 分支待真实事件触发；命中判据：ui 日志 `reason=stale-mismatch`（或 `stale-unconfirmed`）+ 约 300 s（180 s）内新 `sess=`，且无 `UI_START_RESTART`。

## 4. 遗留

- 平台受保护 DNS EPERM：恢复时长的主导因素，根因平台侧未解；0.0.58 R0–R3 诊断字段继续收集。
- connector 重启路径系新代码，现网仅命中 1 次；若出现重启后仍不愈合且预算耗尽的场景，UI 停留在「已连接（对端恢复中…）」，需拉日志分析。
- 11-05 手机 B1 观察窗与 11-12 采集中 `not-foreground` skip（keep-alive 任务已停时）不在本轮放宽范围。

## 5. 回滚

`artifacts/build-20261027-0058/signed/netbird-0.0.58-{phone,pc}-signed.hap`（install -r 覆盖即可；降级通常被拒，需卸载重装并会丢配置/登录态——与 0.0.58 发布记录同口径）。
