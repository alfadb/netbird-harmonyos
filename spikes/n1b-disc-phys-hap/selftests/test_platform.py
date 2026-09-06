#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc platform/death/join selftests（gate 10 前置义务分期项 9/10/11 钉）。

gate 3 整改重写（M2）：全部夹具 **producer-conformant**——逐处镜像所引证的
probe 源码行（d4.rs/d5.rs/d6.rs/d8.rs/dw.rs/ets），反例钉覆盖 B1-B7+M1：

- D4 夹具按 d4.rs 真实 marker 形态：``D4_READ|off=`` = 双偏移**可解析性**
  （net.rs offsets_parsable，非身份匹配）；身份匹配载体 = ``u3hex`` chunk
  （d4.rs:186-187 恰在首匹配帧发射）+ ``D4_END|u1=``（d4.rs:171）；外来包
  反例：``off=4`` 可解析而身份不匹配 → U1 绝不 true；
- D5 夹具含端口 src（``地址:端口``，d5.rs:83-91）与坏 payload 反例（RECV 在
  而 ``D5_END|u2=observed-false``，d5.rs:117-130 身份收口）；
- U5 夹具发真实 ``not_attempted``（ets:232-252）；join 夹具按修后十值域
  （不接纳 pending）；D8b 夹具测负值/域外/bogus 五值反例（d8.rs:145-165）。

一条命令运行（pytest 与自研 main 双兼容）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_platform.py
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_platform.py
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
# 夹具 helpers（producer-conformant；逐处标注所镜像的 probe 源码行）
# --------------------------------------------------------------------------

def events_of(*fakes):
    """FakeMarker 序列 → MarkerEvent 序列（scan 保序、line_no 按输入序）。"""
    return core.scan_markers([f.line() for f in fakes])


def chunk_map(**texts):
    """(stream) → 文本 的 chunk 重组结果（单记录形态）。"""
    return core.ChunkReassembly({(s, 0): t for s, t in texts.items()}, (), ())


#: 冻结身份包 Q（44 B，net.rs d4_packet 布局 / d4.rs:43-44 身份四元组）：
#: IPv4(ver4/IHL5, total=44, proto=17, src 10.99.0.1, dst **10.99.0.2**)
#: + UDP(sport 47001, **dport 47001**, len 24) + payload "N1DISCD4"|01|0001|5A×5。
_Q_HEX = (
    "4500002c" "00010000" "4011" "0000"
    "0a630001" "0a630002"
    "b799b799" "0018" "0000"
    "4e31444953434434" "01" "0001" "5a5a5a5a5a"
)
assert len(_Q_HEX) == 88
#: tun_pi 形态帧（48 B）＝ PI ``00 00 08 00`` + Q：镜像 fake._U3HEX_FRAME_HEX
#: （d4.rs:127/:549 tun_pi 前缀形态）。
FRAME_HEX = fake._U3HEX_FRAME_HEX
#: 其他 S5 行构造帧（:551-561 分区表逐行）：
_PI_OTHER = "deadbeef"                      # 行 3 反例：非 tun_pi 前缀
_PREFIX_PARSEABLE = "4500002c"              # 行 4：offset-0 亦可解析 → ambiguous
_FRAME_NO_PREFIX_HEX = _Q_HEX               # 行 2：offset-0 即身份包（readlen==total）
_FRAME_AMBIGUOUS_HEX = _PREFIX_PARSEABLE + _Q_HEX
_FRAME_OTHER_PREFIX_HEX = _PI_OTHER + _Q_HEX


def fm(short, at, kv=None, form="entry"):
    return fake.FakeMarker(short, at, kv or {}, form)


def d4_stream(ret="16", off="4", end_u1="observed-true", read_len="48"):
    """D4 marker 族（d4.rs:74/95/171 发射形态：SENT|n|ret|errno、READ|len|off、
    END|u1=三态）。``off`` 仅承载可解析性（net.rs offsets_parsable :179-189）。"""
    evs = [fm("D4_BEGIN", 100, {"mono_ms": "100"}),
           fm("D4_SENT", 110, {"n": "1", "ret": ret, "errno": "0"}),
           fm("D4_READ", 120, {"off": off, "len": read_len})]
    if end_u1 is not None:
        evs.append(fm("D4_END", 130, {"u1": end_u1}))
    return evs


# ==========================================================================
# A. 任务 9：u1-u7 平台分量派生（producer 语义基准）
# ==========================================================================

def test_u1_premise_identity_and_foreign_packet():
    # 正例：前提（ret==16）+ u3hex chunk（d4.rs:186-187 首匹配帧载体）→ true
    u1 = pf.derive_u1(events=events_of(*d4_stream()), retained_entry="MR1",
                      chunks=chunk_map(u3hex=FRAME_HEX))
    expect(u1["u1_socket_to_tun_delivery"] == "observed-true"
           and u1["u1_match_offset"] == "4"
           and u1["u1_no_route_control"] == "observed-true",
           "前提 + chunk 身份复核(offset-4) → true / offset=4 / MR* 对照镜像主字段")
    # B1 外来包反例：off=4 可解析但无 u3hex chunk（身份不匹配）+ END 收口 →
    # 绝不 observed-true（off 只是可解析性，d4.rs:94-95）
    u1f = pf.derive_u1(events=events_of(*d4_stream(end_u1="observed-false")),
                       retained_entry="MR1", chunks=chunk_map())
    expect(u1f["u1_socket_to_tun_delivery"] == "observed-false"
           and u1f["u1_match_offset"] == "none"
           and not u1f["f4_facets"] and not u1f["f8_facets"],
           "外来可解析包 + END=observed-false → false/none、零 facet")
    # 前提失败族（:540/:545-546；d4.rs:143-144 rets 语义）
    u1s = pf.derive_u1(events=events_of(
        fm("D4_SENT", 110, {"n": "1", "ret": "-1", "errno": "13"}),
        fm("D4_END", 130, {"u1": UNOBS("send-failed")})),
        retained_entry="MR1", chunks=chunk_map())
    expect(u1s["u1_socket_to_tun_delivery"] == UNOBS("send-failed"),
           "全部 -1 → send-failed")
    u1p = pf.derive_u1(events=events_of(
        fm("D4_SENT", 110, {"n": "1", "ret": "8", "errno": "0"}),
        fm("D4_SENT", 111, {"n": "2", "ret": "0", "errno": "0"}),
        fm("D4_END", 130, {"u1": UNOBS("short-or-zero-io")})),
        retained_entry="MR1", chunks=chunk_map())
    expect(u1p["u1_socket_to_tun_delivery"] == UNOBS("short-or-zero-io"),
           "有 0/部分、从无 n==16 → short-or-zero-io")
    # END 声称 true 而 chunk 缺 → F4 增量落盘缺项（:1128/:430-1）
    u1m = pf.derive_u1(events=events_of(*d4_stream()), retained_entry="MR1",
                       chunks=chunk_map())
    expect(u1m["u1_socket_to_tun_delivery"] == "observed-true"
           and any("u3hex chunk group missing" in f for f in u1m["f4_facets"]),
           "END=observed-true 而 chunk 缺 → 值成立 + F4 增量缺项")
    # END 缺（窗口未收口、非 skip）→ F4 terminal missing，不伪造 cause（B6）
    u1e = pf.derive_u1(events=events_of(*d4_stream(end_u1=None)),
                       retained_entry="MR1", chunks=chunk_map())
    expect(u1e["u1_socket_to_tun_delivery"] is None
           and any("D4 terminal missing" in f for f in u1e["f4_facets"]),
           "END 缺 → F4 面、值不落（不造 cause）")
    # ret 非整数字面 → F8 解析域（非 value-outside，M1 口径）
    u1o = pf.derive_u1(events=events_of(
        fm("D4_SENT", 110, {"n": "1", "ret": "abc", "errno": "0"}),
        fm("D4_END", 130, {"u1": UNOBS("short-or-zero-io")})),
        retained_entry="MR1", chunks=chunk_map())
    expect(any("D4_SENT ret unparsable" in f for f in u1o["f8_facets"]),
           "ret 非整数 → F8 解析域 facet")
    # skip 分支 → 整组同 cause（:993）
    u1k = pf.derive_u1(events=events_of(fm("SKIP", 100, {"item": "D4",
                                                          "cause": "no-live-fd"})),
                       retained_entry=None)
    expect(u1k["u1_socket_to_tun_delivery"] == UNOBS("no-live-fd")
           and u1k["u1_match_offset"] == UNOBS("no-live-fd"),
           "D4 skip → u1/u1_match_offset 同 cause 编码")


def test_u1_mb1_no_route_control():
    # MB1 冻结 dst=192.0.2.2（net.rs ADDR_MB1_PEER；d4.rs:43 dest=dst_peer(mb1)）
    mb1_frame = (
        "00000800"
        "4500002c" "00010000" "4011" "0000"
        "c0000201" "c0000202"      # src 192.0.2.1 → dst 192.0.2.2
        "b799b799" "0018" "0000"
        "4e31444953434434" "01" "0001" "5a5a5a5a5a"
    )
    evs = [fm("D4_BEGIN", 100), fm("D4_SENT", 110, {"n": "1", "ret": "16",
                                                    "errno": "0"}),
           fm("D4_END", 130, {"u1": UNOBS("mb1-no-route")})]
    # MB1 保留 → 主字段固定 mb1-no-route（:542；d4.rs:145-158 END 恒发主字段）
    u1 = pf.derive_u1(events=events_of(*evs), retained_entry="MB1",
                      chunks=chunk_map(u3hex=mb1_frame))
    expect(u1["u1_socket_to_tun_delivery"] == UNOBS("mb1-no-route")
           and u1["u1_no_route_control"] == "observed-true",
           "MB1 主字段固定 + 对照字段按身份复核门 → observed-true")
    # 对照字段零匹配（无 chunk）→ observed-false（:543-547 E9 表）
    u1f = pf.derive_u1(events=events_of(
        fm("D4_SENT", 110, {"n": "1", "ret": "16", "errno": "0"}),
        fm("D4_END", 130, {"u1": UNOBS("mb1-no-route")})),
        retained_entry="MB1", chunks=chunk_map())
    expect(u1f["u1_no_route_control"] == "observed-false",
           "MB1 对照字段零匹配 → observed-false")
    # MR* 帧对 MB1 不复核通过（dst 冻结差异）→ 身份 F8
    u1x = pf.derive_u1(events=events_of(*evs), retained_entry="MB1",
                       chunks=chunk_map(u3hex=FRAME_HEX))
    expect(any("fails frozen D4 identity" in f for f in u1x["f8_facets"]),
           "MR* 身份帧在 MB1 保留下不复核通过 → F8 身份 facet")


def test_u2_identity_via_end_and_bad_payload():
    # B2：身份判定载体 = D5_END|u2（d5.rs:117-130 E10 收口）；RECV src 含端口
    def d5(src="10.99.0.2:47001", ret="44", end_u2="observed-true"):
        evs = [fm("D5_BEGIN", 200),
               fm("D5_WRITE", 210, {"round": "1", "ret": ret, "errno": "0"})]
        if src is not None:
            evs.append(fm("D5_RECV", 220, {"round": "1", "src": src}))
        if end_u2 is not None:
            evs.append(fm("D5_END", 230, {"u2": end_u2}))
        return events_of(*evs)

    u2 = pf.derive_u2(events=d5(), retained_entry="MR1")
    expect(u2["u2_tun_write_to_sink_delivery"] == "observed-true"
           and u2["raw"]["recv_src_identity_form_matches"] == ["10.99.0.2:47001"],
           "前提 + END=observed-true（身份经 E10 收口）→ true、src 形态入 raw")
    # 坏 payload 反例：RECV src 形态相符而 END=observed-false（身份未证实，
    # d5.rs:95-106 payload 逐字节核对失败）→ 绝不折算 true
    u2b = pf.derive_u2(events=d5(end_u2="observed-false"), retained_entry="MR1")
    expect(u2b["u2_tun_write_to_sink_delivery"] == "observed-false"
           and u2b["raw"]["identity_unconfirmed_recvs"] is True,
           "RECV 在而身份未证实 → false + identity_unconfirmed 登记")
    # 无端口 src 字面 → F8 解析域（d5.rs:83-91 「地址:端口」形态）
    u2n = pf.derive_u2(events=d5(src="10.99.0.2"), retained_entry="MR1")
    expect(any("not addr:port literal" in f for f in u2n["f8_facets"]),
           "src 缺端口 → F8 形态 facet")
    # 地址符而端口≠47001 → 身份不成立、仅登记（d5.rs:95-96 端口合取）
    u2p = pf.derive_u2(events=d5(src="10.99.0.2:53", end_u2="observed-false"),
                       retained_entry="MR1")
    expect(u2p["u2_tun_write_to_sink_delivery"] == "observed-false"
           and u2p["raw"]["recv_src_ip_only_matches"] == ["10.99.0.2:53"],
           "端口≠47001 → ip_only 登记、值沿 END")
    # 前提失败族（:581）
    u2w = pf.derive_u2(events=d5(ret="-1", end_u2=UNOBS("write-failed")),
                       retained_entry="MR1")
    expect(u2w["u2_tun_write_to_sink_delivery"] == UNOBS("write-failed"),
           "全部写 -1 → write-failed")
    u2s = pf.derive_u2(events=d5(ret="20", end_u2=UNOBS("short-or-zero-io")),
                       retained_entry="MR1")
    expect(u2s["u2_tun_write_to_sink_delivery"] == UNOBS("short-or-zero-io"),
           "短写从无 44 → short-or-zero-io")
    # END 缺 → F4 terminal missing（B6：不造 cause）
    u2m = pf.derive_u2(events=d5(end_u2=None), retained_entry="MR1")
    expect(u2m["u2_tun_write_to_sink_delivery"] is None
           and any("D5 terminal missing" in f for f in u2m["f4_facets"]),
           "END 缺 → F4 面")
    # MB1 冻结 src=192.0.2.2:47001（net.rs ADDR_MB1_PEER；d5.rs:41）
    u2x = pf.derive_u2(events=d5(src="192.0.2.2:47001"), retained_entry="MB1")
    expect(u2x["u2_tun_write_to_sink_delivery"] == "observed-true",
           "MB1 冻结 src:port 匹配")
    u2k = pf.derive_u2(events=events_of(fm("SKIP", 100, {"item": "D5",
                                                          "cause": "dup-failed"})),
                       retained_entry=None)
    expect(u2k["u2_tun_write_to_sink_delivery"] == UNOBS("dup-failed"),
           "D5 skip → dup-failed 编码")


def test_u3_partition_table_and_decisive_offset():
    # 行 4（ambiguous）：offset-0 亦可解析（前缀 0x45）→ 无条件 ambiguous（:558）
    u3a = pf.derive_u3(events=events_of(*d4_stream(off="both")),
                       retained_entry="MR1",
                       chunks=chunk_map(u3hex=_FRAME_AMBIGUOUS_HEX))
    expect(u3a["u3_pi_header_present"] == "ambiguous"
           and u3a["u3_prefix_format"] == UNOBS("prefix-ambiguous")
           and not u3a["f8_facets"],
           "S5 行 4：两 offset 可解析 → ambiguous（身份复核经 offset-4 通过）")
    # off=both 决定性 offset=4（:563 E4；d4.rs:189 o4 优先）；off0 对照并记（:564）
    expect(u3a["raw"]["decisive_offset"] == 4
           and u3a["u3_readlen_vs_total_length"] == "readlen>total_length"
           and u3a["u3_readlen_vs_total_length_off0"] == "readlen>total_length",
           "决定性 offset=4（48>44）、off0 对照字段并记（off0 口径 total=44）")
    # 行 3 正例（tun_pi-like，d4.rs:127/:549）+ readlen 48>44 显式化
    u3t = pf.derive_u3(events=events_of(*d4_stream()), retained_entry="MR1",
                       chunks=chunk_map(u3hex=FRAME_HEX))
    expect(u3t["u3_pi_header_present"] == "tun_pi-like"
           and u3t["u3_prefix_format"] == "observed-true"
           and u3t["u3_first_read_len"] == 48
           and u3t["u3_readlen_vs_total_length"] == "readlen>total_length",
           "S5 行 3 tun_pi-like / observed-true / readlen 48>44")
    # 行 3 反例（other-prefix）：前 4 字节非 tun_pi → other-prefix + 原文逐字（:557）
    u3o = pf.derive_u3(events=events_of(*d4_stream()), retained_entry="MR1",
                       chunks=chunk_map(u3hex=_FRAME_OTHER_PREFIX_HEX))
    expect(u3o["u3_pi_header_present"] == "other-prefix"
           and u3o["u3_prefix_format"] == "observed-false"
           and u3o["raw"]["other_prefix_raw4"] == "deadbeef",
           "other-prefix → observed-false + 4 字节原文逐字")
    # 行 2（no-prefix）：offset-0 即身份包 → readlen==total_length（equal）
    u3n = pf.derive_u3(events=events_of(*d4_stream(off="0", read_len="44")),
                       retained_entry="MR1",
                       chunks=chunk_map(u3hex=_FRAME_NO_PREFIX_HEX))
    expect(u3n["u3_pi_header_present"] == "no-prefix"
           and u3n["u3_prefix_format"] == "observed-false"
           and u3n["u3_readlen_vs_total_length"] == "equal",
           "S5 行 2 → no-prefix / equal")
    # 行 1（unparsable）：分区表完备性钉（生产形态下匹配帧必在某 offset 可解析，
    # 此行构造帧专钉 runner 分区实现）
    u3u = pf.derive_u3(events=events_of(*d4_stream(off="none")),
                       retained_entry="MR1",
                       chunks=chunk_map(u3hex="0001020304050607"))
    expect(u3u["u3_pi_header_present"] == "unparsable"
           and u3u["u3_prefix_format"] == UNOBS("frame-unparsable")
           and u3u["u3_readlen_vs_total_length"] == "unparsable",
           "S5 行 1 → unparsable / frame-unparsable")
    # 帧恰 64 B（chunk 截断边界，d4.rs:128/:186 min(64,n)）→ read_len 不可判：
    # u3_first_read_len 不落值、readlen 比较落域内 unobservable（:562）
    u3z = pf.derive_u3(events=events_of(*d4_stream(off="0", read_len="64")),
                       retained_entry="MR1",
                       chunks=chunk_map(u3hex=_Q_HEX + "00" * 20))
    expect(u3z["u3_first_read_len"] is None
           and u3z["u3_readlen_vs_total_length"] == "unobservable"
           and u3z["u3_pi_header_present"] == "no-prefix",
           "64 B 截断帧 → read_len 不可判、readlen 字段 unobservable")
    # 零匹配 → 全部 no-controlled-read（:548/:567）；MB1 零匹配同口径
    u3m = pf.derive_u3(events=events_of(*d4_stream(end_u1="observed-false")),
                       retained_entry="MR1", chunks=chunk_map())
    expect(u3m["u3_pi_header_present"] == UNOBS("no-controlled-read")
           and u3m["u3_first_read_len"] == UNOBS("no-controlled-read"),
           "零匹配 → U3 全字段 no-controlled-read")
    u3k = pf.derive_u3(events=events_of(fm("SKIP", 100, {"item": "D4",
                                                          "cause": "no-live-fd"})),
                       retained_entry=None, chunks=chunk_map())
    expect(u3k["u3_prefix_format"] == UNOBS("no-live-fd"), "skip → 同 cause 编码")


def test_u4_seven_subitems_field_parsing():
    # B7：result marker 在且字段齐 → observed-true（:982「ret 已登记」）
    evs = events_of(fm("D6S1_B", 100), fm("D6S1_R", 110, {"ret": "0",
                                                          "errno": "0"}),
                    fm("D6S2_B", 120), fm("D6S2_R", 130, {"ret": "0",
                                                           "errno": "0"}),
                    fm("D6S7_B", 140), fm("D6S7_R", 150, {"fd": "12",
                                                           "reuse": "false"}))
    u4 = pf.derive_u4(events=evs, destroy_call_state="call-returned",
                      death_observed=False, post_present=True)
    expect(u4["subitems"]["u4_orig_getfd"] == "observed-true"
           and u4["subitems"]["u4_orig_getfl"] == "observed-true"
           and u4["raw"]["result_fields"]["u4_dup_fd_reuse"]
           == {"fd": 12, "reuse": False},
           "字段齐 → observed-true、ret/errno/fd/reuse 逐项入 raw")
    # B7 反例：result marker 在而 ret 缺 → F4、不得整包 observed-true（:977）
    u4m = pf.derive_u4(events=events_of(fm("D6S1_B", 100),
                                        fm("D6S1_R", 110, {"errno": "0"})),
                       destroy_call_state="call-returned",
                       death_observed=False, post_present=True)
    expect(u4m["subitems"]["u4_orig_getfd"] is None
           and any("ret missing" in f for f in u4m["f4_facets"]),
           "ret 缺 → F4、值不落")
    u4r = pf.derive_u4(events=events_of(fm("D6S7_B", 100),
                                        fm("D6S7_R", 110, {"fd": "12",
                                                           "reuse": "yes"})),
                       destroy_call_state="call-returned",
                       death_observed=False, post_present=True)
    expect(u4r["subitems"]["u4_dup_fd_reuse"] is None
           and any("reuse" in f for f in u4r["f4_facets"]),
           "reuse 域外字面 → F4 面")
    # complete ∧ 已调用未 resolve（flag-race 格，ets:446-465 destroy 盒到期跳 D6a）
    u4u = pf.derive_u4(events=events_of(
        fm("DW_DESTROY_T", 100, {"mono_ms": "100"}),
        fm("DW_DESTROY_C", 110, {"mono_ms": "110"})),
        destroy_call_state="call-returned", death_observed=False,
        post_present=True)
    expect(u4u["subitems"]["u4_orig_getfd"] == UNOBS("destroy-unresolved")
           and u4u["subitems"]["u4_orig_close"] == UNOBS("destroy-unresolved"),
           "complete∧C 在∧无 resolve 证据 → destroy-unresolved（V2(4) 同款）")
    # D6b 整段 skip → dup 四子项 d6b-skipped-join-timeout（:969）
    u4d = pf.derive_u4(events=events_of(
        fm("SKIP", 100, {"item": "D6b", "cause": "join-timeout-abandoned"}),
        fm("D6S1_B", 110), fm("D6S1_R", 120, {"ret": "0", "errno": "0"})),
        destroy_call_state="call-returned", death_observed=False,
        post_present=True)
    expect(u4d["subitems"]["u4_dup_getfd"] == UNOBS("d6b-skipped-join-timeout")
           and u4d["subitems"]["u4_orig_getfd"] == "observed-true",
           "D6b skip → dup 四子项新具名 cause、D6a 不受影响")
    # pre-only 四支（:1186-1193）
    u4a = pf.derive_u4(events=events_of(), destroy_call_state="not-reached",
                       death_observed=True, post_present=False)
    expect(u4a["subitems"]["u4_orig_getfd"] == UNOBS("destroy-not-reached"),
           "pre-only (1a) → destroy-not-reached")
    u4b = pf.derive_u4(events=events_of(
        fm("SKIP", 100, {"item": "destroy", "cause": "no-live-connection"})),
        destroy_call_state="not-called", death_observed=True, post_present=False)
    expect(u4b["subitems"]["u4_orig_close"] == UNOBS("destroy-not-called"),
           "pre-only (1b) → destroy-not-called")
    u4c = pf.derive_u4(events=events_of(
        fm("DW_DESTROY_T", 100, {"mono_ms": "100"})),
        destroy_call_state="call-boundary-incomplete",
        death_observed=True, post_present=False)
    expect(u4c["subitems"]["u4_dup_read"] == UNOBS("call-boundary-incomplete"),
           "pre-only (2) → call-boundary-incomplete")
    evs3 = events_of(fm("DW_DESTROY_T", 100, {"mono_ms": "100"}),
                     fm("DW_DESTROY_C", 110, {"mono_ms": "110"}),
                     fm("D6S1_B", 120), fm("D6S1_R", 130, {"ret": "0",
                                                            "errno": "0"}),
                     fm("D6S4_B", 140))
    u4r2 = pf.derive_u4(events=evs3, destroy_call_state="call-returned",
                        death_observed=True, post_present=False)
    expect(u4r2["subitems"]["u4_orig_getfd"] == "observed-true"
           and u4r2["subitems"]["u4_dup_getfd"] == "observed-false",
           "pre-only (3)：死亡前 result 保持、未执行子项 observed-false")
    u4u2 = pf.derive_u4(events=events_of(
        fm("DW_DESTROY_T", 100, {"mono_ms": "100"}),
        fm("DW_DESTROY_C", 110, {"mono_ms": "110"})),
        destroy_call_state="call-returned", death_observed=True,
        post_present=False)
    expect(u4u2["subitems"]["u4_orig_getfd"] == UNOBS("destroy-unresolved"),
           "pre-only (4) → destroy-unresolved")
    # D-W 整体 skip → 全部 no-live-fd（:999-1000）+ 摘要全序（:983-984）
    u4k = pf.derive_u4(events=events_of(), destroy_call_state="not-called",
                       death_observed=False, post_present=True,
                       dw_skip="no-live-fd")
    expect(all(v == UNOBS("no-live-fd") for v in u4k["subitems"].values())
           and u4k["u4_post_destroy_sync_observable"] == UNOBS("no-live-fd"),
           "D-W skip → 七子项全 no-live-fd、摘要 unobservable")
    evs5 = events_of(fm("D6S1_B", 100), fm("D6S1_R", 110, {"ret": "-1",
                                                           "errno": "9"}))
    u4s = pf.derive_u4(events=evs5, destroy_call_state="call-returned",
                       death_observed=True, post_present=True)
    expect(u4s["u4_post_destroy_sync_observable"] == "observed-true",
           "任一子项 observed-true → 摘要 observed-true")


def test_u5_not_attempted_matrix_outcomes():
    # B3：not_attempted 按矩阵终局分派（ets:232-252 真实发射形态）
    evs = events_of(
        fm("D2_ENTRY", 100, {"id": "MR1", "outcome": "resolved"}),
        fm("D2_ENTRY", 101, {"id": "MR1B", "outcome": "not_attempted"}),
        fm("D2_ENTRY", 102, {"id": "MR2", "outcome": "not_attempted"}),
        fm("D2_ENTRY", 103, {"id": "MR3", "outcome": "not_attempted"}),
        fm("D2_ENTRY", 104, {"id": "MB1", "outcome": "not_attempted"}))
    u5 = pf.derive_u5(events=evs)
    expect(u5["candidates"]["MR1"] == "observed-true"
           and u5["candidates"]["MR1B"] == UNOBS("protocol-first-accept-lock")
           and u5["candidates"]["MB1"] == UNOBS("protocol-first-accept-lock"),
           "first-accept-lock 后 not_attempted → protocol-first-accept-lock（:481）")
    # timeout 终止族（:474-475）：timeout → indeterminate（m-07 双行）→ 其余
    # not_attempted → matrix-terminated-on-create-timeout
    evs2 = events_of(
        fm("D2_ENTRY", 100, {"id": "MR1", "outcome": "timeout"}),
        fm("D2_ENTRY", 101, {"id": "MR1", "outcome": "indeterminate"}),
        fm("D2_ENTRY", 102, {"id": "MR1B", "outcome": "not_attempted"}),
        fm("D2_ENTRY", 103, {"id": "MB1", "outcome": "not_attempted"}))
    u5b = pf.derive_u5(events=evs2)
    expect(u5b["candidates"]["MR1"] == UNOBS("create-indeterminate")
           and u5b["candidates"]["MR1B"]
           == UNOBS("matrix-terminated-on-create-timeout")
           and u5b["candidates"]["MB1"]
           == UNOBS("matrix-terminated-on-create-timeout"),
           "timeout 终止后 not_attempted → matrix-terminated-on-create-timeout")
    # 常规结局映射（:479）；缺 outcome marker 的条目 → F4（B6：producer 恒发）
    evs3 = events_of(
        fm("D2_ENTRY", 100, {"id": "MR1", "outcome": "rejected"}),
        fm("D2_ENTRY", 101, {"id": "MR1B", "outcome": "late-rejected"}),
        fm("D2_ENTRY", 102, {"id": "MR2", "outcome": "late-resolved"}),
        fm("D2_ENTRY", 103, {"id": "MR3", "outcome": "not_attempted"}))
    u5c = pf.derive_u5(events=evs3)
    expect(u5c["candidates"]["MR1"] == "observed-false"
           and u5c["candidates"]["MR1B"] == "observed-false"
           and u5c["candidates"]["MR2"] == "observed-true"
           and u5c["candidates"]["MR3"] == UNOBS("protocol-first-accept-lock"),
           "rejected/late-rejected → false、late-resolved → true+lock")
    # outcome 字面域外 → F8（producer 分类缺陷，:511 冻结域）
    u5d = pf.derive_u5(events=events_of(
        fm("D2_ENTRY", 100, {"id": "MR1", "outcome": "bogus"})))
    expect(u5d["candidates"]["MR1"] is None
           and any("outside frozen domain" in f for f in u5d["f8_facets"]),
           "域外 outcome → F8、值不落")
    # outcome marker 全缺 → F4（B6：producer 恒发 outcome 行，ets:233/251）
    u5e = pf.derive_u5(events=events_of(fm("D1_BEGIN", 100)))
    expect(u5e["candidates"]["MR1"] is None
           and any("outcome marker missing" in f for f in u5e["f4_facets"]),
           "条目 marker 全缺 → F4 面")


def test_u6_s4_domain_and_gap_handling():
    # 派生表（:497-503；producer d2.rs:62-84 三字面域）
    u6 = pf.derive_u6(events=events_of(fm("D2_S4", 100,
                                           {"u6": "o_nonblock_present"})),
                      fd_roles_created=frozenset({"fd_orig", "fd_dup"}))
    expect(u6["u6_nonblocking_initial"] == "observed-true",
           "o_nonblock_present → observed-true（:499）")
    u6b = pf.derive_u6(events=events_of(fm("D2_S4", 100,
                                            {"u6": "o_nonblock_absent"})),
                       fd_roles_created=frozenset({"fd_orig", "fd_dup"}))
    expect(u6b["u6_nonblocking_initial"] == "observed-false",
           "o_nonblock_absent → observed-false（:500）")
    # 裸 unobservable（:492 域成员；d2.rs:62 F_GETFL 失败形态）：detail 逐字保真、
    # 主字段无预注册 cause → unregistered-cause facet（B6 → F4）
    u6u = pf.derive_u6(events=events_of(fm("D2_S4", 100, {"u6": "unobservable"})),
                       fd_roles_created=frozenset({"fd_orig", "fd_dup"}))
    expect(u6u["u6_initial_flags_and_isblocking_effect"] == "unobservable"
           and u6u["u6_nonblocking_initial"] is None
           and any(f.startswith("unregistered-cause:") for f in u6u["f4_facets"]),
           "裸 unobservable → detail 保真 + unregistered-cause facet")
    # 域外字面 → F8（producer 分类域，非平台原始值）
    u6o = pf.derive_u6(events=events_of(fm("D2_S4", 100, {"u6": "weird"})),
                       fd_roles_created=frozenset({"fd_orig", "fd_dup"}))
    expect(any("outside frozen literal set" in f for f in u6o["f8_facets"]),
           "S4 域外字面 → F8")
    # S4 缺 → 沿分支表（:501）：无 fd → no-live-ffd / fd_orig 在 → dup-failed
    u6n = pf.derive_u6(events=events_of(), fd_roles_created=frozenset())
    expect(u6n["u6_nonblocking_initial"] == UNOBS("no-live-fd"),
           "S4 缺 + 无 fd → no-live-fd")
    u6d = pf.derive_u6(events=events_of(),
                       fd_roles_created=frozenset({"fd_orig"}))
    expect(u6d["u6_nonblocking_initial"] == UNOBS("dup-failed"),
           "fd_orig 在 fd_dup 缺 → dup-failed")
    # S4 缺 ∧ 无 skip ∧ 进程活 → F4（B6：不造 cause）
    u6m = pf.derive_u6(events=events_of(),
                       fd_roles_created=frozenset({"fd_orig", "fd_dup"}))
    expect(u6m["u6_nonblocking_initial"] is None
           and any("D2_S4 marker missing" in f for f in u6m["f4_facets"]),
           "S4 缺无 skip → F4 面")


def test_u7_intervals_numerics_and_skip_first():
    def d7(elapsed=None, iters="400", end=True, begin=True, start="100"):
        evs = []
        if begin:
            evs.append(fm("D7_BEGIN", 100, {"start_mono_ms": start}))
        if end:
            kv = {"elapsed_ms": str(elapsed)}
            if iters is not None:
                kv["iters"] = iters
            evs.append(fm("D7_END", 31000, kv))
        return events_of(*evs)

    for e in (20000, 25000):
        expect(pf.derive_u7(events=d7(elapsed=e), death_observed=False,
                            last_site="P6",
                            tail_state="tail-complete"
                            )["u7_long_task_watchdog_behavior"] == "observed-true",
               "elapsed=%d ∈ [20000,25000] → observed-true" % e)
    u7o = pf.derive_u7(events=d7(elapsed=25001), death_observed=False,
                       last_site="P6", tail_state="tail-complete")
    expect(u7o["u7_long_task_watchdog_behavior"]
           == UNOBS("d7-elapsed-overshoot-beyond-grace")
           and not u7o["f8_facets"],
           "elapsed>25000 → overshoot 具名观察、无 F8")
    u7e = pf.derive_u7(events=d7(elapsed=19999), death_observed=False,
                       last_site="P6", tail_state="tail-complete")
    expect(u7e["u7_long_task_watchdog_behavior"] == UNOBS("d7-early-exit-anomaly")
           and any("d7-early-exit-contradiction" in f
                   for f in u7e["f8_facets"]),
           "elapsed<20000 → early-exit 标签 + F8（:647/:653）")
    # M1：负值/不可解析 → F8（单调钟域门/解析域），不归 F4 missing
    u7n = pf.derive_u7(events=d7(elapsed=-1), death_observed=False,
                       last_site="P6", tail_state="tail-complete")
    expect(any("monotonic-domain" in f for f in u7n["f8_facets"])
           and not any("elapsed_ms missing" in f for f in u7n["f4_facets"]),
           "elapsed<0 → F8 单调钟域、非 F4 missing")
    u7s = pf.derive_u7(events=d7(elapsed=20000, start="-3"),
                       death_observed=False, last_site="P6",
                       tail_state="tail-complete")
    expect(any("start_mono_ms" in f and "monotonic-domain" in f
               for f in u7s["f8_facets"]),
           "start_mono_ms<0 → F8 单调钟域（M1）")
    u7x = pf.derive_u7(events=d7(elapsed="abc"), death_observed=False,
                       last_site="P6", tail_state="tail-complete")
    expect(any("elapsed_ms" in f and "not an integer" in f
               for f in u7x["f8_facets"]),
           "elapsed 非整数字面 → F8 解析域（M1）")
    u7i = pf.derive_u7(events=d7(elapsed=20000, iters=None), death_observed=False,
                       last_site="P6", tail_state="tail-complete")
    expect(any("iters missing" in f for f in u7i["f4_facets"]),
           "iters 键缺 → F4")
    # END 缺、BEGIN 后静默 + 死亡 → observed-false（:655）；tail-loss 约束（:663-666）
    u7f = pf.derive_u7(events=events_of(
        fm("D7_BEGIN", 100, {"start_mono_ms": "100"})),
        death_observed=True, last_site="P6",
        tail_state="tail-loss-not-indicated")
    expect(u7f["u7_long_task_watchdog_behavior"] == "observed-false",
           "BEGIN 后静默 + 死亡 → observed-false")
    u7t = pf.derive_u7(events=events_of(
        fm("D7_BEGIN", 100, {"start_mono_ms": "100"})),
        death_observed=True, last_site="P6", tail_state="possible-tail-loss")
    expect(u7t["u7_long_task_watchdog_behavior"] == UNOBS("marker-tail-loss"),
           "位点 P6 + tail-loss → marker-tail-loss（宁缺勿误）")
    # 阶段未达（:658-661）与 skip 优先（:662）
    u7r = pf.derive_u7(events=events_of(fm("PRE", 100, {})),
                       death_observed=True, last_site="P5T",
                       tail_state="tail-loss-not-indicated")
    expect(u7r["u7_long_task_watchdog_behavior"]
           == UNOBS(death.STAGE_NOT_REACHED_CAUSE),
           "阶段未达 → stage-not-reached")
    u7k = pf.derive_u7(events=events_of(fm("SKIP", 100, {"item": "D7",
                                                          "cause": "no-live-vpn"})),
                       death_observed=True, last_site="P5T",
                       tail_state="tail-loss-not-indicated")
    expect(u7k["u7_long_task_watchdog_behavior"] == UNOBS("no-live-vpn"),
           "skip 分支优先 → no-live-vpn")
    # marker-gap-indeterminate 唯一授权位点（:657；B6 收窄核对）
    u7m = pf.derive_u7(events=events_of(), death_observed=False,
                       last_site="P6", tail_state="no-death-evidence")
    expect(u7m["u7_long_task_watchdog_behavior"]
           == UNOBS("marker-gap-indeterminate"),
           "marker 缺失无死亡证据 → marker-gap-indeterminate（:657 授权位）")


def test_platform_components_e2e_builtin_scenarios():
    for name in ("happy", "pre-only", "no-live-fd"):
        camp = run.run_dryrun_campaign(name)
        judged = run.derive_and_judge(camp)
        pc = judged["platform_components"]
        result = judged["verdict_result"]
        expect(result.verdict == "pass" and not result.gates,
               "%s：既有 verdict 不变（gates=%r）" % (name, result.gates))
        expect(not pc["f8_facets"] and not pc["f4_facets"],
               "%s：平台分量派生零 F8/F4 facet" % name)
        expect(pc["u1"]["u1_socket_to_tun_delivery"] is not None
               and pc["u7"]["u7_long_task_watchdog_behavior"] is not None,
               "%s：u1-u7 字段全部有值" % name)
        expect(judged["d6_items"]["f4"] == [] and judged["d6_items"]["f8"] == [],
               "%s：POST d6_items 七子项核对零 facet" % name)


# ==========================================================================
# B. 任务 10：dw_join_result 完整重建 + P12 比对（B4-a/B4-b）
# ==========================================================================

def j(**kw):
    defaults = dict(dw_skip_cause=None, destroy_call_state="not-reached",
                    d6b_skip_present=False, join_timeout_registered=False,
                    join_blocked_registered=False, exit_present=False,
                    exit_rc=None, post_present=True, death_observed=False)
    defaults.update(kw)
    return core.DwJoinInput(**defaults)


def test_join_ten_value_no_exit_inference():
    # B4-a：DW_EXIT 存在性不喂 join 轴（:693 明文）——EXIT 在而无 runner 侧
    # join 事实 → no-fact（不是 joined、也不是 fail）
    r = core.derive_dw_join_result(j(exit_present=True))
    expect(r.outcome == "no-fact" and r.value is None,
           "EXIT 在、无 join 事实 → no-fact（:693 EXIT 只喂 watchdog ④）")
    # runner 侧 pthread_join 返回登记（:717-721 终态轮询事实）→ joined
    r0 = core.derive_dw_join_result(j(exit_rc=0, exit_present=True))
    expect(r0.outcome == "value" and r0.value == "joined"
           and r0.value in core.DW_JOIN_RESULT_10,
           "exit_rc=0 登记 → joined（正当依据=轮询/join 返回事实）")
    expect(core.derive_dw_join_result(j(exit_rc=3)).value == "ESRCH",
           "join 返回 ESRCH → ESRCH")
    expect(core.derive_dw_join_result(j(exit_rc=22)).value == "other+errno",
           "join 返回其他 errno → other+errno")
    # skip 编码两值（:997-1000；B4 探针面修复后 dw.rs join_result_fallback 同值）
    for cause in ("no-live-fd", "dup-failed"):
        r2 = core.derive_dw_join_result(j(dw_skip_cause=cause, exit_present=True))
        expect(r2.value == UNOBS(cause), "D-W skip → %s 编码" % cause)
    # join sticky（:693）：D6b skip 在，迟到 EXIT 不得改写
    r3 = core.derive_dw_join_result(j(d6b_skip_present=True, exit_present=True))
    expect(r3.value == "join-timeout" and r3.sticky == "join",
           "D6b skip ∧ 迟到 EXIT → join-timeout（不得改写 joined）")
    r4 = core.derive_dw_join_result(j(join_timeout_registered=True,
                                       exit_present=True))
    expect(r4.value == "join-timeout", "JT 已登记 → join-timeout（同 sticky）")
    r5 = core.derive_dw_join_result(j(destroy_call_state="not-called",
                                       join_timeout_registered=True,
                                       post_present=False, death_observed=True))
    expect(r5.value == "join-timeout",
           "barrier-never-observed 顺延 → join-timeout（:1177）")
    # join-blocked-observed（runner 依轮询超时登记，:720/:1091）
    r6 = core.derive_dw_join_result(j(join_blocked_registered=True))
    expect(r6.value == "join-blocked-observed",
           "runner 登记 join 阻塞 → join-blocked-observed")
    # pre-only 死亡收口三 cause（:1171-1175）
    for state, cause in (("not-reached", "destroy-not-reached"),
                         ("call-returned", "post-destroy-unobservable"),
                         ("call-boundary-incomplete",
                          "call-boundary-incomplete")):
        r7 = core.derive_dw_join_result(j(destroy_call_state=state,
                                           post_present=False,
                                           death_observed=True))
        expect(r7.value == UNOBS(cause), "pre-only %s → %s" % (state, cause))
    r8 = core.derive_dw_join_result(j(destroy_call_state="not-called",
                                       post_present=False, death_observed=True))
    expect(r8.outcome == "fail" and "truth-table" in r8.fail_reason,
           "not-called 残余格 → fail（不伪装域内值）")


def test_join_p12_strict_domain():
    # B4-b：严格十值域——域内字面逐字比对
    expect(verdict.p12_join_consistency("joined", "joined"),
           "重建 joined = POST join=joined → 一致")
    expect(not verdict.p12_join_consistency("joined", "join-timeout"),
           "不一致 → False（F8(2) 面）")
    # pending 不再是合法形态（dw.rs 修复后 skip 形态逐字携带 skip 编码）
    expect(not verdict.p12_join_consistency(UNOBS("no-live-fd"), "pending"),
           "pending 域外 → 不一致（fail-closed）")
    expect(not verdict.p12_join_consistency("joined", "pending"),
           "pending + joined 重建 → 不一致")
    expect(not verdict.p12_join_consistency(None, "bogus"),
           "域外 POST 字面 / 重建缺席 → 不一致")
    expect(not verdict.p12_join_consistency("join-timeout", None),
           "POST join 缺字段 → 不一致")
    # F8(2) 挂载路径：不一致进入 parse_domain_gaps → verdict fail
    result = verdict.evaluate(verdict.VerdictInput(
        pre_present=True, post_present=True,
        parse_domain_gaps=["P12 dw_join_result='pending' != runner rebuild "
                           "'joined'"]))
    expect(result.verdict == "fail" and "F8" in result.gates,
           "join 比对不一致 → fail（F8(2)）")
    # unregistered_cause_hits → F4（B6 run.py 路由）
    result2 = verdict.evaluate(verdict.VerdictInput(
        pre_present=True, post_present=True,
        unregistered_cause_hits=["mb1-not-retained (removed literal)"]))
    expect(result2.verdict == "fail" and "F4" in result2.gates,
           "未注册 cause 命中 → fail（F4 (4)）")


def test_join_e2e_scenarios():
    # flag-race：D6b skip 在 → runner sticky 重建 join-timeout，与 POST 一致
    camp = run.run_dryrun_campaign("flag-race", fake.make_flag_race_scenario())
    judged = run.derive_and_judge(camp)
    dw = judged["dw"]
    expect(dw["rebuilt_join"] == "join-timeout" and dw["join_sticky"] == "join",
           "flag-race：join sticky 重建 join-timeout（D6b skip 载体）")
    expect(dw["post_join"] == "join-timeout", "POST 内层 join=join-timeout 一致")
    expect(judged["verdict_result"].verdict == "pass",
           "flag-race 合法 campaign → pass")
    # pre-only：死亡收口 → destroy-not-reached
    judged2 = run.derive_and_judge(run.run_dryrun_campaign("pre-only"))
    expect(judged2["dw"]["rebuilt_join"] == UNOBS("destroy-not-reached"),
           "pre-only 死于 D7 → dw_join_result=destroy-not-reached")
    # no-live-fd：skip 编码（B4 探针面修复后的 POST join 列 = skip 字面）
    judged3 = run.derive_and_judge(run.run_dryrun_campaign("no-live-fd"))
    expect(judged3["dw"]["rebuilt_join"] == UNOBS("no-live-fd")
           and judged3["dw"]["post_join"] == UNOBS("no-live-fd"),
           "no-live-fd：join 轴双侧 skip 编码一致（dw.rs join_result_fallback）")
    # happy：runner 侧 join rc 登记（B4-c 真实接线）→ joined 与 POST 一致
    judged4 = run.derive_and_judge(run.run_dryrun_campaign("happy"))
    expect(judged4["dw"]["rebuilt_join"] == "joined"
           and judged4["dw"]["post_join"] == "joined"
           and judged4["dw"]["join_facts"]["join_exit_rc"] == 0,
           "happy：join_exit_rc=0 登记 → joined、比对一致")


# ==========================================================================
# C. 任务 11：D8b 收口三分流 + 剧本格端到端（B5/M1/反例）
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
    # B5：墙钟缺失 → :607 强制 span 不得静默省略 → F4 面（criteria-gap (4)）
    out2 = death.eval_d8b_closure(begin_kv={"ws": "31100"}, end_kv=None,
                                  death_observed=True,
                                  death_site_before_storm=False,
                                  death_wall_ms=None,
                                  begin_capture_wall_ms=90_000)
    expect(out2["storm_incomplete_pre_only_span"] is None
           and any("storm_incomplete_pre_only_span" in f
                   for f in out2["f4_missing"]),
           "墙钟不可求值 → span 登记缺口走 F4 面（B5）")


def test_d8b_stage_not_reached_and_contrast():
    # 位点映射联动（:1228；SITE_ORDER 判定）
    def before(site):
        return fsm.SITE_ORDER.index(site) < fsm.SITE_ORDER.index("P7")

    expect(before("P5T") and before("P6") and not before("P7")
           and not before("P8"),
           "死亡位点 < P7 判定沿 SITE_ORDER（:1228）")
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
    out = death.eval_d8b_closure(begin_kv=None, end_kv=None,
                                 death_observed=True,
                                 death_site_before_storm=before("P6"))
    expect(out["branch"] == "stage-not-reached",
           "last_visible_site=P6（D7 内死亡）∧ 无 BEGIN → 阶段未达支")
    out2 = death.eval_d8b_closure(begin_kv={"ws": "31100"}, end_kv=None,
                                  death_observed=False,
                                  death_site_before_storm=False)
    expect(out2["branch"] == "alive-incomplete"
           and len(out2["f4_missing"]) == len(death.D8B_FIELDS),
           "无死亡证据 → alive-incomplete F4 面（:608/:615 对照）")
    out3 = death.eval_d8b_closure(begin_kv=None, end_kv=None,
                                  death_observed=False,
                                  death_site_before_storm=True)
    expect(out3["branch"] == "alive-incomplete"
           and out3["f4_missing"] == ["D8b.%s" % f for f in death.D8B_FIELDS],
           "无 BEGIN 无死亡 → 真未完成（字段缺项沿 F4 面）")


def test_d8b_field_domain_validation():
    # 正常重建（producer d8.rs:156-165 发射形态：三态两值 + u64 + caps 闭域）
    out = death.eval_d8b_closure(
        begin_kv={"ws": "31100"},
        end_kv={"we": "41100", "eagain": "observed-true",
                "partial": "observed-false", "bytes": "4194304",
                "calls": "40000", "caps_hit": "bytes,calls"},
        death_observed=False, death_site_before_storm=False)
    expect(out["branch"] == "rebuilt"
           and out["window_start_monotonic"] == 31100
           and out["window_end_monotonic"] == 41100
           and out["eagain_observed"] == "observed-true"
           and out["bytes_written_total"] == "4194304"
           and out["caps_hit"] == "bytes,calls"
           and not out["f8_facets"] and not out["f4_missing"],
           "正常重建：ws/we + 五字段域内逐字、零 facet")
    # B5 反例：eagain 域外三态 / bytes 非整数 / caps_hit 域外（旧夹具 "false"
    # 形态即反例——producer 只发 none/子集组合）→ 各自 F8
    out2 = death.eval_d8b_closure(
        begin_kv={"ws": "31100"},
        end_kv={"we": "41100", "eagain": "maybe", "partial": "observed-false",
                "bytes": "abc", "calls": "40000", "caps_hit": "false"},
        death_observed=False, death_site_before_storm=False)
    facets = "\n".join(out2["f8_facets"])
    expect("eagain='maybe' outside three-state domain" in facets,
           "eagain 域外 → F8 闭域验证")
    expect("bytes='abc' not a non-negative integer" in facets,
           "bytes 非整数 → F8")
    expect("caps_hit='false' outside closed domain" in facets,
           "caps_hit 域外（bool 形态）→ F8")
    # M1：we 负值 → F8 单调钟域门（不归 F4 missing）；ws 非整数 → F8 解析域
    out3 = death.eval_d8b_closure(
        begin_kv={"ws": "xyz"},
        end_kv={"we": "-5", "eagain": "observed-true",
                "partial": "observed-false", "bytes": "1", "calls": "1",
                "caps_hit": "none"},
        death_observed=False, death_site_before_storm=False)
    facets3 = "\n".join(out3["f8_facets"])
    expect("we=-5 < 0" in facets3 and "monotonic-domain" in facets3,
           "we<0 → F8 单调钟域（M1）")
    expect("ws='xyz' not an integer literal" in facets3,
           "ws 非整数 → F8 解析域（M1）")
    expect(not out3["f4_missing"], "负值/不可解析不归 F4 missing")
    # ws > we 顺序约束（:830）；END 载荷字段缺 → F4
    out4 = death.eval_d8b_closure(begin_kv={"ws": "500"},
                                  end_kv={"we": "400", "eagain": "observed-true",
                                          "partial": "observed-false",
                                          "bytes": "1", "calls": "1",
                                          "caps_hit": "none"},
                                  death_observed=False,
                                  death_site_before_storm=False)
    expect(any("window_start" in f for f in out4["f8_facets"]),
           "ws > we → F8 顺序约束")
    out5 = death.eval_d8b_closure(begin_kv={"ws": "500"}, end_kv={"we": "600"},
                                  death_observed=False,
                                  death_site_before_storm=False)
    expect(len(out5["f4_missing"]) == 5
           and all(m.startswith("D8_STORM_END.") for m in out5["f4_missing"]),
           "END 载荷字段缺 → F4 缺项面")
    # skip 表指派（:995）；未覆盖格字段留 None（B6：不伪造 cause）
    out6 = death.eval_d8b_closure(begin_kv=None, end_kv=None,
                                  death_observed=False,
                                  death_site_before_storm=False,
                                  skip_cause="no-live-fd")
    expect(out6["branch"] == "skip"
           and all(out6[f] == UNOBS("no-live-fd") for f in death.D8B_FIELDS),
           "D8b skip → 全字段 unobservable(no-live-fd)")
    out7 = death.eval_d8b_closure(begin_kv=None, end_kv=None,
                                  death_observed=True,
                                  death_site_before_storm=False)
    expect(out7["branch"] == "uncovered" and out7["f8_facets"]
           and all(out7[f] is None for f in death.D8B_FIELDS),
           "未覆盖格 → f8 facet、字段值留 None（不造 cause）")



def test_e2e_pre_only_death_closures():
    # stage-not-reached 剧本（r13 正例，:1461）
    camp = run.run_dryrun_campaign("death-after-pre",
                                   fake.make_death_after_pre_scenario())
    judged = run.derive_and_judge(camp)
    expect(judged["verdict_result"].verdict == "pass"
           and judged["verdict_result"].gates == [],
           "stage-not-reached 剧本 → pre-only pass")
    expect(camp["last_site"] == "P5T" and camp["death_observed"],
           "last_visible_site=P5T、死亡分量 observed-true")
    u7 = judged["platform_components"]["u7"]
    expect(u7["u7_long_task_watchdog_behavior"] == UNOBS("stage-not-reached"),
           "u7=unobservable(stage-not-reached)")
    d8b = judged["d8b_closure"]
    expect(d8b["branch"] == "stage-not-reached"
           and d8b["storm_incomplete_pre_only"] is False,
           "D8b 七字段 stage-not-reached、布尔 false")
    expect(judged["dw"]["rebuilt_join"] == UNOBS("destroy-not-reached"),
           "join 轴 pre-only 死亡收口 → destroy-not-reached")
    # storm BEGIN-only 剧本（:1456-1457）
    camp2 = run.run_dryrun_campaign("storm-death",
                                    fake.make_storm_death_scenario())
    judged2 = run.derive_and_judge(camp2)
    expect(judged2["verdict_result"].verdict == "pass",
           "storm-incomplete-pre-only → pass（合法平台死亡终态）")
    d8b2 = judged2["d8b_closure"]
    expect(d8b2["branch"] == "storm-incomplete-pre-only"
           and d8b2["storm_incomplete_pre_only"] is True
           and d8b2["storm_incomplete_pre_only_span"] == 10_000
           and d8b2["window_start_monotonic"] == 31100,
           "BEGIN-only：布尔 + 跨度 10000 + window_start 自 ws 重建")
    expect(judged2["platform_components"]["u7"]
           ["u7_long_task_watchdog_behavior"] == "observed-true",
           "D7 完成于死亡前 → u7=observed-true")


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
    expect(judged["platform_components"]["u7"]
           ["u7_long_task_watchdog_behavior"] == "observed-true",
           "D7 完成字段照常有值（存活未完成不抹事实）")
    # B4-a：EXIT 在而 runner 无 join 事实 → no-fact（不再由 EXIT 推 joined）
    expect(judged["dw"]["rebuilt_join"] is None
           and judged["dw"]["join_outcome"] == "no-fact",
           "alive-incomplete：EXIT 在 → join no-fact（:693 EXIT 不喂 join 轴）")


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


def test_e2e_gate3_counterexamples_pass():
    # B1 反例端到端：外来可解析包 → u1=observed-false（绝不 true）
    camp = run.run_dryrun_campaign("foreign-packet",
                                   fake.make_foreign_packet_scenario())
    judged = run.derive_and_judge(camp)
    pc = judged["platform_components"]
    expect(judged["verdict_result"].verdict == "pass"
           and not judged["verdict_result"].gates,
           "外来包剧本 → pass（平台负面事实）")
    expect(pc["u1"]["u1_socket_to_tun_delivery"] == "observed-false"
           and pc["u1"]["u1_match_offset"] == "none",
           "外来可解析 IPv4 包（off=4 + foreign chunk）→ u1=false/none")
    expect(pc["u3"]["u3_pi_header_present"]
           == UNOBS("no-controlled-read"),
           "零匹配 → u3 全字段 no-controlled-read")
    # B2 反例端到端：src 形态相符而 payload 身份未证实 → u2=observed-false
    camp2 = run.run_dryrun_campaign("bad-payload",
                                    fake.make_bad_payload_scenario())
    judged2 = run.derive_and_judge(camp2)
    pc2 = judged2["platform_components"]
    expect(judged2["verdict_result"].verdict == "pass",
           "坏 payload 剧本 → pass（身份未证实的 write 不折算 true）")
    expect(pc2["u2"]["u2_tun_write_to_sink_delivery"] == "observed-false"
           and pc2["u2"]["raw"]["identity_unconfirmed_recvs"] is True,
           "RECV src:port 相符而 END=false → u2=false + 未证实登记")
    # B3 反例端到端：timeout 终止族 not_attempted 分派
    camp3 = run.run_dryrun_campaign(
        "not-attempted-timeout", fake.make_not_attempted_timeout_scenario())
    judged3 = run.derive_and_judge(camp3)
    u5 = judged3["platform_components"]["u5"]
    expect(judged3["verdict_result"].verdict == "pass",
           "not-attempted-timeout 剧本 → pass")
    expect(u5["candidates"]["MR1"] == UNOBS("create-indeterminate")
           and u5["candidates"]["MR1B"]
           == UNOBS("matrix-terminated-on-create-timeout"),
           "timeout 终止族 → create-indeterminate / matrix-terminated 分派")


def test_e2e_gate3_counterexamples_fail_closed():
    # B4-b 反例端到端：POST join=pending（域外）→ F8(2) fail
    camp = run.run_dryrun_campaign("join-bogus",
                                   fake.make_join_bogus_scenario())
    judged = run.derive_and_judge(camp)
    result = judged["verdict_result"]
    expect(result.verdict == "fail" and "F8" in result.gates,
           "join=pending 域外 → fail（F8(2)）")
    expect(any("pending" in d for det in result.details.values()
               for d in det),
           "F8 明细携带 pending 不一致事实")
    # B5/M1 反例端到端：D8b 五值 bogus → F8 fail-closed
    camp2 = run.run_dryrun_campaign("d8b-bogus",
                                    fake.make_d8b_bogus_scenario())
    judged2 = run.derive_and_judge(camp2)
    result2 = judged2["verdict_result"]
    expect(result2.verdict == "fail" and result2.gates == ["F8"],
           "D8b bogus（eagain/bytes/we）→ fail（F8 逐字段）")
    facets = "\n".join(d for det in result2.details.values() for d in det)
    expect("eagain='maybe'" in facets and "bytes='abc'" in facets
           and "we=-5" in facets,
           "F8 明细逐字段携带三值 bogus 事实")


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
