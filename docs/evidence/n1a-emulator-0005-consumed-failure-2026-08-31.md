# N1a Emulator campaign 0005 consumed-failure 记录（2026-08-31 执行）——chunk 传输被 hilog 前缀含入截断

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260831-0004` / `EV-N1A-EMU24-20260831-0004`（AUTH-N1A-EMU24-20260831-0004 granted，判据 frozen-r3，attempt initial）的执行事实：**探针 Phase A 全部判据再次实测通过（诊断通道与前次一致），但 chunked JSON 传输的每条 hilog 行被 ~500 字节上限截断**——384 字节 chunk + ~100 字节 hilog 时间戳/tag 前缀 + ~50 字节协议头 = 534 > ~500 上限，data 尾部 ~34 字节丢失，runner 的重组 sha 校验正确拒绝（`run_status=defect`），campaign 终止于 runner defect（未触达判定聚合）。

## 事实

- AA test RC=0、ohosTest marker `N1A_PROBE_TEST_RESULT|verdict=FAIL|detail=N1a probe detail criteria inconsistent`（ets 内 JSON 分段字段校验——detail 段在被截断的 chunk 里 c2 字段恰好落在丢失尾部）。
- tag hilog 实测行行长 416/500×5——**500 上限实证**；6 条 `N1A_JSON|part=0..5` 协议序号/total/sha 全齐完好，仅 data 尾部被截。
- 诊断通道（`N1A_DIAG` 直出）仍完整（不经 chunk 协议），前次探针实测结论（全部判据 pass）在本运行中无变化迹象——但本 campaign 因 runner defect 未完成判定，不作测量登记。

## 缺陷 #6（实现层，本次已修复）

ets `JSON_CHUNK_BYTES` 与 runner `N1A_JSON_CHUNK_MAX`：384 → **320**（100 前缀 + 50 协议头 + 320 data = 470 < 500 上限）。修复后 `bash -n` + `--selftest` 22 用例全过。探针零改动。

## 处置

本 campaign `consumed-failure`（runner defect 字面记录不改写；探针无测量消费——同 0003 逻辑本登记为 defect 终态而非测量 fail）。下一个 campaign（第 6 个 ID）预期首次完整走通两阶段流。重测须新 evidence ID（用户授权）。
