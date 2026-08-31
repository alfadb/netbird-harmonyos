# N1a Emulator 数据面 campaign 0001 consumed-failure 记录（2026-08-31 执行）

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260830-0001` / `EV-N1A-EMU24-20260830-0001`（判据 frozen r2 下的 attempt initial，单次执行已消费、禁止同 ID 重跑）的完整执行事实：**探针在 Emulator 上实测 FAIL（C7/C8），且 runner 在 fail 路径存在封签缺陷（manifest/seal 未产出）**。本记录如实登记两类事实，不将 FAIL 升格或降格；后续动作须新治理决定。

## 执行事实

- 执行时间：2026-08-31 11:46-11:48（evidence ID 日期 20260830 为治理命名日期；先例：G0 AUTH 20260829 于 20260830 执行）。
- 环境：官方 API 24 x86_64 phone Emulator（`netbird_api24_phone`、HDC `127.0.0.1:10000`），Phase A `aa test`（N0 同路径）。
- 冻结输入：判据 `docs/n1a-gate-plan.md`（criteria-frozen-r2）；libentry x86_64 `7725d9fc…dc5b`、aarch64 `ee2ad39c…2824`（仓内 commit `a1e7069`，code_sha 随 raw 保留）；BoringTun 0.7.1 checksum `15dd6a8a…`。
- **Phase A 探针实测**（唯一 `N1A_RESULT` marker，窗口隔离语义正确）：

```text
N1A_RESULT|verdict=FAIL|c5=not-triggered|throughput_mibps=93.10
```

探针 JSON criteria 段（ohosTest marker 携带，逐字）：`{"c1":"pass","c2":"pass","c3":"pass","c4":"pass","c5":"not-triggered","c6":"pass","c7":"fail","c8":"fail","c9":"pass"}`。

- **强正面测量事实**（在 FAIL 终态下仍然有效）：握手 1173ms 建立（stats 双侧确认）、C3 完整性全过、**吞吐 93.10 MiB/s**（冻结地板 5 的 18.6 倍）、C6 真实 persistent keepalive 路径全过、C1/C9 pass、背压信道 not-triggered（loopback 拓扑下与 host 一致，符合判据预判）。native 数据面核心的加密转发能力在 Emulator 上被强测量支撑。
- **FAIL 判据**：C7（fd/task T3==T0 精确相等）与 C8（清理回基线）fail。host 串行自测为 4→4 / 2→2 精确相等；Emulator 的 TestRunner 进程环境与 host 不同。**fd/task 具体数字未取得**：detailJson 被 HiLog 行长截断（观测性缺口，如实登记），无法区分「探针资源泄漏」与「TestRunner 进程环境噪声扰动 C7 快照窗口」两种解释——根因未证。
- **runner 封签缺陷**：`measured_fail()` 短路退出在 base manifest 创建之前，fail 路径未产 manifest/seal（与其自述"pass/blocked/fail 均产 sealed evidence"不符；N0-0001 同款缺陷形态）。清理链完整执行（staging 清除、卸载、Emulator 停止、hdc kill、残留三查空、临时清理），exit 0、`RESULT=fail`。

## raw 证据（docs/evidence/raw/，未封签状态保留）

`EV-N1A-EMU24-20260830-0001-{transcript.log(48412B), hilog-app-full.log(89716B), hilog-tag.log(907B), aa-test.log(1033B), emulator-console.log, build.log, n1a-build.log, snapshot-prep.log, source-manifest.txt}`；transcript 尾部 `TRAP_EXIT_CODE=0 RESULT=fail`。无 manifest.txt（缺陷事实本身）。

## 判定与后续（须新治理，本记录不预授权任何重跑）

- 本 campaign **consumed-failure**：测量事实有效登记，ID 不可复用；runner 封签缺陷与探针 JSON 观测性缺口（hilog 截断）均已定位，修复属实现层、不触判据。
- C7/C8 根因二选一未决：(a) 判据 C7 的进程级快照 locus 在 aa-test 环境对框架线程/fd 噪声敏感（判据缺陷方向）；(b) 探针在 OHOS 平台真实资源泄漏（测量事实方向）。**未证，不得假设**。
- 下一步候选（交用户/T0 裁决）：修复 runner 封签 + 探针 JSON 分段输出 → 判据持有人对 C7 locus 作环境敏感性裁决 → 若判据缺陷则修订判据（新审查轮）+ 新 evidence ID 新 campaign；若判据无缺陷则按单次测量纪律由治理决定 N1a 后续。本记录不构成任何方向的预授权。
