# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-10）

最后核验：2026-08-10

本文登记用户（直接人类决策者）于 2026-08-10 显式授予的 `E3-PHYS-PREFLIGHT` 物理设备执行授权，以及 Windows signing/build host handoff 的治理前置门。用户本次选择为：「回 Windows signing/build host：用原仓外签名对象重生 ready freeze，再 selftest、DryRun、Live」，并明确「授权完整白名单 campaign，设备现在已准备好」。

本文是 **授权登记**，**不是** live campaign 证据、**不是** 设备实测记录。本 Linux host 登记期间 **不** 运行任何 HDC/设备命令、**不** 安装工具、**不** 生成 ready freeze；机器 fresh device confirmation 由 Windows signing/build host 通过现行 runner 的 `-TargetBindingConfirm` 模式执行（仅 3 条白名单 target-binding 命令，产出仓外 confirmation record + `.sha256` companion，**不** 在仓内写入任何内容、不产生任何 git commit；后续的仓内 runner/freeze example/selftest 修复与最终 bundle commit 由本 Linux 侧在审查通过后统一提交，作为权威字节源）。它 **不** 创建新 campaign/evidence ID：候选 `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 保持 `pending-windows-out-of-repo-consumption-audit`，执行前必须先做两次候选 ID 消费审计。**用户就绪声明不替代机器 fresh device confirmation**。E3 未关闭，E8 保持 `CLOSED`。

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260810-0001
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: collected
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: not-run-authorization-registration
is_evidence: false
authorization_status: granted
device_readiness: user-attested-ready # user readiness attestation does NOT replace machine fresh confirmation
machine_fresh_confirmation: pending # completed by the Windows host via -TargetBindingConfirm; the current AUTH fixes attempt=initial and the candidate pair below (retry N/A); any later retry requires new governance
plan_status_at_registration: authorized-awaiting-windows-ready-freeze
campaign_status: candidate-retained-not-live
independent_review_record: pending # ready Live / ready DryRun require a pass review record (record_kind=e3-ready-freeze-review, is_evidence=false) with an out-of-repo JSON + .sha256 companion; independent_review_ready=true on a blocked confirmation freeze is only a static contract/role readiness marker and never an execution gate
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
hdc_target_reference: controlled external mapping; repository alias PHYS-1 only
linux_host_preflight:
  head_sha: 79d0b0d078110634ec91b8b91c2fcd990b8551ec
  worktree_clean: true
  pwsh: missing
  hdc: missing
  freeze: missing
  signed_hap: missing
  profile: missing
  cert: missing
  manifests: missing
  evidence_roots: missing
  phys_target_mapping: missing
  hdc_process_count: 0
  device_commands_issued: false
  host_ready: no
repo_bytes:
  runner_sha256: 7ede1e2f295d0e91ce265b876b23eefced452d34271b7e6346e1c178d21a2a52 # spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-campaign.ps1
  freeze_example_sha256: 28fd8da91a640542ee16636a5b1135b3ac4519d0ecb0ee6262896b16aac2ee2a # e3-phys-preflight-freeze.example.json (intentionally blocked)
  selftest_sha256: d32c7c6071c27b78fcd7947d37ea4a3ee8feb116baf3dd105630fffce34a0367 # tests/e3-phys-preflight-runner-selftest.ps1
  hash_note: these SHA-256 are registration-time snapshots; the runner/freeze example/selftest bytes were updated by ADJ-20260810-0001 (C6) review fixes afterwards, so the FINAL git commit that bundles this authorization and these files is authoritative. The Windows host must recompute and bind the SHA-256 of the exact committed bytes, never the values above
final_candidate_bytes: # post-ADJ-20260810-0001 (C6) review-fix bytes of the three files; pending commit, but these files themselves are the authoritative Windows bundle candidate bytes (recompute at commit time and bind the committed values)
  runner_sha256: 690a5af31177828ca51ef3e1bc2c897b4c6f245ed76d8c9f738e74a2aba93424 # spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-campaign.ps1
  freeze_example_sha256: b74aa5d078a81e8ff2068e4d0888f015929ade746c4a712d30e9eb226295cd0c # e3-phys-preflight-freeze.example.json (intentionally blocked)
  selftest_sha256: cc02a7a091ce4104e87b24b815988883c9c0fa7b5ee6da79a9fe23404741a185 # tests/e3-phys-preflight-runner-selftest.ps1
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260808-0001
  evidence_id: EV-E3-PHYS1API26-20260808-0001
  attempt: initial # the current AUTH fixes attempt=initial with retry.basis/infrastructure_reason=N/A; the generic infrastructure retry branch never applies to this AUTH path
  identity_status: pending-windows-out-of-repo-consumption-audit # candidate IDs are NOT yet consumable; an ID consumption audit must run on the Windows host first
  live: false
  consumed: false
  note: candidate IDs retained; this authorization creates no new IDs. The candidate identity is pending-windows-out-of-repo-consumption-audit: before any TargetBindingConfirm or ready freeze, the Windows host must audit that no execution_mode=live / is_evidence=true / sealed record occupies E3-PHYS-PREFLIGHT-20260808-0001 or EV-E3-PHYS1API26-20260808-0001 (repo grep + out-of-repo evidence roots). If ANY such record exists the candidate must NOT be reused; stop and wait for the user to authorize new IDs. The ready freeze must be a NEW out-of-repository object binding the exact clean HEAD plus the repository bytes above and full external hashes; the old blocked candidate freeze must not be edited in place
windows_gate_order:
  - candidate-id-consumption-audit-1 # after syncing trusted refs/bundle; hash-recorded
  - blocked-confirmation-freeze-static-review # independent_review_ready=true here means the contract and roles are statically ready, NOT device or execution readiness; no review record is required on the blocked confirmation freeze
  - target-binding-confirm # machine fresh confirmation: 3 whitelisted probes, out-of-repo double-file record (JSON + .sha256 companion), never enters campaign roots
  - ready-freeze-draft-binds-record-hash # new out-of-repo ready freeze draft binds machine_fresh_confirmation.status=pass + authorization_id + record_path + record_sha256 (same contract as the blocked confirmation freeze)
  - independent-review-record # separate reviewer role writes an out-of-repo e3-ready-freeze-review record + .sha256 companion over the same freeze contract, binding machine_confirmation_sha256
  - final-ready-freeze-binds-review-record # final out-of-repo ready freeze (same contract bytes) binds independent_review_record.status=pass + reviewer_role + record_path + record_sha256
  - candidate-id-consumption-audit-2 # second external audit before selftest/DryRun/Live; hash-recorded
  - selftest-hdc0
  - dryrun-same-ready-freeze
  - review-dryrun-freeze-bytes-unchanged
  - single-live
dry_run_vs_live_freeze: DryRun accepts plan_status blocked or ready (always emits non-evidence blocked); Live requires plan_status ready only
retry_policy: no-auto-retry; the current AUTH AUTH-E3-PHYS1API26-20260810-0001 fixes attempt=initial and retry.basis/infrastructure_reason=N/A for TargetBindingConfirm and for every ready Live/DryRun consuming its confirmation; the runner-enforced generic infrastructure retry branch (attempt=infrastructure-blocked-retry-1, allowlist hdc-usb-interruption|collection-storage-failure|runner-host-failure, frozen matching Live blocked evidence prior with is_evidence=true/record_status=blocked/overall=blocked/verdict=blocked) can never enter this AUTH path; any retry after this campaign requires new governance and a new authorization; a TargetBindingConfirm blocked record (is_evidence=false) never authorizes a retry
verdict: N/A - authorization registration; not a live campaign verdict
scope_statement: whitelist-only per current runner; no extra discovery/UDID/serial/hidumper/root/privileged/Go/NetBird/product; user readiness attestation does not replace machine fresh confirmation; E3 open, E8 CLOSED
reviewer: pending-independent-review
reviewed_at: pending
```

## 授权摘要

| 字段 | 值 |
| --- | --- |
| AUTH ID | `AUTH-E3-PHYS1API26-20260810-0001` |
| 授权者 | 用户（直接人类决策者），2026-08-10 |
| authorization_status | `granted` |
| device_readiness | `user-attested-ready`（用户就绪声明，**不** 替代机器 fresh confirmation） |
| machine_fresh_confirmation | `pending` |
| plan_status | `authorized-awaiting-windows-ready-freeze` |
| 目标元组（不变，冻结于 `ADJ-20260806-0003`） | `PLA-AL10` / `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`（`EV-E3-PHYS1BUILD7-20260806-0001` 实测逐字匹配）/ API `26` / `aarch64` / `arm64-v8a`（rebind `EV-E3-PHYS1REBIND7-20260806-0001` 实测）/ 设备别名 `PHYS-1` |
| candidate（保留，不创建新 ID；identity `pending-windows-out-of-repo-consumption-audit`） | `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001`，未 Live、未消耗；执行前须 ID 消费审计（见下） |
| 允许范围 | 仅现行 runner 白名单：HDC target-binding 复核（`Version` / `TupleModel` / `TupleBuild`，经 `-TargetBindingConfirm`）、A/B install/start/observation/mechanical prompts/final cleanup |
| 禁止 | 任何额外 discovery、UDID、serial、`hidumper`、root、privileged、Go、NetBird、product |
| E8 | `CLOSED`（本授权不是 E8 `OPEN`） |

## 授权范围（唯一例外边界内，与现行 runner 白名单一致）

本授权覆盖 `E3-PHYS-PREFLIGHT` 唯一例外的单次完整白名单 campaign，范围 **仅限** 现行 runner `spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-campaign.ps1`（仓内 SHA-256 以最终 commit 绑定为准；本登记时点快照 `7ede1e2f…` 已被 `ADJ-20260810-0001` C6 审查修复更新）在 `ADJ-20260808-0001/0002/0003` 与 `ADJ-20260810-0001` 下定义的白名单操作集：

- **HDC target-binding 复核**：`Version`（冻结 HDC 版本比对）、`TupleModel`（`param get const.product.model`）、`TupleBuild`（`param get const.product.software.version`），逐字比对冻结元组；漂移即停止（`preflight: model/build precheck drifted before continuous capture`）。这是 **机器 fresh confirmation** 的机制，零新增身份信息。
- **A/B 定向操作**：`BundleDump`（bundle 安装存在观察）、`PidOf`（仅定向精确 `<bundle>:vpn` Extension 能力进程，禁止宽泛 process list）、`MkdirStaging`、`SendA`/`SendB`、`InstallA`/`InstallB`、`StartEntry`、`FaultA`/`FaultB`、`Uninstall`、`RemoveStaging`、`StagingProbe`。
- **observation / mechanical prompts / final cleanup**：单一连续 `E3PhysVpn` HiLog 采集与字节锚点、screen/layout 采集与机器判定、操作员每步「现在只做：X。完成后按回车。」机械提示（`mechanical-action-only-machine-verified-v1`）、finally 残留清理（HDC force-stop 仅 cleanup-only，Reason 限 `exception-cleanup`/`final-cleanup`，永不用于 S5 撤销或任何场景判定）。

**明确禁止**：任何额外 discovery / 重查 dist/model/build/API/arch/ABI、UDID（`bm get --udid` 长选项 enrollment 例外已消耗、短选项 `bm get -u` 仍禁止）、serial、`hidumper`、`uiInput`、全量查询、root/system/debug/enterprise 权限、`MANAGE_VPN`、隐藏服务、Go、NetBird、WireGuard、私有 fork、产品代码或任何超出本列表的操作。真实 target 只从仓外 `PHYS_1_TARGET` 注入。

## 用户就绪声明与机器 fresh confirmation（`-TargetBindingConfirm`）

- `device_readiness: user-attested-ready` 只记录用户声明设备已准备好；它 **不** 替代、**不** 提前满足 `machine_fresh_confirmation`。
- `machine_fresh_confirmation: pending` 必须由 Windows host 上的 **机器** 观测完成：通过 runner 的 `-TargetBindingConfirm` 模式（HDC 白名单 `Version` + `TupleModel` + `TupleBuild` 共 3 次，逐字比对冻结 `PLA-AL10` / `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`）确认当前绑定设备仍为冻结元组。该模式：接受 `plan_status: blocked` 或 `ready` 的 confirmation freeze（`machine_fresh_confirmation.pending`），但完整校验 freeze 结构/clean repo/`code_sha`/runner/HDC/外部输入 hash/`PHYS_1_TARGET` 单 token，且 **固定候选 pair** `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 与 `attempt=initial`（`retry.basis`/`infrastructure_reason=N/A`；generic retry 分支不进入本路径）；**不** 初始化 `EvidenceRoot`/`RawRoot`、**不** 设置 `is_evidence`、**不** 消费 campaign/evidence ID、不进入 capture/install/start/最终 campaign cleanup。产出仓外 **双文件** confirmation record（JSON 临时文件 + `.sha256` 临时文件、复算 hash 后 atomic move JSON、最后 atomic move companion 作为 completion marker；`schema_version=1`、`record_kind: target-binding-confirmation`、`is_evidence: false`、`authorization_id: AUTH-E3-PHYS1API26-20260810-0001`、`exception: E3-PHYS-PREFLIGHT`、alias `PHYS-1`、`target_redacted: true`、expected/observed model+build、`command_attempted`/`command_completed`（pass 时均为 3）、`confirmation_contract_sha256`（**稳定两阶段投影**，见下，非完整 freeze contract）、started/ended、verdict `pass|blocked` + reason；禁止 target/serial/UDID/secret，observed version/model/build 与 reason 均先经 Protect-SensitiveText）。**退出语义**：pre-record 门失败（record/companion 已存在、路径在仓内、reparse 祖先）→ throw + exit 1 且 **不写任何文件**；probe/tuple 失败或 record 写入失败 → 尽力写 blocked record + companion + exit 2；pass 必须 `attempted=completed=3` 且双文件完成。companion 写失败可遗留 orphan JSON，但 **绝不可消费/绑定**（consumer 只接受双文件且 companion 与 record 字节一致）且 **禁止覆盖**。
- **consumer 完整校验**：`ready` Live 与 `ready` DryRun 强制 `machine_fresh_confirmation.status=pass`，且 record 必须仓外、无 reparse 祖先（record 与其 `.sha256` companion 均检查）、companion 存在且等于 record SHA-256；内容逐项核对 `schema_version=1`、record_kind、`is_evidence=false`、exception、AUTH ID、**exact candidate pair**、`attempt=initial`/retry N/A、`device_alias=PHYS-1`、`target_redacted=true`、verdict `pass`/reason `N/A`、code/runner/hdc SHA + `hdc_version`、`confirmation_contract_sha256` 等于当前 freeze 的 confirmation contract（稳定投影）、expected/observed model/build、`command_attempted=3`/`command_completed=3`、`started_at<=ended_at<=freeze.preflight_inputs_frozen_at`。**`blocked` 的 DryRun**：`status=pending`（或缺省）允许并跳过；若声明 `status=pass` 则同样 **完整校验**（blocked DryRun 不得藏起损坏的绑定）。`independent_review_record` 同规则：`blocked` DryRun 声明 review `status=pass` 时同样 **完整校验**（machine `status=pass` 且 review `status=pass` 时 review record 机械门完整执行），machine confirmation `pending`/缺省 而 review 声明 `pass` → **明确拒绝**（review record 绑定 machine confirmation hash，pending/absent machine 无法锚定），review `pending` 允许。**不引入任何未决策的小时有效期**：fresh 只验证顺序双锚——record `ended_at` 不晚于 **最终 ready freeze** 的 `preflight_inputs_frozen_at`，且 Live 在其自身 preflight 中再次执行 `Version`/`TupleModel`/`TupleBuild` 三条机器核对（fresh double anchor：确认时一次、Live 时一次）。
- 该确认在 ready freeze 生成 **之前** 执行；Live 时 runner precheck 会再次强制核对（漂移即停止，且 Live precheck 失败走正常 campaign 流程：定向 cleanup verification，不跳过）。
- 在机器 fresh confirmation 完成前，任何情况下不得进入 `plan_status: ready`、不得 Live。

## 候选 ID 消费审计（`pending-windows-out-of-repo-consumption-audit`）

候选 identity `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 现为 `pending-windows-out-of-repo-consumption-audit`。Windows host **执行两次外部候选消费审计并记录 hash**：

1. **第一次（同步 trusted refs/bundle 之后、任何 `-TargetBindingConfirm`/ready freeze 之前）**：`git fetch`/检出同步后，审计仓库（`git log --all --grep` + evidence 目录与仓外 evidence roots 全文 grep）确认没有任何 `execution_mode=live` / `is_evidence=true` / seal 记录（scenario-results/campaign-seal/hash-manifest 或独立 evidence 文档）占用这两个 ID；将两次 grep 的输出与时间戳记入仓外审计日志（hash 记录）。
2. **第二次（最终 ready freeze 绑定 review record 之后、selftest/DryRun/Live 之前）**：在冻结 HEAD 上重跑同一审计并重新记录 hash，确认候选仍未消耗。

**若任何一次发现此类记录，候选不得复用**：停止并回报主会话，等用户授权新 campaign/evidence ID；绝不静默改用其他 ID 或改写记录。

## 当前 Linux host preflight 登记（本登记时点）

| 项 | 值 |
| --- | --- |
| 仓库 HEAD | `79d0b0d078110634ec91b8b91c2fcd990b8551ec`（main） |
| worktree | clean（无未提交改动） |
| pwsh | missing |
| HDC | missing |
| 仓外 freeze | missing |
| signed HAP / profile / cert | missing |
| build/source/blob manifests | missing |
| controlled EvidenceRoot / RawRoot | missing |
| 仓外 `PHYS_1_TARGET` 映射 | missing |
| HDC 进程数 | 0（未启动任何 HDC） |
| 设备命令 | 未发出（本登记期间禁止） |
| host_ready | **no** |

因此本 Linux host **不** 具备执行条件，ready freeze 的生成、selftest、DryRun 与 Live 均须在 Windows signing/build host 完成。

## 仓内固定字节与全新 ready freeze 要求

本次登记固定当前仓内相关字节（完整 SHA-256 见上方 YAML，**本登记时点快照**）：

- runner：`e3-phys-preflight-campaign.ps1` → `7ede1e2f…`（`ADJ-20260808-0001/0002/0003` 现行语义；`ADJ-20260810-0001` C6 审查修复后再变）
- freeze example：`e3-phys-preflight-freeze.example.json` → `28fd8da9…`（**刻意保持 `plan_status: blocked`**，仅占位，不含 campaign 哈希/路径/秘密）
- selftest：`tests/e3-phys-preflight-runner-selftest.ps1` → `d32c7c60…`

**旧 hash 说明以最终 commit 为准**：上方 YAML 中的 runner/freeze example/selftest SHA-256 是 **本登记时点** 的字节快照；`ADJ-20260810-0001` 的 C6 审查修复已更新这三个文件的字节，Windows host 重建时必须 **重新计算并绑定最终 commit 中这三个文件的新 SHA-256**（不能绑定上方旧值；最终 bundle commit 是权威字节源）。freeze contract 包含 `operator_trust_model` / `scenario_invalid_policy` / `layout_verification_profile` / `vpn_conflict_rejection_codes` / `process_probe_target` 等决策字段，旧 freeze（如 20260807 candidate `INVALID-TIMELINE`、历史 runner 绑定）一律被 runner 拒绝，任何模式（DryRun 含）均不可用于新 Live。`ready` freeze 还须携带 `independent_review_record`（`status: pass` + `reviewer_role` + `record_path`/`record_sha256`）绑定独立审查 record（见下）。

## Windows 顺序门与 PowerShell 命令模板（cwd 均为 `spikes\e3-vpn-extension-physical-preflight-hap`）

Windows host 必须 **按序** 满足下列门，缺一即停止并回报主会话（所有路径均为占位符；不含 target/secret；除 `-TargetBindingConfirm` 的 3 条白名单设备命令外均禁 HDC）：

1. **同步 trusted refs/bundle**：`git fetch` + 检出/核对冻结 HEAD，确认 worktree clean、HEAD 精确为冻结值；`git status --short --branch` 在签名、构建、freeze、selftest、DryRun、Live 前后检查。
2. **候选 ID 消费审计（第一次）**：确认 `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 未被任何 `execution_mode=live` / `is_evidence=true` / seal 记录占用，grep 输出与时间戳记入仓外审计日志（hash 记录；见「候选 ID 消费审计」节）；发现即停止，等用户新 ID 授权。
3. **blocked confirmation freeze 静态审查**：生成全新仓外 confirmation freeze（`plan_status: blocked` + `machine_fresh_confirmation.status: pending` + `independent_review_record.status: pending` + 全部外部 hash），由独立审查角色静态审查契约与角色（0 blocker/0 major）后，freeze 中 `independent_review_ready` 为 `true`——此处 `independent_review_ready=true` 只表示 **契约与角色已静态就绪**，**不是** 设备或执行就绪，也不替代 `-TargetBindingConfirm` 的机器确认；**blocked confirmation freeze 不需要独立审查 record**。
4. **机器 fresh confirmation（`-TargetBindingConfirm`）**：

    ```powershell
    pwsh -NoProfile -File .\e3-phys-preflight-campaign.ps1 `
      -FreezeManifest C:\outside-repo\freeze-blocked-confirm.json `
      -ConfirmationRecord C:\outside-repo\target-binding-confirmation-20260810-0001.json `
      -HapA C:\outside-repo\final-a.hap `
      -HapB C:\outside-repo\final-b.hap `
      -HdcPath C:\tools\hdc.exe -TargetBindingConfirm
    ```

    判据：`RUNNER_RESULT=pass MODE=target-binding-confirm RECORD_KIND=target-binding-confirmation IS_EVIDENCE=false COMMAND_ATTEMPTED=3 COMMAND_COMPLETED=3 RECORD=<path> RECORD_SHA256=<sha>`；产出仓外 **双文件** record（JSON + `.sha256` companion，companion 为 completion marker），record 不含 target/serial/UDID/secret，**不** 创建 EvidenceRoot/RawRoot、**不** 消费 campaign/evidence ID。model/build 逐字匹配冻结；任何失败（含 drift）→ `RUNNER_RESULT=blocked` + **exit 2**（probe/tuple 失败；pre-record 门失败则 **exit 1 且无 record**）。drift 时该模式未安装任何 A/B、未建 staging、未起 capture，runner **不** 执行任何 cleanup/卸载/残留查询（这些操作不在本模式白名单内，也不属于 confirm 计划），无需设备端定向 cleanup；记录 blocked 后回报主会话。
5. **生成全新仓外 `ready` freeze draft（同 confirmation contract）**：新对象绑定步骤 1 的精确 clean HEAD、仓内 bytes（runner / freeze example 结构 / selftest 哈希，以最终 commit 为准）与全部完整外部 hash，并绑定步骤 4 的 record：`machine_fresh_confirmation.status=pass`、`authorization_id=AUTH-E3-PHYS1API26-20260810-0001`、`record_path=<record>`、`record_sha256=<sha>`；`plan_status: ready`；**confirmation contract 与步骤 3 的 blocked confirmation freeze 字节一致**（执行核心/候选/外部输入/code/runner/HDC/角色相同），而 `preflight_inputs_frozen_at` 推进到确认与审查 ended_at 之后（完整 freeze contract hash 因此不同，属预期）。**不得** 原地改旧 `blocked` freeze。
6. **独立审查 record（ready freeze 机械门）**：独立审查角色（`independent_reviewer_role`，与 operator 不同）对该 ready freeze 写出仓外 review record（`record_kind: e3-ready-freeze-review`、`is_evidence: false`、`schema_version: 1`、verdict `pass`、`blockers: 0`、`majors: 0`、`reviewer_role` 匹配、exact candidate pair/code_sha/runner_sha256/`confirmation_contract_sha256` 一致（稳定投影，与步骤 4 的 confirmation record 绑定同一 confirmation contract）、`machine_confirmation_sha256` 等于步骤 4 record 的 SHA-256、started/ended ≤ 最终 ready freeze 的 `preflight_inputs_frozen_at`）＋ `.sha256` companion。
7. **最终 `ready` freeze（同 confirmation contract，绑定 review record）**：新仓外对象，confirmation contract 与步骤 5 相同字节，绑定 `independent_review_record.status=pass` + `reviewer_role` + `record_path`/`record_sha256`（步骤 6 的 record）。
8. **候选 ID 消费审计（第二次）**：在冻结 HEAD 上重跑步骤 2 的审计并重新记录 hash（见「候选 ID 消费审计」节）。
9. **selftest（host-only，`HDC_PROCESSES=0`）**：

    ```powershell
    pwsh -NoProfile -File .\tests\e3-phys-preflight-runner-selftest.ps1
    ```

10. **DryRun（host-only，非证据；对同一份最终 `ready` freeze）**：

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
11. **审查 DryRun 且 freeze 字节不变**：独立审查角色核对 DryRun 结果与 ready freeze 契约（0 blocker/0 major）后确认 **ready freeze 文件字节与步骤 7 完全一致**（重新计算 freeze SHA-256，不得改动任何字段，包括 `plan_status`）；此时方视为 Live 输入就绪。
12. **单次 Live（唯一一次；只接受步骤 7 的同一份 `plan_status: ready` freeze）**：

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
- **当前 AUTH 固定 `attempt=initial`（retry N/A）**：`-TargetBindingConfirm` 与消费本 AUTH confirmation 的 `ready` Live/DryRun 均被 runner 强制 exact candidate pair + `attempt=initial` + `retry.basis`/`infrastructure_reason=N/A`——因此 **现有 generic retry 分支（`attempt: infrastructure-blocked-retry-1`）不进入本次路径**。任何后续重试必须 **新治理**：先取得新的路线决策并重新登记新授权；当前 AUTH `AUTH-E3-PHYS1API26-20260810-0001` 不可用于任何 retry。
- runner 既有 generic retry 规则仍保留给未来新治理：唯一允许的重试为 `attempt: infrastructure-blocked-retry-1`，`retry.infrastructure_reason` 必须命中白名单 `hdc-usb-interruption` / `collection-storage-failure` / `runner-host-failure`，且 `prior_record` 必须是冻结匹配的 **Live blocked evidence record**（`is_evidence: true`、`record_status: blocked`、`overall: blocked`、`verdict: blocked`、同 campaign/attempt/code/runner/artifact/freeze contract）。prior 为 DryRun、非证据、无 seal 或 `is_evidence: false` 时不构成 retry 依据；`-TargetBindingConfirm` 的 blocked confirmation record（`is_evidence: false`）同样 **不** 构成 retry 依据。
- 本授权不豁免任何既有纪律：build drift、operator-aborted、功能 fail、scenario invalid、integrity violation、非基础设施 blocked 均不授权 retry；`consumed-blocked` / `superseded-unexecuted` / `INVALID-TIMELINE` 历史 ID 一律不得复用。
- 任何继续或重试必须先取得新的路线决策并重新登记。

## 门状态

- E3 未关闭；本记录是授权登记，不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`；本授权不是 E8 `OPEN`，也不改变 E1/E4-E7 聚合状态。本实现不扩展授权边界：`-TargetBindingConfirm` 只复用现行 runner 白名单 `Version`/`TupleModel`/`TupleBuild` 三条 target-binding 命令，不新增任何 discovery/UDID/serial/`hidumper`/cleanup/privileged 能力。
- `plan_status: authorized-awaiting-windows-ready-freeze`：Windows 顺序门（同步 trusted refs/bundle → ID 审计① → blocked confirmation freeze 静态审查 → `-TargetBindingConfirm` → ready draft 绑定 record → 独立审查 record → 最终 ready freeze 绑定 review → ID 审计② → selftest → 同一 ready freeze DryRun → 审查 DryRun 且 freeze 字节不变 → Live）完成前不可执行。
- 历史记录全部保留不改写：API23 initial（`EV-E3-PHYS1API23-20260806-0001`）、rebind（`EV-E3-PHYS1REBIND7-20260806-0001`）、build 确认（`EV-E3-PHYS1BUILD7-20260806-0001`）、API26 0001（`EV-E3-PHYS1API26-20260807-0001`，`consumed-blocked`）、API26 0002（`EV-E3-PHYS1API26-20260807-0002`，`consumed-blocked`）、host remediation（`EV-E3-PHYS1HOST-20260808-0001`）、`ADJ-20260808-0001/0002/0003` 登记。
