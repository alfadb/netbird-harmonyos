# N1a Emulator campaign 0004 执行授权登记（2026-08-31 · 0003，granted）

最后核验：2026-08-31

## 授权状态

```yaml
authorization_id: AUTH-N1A-EMU24-20260831-0003
exception: N1A-EMU24-DATAPLANE
record_status: granted-confirmed
is_evidence: false
authorization_status: granted
plan_status: granted-executing
attempt: initial
retry: N/A
candidate:
  campaign_id: N1A-EMU24-20260831-0003
  evidence_id: EV-N1A-EMU24-20260831-0003
  identity_status: pending
predecessors:
  - {campaign: N1A-EMU24-20260830-0001, status: consumed-failure (C7/C8 criteria-defect; defects 1-2 fixed)}
  - {campaign: N1A-EMU24-20260831-0001, status: consumed-failure (overlay guard trip; defect 3 fixed)}
  - {campaign: N1A-EMU24-20260831-0002, status: consumed-blocked (env hang; defect 4 fixed)}
criteria_basis: docs/n1a-gate-plan.md @ criteria-frozen-r3
implementation_basis: HEAD e669864 (all four defect fixes landed)
user_authorization: "确认" (2026-08-31)
```

包含全部四项实现层修复（runner 全终态封签、JSON 分段、overlay 诊断快照、HDC 超时）；诊断通道将在探针触达时捕获 0002 守卫触发与否的真实字段值。单次终态制。
