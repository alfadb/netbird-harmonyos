# N1a Emulator 数据面 campaign 0009 pass 证据（20260831-0008 / pass）

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260831-0008` / `EV-N1A-EMU24-20260831-0008`（AUTH-N1A-EMU24-20260831-0008 granted，判据 frozen-r3，attempt initial / retry N/A）的完整两阶段测量：**`record_status: reviewed-pass`、`is_evidence: true`、`verdict: pass`——N1a 门首次完整 `reviewed-pass/pass`**（两轮记录级独立审查：第一轮 fail（3 major + 5 minor，测量本身经独立重放全部成立）→ 修订 `8844a37` → 第二轮 pass（0 blocker/0 major；3 项新 minor 均为 runner 卫生层、对已封签证据零影响）。）。runner 两窗判定、C9 页面条款、封签、清理全部机器验证通过。pair 已消费（`consumed-pass`），无后继 AUTH；N1a 门状态按治理收敛（见文末）。

## 两阶段实测（全部 machine-verified）

| 窗 | 进程 | marker | 吞吐 | c5 | C7/C8 |
| --- | --- | --- | --- | --- | --- |
| Phase A（`aa test`，N0 同路径） | testrunner | `N1A_RESULT\|verdict=PASS\|c5=not-triggered\|throughput_mibps=85.95` | 85.95 MiB/s（地板 5 的 17.2 倍） | not-triggered | pass（fd2=closed） |
| Phase B（`aa start`，普通 EntryAbility 独立 UIAbility） | entryability | `N1A_RESULT\|verdict=PASS\|c5=not-triggered\|throughput_mibps=63.33` | 63.33 MiB/s（12.7 倍） | not-triggered | pass（fd2=closed、fdset=1 观察值） |

- Phase A：`AA_TEST_RC=0`、`GUEST_RESULT_CODE=0`、ohosTest 交叉一致（缺陷 #8 修复生效）、probe-detail 重组 sha 一致、截图采集。**第二次连续 Phase A 全判据（含 ohosTest 交叉一致）pass**（0007 起算；按仅 app-marker 计为第 5 次——0004-0006 的 ohosTest 交叉 marker 因缺陷 #8 为 FAIL，不入连胜）。
- Phase B：force-stop 后独立 UIAbility、`aboutToAppear` 自动探针、4 次 poll 内 marker 出现、**C9 页面条款 pass**（`C9_PAGE_SCREENSHOT_YAVG=223.719` 非黑帧 + `pass_by_construction_single_pass_marker_plus_nonblack_page`）、两窗 `CROSS_PHASE_VERDICT_CONSISTENT=yes`、c5 双记 `not-triggered/not-triggered`（不折叠，如实双录）。Phase B **detail chunk 首次从 try 路径落盘**（7 片重组：握手 835.55ms、pump 61.685ms、背压 193.53ms、总 7521ms；`delivered_unique=512、corrupted=0、retransmit_rounds=128、eagain_count=0`——**0008 的 Phase B c5=fail 未复现**）。
- 封签：`final_exit_code=0`、`run_status=pass`、transcript 50206B `a7c93717…f2d`、sensitive scan pass。清理与残留核验完整（Emulator/hdc/staging/port 全清）。**封签口径披露**：`console_sha256` 为 Phase A 末尾的中途值（Phase B/teardown 后 console 继续追加，最终文件 sha 为 `7ff49774e702…`）；`aa_start_log_sha256=pending`——runner 已在 HEAD `8844a37` 将两者改为封签时刻计算，自下一 campaign 生效。
- 执行窗口 `2026-08-31T17:14:35+08:00` → `17:17:48`；code_sha `7fdb2ca`；libentry x86_64 `80d0ca8b…`（观测性增强版）。
- **判据版本披露（2026-08-31 记录审查要求）**：manifest 内 `criteria_revision=frozen-r2` 为 runner（n1a-emulator-run.sh:368）的**陈旧硬编码字面量**，与 gate plan 状态头/authorization/本记录的 **frozen-r3** 矛盾。本 campaign 的实际求值语义是 r3（`probe_fd_gate` 逐 fd 门 + 进程级计数仅入 `process_observation_only` 观察字段）；**不得字面采信封签内该字段**——若按 r2 的进程级精确相等求值，Phase A `task 14→10`、Phase B `fd 37→38` 两窗均会翻转 C7 为 fail。权威判据源为 `docs/n1a-gate-plan.md`；runner 字面量已在 HEAD `8844a37` 修复为 frozen-r3，自下一 campaign 生效。

## C5 形态数据（0009 的目的达成）

两窗背压形态完整恢复且一致：`eagain_count=0`（loopback 拓扑 errno 不出现的第 6/7 次实证）、`kernel silent drop` 路径下 **512/512 交付、0 损坏、128 轮有界重传、~195ms**。**0008 的 Phase B c5=fail 在本运行未复现**。按 `C5_PHASEB_RULING=insufficient-evidence` 的原限定，持有人对 0008 机制的解释（cold-start UI 饥饿候选）**不作为定论**，本记录不对其做处置性结论；0008 的 fail 字面保留、0009 的 pass 独立成立。**`C5_PHASEB_RULING` 预定的持有人终裁（measured-fact vs criteria-defect）截至本登记仍未出具**——形态数据已随本证据完整落盘，是否以该终裁为 N1a 门收口前置由治理方决定。

## 登记级观察（不设门、不阻碍 pass）

- **process_model 两窗均 `unknown`**：ets 侧实参已传（`runN1aProbe('testrunner'/'entryability')`），Rust/overlay 两侧源码处理链核对无误——label 未到达 Rust 的具体断点（d.ts/snapshot staging 同步候选）未定位；r3 C1-C9 聚合不含该字段，故不影响判定；其要求出处为 gate plan §流程 2 的实现层附注（`process_model=testrunner|entryability` 标注要求）——**该项实现层要求未满足**，登记为下一轮修复项。
- **bfreeze 事实修正（2026-08-31 记录审查纠正）**：`bfreeze.log` **实际已采样**（`attempt=3` 命中 3-poll 周期、7804 行、1040829 B；marker 在 attempt=4 出现）；manifest 记 `bfreeze_log_sha256=absent` 是 **runner 绑定块位置缺陷**（绑定在 `BFREEZE_LOG` 文件创建之前执行，n1a-emulator-run.sh:1874-1880），导致该 1MB 产物脱离封签链——封签缺口如实登记，本记录内补链：`bfreeze.log` sha256 `2617e4f15799113d93fb303e4c65d6b4a368ff30c2a8987a8577ab824929ca6a`（7804 行）。runner 侧绑定位置已在 HEAD `8844a37` 修复，自下一 campaign 生效。
- Phase B `fdset=1`（观察值：窗口内 1 个非探针 fd 出现——r3 下不设门，如实记录）。

## 门状态

- **N1a 门 `reviewed-pass` 待记录级独立审查**（本登记 `record_status: collected`；审查 0 blocker/0 major 后升级 `reviewed-pass`，`verdict: pass` 维持）。
- 范围：仅官方 API 24 x86_64 phone Emulator 的 BoringTun 0.7.1 数据面 × 双隧道回环泵；不主张 arm64 加载、物理设备、VpnExtension fd（N1b）、protect（N2）、management/relay（N3+）、产品实现。双向不外推。
- 后续：N1b（物理 VpnExtension fd 集成）为 N1 的下一子门，须独立治理/授权。
