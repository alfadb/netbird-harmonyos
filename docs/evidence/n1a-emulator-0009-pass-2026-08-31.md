# N1a Emulator 数据面 campaign 0009 pass 证据（20260831-0008 / pass）

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260831-0008` / `EV-N1A-EMU24-20260831-0008`（AUTH-N1A-EMU24-20260831-0008 granted，判据 frozen-r3，attempt initial / retry N/A）的完整两阶段测量：**`record_status: collected`、`is_evidence: true`、`verdict: pass`——N1a 门首次完整 pass**。runner 两窗判定、C9 页面条款、封签、清理全部机器验证通过。pair 已消费（`consumed-pass`），无后继 AUTH；N1a 门状态按治理收敛（见文末）。

## 两阶段实测（全部 machine-verified）

| 窗 | 进程 | marker | 吞吐 | c5 | C7/C8 |
| --- | --- | --- | --- | --- | --- |
| Phase A（`aa test`，N0 同路径） | testrunner | `N1A_RESULT\|verdict=PASS\|c5=not-triggered\|throughput_mibps=85.95` | 85.95 MiB/s（地板 5 的 17.2 倍） | not-triggered | pass（fd2=closed） |
| Phase B（`aa start`，普通 EntryAbility 独立 UIAbility） | entryability | `N1A_RESULT\|verdict=PASS\|c5=not-triggered\|throughput_mibps=63.33` | 63.33 MiB/s（12.7 倍） | not-triggered | pass（fd2=closed、fdset=1 观察值） |

- Phase A：`AA_TEST_RC=0`、`GUEST_RESULT_CODE=0`、ohosTest 交叉一致（缺陷 #8 修复生效）、probe-detail 重组 sha 一致、截图采集。**第六次连续 Phase A 全判据 pass。**
- Phase B：force-stop 后独立 UIAbility、`aboutToAppear` 自动探针、4 次 poll 内 marker 出现、**C9 页面条款 pass**（`C9_PAGE_SCREENSHOT_YAVG=223.719` 非黑帧 + `pass_by_construction_single_pass_marker_plus_nonblack_page`）、两窗 `CROSS_PHASE_VERDICT_CONSISTENT=yes`、c5 双记 `not-triggered/not-triggered`（不折叠，如实双录）。Phase B **detail chunk 首次从 try 路径落盘**（7 片重组：握手 835.55ms、pump 61.685ms、背压 193.53ms、总 7521ms；`delivered_unique=512、corrupted=0、retransmit_rounds=128、eagain_count=0`——**0008 的 Phase B c5=fail 未复现**）。
- 封签：`final_exit_code=0`、`run_status=pass`、transcript 50206B `a7c93717…f2d`、sensitive scan pass。清理与残留核验完整（Emulator/hdc/staging/port 全清）。
- 执行窗口 `2026-08-31T17:14:35+08:00` → `17:17:48`；code_sha `7fdb2ca`；libentry x86_64 `80d0ca8b…`（观测性增强版）。

## C5 形态数据（0009 的目的达成）

两窗背压形态完整恢复且一致：`eagain_count=0`（loopback 拓扑 errno 不出现的第 6/7 次实证）、`kernel silent drop` 路径下 **512/512 交付、0 损坏、128 轮有界重传、~195ms**。**0008 的 Phase B c5=fail 在本运行未复现**——两运行合计的相容解释（依判据持有人的机制分析）：0008 的 fail 属 cold-start UI 饥饿经时间盒/会话寿命转化的偶发，非数据面背压缺陷；但按裁决纪律，该解释不回写 0008 记录（其 fail 字面保留），0009 的 pass 独立成立。

## 登记级观察（不设门、不阻碍 pass）

- **process_model 两窗均 `unknown`**：ets 侧实参已传（`runN1aProbe('testrunner'/'entryability')`），Rust/overlay 两侧源码处理链核对无误——label 未到达 Rust 的具体断点（d.ts/snapshot staging 同步候选）未定位；该字段为 r3 C7(3) observation-only、never gates，故不影响判定；登记为下一轮实现层修复项。
- bfreeze 采样 `absent`（pass 路径下 marker 4 次 poll 即现、3-poll 周期未触发首次采样——符合设计；bfreeze 只在等待期间采集）。
- Phase B `fdset=1`（观察值：窗口内 1 个非探针 fd 出现——r3 下不设门，如实记录）。

## 门状态

- **N1a 门 `reviewed-pass` 待记录级独立审查**（本登记 `record_status: collected`；审查 0 blocker/0 major 后升级 `reviewed-pass`，`verdict: pass` 维持）。
- 范围：仅官方 API 24 x86_64 phone Emulator 的 BoringTun 0.7.1 数据面 × 双隧道回环泵；不主张 arm64 加载、物理设备、VpnExtension fd（N1b）、protect（N2）、management/relay（N3+）、产品实现。双向不外推。
- 后续：N1b（物理 VpnExtension fd 集成）为 N1 的下一子门，须独立治理/授权。
