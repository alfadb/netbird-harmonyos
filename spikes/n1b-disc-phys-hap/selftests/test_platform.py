#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc platform/death/join selftests（gate 10 前置义务分期项 9/10/11 钉）。

一条命令运行（pytest 与自研 main 双兼容）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_platform.py
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_platform.py

覆盖组：
  A. 任务 9 —— u1-u7 平台分量派生（前提缺失→unobservable、域外→
     value-outside-frozen-domain、正常→observed-true/false 三类钉 + S5 分区表
     全四行 + u4 四支 + u7 三区间/阶段未达/skip 优先/tail-loss）；
  B. 任务 10 —— dw_join_result 完整 10 值域重建（sticky/skip/死亡收口/join-
     blocked/ESRCH）+ P12 内层 join= 比对（F8(2)）；
  C. 任务 11 —— D8b 收口三分流（BEGIN-only span / 阶段未达 / 存活未完成 F9 面）
     + 新剧本格端到端（stage-not-reached / storm-death / F9 / death-in-d6b）。
"""

from __future__ import annotations

import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                os.pardir, "runner"))
import n1bdisc_core as core        # noqa: E402
import n1bdisc_death as death      # noqa: E402
import n1bdisc_fsm as fsm          # noqa: E402
import n1bdisc_platform as pf      # noqa: E402
import n1bdisc_run as run          # noqa: E402
import n1bdisc_verdict as verdict  # noqa: E402
import fake_hdc as fake            # noqa: E402

_ASSERTS = 0
UNOBS = core.unobservable_value


def expect(cond, msg):
    global _ASSERTS
    _ASSERTS += 1
    if not cond:
        raise AssertionError(msg)


# --------------------------------------------------------------------------
# 夹具 helpers
# --------------------------------------------------------------------------

def events_of(*fakes):
    """FakeMarker 序列 → MarkerEvent 序列（scan 保序、line_no 按输入序）。"""
    return core.scan_markers([f.line() for f in fakes])


def chunk_map(**texts):
    """(stream) → 文本 的简易 chunk 重组结果（u3hex 单记录形态）。"""
    return core.ChunkReassembly({(s, 0): t for s, t in texts.items()}, (), ())


#: 与 fake_hdc._U3HEX_FRAME_HEX 同帧（48 B、PI 00000800、total_length=44）。
FRAME_HEX = fake._U3HEX_FRAME_HEX


def fm(short, at, kv=None, form="entry"):
    return fake.FakeMarker(short, at, kv or {}, form)


def d4_stream(off="4", ret="16", end=True):
    evs = [fm("D4_BEGIN", 100, {"mono_ms": "100"}),
           fm("D4_SENT", 110, {"n": "1", "ret": ret, "errno": "0"}),
           fm("D4_READ", 120, {"off": off, "len": "48"})]
    if end:
        evs.append(fm("D4_END", 130, {"ok": "true"}))
    return evs


# ==========================================================================
# A. 任务 9：u1-u7 平台分量派生
# ==========================================================================

def test_u1_premise_and_offset_domain():
    # 正常：前提（ret==16）+ 任一 offset 匹配 → observed-true（:540-541）
    u1 = pf.derive_u1(events=events_of(*d4_stream(off="4")),
                      retained_entry="MR1")
    expect(u1["u1_socket_to_tun_delivery"] == "observed-true"
           and u1["u1_match_offset"] == "4",
           "前提成立 + off=4 匹配 → observed-true / match_offset=4")
    # off=both 同样即 true（:540 任一 offset 匹配即 true）
    u1b = pf.derive_u1(events=events_of(*d4_stream(off="both")),
                       retained_entry="MR1")
    expect(u1b["u1_socket_to_tun_delivery"] == "observed-true"
           and u1b["u1_match_offset"] == "both", "off=both → observed-true")
    # 窗口耗尽（END 在）零匹配 → observed-false；off=none 不是匹配
    u1f = pf.derive_u1(events=events_of(*d4_stream(off="none")),
                       retained_entry="MR1")
    expect(u1f["u1_socket_to_tun_delivery"] == "observed-false"
           and u1f["u1_match_offset"] == "none",
           "off=none 非匹配 + END 在 → observed-false")
    # 前提缺失（无 D4_SENT）→ unobservable（不伪装平台事实）
    u1m = pf.derive_u1(events=events_of(fm("D4_BEGIN", 100)),
                       retained_entry="MR1")
    expect(u1m["u1_socket_to_tun_delivery"] == UNOBS("marker-gap-indeterminate"),
           "无 sendto 前提标记 → unobservable（前提缺失 fail-closed）")
    # 全部 sendto -1 → send-failed（:546）
    u1s = pf.derive_u1(events=events_of(
        fm("D4_SENT", 110, {"n": "1", "ret": "-1", "errno": "13"}),
        fm("D4_SENT", 111, {"n": "2", "ret": "-1", "errno": "13"}),
        fm("D4_END", 130, {})), retained_entry="MR1")
    expect(u1s["u1_socket_to_tun_delivery"] == UNOBS("send-failed"),
           "全部 -1 → unobservable(cause=send-failed)")
    # 短写从无完整成功 → short-or-zero-io（:545）
    u1p = pf.derive_u1(events=events_of(
        fm("D4_SENT", 110, {"n": "1", "ret": "8", "errno": "0"}),
        fm("D4_SENT", 111, {"n": "2", "ret": "0", "errno": "0"}),
        fm("D4_END", 130, {})), retained_entry="MR1")
    expect(u1p["u1_socket_to_tun_delivery"] == UNOBS("short-or-zero-io"),
           "有 0/部分、从无 n==16 → unobservable(cause=short-or-zero-io)")
    # 域外 off 字面 → 不参与匹配 + value-outside-frozen-domain（:1375 不 fail）
    u1o = pf.derive_u1(events=events_of(*d4_stream(off="tun_pi-like")),
                       retained_entry="MR1")
    expect(u1o["u1_match_offset"] == UNOBS("value-outside-frozen-domain")
           and u1o["raw"]["out_of_domain_offs"][0]["off"] == "tun_pi-like",
           "域外 off → match_offset=value-outside-frozen-domain、原值入 raw")
    expect(u1o["u1_socket_to_tun_delivery"] == "observed-false",
           "域外 off 不构成匹配 → 窗口耗尽零匹配 observed-false")
    # skip 分支 → 整组同 cause（:993）
    u1k = pf.derive_u1(events=events_of(fm("SKIP", 100, {"item": "D4",
                                                         "cause": "no-live-fd"})),
                       retained_entry=None)
    expect(u1k["u1_socket_to_tun_delivery"] == UNOBS("no-live-fd")
           and u1k["u1_match_offset"] == UNOBS("no-live-fd"),
           "D4 skip → u1/u1_match_offset 同 cause 编码")


def test_u1_mb1_no_route_control():
    # MB1 保留：主字段固定 mb1-no-route（:542），对照字段按 E9 同一前提门（:543-547）
    u1 = pf.derive_u1(events=events_of(*d4_stream(off="4")),
                      retained_entry="MB1")
    expect(u1["u1_socket_to_tun_delivery"] == UNOBS("mb1-no-route"),
           "MB1 保留 → U1 主字段固定 mb1-no-route（零匹配不伪答）")
    expect(u1["u1_no_route_control"] == "observed-true",
           "u1_no_route_control 按同一前提/匹配门 → observed-true")
    u1f = pf.derive_u1(events=events_of(*d4_stream(off="none")),
                       retained_entry="MB1")
    expect(u1f["u1_no_route_control"] == "observed-false",
           "对照字段窗口耗尽零匹配 → observed-false")
    u1m = pf.derive_u1(events=events_of(fm("D4_BEGIN", 100)),
                       retained_entry="MB1")
    expect(u1m["u1_no_route_control"] == UNOBS("marker-gap-indeterminate"),
           "对照字段前提缺失 → unobservable")


def test_u2_premise_and_identity():
    def d5(src="10.99.0.2", ret="44", end=True):
        evs = [fm("D5_BEGIN", 200),
               fm("D5_WRITE", 210, {"round": "1", "ret": ret, "errno": "0"}),
               fm("D5_RECV", 220, {"round": "1", "src": src})]
        if end:
            evs.append(fm("D5_END", 230, {}))
        return evs
    # 正常：前提（n==44）+ 冻结 src 收到 → observed-true（:581-583）
    u2 = pf.derive_u2(events=events_of(*d5()), retained_entry="MR1")
    expect(u2["u2_tun_write_to_sink_delivery"] == "observed-true",
           "write n==44 + 冻结 src recv → observed-true")
    # 窗口耗尽零收到 → observed-false；非冻结 src 不构成身份匹配
    u2f = pf.derive_u2(events=events_of(*d5(src="172.16.0.9", end=True)),
                       retained_entry="MR1")
    expect(u2f["u2_tun_write_to_sink_delivery"] == "observed-false",
           "非冻结 src（外来）+ END 在 → observed-false")
    # 前提缺失（无 write 轮）→ unobservable
    u2m = pf.derive_u2(events=events_of(fm("D5_BEGIN", 200)), retained_entry="MR1")
    expect(u2m["u2_tun_write_to_sink_delivery"] == UNOBS("marker-gap-indeterminate"),
           "无 write 前提 → unobservable")
    # 全部 -1 → write-failed；短写 → short-or-zero-io（:581）
    u2w = pf.derive_u2(events=events_of(
        fm("D5_WRITE", 210, {"round": "1", "ret": "-1", "errno": "5"}),
        fm("D5_END", 230, {})), retained_entry="MR1")
    expect(u2w["u2_tun_write_to_sink_delivery"] == UNOBS("write-failed"),
           "全部写 -1 → unobservable(cause=write-failed)")
    u2s = pf.derive_u2(events=events_of(
        fm("D5_WRITE", 210, {"round": "1", "ret": "20", "errno": "0"}),
        fm("D5_END", 230, {})), retained_entry="MR1")
    expect(u2s["u2_tun_write_to_sink_delivery"] == UNOBS("short-or-zero-io"),
           "短写从无 n==44 → unobservable(cause=short-or-zero-io)")
    # MB1 保留 → 冻结 src 换 192.0.2.2（:578）
    u2b = pf.derive_u2(events=events_of(*d5(src="192.0.2.2")),
                       retained_entry="MB1")
    expect(u2b["u2_tun_write_to_sink_delivery"] == "observed-true",
           "MB1 保留 → 冻结 src=192.0.2.2 匹配")
    # skip 分支
    u2k = pf.derive_u2(events=events_of(fm("SKIP", 100, {"item": "D5",
                                                         "cause": "dup-failed"})),
                       retained_entry=None)
    expect(u2k["u2_tun_write_to_sink_delivery"] == UNOBS("dup-failed"),
           "D5 skip → dup-failed 编码")


def test_u3_partition_table_all_four_rows():
    # 行 4：offset-0 与 offset-4 均可解析 → ambiguous 无条件（:558）
    # 构造：byte0 与 byte4 均 0x45（ver4/IHL5）——两 offset 双可解析
    ambiguous_hex = "4500002c" + "4500002c" + "00" * 40
    u3 = pf.derive_u3(events=events_of(*d4_stream(off="both")), retained_entry="MR1",
                      chunks=chunk_map(u3hex=ambiguous_hex))
    expect(u3["u3_pi_header_present"] == "ambiguous",
           "S5 行 4：两 offset 均可解析 → 无条件 ambiguous（不被截胡）")
    expect(u3["u3_prefix_format"] == UNOBS("prefix-ambiguous"),
           "ambiguous → u3_prefix_format=unobservable(prefix-ambiguous)")
    expect(u3["u3_readlen_vs_total_length"] == "readlen>total_length"
           and u3["u3_readlen_vs_total_length_off0"] == "readlen>total_length",
           "off=both 决定性 offset=4 + off0 对照字段并记（:563-564）")
    # 行 3：仅 offset-4 可解析 → tun_pi-like（前 4 字节 00000800）
    u3t = pf.derive_u3(events=events_of(*d4_stream(off="4")), retained_entry="MR1",
                       chunks=chunk_map(u3hex=FRAME_HEX))
    expect(u3t["u3_pi_header_present"] == "tun_pi-like"
           and u3t["u3_prefix_format"] == "observed-true",
           "S5 行 3 + tun_pi 形态 → tun_pi-like / observed-true")
    expect(u3t["u3_first_read_len"] == 48
           and u3t["u3_readlen_vs_total_length"] == "readlen>total_length",
           "readlen 48 > total_length 44（offset-4 口径显式化）")
    # 行 3 反例：仅 offset-4、前 4 字节非 tun_pi 形态 → other-prefix + 原文逐字
    other_hex = "deadbeef" + "4500002c" + "00" * 40
    u3o = pf.derive_u3(events=events_of(*d4_stream(off="4")), retained_entry="MR1",
                       chunks=chunk_map(u3hex=other_hex))
    expect(u3o["u3_pi_header_present"] == "other-prefix"
           and u3o["u3_prefix_format"] == "observed-false"
           and u3o["raw"]["other_prefix_raw4"] == "deadbeef",
           "other-prefix → observed-false + 4 字节原文逐字登记（:557/:567）")
    # 行 2：仅 offset-0 可解析 → no-prefix；total_length=0x30(48)==readlen → equal
    no_prefix_hex = "45000030" + "00" * 44
    u3n = pf.derive_u3(events=events_of(*d4_stream(off="0")), retained_entry="MR1",
                       chunks=chunk_map(u3hex=no_prefix_hex))
    expect(u3n["u3_pi_header_present"] == "no-prefix"
           and u3n["u3_prefix_format"] == "observed-false"
           and u3n["u3_readlen_vs_total_length"] == "equal",
           "S5 行 2 → no-prefix / observed-false / readlen==total_length")
    # 行 1：均不可解析 → unparsable
    u3u = pf.derive_u3(events=events_of(*d4_stream(off="4")), retained_entry="MR1",
                       chunks=chunk_map(u3hex="0001020304050607"))
    expect(u3u["u3_pi_header_present"] == "unparsable"
           and u3u["u3_prefix_format"] == UNOBS("frame-unparsable")
           and u3u["u3_readlen_vs_total_length"] == "unparsable",
           "S5 行 1 → unparsable / frame-unparsable / readlen 比较域 unparsable")
    # 零匹配（含 MB1 保留）→ 全部 no-controlled-read（:548）
    u3z = pf.derive_u3(events=events_of(*d4_stream(off="none")), retained_entry="MR1",
                       chunks=chunk_map(u3hex=FRAME_HEX))
    expect(u3z["u3_pi_header_present"] == UNOBS("no-controlled-read")
           and u3z["u3_first_read_len"] == UNOBS("no-controlled-read"),
           "零匹配 → U3 全字段 unobservable(no-controlled-read)")
    u3m = pf.derive_u3(events=events_of(*d4_stream(off="4")), retained_entry="MB1",
                       chunks=chunk_map(u3hex=FRAME_HEX))
    expect(u3m["u3_prefix_format"] == UNOBS("no-controlled-read"),
           "MB1 保留 → U3 全字段 no-controlled-read（:548）")


def test_u4_four_branches_and_summary():
    # 正常（complete）：result marker 在 → observed-true（:982）
    evs = events_of(fm("D6S1_B", 100), fm("D6S1_R", 110, {"ret": "0"}),
                    fm("D6S2_B", 120), fm("D6S2_R", 130, {"ret": "0"}))
    u4 = pf.derive_u4(events=evs, destroy_call_state="call-returned",
                      death_observed=False, post_present=True)
    expect(u4["subitems"]["u4_orig_getfd"] == "observed-true"
           and u4["subitems"]["u4_orig_getfl"] == "observed-true"
           and u4["u4_post_destroy_sync_observable"] == "observed-true",
           "result marker 在 → observed-true，摘要 observed-true")
    # complete：pre 在 + 死亡 + 零 result → observed-false（:982）
    evs2 = events_of(fm("D6S4_B", 100))
    u4f = pf.derive_u4(events=evs2, destroy_call_state="call-returned",
                       death_observed=True, post_present=True)
    expect(u4f["subitems"]["u4_dup_getfd"] == "observed-false",
           "pre 在 + 死亡 + 零 result → observed-false")
    # pre-only (1a)：_T/_C 缺、无 SKIP → destroy-not-reached（:1188）
    u4a = pf.derive_u4(events=events_of(), destroy_call_state="not-reached",
                       death_observed=True, post_present=False)
    expect(u4a["subitems"]["u4_orig_getfd"] == UNOBS("destroy-not-reached"),
           "pre-only (1a) → destroy-not-reached（不得假负值）")
    # pre-only (1b)：任一 SKIP|item=destroy → destroy-not-called（:1189）
    u4b = pf.derive_u4(events=events_of(fm("SKIP", 100, {"item": "destroy",
                                                         "cause": "no-live-connection"})),
                       destroy_call_state="not-called",
                       death_observed=True, post_present=False)
    expect(u4b["subitems"]["u4_orig_close"] == UNOBS("destroy-not-called"),
           "pre-only (1b) → destroy-not-called")
    # pre-only (2)：_T 在 _C 缺 → call-boundary-incomplete（:1191）
    u4c = pf.derive_u4(events=events_of(fm("DW_DESTROY_T", 100, {"mono_ms": "100"})),
                       destroy_call_state="call-boundary-incomplete",
                       death_observed=True, post_present=False)
    expect(u4c["subitems"]["u4_dup_read"] == UNOBS("call-boundary-incomplete"),
           "pre-only (2) → call-boundary-incomplete（不得 observed-false）")
    # pre-only (3)：_C 在 + resolve 证据 → observed-false；D6a 正结果保持（:1186/:1192）
    evs3 = events_of(fm("DW_DESTROY_T", 100, {"mono_ms": "100"}),
                     fm("DW_DESTROY_C", 110, {"mono_ms": "110"}),
                     fm("D6S1_B", 120), fm("D6S1_R", 130, {"ret": "0"}),
                     fm("D6S4_B", 140))
    u4r = pf.derive_u4(events=evs3, destroy_call_state="call-returned",
                       death_observed=True, post_present=False)
    expect(u4r["subitems"]["u4_orig_getfd"] == "observed-true"
           and u4r["subitems"]["u4_dup_getfd"] == "observed-false"
           and u4r["subitems"]["u4_dup_fd_reuse"] == "observed-false",
           "pre-only (3)：死亡前 result 保持、未执行子项 observed-false")
    # pre-only (4)：_C 在、无 resolve 证据 → destroy-unresolved（:1193）
    u4u = pf.derive_u4(events=events_of(fm("DW_DESTROY_T", 100, {"mono_ms": "100"}),
                                        fm("DW_DESTROY_C", 110, {"mono_ms": "110"})),
                       destroy_call_state="call-returned",
                       death_observed=True, post_present=False)
    expect(u4u["subitems"]["u4_orig_getfd"] == UNOBS("destroy-unresolved"),
           "pre-only (4) → destroy-unresolved（不得假负值）")
    # D6b 整段 skip → dup 四子项 d6b-skipped-join-timeout（:969）
    evs4 = events_of(fm("SKIP", 100, {"item": "D6b",
                                      "cause": "join-timeout-abandoned"}),
                     fm("D6S1_B", 110), fm("D6S1_R", 120, {"ret": "0"}))
    u4d = pf.derive_u4(events=evs4, destroy_call_state="call-returned",
                       death_observed=False, post_present=True)
    expect(u4d["subitems"]["u4_dup_getfd"] == UNOBS("d6b-skipped-join-timeout")
           and u4d["subitems"]["u4_dup_read"] == UNOBS("d6b-skipped-join-timeout")
           and u4d["subitems"]["u4_orig_getfd"] == "observed-true",
           "D6b skip → u4_dup_* 四子项新具名 cause、D6a 不受影响")
    # D-W 整体 skip → 全部 no-live-fd（:999-1000）；摘要全序（:983-984）
    u4k = pf.derive_u4(events=events_of(), destroy_call_state="not-called",
                       death_observed=False, post_present=True,
                       dw_skip="no-live-fd")
    expect(all(v == UNOBS("no-live-fd") for v in u4k["subitems"].values())
           and u4k["u4_post_destroy_sync_observable"] == UNOBS("no-live-fd"),
           "D-W skip → 七子项全 no-live-fd、摘要 unobservable")
    # 摘要全序：全 false 且无 true → observed-false；任一 unobservable 无 true → unobservable
    evs5 = events_of(fm("D6S1_B", 100), fm("D6S1_R", 110, {"ret": "-1"}))
    u4s = pf.derive_u4(events=evs5, destroy_call_state="call-returned",
                       death_observed=True, post_present=True)
    expect(u4s["u4_post_destroy_sync_observable"] == "observed-true",
           "任一子项 observed-true → 摘要 observed-true")


def test_u5_candidates_domain():
    evs = events_of(fm("D2_ENTRY", 100, {"id": "MR1", "outcome": "resolved"}),
                    fm("D2_ENTRY", 110, {"id": "MR1B", "outcome": "rejected"}))
    u5 = pf.derive_u5(events=evs)
    expect(u5["candidates"]["MR1"] == "observed-true"
           and u5["candidates"]["MR1B"] == "observed-false",
           "resolved → observed-true / rejected → observed-false（:479）")
    expect(u5["candidates"]["MR2"] == UNOBS("protocol-first-accept-lock")
           and u5["candidates"]["MB1"] == UNOBS("protocol-first-accept-lock"),
           "accept-lock 后未执行条目 → protocol-first-accept-lock")
    evs2 = events_of(fm("D2_ENTRY", 100, {"id": "MR1", "outcome": "timeout"}),
                     fm("D2_ENTRY", 101, {"id": "MR1", "outcome": "indeterminate"}))
    u5b = pf.derive_u5(events=evs2)
    expect(u5b["candidates"]["MR1"] == UNOBS("create-indeterminate"),
           "窗尽 indeterminate → create-indeterminate")
    expect(u5b["candidates"]["MR1B"] == UNOBS("matrix-terminated-on-create-timeout"),
           "timeout 终止后未执行 → matrix-terminated-on-create-timeout")
    evs3 = events_of(fm("D2_ENTRY", 100, {"id": "MR1", "outcome": "late-rejected"}))
    u5c = pf.derive_u5(events=evs3)
    expect(u5c["candidates"]["MR1"] == "observed-false",
           "late-rejected → observed-false")
    # 无任何矩阵终局证据的缺 marker 条目 → marker-gap（偏差登记，不驱动 verdict）
    u5d = pf.derive_u5(events=events_of(fm("D1_BEGIN", 100)))
    expect(u5d["candidates"]["MR1"] == UNOBS("marker-gap-indeterminate"),
           "条目 marker 全缺 → unobservable（fail-closed 记录）")


def test_u6_s4_derivation_table():
    evs = events_of(fm("D2_S4", 100, {"u6": "o_nonblock_present"}))
    u6 = pf.derive_u6(events=evs, fd_roles_created=frozenset({"fd_orig", "fd_dup"}))
    expect(u6["u6_initial_flags_and_isblocking_effect"] == "observed-true"
           and u6["u6_nonblocking_initial"] == "observed-true",
           "o_nonblock_present → observed-true（:499）")
    evs2 = events_of(fm("D2_S4", 100, {"u6": "o_nonblock_absent"}))
    u6b = pf.derive_u6(events=evs2, fd_roles_created=frozenset({"fd_orig", "fd_dup"}))
    expect(u6b["u6_nonblocking_initial"] == "observed-false",
           "o_nonblock_absent → observed-false（:500）")
    # S4 缺 → 沿分支表（:501）：fd_orig 缺 → no-live-fd；dup 缺 → dup-failed
    u6n = pf.derive_u6(events=events_of(), fd_roles_created=frozenset())
    expect(u6n["u6_nonblocking_initial"] == UNOBS("no-live-fd"),
           "S4 缺 + 无 fd → unobservable(cause=no-live-fd)")
    u6d = pf.derive_u6(events=events_of(), fd_roles_created=frozenset({"fd_orig"}))
    expect(u6d["u6_nonblocking_initial"] == UNOBS("dup-failed"),
           "fd_orig 在 fd_dup 缺 → unobservable(cause=dup-failed)")
    # 域外字面 → value-outside-frozen-domain（:1375）
    evs3 = events_of(fm("D2_S4", 100, {"u6": "weird-literal"}))
    u6o = pf.derive_u6(events=evs3, fd_roles_created=frozenset({"fd_orig", "fd_dup"}))
    expect(u6o["u6_nonblocking_initial"] == UNOBS("value-outside-frozen-domain"),
           "S4 域外字面 → value-outside-frozen-domain")


def test_u7_intervals_stage_not_reached_and_skip_first():
    def d7(elapsed=None, iters="400", end=True, begin=True):
        evs = []
        if begin:
            evs.append(fm("D7_BEGIN", 100, {"start_mono_ms": "100"}))
        if end:
            kv = {"elapsed_ms": str(elapsed), "iters": iters}
            evs.append(fm("D7_END", 31000, kv))
        return events_of(*evs)
    # 合法区间两端 → observed-true（:648/:652）
    for e in (20000, 25000):
        expect(pf.derive_u7(events=d7(elapsed=e), death_observed=False,
                            last_site="P6", tail_state="tail-complete"
                            )["u7_long_task_watchdog_behavior"] == "observed-true",
               "elapsed=%d ∈ [20000,25000] → observed-true" % e)
    # 25001 → overshoot 具名观察、不判 fail（:649）
    u7o = pf.derive_u7(events=d7(elapsed=25001), death_observed=False,
                       last_site="P6", tail_state="tail-complete")
    expect(u7o["u7_long_task_watchdog_behavior"]
           == UNOBS("d7-elapsed-overshoot-beyond-grace")
           and u7o["d7_anomaly"] == "elapsed-overshoot-observed"
           and not u7o["f8_facets"],
           "elapsed>25000 → overshoot 具名三态观察、无 F8 facet")
    # 19999 → 矛盾输入 fail(F8)（标签同时驱动 F8，:647/:653）
    u7e = pf.derive_u7(events=d7(elapsed=19999), death_observed=False,
                       last_site="P6", tail_state="tail-complete")
    expect(u7e["u7_long_task_watchdog_behavior"] == UNOBS("d7-early-exit-anomaly")
           and u7e["d7_anomaly"] == "early-exit-with-end-marker"
           and any("d7-early-exit-contradiction" in f for f in u7e["f8_facets"]),
           "elapsed<20000 → early-exit 标签 + F8 facet（verdict 承载）")
    # 负值 → 单调钟域门先命中（:654）
    u7n = pf.derive_u7(events=d7(elapsed=-1), death_observed=False,
                       last_site="P6", tail_state="tail-complete")
    expect(any("monotonic-domain" in f for f in u7n["f8_facets"]),
           "elapsed<0 → 单调钟域门 F8 facet")
    # END 缺、BEGIN 后静默 + 死亡 → observed-false（:655）
    u7f = pf.derive_u7(events=events_of(fm("D7_BEGIN", 100, {"start_mono_ms": "100"})),
                       death_observed=True, last_site="P6",
                       tail_state="tail-loss-not-indicated")
    expect(u7f["u7_long_task_watchdog_behavior"] == "observed-false",
           "BEGIN 后静默 + 死亡 → observed-false（任务被杀）")
    # V4 约束：位点 P6 ∧ possible-tail-loss → 不得 observed-false（:663-666）
    u7t = pf.derive_u7(events=events_of(fm("D7_BEGIN", 100, {"start_mono_ms": "100"})),
                       death_observed=True, last_site="P6",
                       tail_state="possible-tail-loss")
    expect(u7t["u7_long_task_watchdog_behavior"] == UNOBS("marker-tail-loss"),
           "位点 P6 + tail-loss → marker-tail-loss（宁缺勿误）")
    # 阶段未达（:658-661）：D7_BEGIN 缺、位点 ≤ P5T、死亡
    u7s = pf.derive_u7(events=events_of(fm("PRE", 100, {})), death_observed=True,
                       last_site="P5T", tail_state="tail-loss-not-indicated")
    expect(u7s["u7_long_task_watchdog_behavior"]
           == UNOBS(death.STAGE_NOT_REACHED_CAUSE)
           and u7s["elapsed_ms"] == UNOBS(death.STAGE_NOT_REACHED_CAUSE),
           "阶段未达 → u7 与派生字段同 cause stage-not-reached")
    # skip 分支优先（:662，r14 夹具 :1463）：SKIP|item=D7 → no-live-vpn 不是 stage-not-reached
    u7k = pf.derive_u7(events=events_of(fm("SKIP", 100, {"item": "D7",
                                                         "cause": "no-live-vpn"})),
                       death_observed=True, last_site="P5T",
                       tail_state="tail-loss-not-indicated")
    expect(u7k["u7_long_task_watchdog_behavior"] == UNOBS("no-live-vpn"),
           "skip 分支优先 → no-live-vpn（不得 stage-not-reached）")
    # no-live-fd 全局分支 → D7 skip 表指派（:996）
    u7g = pf.derive_u7(events=events_of(), death_observed=False,
                       last_site="P2", tail_state="no-death-evidence",
                       dw_skip="no-live-fd")
    expect(u7g["u7_long_task_watchdog_behavior"] == UNOBS("no-live-vpn"),
           "no-live-fd 分支 → u7=unobservable(no-live-vpn)（无 live VPN 不得 true/false）")
    # END、BEGIN 均缺、无死亡 → marker-gap（:657 禁止以 marker 缺失推断被杀）
    u7m = pf.derive_u7(events=events_of(), death_observed=False,
                       last_site="P6", tail_state="no-death-evidence")
    expect(u7m["u7_long_task_watchdog_behavior"]
           == UNOBS("marker-gap-indeterminate"),
           "marker 缺失无死亡证据 → marker-gap-indeterminate")


def test_platform_components_e2e_three_builtin_scenarios():
    for name, expect_verdict in (("happy", "pass"), ("pre-only", "pass"),
                                 ("no-live-fd", "pass")):
        camp = run.run_dryrun_campaign(name)
        judged = run.derive_and_judge(camp)
        pc = judged["platform_components"]
        result = judged["verdict_result"]
        expect(result.verdict == expect_verdict and not result.gates,
               "%s：既有 verdict 不变（gates=%r）" % (name, result.gates))
        expect(not pc["f8_facets"] and not pc["f4_facets"],
               "%s：平台分量派生零 F8/F4 facet" % name)
        expect(pc["u1"]["u1_socket_to_tun_delivery"] is not None
               and pc["u7"]["u7_long_task_watchdog_behavior"] is not None,
               "%s：u1-u7 字段全部有值" % name)


# ==========================================================================
# B. 任务 10：dw_join_result 完整重建 + P12 比对
# ==========================================================================

def j(**kw):
    defaults = dict(dw_skip_cause=None, destroy_call_state="not-reached",
                    d6b_skip_present=False, join_timeout_registered=False,
                    join_blocked_registered=False, exit_present=False,
                    exit_rc=None, post_present=True, death_observed=False)
    defaults.update(kw)
    return core.DwJoinInput(**defaults)


def test_join_authoritative_ten_value_pins():
    # joined（EXIT 在）；域闭合
    r = core.derive_dw_join_result(j(exit_present=True))
    expect(r.outcome == "value" and r.value == "joined", "EXIT 在 → joined")
    expect(r.value in core.DW_JOIN_RESULT_10, "落值 ∈ 10 值域")
    # skip 编码两值（:997-1000）
    for cause in ("no-live-fd", "dup-failed"):
        r2 = core.derive_dw_join_result(j(dw_skip_cause=cause, exit_present=True))
        expect(r2.value == UNOBS(cause), "D-W skip → %s 编码" % cause)
    # join sticky（:693）：D6b skip 在，迟到 EXIT/RETURN 存在不得改写 joined
    r3 = core.derive_dw_join_result(j(d6b_skip_present=True, exit_present=True))
    expect(r3.value == "join-timeout" and r3.sticky == "join",
           "D6b skip ∧ 迟到 EXIT → join-timeout（不得改写 joined）")
    # runner 侧 JT 登记前件等价
    r4 = core.derive_dw_join_result(j(join_timeout_registered=True,
                                      exit_present=True))
    expect(r4.value == "join-timeout", "JT 已登记 → join-timeout（同 sticky）")
    # barrier-never-observed 顺延路径：not-called + JT 登记 → join-timeout（:1177）
    r5 = core.derive_dw_join_result(j(destroy_call_state="not-called",
                                      join_timeout_registered=True,
                                      post_present=False, death_observed=True))
    expect(r5.value == "join-timeout",
           "barrier-never-observed 顺延 → join-timeout（P10 盒先于 SKIP 到期）")
    # join-blocked-observed（runner 依轮询超时登记，:720/:1091）
    r6 = core.derive_dw_join_result(j(join_blocked_registered=True))
    expect(r6.value == "join-blocked-observed",
           "runner 登记 join 阻塞 → join-blocked-observed")
    # pthread_join 返回：ESRCH / other+errno
    expect(core.derive_dw_join_result(j(exit_rc=3)).value == "ESRCH",
           "join 返回 ESRCH → ESRCH")
    expect(core.derive_dw_join_result(j(exit_rc=22)).value == "other+errno",
           "join 返回其他 errno → other+errno")
    # pre-only 死亡收口三 cause（:1171-1175）
    for state, cause in (("not-reached", "destroy-not-reached"),
                         ("call-returned", "post-destroy-unobservable"),
                         ("call-boundary-incomplete", "call-boundary-incomplete")):
        r7 = core.derive_dw_join_result(j(destroy_call_state=state,
                                          post_present=False, death_observed=True))
        expect(r7.value == UNOBS(cause), "pre-only %s → %s" % (state, cause))
    # not-called 无 JT/D-W skip → 真值表缺口 fail（F8 面，调用方入档）
    r8 = core.derive_dw_join_result(j(destroy_call_state="not-called",
                                      post_present=False, death_observed=True))
    expect(r8.outcome == "fail" and "truth-table" in r8.fail_reason,
           "not-called 残余格 → fail（不伪装域内值）")


def test_join_p12_consistency_and_f82():
    # 域内字面逐字比对
    expect(verdict.p12_join_consistency("joined", "joined", "fd-event-like"),
           "重建 joined = POST join=joined → 一致")
    expect(not verdict.p12_join_consistency("joined", "join-timeout", None),
           "不一致 → False（F8(2) 面）")
    # pending 形态（探针 skip 形态内层字面）：skip 重建 + class 列同 cause → 一致
    expect(verdict.p12_join_consistency(UNOBS("no-live-fd"), "pending",
                                        UNOBS("no-live-fd")),
           "pending + runner skip 编码 + class 列同 cause → 一致")
    expect(not verdict.p12_join_consistency("joined", "pending", "fd-event-like"),
           "pending + 非 skip 重建 → 不一致（fail-closed）")
    expect(not verdict.p12_join_consistency(UNOBS("no-live-fd"), "pending",
                                            UNOBS("dup-failed")),
           "pending + class 列 cause 不同 → 不一致")
    expect(not verdict.p12_join_consistency(None, "bogus", None),
           "域外 POST 字面 / 重建缺席 → 不一致")
    # F8(2) 挂载路径：不一致进入 parse_domain_gaps → verdict fail
    result = verdict.evaluate(verdict.VerdictInput(
        pre_present=True, post_present=True,
        parse_domain_gaps=["P12 dw_join_result='joined' != runner rebuild "
                           "'join-timeout'"]))
    expect(result.verdict == "fail" and "F8" in result.gates,
           "join 比对不一致 → fail（F8(2)）")


def test_join_e2e_flag_race_sticky_and_pre_only():
    # flag-race 剧本：D6b skip 在 → runner sticky 重建 join-timeout，与 POST 一致
    camp = run.run_dryrun_campaign("flag-race", fake.make_flag_race_scenario())
    judged = run.derive_and_judge(camp)
    dw = judged["dw"]
    expect(dw["rebuilt_join"] == "join-timeout" and dw["join_sticky"] == "join",
           "flag-race：join sticky 重建 join-timeout（D6b skip 载体）")
    expect(dw["post_join"] == "join-timeout", "POST 内层 join=join-timeout 一致")
    result = judged["verdict_result"]
    expect(result.verdict == "pass" and "F8" not in result.gates,
           "flag-race 合法 campaign → pass（F8 不命中）")
    # pre-only：死亡收口 → destroy-not-reached（:1551 同款）
    camp2 = run.run_dryrun_campaign("pre-only")
    judged2 = run.derive_and_judge(camp2)
    expect(judged2["dw"]["rebuilt_join"] == UNOBS("destroy-not-reached"),
           "pre-only 死于 D7 → dw_join_result=destroy-not-reached（:1551）")
    # happy：join 轴 joined
    camp3 = run.run_dryrun_campaign("happy")
    judged3 = run.derive_and_judge(camp3)
    expect(judged3["dw"]["rebuilt_join"] == "joined"
           and judged3["dw"]["post_join"] == "joined",
           "happy：join 轴 joined、比对一致")


# ==========================================================================
# C. 任务 11：D8b 收口三分流 + 新剧本格端到端
# ==========================================================================

def test_d8b_begin_only_closure_and_span():
    out = death.eval_d8b_closure(begin_kv={"ws": "31100"}, end_kv=None,
                                 death_observed=True,
                                 death_site_before_storm=False,
                                 death_wall_ms=100_000,
                                 begin_capture_wall_ms=90_000)
    expect(out["branch"] == "storm-incomplete-pre-only", "BEGIN-only 支命中")
    expect(out["window_start_monotonic"] == 31100,
           "window_start 自 ws 照常重建（不受 END 缺失影响，:604）")
    expect(out["window_end_monotonic"] == UNOBS("storm-incomplete-pre-only"),
           "window_end → storm-incomplete-pre-only（:604）")
    for f in ("eagain_observed", "partial_write_observed",
              "bytes_written_total", "write_calls", "caps_hit"):
        expect(out[f] == UNOBS("storm-incomplete-pre-only"),
               "%s → storm-incomplete-pre-only（一条 cause 覆盖六项）" % f)
    expect(out["storm_incomplete_pre_only"] is True
           and out["storm_incomplete_pre_only_span"] == 10_000,
           "布尔置位 + 跨度 = 死亡墙钟 − BEGIN capture 墙钟 = 10000（:607）")
    expect(out["raw"]["storm_begin_ws"] == 31100
           and out["raw"]["death_wall_ms"] == 100_000,
           "ws 原值与死亡证据墙钟逐字入 raw（:607）")
    # 墙钟不可求值 → 跨度不登记（:607 同侧墙钟禁止跨钟相减的输入面）
    out2 = death.eval_d8b_closure(begin_kv={"ws": "31100"}, end_kv=None,
                                  death_observed=True,
                                  death_site_before_storm=False,
                                  death_wall_ms=None,
                                  begin_capture_wall_ms=90_000)
    expect(out2["storm_incomplete_pre_only_span"] is None,
           "任一墙钟不可求值 → 跨度不登记")


def test_d8b_stage_not_reached_and_contrast():
    out = death.eval_d8b_closure(begin_kv=None, end_kv=None,
                                 death_observed=True,
                                 death_site_before_storm=True,
                                 death_wall_ms=100_000)
    expect(out["branch"] == "stage-not-reached", "阶段未达支命中（:609）")
    for f in death.D8B_FIELDS:
        expect(out[f] == UNOBS("stage-not-reached"),
               "%s → stage-not-reached（七字段全列，:610）" % f)
    expect(out["storm_incomplete_pre_only"] is False
           and out["storm_incomplete_pre_only_span"] is None,
           "布尔 false、不登记跨度（:614）")
    # 对照（:608）：BEGIN 在 END 缺、进程仍活 → 存活未完成 F4 面（非 storm cause）
    out2 = death.eval_d8b_closure(begin_kv={"ws": "31100"}, end_kv=None,
                                  death_observed=False,
                                  death_site_before_storm=False)
    expect(out2["branch"] == "alive-incomplete"
           and out2["eagain_observed"] is None
           and len(out2["f4_missing"]) == len(death.D8B_FIELDS)
           and all("storm-incomplete" not in str(v) for v in out2.values()
                   if isinstance(v, str)),
           "无死亡证据 → alive-incomplete F4 面（不取 storm-incomplete cause）")
    # 对照（:615）：无 BEGIN、无死亡 → 同 F4 面
    out3 = death.eval_d8b_closure(begin_kv=None, end_kv=None,
                                  death_observed=False,
                                  death_site_before_storm=True)
    expect(out3["branch"] == "alive-incomplete"
           and out3["f4_missing"] == ["D8b.%s" % f for f in death.D8B_FIELDS],
           "无 BEGIN 无死亡 → 真未完成（字段缺项沿 F4 面）")


def test_d8b_rebuild_order_gate_and_skip():
    out = death.eval_d8b_closure(
        begin_kv={"ws": "31100"},
        end_kv={"we": "41100", "eagain": "true", "partial": "false",
                "bytes": "4194304", "calls": "40000", "caps_hit": "false"},
        death_observed=False, death_site_before_storm=False)
    expect(out["branch"] == "rebuilt"
           and out["window_start_monotonic"] == 31100
           and out["window_end_monotonic"] == 41100
           and out["bytes_written_total"] == "4194304"
           and out["write_calls"] == "40000"
           and not out["f8_facets"] and not out["f4_missing"],
           "正常重建：ws/we + 五字段逐字、零 facet")
    # ws > we → 单调钟顺序约束 F8（:830）
    out2 = death.eval_d8b_closure(begin_kv={"ws": "500"},
                                  end_kv={"we": "400", "eagain": "true",
                                          "partial": "false", "bytes": "1",
                                          "calls": "1", "caps_hit": "false"},
                                  death_observed=False, death_site_before_storm=False)
    expect(any("window_start" in f for f in out2["f8_facets"]),
           "ws > we → F8 顺序约束 facet")
    # END 载荷字段缺 → F4 面
    out3 = death.eval_d8b_closure(begin_kv={"ws": "500"}, end_kv={"we": "600"},
                                  death_observed=False, death_site_before_storm=False)
    expect(len(out3["f4_missing"]) == 5
           and all(m.startswith("D8_STORM_END.") for m in out3["f4_missing"]),
           "END 载荷字段缺 → F4 缺项面")
    # skip 表指派（:995）
    out4 = death.eval_d8b_closure(begin_kv=None, end_kv=None,
                                  death_observed=False,
                                  death_site_before_storm=False,
                                  skip_cause="no-live-fd")
    expect(out4["branch"] == "skip"
           and all(v == UNOBS("no-live-fd") for v in
                   (out4[f] for f in death.D8B_FIELDS)),
           "D8b skip → 全字段 unobservable(no-live-fd)")
    # 真值表未覆盖：死亡、无 BEGIN、位点不早于 P7 → f8 facet
    out5 = death.eval_d8b_closure(begin_kv=None, end_kv=None,
                                  death_observed=True,
                                  death_site_before_storm=False)
    expect(out5["branch"] == "uncovered" and out5["f8_facets"],
           "未覆盖格 → fail-closed f8 facet")


def test_site_order_linkage_for_storm_branch():
    # last_visible_site 与收口支联动（位点映射沿 fsm.SITE_ORDER，:1228）
    def before(site):
        idx = fsm.SITE_ORDER.index(site)
        return idx < fsm.SITE_ORDER.index("P7")
    expect(before("P5T") and before("P6") and not before("P7") and not before("P8"),
           "死亡位点 < P7 判定沿 SITE_ORDER（P5T/P6 在 storm 前，P7/P8 不在）")
    # 联动端到端：last_site=P6 的死亡走 BEGIN-only（BEGIN 在）；无 BEGIN 的 P6 死亡
    # 走阶段未达
    out = death.eval_d8b_closure(begin_kv=None, end_kv=None,
                                 death_observed=True,
                                 death_site_before_storm=before("P6"))
    expect(out["branch"] == "stage-not-reached",
           "last_visible_site=P6（D7 内死亡）∧ 无 BEGIN → 阶段未达支")


def test_e2e_death_after_pre_stage_not_reached():
    camp = run.run_dryrun_campaign("death-after-pre",
                                   fake.make_death_after_pre_scenario())
    judged = run.derive_and_judge(camp)
    result = judged["verdict_result"]
    expect(result.verdict == "pass"
           and result.gates == [],
           "stage-not-reached 剧本 → pre-only pass（F4/F8/F1 不命中）")
    expect(camp["last_site"] == "P5T" and camp["death_observed"],
           "last_visible_site=P5T、死亡分量 observed-true（:1461 夹具）")
    u7 = judged["platform_components"]["u7"]
    expect(u7["u7_long_task_watchdog_behavior"]
           == UNOBS("stage-not-reached"),
           "u7=unobservable(stage-not-reached)（:1461）")
    d8b = judged["d8b_closure"]
    expect(d8b["branch"] == "stage-not-reached"
           and d8b["window_start_monotonic"] == UNOBS("stage-not-reached")
           and d8b["storm_incomplete_pre_only"] is False,
           "D8b 七字段 stage-not-reached、布尔 false（:610/:614）")
    expect(judged["dw"]["rebuilt_join"] == UNOBS("destroy-not-reached"),
           "join 轴 pre-only 死亡收口 → destroy-not-reached")


def test_e2e_storm_death_begin_only_pass():
    camp = run.run_dryrun_campaign("storm-death", fake.make_storm_death_scenario())
    judged = run.derive_and_judge(camp)
    result = judged["verdict_result"]
    expect(result.verdict == "pass" and result.gates == [],
           "storm-incomplete-pre-only → pass（合法平台死亡终态，:1457）")
    d8b = judged["d8b_closure"]
    expect(d8b["branch"] == "storm-incomplete-pre-only"
           and d8b["storm_incomplete_pre_only"] is True
           and d8b["storm_incomplete_pre_only_span"] == 10_000
           and d8b["window_start_monotonic"] == 31100,
           "BEGIN-only：布尔 + 跨度 10000 + window_start 自 ws 重建")
    expect(d8b["eagain_observed"] == UNOBS("storm-incomplete-pre-only"),
           "END 承载字段 → storm-incomplete-pre-only（:604）")
    expect(judged["platform_components"]["u7"]["u7_long_task_watchdog_behavior"]
           == "observed-true",
           "D7 完成于死亡前 → u7=observed-true")
    expect(camp["tail_state"] == "tail-loss-not-indicated",
           "静默跨度 20000 ≤ T_tail → tail-loss-not-indicated（联动）")


def test_e2e_alive_incomplete_f9_fail():
    camp = run.run_dryrun_campaign("alive-incomplete",
                                   fake.make_alive_incomplete_scenario())
    judged = run.derive_and_judge(camp)
    result = judged["verdict_result"]
    expect(result.verdict == "fail" and result.gates == ["F9"],
           "存活未完成 → fail（F9，:1152/:1067——不是平台事实）")
    expect(camp["close_kind"] == fsm.CLOSE_FAIL_F9
           and camp["death_observed"] is False,
           "FSM fail-f9 收口、死亡分量未确认")
    expect(judged["platform_components"]["u7"]["u7_long_task_watchdog_behavior"]
           == "observed-true",
           "D7 完成字段照常有值（存活未完成不抹事实）")
    expect(judged["dw"]["rebuilt_join"] == "joined",
           "EXIT 在 → join 轴 joined")


def test_e2e_death_in_d6b_u4_branch3():
    camp = run.run_dryrun_campaign("death-in-d6b",
                                   fake.make_death_in_d6b_scenario())
    judged = run.derive_and_judge(camp)
    result = judged["verdict_result"]
    expect(result.verdict == "pass" and result.gates == [],
           "死于 D6b 中途 → pre-only pass（:1196）")
    u4 = judged["platform_components"]["u4"]
    expect(u4["subitems"]["u4_orig_getfd"] == "observed-true"
           and u4["subitems"]["u4_orig_close"] == "observed-true",
           "死亡前 D6a 正结果保持（:1186）")
    expect(all(u4["subitems"][k] == "observed-false" for k in (
        "u4_dup_getfd", "u4_dup_read", "u4_dup_close", "u4_dup_fd_reuse")),
           "V2 (3)：destroy 已 resolve → 未执行子项逐项 observed-false（:1192）")
    expect(judged["platform_components"]["destroy_call_state"] == "call-returned",
           "五态 call-returned")
    expect(judged["dw"]["rebuilt_join"] == UNOBS("post-destroy-unobservable"),
           "join 轴死亡收口 → post-destroy-unobservable（:1173）")


# ==========================================================================
# main runner（pytest 不在场时的一条命令入口）
# ==========================================================================

def main() -> int:
    import time
    tests = [(name, fn) for name, fn in sorted(globals().items())
             if name.startswith("test_") and callable(fn)]
    passed = failed = 0
    started = time.time()
    for name, fn in tests:
        try:
            fn()
            passed += 1
            print("PASS %s" % name)
        except Exception as exc:  # noqa: BLE001 — main 汇总所有失败后统一退出码
            failed += 1
            print("FAIL %s: %s" % (name, exc))
    elapsed = time.time() - started
    print("-" * 60)
    print("n1bdisc_platform selftests: tests=%d passed=%d failed=%d assertions=%d (%.0f ms)"
          % (len(tests), passed, failed, _ASSERTS, elapsed * 1000))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
