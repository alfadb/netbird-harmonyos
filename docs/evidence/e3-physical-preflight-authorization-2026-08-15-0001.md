# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-15 · 0001）

最后核验：2026-08-15

本文登记用户（直接人类决策者）于 2026-08-15 上午显式批准的 **新** `E3-PHYS-PREFLIGHT` 物理设备执行授权（`AUTH-E3-PHYS1API26-20260815-0001`），**取代** [`AUTH-E3-PHYS1API26-20260814-0002`](e3-physical-preflight-authorization-2026-08-14-0002.md) 的未完成 Live 执行：该 AUTH 的 Live（2026-08-15 09:09–09:27 +08:00）S1–S4 全 pass、S5 step1 pass，但 **S5 step3 operator-timeout**（「打开测试 App A 的应用信息页」超时——应用管理里两个 APP 显示名相同，操作员无法区分 A）→ `SCENARIO_INVALID` → seal 为 **invalid** 证据（`overall=invalid`、S5 invalid、S6/S7 not-run-due-to-invalid、cleanup `verified-clean`、`is_evidence=true`、seal 完整、exit 2）。该 sealed invalid live 证据占用 0002 候选 pair `E3-PHYS-PREFLIGHT-20260814-0002` / `EV-E3-PHYS1API26-20260814-0002`——**旧 pair 因此已消费，不得复用**。0002 的历史登记记录 **正文保留、不改写**；本授权 **新建** 候选 pair `E3-PHYS-PREFLIGHT-20260815-0001` / `EV-E3-PHYS1API26-20260815-0001`（attempt=initial、retry N/A）。本文是当前唯一生效的授权登记。

> **状态注记**：本文已获用户最终批准（2026-08-15 上午，聊天确认，用户选择方案 1），`authorization_status: granted`。批准前本登记不构成可执行授权；批准后本登记是当前唯一生效的授权登记。

用户本次显式批准的完整事实（直接人类决策）：

1. **新授权（因 0002 Live S5 step3 operator-timeout 真因定位为应用显示名相同）**：2026-08-15 上午用户批准（"1"，选择方案 1）：**A/B HAP 加区分显示名 + Linux 重建重签 + 重新冻结 + 新 pair 重跑**；授权新 AUTH `AUTH-E3-PHYS1API26-20260815-0001` 与 **新建** 候选 pair `E3-PHYS-PREFLIGHT-20260815-0001` / `EV-E3-PHYS1API26-20260815-0001`；`attempt=initial`、retry N/A。
2. **0002 Live 事实定性（证据分析结论，非设备/执行故障）**：0002 AUTH 的 Live 于 2026-08-15 09:09:14–09:27:39 +08:00 执行：**S1 pass**（machine-cleanup-baseline-and-install）、**S2 pass**（machine-verified-Allow-onCreate-create-fd——**marker 检测修复验证成功，API26 首次**：`:vpn` 子进程行以截断 tag `.alfadb.netbird.e3physvpna:vpn` / `requestId=missing` 形态被 `test_line_correlated` 正确关联，观测到 `VPN_ONCREATE` / `VPN_CONNECTION_CREATED` / `VPN_CREATE_BEGIN` / `VPN_FD_SNAPSHOT`（`fd=32`）/ `VPN_CREATE_RESOLVED`（`accepted=true`））、**S3 pass**（strict-process-boundary-terminal）、**S4 pass**（deny-layout-and-full-window-without-B-create——**"取消"按钮被 cancel|deny 布局验证接受**：授权对话框 layout checkpoint `matched` 含 `allow-control` + `cancel-control`，`deterministic-layout-match`）。**S5 step1 pass**（unique-UI_START）、**S5 step3 operator-timeout**（「打开测试 App A 的应用信息页」在 5 分钟窗口内无操作完成，09:22:36 进入等待、09:27:36 判 timeout）→ `SCENARIO_INVALID`（scenario=5、step_index=3、reason=step-3 operator-timeout）→ S6/S7 `not-run-due-to-invalid`；exit 2；cleanup `verified-clean`（A/B 卸载无残留、staging 无残留、force-stop 仅作残留清理未用作 revoke）；`integrity_violations: []`。**根因：应用管理页两个 APP 显示名相同（均为 "E3 Physical VPN Preflight"），操作员无法区分 A**——不是设备故障、不是 runner 缺陷、不构成任何 retry 依据；新授权按 initial 全新开始。
3. **旧 pair 已消费、不得复用**：`E3-PHYS-PREFLIGHT-20260814-0002` / `EV-E3-PHYS1API26-20260814-0002` 被 0002 的 sealed invalid live 证据占用（证据根 `$HOME/harmonyos-signing/netbird-e3/evidence-live-20260814-0002`，`is_evidence=true`、`execution_mode=live`、`overall=invalid`、cleanup `verified-clean`、seal 完整），**已消费**；该证据 **保留不改写**、不作为 retry 依据，其正式 evidence 登记（reviewed 审查）由 campaign 后独立流程处理，不在本登记内。本授权 **新建** pair，不沿用、不复用任何旧 ID（含 0002 pair、0001 pair、20260813 pair、20260810 pair）。
4. **本次变更清单（写入本文，见「本次变更清单」节）**：**a)** 显示名区分——`AppScope/resources/base/element/string.json` 新增 `app_name_a="E3 Preflight A"` / `app_name_b="E3 Preflight B"`；`build-profile.json5` 的 `products.default.label='$string:app_name_a'`、`products.vpnB.label='$string:app_name_b'`。**b)** audit 脚本 3 处既有漂移修复（**非本次改动引起**）：raw pin `608d33f9…`→`b584bad2…`（跟进 `4d4caef` N0 0002 sealed evidence 合法新增）、`LEDGER_MAX_FUTURE_MS` count 1→2 与 `LEDGER_MAX_AGE_MS` count 1→2（跟进 `c6acae7` ledger 加固使用点）。**c)** 新 signed HAP（**Linux 构建 + 签名**，与 Windows 旧包差异 ~27.7KB，audit 全过）：A 路径 `$HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/a/e3-phys-preflight-a-signed.hap`、sha256 `828fefed…`（完整值见 `repo_bytes`）、size `133935`；B 路径 `…/b/e3-phys-preflight-b-signed.hap`、sha256 `c9e064ed…`、size `133945`。验签 4 项 exit 0、A/B 归属互不串扰、`device_id_count:1`。**d)** 旧 HAP（`f44be17-final`，A `3a98ad68…`/`106210`、B `1adfa966…`/`106212`）**保留作为历史输入**，不再用于本 AUTH。
5. **冻结值复用与常量迁移**：HDC `03123a78…` / `3.2.0d`；目标元组 `PLA-AL10` / `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` / API `26` / `aarch64` / `arm64-v8a` / 别名 `PHYS-1`；freeze 决策字段与 0002 invalid freeze 一致。**Python 三文件治理常量已迁移（20260814-0002 → 20260815-0001：AUTH_ID / 候选 pair / 正向 fixture / authorization_id 等；OLD/负向 mutant 保留历史 ID 语义）并重新冻结**（见 `repo_bytes`；0002 历史字节随 0002 登记保留）。
6. **修复措施沿用（防再犯，写入本文）**：Live 执行环境设置 `PYTHONUNBUFFERED=1`（禁止 stdout 块缓冲掩盖 operator 提示）；主会话监控改用 **证据目录轮询**（`operator-wait-state.json` / `scenario-results.json` 的 `updated_at` 与产物增量）而非 pane 输出空白判断；operator 提示经 `capture-pane` 转发给用户。三项措施在执行本 AUTH 顺序门时强制生效。
7. **设备连接纪律沿用**：执行 host 仍为当前 Linux Pod；沿用「恰一次 `hdc tconn` + 恰一次内存级 `hdc list targets`」host-prep 窄例外；endpoint/token 仍 **不落盘、不写入任何记录/freeze/evidence**（详见「设备连接纪律」节）。
8. **治理语义沿用 0002 授权**：13 步顺序门结构原样沿用（见 `linux_gate_order`）；E8 保持 `CLOSED`；Live 前提交/推送授权沿用（runner + 治理变更审查后提交，campaign evidence 不提前提交）；invalid 后停止不重试。
9. **reviewer_role 冻结**：本 AUTH 的独立审查角色（`independent_reviewer_role`）固定为 `isolated-anthropic-claude-opus-5-reviewer`（与 operator 不同角色，供 ready freeze review record 绑定）。
10. **code_sha：pending-final-commit**：以本登记文档与必要治理变更 **commit 后的最终 HEAD** 为准；本任务不 commit，`code_sha` 在 commit 时冻结（见「仓内固定字节与 clean-commit requirement」节）。

本文是 **授权登记**，**不是** live campaign 证据、**不是** 设备实测记录。本登记（当前 Linux Pod，host-only 只读复测）期间 **不** 运行任何 HDC/设备命令、**不** 安装工具、**不** 生成 ready freeze。**用户就绪声明不替代机器 fresh confirmation**。E3 未关闭，E8 保持 `CLOSED`。

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260815-0001
supersedes: AUTH-E3-PHYS1API26-20260814-0002 # its Live (2026-08-15 09:09-09:27 +08:00) ran S1-S4 pass and S5 step1 pass, then S5 step3 operator-timeout (open App A's app-info page; the two apps shared the same display name "E3 Physical VPN Preflight", operator could not distinguish A) -> SCENARIO_INVALID -> sealed invalid: overall=invalid, S5 invalid, S6/S7 not-run-due-to-invalid, cleanup verified-clean, is_evidence=true, seal complete at $HOME/harmonyos-signing/netbird-e3/evidence-live-20260814-0002; the 0002 record is preserved unchanged, not rewritten; the 0002 candidate pair E3-PHYS-PREFLIGHT-20260814-0002 / EV-E3-PHYS1API26-20260814-0002 is CONSUMED by that sealed invalid live evidence and must never be reused
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: collected
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: not-run-authorization-registration
is_evidence: false
authorization_status: granted # user final approval 2026-08-15 morning via chat confirmation (user chose 方案 1); see 用户批准记录 section
device_readiness: user-attested-ready # user readiness attestation does NOT replace machine fresh confirmation
machine_fresh_confirmation: pending # to be completed by the Linux Pod via -TargetBindingConfirm; this AUTH fixes attempt=initial and the NEW candidate pair (retry N/A)
plan_status_at_registration: authorized-awaiting-linux-ready-freeze
campaign_status: candidate-new-created-not-live-pending-audits # NEW pair created by this AUTH; never Live, never consumed; audit-1/audit-2 both pending (hash-recorded out-of-repo)
independent_review_record: pending # ready Live / ready DryRun require a pass review record (record_kind=e3-ready-freeze-review, is_evidence=false) with an out-of-repo JSON + .sha256 companion
reviewer_role: isolated-anthropic-claude-opus-5-reviewer # frozen independent reviewer role for this AUTH's ready-freeze review record (distinct from operator)
code_sha: pending-final-commit # frozen at commit time to the final HEAD of this registration document + necessary governance changes; this task does not commit
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
hdc_target_reference: controlled external mapping; repository alias PHYS-1 only # single user-local hdc tconn + single memory-only hdc list targets host-prep exception (see below); endpoint/token never persist and never enter any record/freeze/evidence
host_prep_target_mapping:
  tconn: "hdc tconn <dynamic-ip:port>" # ONE execution only, user-local in the Linux terminal; endpoint value NEVER persisted/recorded; value changes => STOP and report, no reconnect, no repeat tconn
  list_targets: "hdc list targets" # ONE execution only per mapping, memory-only; output token read in memory only; never written to file/log/repo, never echoed
  scope: memory-only # output token never persisted; endpoint/token never in any record/freeze/evidence/transcript
  acceptance: exactly one target token # 0 / multiple / malformed => STOP, no guessing, no selection
  persistence: "PHYS_1_TARGET" set as process-scope environment variable only
  privacy_boundary: no endpoint/token persistence, no endpoint/token in any record/freeze/evidence; runner HDC whitelist is NOT expanded by this exception (tconn/list targets are host-prep operator steps, not runner commands)
  consumed_authorization: false # NOT yet consumed: to be consumed exactly once (future) by the operator at the mapping gate on the Linux Pod; this registration does not run it
repo_bytes: # governance-constant migration re-freeze: AUTH ID + candidate pair constants migrated in the Python trio (0002 frozen bytes 33a3b454…/3a32e171…/3d5f0194… remain recorded under the 0002 AUTH, not applicable here); HAP inputs NEW (20260815-relabeled, Linux build+re-sign); hdc/target-tuple/decision-field values reused from the 0002 AUTH
  python_port: frozen # Python 3 semantic-equivalent port of the 0002 AUTH (marker-detection fix ADJ-20260814-0002 C6 carried forward unchanged), three-file bytes re-frozen after the constant migration; the PowerShell trio remains in-repo unchanged as historical input
    runner_sha256: 9572d55afefc7455a7dc9ae2fb8c8fe56f708ec1d862173f1120996ec4d16352 # e3-phys-preflight-campaign.py (AUTH_ID/candidate pair constants migrated; settings-app-info app-label regex aligned to new display names, BLOCKER-1 fix; re-frozen)
    freeze_example_sha256: 030afd0e7adbbd4ac50f831bca412eac21c710e7cc625836f2ef8acec04b8e64 # e3-phys-preflight-freeze.example.json (authorization_id/pair updated to this AUTH)
    selftest_sha256: c0d566f366922ba51855d840930dcf26e42d6075378cf5e87915aba767de3e7f # tests/e3-phys-preflight-runner-selftest.py (positive fixtures + label fixtures + label negative cases; OLD_AUTH_ID negative mutants kept)
  signed_hap_frozen: # NEW Linux-built + re-signed HAPs (relabeled display names); path $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/{a,b}/; the old f44be17-final packages (A 3a98ad68…/106210, B 1adfa966…/106212) are RETAINED as historical inputs and no longer used for this AUTH
    hap_a_sha256: 828fefedba3b4ec63cb3fbe6336f4bf34266770cb29465c854f0ec94491b503d # size 133935, e3-phys-preflight-a-signed.hap
    hap_b_sha256: c9e064ed1dcf0df112c27c5f6bcc282e9a9913c63d3dfd8ce8377b1b46218a37 # size 133945, e3-phys-preflight-b-signed.hap
  hdc: sha256 03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81, version 3.2.0d # $HARMONYOS_STABLE_HDC (source "$HOME/harmonyos/env.sh")
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260815-0001 # NEW pair created by this AUTH
  evidence_id: EV-E3-PHYS1API26-20260815-0001
  attempt: initial # this AUTH fixes attempt=initial with retry.basis/infrastructure_reason=N/A; the generic infrastructure retry branch never applies to this AUTH path; the 0002 S5-step3-timeout Live is NOT a retry basis (same display name for both apps, not infrastructure failure)
  identity_status: pending-two-consumption-audits # NEW IDs: audit-1 (after trusted refs/bundle sync, before any TargetBindingConfirm) and audit-2 (after the final ready freeze, before selftest/DryRun/Live); both hash-recorded out-of-repo
  live: false
  consumed: false
  note: NEW candidate identity created by this authorization. The PRIOR pair E3-PHYS-PREFLIGHT-20260814-0002 / EV-E3-PHYS1API26-20260814-0002 (carried by AUTH-E3-PHYS1API26-20260814-0002) is CONSUMED: the 2026-08-15 Live was sealed invalid by the runner at $HOME/harmonyos-signing/netbird-e3/evidence-live-20260814-0002 (scenario-results.json record_status=invalidated / overall=invalid / verdict=invalid, execution_mode=live, is_evidence=true, cleanup verified-clean; seal complete; exit 2) and must never be reused. That sealed invalid live evidence is PRESERVED UNCHANGED, is NOT a retry basis (S5 step3 operator-timeout: the two apps shared the same display name "E3 Physical VPN Preflight" so the operator could not distinguish A in the app-management page — now fixed by relabeled display names), and its formal evidence registration (reviewed) is handled by a separate post-campaign flow, not in this registration. new-candidate-id-consumption-audit-1 (PENDING): fixed out-of-repo log $HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260815-0001/audit/new-pair-id-consumption-audit-1.txt (+ .sha256). new-candidate-id-consumption-audit-2 (PENDING): fixed out-of-repo log $HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260815-0001/audit/new-pair-id-consumption-audit-2.txt (+ .sha256). ANY occupying live/evidence/seal record for the new pair => STOP and await user-authorized new IDs, never silently switch or rewrite records
linux_gate_order:
  - sync-trusted-refs-bundle # git fetch + checkout frozen HEAD, clean worktree check before/after signing/build/freeze/selftest/DryRun/Live
  - candidate-id-consumption-audit-1 # NEW pair E3-PHYS-PREFLIGHT-20260815-0001 / EV-E3-PHYS1API26-20260815-0001 must be unoccupied; hash-recorded; does NOT repeat the consumed prior-pair audit; fixed log: $HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260815-0001/audit/new-pair-id-consumption-audit-1.txt (+ .sha256)
  - blocked-confirmation-freeze-static-review # independent_review_ready=true = contract/roles statically ready, NOT device/execution readiness; no review record required on the blocked confirmation freeze
  - host-prep-device-connect-mapping # user-local hdc tconn exactly once + hdc list targets exactly once per mapping (memory-only, exactly one token, process-scope PHYS_1_TARGET); runner HDC whitelist unchanged
  - target-binding-confirm # machine fresh confirmation: 3 whitelisted probes (Version/TupleModel/TupleBuild), out-of-repo double-file record (JSON + .sha256 companion), never enters campaign roots
  - ready-freeze-draft-binds-record-hash # new out-of-repo ready freeze draft binds machine_fresh_confirmation.status=pass + authorization_id + record_path + record_sha256 (same confirmation contract as the blocked confirmation freeze)
  - independent-review-record # isolated-anthropic-claude-opus-5-reviewer writes an out-of-repo e3-ready-freeze-review record + .sha256 companion over the same freeze contract, binding machine_confirmation_sha256
  - final-ready-freeze-binds-review-record # final out-of-repo ready freeze (same confirmation contract bytes) binds independent_review_record.status=pass + reviewer_role + record_path + record_sha256
  - candidate-id-consumption-audit-2 # second external audit on the frozen HEAD before selftest/DryRun/Live; hash-recorded; fixed log: $HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260815-0001/audit/new-pair-id-consumption-audit-2.txt (+ .sha256)
  - selftest-hdc0
  - dryrun-same-ready-freeze
  - review-dryrun-freeze-bytes-unchanged
  - single-live
dry_run_vs_live_freeze: DryRun accepts plan_status blocked or ready (always emits non-evidence blocked); Live requires plan_status ready only
retry_policy: no-auto-retry; this AUTH AUTH-E3-PHYS1API26-20260815-0001 fixes attempt=initial and retry.basis/infrastructure_reason=N/A for TargetBindingConfirm and for every ready Live/DryRun consuming its confirmation; the 0002 S5-step3-timeout Live (sealed invalid, is_evidence=true) is NOT a retry prior — its cause was the two apps sharing one display name (operator could not distinguish A in the app-management page; now fixed by relabeled display names), not an infrastructure allowlist reason, and the runner-enforced generic infrastructure retry branch (attempt=infrastructure-blocked-retry-1, allowlist hdc-usb-interruption|collection-storage-failure|runner-host-failure, frozen matching Live blocked evidence prior) can never enter this AUTH path; ANY gate failure stops the campaign with no retry and no ID switch; any later attempt requires new governance and a new authorization
live_gate_commit_authorization: user authorized committing/pushing the runner + governance changes (the Python trio + this registration) after review and BEFORE Live; campaign evidence (selftest/DryRun/Live outputs) still never commits ahead of Live; this registration task itself must not commit/push
verdict: N/A - authorization registration; not a live campaign verdict
scope_statement: whitelist-only per current runner plus the single user-local hdc tconn + single memory-only hdc list targets host-prep exception; no extra discovery/UDID/serial/hidumper/root/privileged/Go/NetBird/product; user readiness attestation does not replace machine fresh confirmation; E3 open, E8 CLOSED
reviewer: pending-independent-review
reviewed_at: pending
```

## 授权摘要

| 字段 | 值 |
| --- | --- |
| AUTH ID | `AUTH-E3-PHYS1API26-20260815-0001`（取代 0002 AUTH 的未完成 Live 执行；0002 历史登记保留不改写） |
| 授权者 | 用户（直接人类决策者），2026-08-15 上午（已批准；批准方式：聊天确认，用户选择方案 1） |
| authorization_status | `granted`（2026-08-15 上午用户聊天确认批准） |
| 事故（授权起因） | 0002 AUTH 的 Live S5 step3 operator-timeout（「打开测试 App A 的应用信息页」超时）→ `SCENARIO_INVALID` → seal invalid；**根因：应用管理页两个 APP 显示名相同（"E3 Physical VPN Preflight"），操作员无法区分 A**——不是设备/runner 故障 |
| device_readiness | `user-attested-ready`（用户就绪声明，**不** 替代机器 fresh confirmation） |
| machine_fresh_confirmation | `pending`（由 Linux Pod 通过 `-TargetBindingConfirm` 观测） |
| plan_status | `authorized-awaiting-linux-ready-freeze` |
| 执行 host | Linux Pod：`host-dev-alfadb-full` / `Debian GNU/Linux 13 (trixie)` / `x86_64` / Python `3.13.5` |
| runner | Python 3 版（**常量迁移后重新冻结**）：runner `9572d55a…`、freeze example `030afd0e…`、selftest `c0d566f3…`；0002 AUTH 历史字节（runner `33a3b454…`、selftest `3a32e171…`、freeze example `3d5f0194…`）随 0002 登记保留，不再适用本 AUTH |
| 目标元组（不变，冻结于 `ADJ-20260806-0003`） | `PLA-AL10` / `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` / API `26` / `aarch64` / `arm64-v8a` / 设备别名 `PHYS-1` |
| candidate（**新建**） | `E3-PHYS-PREFLIGHT-20260815-0001` / `EV-E3-PHYS1API26-20260815-0001`，未 Live、未消耗；`attempt=initial`、retry N/A；执行前须两次 ID 消费审计（audit-1/audit-2，均 hash 记录） |
| signed A/B HAP（**新建** `20260815-relabeled`，Linux 构建+重签，显示名已区分） | `$HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/{a,b}/`：A `828fefedba3b4ec63cb3fbe6336f4bf34266770cb29465c854f0ec94491b503d` size `133935`（`e3-phys-preflight-a-signed.hap`）；B `c9e064ed1dcf0df112c27c5f6bcc282e9a9913c63d3dfd8ce8377b1b46218a37` size `133945`（`e3-phys-preflight-b-signed.hap`）；与 Windows 旧包差异 ~27.7KB，audit 全过，验签 4 项 exit 0、A/B 归属互不串扰、`device_id_count:1` |
| HDC（复用） | `$HARMONYOS_STABLE_HDC` 文件 SHA-256 `03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81`，version `3.2.0d` |
| 旧 HAP 处置 | `f44be17-final`（A `3a98ad68…`/`106210`、B `1adfa966…`/`106212`）**保留作为历史输入**，不再用于本 AUTH |
| 旧 pair（0002）处置 | `E3-PHYS-PREFLIGHT-20260814-0002` / `EV-E3-PHYS1API26-20260814-0002` **已消费**：0002 Live 的 sealed invalid 证据（`$HOME/harmonyos-signing/netbird-e3/evidence-live-20260814-0002`，`record_status=invalidated`/`overall=invalid`/`verdict=invalid`、`execution_mode=live`、`is_evidence=true`、cleanup `verified-clean`、seal 完整、exit 2）；**保留不改写、不作为 retry 依据**；正式 evidence 登记（reviewed）由 campaign 后独立流程处理；**不得复用**（0001 pair、20260813 pair、20260810 pair 同样已消费，不得复用） |
| 允许范围 | 现行 runner 白名单（HDC target-binding `Version`/`TupleModel`/`TupleBuild`、A/B install/start/observation/mechanical prompts/final cleanup）+ **一次用户本地 `hdc tconn` + 一次内存级 `hdc list targets` host-prep 窄例外**（endpoint 不落盘） |
| 禁止 | 任何额外 discovery、UDID、serial、`hidumper`、root、privileged、Go、NetBird、product；`hdc tconn` 与 `hdc list targets` 各只允许一次且 endpoint/token 不落盘、不写入任何记录/freeze/evidence |
| 修复措施 | `PYTHONUNBUFFERED=1`；主会话改证据目录轮询；operator 提示经 `capture-pane` 转发 |
| 本次变更 | **a)** 显示名区分：`app_name_a="E3 Preflight A"` / `app_name_b="E3 Preflight B"`（string.json + build-profile.json5 label 绑定）；**b)** audit 脚本 3 处既有漂移修复（raw pin `b584bad2…` 跟进 `4d4caef`；`LEDGER_MAX_FUTURE_MS`/`LEDGER_MAX_AGE_MS` count 1→2 跟进 `c6acae7`）；**c)** 新 Linux 构建+重签 HAP（见上）；**d)** 旧 HAP 保留为历史输入 |
| reviewer_role | `isolated-anthropic-claude-opus-5-reviewer`（ready freeze review record 绑定） |
| code_sha | `pending-final-commit`（本登记 + 必要治理变更 commit 后冻结为最终 HEAD） |
| E8 | `CLOSED`（本授权不是 E8 `OPEN`） |

## Linux Pod host 事实（本登记时点实测）

执行 host 为当前 Linux Pod（与 0002 AUTH 相同，无迁移）。本登记时点 host-only 只读复测事实：

| 项 | 值 |
| --- | --- |
| hostname | `host-dev-alfadb-full` |
| 架构 | `uname -m` → `x86_64` |
| 发行版 | `/etc/os-release` `PRETTY_NAME` → `Debian GNU/Linux 13 (trixie)` |
| Python | `python3 --version` → `Python 3.13.5` |
| Linux Stable HDC | `$HARMONYOS_STABLE_HDC`（`source "$HOME/harmonyos/env.sh"` 后）→ `/home/worker/harmonyos/command-line-tools/current/sdk/default/openharmony/toolchains/hdc`；文件 SHA-256 `03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81`；`hdc version` → `Ver: 3.2.0d`（仅定位 + 文件 hash + version 输出，**未执行** 设备命令） |
| 签名材料受控目录 | 仓外 `$HOME/harmonyos-signing`（含 `netbird-e3/`）；签名材料只存该受控目录，绝不入仓 |
| 仓外 freeze | `freeze/20260815-relabeled/` 存在新建冻结 artifacts（A/B HAP hash 已复核一致，见授权摘要）；**20260815-0001 全新 blocked / ready freeze 尚未生成**（必须同步新 pair + 新 AUTH ID 字段，不得复用 0002 冻结文件）；旧 `freeze/f44be17-final/` 保留作为历史输入 |
| 旧 live 证据根 | `$HOME/harmonyos-signing/netbird-e3/evidence-live-20260814-0002` 为 **sealed invalid 证据**（0002 AUTH 的 Live；`is_evidence=true`、`execution_mode=live`、`overall=invalid`、cleanup `verified-clean`、seal 完整、exit 2）；**保留不改写、不作为 retry 依据**；本授权新 Live 使用 **新证据根**（`evidence-live-20260815-0001`，见顺序门模板），不得与旧证据根混用 |
| 受控 EvidenceRoot / RawRoot | 新 pair 的 EvidenceRoot / RawRoot **尚未初始化**（按顺序门由 runner 在对应模式初始化，本登记不初始化；新 Live/DryRun 根见命令模板） |
| 仓外 `PHYS_1_TARGET` 映射 | **未建立**（当前 shell 未设置；须 host-prep 恰一次用户本地 `hdc tconn` + 恰一次内存级 `hdc list targets` 建立 process-scope 映射） |
| 新 pair 消费审计 | audit-1 / audit-2 **均未执行**（固定仓外日志路径见「两次新 ID 消费审计」节：`$HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260815-0001/audit/new-pair-id-consumption-audit-{1,2}.txt` + `.sha256`） |
| 0002 旧 pair 消费 | **已消费**：sealed invalid live 证据占用（见授权摘要「旧 pair 处置」行），**不得复用**；其消费事实由本登记直接记录（0002 顺序门 audit-1/audit-2 已执行，Live seal 完整） |
| HDC 进程数 | 1（host 上存在 hdc server 进程 `hdc -m -s ::ffff:127.0.0.1:8710`，自 2026-08-14 起运行，由 hdc 客户端自管理的正常 host 行为；本授权 **不** 附带任何额外 cleanup/stop-server/retry 授权） |
| 设备命令 | 未发出（本登记期间禁止） |
| 仓库 HEAD | 工作区基于 `main`（HEAD `ddba6e7`，待本登记 commit 后以最终 HEAD 冻结 `code_sha`；本任务禁止提交/推送；当前工作区含六文件未提交修改：runner / selftest / freeze example（本 AUTH 常量迁移字节）+ string.json / build-profile.json5 / audit-physical-preflight.sh（本次变更清单 a/b，先前已改）） |
| host 状态 | `ready_for_commit_and_ordered_preflight`——工具与签名输入齐备，可审查后提交并按顺序门执行 preflight；**非全部 Live-ready**（未提交 clean final commit、新 pair audit-1、全新 blocked/ready freeze、confirmation/review records、新 EvidenceRoot/RawRoot、process-scope `PHYS_1_TARGET` 映射均尚待） |

因此本 Linux Pod **具备提交与按序 preflight 条件**（`ready_for_commit_and_ordered_preflight`），ready freeze 的生成、selftest、DryRun 与 Live 均须在本 host 按本登记顺序门完成；**不得** 表述为全部 Live-ready。

## 0002 Live 事实（本次新执行的 Live；证据分析结论）

**事故事实（如实记录）**：0002 AUTH（`AUTH-E3-PHYS1API26-20260814-0002`）的顺序门 1–13 步全部完成，Live 于 2026-08-15 09:09:14–09:27:39 +08:00 执行。**S1 pass**（machine-cleanup-baseline-and-install）。**S2 pass**（machine-verified-Allow-onCreate-create-fd）——**marker 检测修复验证成功（API26 首次）**：`:vpn` Extension 子进程行以截断 tag `.alfadb.netbird.e3physvpna:vpn` / `requestId=missing` 形态被 `test_line_correlated` 正确关联，观测到完整 create 终端链 `VPN_ONCREATE` / `VPN_CONNECTION_CREATED` / `VPN_CREATE_BEGIN` / `VPN_FD_SNAPSHOT`（`fd=32`）/ `VPN_CREATE_RESOLVED`（`accepted=true`）。**S3 pass**（strict-process-boundary-terminal）。**S4 pass**（deny-layout-and-full-window-without-B-create）——**"取消"按钮被 cancel|deny 布局验证接受**（授权对话框 layout checkpoint `deterministic-layout-match`，`matched` 含 `allow-control` + `cancel-control`）。**S5 step1 pass**（unique-UI_START）。**S5 step3 operator-timeout**：「打开测试 App A 的应用信息页」09:22:36 进入等待、5 分钟窗口内操作未完成，09:27:36 判 `step-3 operator-timeout` → `SCENARIO_INVALID`（scenario=5、step_index=3、reason=step-3 operator-timeout）；S6/S7 `not-run-due-to-invalid`；exit 2；cleanup `verified-clean`（A/B 卸载无残留、staging 无残留、force-stop 仅作残留清理未用作 revoke）；`integrity_violations: []`。runner 按失败路径将本次 Live seal 为 invalid 证据（`overall=invalid`、7 场景 invalid/not-run、cleanup `verified-clean`、`is_evidence=true`、seal 完整）。证据根 `$HOME/harmonyos-signing/netbird-e3/evidence-live-20260814-0002`。

**证据分析结论（定性）**：**设备侧无故障、runner 无缺陷**。S5 step3 超时根因为 **应用管理页两个 APP 显示名相同**（均为 "E3 Physical VPN Preflight"），操作员无法区分 A 与 B，无法完成「打开测试 App A 的应用信息页」这一机械步骤。S1–S4 全链路（安装、Allow→create→fd、Stop 严格进程边界、Deny 取消布局）均机器验证通过。

**定性**：非设备问题、非执行失败、非基础设施故障——纯属 **应用显示名不区分 A/B** 导致的操作员机械步骤不可完成。该 invalid **不构成任何 retry 依据**（retry 白名单 `hdc-usb-interruption`/`collection-storage-failure`/`runner-host-failure` 均不命中；且 generic retry 分支不进入本 AUTH 路径），新授权按 `attempt=initial` 全新开始。

## 本次变更清单（本授权强制生效）

1. **显示名区分（修复 0002 S5 step3 根因）**：
   - `AppScope/resources/base/element/string.json` 新增 `app_name_a = "E3 Preflight A"`、`app_name_b = "E3 Preflight B"`（原 `app_name = "E3 Physical VPN Preflight"` 保留不动）。
   - `build-profile.json5`：`products.default.label = '$string:app_name_a'`、`products.vpnB.label = '$string:app_name_b'`。
   - 效果：应用管理/设置页 A/B 以不同显示名呈现，操作员可区分 A（"E3 Preflight A"）。
2. **audit 脚本 3 处既有漂移修复（非本次改动引起，跟进既有提交）**：
   - raw pin `608d33f902b3a3f356a27d3f433aa46613df36d2` → `b584bad2b877698c0daab43a3b634546fb96a220`（跟进 commit `4d4caef` "Record N0 0002 sealed emulator evidence" 的合法新增）。
   - `LEDGER_MAX_FUTURE_MS` count 1→2 与 `LEDGER_MAX_AGE_MS` count 1→2（跟进 commit `c6acae7` 场景7 stop-request 唯一门控与 ledger 加固的 L272 越界检查使用点；L14/L15 定义 + L272 使用 = 2）。
3. **新 signed HAP（Linux 构建 + 签名，显示名已区分）**：
   - A：`$HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/a/e3-phys-preflight-a-signed.hap`，sha256 `828fefedba3b4ec63cb3fbe6336f4bf34266770cb29465c854f0ec94491b503d`，size `133935`。
   - B：`$HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/b/e3-phys-preflight-b-signed.hap`，sha256 `c9e064ed1dcf0df112c27c5f6bcc282e9a9913c63d3dfd8ce8377b1b46218a37`，size `133945`。
   - 与 Windows 旧包（`f44be17-final`）差异约 ~27.7KB；`audit-physical-preflight.sh` 全过；验签 4 项 exit 0、A/B 归属互不串扰、`device_id_count:1`。
4. **旧 HAP 处置**：`f44be17-final`（A `3a98ad68…`/`106210`、B `1adfa966…`/`106212`）**保留作为历史输入**（不删除、不改写），不再用于本 AUTH 的任何 freeze/Live。

## 旧 live 证据处置声明（0002 invalid live）

- `$HOME/harmonyos-signing/netbird-e3/evidence-live-20260814-0002` 是 **sealed invalid 证据**（`is_evidence=true`、`execution_mode=live`、`overall=invalid`、cleanup `verified-clean`、seal 完整、exit 2，含 `campaign-seal.json` / `hash-manifest.json` / `scenario-results.json` / `operator-attestation.json` / `operator-wait-state.json` / `projection/` 等），占用 0002 候选 pair。
- 该证据 **保留不改写**（不编辑、不覆盖、不删除、不重 seal）；**不作为 retry 依据**；本授权新 campaign 不使用该证据根（新 Live 用 `evidence-live-20260815-0001`）。
- 其正式 evidence 登记（reviewed 审查，含 7 场景 invalid/not-run 的审查结论）由 campaign 后 **独立流程** 处理，不在本登记内执行。

## 仓内固定字节与 clean-commit requirement（复用声明）

**Python 移植三文件（runner / freeze example / selftest）——常量迁移后重新冻结**：

- runner：`e3-phys-preflight-campaign.py` → `9572d55afefc7455a7dc9ae2fb8c8fe56f708ec1d862173f1120996ec4d16352`（AUTH_ID 与候选 pair 常量已迁移；**BLOCKER-1 修复**：settings-app-info 布局 profile 的 app-label 正则改为按 expected_bundle 匹配新显示名 `E3 Preflight A`/`E3 Preflight B`（兼容旧形态、负向拒绝错误尾标），S5 step3 提示词改带显示名，模拟 fixture 同步）
- freeze example：`e3-phys-preflight-freeze.example.json` → `030afd0e7adbbd4ac50f831bca412eac21c710e7cc625836f2ef8acec04b8e64`（**刻意保持 `plan_status: blocked`**，仅占位，不含 campaign 哈希/路径/秘密）
- selftest：`tests/e3-phys-preflight-runner-selftest.py` → `c0d566f366922ba51855d840930dcf26e42d6075378cf5e87915aba767de3e7f`（正向 fixture 迁移；label 正/负向用例；旧 AUTH ID 仅作负向 mutant 样本保留历史语义）

**clean-commit requirement**：`code_sha` 以本登记文档与必要治理变更 **commit 后的最终 HEAD** 为准（本任务不 commit；文档中写 `pending-final-commit`，**在 commit 时冻结**）；后续 blocked freeze 绑定该 HEAD。提交时须重新计算最终 commit 中三文件的 SHA-256，复算结果**预期与绑定值一致**；若不一致，说明文件在登记后被改动，必须停止并重新登记，**不得** 绑定任何发散 hash。最终 bundle commit 是权威字节源。

freeze contract 包含 `operator_trust_model` / `scenario_invalid_policy` / `layout_verification_profile` / `vpn_conflict_rejection_codes` / `process_probe_target` 等决策字段——**与 0002 invalid freeze 一致**；旧 freeze（如 20260813 candidate `INVALID-TIMELINE`、历史 runner 绑定、0002 冻结文件）一律被 runner 拒绝，任何模式（DryRun 含）均不可用于新 Live。`ready` freeze 还须携带 `independent_review_record`（`status: pass` + `reviewer_role: isolated-anthropic-claude-opus-5-reviewer` + `record_path`/`record_sha256`）绑定独立审查 record（见下）。

## 设备连接纪律（Linux Pod，host-prep 窄例外；沿用）

本授权沿用 0002 AUTH 的 host-prep 窄例外（执行 host 仍为 Linux Pod）：

- **`hdc tconn <dynamic-ip:port>`**：由 **用户在 Linux 终端本地** 执行 **一次且仅一次**（host-prep 门，`-TargetBindingConfirm` 之前）。endpoint 值 **不落盘、不记录**（不写入任何文件/日志/仓库/记录/freeze/evidence）。IP:port 变化 → **停止并回报主会话**，不自行重连、不重复 tconn。
- **`hdc list targets`**：紧随 tconn 之后执行 **一次且仅一次**。仅 **内存** 取得输出；token **不持久化** 到任何文件/环境持久层/仓库、**不写入** confirmation record / freeze / evidence / transcript。输出必须恰一 target token；0 个、多个或格式异常 → **立即停止**，不猜测、不选择、不重跑。
- **用途**：把该 token 设置为 **process-scope** 环境变量 `PHYS_1_TARGET`（仅当次进程可见），供后续 runner 门（`-TargetBindingConfirm` / selftest / DryRun / Live）作为仓外受控 target 注入。
- **边界**：本例外 **不扩大 runner 的 HDC 白名单**——`tconn`/`list targets` 是操作者的 host-prep 步骤，**不是** runner 命令，runner 自身仍只按白名单 argv 执行 `Version`/`TupleModel`/`TupleBuild` 与 A/B 定向操作；任何把 `tconn`/`list targets` 编入 runner、重跑、把 endpoint/token 落盘或写进任何记录的行为都违反本授权。
- **host HDC server 注记（不新增任何授权）**：执行一次 `hdc tconn`/`hdc list targets` 可能顺带启动 host 上正常的 HDC server 进程（由 hdc 客户端自管理，属正常 host 行为；当前 host 已存在一个自 2026-08-14 起运行的 hdc server 进程）；本授权 **不** 附带任何额外 cleanup/stop-server/retry 授权；输出 0 个或多个 token 时 **立即停止**，不清理、不重试、不猜测。
- 该窄例外由操作者执行一次并消耗；本登记期间不执行。

## 用户就绪声明与机器 fresh confirmation（`-TargetBindingConfirm`）

- `device_readiness: user-attested-ready` 只记录用户声明设备已准备好；它 **不** 替代、**不** 提前满足 `machine_fresh_confirmation`。
- `machine_fresh_confirmation: pending` 必须由 **Linux Pod** 上的 **机器** 观测完成：通过 runner 的 `-TargetBindingConfirm` 模式（HDC 白名单 `Version` + `TupleModel` + `TupleBuild` 共 3 次，逐字比对冻结 `PLA-AL10` / `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`）确认当前绑定设备仍为冻结元组。该模式：接受 `plan_status: blocked` 或 `ready` 的 confirmation freeze（`machine_fresh_confirmation.pending`），但完整校验 freeze 结构/clean repo/`code_sha`/runner/HDC/外部输入 hash/`PHYS_1_TARGET` 单 token，且 **固定候选 pair** `E3-PHYS-PREFLIGHT-20260815-0001` / `EV-E3-PHYS1API26-20260815-0001` 与 `attempt=initial`（`retry.basis`/`infrastructure_reason=N/A`；generic retry 分支不进入本路径）；**不** 初始化 `EvidenceRoot`/`RawRoot`、**不** 设置 `is_evidence`、**不** 消费 campaign/evidence ID、不进入 capture/install/start/最终 campaign cleanup。产出仓外 **双文件** confirmation record（JSON 临时文件 + `.sha256` 临时文件、复算 hash 后 atomic move JSON、最后 atomic move companion 作为 completion marker；`schema_version=1`、`record_kind: target-binding-confirmation`、`is_evidence: false`、`authorization_id: AUTH-E3-PHYS1API26-20260815-0001`、`exception: E3-PHYS-PREFLIGHT`、alias `PHYS-1`、`target_redacted: true`、expected/observed model+build、`command_attempted`/`command_completed`（pass 时均为 3）、`confirmation_contract_sha256`（稳定两阶段投影，非完整 freeze contract）、started/ended、verdict `pass|blocked` + reason；禁止 target/serial/UDID/secret，observed version/model/build 与 reason 均先经敏感文本脱敏）。**退出语义**：pre-record 门失败（record/companion 已存在、路径在仓内、reparse 祖先）→ throw + exit 1 且 **不写任何文件**；probe/tuple 失败或 record 写入失败 → 尽力写 blocked record + companion + exit 2；pass 必须 `attempted=completed=3` 且双文件完成。companion 写失败可遗留 orphan JSON，但 **绝不可消费/绑定**（consumer 只接受双文件且 companion 与 record 字节一致）且 **禁止覆盖**。
- **consumer 完整校验**：`ready` Live 与 `ready` DryRun 强制 `machine_fresh_confirmation.status=pass`，且 record 必须仓外、无 reparse 祖先（record 与其 `.sha256` companion 均检查）、companion 存在且等于 record SHA-256；内容逐项核对 `schema_version=1`、record_kind、`is_evidence=false`、exception、AUTH ID、**exact candidate pair**、`attempt=initial`/retry N/A、`device_alias=PHYS-1`、`target_redacted=true`、verdict `pass`/reason `N/A`、code/runner/hdc SHA + `hdc_version`、`confirmation_contract_sha256` 等于当前 freeze 的 confirmation contract（稳定投影）、expected/observed model/build、`command_attempted=3`/`command_completed=3`、`started_at<=ended_at<=freeze.preflight_inputs_frozen_at`。**`blocked` 的 DryRun**：`status=pending`（或缺省）允许并跳过；若声明 `status=pass` 则同样 **完整校验**（blocked DryRun 不得藏起损坏的绑定）。`independent_review_record` 同规则：`blocked` DryRun 声明 review `status=pass` 时同样 **完整校验**（machine `status=pass` 且 review `status=pass` 时 review record 机械门完整执行），machine confirmation `pending`/缺省 而 review 声明 `pass` → **明确拒绝**（review record 绑定 machine confirmation hash，pending/absent machine 无法锚定），review `pending` 允许。**不引入任何未决策的小时有效期**：fresh 只验证顺序双锚——record `ended_at` 不晚于 **最终 ready freeze** 的 `preflight_inputs_frozen_at`，且 Live 在其自身 preflight 中再次执行 `Version`/`TupleModel`/`TupleBuild` 三条机器核对（fresh double anchor：确认时一次、Live 时一次）。
- 该确认在 ready freeze 生成 **之前** 执行；Live 时 runner precheck 会再次强制核对（漂移即停止，且 Live precheck 失败走正常 campaign 流程：定向 cleanup verification，不跳过）。
- 在机器 fresh confirmation 完成前，任何情况下不得进入 `plan_status: ready`、不得 Live。

## 两次新 ID 消费审计（audit-1 / audit-2）

新候选 identity `E3-PHYS-PREFLIGHT-20260815-0001` / `EV-E3-PHYS1API26-20260815-0001` 必须通过 **两次外部消费审计** 且每次 **hash 记录**：

1. **new-candidate-id-consumption-audit-1（同步 trusted refs/bundle 之后、任何 `-TargetBindingConfirm`/ready freeze 之前；针对新 pair，尚未执行）**：`git fetch`/检出同步后，审计仓库（`git log --all --grep` + evidence 目录与仓外 evidence roots 全文 grep）确认没有任何 `execution_mode=live` / `is_evidence=true` / seal 记录（scenario-results/campaign-seal/hash-manifest 或独立 evidence 文档）占用新 pair `E3-PHYS-PREFLIGHT-20260815-0001` / `EV-E3-PHYS1API26-20260815-0001`；将两次 grep 的输出与时间戳记入仓外审计日志（固定路径 `$HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260815-0001/audit/new-pair-id-consumption-audit-1.txt` + `.sha256`，hash 记录）。**旧 pair（0002）的消费已由本登记直接记录**（0002 顺序门 audit-1/audit-2 已执行，Live 被 runner seal 为 invalid 证据占用旧 pair——见「旧 live 证据处置声明」节；20260815-0001 执行 audit-1 时不重复审计旧 pair 消费）。
2. **audit-2（最终 ready freeze 绑定 review record 之后、selftest/DryRun/Live 之前）**：在冻结 HEAD 上重跑同一审计并重新记录 hash（固定路径 `$HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260815-0001/audit/new-pair-id-consumption-audit-2.txt` + `.sha256`），确认新 pair 仍未消耗。

**若任何一次发现占用记录，候选不得复用**：停止并回报主会话，等用户授权新 campaign/evidence ID；绝不静默改用其他 ID 或改写记录。

## Linux 顺序门与命令模板（cwd 均为 `spikes/e3-vpn-extension-physical-preflight-hap`）

Linux Pod 必须 **按序** 满足下列门，缺一即停止并回报主会话（所有路径均为占位符；不含 target/secret；除 host-prep 一次用户本地 `hdc tconn`、一次 `hdc list targets` 与 `-TargetBindingConfirm` 的 3 条白名单设备命令外均禁 HDC）。命令模板为 Python 3 版（本 AUTH 冻结字节，常量迁移后重新冻结；`$HARMONYOS_STABLE_HDC` 经 `source "$HOME/harmonyos/env.sh"` 取得；**所有 runner 模式执行环境必须设置 `PYTHONUNBUFFERED=1`**）：

1. **同步 trusted refs/bundle**：`git fetch` + 检出/核对冻结 HEAD，确认 worktree clean、HEAD 精确为冻结值（`code_sha` 在 commit 时冻结）；`git status --short --branch` 在签名、构建、freeze、selftest、DryRun、Live 前后检查。
2. **候选 ID 消费审计（new-candidate-id-consumption-audit-1，尚未执行）**：确认新 pair `E3-PHYS-PREFLIGHT-20260815-0001` / `EV-E3-PHYS1API26-20260815-0001` 未被任何 `execution_mode=live` / `is_evidence=true` / seal 记录占用；0002 旧 pair 的消费不在此处重复执行（已由本登记直接记录，见「旧 live 证据处置声明」节）；grep 输出与时间戳记入仓外审计日志（固定路径 `$HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260815-0001/audit/new-pair-id-consumption-audit-1.txt` + `.sha256`，hash 记录）；发现即停止，等用户新 ID 授权。
3. **blocked confirmation freeze 静态审查**：生成全新仓外 confirmation freeze（`plan_status: blocked` + `machine_fresh_confirmation.status: pending` + `independent_review_record.status: pending` + 全部外部 hash，候选 pair 为新 pair `E3-PHYS-PREFLIGHT-20260815-0001` / `EV-E3-PHYS1API26-20260815-0001`、AUTH ID `AUTH-E3-PHYS1API26-20260815-0001`；**决策字段与 0002 invalid freeze 一致**），由独立审查角色静态审查契约与角色（0 blocker/0 major）后，freeze 中 `independent_review_ready` 为 `true`——此处 `independent_review_ready=true` 只表示 **契约与角色已静态就绪**，**不是** 设备或执行就绪，也不替代 `-TargetBindingConfirm` 的机器确认；**blocked confirmation freeze 不需要独立审查 record**。
4. **host-prep 设备连接与映射（一次，用户本地 + 内存级）**：用户在 Linux 终端本地执行 `hdc tconn <dynamic-ip:port>` **恰一次**（endpoint **不落盘、不记录**），随后执行 `hdc list targets` **恰一次**；输出必须恰一 token，仅内存读取，设置 process-scope `PHYS_1_TARGET`；不持久化、不记录 endpoint/token；0/多/异常 → 停止。**不扩大 runner HDC 白名单**。
5. **机器 fresh confirmation（`-TargetBindingConfirm`）**：

    ```bash
    python3 e3-phys-preflight-campaign.py \
      -FreezeManifest $HOME/harmonyos-signing/netbird-e3/freeze/freeze-blocked-confirm-20260815-0001.json \
      -ConfirmationRecord $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260815-0001.json \
      -HapA $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/a/e3-phys-preflight-a-signed.hap \
      -HapB $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/b/e3-phys-preflight-b-signed.hap \
      -HdcPath "$HARMONYOS_STABLE_HDC" -TargetBindingConfirm
    ```

    判据：`RUNNER_RESULT=pass MODE=target-binding-confirm RECORD_KIND=target-binding-confirmation IS_EVIDENCE=false COMMAND_ATTEMPTED=3 COMMAND_COMPLETED=3 RECORD=<path> RECORD_SHA256=<sha>`；产出仓外 **双文件** record（JSON + `.sha256` companion，companion 为 completion marker），record 不含 target/serial/UDID/secret，**不** 创建 EvidenceRoot/RawRoot、**不** 消费 campaign/evidence ID。model/build 逐字匹配冻结；任何失败（含 drift）→ `RUNNER_RESULT=blocked` + **exit 2**（probe/tuple 失败；pre-record 门失败则 **exit 1 且无 record**）。drift 时该模式未安装任何 A/B、未建 staging、未起 capture，runner **不** 执行任何 cleanup/卸载/残留查询（这些操作不在本模式白名单内，也不属于 confirm 计划），无需设备端定向 cleanup；记录 blocked 后回报主会话。
6. **生成全新仓外 `ready` freeze draft（同 confirmation contract）**：新对象绑定步骤 1 的精确 clean HEAD（`code_sha`）、仓内 bytes（Python 三文件哈希，**以最终 commit 为准**）与全部完整外部 hash，并绑定步骤 5 的 record：`machine_fresh_confirmation.status=pass`、`authorization_id=AUTH-E3-PHYS1API26-20260815-0001`、`record_path=<record>`、`record_sha256=<sha>`；`plan_status: ready`；**confirmation contract 与步骤 3 的 blocked confirmation freeze 字节一致**（执行核心/候选/外部输入/code/runner/HDC/角色相同），而 `preflight_inputs_frozen_at` 推进到确认与审查 ended_at 之后（完整 freeze contract hash 因此不同，属预期）。**不得** 原地改旧 `blocked` freeze。
7. **独立审查 record（ready freeze 机械门）**：独立审查角色（`independent_reviewer_role = isolated-anthropic-claude-opus-5-reviewer`，与 operator 不同）对该 ready freeze 写出仓外 review record（`record_kind: e3-ready-freeze-review`、`is_evidence: false`、`schema_version: 1`、verdict `pass`、`blockers: 0`、`majors: 0`、`reviewer_role: isolated-anthropic-claude-opus-5-reviewer`、exact candidate pair/code_sha/runner_sha256/`confirmation_contract_sha256` 一致（稳定投影，与步骤 5 的 confirmation record 绑定同一 confirmation contract）、`machine_confirmation_sha256` 等于步骤 5 record 的 SHA-256、started/ended ≤ 最终 ready freeze 的 `preflight_inputs_frozen_at`）＋ `.sha256` companion。
8. **最终 `ready` freeze（同 confirmation contract，绑定 review record）**：新仓外对象，confirmation contract 与步骤 6 相同字节，绑定 `independent_review_record.status=pass` + `reviewer_role: isolated-anthropic-claude-opus-5-reviewer` + `record_path`/`record_sha256`（步骤 7 的 record）。
9. **候选 ID 消费审计（audit-2）**：在冻结 HEAD 上重跑步骤 2 的审计并重新记录 hash（见「两次新 ID 消费审计」节）。
10. **selftest（host-only，`HDC_PROCESSES=0`）**：

    ```bash
    python3 tests/e3-phys-preflight-runner-selftest.py
    ```

11. **DryRun（host-only，非证据；对同一份最终 `ready` freeze）**：

    ```bash
    python3 e3-phys-preflight-campaign.py \
      -FreezeManifest $HOME/harmonyos-signing/netbird-e3/freeze/freeze-ready-20260815-0001.json \
      -EvidenceRoot $HOME/harmonyos-signing/netbird-e3/evidence-dry-run-20260815-0001 \
      -RawRoot $HOME/harmonyos-signing/netbird-e3/evidence-dry-run-20260815-0001.raw \
      -HapA $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/a/e3-phys-preflight-a-signed.hap \
      -HapB $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/b/e3-phys-preflight-b-signed.hap \
      -HdcPath "$HARMONYOS_STABLE_HDC" -DryRun
    ```

    DryRun 判据：`is_evidence: false`、`HDC_PROCESSES=0`、`integrity_violations: []`、产出显式非证据 blocked record。`ready` freeze 的 DryRun 与 Live 一样验证 `machine_fresh_confirmation` 绑定（status=pass/authorization_id/record_path+sha/内容一致）与 `independent_review_record` 绑定（status=pass/reviewer_role/record_path+sha/内容一致）；`blocked` DryRun 允许 `pending`。
12. **审查 DryRun 且 freeze 字节不变**：独立审查角色（`isolated-anthropic-claude-opus-5-reviewer`）核对 DryRun 结果与 ready freeze 契约（0 blocker/0 major）后确认 **ready freeze 文件字节与步骤 8 完全一致**（重新计算 freeze SHA-256，不得改动任何字段，包括 `plan_status`）；此时方视为 Live 输入就绪。
13. **单次 Live（唯一一次；只接受步骤 8 的同一份 `plan_status: ready` freeze）**：

    ```bash
    python3 e3-phys-preflight-campaign.py \
      -FreezeManifest $HOME/harmonyos-signing/netbird-e3/freeze/freeze-ready-20260815-0001.json \
      -EvidenceRoot $HOME/harmonyos-signing/netbird-e3/evidence-live-20260815-0001 \
      -RawRoot $HOME/harmonyos-signing/netbird-e3/evidence-live-20260815-0001.raw \
      -HapA $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/a/e3-phys-preflight-a-signed.hap \
      -HapB $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/b/e3-phys-preflight-b-signed.hap \
      -HdcPath "$HARMONYOS_STABLE_HDC"
    ```

    Live 需要操作员人工机械输入；runner 在 continuous capture 前强制再次执行 `Version`/`TupleModel`/`TupleBuild` 机器核对（fresh double anchor，漂移即停止）。Live 的 preflight 属正常 campaign 流程：drift 停止后仍执行定向 cleanup verification（与 confirm 模式不同——confirm 模式不执行任何 cleanup 查询）。**执行环境必须设置 `PYTHONUNBUFFERED=1`**；主会话监控以证据目录轮询（`operator-wait-state.json`/`scenario-results.json` 的 `updated_at` 与产物增量）为准，operator 提示经 `capture-pane` 转发给用户；主会话不得对运行中的 Live 擅自发送中断信号。

**freeze 状态规则**：`-DryRun` 接受 `plan_status: blocked` 或 `ready`（永远产出非证据 blocked record）；`-TargetBindingConfirm` 接受 `plan_status: blocked` 或 `ready`（consume `machine_fresh_confirmation.pending`，产出 non-evidence confirmation record；强制 exact candidate pair + `attempt=initial` + retry N/A）；`Live`/`LiveSimulation` **只** 接受 `plan_status: ready`（runner 强制；Live 的 `ready` 还强制 `machine_fresh_confirmation` 与 `independent_review_record` 绑定，见上）。旧 `blocked` candidate freeze 不得原地改为 `ready`；`ready` freeze 必须是新仓外对象。confirmation record 状态：`record_kind: target-binding-confirmation`、`is_evidence: false`、`record_status: N/A`（非 campaign record），不进 evidence 目录、不占 campaign/evidence ID、可被 `ready` freeze 通过 `record_path`+`record_sha256` 绑定引用；review record 同理（`record_kind: e3-ready-freeze-review`、`is_evidence: false`）。sealed campaign complete record 与 preflight transcript 投影 `machine_fresh_confirmation`/`independent_review_record`：仅 status/authorization_id/reviewer_role/record_sha256/`record_path_sha256`（不泄露真实路径）并绑定 **confirmation contract**（`confirmation_contract_sha256`）；sealed complete record 顶层同时投影标准最终 `freeze_contract_sha256`（完整契约）与稳定 `confirmation_contract_sha256` 两个字段。

**两阶段 confirmation contract 规则**：完整契约（`Get-FreezeContract`）含治理/时间字段 `preflight_inputs_frozen_at`。blocked confirmation freeze 在机器确认前冻结（`T1`），最终 ready freeze 必须满足时间门（`started<=ended<=preflight_inputs_frozen_at`）而推进到 `T2 > T1`——因此 blocked freeze / ready draft / final ready freeze 的 **完整 freeze contract hash 允许且必然不同**（frozen_at 治理字段不同），但三者的 **confirmation contract 必须字节相同**（执行核心/候选 pair/外部输入/code/runner/HDC/角色；排除 `plan_status`、`preflight_inputs_frozen_at`、`machine_fresh_confirmation`、`independent_review_record`、`independent_review_ready`）。confirmation/review record 一律绑定 confirmation contract；若两阶段间 confirmation contract 发生变化，consumer 拒绝（`confirmation_contract_sha256 does not match`），防篡改绑定不因 frozen_at 推进而失效。

## 重试纪律

- `blocked`、`fail`、`invalid` 或 **no seal**（未完成封印/证据不完整）之后 **不得自动重跑**，也不得自动分配新 ID。
- **当前 AUTH 固定 `attempt=initial`（retry N/A）**：`-TargetBindingConfirm` 与消费本 AUTH confirmation 的 `ready` Live/DryRun 均被 runner 强制 exact candidate pair + `attempt=initial` + `retry.basis`/`infrastructure_reason=N/A`——因此 **现有 generic retry 分支（`attempt: infrastructure-blocked-retry-1`）不进入本次路径**。**任一 gate 失败即停止，不重试、不换 ID**。任何后续尝试必须 **新治理**：先取得新的路线决策并重新登记新授权；当前 AUTH `AUTH-E3-PHYS1API26-20260815-0001` 不可用于任何 retry。
- **0002 被误判 Live 不构成 retry 依据**：其 sealed invalid evidence（`is_evidence=true`）虽满足「prior_record 为 Live evidence」的形式条件，但其成因为 **应用显示名不区分 A/B**（S5 step3 operator-timeout；已通过显示名区分修复），不命中 retry 白名单 `hdc-usb-interruption` / `collection-storage-failure` / `runner-host-failure` 中任何一项；本 AUTH 已按新治理新建 pair 并以 `attempt=initial` 全新开始。旧 pair（`E3-PHYS-PREFLIGHT-20260814-0002` / `EV-E3-PHYS1API26-20260814-0002`）**已消费，不得复用**。
- **20260814-0001 / 20260813 / 20260810 各 pair 同样已消费、不得复用**：`E3-PHYS-PREFLIGHT-20260814-0001` / `EV-E3-PHYS1API26-20260814-0001`（0001 blocked live 证据占用）与 `E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001`（20260813 被中断 Live 的 sealed blocked 证据占用）及更早的 `20260808` 等 pair 一律不得复用。
- runner 既有 generic retry 规则仍保留给未来新治理：唯一允许的重试为 `attempt: infrastructure-blocked-retry-1`，`retry.infrastructure_reason` 必须命中白名单 `hdc-usb-interruption` / `collection-storage-failure` / `runner-host-failure`，且 `prior_record` 必须是冻结匹配的 **Live blocked evidence record**（`is_evidence: true`、`record_status: blocked`、`overall: blocked`、`verdict: blocked`、同 campaign/attempt/code/runner/artifact/freeze contract）。prior 为 DryRun、非证据、无 seal 或 `is_evidence: false` 时不构成 retry 依据；`-TargetBindingConfirm` 的 blocked confirmation record（`is_evidence: false`）同样 **不** 构成 retry 依据。
- 本授权不豁免任何既有纪律：build drift、operator-aborted、功能 fail、scenario invalid、integrity violation、非基础设施 blocked 均不授权 retry；`consumed-blocked` / `superseded-unexecuted` / `INVALID-TIMELINE` 历史 ID 一律不得复用（含旧 pair `E3-PHYS-PREFLIGHT-20260814-0002` / `EV-E3-PHYS1API26-20260814-0002`、`E3-PHYS-PREFLIGHT-20260814-0001` / `EV-E3-PHYS1API26-20260814-0001`、`E3-PHYS-PREFLIGHT-20260810-0001` / `EV-E3-PHYS1API26-20260810-0001` 与更早的 `20260808` 等 pair）。
- 任何继续或重试必须先取得新的路线决策并重新登记。

## 禁止项（明文）

本授权 **明文禁止** 下列行为，任何一项违反即停止并回报主会话：

1. **禁止任何额外动作**：禁止任何额外 discovery、UDID、serial、`hidumper`、root、privileged、Go、NetBird、product 动作；禁止任何不在现行 runner 白名单内的 HDC/设备命令（白名单仅 `Version`/`TupleModel`/`TupleBuild` 与 A/B install/start/observation/mechanical prompts/final cleanup）。
2. **禁止重复连接**：禁止 `hdc tconn` 重复执行（恰一次，用户本地；endpoint **不落盘、不记录**）；禁止 `hdc list targets` 重复执行（恰一次，内存级）；endpoint 变化 → 停止并回报主会话，不自行重连。
3. **禁止敏感值落盘**：禁止 endpoint、target token、UDID 写入仓库、日志、截图、freeze、record、evidence、transcript 或任何持久层；`PHYS_1_TARGET` 仅 process-scope 环境变量。
4. **签名材料受控**：签名材料（`.p12`、`.p7b`、`.cer`、`.csr`、密码、alias、私钥内容）只存仓外 `$HOME/harmonyos-signing` 受控目录，绝不入仓、不入聊天、不入日志；密码只使用本机安全机制，绝不写进脚本、命令行、聊天或日志。
5. **禁止改写历史**：0002 及更早授权/证据登记记录保留不改写；旧 pair（`20260814-0002`/`20260814-0001`/`20260810`/`20260808` 等）不得复用；`$HOME/harmonyos-signing/netbird-e3/evidence-live-20260814-0002`（0002 sealed invalid 证据）**保留不改写**，其正式 evidence 登记由 campaign 后独立流程处理；本登记任务本身禁止提交/推送。
6. **禁止对运行中的 Live 擅自打断**：主会话不得对运行中的 Live 发送中断信号；runner 卡住与否以证据目录轮询（`operator-wait-state.json`/`scenario-results.json` 的 `updated_at` 与产物增量）判定，operator 提示经 `capture-pane` 转发；确需中止须先经用户确认。

## 门状态

- E3 未关闭；本记录是授权登记，不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`；本授权不是 E8 `OPEN`，也不改变 E1/E4-E7 聚合状态。本实现不扩展授权边界：`-TargetBindingConfirm` 只复用现行 runner 白名单 `Version`/`TupleModel`/`TupleBuild` 三条 target-binding 命令，唯一新增为一次用户本地 `hdc tconn` + 一次内存级 `hdc list targets` host-prep 窄例外（不扩大 runner HDC 白名单），不新增任何 discovery/UDID/serial/`hidumper`/cleanup/privileged 能力。
- `plan_status: authorized-awaiting-linux-ready-freeze`：Linux 顺序门（同步 trusted refs/bundle → audit-1 → blocked confirmation freeze 静态审查 → host-prep 设备连接与映射（恰一次 tconn + 恰一次 list targets）→ `-TargetBindingConfirm` → ready draft 绑定 record → 独立审查 record → 最终 ready freeze 绑定 review → audit-2 → selftest → 同一 ready freeze DryRun → 审查 DryRun 且 freeze 字节不变 → 单次 Live）完成前不可执行。
- **Live 前提交/推送**：用户授权在 Live 前将 runner + 治理变更审查后提交/推送（Python 移植三文件 + 本登记文档），但 campaign evidence 仍不提前提交；**本登记任务未提交/推送**，提交由 Live 前的独立审查步骤执行；`code_sha` 在 commit 时冻结为最终 HEAD。
- 历史记录全部保留不改写：API23 initial（`EV-E3-PHYS1API23-20260806-0001`）、rebind（`EV-E3-PHYS1REBIND7-20260806-0001`）、build 确认（`EV-E3-PHYS1BUILD7-20260806-0001`）、API26 0001（`EV-E3-PHYS1API26-20260807-0001`，`consumed-blocked`）、API26 0002（`EV-E3-PHYS1API26-20260807-0002`，`consumed-blocked`）、host remediation（`EV-E3-PHYS1HOST-20260808-0001`）、外部 sealed blocked evidence `EV-E3-PHYS1API26-20260808-0001`（占用 20260808 pair）、`ADJ-20260808-0001/0002/0003` 登记、旧授权 `AUTH-E3-PHYS1API26-20260810-0001`（superseded，consumed）、`AUTH-E3-PHYS1API26-20260810-0002`、`AUTH-E3-PHYS1API26-20260813-0001`、`AUTH-E3-PHYS1API26-20260814-0001` 与 `AUTH-E3-PHYS1API26-20260814-0002`（历史登记保留不改写）；**0002 事故证据**：`$HOME/harmonyos-signing/netbird-e3/evidence-live-20260814-0002`（sealed invalid，`is_evidence=true`、`overall=invalid`、cleanup `verified-clean`、seal 完整、exit 2）占用 0002 pair，**保留不改写、不作为 retry 依据**；正式 evidence 登记（reviewed）由 campaign 后独立流程处理。

## 用户批准记录

本文起草即获用户批准（2026-08-15 上午，用户选择方案 1）。批准前本登记不构成可执行授权；批准后 `authorization_status: granted`，本登记是当前唯一生效的授权登记。

| 字段 | 值 |
| --- | --- |
| 批准时间 | `2026-08-15T09:3x:xx+08:00`（Asia/Shanghai；约 09:30，落盘精确值以生成时刻为准） |
| 批准方式 | 聊天确认（用户选择方案 1） |
| 批准后 authorization_status | `granted` |

## 执行注记（2026-08-15 上午）

1. **0002 Live 事实定性**：S1–S4 全 pass（S2 为 **API26 首次** marker 修复验证成功——`:vpn` 截断 tag / `requestId=missing` 行被 `test_line_correlated` 正确关联；S4 "取消"按钮被 cancel|deny 布局验证接受）；S5 step1 pass、S5 step3 operator-timeout（应用管理页两 APP 同名无法区分 A）→ `SCENARIO_INVALID` → seal invalid（exit 2）；cleanup `verified-clean`。**根因为应用显示名相同，非设备/runner 故障**。
2. **本次变更**：显示名区分（`app_name_a="E3 Preflight A"` / `app_name_b="E3 Preflight B"` + build-profile label 绑定）；audit 脚本 3 处既有漂移修复（raw pin `b584bad2…` 跟进 `4d4caef`；`LEDGER_MAX_FUTURE_MS`/`LEDGER_MAX_AGE_MS` count 1→2 跟进 `c6acae7`）；新 Linux 构建+重签 HAP（`20260815-relabeled`，A `828fefed…`/`133935`、B `c9e064ed…`/`133945`，验签 4 项 exit 0、A/B 归属互不串扰、`device_id_count:1`）；旧 HAP（`f44be17-final`）保留为历史输入。
3. **常量迁移与重新冻结**：runner/selftest/freeze example 治理常量迁移至 20260815-0001 系列，重新冻结（runner `9572d55a…`、selftest `c0d566f3…`、freeze example `030afd0e…`）；三文件已随本登记提交，`code_sha` 以 blocked freeze 绑定的最终 commit 为准（权威值由 freeze 承载）。
4. **按纪律停止**：0002 invalid 后不重试、不换 ID；本授权按新治理新建 pair 并以 `attempt=initial` 全新开始。E3 未关闭，E8 保持 `CLOSED`。
