# N1a Emulator 数据面 campaign 0002 consumed-failure 记录（2026-08-31 执行）

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260831-0001` / `EV-N1A-EMU24-20260831-0001`（AUTH-N1A-EMU24-20260831-0001，判据 frozen-r3，attempt initial，单次执行已消费、禁止同 ID 重跑）的完整执行事实：**Phase A ohosTest 内 NAPI 调用抛出"probe verdict inconsistent with integrity counters"（overlay 一致性守卫触发），导致 `N1A_RESULT` marker 从未发射，campaign 按 C9 缺 marker 规则判 fail**；同时暴露**第三个观测性缺陷**：overlay 全部 12 处 `ThrowError` 均不携带 `detailJson`，探针真实字段值（ok/计数/criteria）在异常路径上被吞，根因无法从证据确定。

## 执行事实

- 执行窗口：2026-08-31T13:02:24+08:00 启动（manifest started_at）；Emulator 冷启动/就绪/安装/HiLog 全 pass；code_sha `cba35f5`；libentry x86_64 `18297fef…ba65`（r3 重构版）、aarch64 `b51bd2f2…6b76`。
- Phase A：`aa test` 执行 → overlay init → TestRunner onPrepare → **8.4 秒后** ohosTest 抛 `N1A_PROBE_TEST_RESULT|verdict=FAIL|detail=probe verdict inconsistent with integrity counters`（overlay `n1a_overlay.cpp:227` 的守卫：`ok==0 && (verified!=2000 || mismatch!=0 || lost!=0)`）→ `N1A_RESULT` 四字段 marker **从未发射**（探针在 NAPI 层 throw，未到 marker 输出）→ chunked `N1A_JSON` 亦未发射（`err instanceof N1aProbeError` 为 false——overlay 的原生 throw 不是 N1aProbeError）→ runner 按 C9 缺 marker 判 **fail**，Phase B 未执行（Phase A fail 短路）。
- **封签完整**（对比 0001 的修复生效）：base manifest 在测量前创建、`final_exit_code=0`、`run_status=fail`、`fail_reason=...`、transcript 46408B `4226f672…c6ef`、manifest `9950b85c…02a6`；清理完整、sensitive scan 待 manifest 尾段核实。
- **8.4 秒探针运行时长**本身是强信号：握手（~1.2s）+ C3 泵（~2s）+ C6 三间隔（3.3s）+ C5（~0.7s）+ C7/C8/清理——探针**大概率跑完了全部阶段**且 Rust 侧可能全 pass（守卫条件要求 `ok==0` 即全 pass）——但具体计数被观测性缺陷吞掉，**不假设**。

## 数学矛盾（如实登记，未解）

overlay 守卫条件 `ok==0 && verified!=2000` 与 C3 pass 蕴含 `verified==2000` 在 host 代码路径上矛盾（host 串行 13/13 过）；Emulator 上触发该矛盾的事实证明存在 host 不可复现的运行时差异（候选：ABI 布局/加载差异、快照 staging 边界、运行时字段值与源码推断不符——均未证）。**根因未证的原因是缺陷 #3（异常路径观测性）**：修复它之前任何 further campaign 都可能再次盲跑。

## 已定位缺陷（实现层，累计三个）

| # | 缺陷 | 状态 |
| --- | --- | --- |
| 1 | runner fail 路径不封签（0001） | **已修复**（0002 封签完整验证生效） |
| 2 | detailJson 被 hilog 行长截断（0001） | **已修复**（chunked N1A_JSON；但 0002 因 overlay throw 早于 chunk 发射而未产出） |
| 3 | overlay 12 处 `ThrowError` 不带 detailJson/字段快照（0002 新暴露） | **待修复** |

## raw 证据（docs/evidence/raw/，已封签）

`EV-N1A-EMU24-20260831-0001-{transcript.log(46408B), hilog-app-full.log, hilog-tag.log, aa-test.log, emulator-console.log, build.log, n1a-build.log, snapshot-prep.log, source-manifest.txt, manifest.txt}`。

## 判定与后续（须新治理，本记录不预授权）

- 本 campaign **consumed-failure**（C9 缺 marker → fail，字面记录不改写）。
- 修复方向（全部实现层，不触判据）：overlay 异常路径必须携带探针字段快照（ok/criteria/计数/c7/c8）——最小改法是 overlay 在每处 `ThrowError` 前把结构化字段发短 marker 或直接把 `probe->json` 附进异常消息；ohosTest catch 对原生 Error 也发射 detail chunk（或 overlay 保证 throw 前 hilog 直出）。修复后须新 evidence ID（用户授权）重测。
- 两次 campaign 的**正面测量事实**累计（0001: 加密数据面全过 93.10 MiB/s + 0002: 探针 8.4s 全程运行 + 守卫暗示 ok==0）都强烈指向数据面核心健康、失败全部卡在观测性/资源门环境敏感性——但这不替代新 campaign 的正式 pass。
