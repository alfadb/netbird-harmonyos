# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-10 · 0002）

最后核验：2026-08-10

本文登记用户（直接人类决策者）于 2026-08-10 显式授予的 **新** `E3-PHYS-PREFLIGHT` 物理设备执行授权（`AUTH-E3-PHYS1API26-20260810-0002`），**取代** 已消耗的 [`AUTH-E3-PHYS1API26-20260810-0001`](e3-physical-preflight-authorization-2026-08-10.md)。旧授权登记文件 **正文历史内容保留、不改写**，仅在顶部新增 superseded/consumed 状态注记（2026-08-10 追加），本文是当前唯一生效的授权登记。

用户本次显式授权的完整事实（直接人类决策）：

1. **一次 host-prep `hdc list targets` 窄例外**：授权在 Windows signing/build host 上执行 **一次且仅一次** `hdc list targets`，仅 **内存** 取得输出并验证 **恰一** target token，随后将该 token 设置到 process-scope 环境（`PHYS_1_TARGET`）；**不输出、不持久化、不记录** 该 token 到任何文件/日志/仓库。若输出不是恰一 token（0 个、多个、异常格式）→ 立即停止，不猜测、不选择。
2. **完整 campaign 授权**：在上述 host-prep 之后，按本文「Windows 顺序门」的 **原顺序** 执行 **一次** 完整 campaign，`attempt=initial`、`retry` N/A；**任一 gate 失败立即停止**，**不重试、不换 ID**；E8 保持 `CLOSED`。
3. **Live 前治理/runner 提交授权**：用户明确授权在 Live 前将本次 runner + 治理变更（三文件 diff 与本文档等）**审查后提交/推送**，该授权覆盖旧 0001 中「最终 bundle commit 由 Linux 侧统一提交」的既有安排；但 **campaign evidence 仍不提前提交**（selftest/DryRun/Live 产出的非证据/证据材料留在仓外）。**本登记任务本身禁止提交/推送**（提交/推送由 Live 前的独立审查步骤执行）。

本文是 **授权登记**，**不是** live campaign 证据、**不是** 设备实测记录。本登记（当前 Windows signing/build host，host-only 只读复测）期间 **不** 运行任何 HDC/设备命令、**不** 安装工具、**不** 生成 ready freeze。**用户就绪声明不替代机器 fresh confirmation**。E3 未关闭，E8 保持 `CLOSED`。

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260810-0002
supersedes: AUTH-E3-PHYS1API26-20260810-0001 # consumed: its candidate pair E3-PHYS-PREFLIGHT-20260808-0001 / EV-E3-PHYS1API26-20260808-0001 is occupied by the external sealed blocked evidence EV-E3-PHYS1API26-20260808-0001 (external host evidence, see legacy-pair-consumption-audit below); the old authorization must not be used for any gate
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: collected
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: not-run-authorization-registration
is_evidence: false
authorization_status: granted
device_readiness: user-attested-ready # user readiness attestation does NOT replace machine fresh confirmation
machine_fresh_confirmation: pending # completed by the Windows host via -TargetBindingConfirm; this AUTH fixes attempt=initial and the new candidate pair below (retry N/A); any later retry requires new governance
plan_status_at_registration: authorized-awaiting-windows-ready-freeze
campaign_status: candidate-new-created-not-live-pending-audits # NEW pair created by this AUTH; never Live, never consumed; audit-1/audit-2 both pending (hash-recorded out-of-repo)
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
hdc_target_reference: controlled external mapping; repository alias PHYS-1 only # single hdc list targets host-prep exception is memory-only, never persisted (see below)
host_prep_target_mapping:
  command: "hdc list targets" # ONE execution only, host-prep, before any TargetBindingConfirm
  scope: memory-only # output token read in memory only; never written to file/log/repo, never echoed
  acceptance: exactly one target token # 0 / multiple / malformed => STOP, no guessing, no selection
  persistence: "PHYS_1_TARGET" set as process-scope environment variable only
  privacy_boundary: no token output, no token persistence, no token in any record/freeze/evidence; runner HDC whitelist is NOT expanded by this exception (hdc list targets is a host-prep operator step, not a runner command)
  consumed_authorization: false # NOT yet consumed: to be consumed exactly once (future) by the operator at the mapping gate on the Windows host; this registration does not run it
repo_bytes: # exact committed-byte SHA-256 of the CURRENT WORKSPACE three files (post-ADJ-20260810-0001 C6 review fixes, both majors resolved); recompute at commit time and bind the committed values
  runner_sha256: e1da598d8cdf6bad3d243e393357c03ef40550ebab207714797d53e5a388ab41 # spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-campaign.ps1
  freeze_example_sha256: 5ea7acda269873a5d0ba2af2c0396ee6e3e62b9cf0990d83ddc55aadb49af62d # e3-phys-preflight-freeze.example.json (intentionally blocked)
  selftest_sha256: 28c26b72b7ff9adaddc0eb734a4441f2e7ed4e16435af6f08851c03cf3a5d220 # tests/e3-phys-preflight-runner-selftest.ps1
  hash_note: values above are the CURRENT WORKSPACE bytes (the uncommitted three-file identity diff: new AUTH/candidate IDs plus the C6 selftest confirmation-contract projection fix for PowerShell single-element-array unrolling). These registered values are the EXPECTED hashes: at commit time the Windows host MUST recompute the SHA-256 of the exact committed bytes of these three files and the result is EXPECTED to equal the registered values above (the registration task does not modify these files after this registration; any mismatch means the files changed after registration => STOP and re-register, never bind a divergent hash). clean-commit requirement: commit/hash frozen only AFTER the review pass that precedes Live (this registration task itself must not commit/push)
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260810-0001
  evidence_id: EV-E3-PHYS1API26-20260810-0001
  attempt: initial # this AUTH fixes attempt=initial with retry.basis/infrastructure_reason=N/A; the generic infrastructure retry branch never applies to this AUTH path
  identity_status: pending-two-consumption-audits # NEW IDs: audit-1 (after trusted refs/bundle sync, before any TargetBindingConfirm) and audit-2 (after the final ready freeze, before selftest/DryRun/Live); both hash-recorded out-of-repo
  live: false
  consumed: false
  note: NEW candidate identity created by this authorization, replacing the old pair. The OLD pair E3-PHYS-PREFLIGHT-20260808-0001 / EV-E3-PHYS1API26-20260808-0001 is consumed: EV-E3-PHYS1API26-20260808-0001 is the external sealed blocked evidence (external host evidence at D:/HarmonyEvidence/netbird-e3/EV-E3-PHYS1API26-20260808-0001/: scenario-results.json record_status=collected / overall=blocked / verdict=blocked, execution_mode=live, is_evidence=true, cleanup verified-clean; campaign-seal.json SHA-256 ec55603a2555b967c5912e79a846262d262e53069e3f1275105af5ccf4ef245f sealed_at 2026-08-08T09:53:23+08:00; hash-manifest.json SHA-256 36852f6c6d9d6e27c6b5ee07bd6b6037e8aa2a22f1fdbf45591addb047f5f843), so the old identity must never be reused. The record_status of that sealed evidence is collected (overall/verdict blocked), NOT record_status=blocked. Timeline: that Live occurred AFTER the 08-08 host-remediation registration (whose snapshot stated the candidate was not yet Live); the two snapshots do not conflict. legacy-pair-consumption-audit (COMPLETED): out-of-repo audit log D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260808-0001/audit/id-consumption-audit-1.txt, SHA-256 b530c438769e65390ee9065f918d1ea550a8f9745c61516b6899ff90c736e9c7, run 2026-08-10T10:43:09+08:00, confirms the OLD pair is occupied (never reused). new-candidate-id-consumption-audit-1 (PENDING, for the NEW pair): not yet run; it is audit-1 in the windows_gate_order below; fixed out-of-repo log path D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-1.txt (+ .sha256). new-candidate-id-consumption-audit-2 (PENDING): audit-2 below; fixed out-of-repo log path D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-2.txt (+ .sha256). Any audit run against the NEW pair is separate from the completed legacy-pair-consumption-audit; ANY occupying live/evidence/seal record => STOP and await user-authorized new IDs, never silently switch or rewrite records
windows_gate_order:
  - sync-trusted-refs-bundle # git fetch + checkout frozen HEAD, clean worktree check before/after signing/build/freeze/selftest/DryRun/Live
  - candidate-id-consumption-audit-1 # NEW pair E3-PHYS-PREFLIGHT-20260810-0001 / EV-E3-PHYS1API26-20260810-0001 must be unoccupied; hash-recorded; does NOT repeat the legacy pair audit (OLD pair consumed status already covered by the completed legacy-pair-consumption-audit); fixed log: D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-1.txt (+ .sha256)
  - blocked-confirmation-freeze-static-review # independent_review_ready=true = contract/roles statically ready, NOT device/execution readiness; no review record required on the blocked confirmation freeze
  - host-prep-list-targets-mapping # ONE hdc list targets, memory-only, exactly one token, set process-scope PHYS_1_TARGET, no output/persistence; runner HDC whitelist unchanged
  - target-binding-confirm # machine fresh confirmation: 3 whitelisted probes (Version/TupleModel/TupleBuild), out-of-repo double-file record (JSON + .sha256 companion), never enters campaign roots
  - ready-freeze-draft-binds-record-hash # new out-of-repo ready freeze draft binds machine_fresh_confirmation.status=pass + authorization_id + record_path + record_sha256 (same confirmation contract as the blocked confirmation freeze)
  - independent-review-record # separate reviewer role writes an out-of-repo e3-ready-freeze-review record + .sha256 companion over the same freeze contract, binding machine_confirmation_sha256
  - final-ready-freeze-binds-review-record # final out-of-repo ready freeze (same confirmation contract bytes) binds independent_review_record.status=pass + reviewer_role + record_path + record_sha256
  - candidate-id-consumption-audit-2 # second external audit on the frozen HEAD before selftest/DryRun/Live; hash-recorded; fixed log: D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-2.txt (+ .sha256)
  - selftest-hdc0
  - dryrun-same-ready-freeze
  - review-dryrun-freeze-bytes-unchanged
  - single-live
dry_run_vs_live_freeze: DryRun accepts plan_status blocked or ready (always emits non-evidence blocked); Live requires plan_status ready only
retry_policy: no-auto-retry; this AUTH AUTH-E3-PHYS1API26-20260810-0002 fixes attempt=initial and retry.basis/infrastructure_reason=N/A for TargetBindingConfirm and for every ready Live/DryRun consuming its confirmation; the runner-enforced generic infrastructure retry branch (attempt=infrastructure-blocked-retry-1, allowlist hdc-usb-interruption|collection-storage-failure|runner-host-failure, frozen matching Live blocked evidence prior) can never enter this AUTH path; ANY gate failure stops the campaign with no retry and no ID switch; any later attempt requires new governance and a new authorization
live_gate_commit_authorization: user authorized committing/pushing the runner + governance changes (this three-file identity diff and this registration) after review and BEFORE Live; this overrides the 0001-era arrangement that the bundle commit be done Linux-side after review; campaign evidence (selftest/DryRun/Live outputs) still never commits ahead of Live; this registration task itself must not commit/push
verdict: N/A - authorization registration; not a live campaign verdict
scope_statement: whitelist-only per current runner plus the single memory-only hdc list targets host-prep exception; no extra discovery/UDID/serial/hidumper/root/privileged/Go/NetBird/product; user readiness attestation does not replace machine fresh confirmation; E3 open, E8 CLOSED
reviewer: pending-independent-review
reviewed_at: pending
```

## 授权摘要

| 字段 | 值 |
| --- | --- |
| AUTH ID | `AUTH-E3-PHYS1API26-20260810-0002`（取代 `AUTH-E3-PHYS1API26-20260810-0001`，旧授权已消耗） |
| 授权者 | 用户（直接人类决策者），2026-08-10 |
| authorization_status | `granted` |
| device_readiness | `user-attested-ready`（用户就绪声明，**不** 替代机器 fresh confirmation） |
| machine_fresh_confirmation | `pending` |
| plan_status | `authorized-awaiting-windows-ready-freeze` |
| 目标元组（不变，冻结于 `ADJ-20260806-0003`） | `PLA-AL10` / `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`（`EV-E3-PHYS1BUILD7-20260806-0001` 实测逐字匹配）/ API `26` / `aarch64` / `arm64-v8a`（rebind `EV-E3-PHYS1REBIND7-20260806-0001` 实测）/ 设备别名 `PHYS-1` |
| candidate（**新建**） | `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`，未 Live、未消耗；执行前须两次 ID 消费审计（audit-1/audit-2，均 hash 记录） |
| 旧 pair 处置 | `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` **已消费**（外部 sealed blocked evidence `EV-E3-PHYS1API26-20260808-0001`：scenario-results `record_status=collected`/`overall=blocked`/`verdict=blocked`、`execution_mode=live`、`is_evidence=true`、cleanup `verified-clean`；campaign-seal SHA-256 `ec55603a2555b967c5912e79a846262d262e53069e3f1275105af5ccf4ef245f`（sealed_at `2026-08-08T09:53:23+08:00`）、manifest SHA-256 `36852f6c6d9d6e27c6b5ee07bd6b6037e8aa2a22f1fdbf45591addb047f5f843`，位于仓外证据根 `D:/HarmonyEvidence/netbird-e3/EV-E3-PHYS1API26-20260808-0001/`；该 Live 发生于 08-08 host-remediation 登记（当时 candidate 未 Live）之后，两时间快照不冲突）；legacy-pair-consumption-audit 已完成（仓外审计日志 `D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260808-0001/audit/id-consumption-audit-1.txt`，SHA-256 `b530c438769e65390ee9065f918d1ea550a8f9745c61516b6899ff90c736e9c7`，2026-08-10T10:43:09+08:00）；**不得复用** |
| 允许范围 | 现行 runner 白名单（HDC target-binding `Version`/`TupleModel`/`TupleBuild`、A/B install/start/observation/mechanical prompts/final cleanup）+ **一次内存级 `hdc list targets` host-prep 窄例外**（见下） |
| 禁止 | 任何额外 discovery、UDID、serial、`hidumper`、root、privileged、Go、NetBird、product；`hdc list targets` 只允许一次且不输出/持久化 token |
| E8 | `CLOSED`（本授权不是 E8 `OPEN`） |

## 一次 `hdc list targets` host-prep 窄例外（唯一新增边界）

本授权在既有 runner 白名单之外新增 **唯一** 一项窄例外，仅用于在 Windows host 上把仓外目标绑定到 process-scope 环境：

- **命令**：`hdc list targets`，在 Windows signing/build host 上执行 **一次且仅一次**（host-prep 门，`-TargetBindingConfirm` 之前）。
- **范围**：仅 **内存** 取得输出；**不输出** 到任何记录/日志/回显、**不持久化** 到任何文件/环境持久层/仓库、**不写入** confirmation record / freeze / evidence / transcript。
- **验收**：输出必须恰一 target token；0 个、多个或格式异常 → **立即停止**，不猜测、不选择、不重跑。
- **用途**：把该 token 设置为 **process-scope** 环境变量 `PHYS_1_TARGET`（仅当次进程可见），供后续 runner 门（`-TargetBindingConfirm` / selftest / DryRun / Live）作为仓外受控 target 注入。
- **边界**：本例外 **不扩大 runner 的 HDC 白名单**——`hdc list targets` 是操作者的 host-prep 步骤，**不是** runner 命令，runner 自身仍只按白名单 argv 执行 `Version`/`TupleModel`/`TupleBuild` 与 A/B 定向操作；任何把 `list targets` 编入 runner、重跑、把 token 落盘或写进任何记录的行为都违反本授权。
- **host HDC server 注记（不新增任何授权）**：执行一次 `hdc list targets` 可能顺带启动 host 上正常的 HDC server 进程（由 hdc 客户端自管理，属正常 host 行为）；「内存级」承诺仅覆盖 **target token 不输出、不落盘**——本授权 **不** 附带任何额外 cleanup/stop-server/`hdc tconn`/retry 授权；输出 0 个或多个 token 时 **立即停止**，不清理、不重试、不猜测。
- 该窄例外由操作者执行一次并消耗；本登记期间不执行。

## 用户就绪声明与机器 fresh confirmation（`-TargetBindingConfirm`）

- `device_readiness: user-attested-ready` 只记录用户声明设备已准备好；它 **不** 替代、**不** 提前满足 `machine_fresh_confirmation`。
- `machine_fresh_confirmation: pending` 必须由 Windows host 上的 **机器** 观测完成：通过 runner 的 `-TargetBindingConfirm` 模式（HDC 白名单 `Version` + `TupleModel` + `TupleBuild` 共 3 次，逐字比对冻结 `PLA-AL10` / `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`）确认当前绑定设备仍为冻结元组。该模式：接受 `plan_status: blocked` 或 `ready` 的 confirmation freeze（`machine_fresh_confirmation.pending`），但完整校验 freeze 结构/clean repo/`code_sha`/runner/HDC/外部输入 hash/`PHYS_1_TARGET` 单 token，且 **固定候选 pair** `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001` 与 `attempt=initial`（`retry.basis`/`infrastructure_reason=N/A`；generic retry 分支不进入本路径）；**不** 初始化 `EvidenceRoot`/`RawRoot`、**不** 设置 `is_evidence`、**不** 消费 campaign/evidence ID、不进入 capture/install/start/最终 campaign cleanup。产出仓外 **双文件** confirmation record（JSON 临时文件 + `.sha256` 临时文件、复算 hash 后 atomic move JSON、最后 atomic move companion 作为 completion marker；`schema_version=1`、`record_kind: target-binding-confirmation`、`is_evidence: false`、`authorization_id: AUTH-E3-PHYS1API26-20260810-0002`、`exception: E3-PHYS-PREFLIGHT`、alias `PHYS-1`、`target_redacted: true`、expected/observed model+build、`command_attempted`/`command_completed`（pass 时均为 3）、`confirmation_contract_sha256`（**稳定两阶段投影**，见下，非完整 freeze contract）、started/ended、verdict `pass|blocked` + reason；禁止 target/serial/UDID/secret，observed version/model/build 与 reason 均先经 Protect-SensitiveText）。**退出语义**：pre-record 门失败（record/companion 已存在、路径在仓内、reparse 祖先）→ throw + exit 1 且 **不写任何文件**；probe/tuple 失败或 record 写入失败 → 尽力写 blocked record + companion + exit 2；pass 必须 `attempted=completed=3` 且双文件完成。companion 写失败可遗留 orphan JSON，但 **绝不可消费/绑定**（consumer 只接受双文件且 companion 与 record 字节一致）且 **禁止覆盖**。
- **consumer 完整校验**：`ready` Live 与 `ready` DryRun 强制 `machine_fresh_confirmation.status=pass`，且 record 必须仓外、无 reparse 祖先（record 与其 `.sha256` companion 均检查）、companion 存在且等于 record SHA-256；内容逐项核对 `schema_version=1`、record_kind、`is_evidence=false`、exception、AUTH ID、**exact candidate pair**、`attempt=initial`/retry N/A、`device_alias=PHYS-1`、`target_redacted=true`、verdict `pass`/reason `N/A`、code/runner/hdc SHA + `hdc_version`、`confirmation_contract_sha256` 等于当前 freeze 的 confirmation contract（稳定投影）、expected/observed model/build、`command_attempted=3`/`command_completed=3`、`started_at<=ended_at<=freeze.preflight_inputs_frozen_at`。**`blocked` 的 DryRun**：`status=pending`（或缺省）允许并跳过；若声明 `status=pass` 则同样 **完整校验**（blocked DryRun 不得藏起损坏的绑定）。`independent_review_record` 同规则：`blocked` DryRun 声明 review `status=pass` 时同样 **完整校验**（machine `status=pass` 且 review `status=pass` 时 review record 机械门完整执行），machine confirmation `pending`/缺省 而 review 声明 `pass` → **明确拒绝**（review record 绑定 machine confirmation hash，pending/absent machine 无法锚定），review `pending` 允许。**不引入任何未决策的小时有效期**：fresh 只验证顺序双锚——record `ended_at` 不晚于 **最终 ready freeze** 的 `preflight_inputs_frozen_at`，且 Live 在其自身 preflight 中再次执行 `Version`/`TupleModel`/`TupleBuild` 三条机器核对（fresh double anchor：确认时一次、Live 时一次）。
- 该确认在 ready freeze 生成 **之前** 执行；Live 时 runner precheck 会再次强制核对（漂移即停止，且 Live precheck 失败走正常 campaign 流程：定向 cleanup verification，不跳过）。
- 在机器 fresh confirmation 完成前，任何情况下不得进入 `plan_status: ready`、不得 Live。

## 两次新 ID 消费审计（audit-1 / audit-2）

新候选 identity `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001` 必须通过 **两次外部消费审计** 且每次 **hash 记录**：

1. **new-candidate-id-consumption-audit-1（同步 trusted refs/bundle 之后、任何 `-TargetBindingConfirm`/ready freeze 之前；针对新 pair，尚未执行）**：`git fetch`/检出同步后，审计仓库（`git log --all --grep` + evidence 目录与仓外 evidence roots 全文 grep）确认没有任何 `execution_mode=live` / `is_evidence=true` / seal 记录（scenario-results/campaign-seal/hash-manifest 或独立 evidence 文档）占用新 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`；将两次 grep 的输出与时间戳记入仓外审计日志（固定路径 `D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-1.txt` + `.sha256`，hash 记录）。**旧 pair 的消费确认已由独立的 legacy-pair-consumption-audit 完成**（`id-consumption-audit-1.txt`，SHA-256 `b530c438769e65390ee9065f918d1ea550a8f9745c61516b6899ff90c736e9c7`，2026-08-10T10:43:09+08:00）：外部 sealed blocked evidence `EV-E3-PHYS1API26-20260808-0001`（scenario-results `record_status=collected`/`overall=blocked`/`verdict=blocked`、`execution_mode=live`、`is_evidence=true`、cleanup `verified-clean`；campaign-seal SHA-256 `ec55603a2555b967c5912e79a846262d262e53069e3f1275105af5ccf4ef245f`（sealed_at `2026-08-08T09:53:23+08:00`）、manifest SHA-256 `36852f6c6d9d6e27c6b5ee07bd6b6037e8aa2a22f1fdbf45591addb047f5f843`）占用旧 pair，旧 pair **不得复用**。该 Live 发生在 08-08 host-remediation 登记（当时 candidate 未 Live）之后，与 08-08 文档时间快照不冲突。两次新 pair 审计（audit-1/audit-2）与已完成的 legacy-pair-consumption-audit 相互独立，各自 hash 记录。
2. **audit-2（最终 ready freeze 绑定 review record 之后、selftest/DryRun/Live 之前）**：在冻结 HEAD 上重跑同一审计并重新记录 hash（固定路径 `D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-2.txt` + `.sha256`），确认新 pair 仍未消耗。

**若任何一次发现占用记录，候选不得复用**：停止并回报主会话，等用户授权新 campaign/evidence ID；绝不静默改用其他 ID 或改写记录。

## 当前 Windows signing/build host preflight 复测（本登记时点，host-only 只读）

| 项 | 值 |
| --- | --- |
| host | Windows（`MINGW64_NT-10.0-26100` / `COMPUTERNAME=ALFADB-V-WIN`、`OS=Windows_NT`）——即授权登记所指的 Windows signing/build host 本机 |
| pwsh | `C:\Program Files\PowerShell\7\pwsh.exe` 存在，7.6.4 |
| HDC | `C:\Program Files\Huawei\DevEco Studio\sdk\default\openharmony\toolchains\hdc.exe` 存在，文件 SHA-256 `fff02abf2e61603e491e896aa6195e78db0c1779a6d7b992b89757a9a3c72116`（仅定位 + 文件 hash，**未执行**） |
| 仓外 freeze | 存在历史 freeze（`D:/HarmonySigning/netbird-e3/freeze/`：多组 20260806–08 旧候选 freeze；`e3fe0c6-api26-cn-ready-20260808` 已标 `SUPERSEDED-DO-NOT-EXECUTE`）；**全新 blocked / ready freeze 尚未生成** |
| signed HAP / profile / cert | 存在：`D:/HarmonySigning/netbird-e3/staging/e3fe0c6-adj0003-final/artifacts/a|b/e3-phys-preflight-{a,b}-signed.hap`（SHA256SUMS 公开 hash `1e902bdf…`/`abb598e9…`）、`profiles/a|b/NetBird E3 Debug {A,B}Debug.p7b`、`cert/NetBird E3 Debug.cer` |
| 受控 EvidenceRoot / RawRoot | 存在旧证据根（`D:/HarmonyEvidence/netbird-e3/`、`D:/HarmonyEvidenceRaw/netbird-e3/`，含 sealed `EV-E3-PHYS1API26-20260808-0001` 等）；**新 pair 的 EvidenceRoot / RawRoot 尚未初始化**（按顺序门由 runner 在对应模式初始化，本登记不初始化） |
| 仓外 `PHYS_1_TARGET` 映射 | **未建立**（当前 shell 未设置；须 host-prep 一次 `hdc list targets` 内存级建立 process-scope 映射） |
| 新 pair 消费审计 | audit-1 / audit-2 **均未执行**（固定仓外日志路径见「两次新 ID 消费审计」节：`campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-{1,2}.txt` + `.sha256`） |
| legacy-pair-consumption-audit | 已完成（`D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260808-0001/audit/id-consumption-audit-1.txt`，SHA-256 `b530c438…`，2026-08-10T10:43:09+08:00，与本登记一致） |
| HDC 进程数 | 0（未启动任何 HDC；仅定位与文件 hash，未执行） |
| 设备命令 | 未发出（本登记期间禁止） |
| 仓库 HEAD | 工作区基于 `main`（HEAD `324d0b5`）；未提交三文件 diff + 本登记文档待审查后提交（本任务禁止提交/推送） |
| host 状态 | `ready_for_commit_and_ordered_preflight`——工具与签名输入齐备，可审查后提交并按顺序门执行 preflight；**非全部 Live-ready**（未提交 clean final commit、新 pair audit-1、全新 blocked/ready freeze、confirmation/review records、新 EvidenceRoot/RawRoot、process-scope `PHYS_1_TARGET` 映射均尚待） |

因此本 Windows signing/build host **具备提交与按序 preflight 条件**（`ready_for_commit_and_ordered_preflight`），ready freeze 的生成、selftest、DryRun 与 Live 均须在本 host 按 0002 顺序门完成；**不得**表述为全部 Live-ready。

## 仓内固定字节与 clean-commit requirement

本次登记固定当前工作区三文件字节（**执行前必须复算并绑定最终 commit 的字节**，而非登记时快照）：

- runner：`e3-phys-preflight-campaign.ps1` → `e1da598d8cdf6bad3d243e393357c03ef40550ebab207714797d53e5a388ab41`
- freeze example：`e3-phys-preflight-freeze.example.json` → `5ea7acda269873a5d0ba2af2c0396ee6e3e62b9cf0990d83ddc55aadb49af62d`（**刻意保持 `plan_status: blocked`**，仅占位，不含 campaign 哈希/路径/秘密）
- selftest：`tests/e3-phys-preflight-runner-selftest.ps1` → `28c26b72b7ff9adaddc0eb734a4441f2e7ed4e16435af6f08851c03cf3a5d220`

以上为当前未提交工作区字节（含 `ADJ-20260810-0001` C6 审查两项 major 修复：selftest confirmation-contract 投影对 PowerShell 单元素数组 unroll 的 `Get-OptionalProperty` 处理等，以及三文件的新 AUTH/候选 ID 落地）。**clean-commit requirement**：最终 commit/hash 在 Live 前审查通过后提交时冻结——Windows host 必须在提交时**重新计算** **最终 commit 中** 这三个文件的 SHA-256，复算结果**预期与上方登记值一致**（登记值为当前工作区字节，本登记后这三文件不再改动；若复算不一致，说明文件在登记后被改动，必须停止并重新登记，**不得**绑定任何发散 hash）；最终 bundle commit 是权威字节源。freeze contract 包含 `operator_trust_model` / `scenario_invalid_policy` / `layout_verification_profile` / `vpn_conflict_rejection_codes` / `process_probe_target` 等决策字段，旧 freeze（如 20260807 candidate `INVALID-TIMELINE`、历史 runner 绑定）一律被 runner 拒绝，任何模式（DryRun 含）均不可用于新 Live。`ready` freeze 还须携带 `independent_review_record`（`status: pass` + `reviewer_role` + `record_path`/`record_sha256`）绑定独立审查 record（见下）。

## Windows 顺序门与 PowerShell 命令模板（cwd 均为 `spikes\e3-vpn-extension-physical-preflight-hap`）

Windows host 必须 **按序** 满足下列门，缺一即停止并回报主会话（所有路径均为占位符；不含 target/secret；除 host-prep 一次 `hdc list targets` 与 `-TargetBindingConfirm` 的 3 条白名单设备命令外均禁 HDC）：

1. **同步 trusted refs/bundle**：`git fetch` + 检出/核对冻结 HEAD，确认 worktree clean、HEAD 精确为冻结值；`git status --short --branch` 在签名、构建、freeze、selftest、DryRun、Live 前后检查。
2. **候选 ID 消费审计（new-candidate-id-consumption-audit-1，尚未执行）**：确认新 pair `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001` 未被任何 `execution_mode=live` / `is_evidence=true` / seal 记录占用；旧 pair 的消费确认不在此处重复执行——已由完成的 legacy-pair-consumption-audit 覆盖（`id-consumption-audit-1.txt` SHA-256 `b530c438769e65390ee9065f918d1ea550a8f9745c61516b6899ff90c736e9c7`，见「两次新 ID 消费审计」节）；grep 输出与时间戳记入仓外审计日志（固定路径 `D:/HarmonySigning/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260810-0001/audit/new-pair-id-consumption-audit-1.txt` + `.sha256`，hash 记录）；发现即停止，等用户新 ID 授权。
3. **blocked confirmation freeze 静态审查**：生成全新仓外 confirmation freeze（`plan_status: blocked` + `machine_fresh_confirmation.status: pending` + `independent_review_record.status: pending` + 全部外部 hash，候选 pair 为新三 ID），由独立审查角色静态审查契约与角色（0 blocker/0 major）后，freeze 中 `independent_review_ready` 为 `true`——此处 `independent_review_ready=true` 只表示 **契约与角色已静态就绪**，**不是** 设备或执行就绪，也不替代 `-TargetBindingConfirm` 的机器确认；**blocked confirmation freeze 不需要独立审查 record**。
4. **host-prep `hdc list targets` 映射（一次，内存级）**：执行 `hdc list targets` 恰一次；输出必须恰一 token，仅内存读取，设置 process-scope `PHYS_1_TARGET`；不输出、不持久化、不记录 token；0/多/异常 → 停止。**不扩大 runner HDC 白名单**。
5. **机器 fresh confirmation（`-TargetBindingConfirm`）**：

    ```powershell
    pwsh -NoProfile -File .\e3-phys-preflight-campaign.ps1 `
      -FreezeManifest C:\outside-repo\freeze-blocked-confirm.json `
      -ConfirmationRecord C:\outside-repo\target-binding-confirmation-20260810-0001.json `
      -HapA C:\outside-repo\final-a.hap `
      -HapB C:\outside-repo\final-b.hap `
      -HdcPath C:\tools\hdc.exe -TargetBindingConfirm
    ```

    判据：`RUNNER_RESULT=pass MODE=target-binding-confirm RECORD_KIND=target-binding-confirmation IS_EVIDENCE=false COMMAND_ATTEMPTED=3 COMMAND_COMPLETED=3 RECORD=<path> RECORD_SHA256=<sha>`；产出仓外 **双文件** record（JSON + `.sha256` companion，companion 为 completion marker），record 不含 target/serial/UDID/secret，**不** 创建 EvidenceRoot/RawRoot、**不** 消费 campaign/evidence ID。model/build 逐字匹配冻结；任何失败（含 drift）→ `RUNNER_RESULT=blocked` + **exit 2**（probe/tuple 失败；pre-record 门失败则 **exit 1 且无 record**）。drift 时该模式未安装任何 A/B、未建 staging、未起 capture，runner **不** 执行任何 cleanup/卸载/残留查询（这些操作不在本模式白名单内，也不属于 confirm 计划），无需设备端定向 cleanup；记录 blocked 后回报主会话。
6. **生成全新仓外 `ready` freeze draft（同 confirmation contract）**：新对象绑定步骤 1 的精确 clean HEAD、仓内 bytes（runner / freeze example 结构 / selftest 哈希，**以最终 commit 为准**）与全部完整外部 hash，并绑定步骤 5 的 record：`machine_fresh_confirmation.status=pass`、`authorization_id=AUTH-E3-PHYS1API26-20260810-0002`、`record_path=<record>`、`record_sha256=<sha>`；`plan_status: ready`；**confirmation contract 与步骤 3 的 blocked confirmation freeze 字节一致**（执行核心/候选/外部输入/code/runner/HDC/角色相同），而 `preflight_inputs_frozen_at` 推进到确认与审查 ended_at 之后（完整 freeze contract hash 因此不同，属预期）。**不得** 原地改旧 `blocked` freeze。
7. **独立审查 record（ready freeze 机械门）**：独立审查角色（`independent_reviewer_role`，与 operator 不同）对该 ready freeze 写出仓外 review record（`record_kind: e3-ready-freeze-review`、`is_evidence: false`、`schema_version: 1`、verdict `pass`、`blockers: 0`、`majors: 0`、`reviewer_role` 匹配、exact candidate pair/code_sha/runner_sha256/`confirmation_contract_sha256` 一致（稳定投影，与步骤 5 的 confirmation record 绑定同一 confirmation contract）、`machine_confirmation_sha256` 等于步骤 5 record 的 SHA-256、started/ended ≤ 最终 ready freeze 的 `preflight_inputs_frozen_at`）＋ `.sha256` companion。
8. **最终 `ready` freeze（同 confirmation contract，绑定 review record）**：新仓外对象，confirmation contract 与步骤 6 相同字节，绑定 `independent_review_record.status=pass` + `reviewer_role` + `record_path`/`record_sha256`（步骤 7 的 record）。
9. **候选 ID 消费审计（audit-2）**：在冻结 HEAD 上重跑步骤 2 的审计并重新记录 hash（见「两次新 ID 消费审计」节）。
10. **selftest（host-only，`HDC_PROCESSES=0`）**：

    ```powershell
    pwsh -NoProfile -File .\tests\e3-phys-preflight-runner-selftest.ps1
    ```

11. **DryRun（host-only，非证据；对同一份最终 `ready` freeze）**：

    ```powershell
    pwsh -NoProfile -File .\e3-phys-preflight-campaign.ps1 `
      -FreezeManifest C:\outside-repo\freeze-ready.json `
      -EvidenceRoot C:\outside-repo\evidence-dry-run `
      -RawRoot C:\outside-repo\evidence-dry-run.raw `
      -HapA C:\outside-repo\final-a.hap `
      -HapB C:\outside-repo\final-b.hap `
      -HdcPath C:\tools\hdc.exe -DryRun
    ```

    DryRun 判据：`is_evidence: false`、`HDC_PROCESSES=0`、`integrity_violations: []`、产出显式非证据 blocked record。`ready` freeze 的 DryRun 与 Live 一样验证 `machine_fresh_confirmation` 绑定（status=pass/authorization_id/record_path+sha/内容一致）与 `independent_review_record` 绑定（status=pass/reviewer_role/record_path+sha/内容一致）；`blocked` DryRun 允许 `pending`。
12. **审查 DryRun 且 freeze 字节不变**：独立审查角色核对 DryRun 结果与 ready freeze 契约（0 blocker/0 major）后确认 **ready freeze 文件字节与步骤 8 完全一致**（重新计算 freeze SHA-256，不得改动任何字段，包括 `plan_status`）；此时方视为 Live 输入就绪。
13. **单次 Live（唯一一次；只接受步骤 8 的同一份 `plan_status: ready` freeze）**：

    ```powershell
    pwsh -NoProfile -File .\e3-phys-preflight-campaign.ps1 `
      -FreezeManifest C:\outside-repo\freeze-ready.json `
      -EvidenceRoot C:\outside-repo\evidence-live `
      -RawRoot C:\outside-repo\evidence-live.raw `
      -HapA C:\outside-repo\final-a.hap `
      -HapB C:\outside-repo\final-b.hap `
      -HdcPath C:\tools\hdc.exe
    ```

    Live 需要操作员人工机械输入；runner 在 continuous capture 前强制再次执行 `Version`/`TupleModel`/`TupleBuild` 机器核对（fresh double anchor，漂移即停止）。Live 的 preflight 属正常 campaign 流程：drift 停止后仍执行定向 cleanup verification（与 confirm 模式不同——confirm 模式不执行任何 cleanup 查询）。

**freeze 状态规则**：`-DryRun` 接受 `plan_status: blocked` 或 `ready`（永远产出非证据 blocked record）；`-TargetBindingConfirm` 接受 `plan_status: blocked` 或 `ready`（consume `machine_fresh_confirmation.pending`，产出 non-evidence confirmation record；强制 exact candidate pair + `attempt=initial` + retry N/A）；`Live`/`LiveSimulation` **只** 接受 `plan_status: ready`（runner `Assert-FreezeManifest` 强制；Live 的 `ready` 还强制 `machine_fresh_confirmation` 与 `independent_review_record` 绑定，见上）。旧 `blocked` candidate freeze 不得原地改为 `ready`；`ready` freeze 必须是新仓外对象。confirmation record 状态：`record_kind: target-binding-confirmation`、`is_evidence: false`、`record_status: N/A`（非 campaign record），不进 evidence 目录、不占 campaign/evidence ID、可被 `ready` freeze 通过 `record_path`+`record_sha256` 绑定引用；review record 同理（`record_kind: e3-ready-freeze-review`、`is_evidence: false`）。sealed campaign complete record 与 preflight transcript 投影 `machine_fresh_confirmation`/`independent_review_record`：仅 status/authorization_id/reviewer_role/record_sha256/`record_path_sha256`（不泄露真实路径）并绑定 **confirmation contract**（`confirmation_contract_sha256`）；sealed complete record 顶层同时投影标准最终 `freeze_contract_sha256`（完整契约）与稳定 `confirmation_contract_sha256` 两个字段。

**两阶段 confirmation contract 规则**：`Get-FreezeContract`（完整契约）含治理/时间字段 `preflight_inputs_frozen_at`。blocked confirmation freeze 在机器确认前冻结（`T1`），最终 ready freeze 必须满足时间门（`started<=ended<=preflight_inputs_frozen_at`）而推进到 `T2 > T1`——因此 blocked freeze / ready draft / final ready freeze 的 **完整 freeze contract hash 允许且必然不同**（frozen_at 治理字段不同），但三者的 **confirmation contract 必须字节相同**（执行核心/候选 pair/外部输入/code/runner/HDC/角色；排除 `plan_status`、`preflight_inputs_frozen_at`、`machine_fresh_confirmation`、`independent_review_record`、`independent_review_ready`）。confirmation/review record 一律绑定 confirmation contract；若两阶段间 confirmation contract 发生变化，consumer 拒绝（`confirmation_contract_sha256 does not match`），防篡改绑定不因 frozen_at 推进而失效。

## 重试纪律

- `blocked`、`fail`、`invalid` 或 **no seal**（未完成封印/证据不完整）之后 **不得自动重跑**，也不得自动分配新 ID。
- **当前 AUTH 固定 `attempt=initial`（retry N/A）**：`-TargetBindingConfirm` 与消费本 AUTH confirmation 的 `ready` Live/DryRun 均被 runner 强制 exact candidate pair + `attempt=initial` + `retry.basis`/`infrastructure_reason=N/A`——因此 **现有 generic retry 分支（`attempt: infrastructure-blocked-retry-1`）不进入本次路径**。**任一 gate 失败即停止，不重试、不换 ID**。任何后续尝试必须 **新治理**：先取得新的路线决策并重新登记新授权；当前 AUTH `AUTH-E3-PHYS1API26-20260810-0002` 不可用于任何 retry。
- runner 既有 generic retry 规则仍保留给未来新治理：唯一允许的重试为 `attempt: infrastructure-blocked-retry-1`，`retry.infrastructure_reason` 必须命中白名单 `hdc-usb-interruption` / `collection-storage-failure` / `runner-host-failure`，且 `prior_record` 必须是冻结匹配的 **Live blocked evidence record**（`is_evidence: true`、`record_status: blocked`、`overall: blocked`、`verdict: blocked`、同 campaign/attempt/code/runner/artifact/freeze contract）。prior 为 DryRun、非证据、无 seal 或 `is_evidence: false` 时不构成 retry 依据；`-TargetBindingConfirm` 的 blocked confirmation record（`is_evidence: false`）同样 **不** 构成 retry 依据。
- 本授权不豁免任何既有纪律：build drift、operator-aborted、功能 fail、scenario invalid、integrity violation、非基础设施 blocked 均不授权 retry；`consumed-blocked` / `superseded-unexecuted` / `INVALID-TIMELINE` 历史 ID 一律不得复用（含旧 pair `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001`）。
- 任何继续或重试必须先取得新的路线决策并重新登记。

## 门状态

- E3 未关闭；本记录是授权登记，不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`；本授权不是 E8 `OPEN`，也不改变 E1/E4-E7 聚合状态。本实现不扩展授权边界：`-TargetBindingConfirm` 只复用现行 runner 白名单 `Version`/`TupleModel`/`TupleBuild` 三条 target-binding 命令，唯一新增为一次内存级 `hdc list targets` host-prep 窄例外（不扩大 runner HDC 白名单），不新增任何 discovery/UDID/serial/`hidumper`/cleanup/privileged 能力。
- `plan_status: authorized-awaiting-windows-ready-freeze`：Windows 顺序门（同步 trusted refs/bundle → audit-1 → blocked confirmation freeze 静态审查 → host-prep `hdc list targets` 映射 → `-TargetBindingConfirm` → ready draft 绑定 record → 独立审查 record → 最终 ready freeze 绑定 review → audit-2 → selftest → 同一 ready freeze DryRun → 审查 DryRun 且 freeze 字节不变 → 单次 Live）完成前不可执行。
- **Live 前提交/推送**：用户授权在 Live 前将 runner + 治理变更审查后提交/推送（覆盖旧 0001 禁令），但 campaign evidence 仍不提前提交；**本登记任务未提交/推送**，提交由 Live 前的独立审查步骤执行。
- 历史记录全部保留不改写：API23 initial（`EV-E3-PHYS1API23-20260806-0001`）、rebind（`EV-E3-PHYS1REBIND7-20260806-0001`）、build 确认（`EV-E3-PHYS1BUILD7-20260806-0001`）、API26 0001（`EV-E3-PHYS1API26-20260807-0001`，`consumed-blocked`）、API26 0002（`EV-E3-PHYS1API26-20260807-0002`，`consumed-blocked`）、host remediation（`EV-E3-PHYS1HOST-20260808-0001`）、外部 sealed blocked evidence `EV-E3-PHYS1API26-20260808-0001`（`record_status=collected`/`overall=blocked`/`verdict=blocked`，占用旧 pair 故旧 pair 已消费；campaign-seal SHA-256 `ec55603a2555b967c5912e79a846262d262e53069e3f1275105af5ccf4ef245f`、manifest SHA-256 `36852f6c6d9d6e27c6b5ee07bd6b6037e8aa2a22f1fdbf45591addb047f5f843`）、`ADJ-20260808-0001/0002/0003` 登记、旧授权 `AUTH-E3-PHYS1API26-20260810-0001`（superseded，consumed-audit 见旧文件顶部 note）。
