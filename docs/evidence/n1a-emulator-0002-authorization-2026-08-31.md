# N1a Emulator campaign 0002 执行授权登记（2026-08-31 · 0001，granted）

最后核验：2026-08-31

## 授权状态

```yaml
authorization_id: AUTH-N1A-EMU24-20260831-0001
exception: N1A-EMU24-DATAPLANE
information_status: current-governance-registration
record_status: granted
is_evidence: false
authorization_status: granted
plan_status: authorized-awaiting-implementation-alignment
attempt: initial
retry: N/A
candidate:
  campaign_id: N1A-EMU24-20260831-0001
  evidence_id: EV-N1A-EMU24-20260831-0001
  identity_status: pending
  consumed: false
  reusable: false
criteria_basis: docs/n1a-gate-plan.md @ criteria-frozen-r3 (HEAD 2cfa8b5 或其后代)
predecessor:
  campaign_id: N1A-EMU24-20260830-0001
  evidence_id: EV-N1A-EMU24-20260830-0001
  status: consumed-failure (C7/C8 fail; criteria-defect ruled; runner seal defect; ID 不可复用)
  disposition: 0001 fail 不作为探针泄漏主张; C7/C8 判据缺陷已由 r3 修复
governance_chain: ADJ-T0-NATIVE-NX-20260830-0001 -> N0 (pass) -> G0 (blocked) -> T0 route A -> N1a gate open
user_authorization: "都做" (2026-08-31, direct human decision: 探针 r3 重构 + 新 ID 重测一并批准)
operator: authorized user
orchestrator: main agent
```

## 授权范围

- campaign `N1A-EMU24-20260831-0001` / evidence `EV-N1A-EMU24-20260831-0001`（attempt initial，单次执行、失败/阻塞均消费本 ID）。
- 判据：`docs/n1a-gate-plan.md` frozen-r3（C7/C8 探针自有资源门 + 两阶段流解释的全部登记限定语）。
- 环境：官方 API 24 x86_64 phone Emulator（`netbird_api24_phone`、HDC `127.0.0.1:10000`）——Emulator HDC 属既有 E/N campaign 授权面。
- 实现层前置：探针 C7/C8 r3 重构完成 + 全套 host 验证通过（build/cargo/runner selftest）+ runner fail 路径封签修复已入库。
- **无后继 AUTH**：本 campaign 结果（pass/blocked/fail）为终态；后续须新治理。

## 边界

- 单次执行，不 retry 不换 ID。
- 白名单 HDC（N0 同路径 `aa test`/`aa start`/`bm install`/`bm uninstall`/HiLog 采集/截图）。
- 不触物理设备、不触 VPN/protect/N2+ 内容、不外推。
