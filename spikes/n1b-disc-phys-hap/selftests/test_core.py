#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc_core selftests（host-only，纯函数核心库四块）。

一条命令运行（pytest 与自研 main 双兼容）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_core.py     # 自研 main，全绿 exit 0
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_core.py

覆盖组：
  A. tag 三形态关联（含 comm 带空格/干扰行/不相关 pid）+ marker 解析
  B. chunk 重组全规则正反例（缺口/重复一致/重复不一致/count 与 sha256 字面不同…）
  C. fd ledger 六类场景（规格 :407 selftest 强制清单）+ canonical 序列化
  D. dw_return_class 真值表抽样+关键边界 + 20 值域枚举 + dw_join_result 10 值域
"""

from __future__ import annotations

import base64
import hashlib
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                os.pardir, "runner"))
import n1bdisc_core as core  # noqa: E402

BUNDLE = core.DEFAULT_BUNDLE
_ASSERTS = 0


def expect(cond, msg):
    """断言计数器（main 汇总每断言数；pytest 下等价 assert）。"""
    global _ASSERTS
    _ASSERTS += 1
    if not cond:
        raise AssertionError(msg)


# --------------------------------------------------------------------------
# hilog 行构造 helpers
# --------------------------------------------------------------------------

def hilog(tag_field, message, pid=12345, level="D",
          ts="08-30 10:00:00.000"):
    return "%s  %5d  %5d %s %s: %s" % (ts, pid, pid, level, tag_field, message)


def mk(tag_path, message, channel=core.HILOG_TAG, **kw):
    tag_field = "%s/%s" % (tag_path, channel) if channel else tag_path
    return hilog(tag_field, message, **kw)


def chunk(stream, item, index, count, raw, sha256=None, payload=None):
    return {
        "stream": stream, "item": str(item), "index": str(index),
        "count": str(count),
        "sha256": sha256 or hashlib.sha256(raw).hexdigest(),
        "payload": payload if payload is not None else base64.b64encode(raw).decode("ascii"),
    }


def fdm(role, inst, action, at, fd="none", by="none", cause="none", **kw):
    kv = {"fd": str(fd), "role": role, "inst": str(inst), "action": action,
          "at_mono_ms": str(at), "by": by, "cause": cause}
    kv.update(kw)
    return kv


def dw_input(**kw):
    defaults = dict(ret=1, revents=core.POLLIN, errno=None,
                    at_mono_ms=9000, elapsed_ms=100, drain_end="eagain",
                    has_skip_destroy=False, has_destroy_c=True,
                    t_mono_ms=5000, c_mono_ms=5100)
    defaults.update(kw)
    return core.DwReturnInput(**defaults)


def expect_class(result, want, msg=""):
    expect(result.outcome == "class" and result.value == want,
           "%s: expected class %r, got outcome=%r value=%r fail=%r detail=%r"
           % (msg, want, result.outcome, result.value, result.fail_reason,
              dict(result.detail)))


def expect_fail(result, reason, msg=""):
    expect(result.outcome == "fail" and result.fail_reason == reason,
           "%s: expected fail %r, got outcome=%r value=%r fail=%r"
           % (msg, reason, result.outcome, result.value, result.fail_reason))


# ==========================================================================
# A. tag 三形态关联 + marker 解析
# ==========================================================================

def test_tag_three_forms_positive():
    lines = [
        mk("cn.alfadb.netbird.n1bdisc", "N1BDISC_D1_BEGIN|mono_ms=100"),           # entry
        mk(".alfadb.netbird.n1bdisc:vpn", "N1BDISC_D1_LOADED|ok=true"),            # :vpn 截断（丢 cn.）
        mk("cn.alfadb.netbird.n1bdisc:vpn", "N1BDISC_D2_ENTRY|id=MR1"),            # :vpn 完整
        hilog("cn.alfadb.netbird.n1bdisc", "N1BDISC_D1_END|ok=true"),              # 无 /channel 通道形态
    ]
    events = core.scan_markers(lines)
    expect([e.name for e in events] ==
           ["N1BDISC_D1_BEGIN", "N1BDISC_D1_LOADED", "N1BDISC_D2_ENTRY", "N1BDISC_D1_END"],
           "three forms should all correlate: %r" % [e.name for e in events])
    expect([e.tag_form for e in events] == ["entry", "truncated", "complete", "entry"],
           "tag_form values: %r" % [e.tag_form for e in events])
    expect(events[1].line_no == 2 and events[1].raw == lines[1].rstrip("\n"),
           "line_no/raw must be carried through")
    expect(events[2].kv == {"id": "MR1"}, "kv parse: %r" % events[2].kv)


def test_tag_forms_literal_enumeration():
    forms = core.bundle_tag_forms(BUNDLE)
    expect(forms == ("cn.alfadb.netbird.n1bdisc",
                     ".alfadb.netbird.n1bdisc:vpn",
                     "cn.alfadb.netbird.n1bdisc:vpn"),
           "frozen three tag forms: %r" % (forms,))
    for path, want in zip(forms, ("entry", "truncated", "complete")):
        expect(core.tag_form_of(path, BUNDLE) == want, "form of %r" % path)
    expect(core.tag_form_of("cn.alfadb.netbird.n1bdisc:vpnx", BUNDLE) is None,
           "near-miss :vpnx must not correlate")
    expect(core.tag_form_of(".alfadb.netbird.n1bdisc", BUNDLE) is None,
           "truncated form without :vpn must not correlate")


def test_foreign_tag_and_body_noise_negative():
    lines = [
        # 外来 tag，消息体带 bundle 名与 marker 字面 → 不得关联（只扫 tag 路径组件）
        mk("com.example.other", "saw cn.alfadb.netbird.n1bdisc:vpn/N1BDiscVpn: N1BDISC_POST|ledger_digest=%s" % ("ab" * 32),
           channel="OtherTag"),
        # 外来 tag + 标准通道
        mk("com.example.other", "N1BDISC_D1_BEGIN|mono_ms=1"),
        # 无 ": " 分界的垃圾行
        "total garbage without separator 12345",
        # 空行
        "",
    ]
    expect(core.scan_markers(lines) == [], "foreign tags / garbage must not correlate")


def test_pid_not_filtered_and_comm_with_spaces():
    # 不按 pid 过滤：同一 tag 不同 pid 均关联
    lines = [
        mk("cn.alfadb.netbird.n1bdisc:vpn", "N1BDISC_DW_SPAWN|tid=42", pid=777),
        mk("cn.alfadb.netbird.n1bdisc:vpn", "N1BDISC_DW_EXIT|ok=true", pid=424242),
        mk("cn.alfadb.netbird.n1bdisc", "N1BDISC_D7_BEGIN|start_mono_ms=9", pid=1),
    ]
    events = core.scan_markers(lines)
    expect(len(events) == 3, "pid must not filter markers: %r" % [e.name for e in events])
    # comm 带空格：消息体与前置列含空格/括号/冒号的行不破坏 tag 定位与 kv 解析
    spaced = mk("cn.alfadb.netbird.n1bdisc:vpn",
                "N1BDISC_DW_INWAIT|src=proc-stat|conf=observed-true|samples=3|note=(comm with spaces) state S value: ok")
    events = core.scan_markers([spaced])
    expect(len(events) == 1 and events[0].name == "N1BDISC_DW_INWAIT",
           "spaced message must still correlate")
    expect(events[0].kv.get("note") == "(comm with spaces) state S value: ok",
           "kv value keeps spaces: %r" % events[0].kv)
    # 非 hilog 形态的 comm 带空格行（tag 位被普通词占据）不得关联
    decoy = "comm: netbird vpn worker (spaces here) D bogus-tag: N1BDISC_D1_BEGIN|mono_ms=1"
    expect(core.scan_markers([decoy]) == [], "comm-spaced decoy line must not correlate")


def test_marker_parse_malformed_and_unknown_literals():
    lines = [
        mk("cn.alfadb.netbird.n1bdisc", "N1BDISC_D2_ENTRY|attempted=true|outcome"),  # 段缺 =
        mk("cn.alfadb.netbird.n1bdisc", "N1BDISC_RESULT|legacy=1"),                  # 豁免集字面
        mk("cn.alfadb.netbird.n1bdisc", "N1BDISC_D2_REJTEXT|text=x"),                # 豁免集字面
        mk("cn.alfadb.netbird.n1bdisc", "N1BDISC_FUTURE_X|a=1"),                     # 未冻结字面
    ]
    events = core.scan_markers(lines)
    expect([e.known for e in events] == [True, False, False, False],
           "known flags: %r" % [e.known for e in events])
    expect(events[0].malformed_segments == ("outcome",),
           "malformed segment archived: %r" % (events[0].malformed_segments,))
    expect(len(core.FROZEN_MARKERS) == 56, "frozen marker set must be exactly 56")
    expect(core.EXEMPT_MARKERS == {"N1BDISC_D2_REJTEXT", "N1BDISC_RESULT"},
           "exempt set per spec :1100")
    expect("N1BDISC_D2_S1" in core.FROZEN_MARKERS and "N1BDISC_D6S7_R" in core.FROZEN_MARKERS
           and "N1BDISC_DW_RACEWIN" in core.FROZEN_MARKERS and "N1BDISC_POST" in core.FROZEN_MARKERS,
           "spot-check frozen literals")


def test_d1_end_payload_carries_elapsed_ms_after_b1():
    """gate 3 复审 B-1：D1_END payload 追加 `elapsed_ms` 后仍可经解析入口逐字捕获。"""
    message = "N1BDISC_D1_END|load=loaded|elapsed_ms=123"
    events = core.scan_markers([mk("cn.alfadb.netbird.n1bdisc", message)])
    expect(len(events) == 1 and events[0].name == "N1BDISC_D1_END",
           "marker name must stay N1BDISC_D1_END: %r" % [e.name for e in events])
    expect(events[0].kv.get("elapsed_ms") == "123",
           "elapsed_ms must be captured verbatim: %r" % events[0].kv)
    expect(events[0].kv.get("load") == "loaded",
           "existing load field must not regress: %r" % events[0].kv)
    name, kv, malformed, duplicate = core.parse_marker_message(message)
    expect(name == "N1BDISC_D1_END" and kv == {"load": "loaded", "elapsed_ms": "123"},
           "parse_marker_message on D1_END payload: %r" % (kv,))
    expect(malformed == () and duplicate == (),
           "no malformed/duplicate segments: %r %r" % (malformed, duplicate))


# ==========================================================================
# B. chunk 重组
# ==========================================================================

def test_chunk_roundtrip_single_and_multi_utf8_byteslice():
    single = core.encode_chunks("dlerror text: some symbol not found", "dlerror", 0)
    expect(len(single) == 1 and single[0]["item"] == "0", "dlerror single chunk item=0")
    res = core.reassemble_chunks(single)
    expect(res.ok, "single roundtrip failures: %r" % [f.reason for f in res.failures])
    expect(res.texts == {("dlerror", 0): "dlerror text: some symbol not found"},
           "roundtrip text: %r" % res.texts)

    long_text = "拒绝:" * 300  # 700+ 字节 UTF-8，多字节字符跨片
    pieces = core.encode_chunks(long_text, "rejtext", 2)   # MR2→2
    expect(len(pieces) >= 2, "multi chunk expected, got %d" % len(pieces))
    res = core.reassemble_chunks(pieces)
    expect(res.ok and res.texts == {("rejtext", 2): long_text},
           "utf-8 byteslice roundtrip: %r" % ([f.reason for f in res.failures],))


def test_chunk_item_mapping_and_256b_boundary():
    expect(core.REJTEXT_ITEM_IDS == {"MR1": 0, "MR1B": 1, "MR2": 2, "MR3": 3, "MB1": 4},
           "matrix item id mapping (spec :421)")
    for stream in ("dlerror", "u3hex", "foreign"):
        try:
            core.validate_chunk_stream_item(stream, 1)
            raise SystemExit("stream %s must require item=0" % stream)
        except ValueError:
            pass
        expect(len(core.encode_chunks("x", stream, 0)) == 1, "%s item=0 ok" % stream)
    exactly = core.encode_chunks("b" * 256, "u3hex", 0)
    expect(len(exactly) == 1, "256B slice stays single")
    over = core.encode_chunks("b" * 257, "u3hex", 0)
    expect(len(over) == 2, "257B splits into two")
    # 重组侧逐字节核对第二片长度
    raw = ("b" * 257).encode()
    joined = b"".join(base64.b64decode(p["payload"]) for p in over)
    expect(joined == raw, "byte-exact rejoin")


def test_chunk_gap_fail():
    pieces = core.encode_chunks("A" * 600, "rejtext", 0)   # count=3
    res = core.reassemble_chunks(pieces[:1] + pieces[2:])  # 丢 index=1
    expect(not res.ok and res.texts == {}, "gap must fail the group")
    failure = res.failures[0]
    expect(failure.reason == "coverage-gap" and failure.stream == "rejtext" and failure.item == 0,
           "gap reason: %r" % failure.reason)
    # raw 逐字入档：经 hilog 行扫描的 MarkerEvent 携带原始行（规格 :422/:431）
    lines = [mk("cn.alfadb.netbird.n1bdisc:vpn",
                "N1BDISC_CHUNK|stream=rejtext|item=0|index=%s|count=%s|sha256=%s|payload=%s"
                % (p["index"], p["count"], p["sha256"], p["payload"]))
             for p in (pieces[0], pieces[2])]
    res2 = core.reassemble_chunks(core.scan_markers(lines))
    expect(not res2.ok and res2.failures[0].reason == "coverage-gap",
           "scanned events reassemble: %r" % [f.reason for f in res2.failures])
    expect(len(res2.failures[0].raw_lines) == 2, "raw lines archived verbatim")


def test_chunk_duplicate_identical_observed():
    pieces = core.encode_chunks("hello", "foreign", 0)
    res = core.reassemble_chunks(pieces + [dict(pieces[0])])
    expect(res.ok and res.texts == {("foreign", 0): "hello"},
           "identical duplicate must not change verdict")
    expect(len(res.observations) == 1 and
           res.observations[0]["type"] == "duplicate_chunk_observed" and
           res.observations[0]["index"] == 0,
           "duplicate_chunk_observed registered verbatim: %r" % (res.observations,))


def test_chunk_duplicate_payload_mismatch():
    pieces = core.encode_chunks("hello", "foreign", 0)
    tampered = dict(pieces[0])
    tampered["payload"] = base64.b64encode(b"hellO").decode("ascii")  # 同长度不同字节
    res = core.reassemble_chunks(pieces + [tampered])
    expect(not res.ok and res.failures[0].reason == "duplicate-payload-mismatch",
           "byte-inequal duplicate must fail: %r" % [f.reason for f in res.failures])


def test_chunk_duplicate_count_literal_mismatch():
    pieces = core.encode_chunks("hello", "foreign", 0)
    tampered = dict(pieces[0], count="2")
    res = core.reassemble_chunks(pieces + [tampered])
    expect(not res.ok and res.failures[0].reason == "duplicate-count-mismatch",
           "same-index count literal mismatch must fail (spec :425)")


def test_chunk_duplicate_sha256_literal_mismatch():
    pieces = core.encode_chunks("hello", "foreign", 0)
    tampered = dict(pieces[0], sha256="0" * 64)
    res = core.reassemble_chunks(pieces + [tampered])
    expect(not res.ok and res.failures[0].reason == "duplicate-sha256-mismatch",
           "same-index sha256 literal mismatch must fail (spec :425)")


def test_chunk_sha256_mismatch():
    pieces = core.encode_chunks("hello", "foreign", 0)
    wrong = [dict(p, sha256=hashlib.sha256(b"other").hexdigest()) for p in pieces]
    res = core.reassemble_chunks(wrong)
    expect(not res.ok and res.failures[0].reason == "sha256-mismatch",
           "sha256 over original bytes must verify (spec :429)")


def test_chunk_base64_decode_fail():
    raw = b"payload"
    res = core.reassemble_chunks([chunk("foreign", 0, 0, 1, raw, payload="!!!not-base64!!")])
    expect(not res.ok and res.failures[0].reason == "base64-decode",
           "strict RFC4648 decode failure: %r" % [f.reason for f in res.failures])


def test_chunk_utf8_decode_fail():
    raw = b"\xff\xfe\x00broken"
    res = core.reassemble_chunks([chunk("u3hex", 0, 0, 1, raw)])
    expect(not res.ok and res.failures[0].reason == "utf8-decode",
           "utf8 step must fail on invalid bytes: %r" % [f.reason for f in res.failures])
    # sha256 正确但 base64 与 sha 不符 → 先 sha 后 utf8 的顺序不被跳过
    res2 = core.reassemble_chunks([chunk("u3hex", 0, 0, 1, b"abc", sha256="1" * 64)])
    expect(res2.failures[0].reason == "sha256-mismatch", "sha precedes utf8")


def test_chunk_item_domain_and_stream_domain_fail():
    res = core.reassemble_chunks([chunk("rejtext", 5, 0, 1, b"x")])   # 矩阵域 {0..4}
    expect(not res.ok and res.failures[0].reason == "item-domain", "rejtext item=5 rejected")
    res = core.reassemble_chunks([chunk("foreign", 1, 0, 1, b"x")])   # 恒 item=0
    expect(not res.ok and res.failures[0].reason == "item-domain", "foreign item=1 rejected")
    res = core.reassemble_chunks([chunk("bogus", 0, 0, 1, b"x")])
    expect(not res.ok and res.failures[0].reason == "stream-out-of-domain",
           "unknown stream rejected")


def test_chunk_interleaved_groups_no_crossmix():
    a = core.encode_chunks("A" * 300, "dlerror", 0)      # 2 片
    b = core.encode_chunks("B" * 300, "rejtext", 4)      # 2 片
    interleaved = [a[0], b[0], a[1], b[1]]               # 异步 marker 交错
    res = core.reassemble_chunks(interleaved)
    expect(res.ok, "interleave ok: %r" % [f.reason for f in res.failures])
    expect(res.texts == {("dlerror", 0): "A" * 300,
                         ("rejtext", 4): "B" * 300},
           "groups never mix: %r" % res.texts)


# ==========================================================================
# C. fd ledger 六类场景（规格 :407）+ canonical 序列化
# ==========================================================================

def test_ledger_dw_inwait_dual_instance_paired():
    transitions = [
        fdm("dw_inwait_proc_fd", 1, "create", 100, fd=21),
        fdm("dw_inwait_proc_fd", 2, "create", 105, fd=22),
        fdm("dw_inwait_proc_fd", 1, "close", 110, fd=21, by="probe-protocol-close"),
        fdm("dw_inwait_proc_fd", 2, "close", 115, fd=22, by="probe-protocol-close"),
    ]
    res = core.rebuild_fd_ledger(transitions, "complete")
    expect(res.ok and res.digest, "dual-instance pairing ok: %r"
           % [f.reason for f in res.failures])
    expect(res.rebuilt_from_transition_marker is False, "complete cut is not pre-only rebuild")
    expect(len(res.entries) == 2 and all(e.role_key.startswith("dw_inwait_proc_fd#")
                                         for e in res.entries), "role#k entries")
    canonical = ("dw_inwait_proc_fd#1|21|100|probe-protocol-close|110|none\n"
                 "dw_inwait_proc_fd#2|22|105|probe-protocol-close|115|none")
    expect(res.canonical_text == canonical, "canonical text: %r" % res.canonical_text)
    expect(res.digest == hashlib.sha256(canonical.encode("utf-8")).hexdigest(),
           "digest = sha256(canonical utf-8)")
    expect(res.digest == core.ledger_digest(res.entries), "ledger_digest helper agrees")


def test_ledger_close_before_create_fail():
    res = core.rebuild_fd_ledger(
        [fdm("d4_send_socket", 1, "close", 50, fd=6, by="probe-protocol-close")], "complete")
    expect(not res.ok and res.digest is None, "fail-closed: no digest on failure")
    expect(res.failures[0].reason == "close-before-create", "close-before-create")


def test_ledger_double_close_fail():
    transitions = [
        fdm("fd_dup", 1, "create", 10, fd=9),
        fdm("fd_dup", 1, "close", 20, fd=9, by="probe-protocol-close"),
        fdm("fd_dup", 1, "close", 30, fd=9, by="probe-protocol-close"),
    ]
    res = core.rebuild_fd_ledger(transitions, "complete")
    expect(not res.ok and res.failures[0].reason == "state-conflict",
           "close after closed → state-conflict (spec :406c)")


def test_ledger_create_after_close_also_fails():
    transitions = [
        fdm("fd_orig", 1, "create", 10, fd=7),
        fdm("fd_orig", 1, "close", 20, fd=7, by="destroy"),
        fdm("fd_orig", 1, "create", 30, fd=7),
    ]
    res = core.rebuild_fd_ledger(transitions, "complete")
    expect(not res.ok and res.failures[0].reason == "state-conflict",
           "any marker after closed → fail (spec :406c)")


def test_ledger_by_out_of_domain_fail():
    res = core.rebuild_fd_ledger(
        [fdm("fd_orig", 1, "create", 10, fd=7),
         fdm("fd_orig", 1, "close", 20, fd=7, by="kernel-gc")], "complete")
    expect(not res.ok and res.failures[0].reason == "by-out-of-domain",
           "close by= outside 7-value domain → fail (spec :396-397/:405)")
    # close by=none 也在七值域外
    res2 = core.rebuild_fd_ledger(
        [fdm("fd_orig", 1, "create", 10, fd=7),
         fdm("fd_orig", 1, "close", 20, fd=7)], "complete")
    expect(not res2.ok and res2.failures[0].reason == "by-out-of-domain",
           "close by=none is outside closed_by domain")
    # 七值域逐字可过域校验（合法收口字面）
    for by in core.CLOSED_BY_VALUES:
        one = core.rebuild_fd_ledger(
            [fdm("d2_late_fd", 1, "create", 1, fd=5),
             fdm("d2_late_fd", 1, "close", 2, fd=5, by=by)], "complete")
        expect(one.ok, "closed_by literal %r passes domain: %r"
               % (by, [f.reason for f in one.failures]))


def test_ledger_pre_only_rebuild_process_exit():
    transitions = [
        fdm("fd_orig", 1, "create", 10, fd=7),
        fdm("fd_dup", 1, "create", 12, fd=8),
        fdm("fd_dup", 1, "close", 14, fd=8, by="probe-protocol-close"),
    ]
    res = core.rebuild_fd_ledger(transitions, "pre-only")
    expect(res.ok and res.digest, "pre-only rebuild ok")
    expect(res.rebuilt_from_transition_marker is True,
           "pre-only rebuild annotated rebuilt-from-transition-marker (spec :387)")
    expect(res.entries[0].closed_by == "process-exit" and res.entries[0].closed_at_mono_ms is None,
           "open instance → process-exit/none (r6 W3)")
    canonical = ("fd_orig#1|7|10|process-exit|none|none\n"
                 "fd_dup#1|8|12|probe-protocol-close|14|none")
    expect(res.canonical_text == canonical, "pre-only canonical: %r" % res.canonical_text)
    expect(res.digest == hashlib.sha256(canonical.encode("utf-8")).hexdigest(),
           "pre-only digest matches canonical rule")


def test_ledger_forcestop_cleanup_host_forcestop():
    res = core.rebuild_fd_ledger(
        [fdm("fd_dup", 1, "create", 20, fd=8)], "host-forcestop")
    expect(res.ok, "forcestop cut ok")
    expect(res.entries[0].closed_by == "host-forcestop",
           "surviving fail-cleanup → host-forcestop (spec :402/:407)")
    expect(res.rebuilt_from_transition_marker is False,
           "annotation is pre-only-only per :387")
    # P5T 快照切点 → open-at-pre
    res2 = core.rebuild_fd_ledger(
        [fdm("d2_late_fd", 1, "create", 20, fd=9)], "pre-snapshot")
    expect(res2.entries[0].closed_by == "open-at-pre",
           "P5T snapshot → open-at-pre (r5 U5/:404)")
    # complete 切点 → open-at-exit
    res3 = core.rebuild_fd_ledger(
        [fdm("fd_dup", 1, "create", 20, fd=8)], "complete")
    expect(res3.entries[0].closed_by == "open-at-exit", "complete → open-at-exit")


def test_ledger_not_created_and_ordering():
    transitions = [
        fdm("d6b_reuse_probe_socket", 1, "not-created", 30, cause="no-live-fd"),
        fdm("fd_dup", 1, "create", 10, fd=8),
        fdm("d4_send_socket", 1, "create", 10, fd=6),        # 同刻 → 角色集序 fd_orig<fd_dup<d4...
        fdm("dw_inwait_proc_fd", 2, "create", 10, fd=22),    # 同刻同前 → inst 升序（inst=2 后于 inst=1）
        fdm("dw_inwait_proc_fd", 1, "create", 10, fd=21),
    ]
    res = core.rebuild_fd_ledger(transitions, "complete")
    expect(res.ok, "ordering fixture ok: %r" % [f.reason for f in res.failures])
    # 排序施于 canonical 行序（条目元组保持到达序，不承诺有序）
    expect([line.split("|")[0] for line in res.canonical_text.splitlines()] ==
           ["fd_dup#1", "d4_send_socket#1", "dw_inwait_proc_fd#1",
            "dw_inwait_proc_fd#2", "d6b_reuse_probe_socket#1"],
           "sort = created_at asc → role-set order → inst asc: %r" % res.canonical_text)
    last = res.entries[-1]
    expect(last.not_created and last.cause == "no-live-fd"
           and last.fd is None and last.created_at_mono_ms is None
           and last.closed_by == "none" and last.closed_at_mono_ms is None,
           "not-created canonical row fields (spec :380)")
    expect(res.canonical_text.endswith("d6b_reuse_probe_socket#1|none|none|none|none|no-live-fd"),
           "not-created row literal: %r" % res.canonical_text.splitlines()[-1])
    expect(not res.canonical_text.endswith("\n"), "no trailing newline (spec :386d)")


def test_ledger_malformed_fields_and_domain_gates():
    cases = [
        (fdm("fd_orig", 1, "create", 10), "fd-required"),                       # create fd 缺实际号
        (fdm("not_a_role", 1, "create", 10, fd=3), "role-out-of-domain"),
        (fdm("fd_orig", 1, "explode", 10, fd=3), "action-out-of-domain"),
        ({"fd": "3", "role": "fd_orig", "inst": "0", "action": "create",
          "at_mono_ms": "10", "by": "none", "cause": "none"}, "inst-domain"),
        ({"fd": "3", "role": "fd_orig#2", "inst": "1", "action": "create",
          "at_mono_ms": "10", "by": "none", "cause": "none"}, "role-inst-mismatch"),
        (fdm("fd_orig", 1, "create", -5, fd=3), "malformed-at-mono-ms"),        # BL-4 非负域门
        (fdm("d6b_reuse_probe_socket", 1, "not-created", 30), "not-created-missing-cause"),
        (fdm("d6b_reuse_probe_socket", 1, "not-created", 30, cause="none"), "not-created-missing-cause"),
        (fdm("d5_sink_socket", 1, "not-created", 30, fd=4, cause="x"), "fd-must-be-none"),
        (fdm("fd_orig", 1, "create", 10, fd=3, cause="oops"), "cause-expected-none"),
        ({"fd": "3", "role": "fd_orig", "action": "create", "at_mono_ms": "10",
          "by": "none", "cause": "none"}, "missing-field"),
    ]
    for transition, want in cases:
        res = core.rebuild_fd_ledger([transition], "complete")
        expect(not res.ok and res.failures[0].reason == want,
               "%r → %s, got %r" % (transition, want,
                                    [f.reason for f in res.failures]))
    # 规格 :830 顺序约束：created_at ≤ closed_at
    res = core.rebuild_fd_ledger(
        [fdm("fd_orig", 1, "create", 100, fd=7),
         fdm("fd_orig", 1, "close", 99, fd=7, by="destroy")], "complete")
    expect(not res.ok and res.failures[0].reason == "closed-before-created",
           "closed_at < created_at → fail (spec :830)")


# ==========================================================================
# D. dw_return_class + dw_join_result
# ==========================================================================

def test_dw_legality_gate_failures():
    expect_fail(core.derive_dw_return_class(dw_input(ret=2)),
                "ret-out-of-domain", "ret=2 (spec :812)")
    expect_fail(core.derive_dw_return_class(dw_input(ret=-2)),
                "ret-out-of-domain", "ret=-2")
    expect_fail(core.derive_dw_return_class(dw_input(ret=0, revents=core.POLLHUP)),
                "ret-zero-with-revents", "ret=0 with revents")
    expect_fail(core.derive_dw_return_class(dw_input(ret=1, revents=0)),
                "ret-one-empty-revents", "ret=1 empty revents")
    expect_fail(core.derive_dw_return_class(dw_input(ret=-1, errno=None)),
                "ret-minus-one-missing-errno", "ret=-1 without errno")
    # 合法域门先于一切：矛盾输入 + SKIP 同现仍 fail
    expect_fail(core.derive_dw_return_class(dw_input(ret=2, has_skip_destroy=True)),
                "ret-out-of-domain", "legality precedes class 0")
    # 合法域门通过的最简正例不 fail
    expect_class(core.derive_dw_return_class(dw_input(ret=0, revents=0, elapsed_ms=10)),
                 "spurious-early", "legal minimal input")


def test_dw_class0_skip_destroy():
    # 类 0 只认 SKIP|item=destroy 存在性，不要求 RETURN，先于未知位门（规格 :730-731）
    result = core.derive_dw_return_class(dw_input(ret=1, revents=64, has_skip_destroy=True))
    expect_class(result, "destroy-skip-proven", "skip wins over unknown bits")
    result = core.derive_dw_return_class(dw_input(has_skip_destroy=True, has_destroy_c=False))
    expect_class(result, "destroy-skip-proven", "skip needs neither _C nor revents sense")


def test_dw_class0b_c_missing():
    result = core.derive_dw_return_class(dw_input(has_destroy_c=False))
    expect_class(result, "destroy-call-unobserved", "no SKIP + _C missing → 0b")
    # r16 两钉：未知位 64 + 无 SKIP + _C 缺 → 0b（0/0b 前置先于未知位门）
    result = core.derive_dw_return_class(dw_input(ret=1, revents=64, has_destroy_c=False))
    expect_class(result, "destroy-call-unobserved", "unknown-bit 64 with _C missing → 0b")


def test_dw_unknown_bit_gate():
    # r16 两钉：64 + _C 在 → other-revents + 原值入档 + 不 fail
    result = core.derive_dw_return_class(dw_input(ret=1, revents=64))
    expect_class(result, "other-revents", "revents=64 with _C present")
    expect(result.detail.get("unknown_bit_revents") == 64, "raw value archived")
    # 混合位 72（64+POLLERR）即便满足行 7 全条件也不得 fd-event-like（规格 :817/:822）
    result = core.derive_dw_return_class(
        dw_input(ret=1, revents=72, elapsed_ms=1000, at_mono_ms=6000, drain_end="eagain"))
    expect_class(result, "other-revents", "mixed 72 never unlocks row 7")
    # 混合位 65（64+POLLIN）同理不得命中行 8
    result = core.derive_dw_return_class(
        dw_input(ret=1, revents=65, elapsed_ms=1000, at_mono_ms=6000, drain_end="eagain"))
    expect_class(result, "other-revents", "mixed 65 never unlocks row 8")
    expect(result.detail.get("unknown_bit_revents") == 65, "raw 65 archived")
    # 未知位门只施于 ret≥0：ret=-1 revents 无意义 → 行 1/2 收口（规格 :819）
    expect_class(core.derive_dw_return_class(dw_input(ret=-1, errno=4, revents=64)),
                 "interrupted", "unknown bits ignored for ret=-1")


def test_dw_rows_1_2_errno_priority():
    expect_class(core.derive_dw_return_class(dw_input(ret=-1, errno=4, revents=core.POLLHUP)),
                 "interrupted", "EINTR=4 → row 1 (revents meaningless)")
    result = core.derive_dw_return_class(dw_input(ret=-1, errno=11, revents=0))
    expect_class(result, "poll-error", "other errno → row 2")
    expect(result.detail.get("errno") == 11, "errno archived verbatim")
    # 行 1/2 先于一切 revents 判定（规格 :810）
    expect_class(core.derive_dw_return_class(
        dw_input(ret=-1, errno=12, revents=core.POLLNVAL, at_mono_ms=100)),
        "poll-error", "rows 1-2 precede POLLNVAL")


def test_dw_row3_nval_precedence():
    expect_class(core.derive_dw_return_class(dw_input(revents=core.POLLNVAL)),
                 "fd-invalid", "POLLNVAL → row 3")
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLNVAL | core.POLLIN, at_mono_ms=100)),  # at < T
        "fd-invalid", "POLLNVAL precedes pre-destroy-ready (spec :798/:810)")


def test_dw_row4_pre_destroy_band():
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLIN, at_mono_ms=4999)),   # < T=5000
        "pre-destroy-ready", "at < T strict")
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLHUP | core.POLLERR, at_mono_ms=4999,
                 elapsed_ms=4600, drain_end="zero-read")),
        "pre-destroy-ready", "row 4 precedes rows 5-8, whatever drain")
    # at == T → 闭区间歧义带，行 4 不命中
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLIN, at_mono_ms=5000, elapsed_ms=100)),
        "other-revents", "at == T is ambiguous band, not pre")


def test_dw_rows_5_6_late():
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLHUP, elapsed_ms=4500, at_mono_ms=5101)),
        "late-fd-event", "HUP elapsed>=4500 at>C")
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLERR, elapsed_ms=6000, at_mono_ms=5200,
                 drain_end="zero-read")),
        "late-fd-event", "row 5 needs no drain condition")
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLIN, elapsed_ms=4500, at_mono_ms=5000)),  # 歧义带也行
        "late-data", "row 6 has no band condition (spec :801)")


def test_dw_row7_fd_event_like_conjunction():
    base = dict(revents=core.POLLHUP, elapsed_ms=4499, at_mono_ms=5101, drain_end="eagain")
    expect_class(core.derive_dw_return_class(dw_input(**base)),
                 "fd-event-like", "all four conjuncts hold")
    # 缺一反例 ×4（规格 :802 四前件合取）
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLIN, elapsed_ms=4499, at_mono_ms=5101, drain_end="eagain")),
        "data-ready-post-destroy", "missing HUP/ERR (conjunct 1) → row 8 for POLLIN")
    expect_class(core.derive_dw_return_class(dw_input(**dict(base, elapsed_ms=4500))),
                 "late-fd-event", "elapsed>=4500 (conjunct 2 broken) → row 5")
    expect_class(core.derive_dw_return_class(dw_input(**dict(base, at_mono_ms=5100))),
                 "other-revents", "at==C not strictly post (conjunct 3) → row 11")
    expect_class(core.derive_dw_return_class(dw_input(**dict(base, drain_end="zero-read"))),
                 "other-revents", "drain!=eagain (conjunct 4) → row 11")
    # at > C 锚不可得（c_mono_ms=None）时行 5/7/8 均不吸附 → 兜底行 11
    # （规格 :799-802 "「_C」缺均不满足本行"的派生面；marker 级 _C 缺失由 0/0b 前置收口）
    expect_class(core.derive_dw_return_class(dw_input(**base, c_mono_ms=None)),
                 "other-revents", "no C anchor → post-band rows unsatisfied")


def test_dw_row8_data_ready_post_destroy():
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLIN, elapsed_ms=4499, at_mono_ms=5101)),
        "data-ready-post-destroy", "POLLIN post-destroy eagain")
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLIN | core.POLLOUT, elapsed_ms=4499, at_mono_ms=5101)),
        "data-ready-post-destroy", "POLLOUT does not exclude row 8 (no HUP/ERR)")
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLIN | core.POLLHUP, elapsed_ms=4499, at_mono_ms=5101)),
        "fd-event-like", "HUP present → row 7 precedes row 8")
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLIN, elapsed_ms=4499, at_mono_ms=5101,
                 drain_end="box-expiry")),
        "other-revents", "row 8 also requires drain==eagain")


def test_dw_rows_9_10_timeout_boundary():
    expect_class(core.derive_dw_return_class(dw_input(ret=0, revents=0, elapsed_ms=4500)),
                 "timeout-like", "empty revents elapsed>=4500")
    expect_class(core.derive_dw_return_class(dw_input(ret=0, revents=0, elapsed_ms=4499)),
                 "spurious-early", "empty revents elapsed<4500")
    expect_class(core.derive_dw_return_class(dw_input(ret=0, revents=0, elapsed_ms=4501)),
                 "timeout-like", "4501 timeout side")


def test_dw_row11_other_and_ambiguous_band():
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLPRI, elapsed_ms=100, at_mono_ms=6000)),
        "other-revents", "POLLPRI-only falls to exhaustive row 11")
    # 三分带闭区间 [T, C] 全歧义：HUP+eagain 在带内不归因（规格 :799-802）
    for at in (5000, 5050, 5100):
        expect_class(core.derive_dw_return_class(
            dw_input(revents=core.POLLHUP, elapsed_ms=4400, at_mono_ms=at)),
            "other-revents", "ambiguous band at=%d never attributes" % at)
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLIN | core.POLLERR, elapsed_ms=200, at_mono_ms=5050)),
        "other-revents", "mask-internal combo unmatched → row 11")


def test_dw_elapsed_boundaries_4499_4500_4501():
    cases = {
        4499: ("POLLIN", "data-ready-post-destroy"),
        4500: ("POLLIN", "late-data"),
        4501: ("POLLIN", "late-data"),
    }
    for elapsed, (bit, want) in cases.items():
        revents = getattr(core, bit)
        expect_class(core.derive_dw_return_class(
            dw_input(revents=revents, elapsed_ms=elapsed, at_mono_ms=5200)),
            want, "POLLIN elapsed=%d" % elapsed)
    # HUP 侧 4499/4500 边界（行 7/5 交界）
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLHUP, elapsed_ms=4499, at_mono_ms=5200)),
        "fd-event-like", "HUP elapsed=4499 → row 7")
    expect_class(core.derive_dw_return_class(
        dw_input(revents=core.POLLHUP, elapsed_ms=4500, at_mono_ms=5200)),
        "late-fd-event", "HUP elapsed=4500 → row 5")


def test_dw_negative_monotonic_gate():
    expect_fail(core.derive_dw_return_class(dw_input(at_mono_ms=-1)),
                "negative-monotonic", "BL-4 at_mono_ms >= 0 (spec :829)")
    expect_fail(core.derive_dw_return_class(dw_input(elapsed_ms=-1)),
                "negative-monotonic", "BL-4 elapsed_ms >= 0")
    expect_fail(core.derive_dw_return_class(dw_input(t_mono_ms=-3)),
                "negative-monotonic", "BL-4 T_mono_ms >= 0")


def test_dw_domain_20_values_enumeration():
    expect(len(core.DW_RETURN_CLASS_13) == 13, "13 normal classes")
    expect(len(core.DW_UNOBSERVABLE_CAUSES_CLASS) == 7, "7 unobservable causes for class")
    expect(len(core.DW_RETURN_CLASS_20) == 20, "dw_return_class = 20 values (spec :866)")
    expect(len(set(core.DW_RETURN_CLASS_20)) == 20, "20 values pairwise distinct")
    expect(set(core.DW_RETURN_CLASS_13) <= set(core.DW_RETURN_CLASS_20),
           "13 ⊂ 20")
    for cause in core.DW_UNOBSERVABLE_CAUSES_CLASS:
        expect(unq := core.unobservable_value(cause) in core.DW_RETURN_CLASS_20,
               "encoded %r in domain" % cause)
    expect(core.unobservable_value("poll-never-returned") == "unobservable(cause=poll-never-returned)",
           "(d) 支具名 cause 字面")
    expect(core.unobservable_value("flag-race-window-expired") == "unobservable(cause=flag-race-window-expired)",
           "r20 收口字面")
    # 四步派生的落点全部 ∈ 20 值域（对若干代表性输入抽查）
    samples = ["destroy-skip-proven", "destroy-call-unobserved", "interrupted",
               "poll-error", "fd-invalid", "pre-destroy-ready", "late-fd-event",
               "late-data", "fd-event-like", "data-ready-post-destroy",
               "timeout-like", "spurious-early", "other-revents"]
    expect(set(samples) == set(core.DW_RETURN_CLASS_13),
           "derived classes exactly cover the 13-class table")


def test_dw_join_result_10_values_and_mapping():
    expect(len(core.DW_JOIN_RESULT_10) == 10 and len(set(core.DW_JOIN_RESULT_10)) == 10,
           "dw_join_result = 10 values (spec :870)")
    expect(core.DW_JOIN_RESULT_10[:5] ==
           ("joined", "join-timeout", "join-blocked-observed", "ESRCH", "other+errno"),
           "terminal 5 values order/literals")
    # r17 核对结论：join 域不扩 poll-never-returned / flag-race-window-expired
    expect(not {"poll-never-returned", "flag-race-window-expired"} &
           set(core.DW_UNOBSERVABLE_CAUSES_JOIN), "join domain unchanged (spec :746)")
    expect(core.map_dw_join_result("joined") == "joined", "map joined")
    expect(core.map_dw_join_result("join-timeout") == "join-timeout", "map join-timeout")
    expect(core.map_dw_join_result("join-blocked-observed") == "join-blocked-observed",
           "map join-blocked-observed")
    expect(core.map_dw_join_result("ESRCH") == "ESRCH", "map ESRCH")
    expect(core.map_dw_join_result(("other", 11)) == "other+errno", "map other+errno")
    expect(core.map_dw_join_result("no-live-fd") == "unobservable(cause=no-live-fd)",
           "map skip cause")
    for cause in core.DW_UNOBSERVABLE_CAUSES_JOIN:
        expect(core.map_dw_join_result(cause) in core.DW_JOIN_RESULT_10,
               "cause %r maps into 10-value domain" % cause)
    for bad in (("other",), ("weird", 1), "joined-now", 42, None):
        try:
            core.map_dw_join_result(bad)
            raise SystemExit("map_dw_join_result(%r) must raise" % (bad,))
        except ValueError:
            pass


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
    print("n1bdisc_core selftests: tests=%d passed=%d failed=%d assertions=%d (%.0f ms)"
          % (len(tests), passed, failed, _ASSERTS, elapsed * 1000))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
