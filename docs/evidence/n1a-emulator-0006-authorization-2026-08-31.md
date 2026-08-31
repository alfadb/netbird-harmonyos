# N1a Emulator campaign 0006 执行授权登记（2026-08-31 · 0005，granted）

最后核验：2026-08-31

```yaml
authorization_id: AUTH-N1A-EMU24-20260831-0005
record_status: granted-confirmed
is_evidence: false
authorization_status: granted
plan_status: granted-executing
attempt: initial
retry: N/A
candidate:
  campaign_id: N1A-EMU24-20260831-0005
  evidence_id: EV-N1A-EMU24-20260831-0005
predecessors:
  - {campaign: N1A-EMU24-20260830-0001, status: consumed-failure, defects: "1-2 fixed"}
  - {campaign: N1A-EMU24-20260831-0001, status: consumed-failure, defects: "3 fixed"}
  - {campaign: N1A-EMU24-20260831-0002, status: consumed-blocked, defects: "4 fixed"}
  - {campaign: N1A-EMU24-20260831-0003, status: consumed-failure, defects: "5 fixed"}
  - {campaign: N1A-EMU24-20260831-0004, status: consumed-failure, defects: "6 fixed (chunk 384->320)"}
criteria_basis: docs/n1a-gate-plan.md @ criteria-frozen-r3
implementation_basis: HEAD 42981a0 (all six defect fixes landed; probe unchanged)
user_authorization: "确认" (2026-08-31)
```
