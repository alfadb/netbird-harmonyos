# N1a Emulator campaign 0003 consumed-blocked 记录（2026-08-31 执行）

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260831-0002` / `EV-N1A-EMU24-20260831-0002`（AUTH-N1A-EMU24-20260831-0002 granted，判据 frozen-r3，attempt initial）的执行事实：**campaign 在安装成功后的 `bm dump` 阶段被无超时的 HDC 命令挂死，由外层作业超时（exit 124）终止；runner 标 `defect`，按冻结判据聚合的意图应归类为环境类 blocked（Emulator/HDC 退化），本登记按 `consumed-blocked` 处置**。探针与判定均未被触达（无 aa test、无 HiLog 采集、无 marker），不构成任何测量事实。

## 执行事实

- 启动 `2026-08-31T14:18:11+08:00`；本次 Emulator 冷启动异常慢（boot readiness 30 次尝试才见 qemu.boot completed），构建/快照/安装全 pass（`INSTALL_VERDICT=pass`，双 HAP 成功）。
- 挂点：安装审计的 `hdc shell "bm dump -n $BUNDLE"`（transcript 中 `INSTALL_VERDICT=pass` 后无 `NORMAL_ENTRY_ABILITY_AUDIT` 行）——该调用**无 timeout 包裹**，在慢速 Emulator 上 HDC 命令无限等待；外层作业 600s 超时杀进程（`TRAP_EXIT_CODE=124 RESULT=defect`）。
- 清理链完整执行（staging/uninstall/Emulator stop/hdc kill/残留三查空/临时清理）；manifest 封签存在（`run_status=defect`、manifest `8690b36c…`）——但 `defect` 分类与冻结聚合的环境类 blocked 意图不符（runner 自身把外层 124 记为 defect 而非识别挂点）。

## 缺陷 #4（实现层，已修复）

runner 内多处 HDC 调用无 timeout（`bm dump` ×2 已修为 `timeout 30`；另有约 19 处裸 `hdc shell` 调用为低风险路径——多数在清理/采集段且已有外层 timeout 或 `|| true`）。修复已过 `bash -n` 与 `--selftest`。

## 处置

- 本 campaign `consumed-blocked`（环境类挂死，无测量事实消费探针/判据）；ID 不可复用。
- 缺陷 #4 修复后重测须新 evidence ID（用户授权）。
- 0002 的未解矛盾（overlay 守卫触发 vs host 数学矛盾）仍待下一次成功触达探针的 campaign 由诊断通道定位——本 campaign 未触达探针，不推进该问题。
