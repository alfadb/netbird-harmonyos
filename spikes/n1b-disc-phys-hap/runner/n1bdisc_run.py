# -*- coding: utf-8 -*-
"""n1bdisc_run — N1BDISC host runner CLI 薄转发入口（正式 CLI 在 n1bdisc_cli）。

权威规格：``docs/n1b-disc-gate-plan.md``（只读冻结）。CLI 生产路径统一收敛到
:mod:`n1bdisc_cli`（``--live`` 冻结门 + ``--dryrun`` 同一 ``run_campaign``
engine，见其模块 docstring）；本模块 ``main`` 只做薄转发，``--help`` 语义与
退出码以 :mod:`n1bdisc_cli` 为准。既有派生 helpers（``run_dryrun_campaign`` /
``derive_and_judge`` / ``build_record`` / ``run_selftests``）保留给基线单测与
派生层复用，**不再是任何 CLI 生产路径**。

基线单测仍直接使用本模块 helpers（test_fsm/test_platform/test_engine）::

    camp = run_dryrun_campaign("happy")     # 旧平行模拟器：仅基线钉用
    judged = derive_and_judge(camp)
"""

from __future__ import annotations

import importlib.util
import io
import os
import re
import sys
import time
from contextlib import redirect_stdout
from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Tuple

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import n1bdisc_core as core                     # noqa: E402
import n1bdisc_fsm as fsm                       # noqa: E402
import n1bdisc_hdc as hdc                       # noqa: E402
import n1bdisc_death as death                   # noqa: E402
import n1bdisc_verdict as verdict               # noqa: E402
import n1bdisc_platform as platform             # noqa: E402
import fake_hdc as fake                         # noqa: E402

_RUNNER_DIR = os.path.dirname(os.path.abspath(__file__))
_SELFTEST_DIR = os.path.normpath(os.path.join(_RUNNER_DIR, os.pardir, "selftests"))

SCENARIOS = ("happy", "pre-only", "no-live-fd", "foreign-packet",
             "bad-payload", "not-attempted-timeout", "join-bogus", "d8b-bogus")


def _unbuffered() -> None:
    """PYTHONUNBUFFERED 支持（gate 13 口径：不因暂无终端输出中断）。"""
    if os.environ.get("PYTHONUNBUFFERED") != "1":
        try:
            sys.stdout.reconfigure(line_buffering=True)  # type: ignore[attr-defined]
        except (AttributeError, ValueError):
            pass


# ---------------------------------------------------------------------------
# gate 10 形态：selftests（test_core + test_fsm + test_platform）
# ---------------------------------------------------------------------------

def _load_selftest_module(name: str):
    path = os.path.join(_SELFTEST_DIR, name + ".py")
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise ImportError("cannot load selftest module: %s" % path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run_selftests() -> Dict[str, Any]:
    """运行三套 selftest 模块（pytest 不在场的自研 main 双兼容形态）。"""
    out: Dict[str, Any] = {}
    ok = True
    for name in ("test_core", "test_fsm", "test_platform"):
        buf = io.StringIO()
        try:
            module = _load_selftest_module(name)
            with redirect_stdout(buf):
                code = module.main()
            summary = [ln for ln in buf.getvalue().splitlines()
                       if "passed=" in ln]
            out[name] = {"exit": code, "summary": summary[-1] if summary else ""}
            ok = ok and code == 0
        except Exception as exc:  # noqa: BLE001 — gate 10 形态汇总失败面
            out[name] = {"exit": 1, "error": repr(exc)}
            ok = False
    return {"ok": ok, "modules": out}


# ---------------------------------------------------------------------------
# DryRun 组装（gate 11 形态）
# ---------------------------------------------------------------------------

def _marker_names(events) -> List[str]:
    return [e.name for e in events]


def _has(events, name: str) -> bool:
    return any(e.name == name for e in events)


def _find(events, name: str):
    return next((e for e in events if e.name == name), None)


def _rebuild_digest(transitions, cut: str):
    rebuild = core.rebuild_fd_ledger(transitions, cut)
    return rebuild


# ---------------------------------------------------------------------------
# m-07 / m-11 登记（runner 解析侧与流路径的整改钉）
# ---------------------------------------------------------------------------

# m-07 登记：``N1BDISC_D2_ENTRY`` 同 id 双 outcome 行的取代语义（runner 解析侧写死）。
# 规格 :474-479/:510-511：条目 timeout 后进入迟到观察窗（恰一次），窗内 resolve/reject
# 再落 ``late-resolved``/``late-rejected`` 结局 marker——同 id 因此可出现两条 outcome 行。
# 「两行并存时终值取哪条」判据在规格侧未定；本实现写死**后到者为准**：终值 = 同 id
# 最后一条 outcome 行的字面。只影响解析登记面，不驱动 verdict；判据定案后在此收口。
D2_ENTRY_OUTCOME_DOMAIN = frozenset((
    "resolved", "rejected", "timeout", "late-resolved", "late-rejected",
    "indeterminate", "not_attempted",
))


def d2_entry_final_outcomes(events) -> Dict[str, str]:
    """同 id 多条 ``N1BDISC_D2_ENTRY`` outcome 行 → 终值映射（后到者为准，m-07）。

    ``phase=attempted`` 形态（无 ``outcome`` 键，规格 :510）不参与取代；无 outcome
    行的 id 不入映射。事件次序 = capture 流次序（``scan_markers`` 保序）。
    """
    final: Dict[str, str] = {}
    for e in events:
        if e.name != "N1BDISC_D2_ENTRY":
            continue
        outcome = e.kv.get("outcome")
        if outcome is None:
            continue
        final[e.kv.get("id", "")] = outcome
    return final


def hilog_wallclock_fallback_expired(started_mono_s: float,
                                     now_mono_s: float) -> bool:
    """HilogStream 825 s 墙钟兜底检查点（规格 :1063-1064「runner 挂起时的保险丝」；
    ``fsm.HILOG_WALLCLOCK_FALLBACK_S``，m-11 接线登记）。

    观测窗（525 s，自首 marker 起算）**之外**的最后兜底：自 HilogStream 实际开流点
    （M-03 位点，先于 StartEntry）起算，到点（``>=``）即熔断。实参取
    ``time.monotonic()`` 读数（实测时长语义，防墙钟跳变）。dryrun/live 共用本
    检查点的流路径；dryrun 合成流即时完成，不触发。
    """
    return (now_mono_s - started_mono_s) >= fsm.HILOG_WALLCLOCK_FALLBACK_S


def run_dryrun_campaign(scenario_name: str,
                        scenario: Optional[fake.FakeScenario] = None
                        ) -> Dict[str, Any]:
    """一次 DryRun campaign：fake-hdc 全流程 + host finally + verdict。

    ``scenario`` 可注入自定义剧本（selftest 钉用）；缺省按 ``scenario_name``
    取内置三剧本之一。
    """
    builders = {
        "happy": fake.make_happy_path_scenario,
        "pre-only": fake.make_pre_only_scenario,
        "no-live-fd": fake.make_no_live_fd_scenario,
        # gate 3 整改反例剧本（B1/B2/B3/B4/B5 端到端反例钉）
        "foreign-packet": fake.make_foreign_packet_scenario,
        "bad-payload": fake.make_bad_payload_scenario,
        "not-attempted-timeout": fake.make_not_attempted_timeout_scenario,
        "join-bogus": fake.make_join_bogus_scenario,
        "d8b-bogus": fake.make_d8b_bogus_scenario,
    }
    scenario = scenario if scenario is not None else builders[scenario_name]()
    transport = fake.FakeHdc(scenario)
    executor = hdc.HdcExecutor(transport, target=transport.target,
                               hap_path=transport.hap_path)
    mon = fsm.ObservationWindowFSM()

    audit: List[str] = []

    def exec_op(op: str, **params) -> hdc.HdcCallRecord:
        record = executor.execute(op, **params)
        audit.append(op)
        return record

    # P0：operator-ready → HilogStream（实际开流先于 StartEntry）→ StartEntry
    #（:1053/:1062-1064）
    mon.register_operator_ready(0, 0)
    # gate 5 三探针 + pre-campaign 快照（:1621/:1213）
    exec_op("Version")
    exec_op("ParamModel")
    exec_op("ParamSoftwareVersion")
    snapshot_result = exec_op("FaultProbe")
    snapshot_files = sorted(ln.strip().rsplit("/", 1)[-1]
                            for ln in snapshot_result.result.stdout.splitlines()
                            if ln.strip())
    exec_op("MkdirStaging")
    exec_op("SendHap")
    exec_op("InstallHap")
    # M-03：HilogStream 实际开流（executor.open_stream）先于 StartEntry（规格
    # :1062/:1063「HilogStream 在 StartEntry 之前启动」）；FSM start_hilog_stream
    # 记账与真实开流同点位——开流成功即记账（开流失败不留记账）。
    stream_iter = executor.open_stream("HilogStream")
    # m-11：825 s 墙钟兜底自真实开流点起算（time.monotonic() 承载实测时长，防
    # 墙钟跳变；dryrun/live 共用流路径，见消费循环内逐行检查点）。
    stream_opened_mono_s = time.monotonic()
    mon.start_hilog_stream(1)
    exec_op("StartEntry")
    mon.issue_start_entry(2)

    # capture 流增量求值；首个 marker 后取 positive 基线（:1217）
    # m-11：825 s 墙钟兜底以「外部计时检查点」最小形态接入 dryrun/live 共用流路径
    #（规格 :1063-1064 观测窗之外的最后保险丝；fsm.HILOG_WALLCLOCK_FALLBACK_S）——
    # 逐行检查，触发即停采转 host finally 收口；dryrun 合成流即时完成不触发，
    # 触发事实登记 hilog_fallback_triggered。
    hilog_fallback_triggered = False
    lines: List[str] = []
    positive_baseline_pid: Optional[str] = None
    for line in stream_iter:
        if hilog_wallclock_fallback_expired(stream_opened_mono_s,
                                            time.monotonic()):
            hilog_fallback_triggered = True   # 兜底熔断（m-11）：停采，不再接受流行
            break
        lines.append(line)
        events_now = core.scan_markers([line])
        if not events_now:
            continue
        if positive_baseline_pid is None:
            # positive 基线（:1217）：首个 N1BDISC_ marker 出现之后恰一次 PidOfVpn
            baseline = exec_op("PidOfVpn")
            positive_baseline_pid = baseline.result.stdout.strip() or None
        for e in events_now:
            mon.feed_marker(e.name, 0)
    stream_events = core.scan_markers(lines)

    # host finally（:1645-1662，顺序冻结）
    finally_log: List[str] = []
    finally_log.append("1.stop-hilogstream")          # 步 1（流已随生成器耗尽停止）
    fault_result = exec_op("FaultProbe")              # 步 2
    current_files = sorted(ln.strip().rsplit("/", 1)[-1]
                           for ln in fault_result.result.stdout.splitlines()
                           if ln.strip())
    new_files = death.snapshot_diff(snapshot_files, current_files)
    received: Dict[str, str] = {}
    recv_failures: List[str] = []
    for name in new_files:
        try:
            record = exec_op("FaultRecv", fault_file=name,
                             host_path="/host/fake/faultlogger/" + name)
        except hdc.HdcViolation:
            # m-10：条目名穿越/违规在白名单侧（build_argv）拒绝 → 按 FaultRecv
            # 部分失败支登记（failed_files 面，:1523-1525），不中断 finally 序列。
            recv_failures.append(name)
            continue
        if record.result.exit_code == 0:
            received[name] = transport.received[name]
        else:
            recv_failures.append(name)
    finally_log.append("2.faultprobe+faultrecv")
    pidof_final = exec_op("PidOfVpn")                 # 步 3（ForceStop 之前）
    pidof_absent = pidof_final.result.stdout.strip() == ""
    finally_log.append("3.pidofvpn-sample")

    # 步 4：死亡事实冻结（七分量；ForceStop 之前，:1365）
    fault_probe_failed = fault_result.result.exit_code != 0
    parses = [death.parse_fault_entry(name, text)
              for name, text in sorted(received.items())]
    sites = [fsm.site_of(e.name) for e in stream_events
             if fsm.site_of(e.name) is not None]
    last_site = death.eval_last_visible_site(sites)
    skip_destroy = _has(stream_events, "N1BDISC_SKIP") and any(
        e.name == "N1BDISC_SKIP" and e.kv.get("item") == "destroy"
        for e in stream_events)
    t_present = _has(stream_events, "N1BDISC_DW_DESTROY_T")
    c_present = _has(stream_events, "N1BDISC_DW_DESTROY_C")
    destroy_state = death.eval_destroy_call_state(skip_destroy, t_present, c_present)
    components = death.aggregate_fault_components(
        parses, failed_files=recv_failures, fault_probe_failed=fault_probe_failed,
        last_visible_site=last_site, marker_seq_ok=not _skip_anchor_conflict(stream_events))
    death_observed_value = death.eval_process_death(
        positive_baseline=positive_baseline_pid is not None,
        pidof_absent=pidof_absent,
        capture_silent=True)   # DryRun capture = 全流合成，流末静默可判
    death_observed = death_observed_value == "observed-true"
    post_present = _has(stream_events, "N1BDISC_POST")
    tail_state = death.eval_marker_tail_state(
        post_present=post_present,
        death_evidence_present=death_observed,
        death_wall_ms=100_000 if death_observed else None,
        last_marker_wall_ms=80_000 if sites else None)
    signature = death.eval_probe_crash_signature(
        components.fault_type_observed, components.signal_observed,
        last_site, marker_seq_ok=not _skip_anchor_conflict(stream_events))
    finally_log.append("4.death-facts-frozen")

    # 步 5-8：清理与 absent 复核
    exec_op("ForceStop", reason="final-cleanup")
    finally_log.append("5.forcestop")
    exec_op("Uninstall")
    finally_log.append("6.uninstall")
    exec_op("RemoveStaging")
    finally_log.append("7.removestaging")
    absent_checks = {
        "bundle_dump_absent": exec_op("BundleDump").result.exit_code != 0,
        "pidof_post_empty": exec_op("PidOfPost").result.stdout.strip() == "",
        "pidof_vpn_post_empty": exec_op("PidOfVpnPost").result.stdout.strip() == "",
        "staging_probe_absent": exec_op("StagingProbe").result.exit_code != 0,
    }
    verified_clean = all(absent_checks.values())
    finally_log.append("8.absent-probes")

    # 步 9：终态判定与封签（观测窗四分支同表，:1658-1661）
    close_kind = mon.close_by_host_finally(90_000, death_observed).detail \
        if not mon.closed else mon.close_kind
    finally_log.append("9.terminal-judgement:" + str(close_kind))
    finally_log.append("10.integrity-close")          # 步 10（integrity empty）

    # B4-c：runner 侧 join 事实登记（:720/:1047——join-blocked-observed 由 runner
    # 依轮询超时登记、join rc 为 runner 侧 pthread_join 返回登记；DryRun 由剧本
    # 声明这些 runner 侧观测，live 形态由 fsm 轮询路径供给同一布尔/整数）。
    join_facts = {
        "join_blocked_registered": bool(getattr(
            scenario, "join_blocked_registered", False)),
        "join_exit_rc": getattr(scenario, "join_exit_rc", None),
    }

    return {
        "scenario": scenario_name,
        "transport": transport,
        "executor": executor,
        "fsm": mon,
        "stream_events": stream_events,
        "lines": lines,
        "snapshot_files": snapshot_files,
        "new_files": new_files,
        "recv_failures": recv_failures,
        "parses": parses,
        "components": components,
        "signature": signature,
        "sites": sites,
        "last_site": last_site,
        "destroy_state": destroy_state,
        "death_observed": death_observed,
        "death_observed_value": death_observed_value,
        "tail_state": tail_state,
        "positive_baseline_pid": positive_baseline_pid,
        "pidof_absent": pidof_absent,
        "close_kind": close_kind,
        "hilog_fallback_triggered": hilog_fallback_triggered,
        "finally_log": finally_log,
        "absent_checks": absent_checks,
        "verified_clean": verified_clean,
        "audit_ops": audit,
        "join_facts": join_facts,
    }


def _skip_anchor_conflict(events) -> bool:
    """SKIP|item=destroy 与调用锚同现域门（:1296）+ `_C` 在 `_T` 缺（五态第 5 行）。"""
    skip = any(e.name == "N1BDISC_SKIP" and e.kv.get("item") == "destroy"
               for e in events)
    t_present = _has(events, "N1BDISC_DW_DESTROY_T")
    c_present = _has(events, "N1BDISC_DW_DESTROY_C")
    return (skip and (t_present or c_present)) or (c_present and not t_present)


@dataclass(frozen=True)
class CaptureWallFacts:
    """capture/证据侧真实墙钟事实（毫秒；engine/live 传入，DryRun 缺省合成对）。

    - ``death_wall_ms``：死亡证据墙钟（engine = 快照差分新增 fault 条目内首个
      可解析 ``YYYY-MM-DD hh:mm:ss.mmm`` 设备侧时间戳；原 DryRun 合成 100_000）；
    - ``begin_capture_wall_ms``：``D8_STORM_BEGIN`` 捕获行设备侧墙钟
      （原 DryRun 合成 90_000）。

    两个值只用于同源设备侧差值（D8b ``storm_incomplete_pre_only_span``、
    ``marker_tail_state`` 静默跨度）；任一为 ``None`` 时由既有冻结求值落
    ``tail-clock-unresolvable`` / :607 强制跨度 F4 面——engine 绝不携带合成值进
    Live，也不新造 cause。
    """

    death_wall_ms: Optional[int] = None
    begin_capture_wall_ms: Optional[int] = None


#: DryRun 既有合成约定（is_evidence=false）：死亡 100_000、BEGIN capture 90_000
#: （介于死亡与最后可见 marker 80_000 之间）——``derive_and_judge`` 缺省保持原
#: DryRun 行为不回归；engine/live 显式传真实 :class:`CaptureWallFacts`。
_DRYRUN_WALL_FACTS = CaptureWallFacts(death_wall_ms=100_000,
                                      begin_capture_wall_ms=90_000)


def _final_rebuild_cut(close_kind: Optional[str]) -> str:
    """最终 ledger 重建切点 = 收口形态驱动（观察 (i) 整改钉；判据 :385/:402-404）。

    complete-seal → ``complete``（仍 open 条目记 ``open-at-exit``——与探针 POST
    digest 的切口同源：探针 POST 恒按 ``ledger::digest(false)`` 即 open-at-exit
    计算，probe/src/dw.rs:1006，与 ``worker_terminal_at_p12`` FLAG 读值无关）；
    ``pre-only`` 收口 → ``pre-only``（``process-exit``）；fail-cleanup（观测窗到点
    ForceStop 前进程仍活）→ ``host-forcestop``。其余完整性失败收口（PRE 缺/Allow
    盒已消费）无合法终值形态可比，沿 complete 形态切口登记。
    """
    if close_kind == fsm.CLOSE_PRE_ONLY:
        return "pre-only"
    if close_kind == fsm.CLOSE_FAIL_F9:
        return "host-forcestop"
    return "complete"


def derive_and_judge(camp: Dict[str, Any],
                     *, wall_facts: Optional[CaptureWallFacts] = None,
                     freeze_integrity_failures: Optional[List[str]] = None
                     ) -> Dict[str, Any]:
    """capture 全流解析（chunk/ledger/终态字段/全序/dw 比对）+ verdict 主求值。

    ``wall_facts``：capture/证据侧真实墙钟事实（engine/live 路径显式传入；
    ``None`` = DryRun 既有合成对，原 CLI 行为逐字不变）。
    ``freeze_integrity_failures``：live freeze 收口复核失败码（engine 步 10
    integrity 收尾传入；``None``/空 = 无复核失败，DryRun 恒空）。非空时走既有
    verdict invalid 优先轴（:1124 ``freeze-integrity``），不新造判据/cause。
    """
    walls = wall_facts if wall_facts is not None else _DRYRUN_WALL_FACTS
    events = camp["stream_events"]
    # m-07：同 id 双 D2_ENTRY outcome 行按「后到者为准」收敛终值（登记见
    # d2_entry_final_outcomes；判据未定，只登记不驱动 verdict）。
    d2_entry_final = d2_entry_final_outcomes(events)
    gaps_f8: List[str] = []
    gaps_f4: List[str] = []
    order_violations: List[str] = []

    # chunk 重组（增量落盘缺项 → F4 面；重组失败 → F8(2) 解析域缺口，:1128/:1377）
    chunks = core.reassemble_chunks([e for e in events if e.name == "N1BDISC_CHUNK"])
    chunk_failures = [f.reason for f in chunks.failures]
    gaps_f4 += ["chunk-group: %s" % r for r in chunk_failures
                if r in ("coverage-gap", "sha256-mismatch")]

    # ledger 同切点一致性（:1200-1203；P5T 快照 = 流内先于 PRE 的 transition 快照，:435）
    transitions = [e.kv for e in events if e.name == "N1BDISC_FD"]
    fd_events = [e for e in events if e.name == "N1BDISC_FD"]
    pre = _find(events, "N1BDISC_PRE")
    post = _find(events, "N1BDISC_POST")
    ledger_failures: List[str] = []
    ledger_report: Dict[str, Any] = {}
    pre_rebuild = None
    final_rebuild = None
    if pre is not None:
        at = [e.kv for e in fd_events if e.line_no < pre.line_no]
        pre_rebuild = _rebuild_digest(at, "pre-snapshot")
        if pre_rebuild.ok and pre.kv.get("ledger_digest") != pre_rebuild.digest:
            ledger_failures.append("PRE ledger_digest != P5T snapshot rebuild")
        elif not pre_rebuild.ok:
            ledger_failures.append("P5T snapshot rebuild failed")
    # 最终重建切点由 close_kind（收口形态）驱动，**非** POST 的
    # ``worker_terminal_at_p12`` FLAG 读值（观察 (i) 整改钉；判据 :390-395「digest
    # 一致性校验限同一切点」）：真机 flag-race/join-timeout 格 POST 在而 FLAG=false，
    # 探针 POST digest 恒按 open-at-exit 切口算（dw.rs:1006），runner 若按 FLAG 选
    # pre-only 切口重建，含 open 条目的账本必然跨切点不等 → 合法 campaign 假 F5。
    # wtap 仅作登记字段保留（POST 冻结字段集与 cut-state (B) 校验仍消费，:440）。
    if post is not None:
        final_rebuild = _rebuild_digest(
            transitions, _final_rebuild_cut(camp["fsm"].close_kind))
        if final_rebuild.ok and post.kv.get("ledger_digest") != final_rebuild.digest:
            ledger_failures.append("POST ledger_digest != final rebuild")
        elif not final_rebuild.ok:
            ledger_failures.append("final rebuild failed")
    elif camp["fsm"].close_kind in (fsm.CLOSE_PRE_ONLY, fsm.CLOSE_FAIL_F9):
        # POST 缺的收口（pre-only / fail-cleanup）：探针来不及发最终 digest
        #（:389）——runner 重建并携带于记录（pre-only 逐字标注
        # rebuilt-from-transition-marker，LedgerRebuild 承载）；无 POST digest
        # 在场即无对端值，不做一致性比较（:390-395 限同切点、禁止跨切点比较）。
        final_rebuild = _rebuild_digest(
            transitions, _final_rebuild_cut(camp["fsm"].close_kind))

    # PRE/POST 冻结字段集（:435/:437）
    frozen_missing: List[str] = []
    if pre is not None:
        frozen_missing += ["PRE." + k for k in ("ledger_digest", "skip_summary")
                           if k not in pre.kv]
    if post is not None:
        frozen_missing += ["POST." + k for k in
                           ("d6_items", "dw_outcome", "ledger_digest",
                            "worker_terminal_at_p12") if k not in post.kv]

    # 全序/时序（F3）
    order_violations += fsm.stage_order_violations(camp["sites"])
    order_violations += _mono_order_violations(events)
    if _skip_anchor_conflict(events):
        order_violations.append("SKIP|item=destroy 与调用锚同现域门 / _C 在 _T 缺")

    # dw_return_class 重建 vs P12 派生（F8(2)，:1559）+ cut-state（:772-789）
    rebuilt_class, post_class, cut_report = _dw_class_pipeline(events)
    # POST dw_outcome 解析失败（探针契约外形态）→ 解析域缺口（fail-closed，F8(2) 轴）
    if post is not None and not cut_report["parse_ok"]:
        gaps_f8.append("POST dw_outcome parse failed: probe actual format contract "
                       "';'-separated k=v (probe/src/dw.rs:1008-1018)")
    if post_class is not None and rebuilt_class is not None:
        if not verdict.p12_class_consistency(rebuilt_class, post_class):
            gaps_f8.append("P12-derived dw_return_class=%r != runner rebuild %r"
                           % (post_class, rebuilt_class))
    cut_report_violations = []
    if cut_report.get("cut_false"):
        cut_report_violations = verdict.cut_state_b_violations(
            True, cut_report["class"], cut_report["poll_raw"],
            cut_report["watchdog"], cut_report["racewin_present"])
    elif cut_report.get("jt1"):
        cut_report_violations = verdict.cut_state_a_violations(
            _has(events, "N1BDISC_DW_RETURN"), _has(events, "N1BDISC_DW_EXIT"))
    gaps_f8 += cut_report_violations

    # dw_join_result 完整重建（分期项 10：单一权威派生入口，规格 :870/:693）+
    # P12 内层 join= 严格十值域比对（B4-b：pending 不再是合法形态；B4-c：
    # join_blocked_registered/exit_rc 自剧本/fsm 登记路径真实接线，:720/:1047）
    d6b_skip_present = any(e.name == "N1BDISC_SKIP" and e.kv.get("item") == "D6b"
                           for e in events)
    join_facts = camp.get("join_facts") or {}
    join_input = core.DwJoinInput(
        dw_skip_cause=platform.dw_skip_cause_of(events),
        destroy_call_state=camp["destroy_state"],
        d6b_skip_present=d6b_skip_present,
        # runner 侧 JT 登记：DryRun/重建以 capture 中 D6b skip 为同一载体的
        # 登记事实（规格 :693 括注「或 join-timeout-worker-abandoned=true 已
        # 登记」——两前件等价，真机 runner 侧轮询登记落同一布尔）。
        join_timeout_registered=d6b_skip_present,
        # B4-c 真实接线（:720/:1047）：runner 依轮询超时登记 join 阻塞 /
        # runner 侧 pthread_join 返回值登记（0=joined、ESRCH、other+errno）。
        join_blocked_registered=bool(join_facts.get("join_blocked_registered",
                                                    False)),
        exit_rc=join_facts.get("join_exit_rc"),
        exit_present=_has(events, "N1BDISC_DW_EXIT"),
        post_present=post is not None,
        death_observed=camp["death_observed"],
    )
    join_rebuilt = core.derive_dw_join_result(join_input)
    rebuilt_join = join_rebuilt.value
    if join_rebuilt.outcome == "fail":
        gaps_f8.append("dw_join_result rebuild truth-table gap: %s"
                       % join_rebuilt.fail_reason)
        rebuilt_join = None
    # POST join 值：十值域闭域检查**恒在**（T0 裁定 join-bogus 回归整改）——
    # POST 在场时 pending/未知/缺失一律 F8(2)；no-fact 只免除「重建 vs P12」
    # 比对面，不免除域检查。比对仍仅在 runner 侧有独立 join 事实（rebuilt 非
    # None）时进行；EXIT 不喂 join、join_exit_rc None、join_blocked False 保持。
    post_join: Optional[str] = None
    if post is not None:
        post_join = verdict.parse_dw_outcome(post.kv.get("dw_outcome", "")).get("join")
        if post_join is None or post_join not in core.DW_JOIN_RESULT_10:
            gaps_f8.append(
                "P12 dw_join_result=%r outside DW_JOIN_RESULT_10 closed domain"
                " (probe contract ;'-separated k=v, dw.rs:1008-1018)"
                % (post_join,))
        elif rebuilt_join is not None and not verdict.p12_join_consistency(
                rebuilt_join, post_join, post_class):
            gaps_f8.append("P12 dw_join_result=%r != runner rebuild %r"
                           % (post_join, rebuilt_join))

    # u1-u7 平台分量派生（分期项 9；F8/F4 facet 交 verdict 承载）
    platform_components = platform.derive_platform_components(
        events=events, chunks=chunks,
        death_observed=camp["death_observed"],
        last_site=camp["last_site"], tail_state=camp["tail_state"],
        post_present=post is not None,
        destroy_call_state=camp["destroy_state"])
    gaps_f8 += platform_components["f8_facets"]
    gaps_f4 += platform_components["f4_facets"]

    # POST d6_items 七子项解析与核对（B7：非仅外层存在；:439-440 三选一编码、
    # :1128 任一 D 项终态 missing → fail；值内 ``|`` 经 producer
    # sanitize_marker_field 转义，消费侧先还原——core.unescape_marker_field）
    d6_items_report: Dict[str, Any] = {"items": None, "f4": [], "f8": []}
    if post is not None:
        d6_raw = core.unescape_marker_field(post.kv.get("d6_items", ""))
        items, d4g, d8g = parse_d6_items(d6_raw)
        d8g += _d6_items_cross_check(events, items)
        d6_items_report = {"items": {k: dict(v) for k, v in items.items()},
                           "f4": d4g, "f8": d8g}
        gaps_f4 += ["d6-items: %s" % f for f in d4g]
        gaps_f8 += ["d6-items: %s" % f for f in d8g]

    # D8b 收口三分流（分期项 11：BEGIN-only / 阶段未达 / 存活未完成 F9 面）
    storm_begin = _find(events, "N1BDISC_D8_STORM_BEGIN")
    storm_end = _find(events, "N1BDISC_D8_STORM_END")
    last_site_idx = (fsm.SITE_ORDER.index(camp["last_site"])
                     if camp["last_site"] in fsm.SITE_ORDER else None)
    d8b_closure = death.eval_d8b_closure(
        begin_kv=storm_begin.kv if storm_begin is not None else None,
        end_kv=storm_end.kv if storm_end is not None else None,
        death_observed=camp["death_observed"],
        death_site_before_storm=(last_site_idx is not None
                                 and last_site_idx < fsm.SITE_ORDER.index("P7")),
        # 墙钟事实：DryRun 沿既有合成约定（is_evidence=false，上方注释）；engine/
        # live 经 wall_facts 传真实捕获/证据侧读数（不携带 100_000/90_000 进 Live）。
        death_wall_ms=walls.death_wall_ms if camp["death_observed"] else None,
        begin_capture_wall_ms=walls.begin_capture_wall_ms
        if storm_begin is not None else None,
        skip_cause=platform.dw_skip_cause_of(events))
    gaps_f4 += ["d8b-increment-gap: %s" % f for f in d8b_closure["f4_missing"]]
    gaps_f8 += d8b_closure["f8_facets"]

    # 时间字段不可解析条目 → 解析域缺口（:1242；Fault_Type/Signal 豁免，:1378）
    gaps_f8 += ["fault-entry-time-unparsable: %s" % name
                for name in camp["components"].time_gap_files]

    # B6：f4 facet 按「unregistered-cause:」前缀两分（:1373-1380）——未预注册
    # cause 的缺口送 unregistered_cause_hits（F4 (4)），其余为增量落盘缺项。
    unregistered_causes = [f[len("unregistered-cause: "):].strip()
                           for f in gaps_f4 if f.startswith("unregistered-cause:")]
    gaps_f4_plain = [f for f in gaps_f4
                     if not f.startswith("unregistered-cause:")]

    v_input = verdict.VerdictInput(
        pre_present=pre is not None,
        post_present=post is not None,
        crash_signature=camp["signature"].value,
        order_violations=order_violations,
        frozen_field_missing=frozen_missing,
        increment_gaps=gaps_f4_plain,
        unregistered_cause_hits=unregistered_causes,
        ledger_failures=ledger_failures,
        d1_cmdline_ok=True if _has(events, "N1BDISC_D1_BEGIN") else None,
        parse_domain_gaps=gaps_f8,
        window_expired_alive=(camp["fsm"].close_kind == fsm.CLOSE_FAIL_F9),
        freeze_integrity_failures=list(freeze_integrity_failures or ()),
    )
    result = verdict.evaluate(v_input)

    # complete + 崩溃签名 → runner evidence 记录观察（:1136，不驱动 verdict）
    if result.verdict == "pass" and camp["signature"].value == "observed-true":
        result.details.setdefault("abnormal-observations", []).append(
            "crash-signature observed-true under complete (需 N1b 关注的异常观察)")

    ledger_report = {
        "pre_digest": pre.kv.get("ledger_digest") if pre else None,
        "pre_rebuild_digest": pre_rebuild.digest if pre_rebuild else None,
        "final_digest": post.kv.get("ledger_digest") if post else None,
        "final_rebuild_digest": final_rebuild.digest if final_rebuild else None,
        "pre_rebuild_ok": pre_rebuild.ok if pre_rebuild else None,
        "final_rebuild_ok": final_rebuild.ok if final_rebuild else None,
        # :389 pre-only 收口重建逐字标注（runner 侧携带的最终重建值）
        "final_rebuild_from_transition_marker":
            final_rebuild.rebuilt_from_transition_marker if final_rebuild else None,
        "final_rebuild_cut": _final_rebuild_cut(camp["fsm"].close_kind),
    }
    return {
        "verdict_result": result,
        "d2_entry_final_outcomes": d2_entry_final,
        "ledger": ledger_report,
        "dw": {"rebuilt_class": rebuilt_class, "post_class": post_class,
               "rebuilt_join": rebuilt_join, "post_join": post_join,
               "join_outcome": join_rebuilt.outcome,
               "join_sticky": join_rebuilt.sticky,
               "join_facts": join_facts,
               "cut_report": cut_report},
        "platform_components": platform_components,
        "d6_items": d6_items_report,
        "d8b_closure": d8b_closure,
        "chunks": {"failures": chunk_failures,
                   "observations": [dict(o) for o in chunks.observations]},
    }


def _site_mono(events, site: str) -> int:
    ev = next((e for e in events if fsm.site_of(e.name) == site), None)
    return int(ev.kv["at_mono_ms"]) if ev and "at_mono_ms" in ev.kv else 0


def _mono_order_violations(events) -> List[str]:
    """``_T.mono_ms ≤ _C.mono_ms`` 顺序约束（:830；违反归 F3）。"""
    t = _find(events, "N1BDISC_DW_DESTROY_T")
    c = _find(events, "N1BDISC_DW_DESTROY_C")
    if t and c and int(t.kv["mono_ms"]) > int(c.kv["mono_ms"]):
        return ["DW_DESTROY_T.mono_ms > DW_DESTROY_C.mono_ms"]
    return []


#: POST ``d6_items`` 七子项三选一编码的 cause 闭域（B7）。判据 :439-440 冻结
#: skipped cause 集 {no-live-fd, dup-failed, join-timeout-abandoned}；producer
#: 另沿预注册 skip 位点发射 barrier-never-observed（:717）/destroy-unresolved
#: （:1046 P9 行「跳过 D6a」）/step-not-executed（d6.rs:700/715 不可达兜底）——
#: 按「producer 实际发射语义」契约接受并登记遗留偏差；域外 cause → F8。
D6_ITEMS_SKIPPED_CAUSES = frozenset({
    "no-live-fd", "dup-failed", "join-timeout-abandoned",
    "barrier-never-observed", "destroy-unresolved",
})
D6_ITEMS_UNOBSERVABLE_CAUSES = frozenset({"step-not-executed"})
_D6_ITEMS_ITEM_RE = re.compile(r"\AD6S([1-7])\Z")
_D6_ITEMS_RESULT_RE = re.compile(r"\Aresult\|ret=(-?[0-9]+)\|errno=(-?[0-9]+)\Z")
_D6_ITEMS_REUSE_RE = re.compile(r"\Aresult\|fd=(-?[0-9]+)\|reuse=(true|false)\Z")
_D6_ITEMS_SKIPPED_RE = re.compile(r"\Askipped\(cause=([^()|]+)\)\Z")
_D6_ITEMS_UNOBS_RE = re.compile(r"\Aunobservable\(cause=([^()|]+)\)\Z")


def parse_d6_items(d6_items: str) -> Tuple[Optional[Dict[int, Dict[str, str]]],
                                           List[str], List[str]]:
    """POST ``d6_items`` 七子项解析（B7；producer dw.rs:665-743 实际格式）。

    格式 = ``;`` 分隔的 ``D6S<n>=<value>``，value 三选一编码（:439-440）：
    ``result|ret=<int>|errno=<int>``（S7：``result|fd=<int>|reuse=<bool>``）/
    ``skipped(cause=<c>)`` / ``unobservable(cause=<c>)``。返回
    ``(items, f4, f8)``：七子项须恰各一次（缺/重复 → F4，:1128/:439「缺任一
    冻结字段 → fail」）；编码形态/cause 域外 → F8（解析域缺口）。
    """
    items: Dict[int, Dict[str, str]] = {}
    f4: List[str] = []
    f8: List[str] = []
    for part in d6_items.split(";"):
        key, eq, value = part.partition("=")
        m = _D6_ITEMS_ITEM_RE.match(key)
        if not eq or m is None:
            f8.append("d6_items part %r not 'D6S<n>=<value>' (dw.rs:665-743)"
                      % (part,))
            continue
        idx = int(m.group(1))
        if idx in items:
            f4.append("d6_items D6S%d duplicated" % idx)
            continue
        rm = _D6_ITEMS_RESULT_RE.match(value)
        um = _D6_ITEMS_REUSE_RE.match(value)
        sm = _D6_ITEMS_SKIPPED_RE.match(value)
        om = _D6_ITEMS_UNOBS_RE.match(value)
        if rm and idx != 7:
            items[idx] = {"kind": "result", "ret": rm.group(1),
                          "errno": rm.group(2)}
        elif um and idx == 7:
            items[idx] = {"kind": "result", "fd": um.group(1),
                          "reuse": um.group(2)}
        elif sm and sm.group(1) in D6_ITEMS_SKIPPED_CAUSES:
            items[idx] = {"kind": "skipped", "cause": sm.group(1)}
        elif om and om.group(1) in D6_ITEMS_UNOBSERVABLE_CAUSES:
            items[idx] = {"kind": "unobservable", "cause": om.group(1)}
        else:
            f8.append("d6_items D6S%d=%r outside three-choice encoding "
                      "(spec :439-440; producer dw.rs:690-740)" % (idx, value))
    for idx in range(1, 8):
        if idx not in items and not any("D6S%d" % idx in f for f in f8):
            f4.append("d6_items D6S%d missing (frozen field, :439/:1128)" % idx)
    return items, f4, f8


def _d6_items_cross_check(events, items: Mapping[int, Mapping[str, str]]
                          ) -> List[str]:
    """d6_items 子项编码与 ``D6Sx_R`` marker 字段交叉核对（B7；F8 面）。

    result 编码 ↔ result marker 逐字段相等（ret/errno；S7 fd/reuse）；
    skipped/unobservable 编码 ↔ result marker 缺席（同现 = capture 自相矛盾）。
    """
    gaps: List[str] = []
    for idx in range(1, 8):
        item = items.get(idx)
        if item is None:
            continue
        marker = _find(events, "N1BDISC_D6S%d_R" % idx)
        if item["kind"] == "result":
            if marker is None:
                gaps.append("d6_items D6S%d=result but no D6S%d_R marker"
                            % (idx, idx))
                continue
            if idx == 7:
                if marker.kv.get("fd") != item["fd"] \
                        or marker.kv.get("reuse") != item["reuse"]:
                    gaps.append("d6_items D6S7 fd/reuse != D6S7_R fields "
                                "(%r/%r vs %r/%r)" % (item["fd"], item["reuse"],
                                                      marker.kv.get("fd"),
                                                      marker.kv.get("reuse")))
            else:
                if marker.kv.get("ret") != item["ret"] \
                        or marker.kv.get("errno") != item["errno"]:
                    gaps.append("d6_items D6S%d ret/errno != D6S%d_R fields "
                                "(%r/%r vs %r/%r)"
                                % (idx, idx, item["ret"], item["errno"],
                                   marker.kv.get("ret"), marker.kv.get("errno")))
        elif marker is not None:
            gaps.append("d6_items D6S%d=%s but D6S%d_R marker present "
                        "(capture self-contradiction)" % (idx, item["kind"], idx))
    return gaps


def _dw_class_pipeline(events) -> Tuple[Optional[str], Optional[str], Dict[str, Any]]:
    """runner 重建 dw_return_class + POST 派生值 + cut-state 输入报告。

    B4 整改注：cut-state (A)/(B) 仅施于 **JT=1 竞态族**（:773「capture 有
    ``SKIP|item=D6b``」——D-W 整体 skip 分支的 D6b skip 属 (a) skip 表全量指派
    （:737 ``(a) 先行``，其 D6b 行不入竞态族），jt_registered 从未置位 →
    同 :789 (C) 竞态族外——skip 分支走 skip 字面重建比对、不进 cut-state 闭表）。
    """
    ret = _find(events, "N1BDISC_DW_RETURN")
    post = _find(events, "N1BDISC_POST")
    skip_destroy = any(e.name == "N1BDISC_SKIP" and e.kv.get("item") == "destroy"
                       for e in events)
    dw_skip_marker = next((e for e in events
                           if e.name == "N1BDISC_SKIP"
                           and e.kv.get("item") == "D-W"), None)
    dw_skip_cause = dw_skip_marker.kv.get("cause") \
        if dw_skip_marker is not None else None
    t_present = _has(events, "N1BDISC_DW_DESTROY_T")
    c_present = _has(events, "N1BDISC_DW_DESTROY_C")
    drain = _find(events, "N1BDISC_DW_DRAIN")
    rebuilt: Optional[str] = None
    if dw_skip_cause in ("no-live-fd", "dup-failed"):
        # (a) skip 表全量指派（:734）：runner 重建 = skip 字面逐字。
        rebuilt = core.unobservable_value(dw_skip_cause)
    elif ret is not None:
        result = core.derive_dw_return_class(core.DwReturnInput(
            ret=int(ret.kv["ret"]),
            revents=int(ret.kv["revents"]),
            errno=int(ret.kv["errno"]) if ret.kv.get("errno") not in (None, "none") else None,
            at_mono_ms=int(ret.kv["at_mono_ms"]),
            elapsed_ms=int(ret.kv["elapsed_ms"]),
            drain_end=(drain.kv.get("end") if drain else None),
            has_skip_destroy=skip_destroy,
            has_destroy_c=c_present,
            t_mono_ms=int(_find(events, "N1BDISC_DW_DESTROY_T").kv["mono_ms"])
            if t_present else None,
            c_mono_ms=int(_find(events, "N1BDISC_DW_DESTROY_C").kv["mono_ms"])
            if c_present else None,
        ))
        rebuilt = result.value if result.outcome == "class" else "fail:" + str(result.fail_reason)
    post_class: Optional[str] = None
    cut_report: Dict[str, Any] = {"jt1": False, "cut_false": False, "parse_ok": True}
    if post is not None:
        outcome = verdict.parse_dw_outcome(post.kv.get("dw_outcome", ""))
        post_class = outcome.get("class")
        # JT=1 竞态族代理（:773）：D6b skip 在 ∧ **非 (a) D-W skip 分支**（B4：
        # skip 分支的 D6b 行属 (a) 全量指派、jt_registered 从未置位 → :789 (C
        # 竞态族外——cut-state (A)/(B) 不施于该格）。
        jt1 = (dw_skip_cause not in ("no-live-fd", "dup-failed")) and any(
            e.name == "N1BDISC_SKIP" and e.kv.get("item") == "D6b"
            for e in events)
        cut_false = jt1 and post.kv.get("worker_terminal_at_p12") == "false"
        cut_report = {
            "jt1": jt1,
            "cut_false": cut_false,
            "parse_ok": bool(outcome),
            "class": post_class,
            # 内层 poll 名 → 冻结闭表名换算（DW_OUTCOME_POLL_RAW_KEYS，dw.rs:1009）
            "poll_raw": {raw_name: outcome.get(inner, "")
                         for inner, raw_name
                         in verdict.DW_OUTCOME_POLL_RAW_KEYS.items()},
            "watchdog": outcome.get("watchdog", ""),
            "racewin_present": _has(events, "N1BDISC_DW_RACEWIN"),
        }
    return rebuilt, post_class, cut_report


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def build_record(scenario_name: str, selftests: Dict[str, Any],
                 camp: Dict[str, Any], judged: Dict[str, Any]) -> Dict[str, Any]:
    result: verdict.VerdictResult = judged["verdict_result"]
    return {
        "mode": "dryrun",
        "is_evidence": False,          # 门 11：DryRun 非证据采集
        "hdc0": "fake-hdc",            # 门 11：HDC0 = host-only 假 HDC
        "integrity": {},               # 门 11：integrity empty
        "scenario": scenario_name,
        "steps": {
            "gate10_selftests": selftests,
            "gate11_dryrun_parse": {
                "markers": len(camp["stream_events"]),
                "hilog_lines": len(camp["lines"]),
                "late_markers": len(camp["fsm"].late_markers),
                "positive_baseline_pid": camp["positive_baseline_pid"],
                "snapshot_files": camp["snapshot_files"],
                "new_fault_files": camp["new_files"],
                "faultrecv_failures": camp["recv_failures"],
                "host_finally": camp["finally_log"],
                "verified_clean": camp["verified_clean"],
                "close_kind": camp["close_kind"],
                # m-07 / m-11 登记字段（新增，其余记录面不变）
                "d2_entry_final_outcomes": judged["d2_entry_final_outcomes"],
                "hilog_wallclock_fallback_triggered":
                    camp["hilog_fallback_triggered"],
            },
            "gate13_verdict": result.as_dict(),
        },
        "protocol": verdict.derive_protocol(_has(camp["stream_events"], "N1BDISC_PRE"),
                                            _has(camp["stream_events"], "N1BDISC_POST")),
        "verdict": result.verdict,
        "evidence_vector": {
            "process_death_observed": camp["death_observed_value"],
            "last_visible_site": camp["last_site"],
            "fault_type_observed": camp["components"].fault_type_observed,
            "signal_observed": camp["components"].signal_observed,
            "destroy_call_state": camp["destroy_state"],
            "marker_tail_state": camp["tail_state"],
            "probe_crash_signature_observed": camp["signature"].value,
            "raw_predicates": list(camp["components"].raw_predicates),
            "unknown_causes": list(camp["components"].unknown_causes),
        },
        "ledger": judged["ledger"],
        "dw": judged["dw"],
        "platform_components": judged["platform_components"],
        "d6_items": judged["d6_items"],
        "d8b_closure": {k: v for k, v in judged["d8b_closure"].items()
                        if k in ("branch", "window_start_monotonic",
                                 "window_end_monotonic", "eagain_observed",
                                 "partial_write_observed", "bytes_written_total",
                                 "write_calls", "caps_hit",
                                 "storm_incomplete_pre_only",
                                 "storm_incomplete_pre_only_span", "raw")},
        "time_box_audit": {
            "observation_window_s": fsm.OBSERVATION_WINDOW_S,
            "allow_box_s": fsm.ALLOW_BOX_S,
            "serial_upper_bound_s": fsm.SERIAL_BOX_UPPER_BOUND_S,
            "close_margin_s": fsm.CLOSE_MARGIN_S,
            "wallclock_fallback_s": fsm.HILOG_WALLCLOCK_FALLBACK_S,
        },
        "hdc_audit_ops": camp["audit_ops"],
    }


def main(argv: Optional[List[str]] = None) -> int:
    """CLI 薄转发：生产入口统一收敛到 :func:`n1bdisc_cli.main`（同 argv 契约）。"""
    import n1bdisc_cli   # noqa: PLC0415 — 延迟导入：helpers 单测零 CLI 依赖
    return n1bdisc_cli.main(argv=argv)


if __name__ == "__main__":
    sys.exit(main())
