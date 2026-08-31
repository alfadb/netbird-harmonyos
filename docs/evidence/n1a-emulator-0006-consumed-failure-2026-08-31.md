# N1a Emulator campaign 0006 consumed-failure 记录（2026-08-31 执行）——缺陷 #7（regex ^ 锚）修复后 0006 原始数据验证为全 pass

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260831-0005` / `EV-N1A-EMU24-20260831-0005`（AUTH-N1A-EMU24-20260831-0005 granted，判据 frozen-r3）的执行事实：**探针在 Emulator 上第三次连续全判据 pass（chunk 传输完整、无截断），但 runner 的重组函数因 regex `^N1A_JSON` 行首锚定未匹配带 hilog 时间戳前缀的真实行而失败**（selftest 用裸 `N1A_JSON` 行故不暴露；缺陷 #7）。修复（去锚定）后用本 campaign 的真实 hilog 独立验证：**7/7 片、sha `d042f92ca9d029b5` 与声明摘要逐字匹配、JSON 完整解析、`verdict: pass`**——探针实测事实在缺陷修复后的离线复验中完整成立。

## 0006 探针实测（离线复验自本 campaign 的原始 chunk 数据，非新测量）

- 全部判据 pass（c5=not-triggered）；握手 831ms；4000/4000 逐字节；吞吐 **55.86 MiB/s**；6 个真实 persistent keepalive；fd 门 pass（socket 29/30 T3 全关）；tunnel_free×2；进程级观察 30→30/14→14 零漂移；RSS 92,468→93,952 KB。

## 处置

- 本 campaign `consumed-failure`（runner defect 于判定前终止；探针无测量消费——同 0003/0005 逻辑）。
- 缺陷 #7 修复后 selftest 22/22 过 + 本 campaign 真实数据离线复验 sha/parse 全通——**runner 链路首次对真实 hilog 数据端到端验证**。
- 下一个 campaign（第 7 个 ID）预期首次完整走通两阶段流。须用户授权。
