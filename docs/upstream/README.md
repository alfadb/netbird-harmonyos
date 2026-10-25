# 上游变更调研记录

本目录保存针对 NetBird 上游版本区间的调研报告。它们是一次性调研的产出，不是长期维护的文档：上游继续前进后，这些报告描述的就是历史区间。

## 为什么在这里

0.0.55 把上游 pin 从 commit `7914010`（2026-09-12）对齐到 tag `v0.81.4`（commit `7e791d3`，2026-10-24）。做这个决定前需要回答两个问题：区间内官方改了什么、哪些需要本客户端跟进。两份报告就是证据链，提交进仓库的理由是：pin 为什么停在这个 tag、为什么某些官方变更我们不做，答案都在里面。

## 文件

| 文件 | 覆盖区间 | 说明 |
| --- | --- | --- |
| [netbird-0.79.0-to-0.80.0.md](netbird-0.79.0-to-0.80.0.md) | v0.78.1 → v0.79.0 → v0.80.0 | 两个正式版的逐条变更（88 个 bullet，含标签分类）、note 与 git 区间差异、对第三方客户端有实际影响的条目 |
| [netbird-0.80.0-to-0.81.4.md](netbird-0.80.0-to-0.81.4.md) | v0.80.0 → v0.81.4 | 五个正式版（0.81.0–0.81.4）的逐条变更、协议 diff、需要客户端配合的条目（证书姿态、ingress 下线、SessionExtend、eMudhra CA） |

## 两点使用提醒

1. **不能只看 release note**。0.81.1 的 note 只有 1 条但实际有 17 个提交，0.81.2 的 note 只有 1 条但实际有 6 个；0.81.4 的 note 有一条重复 bullet。两份报告因此都以 git compare 区间为准，note 只作对照。这些版本的 note 里也**没有** Breaking Changes 小节，不能据此认为没有影响。
2. **行号时效**。报告里的 `file:line` 取自各自写作时的检出（`v0.81.4` 或 `v0.80.0`），上游继续前进后会漂移。

## 原始数据在哪

报告依赖的原始 JSON（releases、tags、compare 区间、git trees）、逐 tag 的 proto 原文、以及逐 PR 的 patch 都在 `artifacts/upstream-eval-2026-10-24/`。该目录**不入库**（见根 `.gitignore`），属于本机过程证据；如果换了机器需要重新抓取，报告里每条结论都附了来源 URL。

## 结论怎么落地的

- 协议层：只有 `management.proto` 有纯增量字段（`certificateProofs` / `certificateChallenge`、`SessionExtend`）与 ingress 相关的 deprecated 标记；`signalexchange.proto`、`flow.proto`、relay 协议在整个区间**逐字节未变**。所以 pin 对齐只是让副本可核验，不改行为。
- 行为层：从两份报告里挑出与本仓自研机制同域的上游修复，逐条对照后确认两个真实缺陷（Signal 注册等待无上界、relay 终态不卸承载）与一个能力缺口（per-peer ICE sessionId），已在 0.0.55 修复；其余判为「结构上不成立」或「无关」，理由记在发布记录里。
