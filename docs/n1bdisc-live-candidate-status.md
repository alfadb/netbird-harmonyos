# N1BDISC Live 候选工作区状态（防重启交接）

> 本文件是工程交接快照，只记主进度与当前事实；治理历史见 `docs/n1b-disc-*` 既有文档，不在此展开。本文件不构成任何审查通过记录。创建：2026-09-06；更新：2026-09-07（新 ID 授权登记）、2026-09-11（候选快进合并入 main 后）、2026-09-11（第三对 gate 5 元组漂移退役）。

## 授权与边界（当前有效）

- 用户授权：**host-only** 补齐真实执行链 + 重新验证/签名/冻结；不接触设备。先前 host-only 开发授权继续可引用，但不自动扩大至正式门流程。
- 新 ID 已按本次申请授权（**未执行**）：`AUTH-N1BDISC-PHYS1API26-20260906-0002` / campaign `N1BDISC-PHYS1API26-20260906-0002` / evidence `EV-N1BDISC-PHYS1API26-20260906-0002`（治理命名日期固定 20260906-0002，不随实际执行日期变化；三 ID candidate 态，`consumed=false`/`reusable=false`，`is_evidence=false`），登记见 [evidence/n1bdisc-authorization-2026-09-06-0002.md](evidence/n1bdisc-authorization-2026-09-06-0002.md)。
- 本候选提交包含用户授权的实现与三 ID 登记；提交身份以 `git log` 为准，gate1 `code_sha` 待后续绑定。**该次授权范围原为**：三 ID 分配 + `n1bdisc/live-candidate` 本地提交 + 该登记落盘——不合并 main、不 push、不设备/tconn/list/参数查询/签名/新 freeze 执行/Live；**其后候选经用户授权快进合并入 `main` 并推送（`ea44b87`）**。候选仍未签名/新冻结/接触设备，旧资产原样；设备绑定与 Live 需各自**独立明确授权**。

## 工作区事实（已只读核实）

- 源仓库：`/home/worker/work/base/netbird-harmonyos`，`main` @ `ea44b874bbe0bf8152b0b9c61b9d8d3a4c8d70cf`（候选快进合并后；合并前 base 为 `d4bb2ef7839877759c9641744e9d1a2b2992a94e`），clean。
- 本工作区：`/home/worker/work/base/netbird-harmonyos-live-candidate`（独立 worktree），分支 `n1bdisc/live-candidate`，base 同上；全部候选实现**已提交（`ea44b87`）并快进合并入 `main`、已推送**。
- 旧冻结四件（ready-freeze 20260906）只读 sha256 校验全部 `OK`，保持原样，仅审计用途。

## 已实现（`spikes/n1b-disc-phys-hap/`，已提交并快进合并入 main `ea44b87`）

- `runner/` 新模块：`n1bdisc_transport_real.py`（RealHdcTransport/HdcStream，真 subprocess + 夹具）、`n1bdisc_freeze_manifest.py`、`n1bdisc_capture.py`、`n1bdisc_recording.py`、`n1bdisc_engine.py`、`n1bdisc_cli.py`（`--live`/`--dryrun` 统一 engine 入口）。
- UI 启动 API：`entry/src/main/ets/pages/Index.ets` 与 `module.json5` 改动（配 `selftests/test_ui_trigger.py`）。
- `selftests/`：新增 `test_transport_real`、`test_freeze_manifest`、`test_capture`、`test_recording`、`test_engine`、`test_cli`、`test_ui_trigger`，更新 `test_fsm` 与 `fixtures/`。

## 验证状态（仍不得标 live-ready）

- 回归已修：join 十值域回归关闭；CLI `join-bogus`/`d8b-bogus` 实测 exit 1（F8）钉死。
- Live pre/post integrity 已接入 engine，以 **invalid 优先收口**；DryRun 恒 `integrity={}`（live 与 dryrun 事实分离）。
- 审查链收口：anthropic 候选审查 1 blocker（PidOfVpn 错误当 absent）+ 2 major（日志失败中断 finally、absent 误洁净）+ EOF 静默——均已修；grok 复核原问题消解，另报 CLI happy 夹具并发生成 fault 的 flaky major——`test_cli` 以 `no_death=True` 于三个 happy 用例移除 `die_after_lines`/`crash_files`，从根因消掉 writer/SIGKILL 竞态（未改生产代码、未屏蔽 F8）；grok 同会话聚焦复核（agent `06c4431c-60f0-4090-a0a0-7eaea4bf818f`）：前述阻塞已消解，候选可进入**新 ID 申请与候选冻结准备**——不构成新 freeze 成立或 Live 授权。
- 测试证据：最终亲跑三 happy×3 轮全过、五新钉 5 passed、CLI `join-bogus`/`d8b-bogus` exit 1、全量 **204 passed**（77.15s）；此前执行层三 happy×15 pytest + 15 直调全绿、全量 3 轮 204 passed 为附加记录——**不声明绝无 flake**。
- gate12 PASS 仅是**历史离线校验**，不代表 live 就绪。

## 剩余 minor（审查判不阻塞，未声明已修）

- 半封签 finish 已置 `_closed` 后失败不可补封。
- 基线采样错误记录不对称。
- confirmation 仅比 model/software。
- run_state_root 作治理信任根。
- target 单形态脱敏。
- 超长无 newline 缓冲上界未实现。
- 其余以审查记录为准。

## join T0 裁定规则（engine 共同路径恒取）

- live：`join_exit_rc=None`、`join_blocked_registered=False`；**不从 DW_EXIT 推导**。
- POST 十值域**无条件校验**；`no-fact` 仅免比较。
- 无 POST 且进程活 → F9。
- 合成 join 与墙钟永不进 engine（live 事实边界）；`join-bogus` B4-b 反例由基线单测直调 `run_dryrun_campaign` 钉死。

## pair2 退役与新 ID

- 用户已明确退役第二 pair；旧 confirmation/ready-freeze 仅审计，**不沿用**。
- 退役记录：`/home/worker/harmonyos-signing/netbird-n1bdisc/reviews/pair2-retirement-exact-once.json`（含同名 `.sha256` sidecar），sha256 `5dfd19c39ef45780ad1bb6e42a6afb77d886851f3d9d7ebab1e25bcda0e8c652`，已实测一致。
- 候选已可进入新 ID 申请与候选冻结准备（须用户授权）；获批后才涉及设备/签名/新冻结。
- 新 ID 已获授权（**未执行**）：用户经 question_id=`n1bdisc-candidate-new-ids-local-commit` 选择「授权 ID 分配与候选本地提交」；依据为本次精确批准 + 实现阶段最终 grok 聚焦复核（agent `06c4431c-60f0-4090-a0a0-7eaea4bf818f`，见「验证状态」节）——不构成新 freeze 成立或 Live 授权。三 ID 与登记见「授权与边界」节及 [evidence/n1bdisc-authorization-2026-09-06-0002.md](evidence/n1bdisc-authorization-2026-09-06-0002.md)；实现代码已提交并快进合并入 `main`（`ea44b87`，合并前 base `d4bb2ef`），`code_sha` 待后续 gate 1 绑定届时 clean HEAD。

## 下一步（顺序，均需用户明确授权）

1. 候选 local 提交与合并：**已完成**——本地提交后经用户授权**快进合并入 `main` 并推送（`ea44b87`）**。第三对 **gate 1-3 已 pass（host-only）、freeze-3-v4 成立**（0 blocker / 0 major / 2 minor；code_sha `5a56ba3`；逐门登记见 [第三对授权登记「门序列执行登记」与「终态处置」](evidence/n1bdisc-authorization-2026-09-06-0002.md)）。**gate 4-5 已执行（2026-09-11，设备侧）：gate 5 元组漂移 → `blocked-tuple-drift` + 第三对 pair `retired-terminal`**——gate 4 恰一次内存级 `list targets`（`targets_count=1`，target 未输出未持久化）；gate 5 实测 `PLA-AL10 7.0.0.105(SP10C00E105R7P3)`，构建分支标签 `SP10C00E105R7P3` 与冻结值 `SP6C00E105R7P3` 不同（数字版本 `7.0.0.105` 相同；已由仓外两份 2026-09-06 记录交叉印证为设备侧真实变更）→ `consumed: false`（未测量/未 Live）、`reusable: false`、**无后继 AUTH**；仓外 blocked record `records/target-binding-confirmation-pair3-20260911.json`（sha256 `aa664446723139c388176d026c8c0be64927f695d6b4c3abb767d475c7fbba8e`）。**下一步：设备侧元组需用户重新决策**——是否再次 rebind 到新元组并以新三 ID 重走门序列（gate 1 起），或暂停；**gate 13 Live 须用户全新确认**（决议 §4.3.9）；本轮（2026-09-11 gate 4-5 执行后）不执行任何进一步门/设备/签名/新 freeze。
2. 候选冻结准备/新 freeze（仍需用户明确授权）。
3. 设备绑定：独立明确授权。
4. Live：独立明确授权。

## 常用命令（host-only，在本工作区根执行）

```bash
# 全套单测（回归已修；最近全量记录 204 passed / 77.15s）
python3 -m pytest spikes/n1b-disc-phys-hap/selftests/ -q

# 单文件（各测试文件亦支持直接 python3 执行）
python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_transport_real.py

# markdown 文档静态检查（已装可解析版本，无需安装；仓库根 .markdownlint-cli2.jsonc 仅关 MD013）
npx --no-install markdownlint-cli2 docs/n1bdisc-live-candidate-status.md

# 旧冻结只读完整性校验（审计用；workdir = 该 ready-freeze 目录）
sha256sum -c /home/worker/harmonyos-signing/netbird-n1bdisc/ready-freeze/ready-freeze-final-20260906.txt.sha256
```

Python 侧无 ruff/flake8/pylint/mypy 配置或安装；静态检查当前只有 markdownlint（文档）。

## 终态追加登记（2026-09-12，只追加不改写）

> 本节 2026-09-12 追加：只追加、不改写上文任何内容；只登记已发生事实及其出处，不构成任何新授权。以下 sha256 均为 2026-09-12 以 `sha256sum` 现算。

- **候选状态已被超越**：本文件所载工作区快照（含「候选可进入新 ID 申请与候选冻结准备」及「下一步」各条）已被第四对 pair（`AUTH-N1BDISC-PHYS1API26-20260911-0001`）的实际执行超越——gate 6-12 已执行（仓外 `reviews/gate12-dryrun-review-pair4-20260911.txt`，2026-09-11 19:30:40 CST，PASS）；gate 13 Live 终态 **fail**（StartEntry 后 300 s allow-box 内 0 个首 marker，E6 allow-deadline；独立审查 0 blocker / 3 major / 1 minor，`reviews/gate13-live-terminal-review-pair4-20260911.txt`，20:20:55 CST）；三 ID 终态登记 `verdict=fail`、`identity_status=consumed-terminal-fail`、`consumed=true`、`reusable=false`、`retry_allowed=false`、`successor_auth=none`（仓外 `records/terminal-disposition-pair4-20260911.json`，2026-09-11 20:24:49 CST，sha256 `bb46b3be9f0ec603e018c002b3b3a5a29a0db225aa45b810422236fd4cb1c23d`，sidecar OK）。逐门事实与出处见 [evidence/n1bdisc-authorization-2026-09-11-0001.md](evidence/n1bdisc-authorization-2026-09-11-0001.md) 末「终态追加登记（2026-09-12，只追加不改写）」节。
- **仍不得标 live-ready**：fail 终态之外另有冻结漂移——pair4 冻结 `code_sha 0616aa78` 与 main `33a987d` 已不一致，窗口内 9 个提交（自冻结以来共 10 个）全部改冻结实现 `spikes/n1b-disc-phys-hap/`，`e8e2cf9`（2026-09-12 01:33:19）引入的 MR4 不在冻结 MR 表（`docs/n1b-disc-gate-plan.md:453-456` 只有 MR1/MR1B/MR2/MR3），触 `:464` 硬停止条款——本候选及其后续实现**不得标注 live-ready**，后继工作须重新 freeze + 跨厂商独立重审。
- **下一候选须新授权**：三 ID 已被单次 Live 终态消费、无后继 AUTH；任何新 Live / 新 campaign 须用户**全新授权新三 ID** 并重走门序列（gate 1 起）。09-12 ad-hoc 真机验证（N 轮 `20260912T191913`，联调脚本自身判定 `verdict=pass`）**`is_evidence:false`，不是任何门的判定**，不构成门结论、不构成重开依据。09-11 20:18 → 09-12 19:36 真机活动经独立跨厂商 T0 裁定为「(b) 有授权主体实质同意、但无合规 AUTH 登记的越界执行 = 治理登记缺口」，事后登记见仓外 `~/harmonyos-signing/netbird-n1bdisc/records/retrospective-device-activity-20260912.json`（sha256 `90feab1f8b7af8f475a00f807e5e41246fa29ef76163a537b6152a52e98aef85`）、裁定归档 `reviews/t0-auth-boundary-ruling-20260912.md`（sha256 `080d4f44356d0fda8398c68c5fc8af8d4003e6669870c93a1d35efeb8490869a`）、取证报告归档 `diagnostics/auth-boundary-facts-20260912.md`（sha256 `73c07729719e279c16216c8bf8c6d1394970c5c6447db54f184d40bcaba5f8c6`）。
- **campaign 已终态关闭（2026-09-12 追加，1 行指针）**：N1BDISC campaign 登记终态关闭、pair4 freeze-4 不再约束工作区、工作区移交客户端整体实现阶段（N3-N6 作为实现里程碑 + 每门一次受治理验证）；登记见 [n1b-disc-handoff-20260902.md](n1b-disc-handoff-20260902.md)「N1BDISC campaign 终态关闭（2026-09-12 追加）」子节与仓B `~/harmonyos-signing/netbird-n1bdisc/records/campaign-terminal-n1bdisc-20260912.json`（sha256 `73ec96d4a3fa1785b0560f78611b21a4a936e89cb000247aacd962aa6bcef618`）。
