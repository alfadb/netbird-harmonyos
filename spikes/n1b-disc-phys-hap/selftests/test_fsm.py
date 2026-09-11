#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc FSM/死亡事实/verdict selftests（host-only，本增量六模块）。

一条命令运行（pytest 与自研 main 双兼容，与 test_core 并存且互不干扰）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_fsm.py      # 自研 main，全绿 exit 0
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_fsm.py

覆盖组（规格 :1446-1610 gate 10 清单中本增量相关项）：
  T. 时间盒常量与观测窗冻结值（:1031-1049/:1056-1065 逐项一致断言）
  W. 观测窗状态机四分支 / late_marker / Allow 已消费收口 / 位次约束
  ⑥ fault 解析契约正反例（归一化/JSCRASH 域外不 fail/SIGKILL 对照/多条目聚合/
     混合可解析/FaultRecv 部分失败/raw 谓词/Fault_Type 空值不可解析（M1）/
     空 Signal 其余段回归钉）
  ⑧ fake-HDC 沙箱（违规 argv 拒绝/合法白名单全操作可过/合成流三形态+chunk 注入）
  ⑨ PRE/POST 通道（complete/pre-only、SKIP 同现域门、全拒 POST 照发 pass、
     barrier-never-observed、ledger 同切点、P12 派生比对 F8(2) 钉、flag-race
     complete+cut=false 同切点 digest 一致、dw_outcome 探针格式 fail-closed 钉、
     最终重建切点 close_kind 驱动钉）
  ⑩ 七分量 + F1 三支闭集真值表
  ⑪ watchdog ①-⑤ 全支正反例 + marker_tail 边界 25000/25001
  ⑫ 审查整改钉（M-03 开流位次 / m-07 D2_ENTRY 后到者为准 / m-10 FaultRecv
     路径穿越拦截正反例 + 部分失败支 / m-11 825 s 墙钟兜底接线）
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                os.pardir, "runner"))
import n1bdisc_core as core        # noqa: E402
import n1bdisc_fsm as fsm          # noqa: E402
import n1bdisc_hdc as hdc          # noqa: E402
import n1bdisc_death as death      # noqa: E402
import n1bdisc_verdict as verdict  # noqa: E402
import fake_hdc as fake            # noqa: E402
import n1bdisc_run as run          # noqa: E402

_ASSERTS = 0


def expect(cond, msg):
    """断言计数器（main 汇总每断言数；pytest 下等价 assert）。"""
    global _ASSERTS
    _ASSERTS += 1
    if not cond:
        raise AssertionError(msg)


def entry(ft, sig, ts="2026-09-06 10:00:00.000"):
    """合成 fault 条目文本（None 字段 = 字段行缺失）。"""
    lines = []
    if ft is not None:
        lines.append("Fault_Type: %s" % ft)
    if sig is not None:
        lines.append("Signal: %s" % sig)
    if ts is not None:
        lines.append(ts)
    lines.append("Module: %s" % core.DEFAULT_BUNDLE)
    return "\n".join(lines)


def parse(ft, sig, name="a.txt", ts="2026-09-06 10:00:00.000"):
    return death.parse_fault_entry(name, entry(ft, sig, ts))


def agg(parses, failed=(), probe_failed=False, site=None, seq_ok=True):
    return death.aggregate_fault_components(parses, failed_files=failed,
                                            fault_probe_failed=probe_failed,
                                            last_visible_site=site,
                                            marker_seq_ok=seq_ok)


def unobs(cause):
    return core.unobservable_value(cause)


def watchdog(**kw):
    defaults = dict(skip_cause=None, c_present=False, t_present=False,
                    pidof_absent=False, exit_present=False, spawn_present=True,
                    capture_silent_after_spawn=True, death_observed=True,
                    inwait_confirmed="observed-true", death_site="P8")
    defaults.update(kw)
    return verdict.eval_watchdog_killed(**defaults)


def judge(scenario_name):
    """in-process DryRun：campaign 组装 + 解析求值（gate 11-13 形态、无 selftest 步）。"""
    camp = run.run_dryrun_campaign(scenario_name)
    judged = run.derive_and_judge(camp)
    return camp, judged


# ==========================================================================
# T. 时间盒常量与观测窗冻结值（:1031-1049/:1056-1065）
# ==========================================================================

def test_time_box_table_matches_spec():
    # 规格 :1033-1049 逐行字面值
    expect(fsm.ALLOW_BOX_S == 300, "P0 Allow 盒 300 s（:1033）")
    expect(fsm.P1_DLOPEN_BOX_S == 10, "P1 dlopen+dlsym 10 s（:1034）")
    expect(fsm.P2_CREATE_BOX_S == 60, "P2 每条 create 60 s（:1035）")
    expect(fsm.P2_LATE_OBSERVATION_WINDOW_S == 60, "P2 迟到观察窗 60 s 恰一次（:1036）")
    expect((fsm.P3_D4_WINDOW_TOTAL_S, fsm.P3_D4_POLL_MS) == (10, 500),
           "P3 D4 总 10 s / 每 poll 500 ms（:1037）")
    expect((fsm.P4_D5_WINDOW_TOTAL_S, fsm.P4_D5_ROUND_MS, fsm.P4_D5_MAX_ROUNDS)
           == (10, 500, 5), "P4 D5 总 10 s / 每轮 500 ms / ≤5 轮（:1038）")
    expect((fsm.P5_D8A_PER_LEVEL_POLL_MS, fsm.P5_D8A_PER_LEVEL_MAX_S,
            fsm.P5_D8A_TOTAL_S) == (500, 1, 10),
           "P5 D8a 每级 poll 500 ms + write ≤1 s/级、总 ≤10 s（:1039）")
    expect(fsm.P5T_PRE_EMIT_S == 0, "P5T PRE 即返（:1040）")
    expect((fsm.P6_D7_TASK_S, fsm.P6_D7_GRACE_S) == (20, 5),
           "P6 D7 20 s + 5 s 宽限（:1041）")
    expect((fsm.P7_D8B_STORM_S, fsm.P7_D8B_STORM_BYTES, fsm.P7_D8B_STORM_MAX_WRITES)
           == (10, 4 * 1024 * 1024, 50_000),
           "P7 storm 10 s / 4 MiB / 50 000 写（:1042）")
    expect(fsm.P8_DRAIN_BOX_S == 5, "P8 drain 5 s（:1043）")
    expect(fsm.P8_BARRIER_BOX_S == 7, "P8 barrier 7 s = 5 + 调度裕量 2（:1044）")
    expect((fsm.P8_INWAIT_SAMPLE_S, fsm.P8_INWAIT_SAMPLE_INTERVAL_MS) == (2, 10),
           "P8 in-wait 2 s / 10 ms 间隔（:1045）")
    expect(fsm.P9_DESTROY_BOX_S == 10, "P9 destroy 10 s（:1046）")
    expect((fsm.P10_TERMINAL_POLL_BOX_S, fsm.P10_POLL_INTERVAL_MS) == (8, 10),
           "P10 终态轮询 8 s / 10 ms 间隔（:1047）")
    expect(fsm.P12_RACE_WINDOW_MS == 1000, "P12 迟到竞态窗 1000 ms（:1048）")
    expect(fsm.D6_STEP_IMMEDIATE is True, "D6 各步即返 syscall（:1049）")
    expect(fsm.JOIN_UNBOUNDED is True, "pthread_join 唯一时间盒豁免（:1029）")
    # 表快照与常量一致（防两处漂移）
    expect(len(fsm.TIME_BOX_TABLE) == 27, "时间盒表快照 27 行")


def test_observation_window_frozen_values():
    expect(fsm.OBSERVATION_WINDOW_S == 525, "观测窗冻结值 525 s（:1058）")
    expect(fsm.OBSERVATION_WINDOW_S
           == fsm.SERIAL_BOX_UPPER_BOUND_S + fsm.CLOSE_MARGIN_S,
           "525 = 467 串行上界 + 58 收尾裕量（:1059）")
    expect(fsm.HILOG_WALLCLOCK_FALLBACK_S == 825
           and fsm.HILOG_WALLCLOCK_FALLBACK_S
           >= fsm.ALLOW_BOX_S + fsm.OBSERVATION_WINDOW_S,
           "HilogStream 墙钟兜底 ≥825 = 300 + 525（:1063）")
    expect(fsm.OPERATOR_READY_ACTION == "operator-ready-confirmed",
           "operator-ready 动作字面（:1053）")
    expect(death.T_TAIL_MS == 25000, "T_tail = 25000 ms（D7 20000 + 宽限 5000，:1359）")


# ==========================================================================
# W. 观测窗状态机（四分支 / late_marker / 已消费收口 / 位次）
# ==========================================================================

def _open_fsm():
    m = fsm.ObservationWindowFSM()
    m.register_operator_ready(0, 0)
    m.start_hilog_stream(1)
    m.issue_start_entry(2)
    return m


def test_fsm_order_constraints():
    m = fsm.ObservationWindowFSM()
    try:
        m.issue_start_entry(0)
        raise SystemExit("StartEntry 先于 operator-ready 必须 FsmError")
    except fsm.FsmError:
        pass
    m.register_operator_ready(0, 0)
    try:
        m.issue_start_entry(1)   # 未启 HilogStream
        raise SystemExit("StartEntry 先于 HilogStream 必须 FsmError")
    except fsm.FsmError:
        pass
    m.start_hilog_stream(2)
    m.issue_start_entry(3)
    expect(m.allow_deadline_mono_ms == 3 + 300_000, "Allow 盒自 StartEntry 起算")
    try:
        m2 = fsm.ObservationWindowFSM()
        m2.register_operator_ready(-1, 0)
        raise SystemExit("p0_ready_mono_ms 负值必须违反 BL-4 非负门")
    except fsm.FsmError:
        pass


def test_fsm_window_four_branches():
    d = fsm.ObservationWindowFSM.decide_close
    expect(d(True, True, None) == fsm.CLOSE_COMPLETE,
           "POST 在 → complete 封签（:1066）")
    expect(d(True, False, True) == fsm.CLOSE_PRE_ONLY,
           "仅 PRE + 死亡证据 → pre-only（:1067）")
    expect(d(True, False, False) == fsm.CLOSE_FAIL_F9,
           "仅 PRE 无死亡（false）→ F9 fail（:1067）")
    expect(d(True, False, None) == fsm.CLOSE_FAIL_F9,
           "仅 PRE 死亡不可判（unobservable）→ F9 fail（:1067）")
    expect(d(False, True, True) == fsm.CLOSE_FAIL_PRE_MISSING,
           "PRE 缺（POST 单独现）→ fail（:1068）")
    expect(d(False, False, None) == fsm.CLOSE_FAIL_PRE_MISSING,
           "两者皆缺 → fail（:1068）")


def test_fsm_expire_paths():
    m = _open_fsm()
    m.feed_marker("N1BDISC_D1_BEGIN", 10)
    m.feed_marker("N1BDISC_PRE", 20)
    m.feed_marker("N1BDISC_POST", 30)
    try:
        m.expire(40, True)
        raise SystemExit("观测窗未到点不得 expire")
    except fsm.FsmError:
        pass
    event = m.expire(m.window_deadline_mono_ms, None)
    expect(event.detail == fsm.CLOSE_COMPLETE, "窗到点 POST 在 → complete 封签")

    m2 = _open_fsm()
    m2.feed_marker("N1BDISC_D1_BEGIN", 10)
    m2.feed_marker("N1BDISC_PRE", 20)
    event2 = m2.expire(m2.window_deadline_mono_ms, True)
    expect(event2.detail == fsm.CLOSE_PRE_ONLY, "窗到点仅 PRE + 死亡 → pre-only")

    m3 = _open_fsm()
    m3.feed_marker("N1BDISC_PRE", 20)
    event3 = m3.expire(m3.window_deadline_mono_ms, False)
    expect(event3.detail == fsm.CLOSE_FAIL_F9, "窗到点仅 PRE 存活 → F9 fail")

    m4 = _open_fsm()
    m4.feed_marker("N1BDISC_POST", 20)   # POST 无 PRE 单独出现
    event4 = m4.expire(m4.window_deadline_mono_ms, None)
    expect(event4.detail == fsm.CLOSE_FAIL_PRE_MISSING, "PRE 缺 → fail（F2 面）")


def test_fsm_late_marker_after_close():
    m = _open_fsm()
    m.feed_marker("N1BDISC_PRE", 10)
    m.expire(m.window_deadline_mono_ms, True)
    for name in ("N1BDISC_D7_BEGIN", "N1BDISC_POST"):
        event = m.feed_marker(name, m.window_deadline_mono_ms + 5)
        expect(event is not None and event.kind == "late_marker_observed",
               "窗到点后到达的 marker 登记 late_marker_observed（:1066）")
    expect(m.late_markers == [("N1BDISC_D7_BEGIN", m.window_deadline_mono_ms + 5),
                              ("N1BDISC_POST", m.window_deadline_mono_ms + 5)],
           "late marker 逐枚登记、不参与求值")


def test_fsm_allow_box_consumed():
    m = _open_fsm()
    # Allow 盒到点（StartEntry + 300 s）后才出现首枚 marker → 已消费收口（:1054）
    m.feed_marker("N1BDISC_D1_BEGIN", m.allow_deadline_mono_ms)
    expect(m.closed and m.close_kind == fsm.CLOSE_FAIL_ALLOW_CONSUMED,
           "Allow 盒到点无 marker → 已消费 campaign 收口、观测窗从未开启")
    expect(m.window_start_mono_ms is None, "观测窗未开启")
    event = m.feed_marker("N1BDISC_PRE", m.allow_deadline_mono_ms + 1)
    expect(event.kind == "late_marker_observed", "其后 marker 皆为 late")


def test_stage_order_checker():
    expect(fsm.stage_order_violations(
        ["P1", "P2", "P3", "P4", "P5", "P5T", "P12"]) == [],
        "skip 分支位点缺席不破坏全序（子序列合法，:1010）")
    expect(fsm.stage_order_violations(["P6", "P5T"]) != [],
           "位点逆序 → 全序破坏（F3 输入）")
    expect(fsm.site_of("N1BDISC_D7_BEGIN") == "P6"
           and fsm.site_of("N1BDISC_PRE") == "P5T"
           and fsm.site_of("N1BDISC_POST") == "P12"
           and fsm.site_of("N1BDISC_FD") is None,
           "marker → 位点映射（FD 漂移不参与）")


# ==========================================================================
# ⑥ fault 解析契约（:1234-1255、:1512-1530）
# ==========================================================================

def test_fault_type_normalization():
    for raw, want in (("APPFREEZE", "APPFREEZE"), ("APP_FREEZE", "APPFREEZE"),
                      ("app-freeze", "APPFREEZE"), ("appfreeze", "APPFREEZE"),
                      ("JS_RAWERROR", "JSRAWERROR"), ("js-raw-error", "JSRAWERROR"),
                      ("CPP_CRASH", "CPPCRASH")):
        kind, value = death.normalize_fault_type(raw)
        expect((kind, value) == ("candidate", want),
               "归一化 %r → %s（:1235-1236）" % (raw, want))
    kind, value = death.normalize_fault_type("JSCRASH")
    expect((kind, value) == ("other", "JSCRASH"),
           "域外词根不猜近义（原值 = 归一化前逐字原文，:1237）")


def test_fault_type_jscrash_out_of_domain_no_fail():
    p = parse("JSCRASH", "", name="x.txt")   # Signal 空值 = 其余段（:1241 含空值）
    components = agg([p])
    expect(components.fault_type_observed == "other:JSCRASH",
           "fault_type_observed = other:JSCRASH（:1514）")
    signature = death.eval_probe_crash_signature(
        components.fault_type_observed, components.signal_observed, "P6")
    expect(signature.value == "observed-false" and signature.branch_values[0] == "false",
           "不命中三支任何一支 → observed-false → F1 不命中、不 fail（:1514）")
    # r9 同键对照：无平台信号 vs 有 SIGKILL 条目——期望结论相同（:1515）
    with_kill = agg([parse("JSCRASH", "SIGKILL", name="y.txt")])
    expect(with_kill.signal_observed == "SIGKILL", "signal_observed 照记 SIGKILL")
    sig2 = death.eval_probe_crash_signature(
        with_kill.fault_type_observed, with_kill.signal_observed, "P6")
    expect(sig2.value == signature.value == "observed-false",
           "SIGKILL 不在致命信号集，两支结论相同（:1515）")


def test_signal_three_segments():
    for s in ("SIGSEGV", "SIGABRT", "SIGBUS", "SIGFPE"):
        expect(death.classify_signal(s) == ("crash", s), "crash 段 %s（:1239）" % s)
    for s in ("SIGKILL", "SIGTERM"):
        expect(death.classify_signal(s) == ("platform", s),
               "平台终止段 %s（:1240）" % s)
    expect(death.classify_signal("SIGSTOP")[0] == "other"
           and death.classify_signal("")[0] == "other",
           "其余段原文逐字、不命中任何签名段（:1241）")


def test_fault_timestamp_and_field_contracts():
    p = parse("APPFREEZE", "SIGKILL", name="t.txt")
    expect(p.timestamp == "2026-09-06 10:00:00.000",
           "时间字段 = 首个 YYYY-MM-DD hh:mm:ss.mmm 形态（:1242）")
    expect(not p.time_missing, "可解析时间不构成解析域缺口")
    p2 = death.parse_fault_entry("u.txt", "Fault_Type: APPFREEZE\nno-timestamp-line\n")
    expect(p2.time_missing, "时间字段缺失 → criteria-gap (2) 解析域缺口（F8 面，:1242）")
    p3 = parse(None, None, name="v.txt")
    expect(p3.fault_type_raw is None and p3.signal_raw is None,
           "Fault_Type/Signal 字段行缺失 = 不可解析（分量 unobservable 面，:1378）")
    expect(death.snapshot_diff(["a", "b"], ["b", "c", "a", "d"]) == ["c", "d"],
           "窗界唯一判据 = 快照文件集合差分、字节序（:1214/:1254）")


def test_fault_type_empty_value_unparsable():
    # M1（gate 3 审查）：:1234 值 = ``:`` 后首个非空白 token 起至行尾；无 token 时
    # 契约无定义产物 = 值无法提取 → :1378 走 unobservable(fault-type-unparsable)，
    # 不得洗成 other:<空>（:1237 空值显式排除在 other: 分支外）。
    for tag, text in (
            ("Fault_Type:（无内容）",
             "Fault_Type:\nSignal: SIGSEGV\n2026-09-06 10:00:00.000\n"),
            ("Fault_Type:   （全空白）",
             "Fault_Type:   \nSignal: SIGSEGV\n2026-09-06 10:00:00.000\n")):
        p = death.parse_fault_entry("a.txt", text)
        expect(p.fault_type_raw is None and p.fault_type_kind is None
               and p.fault_type_value is None,
               "%s → 值无法提取 = 字段不可解析 None（:1234/:1378）" % tag)
        components = agg([p])
        expect(components.fault_type_observed == unobs("fault-type-unparsable"),
               "%s → 分量 unobservable(fault-type-unparsable)（:1378）" % tag)
        expect(components.unknown_causes == ("fault-type-unparsable",),
               "%s → unknown cause 入档（:1343）" % tag)
        expect(components.out_of_domain_literals == (),
               "%s → 空值不进 other: 域外档（:1237）" % tag)
    # :1234 语义不变：``:`` 后首个非空白 token 起至行尾（含内部空格原样返回）
    spaced = death.parse_fault_entry(
        "b.txt", "Fault_Type:  foo bar\n2026-09-06 10:00:00.000\n")
    expect(spaced.fault_type_raw == "foo bar",
           "含内部空格的值仍按「至行尾」原样返回（:1234）")


def test_signal_empty_value_regression_other_segment():
    # 回归保护（防未来误对称化）：Signal 空值属「其余」段（:1241 逐字含空值）——
    # 原文逐字入档、不命中任何签名段 → observed-false；不得变 signal-unparsable。
    for tag, text in (
            ("Signal:（无内容）",
             "Fault_Type: JSCRASH\nSignal:\n2026-09-06 10:00:00.000\n"),
            ("Signal:   （全空白）",
             "Fault_Type: JSCRASH\nSignal:   \n2026-09-06 10:00:00.000\n")):
        p = death.parse_fault_entry("a.txt", text)
        expect(p.signal_raw == "" and p.signal_kind == "other"
               and p.signal_value == "",
               "%s → 其余段 other + 空原文逐字（:1241）" % tag)
        components = agg([p])
        expect(components.signal_observed == "observed-false",
               "%s → observed-false（不命中签名段，:1246）" % tag)
        expect(components.signal_literals == ("",),
               "%s → 原文逐字入档（:1241）" % tag)
        expect(components.signal_observed != unobs("signal-unparsable"),
               "%s → 不得走 signal-unparsable（:1241 空值仍可解析）" % tag)


def test_multi_entry_positive_first_not_diluted():
    # (a) APPFREEZE + CPPCRASH（乱序喂入）→ CPPCRASH（正向优先，:1516）
    components = agg([parse("APPFREEZE", None, name="b.txt"),
                      parse("CPPCRASH", "SIGSEGV", name="a.txt")])
    expect(components.fault_type_observed == "CPPCRASH",
           "聚合 = CPPCRASH（正向优先不被 APPFREEZE 稀释，:1516）")
    expect(components.raw_predicates == (True, True, False), "raw 谓词 (1)(2)")
    expect(death.eval_probe_crash_signature(
        components.fault_type_observed, components.signal_observed, "P6").value
        == "observed-true", "第 1 支命中 → observed-true → F1（pre-only）fail（:1516）")
    # (b) APPFREEZE@P6 + SIGSEGV → 致命段优先，第 2 支独立命中（:1517）
    b = agg([parse("APPFREEZE", None, name="a.txt"),
             parse("JSCRASH", "SIGSEGV", name="b.txt")])
    expect(b.signal_observed == "SIGSEGV" and b.fault_type_observed == "APPFREEZE",
           "(b) signal=SIGSEGV、fault=APPFREEZE（:1517）")
    expect(death.eval_probe_crash_signature(
        b.fault_type_observed, b.signal_observed, "P6").value == "observed-true",
        "APPFREEZE@P6 不构成第 3 支、由第 2 支命中（:1517）")
    # (c) 双域外 → 分量单值取第一条、raw 档保留多值（:1518）
    c = agg([parse("JSCRASH", None, name="a.txt"),
             parse("JSERROR", None, name="b.txt")])
    expect(c.fault_type_observed == "other:JSCRASH"
           and c.out_of_domain_literals == ("JSCRASH", "JSERROR"),
           "(c) 分量 other:JSCRASH、raw 档多值不丢（:1518）")


def test_mixed_parsable_and_unparsable_entries():
    # (d) 可解析 APPFREEZE@P6 + 不可解析条目 → 两分量 unobservable、不 fail（:1519-1520）
    parses = [parse("APPFREEZE", None, name="a.txt"),
              death.parse_fault_entry("b.txt", "garbage-line\n2026-09-06 10:00:00.000\n")]
    components = agg(parses, site="P6")
    expect(components.fault_type_observed == unobs("fault-type-unparsable"),
           "(d) fault 侧 unobservable(fault-type-unparsable)（:1519）")
    expect(components.signal_observed == unobs("signal-unparsable"),
           "(d) signal 侧 unobservable(signal-unparsable)（:1519）")
    signature = death.eval_probe_crash_signature(
        components.fault_type_observed, components.signal_observed, "P6")
    expect(signature.value == unobs("fault-type-unparsable"),
           "三支全 unknown、cause 沿冻结优先序（:1520）→ F1 不命中 pre-only 不 fail")
    # r12 验收钉：CPPCRASH 可解析 + Signal 字段缺失 → true 不被旁路 unknown 稀释（:1521）
    r12 = agg([parse("CPPCRASH", None, name="a.txt")])
    expect(r12.fault_type_observed == "CPPCRASH"
           and r12.signal_observed == unobs("signal-unparsable"),
           "r12：fault=CPPCRASH、signal=unobservable(signal-unparsable)")
    sig12 = death.eval_probe_crash_signature(
        r12.fault_type_observed, r12.signal_observed, "P6")
    expect(sig12.value == "observed-true" and sig12.branch_values == ("true", "unknown", "false"),
           "r12：支 1 true、支 2 unknown、支 3 false → 聚合 observed-true（:1521）")


def test_faultrecv_partial_and_global_failure():
    # r13 (a) 部分失败 + 已知正向不被稀释（:1523-1525）
    a = agg([parse("CPPCRASH", None, name="a.txt")], failed=["b.txt"])
    expect(a.fault_type_observed == "CPPCRASH", "(a) 正向照常聚合")
    expect(death.eval_probe_crash_signature(
        a.fault_type_observed, a.signal_observed, "P6").value == "observed-true",
        "(a) true > unknown → observed-true → pre-only fail（F1）（:1525）")
    # r13 (b) 部分失败 + 无正向 → unobservable(faultrecv-unavailable)、不 fail（:1526-1527）
    b = agg([parse("APPFREEZE", None, name="a.txt")], failed=["b.txt"], site="P6")
    expect(b.fault_type_observed == unobs("faultrecv-unavailable")
           and b.signal_observed == unobs("faultrecv-unavailable"),
           "(b) cause 沿冻结优先序：取回失败高于字段不可解析（:1526）")
    expect(death.eval_probe_crash_signature(
        b.fault_type_observed, b.signal_observed, "P6").value
        == unobs("faultrecv-unavailable"), "(b) 三支全 unknown → 签名同 cause → 不 fail")
    # r13 (c) 全局失败回归钉（:1528）
    g1 = agg([], probe_failed=True)
    g2 = agg([], failed=["a.txt", "b.txt"])
    for g, tag in ((g1, "probe-fail"), (g2, "all-recv-fail")):
        expect(g.fault_type_observed == unobs("faultrecv-unavailable")
               and g.signal_observed == unobs("faultrecv-unavailable"),
               "(c)%s 两分量各记 unobservable(faultrecv-unavailable)（:1528）" % tag)
    # r15 raw 谓词 (3) × 部分失败（:1529-1530）
    d = agg([parse("APPFREEZE", None, name="a.txt")], failed=["b.txt"],
            site="P2", seq_ok=True)
    expect(d.fault_type_observed == "APPFREEZE",
           "(d) 谓词 (3) 命中、fault 侧不被失败稀释（:1530）")
    expect(d.signal_observed == unobs("faultrecv-unavailable"),
           "(d) signal 侧无正向（谓词 (2) 不命中）→ unobservable（跨分量不传导）")
    expect(death.eval_probe_crash_signature(
        d.fault_type_observed, d.signal_observed, "P2").value == "observed-true",
        "(d) 第 3 支 true → observed-true（verdict 由 F2 承载：短时步 ⇒ PRE 未发）")
    # 无条目 ≠ 不可求值（:1338）
    empty = agg([])
    expect(empty.fault_type_observed == "observed-false"
           and empty.signal_observed == "observed-false",
           "FaultRecv 已执行且取回空 → 确定观测 observed-false（:1338）")


# ==========================================================================
# ⑧ fake-HDC 沙箱（:1534 ↔ 白名单 :1616-1644）
# ==========================================================================

def test_hdc_whitelist_all_operations_pass():
    transport = fake.FakeHdc(fake.make_happy_path_scenario())
    executor = hdc.HdcExecutor(transport, target=transport.target,
                               hap_path=transport.hap_path)
    exec_op = lambda op, **kw: executor.execute(op, **kw)  # noqa: E731
    expect(exec_op("version").result.exit_code == 0, "gate5 探针 1（大小写不敏感）")
    expect(exec_op("ParamModel").result.exit_code == 0, "gate5 探针 2")
    expect(exec_op("ParamSoftwareVersion").result.exit_code == 0, "gate5 探针 3")
    expect(exec_op("FaultProbe").result.exit_code == 0, "FaultProbe（快照）")
    expect(exec_op("mkdirstaging").result.exit_code == 0, "MkdirStaging")
    expect(exec_op("SendHap").result.exit_code == 0, "SendHap")
    expect(exec_op("InstallHap").result.exit_code == 0, "InstallHap")
    expect(exec_op("StartEntry").result.exit_code == 0, "StartEntry")
    expect(exec_op("PidOf").result.stdout.strip() != "", "PidOf（UI 进程在）")
    expect(exec_op("PidOfVpn").result.stdout.strip() != "",
           "PidOfVpn（positive 基线形态）")
    expect(exec_op("PidOfPost").result.exit_code == 0
           and exec_op("PidOfVpnPost").result.exit_code == 0,
           "PidOfPost/PidOfVpnPost argv 同源")
    expect(exec_op("ForceStop", reason="final-cleanup").result.exit_code == 0,
           "ForceStop（Reason=final-cleanup）")
    expect(exec_op("Uninstall").result.exit_code == 0, "Uninstall")
    expect(exec_op("RemoveStaging").result.exit_code == 0, "RemoveStaging")
    expect(exec_op("StagingProbe").result.exit_code != 0, "StagingProbe（清理后 absent）")
    expect(exec_op("BundleDump").result.exit_code != 0, "BundleDump（卸载后 absent）")
    recv = exec_op("FaultRecv", fault_file="f-cn.alfadb.netbird.n1bdisc-1",
                   host_path="/host/fake/f1")
    expect(recv.result.exit_code != 0, "FaultRecv 未命中文件 → 取回失败面")
    for record in executor.audit:
        expect(record.argv[0] == ("-t" if record.op != "Version" else "version"),
               "审计 argv 均按冻结模板展开（-t <T> 前缀逐字，:1622）")
        if record.op != "Version":
            expect(record.argv[1] == transport.target, "目标句柄逐字绑定")
    try:
        executor.open_stream("PidOf")
        raise SystemExit("非 HilogStream 不得走流式通道")
    except hdc.HdcViolation:
        pass


def test_hdc_whitelist_violations_rejected():
    transport = fake.FakeHdc(fake.make_happy_path_scenario())
    # 未知操作/argv 不在白名单
    for bad_argv in (
        ["-t", "T", "shell", "rm", "-rf", "/data"],               # 白名单外子命令
        ["shell", "pidof", core.DEFAULT_BUNDLE],                  # 缺 -t 前缀
        ["-t", "T", "shell", "pidof", "cn.alfadb.netbird.other"], # bundle 不符
        ["-t", "T", "shell", "pidof", core.DEFAULT_BUNDLE, "-x"], # 多余参数
        ["-t", "T", "shell", "aa", "start", "-a", "EntryAbility",
         "-b", core.DEFAULT_BUNDLE, "-m", "feature"],             # 字面不符
        ["version", "extra"],                                     # version 带多余参数
    ):
        try:
            transport.call(bad_argv)
            raise SystemExit("违规 argv 未被拒: %r" % (bad_argv,))
        except hdc.HdcViolation:
            pass
    # 执行器层：未知操作 / 缺参数 / 多余参数 / Reason 非法 / bundle 不符
    executor = hdc.HdcExecutor(transport, target="T")
    for op, kw, reason in (
        ("Nope", {}, "unknown-operation"),
        ("ForceStop", {}, "missing-parameter"),
        ("PidOf", {"x": "1"}, "extra-parameter"),
        ("ForceStop", {"reason": "because"}, "illegal-reason"),
        ("FaultRecv", {"fault_file": "other-bundle-1", "host_path": "/h"},
         "bundle-mismatch"),
    ):
        try:
            executor.execute(op, **kw)
            raise SystemExit("%s(%r) 未被拒" % (op, kw))
        except hdc.HdcViolation as exc:
            expect(exc.reason == reason, "%s 拒因 = %s" % (op, reason))
    # 真实 transport 占位：任何调用即拒绝
    try:
        hdc.RealHdcTransportStub().call(["version"])
        raise SystemExit("真实 transport 占位必须拒绝")
    except NotImplementedError:
        pass


def test_fake_hdc_lifecycle_semantics():
    transport = fake.FakeHdc(fake.make_pre_only_scenario())
    expect(transport.call(["version"]).exit_code == 0, "version 合法")
    # 未安装 → StartEntry 拒绝（语义面）
    expect(transport.call(["-t", transport.target, "shell", "aa", "start",
                           "-a", "EntryAbility", "-b", core.DEFAULT_BUNDLE,
                           "-m", "entry"]).exit_code != 0, "未安装 StartEntry 失败")
    transport._op_mkdirstaging()
    transport._op_sendhap()
    transport._op_installhap()
    transport._op_startentry()
    expect(transport.vpn_alive and transport.ui_alive, "启动后 UI/:vpn 进程在")
    expect(transport.call(["-t", transport.target, "shell", "pidof",
                           core.DEFAULT_BUNDLE + ":vpn"]).stdout.strip()
           == str(20002), "pidof :vpn 返回 pid 字面（positive 基线可取）")
    transport._mark_vpn_dead()
    expect(transport.call(["-t", transport.target, "shell", "pidof",
                           core.DEFAULT_BUNDLE + ":vpn"]).stdout.strip() == "",
           "死亡后 :vpn absent（空输出）")
    expect(transport.fault_materialized, "死亡时刻物化本窗新增 fault 条目")


def test_fake_hilog_stream_forms_and_chunks():
    transport = fake.FakeHdc(fake.make_happy_path_scenario())
    lines = list(transport.open_stream(["-t", transport.target, "shell", "hilog",
                                        "-T", core.HILOG_TAG,
                                        "-v", "year", "-v", "zone"]))
    events = core.scan_markers(lines)
    expect(len(events) > 40 and events[0].name == "N1BDISC_D1_BEGIN",
           "合成流可被三形态关联并解析（:417）")
    forms = {e.tag_form for e in events}
    expect(forms == {"entry", "truncated", "complete"}, "三形态 tag 全出现（:417）")
    chunks = core.reassemble_chunks([e for e in events if e.name == "N1BDISC_CHUNK"])
    expect(chunks.ok and ("dlerror", 0) in chunks.texts, "注入 chunk 可重组")
    # 重复片注入：一致 → 观察项；payload 不一致 → fail
    dup = fake.duplicate_chunk_markers(
        fake.chunk_markers("dup-detail", "foreign", 0, 999))
    ok = core.reassemble_chunks(dup + dup)
    expect(ok.ok and any(o["type"] == "duplicate_chunk_observed" for o in ok.observations),
           "一致重复片 → duplicate_chunk_observed 观察项（r4 S6）")
    bad = core.reassemble_chunks(dup + fake.duplicate_chunk_markers(
        dup, payload_override="AAAA"))
    expect(not bad.ok, "不一致重复片 → fail（r4 S6/r5 U12）")
    # late marker 注入：不进正常流、由 late_lines 提供（:1066）
    scenario = fake.FakeScenario(
        markers=[fake.FakeMarker("D1_BEGIN", 1, {}, late=True)])
    late_transport = fake.FakeHdc(scenario)
    expect(list(late_transport.open_stream(["-t", late_transport.target, "shell",
                                            "hilog", "-T", core.HILOG_TAG,
                                            "-v", "year", "-v", "zone"])) == [],
           "late marker 不进正常流")
    expect(len(late_transport.late_lines()) == 1
           and core.scan_markers(late_transport.late_lines())[0].name
           == "N1BDISC_D1_BEGIN", "late_lines 提供窗到点后到达的 marker")


# ==========================================================================
# ⑨ PRE/POST 通道（:1536-1560）
# ==========================================================================

def test_protocol_dual_value_derivation():
    expect(verdict.PROTOCOL_VALUES == ("complete", "pre-only"),
           "protocol 取值域恰两值（:1132）")
    expect(verdict.derive_protocol(True, True) == "complete", "(1) 双现 → complete")
    expect(verdict.derive_protocol(True, False) == "pre-only", "(2) PRE 在 POST 缺 → pre-only")
    expect(verdict.derive_protocol(False, True) is None
           and verdict.derive_protocol(False, False) is None,
           "(3)(4) PRE 缺 → protocol 不可求值（F2 承载，:1133）")
    r = verdict.evaluate(verdict.VerdictInput(pre_present=False, post_present=True))
    expect(r.verdict == "fail" and "F2" in r.gates, "POST 无 PRE 单独现 → fail（F2）")


def test_e2e_happy_complete_pass():
    camp, judged = judge("happy")
    rec_protocol = verdict.derive_protocol(
        any(e.name == "N1BDISC_PRE" for e in camp["stream_events"]),
        any(e.name == "N1BDISC_POST" for e in camp["stream_events"]))
    expect(rec_protocol == "complete", "全 marker 剧本 → protocol=complete")
    result = judged["verdict_result"]
    expect(result.verdict == "pass" and not result.gates,
           "happy-path → complete pass（fail 闭集全不命中）")
    # ledger 与 digest 同切点（:1200-1203）
    ledger = judged["ledger"]
    expect(ledger["pre_digest"] == ledger["pre_rebuild_digest"]
           and ledger["final_digest"] == ledger["final_rebuild_digest"],
           "PRE↔P5T 快照切点、POST↔最终切点均一致（跨切点比较禁止）")
    expect(ledger["pre_digest"] != ledger["final_digest"],
           "P5T 快照值与最终值确为两个切点")
    # P12 派生比对一致（runner 重建 vs POST dw_outcome）
    expect(judged["dw"]["rebuilt_class"] == "fd-event-like"
           and judged["dw"]["post_class"] == "fd-event-like",
           "runner 重建 fd-event-like（唯一归因类）与 POST 派生一致")
    expect(camp["close_kind"] == fsm.CLOSE_COMPLETE, "FSM complete 封签")
    expect(camp["verified_clean"], "host finally 步 8 absent 四探针 verified-clean")
    expect(not camp["pidof_absent"],
           "complete 主线 finally 采样（ForceStop 之前）：进程持续在场 → observed-false（:1280）")
    expect(camp["death_observed_value"] == "observed-false",
           "process_death_observed = observed-false（存活到 POST，:1280）")


def test_e2e_pre_only_pass_with_sigkill_entry():
    camp, judged = judge("pre-only")
    result = judged["verdict_result"]
    expect(result.verdict == "pass", "pre-only 合法终态 → pass（死亡非缺陷，:1196）")
    vector = camp
    expect(camp["death_observed_value"] == "observed-true",
           "process_death_observed=observed-true（positive 基线 + absent + 静默）")
    expect(camp["components"].signal_observed == "SIGKILL",
           "SIGKILL 条目照记（平台终止段，仅记录）")
    expect(camp["components"].fault_type_observed == "APPFREEZE",
           "APPFREEZE@P6（非短时步）仅喂事件分量")
    expect(camp["signature"].value == "observed-false",
           "第 2 支不命中（SIGKILL 非致命段）、第 3 支位点守卫不成立 → F1 不命中")
    expect(camp["destroy_state"] == "not-reached", "死于 D7 → 五态 not-reached")
    expect(camp["tail_state"] == "tail-loss-not-indicated",
           "POST 缺 + 有死亡证据 + 跨度 ≤ T_tail")
    expect(camp["close_kind"] == fsm.CLOSE_PRE_ONLY, "FSM pre-only 收口")
    expect(judged["ledger"]["pre_digest"] == judged["ledger"]["pre_rebuild_digest"],
           "PRE P5T 快照 digest 同切点一致")


def test_e2e_no_live_fd_post_still_pass():
    camp, judged = judge("no-live-fd")
    result = judged["verdict_result"]
    expect(result.verdict == "pass", "五 create 全拒 → no-live-fd POST 照发 → pass（:1543）")
    post = next(e for e in camp["stream_events"] if e.name == "N1BDISC_POST")
    expect(post.kv["d6_items"].count("skipped(cause=no-live-fd)") == 7,
           "d6_items 逐子项 skipped(cause=no-live-fd)（:1543）")
    expect("unobservable(cause=no-live-fd)" in post.kv["dw_outcome"],
           "dw_* 沿具名 skip 清单落值（:1543）")
    pre = next(e for e in camp["stream_events"] if e.name == "N1BDISC_PRE")
    expect(pre.kv["skip_summary"] != "none", "PRE skip_summary 携带 skip 列表")
    expect(camp["destroy_state"] == "not-called",
           "SKIP|item=destroy → 五态 not-called（唯一可证「未调用」途径）")
    expect(judged["dw"]["post_class"] == unobs("no-live-fd"),
           "dw_return_class = unobservable(cause=no-live-fd)")


def test_dw_outcome_probe_format_parse_pins():
    # 观察 (ii) 整改钉：探针 dw_outcome 内层实际格式（probe/src/dw.rs:1008-1018
    # post_emit format! 字面：';' 分隔 k=v、八字段固定位序）解析成功；逗号格式
    #（旧 DryRun 合成约定）必须解析失败——fail-closed，不宽松接受垃圾列。
    race = ";".join((
        "class=" + unobs("flag-race-window-expired"),
        "join=join-timeout",
        "watchdog=" + unobs("marker-gap-indeterminate"),
        "dist=" + unobs("flag-race-window-expired"),
        "poll_ret=" + unobs("flag-race-window-expired"),
        "poll_errno=" + unobs("flag-race-window-expired"),
        "poll_revents=" + unobs("flag-race-window-expired"),
        "poll_elapsed_ms=" + unobs("flag-race-window-expired")))
    out = verdict.parse_dw_outcome(race)
    expect(set(out) == set(verdict.DW_OUTCOME_FIELDS), "探针格式全列 → 八字段齐")
    expect(out["class"] == unobs("flag-race-window-expired")
           and out["watchdog"] == unobs("marker-gap-indeterminate")
           and out["join"] == "join-timeout",
           "flag-race 格 class/watchdog/join 按字面取出")
    expect(all(out[inner] == unobs("flag-race-window-expired")
               for inner in ("poll_ret", "poll_errno", "poll_revents",
                             "poll_elapsed_ms")),
           "poll raw 四字段按探针内层名取出（无 dw_ 前缀、elapsed 带 _ms）")
    happy = ";".join(("class=fd-event-like", "join=joined",
                      "watchdog=observed-false", "dist=observed-true",
                      "poll_ret=1", "poll_errno=0", "poll_revents=4",
                      "poll_elapsed_ms=200"))
    out = verdict.parse_dw_outcome(happy)
    expect(out.get("class") == "fd-event-like" and len(out) == 8,
           "happy 13 类格探针字面解析成功")
    mapped = {raw: out[inner] for inner, raw
              in verdict.DW_OUTCOME_POLL_RAW_KEYS.items()}
    expect(set(mapped) == set(verdict.POLL_RAW_FIELDS)
           and mapped["dw_poll_revents"] == "4"
           and mapped["dw_poll_return_elapsed_ms"] == "200",
           "内层 poll 名 → 冻结闭表名（POLL_RAW_FIELDS）映射取值正确")
    expect(verdict.parse_dw_outcome(happy.replace(";", ",")) == {},
           "逗号格式夹具（旧 DryRun 约定）解析失败：fail-closed 非宽松接受")
    expect(verdict.parse_dw_outcome("bogus=x;" + happy) == {},
           "未知键 → 解析失败（契约外形态不收）")
    expect(verdict.parse_dw_outcome(
        happy.replace("poll_ret=1", "poll_ret=")) == {},
        "空值 → 解析失败")
    # NEW-minor-d 钉：字段集强校验（恰八字段）——缺任一字段即解析失败；
    # 位序不校验（探针发射位序固定仅为登记事实，乱序全列仍可解析）。
    expect(verdict.parse_dw_outcome(happy.replace("poll_errno=0;", "")) == {},
           "缺一字段（poll_errno）→ 解析失败：恰八字段集强校验")
    expect(verdict.parse_dw_outcome(
        ";".join(p for p in happy.split(";")
                 if not p.startswith("watchdog="))) == {},
           "缺一字段（watchdog）→ 解析失败：恰八字段集强校验")
    reordered = ";".join(reversed(happy.split(";")))
    expect(verdict.parse_dw_outcome(reordered) == verdict.parse_dw_outcome(happy),
           "位序不校验：乱序全列（八字段齐）解析结果与原序一致")


def test_flag_race_complete_cut_false_e2e():
    # 观察 (i) 端到端钉：complete 收口（POST 在）而 worker_terminal_at_p12=false
    # 的真机格——最终重建切点由 close_kind 驱动（complete-seal → open-at-exit，
    # 对齐探针 POST digest 切口 probe/src/dw.rs:1006），两侧 digest 同切点一致，
    # 合法 campaign 不假 F5。
    camp = run.run_dryrun_campaign("flag-race", fake.make_flag_race_scenario())
    judged = run.derive_and_judge(camp)
    result = judged["verdict_result"]
    post = next(e for e in camp["stream_events"] if e.name == "N1BDISC_POST")
    expect(post.kv["worker_terminal_at_p12"] == "false",
           "剧本格：POST 在而 FLAG=false（真机 flag-race/join-timeout 形态）")
    expect(camp["close_kind"] == fsm.CLOSE_COMPLETE, "POST 在 → complete-seal 收口")
    expect(any(e.name == "N1BDISC_DW_RACEWIN" for e in camp["stream_events"]),
           "RACEWIN 在 capture（r22 仅此格发射）")
    expect(";" in post.kv["dw_outcome"] and "," not in post.kv["dw_outcome"],
           "fake_hdc 合成对齐探针实际格式（';' 内层分隔、无逗号）")
    ledger = judged["ledger"]
    expect(ledger["final_rebuild_cut"] == "complete"
           and ledger["final_rebuild_digest"] == ledger["final_digest"],
           "最终重建与探针 POST digest 同切点（open-at-exit）一致 → 不假 F5")
    transitions = [e.kv for e in camp["stream_events"] if e.name == "N1BDISC_FD"]
    expect(core.rebuild_fd_ledger(transitions, "pre-only").digest
           != ledger["final_digest"],
           "本格切点敏感（open 条目在）：pre-only 切口 ≠ complete 切口——"
           "旧 wtap 启发（FLAG=false → pre-only 切口）在此格必假 F5")
    expect(result.verdict == "pass" and "F5" not in result.gates
           and "F8" not in result.gates,
           "flag-race 格合法 campaign → pass（F5/F8 全不命中）")
    dw = judged["dw"]
    expect(dw["post_class"] == unobs("flag-race-window-expired")
           and dw["cut_report"]["cut_false"] and dw["cut_report"]["racewin_present"],
           "POST 派生 class=flag-race-window-expired、cut-state (B) 输入在")
    expect(verdict.cut_state_b_violations(
        True, dw["cut_report"]["class"], dw["cut_report"]["poll_raw"],
        dw["cut_report"]["watchdog"], True) == [],
        "cut-state (B) 闭表零违规（RACEWIN 双向、poll 四字段恰一允许值、watchdog⑤）")


def test_final_rebuild_cut_driven_by_close_kind():
    # 观察 (i) 单元钉：最终重建切点 = 收口形态闭表（判据 :385/:402-404），
    # worker_terminal_at_p12 仅作登记字段、不驱动切点。
    expect(run._final_rebuild_cut(fsm.CLOSE_COMPLETE) == "complete",
           "complete-seal → complete（open-at-exit，探针 POST digest 同切口）")
    expect(run._final_rebuild_cut(fsm.CLOSE_PRE_ONLY) == "pre-only",
           "pre-only → pre-only（process-exit，r6 W3）")
    expect(run._final_rebuild_cut(fsm.CLOSE_FAIL_F9) == "host-forcestop",
           "fail-cleanup（观测窗到点进程仍活）→ host-forcestop（:404）")
    expect(run._final_rebuild_cut(fsm.CLOSE_FAIL_PRE_MISSING) == "complete",
           "PRE 缺收口无合法终值形态可比 → 沿 complete 形态切口登记")
    # pre-only 收口：无 POST digest 即无对端值，runner 重建携带于记录（:389）
    camp, judged = judge("pre-only")
    ledger = judged["ledger"]
    transitions = [e.kv for e in camp["stream_events"] if e.name == "N1BDISC_FD"]
    rebuild = core.rebuild_fd_ledger(transitions, "pre-only")
    expect(ledger["final_digest"] is None
           and ledger["final_rebuild_digest"] == rebuild.digest
           and ledger["final_rebuild_cut"] == "pre-only"
           and ledger["final_rebuild_from_transition_marker"] is True,
           "pre-only 收口：runner 按 process-exit 切口重建携带于记录并逐字标注（:389）")


def test_skip_anchor_cooccurrence_gate_f3():
    # SKIP|item=destroy 与调用锚同现 → 域门先于五态表（:1296/:1542）
    state = death.eval_destroy_call_state(True, True, False)
    expect(state == unobs("marker-contradiction"),
           "同现域门 → destroy_call_state = marker-contradiction（:1542）")
    expect(death.eval_destroy_call_state(False, False, True)
           == unobs("marker-contradiction"),
           "`_C` 在 `_T` 缺 → 五态第 5 行 marker-contradiction（verdict 走 F3）")
    inp = verdict.VerdictInput(pre_present=True, post_present=True,
                               order_violations=["SKIP|item=destroy 与调用锚同现域门"])
    result = verdict.evaluate(inp)
    expect(result.verdict == "fail" and "F3" in result.gates,
           "同现矛盾 → fail（F3 顺序破坏轴，:1542）")
    expect(death.eval_destroy_call_state(True, False, False) == "not-called",
           "纯 SKIP（无锚）→ not-called")


def test_barrier_never_observed_pre_only_pass():
    # r9 第五步顺延路径（:1544/:1507）：SKIP destroy barrier-never-observed → pre-only pass
    markers = [
        fake.FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
        fake.FakeMarker("D2_ENTRY", 140, {"id": "MR1"}),
        fake.FakeMarker("PRE", 200, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                     "skip_summary": "destroy:barrier-never-observed"}),
        fake.FakeMarker("DW_SPAWN", 300, {}, "complete"),
        fake.FakeMarker("DW_DRAIN", 310, {"elapsed_ms": "100", "end": "eagain"},
                        "complete"),
        fake.FakeMarker("SKIP", 320, {"item": "destroy",
                                      "cause": "barrier-never-observed"}),
    ]
    scenario = fake.FakeScenario(markers=markers, die_at_end=True)
    transport = fake.FakeHdc(scenario)
    lines = list(transport.open_stream(["-t", transport.target, "shell", "hilog",
                                        "-T", core.HILOG_TAG,
                                        "-v", "year", "-v", "zone"]))
    events = core.scan_markers(lines)
    expect(death.eval_destroy_call_state(True, False, False) == "not-called",
           "顺延路径补发 SKIP → 五态 not-called（:1544）")
    result = core.derive_dw_return_class(core.DwReturnInput(
        ret=0, revents=0, has_skip_destroy=True, has_destroy_c=False))
    expect(result.outcome == "class" and result.value == "destroy-skip-proven",
           "分域 (c)：SKIP|item=destroy 存在即类 0（无 RETURN 亦判，:736）")
    inp = verdict.VerdictInput(pre_present=True, post_present=False,
                               crash_signature="observed-false")
    expect(verdict.evaluate(inp).verdict == "pass",
           "PRE 在 + 无崩溃签名 → pre-only pass（:1507）")
    expect(transport.vpn_died, "剧本流末进程死亡")


def test_p12_derivation_mismatch_fails_f82():
    # r17 P12 派生比对钉（:1559）：错写值在 class 域内 → 比对不一致唯一挂 F8(2)
    expect(not verdict.p12_class_consistency("timeout-like", "fd-event-like"),
           "重建 timeout-like vs 错写 fd-event-like → 不一致")
    expect(verdict.p12_class_consistency("timeout-like", "timeout-like"),
           "一致 → 不命中")
    inp = verdict.VerdictInput(
        pre_present=True, post_present=True,
        parse_domain_gaps=["P12-derived dw_return_class='fd-event-like' != "
                           "runner rebuild 'timeout-like'"])
    result = verdict.evaluate(inp)
    expect(result.verdict == "fail" and "F8" in result.gates,
           "P12 派生与 runner 重建不一致 → fail（F8(2)，:1559）")


def test_ledger_failures_fail_f5():
    inp = verdict.VerdictInput(pre_present=True, post_present=True,
                               ledger_failures=["POST ledger_digest != final rebuild"])
    result = verdict.evaluate(inp)
    expect(result.verdict == "fail" and "F5" in result.gates,
           "同切点 digest 校验不符 → fail（F5，:1147）")
    inp2 = verdict.VerdictInput(pre_present=True, post_present=True,
                                frozen_field_missing=["POST.ledger_digest"])
    expect(verdict.evaluate(inp2).verdict == "fail",
           "POST 冻结字段缺项 → fail（沿 MJ-7 → F4 面）")


# ==========================================================================
# ⑩ 七分量 + F1 三支闭集真值表（:1562-1579）
# ==========================================================================

def test_f1_branch1_crash_types():
    for ft in ("CPPCRASH", "JSRAWERROR"):
        components = agg([parse(ft, None, name="a.txt")])
        signature = death.eval_probe_crash_signature(
            components.fault_type_observed, components.signal_observed, "P6")
        expect(signature.value == "observed-true"
               and signature.branch_values[0] == "true",
               "Ⅰ: Fault_Type=%s → 第 1 支命中（:1564）" % ft)
        r = verdict.evaluate(verdict.VerdictInput(pre_present=True, post_present=False,
                                                  crash_signature="observed-true"))
        expect(r.verdict == "fail" and "F1" in r.gates,
               "Ⅰ: pre-only + 签名 → fail（F1）（:1564）")


def test_f1_branch2_fatal_signals():
    for sig in ("SIGSEGV", "SIGABRT", "SIGBUS", "SIGFPE"):
        components = agg([parse(None, sig, name="a.txt")])
        expect(components.signal_observed == sig, "致命段 %s 照记" % sig)
        signature = death.eval_probe_crash_signature(
            components.fault_type_observed, components.signal_observed, "P6")
        expect(signature.value == "observed-true"
               and signature.branch_values[1] == "true",
               "Ⅱ: Signal=%s → 第 2 支命中 → fail（F1）（:1565）" % sig)
        r = verdict.evaluate(verdict.VerdictInput(pre_present=True, post_present=False,
                                                  crash_signature="observed-true"))
        expect(r.verdict == "fail", "Ⅱ: pre-only 判 fail（:1565）")


def test_f1_branch3_short_step_and_guards():
    # Ⅲ：APPFREEZE + last_visible_site ∈ 短时步集 + marker 序列无矛盾（:1566）
    signature = death.eval_probe_crash_signature("APPFREEZE", "observed-false",
                                                 "P2", marker_seq_ok=True)
    expect(signature.value == "observed-true" and signature.branch_values[2] == "true",
           "Ⅲ: 第 3 支命中（事实记录验证；verdict 由 F2 承载，:1566）")
    # 第 3 支守卫反例（:1568）：marker 序列自相矛盾 → 支 3 false（不稀释为 true）
    contradicted = death.eval_probe_crash_signature("APPFREEZE", "observed-false",
                                                    "P2", marker_seq_ok=False)
    expect(contradicted.value == "observed-false"
           and contradicted.branch_values[2] == "false",
           "守卫反例：序列矛盾 → 支 3 false、签名 observed-false（:1568）")
    # 非短时步集上的 APPFREEZE（:1570）
    p6 = death.eval_probe_crash_signature("APPFREEZE", "observed-false", "P6")
    expect(p6.value == "observed-false", "P6/D7 非短时步 → 第 3 支不成立、不 fail（:1570）")
    for site in ("P1", "P3", "P4", "P7", "P8", "P9", "P10"):
        expect(death.SHORT_STEP_SITES.isdisjoint({site}),
               "位点 %s 不在短时步集（P1 已由 R5 移出，:1231）" % site)
    # verdict：第 3 支命中构造（PRE 必未发）→ F2 承载（:1566/:1318）
    r = verdict.evaluate(verdict.VerdictInput(pre_present=False, post_present=False,
                                              crash_signature="observed-true"))
    expect(r.verdict == "fail" and r.gates == ["F2"],
           "Ⅲ verdict 由 F2 承载（PRE 缺失时 protocol 不可求值，:1133）")


def test_f1_complete_form_not_fail():
    # complete + 命中崩溃签名 → 不进 F1、不 fail、登记异常观察（:1575-1576）
    r = verdict.evaluate(verdict.VerdictInput(pre_present=True, post_present=True,
                                              crash_signature="observed-true"))
    expect(r.verdict == "pass" and "F1" not in r.gates,
           "complete 限定：protocol != complete 不满足 → 不进 F1（:1576）")
    # r12 对照例：CPPCRASH + SIGKILL（平台终止段不抵消第 1 支）（:1572）
    components = agg([parse("CPPCRASH", "SIGKILL", name="a.txt")])
    expect(components.fault_type_observed == "CPPCRASH"
           and components.signal_observed == "SIGKILL",
           "r12 对照：fault=CPPCRASH、signal=SIGKILL 各自照记")
    expect(death.eval_probe_crash_signature(
        components.fault_type_observed, components.signal_observed,
        "P6").value == "observed-true", "第 1 支真不受第 2 支 SIGKILL 影响（:1572）")


def test_seven_components_independent_and_valued():
    # 无 fault 条目 + 进程消失（:1573-1574 替代钉）
    components = agg([])
    expect(components.fault_type_observed == "observed-false"
           and components.signal_observed == "observed-false",
           "无条目 → 两事件分量 observed-false")
    pd = death.eval_process_death(True, True, True)
    tail = death.eval_marker_tail_state(False, True, 100_000, 90_000)
    signature = death.eval_probe_crash_signature(
        components.fault_type_observed, components.signal_observed, "P6")
    expect(pd == "observed-true", "进程消失由 pidof 基线转 absent + 静默独立判定（:1574）")
    expect(signature.value == "observed-false", "签名 observed-false → 不 fail（:1574）")
    vector = death.EvidenceVector(
        process_death_observed=pd,
        last_visible_site=death.eval_last_visible_site(["P1", "P5T"]),
        fault_type_observed=components.fault_type_observed,
        signal_observed=components.signal_observed,
        destroy_call_state=death.eval_destroy_call_state(False, False, False),
        marker_tail_state=tail,
        probe_crash_signature_observed=signature.value)
    values = vector.as_dict()
    for key in ("process_death_observed", "last_visible_site", "fault_type_observed",
                "signal_observed", "destroy_call_state", "marker_tail_state",
                "probe_crash_signature_observed"):
        expect(values.get(key), "七分量逐项有值（记录义务，:1284）：%s" % key)
    # 互不推导（:1577-1578）：fault=false 不得推出 death=false；tail 不参与签名
    expect(death.eval_process_death(True, True, True) == "observed-true"
           and components.fault_type_observed == "observed-false",
           "fault=false 与 death=true 并存合法（分量互不推导）")
    expect(death.eval_probe_crash_signature(
        "observed-false", "observed-false", "P6").value == "observed-false",
        "marker_tail 任何取值不参与签名求值（签名输入无 tail）")
    # 基线缺失回退（:1281）
    expect(death.eval_process_death(False, None, None)
           == unobs("pidofvpn-no-positive-baseline"),
           "无 positive 基线 → 不得反推进程死亡（:1218）")
    expect(death.eval_process_death(True, True, None)
           == unobs("marker-gap-indeterminate"),
           "absent 而静默不可判 → 宁缺勿误（:1282）")


# ==========================================================================
# ⑪ watchdog ①-⑤ 有序互斥表 + marker_tail 边界（:1586-1607）
# ==========================================================================

def test_watchdog_branch1_destroy_terminal_candidate():
    value = watchdog(c_present=True, pidof_absent=True, exit_present=False)
    expect(value == unobs("destroy-terminal-candidate"),
           "① `_C` 在 + absent + EXIT 缺 → destroy-terminal-candidate、不得 true（:1586）")
    expect(watchdog(c_present=True, pidof_absent=True, exit_present=True)
           == "observed-false",
           "① EXIT 在时前件不命中 → ④ observed-false（:880）")


def test_watchdog_branch2_call_boundary():
    expect(watchdog(t_present=True, exit_present=False)
           == unobs("call-boundary-incomplete"),
           "② `_T` 在 `_C` 缺 EXIT 缺 → call-boundary-incomplete 透传（:1587）")
    # (a) inwait 竞态交错：EXIT 在 → ④ observed-false，不得 call-boundary（:1589）
    expect(watchdog(t_present=True, exit_present=True) == "observed-false",
           "(a) `_T` 在 `_C` 缺 EXIT 在 → ④（:1589）")


def test_watchdog_branch3_positive_and_negatives():
    positive = watchdog(t_present=False, c_present=False, pidof_absent=True,
                        exit_present=False, spawn_present=True,
                        capture_silent_after_spawn=True, death_observed=True,
                        inwait_confirmed="observed-true", death_site="P8")
    expect(positive == "observed-true",
           "③ 五合取正例：waiter 于 destroy 未及窗内死亡且已确认进入等待（:1585）")
    # (b) 无 BARRIER 且 inwait 非 true（drain 期死亡）→ ⑤（:1590）
    expect(watchdog(inwait_confirmed="observed-false", spawn_present=False)
           == unobs("marker-gap-indeterminate"),
           "(b) SPAWN 缺 → ⑤（:1590）")
    # (c) BARRIER 在而 inwait 非 true（BARRIER→poll 调用间隙死亡）→ ⑤（:1591）
    expect(watchdog(inwait_confirmed="observed-false")
           == unobs("marker-gap-indeterminate"),
           "(c) BARRIER 不再是 ③ 的 poll 进入证据 → ⑤（:1591）")
    # 死亡分量未确认（r12 反例，:1597）
    expect(watchdog(death_observed=False) == unobs("marker-gap-indeterminate"),
           "死亡分量 != observed-true → ⑤、签名条目不得证 true（:1597）")
    # 位点窗外（:891）
    expect(watchdog(death_site="P6") == unobs("marker-gap-indeterminate"),
           "死亡位点 ∉ P8-P10 → ⑤")
    # inwait 死亡收口支（r16 (d)，:1592-1594）：INWAIT 缺 → inwait 非 true → ⑤
    expect(watchdog(inwait_confirmed=unobs("inwait-marker-unobserved"))
           == unobs("marker-gap-indeterminate"),
           "(d) INWAIT 缺 → inwait 非 true → ③ 前件不成立落 ⑤（:1593）")


def test_watchdog_branch4_and_5_and_skip():
    expect(watchdog(exit_present=True, c_present=True, pidof_absent=False)
           == "observed-false", "④ EXIT 在 → observed-false（:890，唯一支）")
    expect(watchdog(exit_present=False, spawn_present=False, death_observed=False,
                    inwait_confirmed="observed-false")
           == unobs("marker-gap-indeterminate"),
           "⑤ 其余一切输入（默认支；禁止以 marker 缺失单独推断被杀，:891）")
    expect(watchdog(skip_cause="no-live-fd") == unobs("no-live-fd")
           and watchdog(skip_cause="dup-failed") == unobs("dup-failed"),
           "skip 具名清单位于表外、不进①-⑤（:894）")
    # 求值序①先于③：`_C` 在 + absent 时即使③合取齐也不得 true（:886）
    expect(watchdog(c_present=True, pidof_absent=True, death_site="P8")
           == unobs("destroy-terminal-candidate"), "先到先得：① 截胡 ③")


def test_marker_tail_state_five_values_and_boundary():
    # (a)/(b) 同源边界对：25000 不置位（严格大于）、25001 置位（:1601-1602）
    expect(death.eval_marker_tail_state(False, True, 125_000, 100_000)
           == "tail-loss-not-indicated", "(a) 静默跨度恰 25000 → 不置位（:1601）")
    expect(death.eval_marker_tail_state(False, True, 125_001, 100_000)
           == "possible-tail-loss", "(b) 静默跨度 25001 → 置位（:1602）")
    # (c) POST 在 → tail-complete（:1603）
    expect(death.eval_marker_tail_state(True, True, None, None) == "tail-complete",
           "(c) POST 在 → tail-complete（:1603）")
    # (d) POST 缺 + 有死亡证据 + 差值 ≤ T_tail（:1604）
    expect(death.eval_marker_tail_state(False, True, 120_000, 100_000)
           == "tail-loss-not-indicated", "(d) 差值 ≤ T_tail（:1604）")
    # (e) 任一墙钟不可求值（:1605）
    expect(death.eval_marker_tail_state(False, True, None, 100_000)
           == unobs("tail-clock-unresolvable"),
           "(e) 死亡墙钟缺失 → tail-clock-unresolvable（:1605）")
    expect(death.eval_marker_tail_state(False, True, 100_000, None)
           == unobs("tail-clock-unresolvable"), "(e) marker 墙钟缺失同支")
    # (f) 减法方向反例：死亡早于最后 marker → 差值恒负、永不置位（:1606）
    expect(death.eval_marker_tail_state(False, True, 90_000, 100_000)
           == "tail-loss-not-indicated", "(f) 反向差值永不置位（:1606）")
    # (g) POST 缺 + 无任何死亡证据 → no-death-evidence（:1607）
    expect(death.eval_marker_tail_state(False, False, None, None)
           == "no-death-evidence",
           "(g) F9 路径有值（无死亡证据即无减法输入，:1607）")


# ==========================================================================
# ⑫ 审查整改钉（M-03 / m-07 / m-10 / m-11）
# ==========================================================================

def test_hilog_stream_opens_before_start_entry():
    # M-03 钉：HilogStream 实际开流（executor.open_stream）先于 StartEntry
    #（规格 :1062/:1063「HilogStream 在 StartEntry 之前启动」）；FSM
    # start_hilog_stream 记账与真实开流同点位、先于 start-entry-issued。
    camp, _judged = judge("happy")
    ops = [r.op for r in camp["executor"].audit]
    expect("HilogStream" in ops and "StartEntry" in ops,
           "开流与 StartEntry 均在 executor 审计流中")
    expect(ops.index("HilogStream") < ops.index("StartEntry"),
           "实际开流先于 StartEntry（:1063）")
    kinds = [e.kind for e in camp["fsm"].events]
    expect(kinds.index("hilog-stream-started")
           < kinds.index("start-entry-issued"),
           "FSM 记账与真实开流同点位且先于 StartEntry 发出")


def test_d2_entry_outcome_last_wins():
    # m-07 钉：同 id 双 D2_ENTRY outcome 行 → 后到者为准
    expect(run.D2_ENTRY_OUTCOME_DOMAIN == frozenset((
        "resolved", "rejected", "timeout", "late-resolved", "late-rejected",
        "indeterminate", "not_attempted")),
        "m-07：outcome 值域七字面与规格 :511 一致")
    expect("后到者为准" in (run.d2_entry_final_outcomes.__doc__ or ""),
           "m-07：「后到者为准」规则随实现登记（判据未定，取最后一条 outcome）")

    def events_of(*messages):
        lines = [fake.format_hilog_line("entry", msg, 140 + i, 20001)
                 for i, msg in enumerate(messages)]
        return core.scan_markers(lines)
    # 主钉：timeout 后跟 late-resolved → 终值 late-resolved（后到者为准）
    expect(run.d2_entry_final_outcomes(events_of(
        "N1BDISC_D2_ENTRY|id=MR1|outcome=timeout",
        "N1BDISC_D2_ENTRY|id=MR1|outcome=late-resolved"))
        == {"MR1": "late-resolved"},
        "m-07：timeout 后跟 late-resolved → 终值 late-resolved")
    # 反例：后到者为准 ≠ late-* 优先（次序反转 → 终值 timeout）
    expect(run.d2_entry_final_outcomes(events_of(
        "N1BDISC_D2_ENTRY|id=MR1|outcome=late-resolved",
        "N1BDISC_D2_ENTRY|id=MR1|outcome=timeout"))
        == {"MR1": "timeout"},
        "m-07 反例：次序反转 → 终值 timeout（取最后一条，非 late-* 优先）")
    # attempted 行（无 outcome 键，:510）不参与取代；同 id 晚行覆盖、异 id 独立
    expect(run.d2_entry_final_outcomes(events_of(
        "N1BDISC_D2_ENTRY|id=MR1",
        "N1BDISC_D2_ENTRY|id=MR1|outcome=timeout",
        "N1BDISC_D2_ENTRY|id=MR2|outcome=late-rejected"))
        == {"MR1": "timeout", "MR2": "late-rejected"},
        "m-07：attempted 行不参与取代；同 id 后到覆盖、异 id 独立")
    expect(run.d2_entry_final_outcomes(events_of(
        "N1BDISC_D2_ENTRY|id=MR1")) == {},
        "m-07：仅 attempted 行 → 不入终值映射")


def test_faultrecv_path_traversal_rejected():
    # m-10 正例：纯文件名（FaultProbe basename 形态）照常拼装远端路径
    ok = hdc.build_argv("FaultRecv", {
        "fault_file": "faultlogger-0001-cn.alfadb.netbird.n1bdisc",
        "host_path": "/host/f1"}, target="T")
    expect(ok[-2] == hdc.FAULTLOGGER_DIR
           + "/faultlogger-0001-cn.alfadb.netbird.n1bdisc",
           "m-10 正例：合法条目名远端路径照常拼装")
    # m-10 反例：../ 穿越形态 / 绝对路径 / 路径分隔符 → 白名单侧拒绝
    for name in (
        "../cn.alfadb.netbird.n1bdisc",                # ../ 前缀穿越
        "cn.alfadb.netbird.n1bdisc/../../data/evil",   # 内嵌 .. 段
        "/data/evil/cn.alfadb.netbird.n1bdisc",        # 绝对路径
        "dir/cn.alfadb.netbird.n1bdisc",               # 嵌套分隔符
        "..\\cn.alfadb.netbird.n1bdisc",               # 反斜杠分隔符
        "cn.alfadb.netbird.n1bdisc/..",                # 尾部 .. 段
    ):
        try:
            hdc.build_argv("FaultRecv", {"fault_file": name, "host_path": "/h"},
                           target="T")
            raise SystemExit("m-10 穿越条目名未被拒: %r" % (name,))
        except hdc.HdcViolation as exc:
            expect(exc.reason == "path-separator-in-fault-file",
                   "m-10 拒因 = path-separator-in-fault-file（%r）" % (name,))
    expect(hdc.fault_file_traversal_reason("..") == "traversal-fault-file"
           and hdc.fault_file_traversal_reason("f-cn.alfadb.netbird.n1bdisc")
           is None,
           "m-10：裸 .. 父目录形态在纯函数面拒绝、合法名放行")


def test_faultrecv_traversal_partial_failure_branch():
    # m-10 钉：FaultProbe 差分出现穿越形态条目名 → 白名单侧拒绝按 FaultRecv
    # 部分失败支登记（recv_failures / failed_files 面），finally 序列不中断；
    # 同流携带 m-07 双 outcome 行；m-11 兜底在 dryrun 合成流下不触发。
    #（条目名取反斜杠穿越形态——runner 侧 FaultProbe basename 提取已剥离 ``/``
    # 形态，反斜杠形态是能到达 FaultRecv 的残余穿越面，:1213 快照差分口径。）
    traversal_name = "..\\evil-cn.alfadb.netbird.n1bdisc"
    spec = fake.FaultFileSpec(traversal_name,
                              fake.fault_entry_text("CPPCRASH", None))
    scenario = fake.FakeScenario(
        markers=[
            fake.FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
            fake.FakeMarker("D2_ENTRY", 140,
                            {"id": "MR1", "outcome": "timeout"}),
            fake.FakeMarker("D2_ENTRY", 141,
                            {"id": "MR1", "outcome": "late-resolved"}),
        ],
        fault_files=[spec], die_at_end=True)
    camp = run.run_dryrun_campaign("traversal-pin", scenario=scenario)
    judged = run.derive_and_judge(camp)
    expect(traversal_name in camp["recv_failures"],
           "m-10：穿越条目名按部分失败支登记（不中断 finally）")
    expect(traversal_name not in camp["transport"].received,
           "m-10：被拒条目名不进入取回结果")
    expect("8.absent-probes" in camp["finally_log"],
           "m-10：部分失败支不中断 host finally 全序列")
    expect(judged["d2_entry_final_outcomes"] == {"MR1": "late-resolved"},
           "m-07（campaign 面）：timeout 后跟 late-resolved → 终值 late-resolved")
    expect(camp["hilog_fallback_triggered"] is False,
           "m-11：dryrun 合成流即时完成，兜底不触发")


def test_hilog_wallclock_fallback_wiring():
    # m-11 钉：825 s 墙钟兜底常量接到流路径（外部计时检查点最小形态）
    expect(fsm.HILOG_WALLCLOCK_FALLBACK_S == 825,
           "兜底常量冻结值 825 s（fsm :121 / 规格 :1063）")
    expect(not run.hilog_wallclock_fallback_expired(0.0, 824.5),
           "825 s 未到 → 不熔断")
    expect(run.hilog_wallclock_fallback_expired(0.0, 825.0),
           "825 s 到点 → 熔断（>= 语义）")
    expect(run.hilog_wallclock_fallback_expired(100.0, 925.0),
           "自开流点起算的相对时长判定")
    # dryrun 三剧本合成流即时完成 → 兜底接线在、不误触发
    for name in ("happy", "pre-only", "no-live-fd"):
        camp, _judged = judge(name)
        expect(camp["hilog_fallback_triggered"] is False,
               "m-11：dryrun 剧本 %s 兜底不触发" % name)


# ==========================================================================
# E2E：CLI 主入口（薄转发 n1bdisc_cli 后更新——CLI 集成增量语义变更钉）
# ==========================================================================

def test_cli_dryrun_happy_end_to_end():
    # CLI 集成增量语义变更（只改本测试并说明）：--dryrun 不再走旧平行模拟器，
    # 统一经 n1bdisc_engine.run_campaign（同一 engine）；--skip-selftests 移除
    # （CLI 不再内嵌 selftest 步）；stdout 只给单行无敏摘要，完整最终记录在
    # run 目录 result.json（增量目录面）。门 11 头部断言保持等价强度。
    import json as _json
    import tempfile
    tmp = tempfile.mkdtemp(prefix="n1bfsm-cli-")
    try:
        root = os.path.join(tmp, "run")
        proc = subprocess.run(
            [sys.executable, run.__file__, "--dryrun", "--scenario", "happy",
             "--output-root", root],
            capture_output=True, text=True, timeout=120)
        expect(proc.returncode == 0, "CLI happy exit 0（stderr=%r）"
               % proc.stderr[-400:])
        record = _json.load(open(os.path.join(root, "result.json"),
                                 encoding="utf-8"))["result"]
        expect(record["mode"] == "dryrun" and record["is_evidence"] is False
               and record["integrity"] == {},
               "记录头部：is_evidence=false、integrity empty（门 11 原字面）")
        expect(record["verdict"] == "pass" and record["protocol"] == "complete",
               "CLI 记录：complete + pass")
        summary = _json.loads(proc.stdout.strip().splitlines()[-1])
        expect(summary["freeze_bound"] is False
               and summary["gate11_eligible"] is False,
               "无 manifest smoke 明确不具门 11 资格")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def test_cli_live_refused():
    # CLI 集成增量语义变更（只改本测试并说明）：--live 不再是打印 target 的
    # 骨架拒绝；缺完整冻结四件套在 argparse 层拒绝（exit 2 不变），target 永不
    # 回显（stdout/stderr 均不含），真实 transport 只有 preflight 全过才构造。
    proc = subprocess.run(
        [sys.executable, run.__file__, "--live", "--target",
         "CLI-E2E-TGT-2099", "--hap", "x"],
        capture_output=True, text=True, timeout=60)
    expect(proc.returncode == 2, "live 缺冻结四件套 → argparse exit 2")
    expect("CLI-E2E-TGT-2099" not in proc.stdout
           and "CLI-E2E-TGT-2099" not in proc.stderr,
           "target 不回显（新旧语义冲突点：旧实现打印 target，已按安全边界移除）")


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
    print("n1bdisc_fsm selftests: tests=%d passed=%d failed=%d assertions=%d (%.0f ms)"
          % (len(tests), passed, failed, _ASSERTS, elapsed * 1000))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
