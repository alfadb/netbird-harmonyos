# N1a Emulator campaign 0003 执行授权登记（2026-08-31 · 0002，granted）

最后核验：2026-08-31

## 授权状态

```yaml
authorization_id: AUTH-N1A-EMU24-20260831-0002
exception: N1A-EMU24-DATAPLANE
record_status: granted-confirmed
is_evidence: false
authorization_status: granted
plan_status: granted-executing
attempt: initial
retry: N/A
candidate:
  campaign_id: N1A-EMU24-20260831-0002
  evidence_id: EV-N1A-EMU24-20260831-0002
  identity_status: pending
predecessors:
  - campaign: N1A-EMU24-20260830-0001
    status: consumed-failure (C7/C8 criteria-defect ruled; sealed-defect)
  - campaign: N1A-EMU24-20260831-0001
    status: consumed-failure (overlay guard trip; observability defect 3)
criteria_basis: docs/n1a-gate-plan.md @ criteria-frozen-r3
implementation_basis: HEAD 478e35e (defect-3 fix: 11/11 diag snapshots,
  guard-replica host tests 17/17, THROW_SNAPSHOT build gate)
strategy: user-approved 修复 -> host 异常链验证 -> 新 ID 重测 (2026-08-31)
```

用户于 2026-08-31 确认（"确认"），授权生效。host 验证门已过（守卫复刻单测证明 overlay 逻辑 host 一致、矛盾锁定运行时 ABI/加载层；诊断通道 12 处覆盖 + 构建断言负例验证）。诊断通道将捕获 0002 未解矛盾的实际字段值。
