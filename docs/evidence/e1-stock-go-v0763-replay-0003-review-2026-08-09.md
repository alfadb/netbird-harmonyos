# E1 v0.76.3 stock Go 重放 EV-E1-EMU24-20260809-0003 终审 REV-E1-EMU24-20260809-0003

最后核验：2026-08-09

本文是 `EV-E1-EMU24-20260809-0003`（[measured-blocked 记录](e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md)）的正式双路独立终审记录。两路独立审查（`anthropic/claude-opus-5` evidence-integrity 与 `moonshotai/kimi-k2.7-code` status-consistency）均 **0 blocker / 0 major**；主会话系统重算 11 份材料 sha256 与记录值全部匹配。结论：`record_status: reviewed-pass`、`verdict: blocked`。`reviewed-pass` 只表示审查完成且无 integrity blocker，**不是** E1 pass；功能判定仍是 `blocked`。

## 审查元数据

```yaml
review_id: REV-E1-EMU24-20260809-0003
evidence_id: EV-E1-EMU24-20260809-0003
reviewed_at: 2026-08-09T18:28:32+08:00
reviewers:
  - anthropic/claude-opus-5 (evidence-integrity)
  - moonshotai/kimi-k2.7-code (status-consistency)
review_mode: dual independent read-only review (no device/Emulator/replay execution, no file modification)
blocker: 0
major: 0
minor: 6
record_status: reviewed-pass
verdict: blocked
```

## 审查范围与方法

- 只读复核：不运行设备/Emulator/replay、不修改任何文件（含 raw）。
- 主会话系统重算 11 份材料 sha256，与 measured-blocked 记录 `raw_log_sha256` 全部匹配：九份 formal raw（transcript / aa-test / baseline-verify / build / go-build / hilog-tag / hilog-app-full / emulator-console / manifest）与两份带外补充（qemu-boot-section / baseline-postmortem）。
- manifest 自 hash 语义复核匹配：
  - manifest 前 23 行 sha256 = `a171d6aa600283f5ed8892655a58a65c96de78cfc0e7036d9599baef3dd4ee0c`（与 manifest 内 `manifest_sha256` 行一致）；
  - full manifest sha256 = `ce5c28d138c00f61aa81b9718b777521cd0f409ed502fb7233ff81bebc874a66`；
  - transcript sha256 = `797882fd814666674474179b636e76ac43a234bba59eda3cfc44e57184b23bec`（与 manifest 内 `transcript_final_sha256` 行及 raw transcript 文件一致）。
- 两路独立审查结论：均 **0 blocker / 0 major**。

## 结论

- `record_status: reviewed-pass`：独立审查确认范围、完整性和判定满足引用标准。
- `verdict: blocked`：功能判定保持 blocked（stock Go 1.25.12 loader 在 API 24 x86_64 phone Emulator 上被 guest loader 精确拒绝，`initial-exec TLS resolves to dynamic definition`；预期行为，非 runner failure）。
- `reviewed-pass` 不构成 E1 pass；E1 overall Go 仍无 pass；E8 保持 `CLOSED`。

## 非阻塞 minor（6 条）

两路审查均无 blocker/major；以下 6 条 minor 为记录改进建议，不改变判定，不要求改写 raw：

1. **系统版本来自冻结元数据非本次查询**：`target_tuple.full_system_version` 的 `6.1.0.125` 来自冻结 Emulator image 元数据，0003 未在 guest 内单独查询；记录已显式标注，但引用时须注意该值不是本次运行实测。
2. **staging 主路径清理但 teardown 标签易误读**：`CLEANUP_STAGING=skipped-emulator-not-started` 易被误读为 staging 未清理或 Emulator 从未启动；实际主流程 step9 已清 staging、卸载并停止 Emulator，并将 `emulator_started` 置 0，EXIT teardown 因此打印 skipped 标签，不表示 Emulator 从未启动或漏清理（`FINAL_RESIDUAL_PROCESS=false`、`FINAL_RESIDUAL_PORT=false`）。
3. **QEMU 摘录只是 post-run 佐证且源会追加**：`qemu-boot-section.log` 是 formal run 之后生成的 out-of-band 摘录，源 `qemu.log` 会随后续运行追加，摘录行范围（33670–33955）只对生成时点有效；引用时须按 OUT-OF-BAND 语义处理。
4. **自检与独立审查需区分**：measured-blocked 记录中的「独立只读审查摘要」实为作者自检（已改名为「作者自检摘要」），正式双路终审以本文为准；引用时不得把自检当作独立审查。
5. **expected 可更明确引用门**：`expected` 字段可更明确引用对应门/退出条款（如 E1 门与 R0 SLO），当前以行为描述为主。
6. **verdict 枚举/注释机器解析注意**：`verdict: blocked` 与 `record_status: reviewed-pass` 的枚举语义（reviewed-pass 不等于功能 pass）依赖注释说明，机器解析时须按 schema 枚举处理，不得把 reviewed-pass 直接映射为 pass。

## 边界

- **无 E1 pass**：本审查不产生、也不改写任何 E1 pass；E1 overall Go 仍无 pass。
- **E1 overall 未关闭**：`EV-E1-EMU24-20260809-0003` 为 measured blocked 证据，E1 overall Go 门未关闭。
- **E8 保持 `CLOSED`**：`e8_status=CLOSED`（manifest）；本记录不改变 E8 状态。
- **仅精确 API 24 x86_64 Emulator**：结论只适用于记录的目标元组（HarmonyOS 6.1.1(24) / API 24 / x86_64 / phone Emulator `netbird_api24_phone`），不得外推 arm64、具名物理设备或华为商用 HarmonyOS。
- **非物理**：`PHYSICAL_DEVICE_USED=false`、`HDC_RUN=true`；E3 物理设备禁 HDC 约束不变。
- **不得同 ID 重跑**：`EV-E1-EMU24-20260809-0003` 已消耗，禁止同 ID 重跑；未来新重验须主会话另行分配新 ID（本记录不预分配）。
- **E4-E7 豁免不适用**：本记录是 E1 的 measured blocked 证据，不构成 E4-E7 任何豁免；E4-E7 义务仍按 schema 移交 E8 `OPEN` 后的具名物理设备 R2/R3 门。
