# N1a Emulator campaign 0005 执行授权登记（2026-08-31 · 0004，granted）

最后核验：2026-08-31

```yaml
authorization_id: AUTH-N1A-EMU24-20260831-0004
exception: N1A-EMU24-DATAPLANE
record_status: granted-confirmed
is_evidence: false
authorization_status: granted
plan_status: granted-executing
attempt: initial
retry: N/A
candidate:
  campaign_id: N1A-EMU24-20260831-0004
  evidence_id: EV-N1A-EMU24-20260831-0004
predecessors:
  - {campaign: N1A-EMU24-20260830-0001, status: consumed-failure (C7/C8 criteria-defect; defects 1-2 fixed)}
  - {campaign: N1A-EMU24-20260831-0001, status: consumed-failure (overlay guard; defect 3 fixed)}
  - {campaign: N1A-EMU24-20260831-0002, status: consumed-blocked (env hang; defect 4 fixed)}
  - {campaign: N1A-EMU24-20260831-0003, status: consumed-failure (constant 2000 vs 4000; defect 5 fixed)}
criteria_basis: docs/n1a-gate-plan.md @ criteria-frozen-r3
implementation_basis: HEAD ddb9cc9 (all five defect fixes landed; probe unchanged)
user_authorization: "确认" (2026-08-31)
```

五项缺陷全部消；探针侧零改动。预期首次完整走通两阶段流（Phase A 机器判定 + Phase B EntryAbility 结果页 + 截图）。单次终态制。
