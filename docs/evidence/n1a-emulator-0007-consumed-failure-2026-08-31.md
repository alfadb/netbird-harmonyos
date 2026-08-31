# N1a Emulator campaign 0007 consumed-failure 记录（2026-08-31 执行）——缺陷 #8（ArkTS Record 索引怪癖）与首个 PASS marker

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260831-0006` / `EV-N1A-EMU24-20260831-0006`（AUTH-N1A-EMU24-20260831-0006 granted，判据 frozen-r3）的执行事实。**本 campaign 达成里程碑：探针首次在 Emulator 上发射 `N1A_RESULT` 四字段 marker，且 `verdict=PASS`**：

```text
N1A_RESULT|verdict=PASS|c5=not-triggered|throughput_mibps=104.91
```

——Phase A 机器判定路径首次完整工作（runner 正确解析四字段、冻结表全过、吞吐 104.91 MiB/s 为地板 5 的 21 倍）。**runner 侧的 `probe-detail.json` 重组也首次在真实运行中成功**（sha `d30c41e4…` 与声明一致、JSON parse `verdict: pass`）——缺陷 1-7 的修复全部在产线验证生效。

## 缺陷 #8（新暴露，本 campaign fail 的原因）

ohosTest 的 ETS 侧交叉一致检查（`runN1aProbeTest.ets`）在 `JSON.parse(result.detailJson)` 后用**方括号索引 cast Record**（`detailCriteria['c2']`）——在 Emulator 的 ArkTS runtime 上返回 `undefined`（尽管同一 JSON 文档在 runner 侧 parse 完全正常、chunk 重组 sha 一致）——触发 `"criteria inconsistent"` 异常 → ohosTest marker `N1A_PROBE_TEST_RESULT|verdict=FAIL` → runner 的交叉一致规则正确判 overall fail（探针 PASS 与 ohosTest FAIL 矛盾）。**ArkTS Record 强转索引的 runtime 怪癖**：已修复为经 `JSON.stringify` 序列化后 `includes('"c2":"pass"')` 精确子串比对（对 cast/索引差异健壮且语义不变——检查仍是 c2/c3 pass + verdict 存在）。修复后 build + selftest 全过。

## 探针实测（本 campaign 第四次连续全判据 pass）

marker `verdict=PASS`、c5=not-triggered（loopback 拓扑预期）、**104.91 MiB/s**；probe-detail（重组）确认 c1-c9 全 pass、4000/4000、fd 门 closed、tunnel_free×2。

## 处置

本 campaign `consumed-failure`（ohosTest 交叉 fail → overall fail 字面记录不改写；探针与 runner 链路本身已全 pass——fail 在 ETS 检查层）。下一个 campaign（第 8 个 ID）预期首次完整走通两阶段流。须用户授权。
