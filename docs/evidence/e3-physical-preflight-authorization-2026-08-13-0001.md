# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-13 · 0001）

最后核验：2026-08-13

本文登记用户（直接人类决策者）于 2026-08-13 显式批准的 **新** `E3-PHYS-PREFLIGHT` 物理设备执行授权（`AUTH-E3-PHYS1API26-20260813-0001`），**取代** [`AUTH-E3-PHYS1API26-20260810-0002`](e3-physical-preflight-authorization-2026-08-10-0002.md) 对 **Windows signing/build host（`ALFADB-V-WIN`）** 的执行绑定——执行 host 已从 Windows 迁移到 **当前 Linux Pod**。0002 的历史登记记录 **正文保留、不改写**；0002 创建的候选 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001` **从未 Live、从未消耗**，本授权 **沿用** 该未消费 pair（不新建 ID）。本文是当前唯一生效的授权登记。

> **状态注记**：本文已获用户最终批准（2026-08-13，聊天确认），`authorization_status: granted`。批准前本登记不构成可执行授权；批准后本登记是当前唯一生效的授权登记。

用户本次显式批准的完整事实（直接人类决策）：

1. **执行 host 迁移**：执行 host 从 Windows（`ALFADB-V-WIN`，`MINGW64_NT-10.0-26100` / `OS=Windows_NT`）迁移到 **当前 Linux Pod**（hostname `host-dev-alfadb-full`，`Debian GNU/Linux 13 (trixie)`，`x86_64`，Python `3.13.5`）。后续签名/构建/冻结/selftest/DryRun/Live 全部改由 Linux Pod 执行；Windows host 不再承担本 campaign 的执行绑定。
2. **runner 移植**：runner 由 PowerShell 改写为 **Python 3**（**语义等价移植**：顺序门、HDC 白名单 argv、`RUNNER_RESULT` 行、seal 六字段、双 contract 哈希、confirmation/review record schema 全部保留；移植完成后独立审查 0 blocker/0 major 并 **重新冻结三文件字节**）。原 PowerShell 三文件在仓内 **保留不改写** 作为历史输入。
3. **候选 ID 沿用**：候选 pair 沿用 0002 创建、**未消费** 的 `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`；`attempt=initial`、retry N/A。旧 pair（`20260808` 等）**不得复用**。
4. **signed A/B HAP 冻结 hash 复用**：用户已将 signed A/B HAP 复制到 Linux 侧，核查任务并行进行；本文 HAP hash 值写 **Windows handoff 登记值**（A `3a98ad68bfc6253fe10b37a262e0051267b3b3083e6943f8207d68482d00c244` size `106210`、B `1adfa9664e59e7a9dc3da6650a59972517f5262a9b8160f4b3f6416770da3c26` size `106212`）。核查完成若与登记值一致则冻结；不一致 → 停止并重新登记，绝不绑定发散 hash。
5. **13 步顺序门结构沿用**：0002 的 13 步顺序门结构原样沿用（见 `linux_gate_order`），仅执行环境与命令模板改为 Linux/Python。
6. **机器 fresh confirmation 改由 Linux Pod 观测**：`machine_fresh_confirmation` 由 Linux Pod 通过 runner 的 `-TargetBindingConfirm` 模式完成（HDC 白名单 `Version`/`TupleModel`/`TupleBuild` 三条机器核对）。
7. **设备连接纪律（Linux Pod）**：用户在 **Linux 终端本地** 执行 **恰一次** `hdc tconn <dynamic-ip:port>`（endpoint 值 **不进聊天、不落盘、不记录**），随后执行 **恰一次** 内存级 `hdc list targets`（恰一 token，设 process-scope `PHYS_1_TARGET`），再执行 `-TargetBindingConfirm` 三条白名单命令。**禁止重复 tconn、禁止重复 list targets**。
8. **attempt=initial、任一 gate 失败立即停止、不重试、不换 ID**；E8 保持 `CLOSED`。

本文是 **授权登记**，**不是** live campaign 证据、**不是** 设备实测记录。本登记（当前 Linux Pod，host-only 只读复测）期间 **不** 运行任何 HDC/设备命令、**不** 安装工具、**不** 生成 ready freeze。**用户就绪声明不替代机器 fresh confirmation**。E3 未关闭，E8 保持 `CLOSED`。

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260813-0001
supersedes: AUTH-E3-PHYS1API26-20260810-0002 # supersedes the Windows host execution binding of 0002 (execution host migrated from Windows ALFADB-V-WIN to the current Linux Pod); the 0002 historical record is preserved unchanged, not rewritten; the 0002-created candidate pair is carried forward (never Live, never consumed)
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: collected
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: not-run-authorization-registration
is_evidence: false
authorization_status: granted # user final approval 2026-08-13 via chat confirmation (see 用户批准记录 section at the end)
device_readiness: user-attested-ready # user readiness attestation does NOT replace machine fresh confirmation
machine_fresh_confirmation: pending # to be completed by the Linux Pod via -TargetBindingConfirm; this AUTH fixes attempt=initial and the carried-forward candidate pair (retry N/A)
plan_status_at_registration: authorized-awaiting-linux-ready-freeze
campaign_status: candidate-carried-forward-not-live-pending-audits # pair carried from 0002 (E3-PHYS-PREFLIGHT-20260810-0001 / EV-E3-PHYS1API26-20260810-0001), never Live, never consumed; audit-1/audit-2 both pending (hash-recorded out-of-repo)
independent_review_record: pending # ready Live / ready DryRun require a pass review record (record_kind=e3-ready-freeze-review, is_evidence=false) with an out-of-repo JSON + .sha256 companion
target_tuple:
  distribution: HarmonyOS
  device_model: PLA-AL10
  device_alias: PHYS-1
  full_system_build: PLA-AL10 7.0.0.100(SP8C00E32R7P2) # HDC binding; Settings manual 7.0.0.100 (SP8C00E32R7P2patch09)
  api: "26"
  kernel_architecture: aarch64
  app_abi: arm64-v8a
  sdk_api_syscap: 6.1.1.125 / API 24 / SystemCapability.Communication.NetManager.Vpn
  channel: ordinary-development-signing-only
hdc_target_reference: controlled external mapping; repository alias PHYS-1 only # single user-local hdc tconn + single memory-only hdc list targets host-prep exception (see below)
host_prep_target_mapping:
  tconn: "hdc tconn <dynamic-ip:port>" # ONE execution only, user-local in the Linux terminal; the endpoint value never enters chat, never persisted, never recorded
  list_targets: "hdc list targets" # ONE execution only, memory-only, after the single tconn, before any TargetBindingConfirm
  scope: memory-only # output token read in memory only; never written to file/log/repo, never echoed
  acceptance: exactly one target token # 0 / multiple / malformed => STOP, no guessing, no selection
  persistence: "PHYS_1_TARGET" set as process-scope environment variable only
  privacy_boundary: no endpoint/token output, no endpoint/token persistence, no endpoint/token in any record/freeze/evidence; runner HDC whitelist is NOT expanded by this exception (tconn/list targets are host-prep operator steps, not runner commands)
  consumed_authorization: false # NOT yet consumed: to be consumed exactly once (future) by the operator at the mapping gate on the Linux Pod; this registration does not run it
repo_bytes:
  powershell_historical_input: # original PowerShell trio kept in-repo unchanged as historical input (never rewritten)
    runner_sha256: e1da598d8cdf6bad3d243e393357c03ef40550ebab207714797d53e5a388ab41 # spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-campaign.ps1
    freeze_example_sha256: 5ea7acda269873a5d0ba2af2c0396ee6e3e62b9cf0990d83ddc55aadb49af62d # e3-phys-preflight-freeze.example.json (intentionally blocked)
    selftest_sha256: 28c26b72b7ff9adaddc0eb734a4441f2e7ed4e16435af6f08851c03cf3a5d220 # tests/e3-phys-preflight-runner-selftest.ps1
  python_port: frozen # Python 3 semantic-equivalent port completed: independent review 0 blocker/0 major (dual reviewer: openai/gpt-5.6-sol semantics closure + xai/grok-4.6 authorization boundary, 2026-08-13), three-file bytes re-frozen below; the PowerShell trio remains in-repo unchanged as historical input
    runner_sha256: 48a25e40442a98e985d5498b7a5c509b0ab62758c4da14b14a4866ab7916e4a3 # e3-phys-preflight-campaign.py
    freeze_example_sha256: cf9080aa5e050897f8f9b182dfc023c9f5f69d61b9ca18e8b7af7a25ad81263d # e3-phys-preflight-freeze.example.json (authorization_id updated to this AUTH)
    selftest_sha256: c70f99b5610dffbf6b838dbcc78b99b4deab7ddf31228f3b4a1bec4dd1d83e70 # tests/e3-phys-preflight-runner-selftest.py
    golden_evidence: pwsh 7.6.4 ConvertTo-Json golden 127 samples (126 comparable) zero diffs; out-of-repo $HOME/migration/jsoncompat-golden.txt
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260810-0001 # carried forward from 0002 (created by AUTH-E3-PHYS1API26-20260810-0002, never Live, never consumed)
  evidence_id: EV-E3-PHYS1API26-20260810-0001
  attempt: initial # this AUTH fixes attempt=initial with retry.basis/infrastructure_reason=N/A; the generic infrastructure retry branch never applies to this AUTH path
  identity_status: pending-two-consumption-audits # carried pair: audit-1 (after trusted refs/bundle sync, before any TargetBindingConfirm) and audit-2 (after the final ready freeze, before selftest/DryRun/Live); both hash-recorded out-of-repo
  live: false
  consumed: false
  note: candidate pair carried forward from 0002 (created by AUTH-E3-PHYS1API26-20260810-0002, never Live, never consumed). The OLD pair E3-PHYS-PREFLIGHT-20260808-0001 / EV-E3-PHYS1API26-20260808-0001 is consumed (external sealed blocked evidence EV-E3-PHYS1API26-20260808-0001, see legacy-pair-consumption-audit below) and must never be reused; the 20260807 pairs (consumed-blocked) and 20260806 pairs (superseded-unexecuted) are likewise never reusable. legacy-pair-consumption-audit (COMPLETED in 0002): out-of-repo audit log D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260808-0001/audit/id-consumption-audit-1.txt, SHA-256 b530c438769e65390ee9065f918d1ea550a8f9745c61516b6899ff90c736e9c7, run 2026-08-10T10:43:09+08:00, confirms the OLD pair is occupied (never reused). new-candidate-id-consumption-audit-1 (PENDING, for the carried pair): not yet run; it is audit-1 in the linux_gate_order below; fixed out-of-repo log path $HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-1.txt (+ .sha256). new-candidate-id-consumption-audit-2 (PENDING): audit-2 below; fixed out-of-repo log path $HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-2.txt (+ .sha256). Any audit run against the carried pair is separate from the completed legacy-pair-consumption-audit; ANY occupying live/evidence/seal record => STOP and await user-authorized new IDs, never silently switch or rewrite records
linux_gate_order:
  - sync-trusted-refs-bundle # git fetch + checkout frozen HEAD, clean worktree check before/after signing/build/freeze/selftest/DryRun/Live
  - candidate-id-consumption-audit-1 # carried pair E3-PHYS-PREFLIGHT-20260810-0001 / EV-E3-PHYS1API26-20260810-0001 must be unoccupied; hash-recorded; does NOT repeat the legacy pair audit; fixed log: $HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-1.txt (+ .sha256)
  - blocked-confirmation-freeze-static-review # independent_review_ready=true = contract/roles statically ready, NOT device/execution readiness; no review record required on the blocked confirmation freeze
  - host-prep-device-connect-mapping # user-local hdc tconn exactly once (endpoint value ephemeral, never in chat/persisted/recorded) + hdc list targets exactly once (memory-only, exactly one token, process-scope PHYS_1_TARGET); runner HDC whitelist unchanged
  - target-binding-confirm # machine fresh confirmation: 3 whitelisted probes (Version/TupleModel/TupleBuild), out-of-repo double-file record (JSON + .sha256 companion), never enters campaign roots
  - ready-freeze-draft-binds-record-hash # new out-of-repo ready freeze draft binds machine_fresh_confirmation.status=pass + authorization_id + record_path + record_sha256 (same confirmation contract as the blocked confirmation freeze)
  - independent-review-record # separate reviewer role writes an out-of-repo e3-ready-freeze-review record + .sha256 companion over the same freeze contract, binding machine_confirmation_sha256
  - final-ready-freeze-binds-review-record # final out-of-repo ready freeze (same confirmation contract bytes) binds independent_review_record.status=pass + reviewer_role + record_path + record_sha256
  - candidate-id-consumption-audit-2 # second external audit on the frozen HEAD before selftest/DryRun/Live; hash-recorded; fixed log: $HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-2.txt (+ .sha256)
  - selftest-hdc0
  - dryrun-same-ready-freeze
  - review-dryrun-freeze-bytes-unchanged
  - single-live
dry_run_vs_live_freeze: DryRun accepts plan_status blocked or ready (always emits non-evidence blocked); Live requires plan_status ready only
retry_policy: no-auto-retry; this AUTH AUTH-E3-PHYS1API26-20260813-0001 fixes attempt=initial and retry.basis/infrastructure_reason=N/A for TargetBindingConfirm and for every ready Live/DryRun consuming its confirmation; the runner-enforced generic infrastructure retry branch (attempt=infrastructure-blocked-retry-1, allowlist hdc-usb-interruption|collection-storage-failure|runner-host-failure, frozen matching Live blocked evidence prior) can never enter this AUTH path; ANY gate failure stops the campaign with no retry and no ID switch; any later attempt requires new governance and a new authorization
live_gate_commit_authorization: user authorized committing/pushing the runner port + governance changes (the Python trio + this registration) after review and BEFORE Live; campaign evidence (selftest/DryRun/Live outputs) still never commits ahead of Live; this registration task itself must not commit/push
verdict: N/A - authorization registration; not a live campaign verdict
scope_statement: whitelist-only per current runner plus the single user-local hdc tconn + single memory-only hdc list targets host-prep exception; no extra discovery/UDID/serial/hidumper/root/privileged/Go/NetBird/product; user readiness attestation does not replace machine fresh confirmation; E3 open, E8 CLOSED
reviewer: pending-independent-review
reviewed_at: pending
```

## 授权摘要

| 字段 | 值 |
| --- | --- |
| AUTH ID | `AUTH-E3-PHYS1API26-20260813-0001`（取代 0002 对 Windows host 的执行绑定；0002 历史记录保留不改写） |
| 授权者 | 用户（直接人类决策者），2026-08-13（已批准；批准方式：聊天确认） |
| authorization_status | `granted`（2026-08-13 用户聊天确认批准） |
| device_readiness | `user-attested-ready`（用户就绪声明，**不** 替代机器 fresh confirmation） |
| machine_fresh_confirmation | `pending`（改由 **Linux Pod** 通过 `-TargetBindingConfirm` 观测） |
| plan_status | `authorized-awaiting-linux-ready-freeze` |
| 执行 host | **Linux Pod**：`host-dev-alfadb-full` / `Debian GNU/Linux 13 (trixie)` / `x86_64` / Python `3.13.5`（Windows `ALFADB-V-WIN` 不再承担执行绑定） |
| runner | **Python 3 语义等价移植**（顺序门、HDC 白名单 argv、`RUNNER_RESULT` 行、seal 六字段、双 contract 哈希、confirmation/review record schema 全部保留）；移植后独立审查 0 blocker/0 major 并重新冻结三文件字节；原 PowerShell 三文件仓内保留不改写 |
| 目标元组（不变，冻结于 `ADJ-20260806-0003`） | `PLA-AL10` / `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`（`EV-E3-PHYS1BUILD7-20260806-0001` 实测逐字匹配）/ API `26` / `aarch64` / `arm64-v8a`（rebind `EV-E3-PHYS1REBIND7-20260806-0001` 实测）/ 设备别名 `PHYS-1` |
| candidate（**沿用 0002 未消费 pair**） | `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`，未 Live、未消耗；执行前须两次 ID 消费审计（audit-1/audit-2，均 hash 记录） |
| signed A/B HAP（Windows handoff 登记值） | A `3a98ad68bfc6253fe10b37a262e0051267b3b3083e6943f8207d68482d00c244` size `106210`；B `1adfa9664e59e7a9dc3da6650a59972517f5262a9b8160f4b3f6416770da3c26` size `106212`（用户已复制到 Linux 侧，核查任务并行进行） |
| 旧 pair 处置 | `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` **已消费**（外部 sealed blocked evidence `EV-E3-PHYS1API26-20260808-0001`：scenario-results `record_status=collected`/`overall=blocked`/`verdict=blocked`、`execution_mode=live`、`is_evidence=true`、cleanup `verified-clean`；campaign-seal SHA-256 `ec55603a2555b967c5912e79a846262d262e53069e3f1275105af5ccf4ef245f`（sealed_at `2026-08-08T09:53:23+08:00`）、manifest SHA-256 `36852f6c6d9d6e27c6b5ee07bd6b6037e8aa2a22f1fdbf45591addb047f5f843`，位于仓外证据根 `D:/HarmonyEvidence/netbird-e3/EV-E3-PHYS1API26-20260808-0001/`）；legacy-pair-consumption-audit 已完成（仓外审计日志 `D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260808-0001/audit/id-consumption-audit-1.txt`，SHA-256 `b530c438769e65390ee9065f918d1ea550a8f9745c61516b6899ff90c736e9c7`，2026-08-10T10:43:09+08:00）；**不得复用** |
| 允许范围 | 现行 runner 白名单（HDC target-binding `Version`/`TupleModel`/`TupleBuild`、A/B install/start/observation/mechanical prompts/final cleanup）+ **一次用户本地 `hdc tconn` + 一次内存级 `hdc list targets` host-prep 窄例外**（见下） |
| 禁止 | 任何额外 discovery、UDID、serial、`hidumper`、root、privileged、Go、NetBird、product；`hdc tconn` 与 `hdc list targets` 各只允许一次且不输出/持久化 endpoint/token |
| E8 | `CLOSED`（本授权不是 E8 `OPEN`） |

## 执行 host 迁移与 Linux Pod host 事实（本登记时点实测）

执行 host 已从 Windows（`ALFADB-V-WIN`）迁移到当前 Linux Pod。本登记时点 host-only 只读复测事实：

| 项 | 值 |
| --- | --- |
| hostname | `host-dev-alfadb-full` |
| 架构 | `uname -m` → `x86_64` |
| 发行版 | `/etc/os-release` `PRETTY_NAME` → `Debian GNU/Linux 13 (trixie)` |
| Python | `python3 --version` → `Python 3.13.5` |
| Linux Stable HDC | `$HARMONYOS_STABLE_HDC`（`source "$HOME/harmonyos/env.sh"` 后）→ `/home/worker/harmonyos/command-line-tools/current/sdk/default/openharmony/toolchains/hdc`；文件 SHA-256 `03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81`；`hdc version` → `Ver: 3.2.0d`（与 Windows HDC 同版本 `3.2.0d`，Linux 二进制文件 hash 不同属预期；仅定位 + 文件 hash + version 输出，**未执行** 设备命令） |
| 仓库 HEAD | 工作区基于 `main`，HEAD `b487c5e39838a4e0f18099eedcc2cd6f3712f51c`，worktree clean |
| 签名材料受控目录 | 仓外 `$HOME/harmonyos-signing`（含 `netbird-e3/`）；签名材料只存该受控目录，绝不入仓 |

## runner 移植声明（PowerShell → Python 3 语义等价）

本授权明确声明：runner 由 PowerShell 改写为 **Python 3**，属 **语义等价移植**，以下契约要素 **全部保留**（以 PowerShell 原 runner 为基准逐项核对）：

1. **13 步顺序门结构**：`sync-trusted-refs-bundle → candidate-id-consumption-audit-1 → blocked-confirmation-freeze-static-review → host-prep-device-connect-mapping → target-binding-confirm → ready-freeze-draft-binds-record-hash → independent-review-record → final-ready-freeze-binds-review-record → candidate-id-consumption-audit-2 → selftest-hdc0 → dryrun-same-ready-freeze → review-dryrun-freeze-bytes-unchanged → single-live`，缺一即停止。
2. **HDC 白名单 argv**：target-binding 仅 `Version`/`TupleModel`/`TupleBuild` 三条；A/B install/start/observation/mechanical prompts/final cleanup 按原白名单；任何额外 argv 一律拒绝。
3. **`RUNNER_RESULT` 行**：campaign 出口行 `RUNNER_RESULT=<overall> RECORD_STATUS=<recordStatus> MODE=<mode> EVIDENCE_ROOT=<path> RAW_ROOT_HASH=<sha> HDC_PROCESSES=<count>`；confirmation 模式出口行 `RUNNER_RESULT=<verdict> MODE=target-binding-confirm RECORD_KIND=target-binding-confirmation IS_EVIDENCE=false COMMAND_ATTEMPTED=<n> COMMAND_COMPLETED=<n> RECORD=<path> RECORD_SHA256=<sha>`。
4. **seal 六字段**：`campaign-seal.json` 的 `schema_version` / `algorithm` / `record.path` / `record.sha256` / `manifest.path` / `manifest.sha256`（外加 `sealed_at` 时间戳），结构与复算语义不变。
5. **双 contract 哈希**：sealed complete record 顶层同时投影标准最终 `freeze_contract_sha256`（完整契约）与稳定 `confirmation_contract_sha256`（两阶段不变投影）；confirmation/review record 一律绑定 confirmation contract。
6. **confirmation/review record schema**：`target-binding-confirmation`（`schema_version=1`、`record_kind`、`is_evidence=false`、`authorization_id`、`exception`、`campaign_id`、`evidence_id`、`attempt`、`retry`、`plan_status`、`device_alias`、`target_redacted`、`code_sha`、`runner_sha256`、`freeze_manifest_sha256`、`confirmation_contract_sha256`、`hdc_sha256`、`hdc_version`、expected/observed model+build、`started_at`/`ended_at`、`command_attempted`/`command_completed`/`command_count`、`repository_fingerprint`、`verdict`、`reason`）与 `e3-ready-freeze-review`（`schema_version`、`record_kind`、`is_evidence`、`exception`、`campaign_id`、`evidence_id`、`code_sha`、`runner_sha256`、`confirmation_contract_sha256`、`machine_confirmation_sha256`、`reviewer_role`、`operator_role`、`verdict`、`blockers`、`majors`、`started_at`/`ended_at`）；consumer 按 **exact schema** 拒绝任何未知 top-level 字段（防 target/secret canary 混入）。

**移植验收**：移植完成后由独立审查角色审查（**0 blocker / 0 major**），随后 **重新冻结三文件字节**（Python 版 runner / freeze example / selftest 的最终 commit 字节 SHA-256 绑定到 `repo_bytes.python_port`）；复算不一致 → 停止并重新登记，绝不绑定发散 hash。**原 PowerShell 三文件在仓内保留、不改写**，作为历史输入（当前工作区字节见 `repo_bytes.powershell_historical_input`）。

## 设备连接纪律（Linux Pod，host-prep 窄例外）

本授权在既有 runner 白名单之外新增 **唯一** 一项 host-prep 窄例外，用于在 Linux Pod 上把仓外目标绑定到 process-scope 环境：

- **`hdc tconn <dynamic-ip:port>`**：由 **用户在 Linux 终端本地** 执行 **一次且仅一次**（host-prep 门，`-TargetBindingConfirm` 之前）。endpoint 值 **不进聊天、不落盘、不记录**（仅当次终端进程可见，进程结束即消失）。IP:port 变化 → **停止并回报主会话**，不自行重连、不重复 tconn。
- **`hdc list targets`**：紧随 tconn 之后执行 **一次且仅一次**。仅 **内存** 取得输出；**不输出** 到任何记录/日志/回显、**不持久化** 到任何文件/环境持久层/仓库、**不写入** confirmation record / freeze / evidence / transcript。输出必须恰一 target token；0 个、多个或格式异常 → **立即停止**，不猜测、不选择、不重跑。
- **用途**：把该 token 设置为 **process-scope** 环境变量 `PHYS_1_TARGET`（仅当次进程可见），供后续 runner 门（`-TargetBindingConfirm` / selftest / DryRun / Live）作为仓外受控 target 注入。
- **边界**：本例外 **不扩大 runner 的 HDC 白名单**——`tconn`/`list targets` 是操作者的 host-prep 步骤，**不是** runner 命令，runner 自身仍只按白名单 argv 执行 `Version`/`TupleModel`/`TupleBuild` 与 A/B 定向操作；任何把 `tconn`/`list targets` 编入 runner、重跑、把 endpoint/token 落盘或写进任何记录的行为都违反本授权。
- **host HDC server 注记（不新增任何授权）**：执行一次 `hdc tconn`/`hdc list targets` 可能顺带启动 host 上正常的 HDC server 进程（由 hdc 客户端自管理，属正常 host 行为）；「内存级」承诺仅覆盖 **endpoint/token 不输出、不落盘**——本授权 **不** 附带任何额外 cleanup/stop-server/retry 授权；输出 0 个或多个 token 时 **立即停止**，不清理、不重试、不猜测。
- 该窄例外由操作者执行一次并消耗；本登记期间不执行。

## 用户就绪声明与机器 fresh confirmation（`-TargetBindingConfirm`）

- `device_readiness: user-attested-ready` 只记录用户声明设备已准备好；它 **不** 替代、**不** 提前满足 `machine_fresh_confirmation`。
- `machine_fresh_confirmation: pending` 必须由 **Linux Pod** 上的 **机器** 观测完成：通过 runner 的 `-TargetBindingConfirm` 模式（HDC 白名单 `Version` + `TupleModel` + `TupleBuild` 共 3 次，逐字比对冻结 `PLA-AL10` / `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`）确认当前绑定设备仍为冻结元组。该模式：接受 `plan_status: blocked` 或 `ready` 的 confirmation freeze（`machine_fresh_confirmation.pending`），但完整校验 freeze 结构/clean repo/`code_sha`/runner/HDC/外部输入 hash/`PHYS_1_TARGET` 单 token，且 **固定候选 pair** `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001` 与 `attempt=initial`（`retry.basis`/`infrastructure_reason=N/A`；generic retry 分支不进入本路径）；**不** 初始化 `EvidenceRoot`/`RawRoot`、**不** 设置 `is_evidence`、**不** 消费 campaign/evidence ID、不进入 capture/install/start/最终 campaign cleanup。产出仓外 **双文件** confirmation record（JSON 临时文件 + `.sha256` 临时文件、复算 hash 后 atomic move JSON、最后 atomic move companion 作为 completion marker；`schema_version=1`、`record_kind: target-binding-confirmation`、`is_evidence: false`、`authorization_id: AUTH-E3-PHYS1API26-20260813-0001`、`exception: E3-PHYS-PREFLIGHT`、alias `PHYS-1`、`target_redacted: true`、expected/observed model+build、`command_attempted`/`command_completed`（pass 时均为 3）、`confirmation_contract_sha256`（稳定两阶段投影，非完整 freeze contract）、started/ended、verdict `pass|blocked` + reason；禁止 target/serial/UDID/secret，observed version/model/build 与 reason 均先经敏感文本脱敏）。**退出语义**：pre-record 门失败（record/companion 已存在、路径在仓内、reparse 祖先）→ throw + exit 1 且 **不写任何文件**；probe/tuple 失败或 record 写入失败 → 尽力写 blocked record + companion + exit 2；pass 必须 `attempted=completed=3` 且双文件完成。companion 写失败可遗留 orphan JSON，但 **绝不可消费/绑定**（consumer 只接受双文件且 companion 与 record 字节一致）且 **禁止覆盖**。
- **consumer 完整校验**：`ready` Live 与 `ready` DryRun 强制 `machine_fresh_confirmation.status=pass`，且 record 必须仓外、无 reparse 祖先（record 与其 `.sha256` companion 均检查）、companion 存在且等于 record SHA-256；内容逐项核对 `schema_version=1`、record_kind、`is_evidence=false`、exception、AUTH ID、**exact candidate pair**、`attempt=initial`/retry N/A、`device_alias=PHYS-1`、`target_redacted=true`、verdict `pass`/reason `N/A`、code/runner/hdc SHA + `hdc_version`、`confirmation_contract_sha256` 等于当前 freeze 的 confirmation contract（稳定投影）、expected/observed model/build、`command_attempted=3`/`command_completed=3`、`started_at<=ended_at<=freeze.preflight_inputs_frozen_at`。**`blocked` 的 DryRun**：`status=pending`（或缺省）允许并跳过；若声明 `status=pass` 则同样 **完整校验**（blocked DryRun 不得藏起损坏的绑定）。`independent_review_record` 同规则：`blocked` DryRun 声明 review `status=pass` 时同样 **完整校验**（machine `status=pass` 且 review `status=pass` 时 review record 机械门完整执行），machine confirmation `pending`/缺省 而 review 声明 `pass` → **明确拒绝**（review record 绑定 machine confirmation hash，pending/absent machine 无法锚定），review `pending` 允许。**不引入任何未决策的小时有效期**：fresh 只验证顺序双锚——record `ended_at` 不晚于 **最终 ready freeze** 的 `preflight_inputs_frozen_at`，且 Live 在其自身 preflight 中再次执行 `Version`/`TupleModel`/`TupleBuild` 三条机器核对（fresh double anchor：确认时一次、Live 时一次）。
- 该确认在 ready freeze 生成 **之前** 执行；Live 时 runner precheck 会再次强制核对（漂移即停止，且 Live precheck 失败走正常 campaign 流程：定向 cleanup verification，不跳过）。
- 在机器 fresh confirmation 完成前，任何情况下不得进入 `plan_status: ready`、不得 Live。

## 两次新 ID 消费审计（audit-1 / audit-2）

沿用候选 identity `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001` 必须通过 **两次外部消费审计** 且每次 **hash 记录**（与 0002 相同，未执行过）：

1. **new-candidate-id-consumption-audit-1（同步 trusted refs/bundle 之后、任何 `-TargetBindingConfirm`/ready freeze 之前；针对沿用 pair，尚未执行）**：`git fetch`/检出同步后，审计仓库（`git log --all --grep` + evidence 目录与仓外 evidence roots 全文 grep）确认没有任何 `execution_mode=live` / `is_evidence=true` / seal 记录（scenario-results/campaign-seal/hash-manifest 或独立 evidence 文档）占用沿用 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`；将两次 grep 的输出与时间戳记入仓外审计日志（固定路径 `$HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-1.txt` + `.sha256`，hash 记录）。**旧 pair 的消费确认已由独立的 legacy-pair-consumption-audit 完成**（`id-consumption-audit-1.txt`，SHA-256 `b530c438769e65390ee9065f918d1ea550a8f9745c61516b6899ff90c736e9c7`，2026-08-10T10:43:09+08:00）：外部 sealed blocked evidence `EV-E3-PHYS1API26-20260808-0001`（scenario-results `record_status=collected`/`overall=blocked`/`verdict=blocked`、`execution_mode=live`、`is_evidence=true`、cleanup `verified-clean`；campaign-seal SHA-256 `ec55603a2555b967c5912e79a846262d262e53069e3f1275105af5ccf4ef245f`（sealed_at `2026-08-08T09:53:23+08:00`）、manifest SHA-256 `36852f6c6d9d6e27c6b5ee07bd6b6037e8aa2a22f1fdbf45591addb047f5f843`）占用旧 pair，旧 pair **不得复用**。两次沿用 pair 审计（audit-1/audit-2）与已完成的 legacy-pair-consumption-audit 相互独立，各自 hash 记录。
2. **audit-2（最终 ready freeze 绑定 review record 之后、selftest/DryRun/Live 之前）**：在冻结 HEAD 上重跑同一审计并重新记录 hash（固定路径 `$HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-2.txt` + `.sha256`），确认沿用 pair 仍未消耗。

**若任何一次发现占用记录，候选不得复用**：停止并回报主会话，等用户授权新 campaign/evidence ID；绝不静默改用其他 ID 或改写记录。

## 当前 Linux Pod host preflight 复测（本登记时点，host-only 只读）

| 项 | 值 |
| --- | --- |
| host | Linux Pod：`host-dev-alfadb-full` / `Debian GNU/Linux 13 (trixie)` / `x86_64` / Python `3.13.5`——即授权登记所指的当前执行 host 本机 |
| HDC | `$HARMONYOS_STABLE_HDC` = `/home/worker/harmonyos/command-line-tools/current/sdk/default/openharmony/toolchains/hdc`，文件 SHA-256 `03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81`，version `3.2.0d`（仅定位 + 文件 hash + version 输出，**未执行** 设备命令） |
| 仓外 freeze | Windows 仓外根 `D:/HarmonySigning/netbird-e3/freeze/` 存在历史 freeze（Linux 侧未同步）；**Linux 侧全新 blocked / ready freeze 尚未生成** |
| signed HAP / profile / cert | Windows handoff 登记值：A `3a98ad68bfc6253fe10b37a262e0051267b3b3083e6943f8207d68482d00c244` size `106210`、B `1adfa9664e59e7a9dc3da6650a59972517f5262a9b8160f4b3f6416770da3c26` size `106212`（用户已复制到 Linux 侧，核查任务并行进行；本登记记录 Windows handoff 登记值，核查一致后冻结） |
| 受控 EvidenceRoot / RawRoot | 旧证据根位于 Windows 仓外（`D:/HarmonyEvidence/netbird-e3/`、`D:/HarmonyEvidenceRaw/netbird-e3/`，含 sealed `EV-E3-PHYS1API26-20260808-0001` 等）；**Linux 侧沿用 pair 的 EvidenceRoot / RawRoot 尚未初始化**（按顺序门由 runner 在对应模式初始化，本登记不初始化） |
| 仓外 `PHYS_1_TARGET` 映射 | **未建立**（当前 shell 未设置；须 host-prep 恰一次用户本地 `hdc tconn` + 恰一次内存级 `hdc list targets` 建立 process-scope 映射） |
| 沿用 pair 消费审计 | audit-1 / audit-2 **均未执行**（固定仓外日志路径见「两次新 ID 消费审计」节：`$HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-{1,2}.txt` + `.sha256`） |
| legacy-pair-consumption-audit | 已完成（0002 登记：`D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260808-0001/audit/id-consumption-audit-1.txt`，SHA-256 `b530c438…`，2026-08-10T10:43:09+08:00，与本登记一致） |
| HDC 进程数 | 0（未启动任何 HDC；仅定位与文件 hash/version，未执行） |
| 设备命令 | 未发出（本登记期间禁止） |
| 仓库 HEAD | 工作区基于 `main`（HEAD `b487c5e39838a4e0f18099eedcc2cd6f3712f51c`，worktree clean）；Python 移植三文件 + 本登记文档待审查后提交（本任务禁止提交/推送） |
| host 状态 | `ready_for_commit_and_ordered_preflight`——工具与签名输入齐备，可审查后提交并按顺序门执行 preflight；**非全部 Live-ready**（未提交 clean final commit、沿用 pair audit-1、全新 blocked/ready freeze、confirmation/review records、新 EvidenceRoot/RawRoot、process-scope `PHYS_1_TARGET` 映射均尚待） |

因此本 Linux Pod **具备提交与按序 preflight 条件**（`ready_for_commit_and_ordered_preflight`），ready freeze 的生成、selftest、DryRun 与 Live 均须在本 host 按本登记顺序门完成；**不得** 表述为全部 Live-ready。

## 仓内固定字节与 clean-commit requirement

**历史输入（原 PowerShell 三文件，仓内保留、不改写）**——当前工作区字节：

- runner：`e3-phys-preflight-campaign.ps1` → `e1da598d8cdf6bad3d243e393357c03ef40550ebab207714797d53e5a388ab41`
- freeze example：`e3-phys-preflight-freeze.example.json` → `5ea7acda269873a5d0ba2af2c0396ee6e3e62b9cf0990d83ddc55aadb49af62d`（**刻意保持 `plan_status: blocked`**，仅占位，不含 campaign 哈希/路径/秘密）
- selftest：`tests/e3-phys-preflight-runner-selftest.ps1` → `28c26b72b7ff9adaddc0eb734a4441f2e7ed4e16435af6f08851c03cf3a5d220`

**Python 移植三文件（runner / freeze example / selftest）**：移植完成后独立审查 0 blocker/0 major，随后 **重新冻结** 最终 commit 字节并绑定到 `repo_bytes.python_port`（本登记时点 `pending-re-freeze`）。**clean-commit requirement**：最终 commit/hash 在 Live 前审查通过后提交时冻结——Linux Pod 必须在提交时**重新计算** **最终 commit 中** 三文件的 SHA-256，复算结果**预期与绑定值一致**；若复算不一致，说明文件在登记后被改动，必须停止并重新登记，**不得** 绑定任何发散 hash；最终 bundle commit 是权威字节源。freeze contract 包含 `operator_trust_model` / `scenario_invalid_policy` / `layout_verification_profile` / `vpn_conflict_rejection_codes` / `process_probe_target` 等决策字段，旧 freeze（如 20260807 candidate `INVALID-TIMELINE`、历史 runner 绑定）一律被 runner 拒绝，任何模式（DryRun 含）均不可用于新 Live。`ready` freeze 还须携带 `independent_review_record`（`status: pass` + `reviewer_role` + `record_path`/`record_sha256`）绑定独立审查 record（见下）。

## Linux 顺序门与命令模板（cwd 均为 `spikes/e3-vpn-extension-physical-preflight-hap`）

Linux Pod 必须 **按序** 满足下列门，缺一即停止并回报主会话（所有路径均为占位符；不含 target/secret；除 host-prep 一次用户本地 `hdc tconn`、一次 `hdc list targets` 与 `-TargetBindingConfirm` 的 3 条白名单设备命令外均禁 HDC）。命令模板为 Python 3 移植版（语义等价，CLI 参数面保留；`$HARMONYOS_STABLE_HDC` 经 `source "$HOME/harmonyos/env.sh"` 取得）：

1. **同步 trusted refs/bundle**：`git fetch` + 检出/核对冻结 HEAD，确认 worktree clean、HEAD 精确为冻结值；`git status --short --branch` 在签名、构建、freeze、selftest、DryRun、Live 前后检查。
2. **候选 ID 消费审计（new-candidate-id-consumption-audit-1，尚未执行）**：确认沿用 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001` 未被任何 `execution_mode=live` / `is_evidence=true` / seal 记录占用；旧 pair 的消费确认不在此处重复执行——已由完成的 legacy-pair-consumption-audit 覆盖（`id-consumption-audit-1.txt` SHA-256 `b530c438769e65390ee9065f918d1ea550a8f9745c61516b6899ff90c736e9c7`，见「两次新 ID 消费审计」节）；grep 输出与时间戳记入仓外审计日志（固定路径 `$HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-1.txt` + `.sha256`，hash 记录）；发现即停止，等用户新 ID 授权。
3. **blocked confirmation freeze 静态审查**：生成全新仓外 confirmation freeze（`plan_status: blocked` + `machine_fresh_confirmation.status: pending` + `independent_review_record.status: pending` + 全部外部 hash，候选 pair 为沿用 pair），由独立审查角色静态审查契约与角色（0 blocker/0 major）后，freeze 中 `independent_review_ready` 为 `true`——此处 `independent_review_ready=true` 只表示 **契约与角色已静态就绪**，**不是** 设备或执行就绪，也不替代 `-TargetBindingConfirm` 的机器确认；**blocked confirmation freeze 不需要独立审查 record**。
4. **host-prep 设备连接与映射（一次，用户本地 + 内存级）**：用户在 Linux 终端本地执行 `hdc tconn <dynamic-ip:port>` **恰一次**（endpoint 值不进聊天/不落盘/不记录），随后执行 `hdc list targets` **恰一次**；输出必须恰一 token，仅内存读取，设置 process-scope `PHYS_1_TARGET`；不输出、不持久化、不记录 endpoint/token；0/多/异常 → 停止。**不扩大 runner HDC 白名单**。
5. **机器 fresh confirmation（`-TargetBindingConfirm`）**：

    ```bash
    python3 e3-phys-preflight-campaign.py \
      -FreezeManifest $HOME/harmonyos-signing/netbird-e3/freeze/freeze-blocked-confirm.json \
      -ConfirmationRecord $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260813-0001.json \
      -HapA $HOME/harmonyos-signing/netbird-e3/staging/final-a.hap \
      -HapB $HOME/harmonyos-signing/netbird-e3/staging/final-b.hap \
      -HdcPath "$HARMONYOS_STABLE_HDC" -TargetBindingConfirm
    ```

    判据：`RUNNER_RESULT=pass MODE=target-binding-confirm RECORD_KIND=target-binding-confirmation IS_EVIDENCE=false COMMAND_ATTEMPTED=3 COMMAND_COMPLETED=3 RECORD=<path> RECORD_SHA256=<sha>`；产出仓外 **双文件** record（JSON + `.sha256` companion，companion 为 completion marker），record 不含 target/serial/UDID/secret，**不** 创建 EvidenceRoot/RawRoot、**不** 消费 campaign/evidence ID。model/build 逐字匹配冻结；任何失败（含 drift）→ `RUNNER_RESULT=blocked` + **exit 2**（probe/tuple 失败；pre-record 门失败则 **exit 1 且无 record**）。drift 时该模式未安装任何 A/B、未建 staging、未起 capture，runner **不** 执行任何 cleanup/卸载/残留查询（这些操作不在本模式白名单内，也不属于 confirm 计划），无需设备端定向 cleanup；记录 blocked 后回报主会话。
6. **生成全新仓外 `ready` freeze draft（同 confirmation contract）**：新对象绑定步骤 1 的精确 clean HEAD、仓内 bytes（Python 移植三文件哈希，**以最终 commit 为准**）与全部完整外部 hash，并绑定步骤 5 的 record：`machine_fresh_confirmation.status=pass`、`authorization_id=AUTH-E3-PHYS1API26-20260813-0001`、`record_path=<record>`、`record_sha256=<sha>`；`plan_status: ready`；**confirmation contract 与步骤 3 的 blocked confirmation freeze 字节一致**（执行核心/候选/外部输入/code/runner/HDC/角色相同），而 `preflight_inputs_frozen_at` 推进到确认与审查 ended_at 之后（完整 freeze contract hash 因此不同，属预期）。**不得** 原地改旧 `blocked` freeze。
7. **独立审查 record（ready freeze 机械门）**：独立审查角色（`independent_reviewer_role`，与 operator 不同）对该 ready freeze 写出仓外 review record（`record_kind: e3-ready-freeze-review`、`is_evidence: false`、`schema_version: 1`、verdict `pass`、`blockers: 0`、`majors: 0`、`reviewer_role` 匹配、exact candidate pair/code_sha/runner_sha256/`confirmation_contract_sha256` 一致（稳定投影，与步骤 5 的 confirmation record 绑定同一 confirmation contract）、`machine_confirmation_sha256` 等于步骤 5 record 的 SHA-256、started/ended ≤ 最终 ready freeze 的 `preflight_inputs_frozen_at`）＋ `.sha256` companion。
8. **最终 `ready` freeze（同 confirmation contract，绑定 review record）**：新仓外对象，confirmation contract 与步骤 6 相同字节，绑定 `independent_review_record.status=pass` + `reviewer_role` + `record_path`/`record_sha256`（步骤 7 的 record）。
9. **候选 ID 消费审计（audit-2）**：在冻结 HEAD 上重跑步骤 2 的审计并重新记录 hash（见「两次新 ID 消费审计」节）。
10. **selftest（host-only，`HDC_PROCESSES=0`）**：

    ```bash
    python3 tests/e3-phys-preflight-runner-selftest.py
    ```

11. **DryRun（host-only，非证据；对同一份最终 `ready` freeze）**：

    ```bash
    python3 e3-phys-preflight-campaign.py \
      -FreezeManifest $HOME/harmonyos-signing/netbird-e3/freeze/freeze-ready.json \
      -EvidenceRoot $HOME/harmonyos-signing/netbird-e3/evidence-dry-run \
      -RawRoot $HOME/harmonyos-signing/netbird-e3/evidence-dry-run.raw \
      -HapA $HOME/harmonyos-signing/netbird-e3/staging/final-a.hap \
      -HapB $HOME/harmonyos-signing/netbird-e3/staging/final-b.hap \
      -HdcPath "$HARMONYOS_STABLE_HDC" -DryRun
    ```

    DryRun 判据：`is_evidence: false`、`HDC_PROCESSES=0`、`integrity_violations: []`、产出显式非证据 blocked record。`ready` freeze 的 DryRun 与 Live 一样验证 `machine_fresh_confirmation` 绑定（status=pass/authorization_id/record_path+sha/内容一致）与 `independent_review_record` 绑定（status=pass/reviewer_role/record_path+sha/内容一致）；`blocked` DryRun 允许 `pending`。
12. **审查 DryRun 且 freeze 字节不变**：独立审查角色核对 DryRun 结果与 ready freeze 契约（0 blocker/0 major）后确认 **ready freeze 文件字节与步骤 8 完全一致**（重新计算 freeze SHA-256，不得改动任何字段，包括 `plan_status`）；此时方视为 Live 输入就绪。
13. **单次 Live（唯一一次；只接受步骤 8 的同一份 `plan_status: ready` freeze）**：

    ```bash
    python3 e3-phys-preflight-campaign.py \
      -FreezeManifest $HOME/harmonyos-signing/netbird-e3/freeze/freeze-ready.json \
      -EvidenceRoot $HOME/harmonyos-signing/netbird-e3/evidence-live \
      -RawRoot $HOME/harmonyos-signing/netbird-e3/evidence-live.raw \
      -HapA $HOME/harmonyos-signing/netbird-e3/staging/final-a.hap \
      -HapB $HOME/harmonyos-signing/netbird-e3/staging/final-b.hap \
      -HdcPath "$HARMONYOS_STABLE_HDC"
    ```

    Live 需要操作员人工机械输入；runner 在 continuous capture 前强制再次执行 `Version`/`TupleModel`/`TupleBuild` 机器核对（fresh double anchor，漂移即停止）。Live 的 preflight 属正常 campaign 流程：drift 停止后仍执行定向 cleanup verification（与 confirm 模式不同——confirm 模式不执行任何 cleanup 查询）。

**freeze 状态规则**：`-DryRun` 接受 `plan_status: blocked` 或 `ready`（永远产出非证据 blocked record）；`-TargetBindingConfirm` 接受 `plan_status: blocked` 或 `ready`（consume `machine_fresh_confirmation.pending`，产出 non-evidence confirmation record；强制 exact candidate pair + `attempt=initial` + retry N/A）；`Live`/`LiveSimulation` **只** 接受 `plan_status: ready`（runner 强制；Live 的 `ready` 还强制 `machine_fresh_confirmation` 与 `independent_review_record` 绑定，见上）。旧 `blocked` candidate freeze 不得原地改为 `ready`；`ready` freeze 必须是新仓外对象。confirmation record 状态：`record_kind: target-binding-confirmation`、`is_evidence: false`、`record_status: N/A`（非 campaign record），不进 evidence 目录、不占 campaign/evidence ID、可被 `ready` freeze 通过 `record_path`+`record_sha256` 绑定引用；review record 同理（`record_kind: e3-ready-freeze-review`、`is_evidence: false`）。sealed campaign complete record 与 preflight transcript 投影 `machine_fresh_confirmation`/`independent_review_record`：仅 status/authorization_id/reviewer_role/record_sha256/`record_path_sha256`（不泄露真实路径）并绑定 **confirmation contract**（`confirmation_contract_sha256`）；sealed complete record 顶层同时投影标准最终 `freeze_contract_sha256`（完整契约）与稳定 `confirmation_contract_sha256` 两个字段。

**两阶段 confirmation contract 规则**：`Get-FreezeContract`（完整契约）含治理/时间字段 `preflight_inputs_frozen_at`。blocked confirmation freeze 在机器确认前冻结（`T1`），最终 ready freeze 必须满足时间门（`started<=ended<=preflight_inputs_frozen_at`）而推进到 `T2 > T1`——因此 blocked freeze / ready draft / final ready freeze 的 **完整 freeze contract hash 允许且必然不同**（frozen_at 治理字段不同），但三者的 **confirmation contract 必须字节相同**（执行核心/候选 pair/外部输入/code/runner/HDC/角色；排除 `plan_status`、`preflight_inputs_frozen_at`、`machine_fresh_confirmation`、`independent_review_record`、`independent_review_ready`）。confirmation/review record 一律绑定 confirmation contract；若两阶段间 confirmation contract 发生变化，consumer 拒绝（`confirmation_contract_sha256 does not match`），防篡改绑定不因 frozen_at 推进而失效。

## 重试纪律

- `blocked`、`fail`、`invalid` 或 **no seal**（未完成封印/证据不完整）之后 **不得自动重跑**，也不得自动分配新 ID。
- **当前 AUTH 固定 `attempt=initial`（retry N/A）**：`-TargetBindingConfirm` 与消费本 AUTH confirmation 的 `ready` Live/DryRun 均被 runner 强制 exact candidate pair + `attempt=initial` + `retry.basis`/`infrastructure_reason=N/A`——因此 **现有 generic retry 分支（`attempt: infrastructure-blocked-retry-1`）不进入本次路径**。**任一 gate 失败即停止，不重试、不换 ID**。任何后续尝试必须 **新治理**：先取得新的路线决策并重新登记新授权；当前 AUTH `AUTH-E3-PHYS1API26-20260813-0001` 不可用于任何 retry。
- runner 既有 generic retry 规则仍保留给未来新治理：唯一允许的重试为 `attempt: infrastructure-blocked-retry-1`，`retry.infrastructure_reason` 必须命中白名单 `hdc-usb-interruption` / `collection-storage-failure` / `runner-host-failure`，且 `prior_record` 必须是冻结匹配的 **Live blocked evidence record**（`is_evidence: true`、`record_status: blocked`、`overall: blocked`、`verdict: blocked`、同 campaign/attempt/code/runner/artifact/freeze contract）。prior 为 DryRun、非证据、无 seal 或 `is_evidence: false` 时不构成 retry 依据；`-TargetBindingConfirm` 的 blocked confirmation record（`is_evidence: false`）同样 **不** 构成 retry 依据。
- 本授权不豁免任何既有纪律：build drift、operator-aborted、功能 fail、scenario invalid、integrity violation、非基础设施 blocked 均不授权 retry；`consumed-blocked` / `superseded-unexecuted` / `INVALID-TIMELINE` 历史 ID 一律不得复用（含旧 pair `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001`）。
- 任何继续或重试必须先取得新的路线决策并重新登记。

## 禁止项（明文）

本授权 **明文禁止** 下列行为，任何一项违反即停止并回报主会话：

1. **禁止任何额外动作**：禁止任何额外 discovery、UDID、serial、`hidumper`、root、privileged、Go、NetBird、product 动作；禁止任何不在现行 runner 白名单内的 HDC/设备命令（白名单仅 `Version`/`TupleModel`/`TupleBuild` 与 A/B install/start/observation/mechanical prompts/final cleanup）。
2. **禁止重复连接**：禁止 `hdc tconn` 重复执行（恰一次，用户本地，endpoint 值不进聊天/不落盘/不记录）；禁止 `hdc list targets` 重复执行（恰一次，内存级）；endpoint 变化 → 停止并回报主会话，不自行重连。
3. **禁止敏感值落盘**：禁止 endpoint、target token、UDID 写入仓库、聊天、日志、截图、freeze、record、evidence、transcript 或任何持久层；`PHYS_1_TARGET` 仅 process-scope 环境变量。
4. **签名材料受控**：签名材料（`.p12`、`.p7b`、`.cer`、`.csr`、密码、alias、私钥内容）只存仓外 `$HOME/harmonyos-signing` 受控目录，绝不入仓、不入聊天、不入日志；密码只使用本机安全机制，绝不写进脚本、命令行、聊天或日志。
5. **禁止改写历史**：0002 及更早授权/证据登记记录保留不改写；旧 pair（`20260808` 等）不得复用；本登记任务本身禁止提交/推送。

## 门状态

- E3 未关闭；本记录是授权登记，不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`；本授权不是 E8 `OPEN`，也不改变 E1/E4-E7 聚合状态。本实现不扩展授权边界：`-TargetBindingConfirm` 只复用现行 runner 白名单 `Version`/`TupleModel`/`TupleBuild` 三条 target-binding 命令，唯一新增为一次用户本地 `hdc tconn` + 一次内存级 `hdc list targets` host-prep 窄例外（不扩大 runner HDC 白名单），不新增任何 discovery/UDID/serial/`hidumper`/cleanup/privileged 能力。
- `plan_status: authorized-awaiting-linux-ready-freeze`：Linux 顺序门（同步 trusted refs/bundle → audit-1 → blocked confirmation freeze 静态审查 → host-prep 设备连接与映射（恰一次 tconn + 恰一次 list targets）→ `-TargetBindingConfirm` → ready draft 绑定 record → 独立审查 record → 最终 ready freeze 绑定 review → audit-2 → selftest → 同一 ready freeze DryRun → 审查 DryRun 且 freeze 字节不变 → 单次 Live）完成前不可执行。
- **Live 前提交/推送**：用户授权在 Live 前将 runner 移植 + 治理变更审查后提交/推送（Python 移植三文件 + 本登记文档），但 campaign evidence 仍不提前提交；**本登记任务未提交/推送**，提交由 Live 前的独立审查步骤执行。
- 历史记录全部保留不改写：API23 initial（`EV-E3-PHYS1API23-20260806-0001`）、rebind（`EV-E3-PHYS1REBIND7-20260806-0001`）、build 确认（`EV-E3-PHYS1BUILD7-20260806-0001`）、API26 0001（`EV-E3-PHYS1API26-20260807-0001`，`consumed-blocked`）、API26 0002（`EV-E3-PHYS1API26-20260807-0002`，`consumed-blocked`）、host remediation（`EV-E3-PHYS1HOST-20260808-0001`）、外部 sealed blocked evidence `EV-E3-PHYS1API26-20260808-0001`（`record_status=collected`/`overall=blocked`/`verdict=blocked`，占用旧 pair 故旧 pair 已消费；campaign-seal SHA-256 `ec55603a2555b967c5912e79a846262d262e53069e3f1275105af5ccf4ef245f`、manifest SHA-256 `36852f6c6d9d6e27c6b5ee07bd6b6037e8aa2a22f1fdbf45591addb047f5f843`）、`ADJ-20260808-0001/0002/0003` 登记、旧授权 `AUTH-E3-PHYS1API26-20260810-0001`（superseded，consumed）与 `AUTH-E3-PHYS1API26-20260810-0002`（本登记取代其 Windows host 执行绑定，历史记录保留不改写）。

## 用户批准记录

本登记起草时 `authorization_status: pending-user-approval`。用户（直接人类决策者）**最终批准** 后，填写下表并将 `authorization_status` 改为 `granted`；批准前本登记不构成可执行授权。

| 字段 | 值 |
| --- | --- |
| 批准时间 | `2026-08-13T21:46:10+08:00`（Asia/Shanghai） |
| 批准方式 | 聊天确认 |
| 批准后 authorization_status | `granted` |

## 执行注记（2026-08-14）

用户（直接人类决策者）后续补充授权与第 5 步执行事实，如实记录：

1. **host-prep 执行主体变更**：`hdc tconn` 由用户本人本地终端执行一次；`hdc list targets` 原定「恰一次、用户本地」改为——用户 2026-08-14 会话内授权「本会话任何时候都可以执行 hdc list targets」（内存级、不输出、不落盘原则不变）；用户另自行执行一次 `hdc list targets` 并确认输出恰一 `IP:PORT`。
2. **第 5 步第一次尝试**（2026-08-14）：pre-record 门失败——仓库不 clean（`__pycache__/` 未跟踪垃圾）→ exit 1，无 record、无设备命令、无 ID 消费；`__pycache__` 已清理。
3. **第 5 步第二次尝试**（2026-08-14 ~09:35）：`RUNNER_RESULT=blocked MODE=target-binding-confirm IS_EVIDENCE=false COMMAND_ATTEMPTED=3 COMMAND_COMPLETED=3`，exit 2；**record 未落盘**（约定路径 `$HOME/harmonyos-signing/netbird-e3/records/` 目录不存在，runner 尽力写失败）；blocked 真因（tuple 漂移或记录层失败）以当时证据不可区分。
4. **按纪律停止**：blocked 后不重试、不换 ID、不自行诊断重跑；后续任何继续需用户新授权/新治理。
