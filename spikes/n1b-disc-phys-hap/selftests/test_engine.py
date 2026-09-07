#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc engine selftests——campaign 单一编排 engine（host-only，零真实设备）。

被测对象：``runner/n1bdisc_engine.py`` :func:`run_campaign`（真实 transport →
capture → 真实 fact → 既有判定面 → host finally → recorder seal）。判定面完全
复用 ``n1bdisc_run.derive_and_judge``（不复制 verdict 逻辑）；协议顺序、join 边
界（live 恒 ``join_exit_rc=None``/``join_blocked_registered=False``）、target
脱敏、记录器显式收口均按 engine 模块契约钉死。

覆盖（12 用例）：完整 producer 格式 happy → pass 完整事实；PRE 后死亡 →
pre-only（真实取材，墙钟不可求值如实登记）；no-live-fd → 合法 skip；静默 Allow
盒 → fail + finally 全序列（注入小时间盒，不等 300 s）；PRE 存活无 POST 到点 →
F9（不被洗成 join-blocked-observed）；StartEntry call 异常 → 仍 cleanup、不重
发、FSM 不可收口如实登记；stream 异常 → exception-cleanup 仍 cleanup；
operator_ready=False → 零操作零消费拒启（含 live×fake transport 配对拒绝）；
stdout 中 target 不入任何记录文件且不全量 argv 入日志；storm 死亡无设备墙钟 →
span 如实 F4（DryRun 合成对不泄漏进 engine，原 DryRun 行为不回归）；墙钟/静默
纯函数钉；**端到端**：RealHdcTransport 真 subprocess 启动
``fixtures/fake_hdc_campaign.py``（argv+状态文件生命周期假 hdc）跨层验证
"捕获→真实 fact→派生→cleanup→seal"；审查整改钉：finally PidOfVpn rc=1 不合成
死亡（F9 保持）、absent 探针自身失败不判 clean、记录器落盘失败不断清理链且
封签必达（尾段失败显式 incomplete + 外抛）、rc=0 早退无 POST/到点 → 静默不可
判（真 subprocess 反例）。

全部用例使用 TEST 身份 recorder / 本机 fake 执行器或临时沙箱夹具——绝不真实
pair/设备/签名，不改 PATH，夹具在系统临时目录。

一条命令运行（pytest 与自研 main 双兼容）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_engine.py
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_engine.py
"""

from __future__ import annotations

import contextlib
import hashlib
import json
import os
import shutil
import signal
import sys
import tempfile
import threading
import time

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(_HERE, os.pardir, "runner"))
sys.path.insert(1, os.path.join(_HERE, "fixtures"))

import n1bdisc_capture as cap            # noqa: E402
import n1bdisc_death as death            # noqa: E402
import n1bdisc_engine as engine          # noqa: E402
import n1bdisc_hdc as hdc                # noqa: E402
import n1bdisc_recording as recording    # noqa: E402
import n1bdisc_run as run_mod            # noqa: E402
import n1bdisc_transport_real as tr      # noqa: E402
import fake_hdc as fake                  # noqa: E402
import fake_hdc_campaign as campaign_fixture  # noqa: E402

_ASSERTS = 0

#: 夹具源文件（只读；端到端用例执行的是它的可执行副本）。
CAMPAIGN_FIXTURE_SRC = os.path.join(_HERE, "fixtures", "fake_hdc_campaign.py")

#: 注入小时间盒（测试钉：绝不等 300/525/825 s 真实时间；FakeClock 每次读数自推
#: ``auto_step_ms``，捕获循环每行 3-4 次读数 → 秒级模拟时间即可覆盖全剧本）。
TEST_TIMING = cap.CaptureTiming(allow_box_s=30, observation_window_s=30,
                                wallclock_fallback_s=60)


def expect(cond, msg):
    global _ASSERTS
    _ASSERTS += 1
    if not cond:
        raise AssertionError(msg)


# --------------------------------------------------------------------------
# 测试替身：注入钟 / FakeHdc 定时流适配（read_event 契约）/ 注入失败面
# --------------------------------------------------------------------------

class FakeClock:
    """注入钟：每次读数自推 ``auto_step_ms``（驱动无行静默期的到点推进）。"""

    def __init__(self, mono_ms=0, wall_ms=0, auto_step_ms=100):
        self._mono = int(mono_ms)
        self._wall = int(wall_ms)
        self._auto = int(auto_step_ms)

    def advance(self, ms):
        self._mono += int(ms)
        self._wall += int(ms)

    def mono_ms(self):
        value = self._mono
        self.advance(self._auto)
        return value

    def wall_ms(self):
        value = self._wall
        self.advance(self._auto)
        return value


class TimedStream:
    """HdcStream 契约替身：逐次弹 FakeHdc 合成流的一行；耗尽后按策略
    eof（携带退出码）/ timeout（长驻静默）/ raise（注入流异常）。"""

    def __init__(self, generator, policy="eof", exit_code=0):
        self._gen = generator
        self._policy = policy
        self._exit_code = exit_code
        self._raised = False
        self.close_calls = 0

    def read_event(self, timeout_s):
        try:
            line = next(self._gen)
        except StopIteration:
            if self._policy == "raise" and not self._raised:
                self._raised = True
                raise RuntimeError("injected stream failure")
            if self._policy == "eof":
                return tr.HdcStreamEvent("eof", None, self._exit_code)
            return tr.HdcStreamEvent("timeout", None, None)
        return tr.HdcStreamEvent("line", line, None)

    def close(self):
        self.close_calls += 1


class FakeTimedTransport(hdc.HdcTransport):
    """既有 FakeHdc 的 engine 适配：call 语义原样（FaultRecv 补真实 file recv
    的 host 落盘副作用），open_stream 包装成有界 read_event 定时流；
    ``fail_ops`` 注入 call 异常、``taint_ops`` 注入携 target 的非零 stderr。"""

    def __init__(self, scenario, fail_ops=(), taint_ops=(), policy="eof",
                 exit_code=0):
        self.inner = fake.FakeHdc(scenario)
        self.fail_ops = tuple(fail_ops)
        self.taint_ops = tuple(taint_ops)
        self.policy = policy
        self.exit_code = exit_code
        self.call_log = []          # (op, argv)（仅测试断言用，绝不入记录）

    @property
    def target(self):
        return self.inner.target

    @property
    def hap_path(self):
        return self.inner.hap_path

    def call(self, argv):
        op = self.inner._validate(argv)
        self.call_log.append((op, list(argv)))
        if op in self.fail_ops:
            raise tr.HdcTransportError("injected transport failure: %s" % op)
        if op in self.taint_ops:
            return hdc.HdcTransportResult(
                1, "", "Fail to connect to %s during %s\n" % (self.target, op))
        result = self.inner.call(argv)
        if result.exit_code == 0 and op == "FaultRecv":
            # 真实 hdc file recv 语义：条目原文落到 host 路径文件（engine 从
            # host 文件读取，不依赖 transport 内存态）。
            name = argv[4].rsplit("/", 1)[-1]
            content = self.inner.received.get(name)
            if content is not None:
                os.makedirs(os.path.dirname(argv[5]), exist_ok=True)
                with open(argv[5], "w", encoding="utf-8") as fh:
                    fh.write(content)
        return result

    def open_stream(self, argv):
        return TimedStream(self.inner.open_stream(argv), self.policy,
                           self.exit_code)


# --------------------------------------------------------------------------
# 夹具 helpers：recorder / 运行封装 / 清单校验 / 真实 subprocess 沙箱
# --------------------------------------------------------------------------

def make_recorder(tmp, label="run", mode="dryrun"):
    """TEST 身份 recorder（绝不真实 pair 记录；ID 全为 TEST 字面）。"""
    root = os.path.join(tmp, label)
    return recording.RunRecorder(
        root, mode=mode, authorisation_id="TEST-AUTH-N1BDISC-0001",
        campaign_id="TEST-CAMPAIGN-0001", evidence_id="TEST-EV-0001",
        code_sha="0" * 40, freeze_hash="TEST-FREEZE-HASH")


def run_with_scenario(scenario, *, tmp, mode="dryrun", operator_ready=True,
                      timing=TEST_TIMING, fail_ops=(), taint_ops=(),
                      policy="eof", stream_exit_code=0, label="run",
                      auto_step_ms=10):
    """engine 一次运行封装：FakeHdc 定时流适配 + FakeClock + TEST recorder。"""
    clock = FakeClock(auto_step_ms=auto_step_ms)
    transport = FakeTimedTransport(scenario, fail_ops=fail_ops,
                                   taint_ops=taint_ops, policy=policy,
                                   exit_code=stream_exit_code)
    executor = hdc.HdcExecutor(transport, target=transport.target,
                               hap_path=transport.hap_path)
    recorder = make_recorder(tmp, label, mode=mode)
    record = engine.run_campaign(
        executor=executor, target=transport.target,
        hap_path=transport.hap_path, clock=clock, recorder=recorder,
        operator_ready=operator_ready, mode=mode, capture_timing=timing)
    return record, transport, recorder, executor


def read_events(recorder):
    with open(os.path.join(recorder.root, "events.jsonl"), "r",
              encoding="utf-8") as fh:
        return [json.loads(ln) for ln in fh if ln.strip()]


def event_labels(recorder):
    return [e["label"] for e in read_events(recorder)]


def verify_manifest(root):
    """封签清单逐文件 sha256/bytes 复核；返回登记的相对路径集合。"""
    with open(os.path.join(root, "manifest.json"), "r",
              encoding="utf-8") as fh:
        manifest = json.load(fh)
    expect(manifest["algorithm"] == "sha256", "清单算法为 sha256")
    for rel, entry in manifest["files"].items():
        path = os.path.join(root, rel)
        with open(path, "rb") as fh:
            digest = hashlib.sha256(fh.read()).hexdigest()
        expect(digest == entry["sha256"], "清单 hash 一致: %s" % rel)
        expect(os.path.getsize(path) == entry["bytes"], "清单字节数一致: %s" % rel)
    return set(manifest["files"])


def read_state_phase(root):
    with open(os.path.join(root, "state.json"), "r", encoding="utf-8") as fh:
        return json.load(fh)["phase"]


@contextlib.contextmanager
def sandbox(prefix="n1b-engine-"):
    tmp = tempfile.mkdtemp(prefix=prefix)
    try:
        yield tmp
    finally:
        for pid in leftover_fixture_procs(tmp, timeout_s=3.0):
            try:
                os.kill(pid, signal.SIGKILL)   # 只清自己 spawn 的夹具进程
            except OSError:
                pass
        shutil.rmtree(tmp, ignore_errors=True)


def leftover_fixture_procs(tmp_root, timeout_s=3.0):
    """/proc 扫描：cmdline 以临时目录为前缀的残留夹具进程（有界等待消失）。"""
    deadline = time.monotonic() + timeout_s
    while True:
        found = []
        for entry in os.listdir("/proc"):
            if not entry.isdigit():
                continue
            pid = int(entry)
            if pid == os.getpid():
                continue
            try:
                with open("/proc/%d/cmdline" % pid, "rb") as fh:
                    args = [a.decode("utf-8", "replace")
                            for a in fh.read().split(b"\x00") if a]
            except OSError:
                continue
            if any(a.startswith(tmp_root) for a in args):
                found.append(pid)
        if not found:
            return []
        if time.monotonic() >= deadline:
            return found
        time.sleep(0.05)


class DeadlineExceeded(Exception):
    pass


@contextlib.contextmanager
def wall_deadline(seconds):
    """用例级墙钟护栏（端到端真 subprocess 用例防挂死）。"""
    if (threading.current_thread() is not threading.main_thread()
            or not hasattr(signal, "setitimer")):
        yield
        return

    def _boom(_signum, _frame):
        raise DeadlineExceeded("wall deadline %gs exceeded (deadlock?)" % seconds)

    old = signal.signal(signal.SIGALRM, _boom)
    signal.setitimer(signal.ITIMER_REAL, seconds)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0.0)
        signal.signal(signal.SIGALRM, old)


# ==========================================================================
# 1. 完整 producer 格式 happy → pass 完整事实
# ==========================================================================

def test_happy_producer_full_pass():
    with sandbox() as tmp:
        record, transport, recorder, executor = run_with_scenario(
            fake.make_happy_path_scenario(), tmp=tmp, label="happy")
        expect(record["refused"] is False, "正式路径非拒启")
        expect(record["verdict"] == "pass", "happy 剧本 verdict pass: %s"
               % record["steps"]["gate13_verdict"])
        expect(record["protocol"] == "complete", "POST 在 → protocol complete")
        facts = record["steps"]["campaign_facts"]
        expect(facts["close_kind"] == "complete-seal", "FSM 收口 complete-seal")
        expect(facts["capture"]["close_reason"] == "post-marker",
               "POST 在场即停捕获")
        expect(record["is_evidence"] is False, "dryrun 门 11 is_evidence=false")
        expect(facts["positive_baseline_pid"] == "20002",
               "唯一 positive 基线 = :vpn pid 字面")
        expect(facts["capture"]["markers"] > 0 and facts["capture"]["lines"] > 0,
               "捕获行/marker 均有量")
        expect(facts["verified_clean"] is True, "四 absent 探针全 absent")
        expect(facts["force_stop_reason"] == "final-cleanup",
               "正常路径 Reason=final-cleanup")
        expect(record["evidence_vector"]["marker_tail_state"] == "tail-complete",
               "POST 在 → tail-complete")
        expect(record["dw"]["join_outcome"] == "no-fact",
               "live join 边界（exit_rc=None）→ no-fact，无比对面")
        ops = record["hdc_audit_ops"]
        expect(ops.count("PidOfVpn") == 2, "PidOfVpn 恰 2 位点（基线+finally）: %s"
               % ops)
        expect(ops.count("StartEntry") == 1, "StartEntry 恰一次")
        expect(ops.index("HilogStream") < ops.index("StartEntry"),
               "HilogStream 审计在 StartEntry 之前（M-03）")
        expect(ops.count("HilogStream") == 1, "开流恰一次")
        expect(verify_manifest(recorder.root)
               >= {"capture.jsonl", "events.jsonl", "state.json", "result.json",
                   "metadata.json"}, "封签清单覆盖全部记录文件")
        expect(read_state_phase(recorder.root) == "complete", "state 终态 complete")
        with open(os.path.join(recorder.root, "result.json"), encoding="utf-8") as fh:
            result_doc = json.load(fh)
        expect(result_doc["result"]["verdict"] == "pass", "result.json verdict pass")
        with open(os.path.join(recorder.root, "capture.jsonl"), encoding="utf-8") as fh:
            capture_lines = [json.loads(ln) for ln in fh if ln.strip()]
        expect(len(capture_lines) == facts["capture"]["lines"],
               "capture.jsonl 行数 = 捕获事实行数（逐行原文+双钟）")
        expect(all("wall_ms" in ln and "mono_ms" in ln for ln in capture_lines),
               "每行捕获带当刻双钟")
        expect(record["sealed"] is True and record["terminal"] == "complete",
               "显式 finish 封签")


# ==========================================================================
# 2. PRE 后死亡 → pre-only（真实取材；墙钟不可求值如实登记）
# ==========================================================================

def test_pre_then_death_pre_only():
    with sandbox() as tmp:
        # m1 整改后 EOF rc=0 早退不再背书静默（尾部未观测到时间边界）；pre-only
        # 合法终态的静默改取材自真实时间边界——policy="hang" = 真实设备形态
        # （hilog 不随应用死亡），捕获由观测窗到点收口。
        record, transport, recorder, _ = run_with_scenario(
            fake.make_pre_only_scenario(), tmp=tmp, label="preonly",
            policy="hang")
        expect(record["verdict"] == "pass", "pre-only 合法终态 pass: %s"
               % record["steps"]["gate13_verdict"])
        expect(record["protocol"] == "pre-only", "protocol pre-only")
        facts = record["steps"]["campaign_facts"]
        expect(facts["close_kind"] == "pre-only", "FSM 收口 pre-only")
        expect(record["evidence_vector"]["process_death_observed"]
               == "observed-true", "基线+absent+静默 → 死亡 observed-true")
        expect(facts["capture"]["close_reason"] == "window-deadline"
               and facts["capture"]["window_deadline_reached"] is True,
               "观测窗到点收口（时间边界背书静默）: %s"
               % facts["capture"]["close_reason"])
        expect(facts["pidof_absent"] is True, "finally 采样 absent")
        expect(facts["new_fault_files"]
               == ["faultlogger-0002-cn.alfadb.netbird.n1bdisc"],
               "快照差分只取窗内新增: %s" % facts["new_fault_files"])
        expect(facts["faultrecv_failures"] == [], "取回无失败")
        fresh_path = os.path.join(recorder.root, engine.FAULTLOGGER_SUBDIR,
                                  "faultlogger-0002-cn.alfadb.netbird.n1bdisc")
        expect(os.path.isfile(fresh_path), "fault 条目原文落 recorder 自有目录")
        with open(fresh_path, encoding="utf-8") as fh:
            expect("APPFREEZE" in fh.read(), "原文逐字落盘")
        expect("faultlogger/faultlogger-0002-cn.alfadb.netbird.n1bdisc"
               in verify_manifest(recorder.root), "fault 条目随封签入清单")
        # fault 条目原文携年形设备时间戳 → 死亡证据墙钟可求值；合成捕获行为
        # MM-DD 形态（无年）→ 最后可见 marker 墙钟 None → 尾静默按既有具名 cause。
        expect(record["wall_facts"]["death_wall_ms"] is not None,
               "年形 fault 条目 → 死亡墙钟真实取材")
        expect(record["wall_facts"]["last_marker_wall_ms"] is None,
               "无年形捕获行 → marker 墙钟 None（不猜年）")
        expect(record["evidence_vector"]["marker_tail_state"]
               == "unobservable(cause=tail-clock-unresolvable)",
               "任一墙钟不可求值 → tail-clock-unresolvable（不造合成值）")
        expect(record["hdc_audit_ops"].count("PidOfVpn") == 2,
               "PidOfVpn 恰 2 位点")


# ==========================================================================
# 3. no-live-fd → 合法 skip（D-W skip 编码一致）
# ==========================================================================

def test_no_live_fd_legal_skip():
    with sandbox() as tmp:
        record, transport, recorder, _ = run_with_scenario(
            fake.make_no_live_fd_scenario(), tmp=tmp, label="nolivefd")
        expect(record["verdict"] == "pass", "no-live-fd skip 形态 pass: %s"
               % record["steps"]["gate13_verdict"])
        expect(record["protocol"] == "complete", "POST 照发 → complete")
        expect(record["dw"]["rebuilt_join"] == "unobservable(cause=no-live-fd)",
               "D-W skip 表指派重建")
        expect(record["dw"]["post_join"] == "unobservable(cause=no-live-fd)",
               "POST join= 同一 skip 编码（十值域内）")
        expect(record["d8b_closure"]["branch"] == "skip", "D8b skip 分支")
        expect(record["steps"]["campaign_facts"]["verified_clean"] is True,
               "finally 清理完成")


# ==========================================================================
# 4. 静默 Allow 盒 → fail + finally 全序列（注入小时间盒）
# ==========================================================================

def test_silent_allow_box_fail_with_full_finally():
    with sandbox() as tmp:
        # 静默盒钉用 FSM 冻结常量（timing=None）：capture 机器盒与 FSM 判定盒
        # 同源（300 s），FakeClock 自推 10 ms/读数 → 全程毫秒级，绝不等真实 300 s。
        record, transport, recorder, _ = run_with_scenario(
            fake.FakeScenario(markers=[]), tmp=tmp, label="allow",
            policy="hang", timing=None)
        expect(record["time_box_audit"]["effective_capture_timing"]
               == "frozen-defaults", "缺省时间盒 = fsm 冻结常量")
        facts = record["steps"]["campaign_facts"]
        expect(facts["capture"]["close_reason"] == "allow-deadline",
               "Allow 盒到点收口（小时间盒，未等 300 s）")
        expect(facts["capture"]["allow_deadline_reached"] is True,
               "Allow 到点标志")
        expect(facts["close_kind"] == "fail-allow-consumed",
               "已消费 campaign 收口")
        expect(record["verdict"] == "fail", "Allow 盒无 marker → fail")
        expect("F2" in record["steps"]["gate13_verdict"]["gates"],
               "PRE 缺 → F2: %s" % record["steps"]["gate13_verdict"]["gates"])
        expect(record["evidence_vector"]["process_death_observed"]
               == "unobservable(cause=pidofvpn-no-positive-baseline)",
               "无基线 → 既有具名 cause，不反推死亡")
        expect(facts["positive_baseline_pid"] is None, "零基线采样")
        ops = record["hdc_audit_ops"]
        expect(ops.count("PidOfVpn") == 1, "无 marker → 仅 finally 1 次采样")
        for op in ("FaultProbe", "ForceStop", "Uninstall", "RemoveStaging",
                   "BundleDump", "PidOfPost", "PidOfVpnPost", "StagingProbe"):
            expect(op in ops, "finally 全序列执行 %s（fail 也 cleanup）" % op)
        expect(record["terminal"] == "complete" and record["sealed"] is True,
               "close_kind 已得 → complete 封签")
        expect(verify_manifest(recorder.root), "封签清单可校验")


# ==========================================================================
# 5. PRE 存活无 POST 到点 → F9（不洗成 join-blocked-observed）
# ==========================================================================

def test_window_expired_alive_f9():
    with sandbox() as tmp:
        scenario = fake.FakeScenario(markers=[
            fake.FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
            fake.FakeMarker("PRE", 1000, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                          "skip_summary": "none"}),
        ])
        record, transport, recorder, _ = run_with_scenario(
            scenario, tmp=tmp, label="f9", policy="hang")
        facts = record["steps"]["campaign_facts"]
        expect(facts["capture"]["close_reason"] == "window-deadline",
               "观测窗到点收口（小时间盒）")
        expect(facts["close_kind"] == "fail-f9-window-alive", "FSM F9 收口")
        expect(record["verdict"] == "fail", "存活未完成 → fail")
        gates = record["steps"]["gate13_verdict"]["gates"]
        expect("F9" in gates, "F9 在 fail 闭集命中: %s" % gates)
        expect(record["evidence_vector"]["process_death_observed"]
               == "observed-false", "进程持续在场 → observed-false（不 fail 该分量）")
        expect(record["join_boundary"]["join_blocked_registered"] is False
               and record["join_boundary"]["join_exit_rc"] is None,
               "join 边界恒 False/None（不由 DW_EXIT/缺席推导）")
        expect(record["dw"]["join_outcome"] == "no-fact",
               "存活无 join 事实 → no-fact，不产生 join-blocked-observed")
        expect(record["evidence_vector"]["last_visible_site"] == "P5T",
               "最后可见位点 P5T")


# ==========================================================================
# 6. StartEntry call 异常 → 仍 cleanup、不重发、FSM 不可收口如实登记
# ==========================================================================

def test_startentry_call_abort_cleanup_no_retry():
    with sandbox() as tmp:
        record, transport, recorder, _ = run_with_scenario(
            fake.make_happy_path_scenario(), tmp=tmp, label="abort",
            fail_ops=("StartEntry",))
        facts = record["steps"]["campaign_facts"]
        expect(facts["launch_aborted"] is not None
               and "StartEntry" in facts["launch_aborted"],
               "启动中止事实登记: %s" % facts["launch_aborted"])
        ops = record["hdc_audit_ops"]
        expect("StartEntry" not in ops, "StartEntry 未成功发出（不重发）")
        start_attempts = [argv for op, argv in transport.call_log
                          if op == "StartEntry"]
        expect(len(start_attempts) == 1, "StartEntry 只尝试一次（恰一次语义）")
        expect("HilogStream" in ops, "开流已发生（先于 StartEntry）")
        for op in ("FaultProbe", "ForceStop", "Uninstall", "RemoveStaging",
                   "BundleDump", "PidOfPost", "PidOfVpnPost", "StagingProbe"):
            expect(op in ops, "中止后 finally 全序列仍执行: %s" % op)
        expect(facts["force_stop_reason"] == "exception-cleanup",
               "异常路径 Reason=exception-cleanup")
        expect(facts["close_kind"] is None, "FSM 未到 allow-wait → 无收口面")
        expect(any(f.startswith("terminal-judgement") for f in facts["step_failures"]),
               "终判不可用登记为失败事实（不默默 pass）")
        expect(record["verdict"] == "fail"
               and "F2" in record["steps"]["gate13_verdict"]["gates"],
               "PRE 缺 → F2 fail")
        expect(record["terminal"] == "incomplete" and record["sealed"] is True,
               "无收口 → incomplete 显式封签（不声称 complete）")
        expect(read_state_phase(recorder.root) == "incomplete", "state incomplete")
        expect(verify_manifest(recorder.root), "incomplete 路径仍封签可校验")


# ==========================================================================
# 7. stream 异常 → exception-cleanup、仍 cleanup、判定面照常封装
# ==========================================================================

def test_stream_exception_still_cleanup():
    with sandbox() as tmp:
        record, transport, recorder, _ = run_with_scenario(
            fake.make_pre_only_scenario(), tmp=tmp, label="streamerr",
            policy="raise")
        facts = record["steps"]["campaign_facts"]
        expect(facts["capture"]["close_reason"] == "exception", "流异常收口")
        expect(facts["capture"]["stream_exception"] is not None,
               "异常事实入记录（脱敏摘要）")
        expect(facts["launch_aborted"] is None, "capture 异常不是启动中止")
        expect(facts["force_stop_reason"] == "exception-cleanup",
               "Reason=exception-cleanup")
        ops = record["hdc_audit_ops"]
        expect(ops.count("StartEntry") == 1 and ops.count("HilogStream") == 1,
               "StartEntry/开流各恰一次")
        for op in ("ForceStop", "Uninstall", "RemoveStaging"):
            expect(op in ops, "流异常仍 cleanup: %s" % op)
        expect(record["evidence_vector"]["process_death_observed"]
               == "unobservable(cause=marker-gap-indeterminate)",
               "capture 自身缺口 → 静默不可判（既有 cause，不洗成死亡/存活）")
        expect(record["verdict"] == "fail", "EOF/异常 ≠ pass：F9 承载")
        expect("F9" in record["steps"]["gate13_verdict"]["gates"], "F9 命中")
        expect(facts["verified_clean"] is True, "清理完成")
        expect(record["hdc_audit_ops"].count("PidOfVpn") == 2, "2 位点保持")


# ==========================================================================
# 8. operator_ready=False → 零操作拒启（+ mode×transport 配对拒绝）
# ==========================================================================

def test_operator_ready_false_zero_ops():
    with sandbox() as tmp:
        record, transport, recorder, executor = run_with_scenario(
            fake.make_happy_path_scenario(), tmp=tmp, label="refused",
            operator_ready=False)
        expect(record["refused"] is True, "拒启记录")
        expect(record["reason"] == "operator-ready-not-confirmed", "拒启原因")
        expect(transport.call_log == [], "零 hdc 操作")
        expect(executor.audit == [], "executor 零审计（未触 transport）")
        expect(record["hdc_audit_ops"] == [], "记录面零操作")
        expect(record["verdict"] is None, "不编造 verdict")
        expect(record["terminal"] == "incomplete" and record["sealed"] is True,
               "拒启也显式 finish（incomplete）")
        labels = event_labels(recorder)
        expect("campaign-refused" in labels, "拒启事件入档")
        expect("operator-ready" not in labels, "无 operator-ready 记账")
        files = verify_manifest(recorder.root)
        expect("capture.jsonl" in files, "空 capture.jsonl 仍在（recorder 初始面）")
        with open(os.path.join(recorder.root, "capture.jsonl"), encoding="utf-8") as fh:
            expect(fh.read() == "", "零捕获行")
        # 配对守卫：live 必须 RealHdcTransport；dryrun 拒绝 real transport。
        clock = FakeClock()
        rec = make_recorder(tmp, "pair-live", mode="live")
        fake_executor = hdc.HdcExecutor(
            FakeTimedTransport(fake.make_happy_path_scenario()),
            target="FAKE-TARGET-1", hap_path="/host/fake/n1bdisc.hap")
        try:
            engine.run_campaign(executor=fake_executor, target="FAKE-TARGET-1",
                                hap_path="/host/fake/n1bdisc.hap", clock=clock,
                                recorder=rec, operator_ready=True, mode="live")
            raise AssertionError("live×fake transport 必须拒绝")
        except engine.EngineError:
            pass
        expect(transport.call_log == [], "配对拒绝发生在零操作之后")
        expect(os.path.isfile(os.path.join(rec.root, "metadata.json")),
               "被拒 recorder 构造面完好")
        expect(not os.path.exists(os.path.join(rec.root, "result.json")),
               "engine 拒绝不越权封签（终态交还 caller）")


# ==========================================================================
# 9. target 不入任何记录文件 + 不全量 argv 入日志 + raw error 定点脱敏
# ==========================================================================

def test_target_never_in_files_and_argv_never_logged():
    with sandbox() as tmp:
        record, transport, recorder, _ = run_with_scenario(
            fake.make_happy_path_scenario(), tmp=tmp, label="redact",
            taint_ops=("ForceStop",))
        facts = record["steps"]["campaign_facts"]
        expect(facts["force_stop_reason"] == "final-cleanup",
               "ForceStop 非零 ≠ capture 异常 → Reason 仍 final")
        expect(any(f.startswith("ForceStop") for f in facts["step_failures"]),
               "ForceStop 失败登记（不默默 pass）")
        expect(facts["verified_clean"] is False,
               "ForceStop 失败 → absent 探针不满足 → 不声称 clean")
        events_text = open(os.path.join(recorder.root, "events.jsonl"),
                           encoding="utf-8").read()
        expect(transport.target not in events_text, "target 不入 events")
        expect("<TARGET>" in events_text, "raw error 中 target 定点替换 <TARGET>")
        for token in ("aa start", "force-stop", "bm install", "pidof", "hilog -T",
                      "file send", "-t "):
            expect(token not in events_text,
                   "argv 片段不入日志: %r" % token)
        # 全目录扫描：target 绝不出现在任何记录文件（含 capture/state/result）。
        for dirpath, _dirs, filenames in os.walk(recorder.root):
            for name in filenames:
                with open(os.path.join(dirpath, name), "rb") as fh:
                    blob = fh.read()
                expect(transport.target.encode() not in blob,
                       "target 不入记录文件: %s" % name)
        labels = event_labels(recorder)
        expect(labels.count("PidOfVpn") == 2, "PidOfVpn 事件恰 2 条")
        hilog_events = [e for e in read_events(recorder)
                        if e["label"] == "HilogStream"]
        expect(len(hilog_events) == 1 and hilog_events[0]["details"]["phase"]
               == "opened", "HilogStream 事件 = 阶段脱敏摘要（无 argv）")
        expect(record["verdict"] == "pass", "ForceStop 失败不改判据（既有闭集）")


# ==========================================================================
# 10. storm 死亡无设备墙钟 → span 如实 F4；DryRun 合成对不泄漏、原行为不回归
# ==========================================================================

def test_storm_death_span_undecidable_without_device_clocks():
    with sandbox() as tmp:
        # policy="hang"：流长驻（真实设备形态）→ 捕获由观测窗到点收口，静默有
        # 时间边界背书（m1 整改后 EOF rc=0 早退不再判 True）。
        record, transport, recorder, _ = run_with_scenario(
            fake.make_storm_death_scenario(), tmp=tmp, label="storm",
            policy="hang")
        expect(record["verdict"] == "fail", "强制跨度不可求值 → F4 fail-closed")
        expect("F4" in record["steps"]["gate13_verdict"]["gates"],
               "F4 命中: %s" % record["steps"]["gate13_verdict"]["gates"])
        closure = record["d8b_closure"]
        expect(closure["branch"] == "storm-incomplete-pre-only",
               "BEGIN-only 死亡收口分支")
        expect(closure["storm_incomplete_pre_only"] is True, "BEGIN-only 标志")
        expect(closure["storm_incomplete_pre_only_span"] is None,
               "engine 无合成墙钟 → 跨度 None（不携带 100000/90000）")
        # SIGKILL 条目携年形设备时间戳 → 死亡墙钟可求值；BEGIN 捕获行（MM-DD）
        # 无年 → begin 墙钟 None → :607 强制跨度如实不可求值。
        expect(record["wall_facts"]["death_wall_ms"] is not None,
               "死亡墙钟来自年形 fault 条目")
        expect(record["wall_facts"]["begin_capture_wall_ms"] is None,
               "BEGIN 捕获行无年形墙钟 → None")
        # 对照：原 DryRun 路径（缺省合成对）行为逐字不回归。
        camp = run_mod.run_dryrun_campaign("storm-death",
                                           fake.make_storm_death_scenario())
        judged = run_mod.derive_and_judge(camp)
        expect(judged["d8b_closure"]["storm_incomplete_pre_only_span"] == 10000,
               "DryRun 缺省合成跨度 100_000-90_000=10_000 保持")
        expect(judged["verdict_result"].verdict == "pass",
               "原 DryRun storm-death 判定不回归")


# ==========================================================================
# 11. 墙钟纯函数钉（engine 事实面取材规则）
# ==========================================================================

def test_wall_and_silence_fact_helpers():
    year_text = "2026-09-06 10:00:00.000  1234  1234 D tag: N1BDISC_PRE"
    wall = engine.device_wall_ms(year_text)
    expect(isinstance(wall, int) and wall > 0, "年形时间戳可解析为正毫秒读数")
    expect(engine.device_wall_ms("09-06 10:00:00.000 x") is None,
           "无年形态不猜年 → None")
    expect(engine.device_wall_ms("garbage") is None, "垃圾文本 → None")
    expect(engine.captured_line_wall_ms(year_text) == wall, "捕获行墙钟同口径")
    later = engine.device_wall_ms("2026-09-06 10:00:15.500 x")
    expect(later - wall == 15500, "同源差值口径（时区口径相消）")
    parse_a = death.parse_fault_entry(
        "b-entry", fake.fault_entry_text("APPFREEZE", "SIGKILL",
                                         "2026-09-06 10:00:02.000"))
    parse_b = death.parse_fault_entry(
        "a-entry", fake.fault_entry_text(None, None,
                                         "2026-09-06 10:00:01.000"))
    expect(engine.death_evidence_wall_ms([parse_a, parse_b]) == wall + 1000,
           "死亡证据墙钟 = 文件名字节序首个可解析条目")
    expect(engine.death_evidence_wall_ms([]) is None, "无条目 → None")
    parse_no_ts = death.parse_fault_entry("c-entry", "Module: x\n")
    expect(engine.death_evidence_wall_ms([parse_a, parse_no_ts]) == wall + 2000,
           "无时间戳条目跳过（字节序首个带时间戳条目为准）")
    # 静默事实（capture_silence_fact）不再逐支复述实现表（m2 整改）：其行为
    # 由端到端钉覆盖——rc=0 早退且无 POST/到点 → silence None（见
    # test_e2e_rc0_early_eof_silence_indeterminate），时间边界/POST 在场 →
    # True 由 test_pre_then_death_pre_only / happy 用例端到端承载。


# ==========================================================================
# 12. 端到端：RealHdcTransport 真 subprocess × fake_hdc_campaign 夹具
# ==========================================================================

def _make_hap_file(tmp):
    hap = os.path.join(tmp, "n1bdisc-test.hap")
    with open(hap, "wb") as fh:
        fh.write(b"fake-hap-payload-for-e2e")
    return hap


def _make_e2e_scene(tmp):
    """端到端剧本：producer 格式行（经 FakeHdc 设备侧模拟生成真实 PRE digest）。"""
    scenario = fake.make_death_after_pre_scenario()
    stream_transport = fake.FakeHdc(scenario)
    argv = hdc.build_argv("HilogStream", target=stream_transport.target,
                          hap_path=stream_transport.hap_path)
    lines = list(stream_transport.open_stream(argv))
    crash_name = "faultlogger-7001-cn.alfadb.netbird.n1bdisc"
    return {
        "bundle": hdc.BUNDLE,
        "model": "E2E-MODEL-1",
        "sw": "7.0.0.999",
        "vpn_pid": 24567,
        "ui_pid": 24566,
        "staging_root": hdc.STAGING_ROOT,
        "staging_sandbox": os.path.join(tmp, "staging-sandbox"),
        "fault_dir": os.path.join(tmp, "faultlogger-sandbox"),
        "crash_files": {
            crash_name: fake.fault_entry_text("APPFREEZE", "SIGKILL",
                                              "2026-09-06 10:00:00.000"),
        },
        "stream": [{"after_ms": 120 + i * 120, "line": ln}
                   for i, ln in enumerate(lines)],
        # m1 整改后 EOF rc=0 早退不背书静默：进程死亡但流长驻（真实设备形态
        # ——hilog 不随应用死亡），捕获由注入时间盒的观测窗到点收口。
        "die_after_lines_keep_stream": True,
    }


def test_end_to_end_real_transport_fixture():
    with sandbox() as tmp:
        exe_dir = os.path.join(tmp, "bin")
        os.makedirs(exe_dir)
        exe = os.path.join(exe_dir, "fake_hdc_campaign.py")
        shutil.copyfile(CAMPAIGN_FIXTURE_SRC, exe)
        os.chmod(exe, 0o755)
        scene = _make_e2e_scene(tmp)
        scene_path = os.path.join(tmp, "scene.json")
        state_path = os.path.join(tmp, "state.json")
        with open(scene_path, "w", encoding="utf-8") as fh:
            json.dump(scene, fh, ensure_ascii=False)
        os.environ[campaign_fixture.ENV_SCENE] = scene_path
        os.environ[campaign_fixture.ENV_STATE] = state_path
        try:
            transport = tr.RealHdcTransport(exe, call_timeout_s=20)
            target = "E2E-TEST-TGT"
            hap = _make_hap_file(tmp)
            executor = hdc.HdcExecutor(transport, target=target, hap_path=hap)
            recorder = make_recorder(tmp, "e2e-run", mode="live")
            with wall_deadline(120):
                record = engine.run_campaign(
                    executor=executor, target=target, hap_path=hap,
                    clock=cap.MonoWallClock(), recorder=recorder,
                    operator_ready=True, mode="live",
                    capture_timing=cap.CaptureTiming(
                        allow_box_s=10, observation_window_s=10,
                        wallclock_fallback_s=30))
        finally:
            os.environ.pop(campaign_fixture.ENV_SCENE, None)
            os.environ.pop(campaign_fixture.ENV_STATE, None)
        expect(record["sealed"] is True and record["terminal"] == "complete",
               "端到端封签完成")
        expect(record["verdict"] == "pass", "端到端 pre-only pass: %s"
               % record["steps"]["gate13_verdict"])
        expect(record["protocol"] == "pre-only", "端到端 protocol pre-only")
        expect(record["is_evidence"] is True, "live 模式 is_evidence=true")
        facts = record["steps"]["campaign_facts"]
        expect(facts["close_kind"] == "pre-only", "FSM pre-only 收口")
        expect(facts["capture"]["close_reason"] == "window-deadline"
               and facts["capture"]["window_deadline_reached"] is True,
               "观测窗到点收口（时间边界背书静默；流长驻真实设备形态）: %s"
               % facts["capture"]["close_reason"])
        expect(record["evidence_vector"]["process_death_observed"]
               == "observed-true", "真实 capture+pidof → 死亡 observed-true")
        expect(facts["pidof_absent"] is True, "真实 pidof 采样 absent")
        expect(facts["positive_baseline_pid"] == "24567", "真实基线 pid")
        expect(len(facts["new_fault_files"]) == 1, "真实快照差分命中新增条目")
        fresh = facts["new_fault_files"][0]
        with open(os.path.join(recorder.root, engine.FAULTLOGGER_SUBDIR, fresh),
                  encoding="utf-8") as fh:
            expect("SIGKILL" in fh.read(), "file recv 原文真实落盘")
        expect(facts["verified_clean"] is True, "真实清理生命周期 verified_clean")
        ops = record["hdc_audit_ops"]
        expect(ops.count("PidOfVpn") == 2 and ops.count("StartEntry") == 1
               and ops.index("HilogStream") < ops.index("StartEntry"),
               "真实命令流位次：开流先于 StartEntry、PidOfVpn 恰 2")
        files = verify_manifest(recorder.root)   # 封签清单逐文件复核
        for name in os.listdir(recorder.root):
            path = os.path.join(recorder.root, name)
            if not os.path.isfile(path):
                continue
            blob = open(path, "rb").read()
            expect(target.encode() not in blob, "target 不入端到端记录: %s" % name)
        expect(leftover_fixture_procs(tmp) == [], "子进程组全部回收（/proc 证据）")
        expect(len(capture_lines_of(recorder)) == facts["capture"]["lines"],
               "capture.jsonl 逐行原文与捕获事实一致")


def capture_lines_of(recorder):
    with open(os.path.join(recorder.root, "capture.jsonl"), encoding="utf-8") as fh:
        return [json.loads(ln) for ln in fh if ln.strip()]


# ==========================================================================
# 13. 审查整改钉：BLOCKER-1（pidof 采样失败不合成死亡）/ MAJOR-2（absent 探针
#     自身失败不判 clean）/ MAJOR-1（记录器落盘失败不断清理链、封签必达）
# ==========================================================================

class FlakyRc1Transport(FakeTimedTransport):
    """自第 ``fail_from_call`` 次起的 ``fail_op`` 注入 rc=1 + 空 stdout + stderr
    （hdc 通道故障形态；不触进程事实面——与 taint_ops 的差别只在计数）。

    ``fail_op`` 按 FakeHdc argv 反向校验名匹配（表序首个 argv 命中）：
    ``PidOfPost``/``PidOf``、``PidOfVpnPost``/``PidOfVpn`` 各为同一 pidof argv
    （校验名分别为 PidOf / PidOfVpn），故 empty 类探针注入用 ``PidOf``
    （campaign 内该 argv 恰一次 = PidOfPost absent 探针位）。
    """

    def __init__(self, scenario, fail_op, fail_from_call=2, **kw):
        super().__init__(scenario, **kw)
        self._fail_op = fail_op
        self._fail_from = int(fail_from_call)
        self._op_calls = {}

    def call(self, argv):
        op = self.inner._validate(argv)
        if op == self._fail_op:
            self._op_calls[op] = self._op_calls.get(op, 0) + 1
            if self._op_calls[op] >= self._fail_from:
                return hdc.HdcTransportResult(
                    1, "", "[Fail]Failed to communicate with the device\n")
        return super().call(argv)


def _run_with_transport(transport, *, tmp, label, mode="dryrun",
                        timing=TEST_TIMING):
    """指定 transport 的一次 engine 运行（其余装配同 run_with_scenario）。"""
    clock = FakeClock(auto_step_ms=10)
    executor = hdc.HdcExecutor(transport, target=transport.target,
                               hap_path=transport.hap_path)
    recorder = make_recorder(tmp, label, mode=mode)
    record = engine.run_campaign(
        executor=executor, target=transport.target,
        hap_path=transport.hap_path, clock=clock, recorder=recorder,
        operator_ready=True, mode=mode, capture_timing=timing)
    return record, recorder


def test_finally_pidof_failure_never_synth_death():
    """BLOCKER-1 钉：观测窗到点（静默可判）+ finally PidOfVpn rc=1 空 stdout
    → 采样不可判（None），不合成 observed-true、F9 保持、不得洗成 pre-only。"""
    with sandbox() as tmp:
        scenario = fake.FakeScenario(markers=[
            fake.FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
            fake.FakeMarker("PRE", 1000,
                            {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                             "skip_summary": "none"}),
        ])
        transport = FlakyRc1Transport(scenario, "PidOfVpn", fail_from_call=2,
                                      policy="hang")
        record, recorder = _run_with_transport(transport, tmp=tmp,
                                               label="pidofrc1")
        facts = record["steps"]["campaign_facts"]
        expect(facts["capture"]["close_reason"] == "window-deadline",
               "观测窗到点收口（静默可判面，隔离 pidof 注入变量）: %s"
               % facts["capture"]["close_reason"])
        expect(facts["positive_baseline_pid"] is not None, "基线采样在场")
        expect(facts["pidof_absent"] is None,
               "rc=1 采样不可判 ≠ absent: %r" % facts["pidof_absent"])
        expect(any(f.startswith("PidOfVpn-finally exit_code=1")
                   for f in facts["step_failures"]),
               "命令失败登记 step_failures: %s" % facts["step_failures"])
        expect(record["evidence_vector"]["process_death_observed"]
               == "unobservable(cause=marker-gap-indeterminate)",
               "不合成死亡（既有闭集值）: %s"
               % record["evidence_vector"]["process_death_observed"])
        expect(facts["close_kind"] == "fail-f9-window-alive"
               and record["verdict"] == "fail",
               "F9 保持不洗 pre-only: %s/%s"
               % (facts["close_kind"], record["verdict"]))
        expect("F9" in record["steps"]["gate13_verdict"]["gates"],
               "F9 在 fail 闭集命中: %s"
               % record["steps"]["gate13_verdict"]["gates"])
        expect(record["sealed"] is True and record["terminal"] == "complete",
               "封签完成")


def test_absent_probe_failure_not_clean():
    """MAJOR-2 钉：absent 探针自身失败（rc=1）≠ absent——该项 ok=False、
    verified_clean 不为 True、失败事实登记；判定面不受影响（如实登记）。"""
    with sandbox() as tmp:
        transport = FlakyRc1Transport(fake.make_happy_path_scenario(),
                                      "PidOf", fail_from_call=1)
        record, recorder = _run_with_transport(transport, tmp=tmp,
                                               label="absentrc1")
        facts = record["steps"]["campaign_facts"]
        expect(facts["absent_checks"]["pidof_post_empty"] is False,
               "rc=1 探针不判 absent: %s" % facts["absent_checks"])
        expect(facts["verified_clean"] is not True,
               "verified_clean 不声称: %s" % facts["verified_clean"])
        expect(any(f.startswith("PidOfPost exit_code=1")
                   for f in facts["step_failures"]),
               "探测失败登记 step_failures: %s" % facts["step_failures"])
        expect(record["verdict"] == "pass",
               "探针失败不改判据（既有闭集）: %s"
               % record["steps"]["gate13_verdict"])


class FailingEventRecorder(recording.RunRecorder):
    """指定 label 的 ``append_event`` 抛 OSError（记录器落盘失败注入）。"""

    fail_label = None

    def append_event(self, label, details=None):
        if self.fail_label is not None and label == self.fail_label:
            raise OSError("injected recorder failure on label=%s" % label)
        return super().append_event(label, details)


class FlakyFinishRecorder(recording.RunRecorder):
    """首次 ``finish`` 抛 OSError（主封签路径失败注入；第二次放行）。"""

    def __init__(self, *args, **kwargs):
        self._fail_finish_once = True
        super().__init__(*args, **kwargs)

    def finish(self, result, terminal="complete"):
        if self._fail_finish_once:
            self._fail_finish_once = False
            raise OSError("injected finish failure")
        return super().finish(result, terminal)


def test_recorder_failure_keeps_finally_chain_and_seal():
    """MAJOR-1 钉：finally 单步记录器落盘失败不中断清理链——10 步 finally_log
    齐全、ForceStop/Uninstall/RemoveStaging/absent 照跑、终态封签落盘。"""
    for fail_label in ("stop-hilogstream", "integrity-close"):
        with sandbox() as tmp:
            transport = FakeTimedTransport(fake.make_happy_path_scenario())
            executor = hdc.HdcExecutor(transport, target=transport.target,
                                       hap_path=transport.hap_path)
            recorder = FailingEventRecorder(
                os.path.join(tmp, "run-" + fail_label), mode="dryrun",
                authorisation_id="TEST-AUTH-N1BDISC-0001",
                campaign_id="TEST-CAMPAIGN-0001", evidence_id="TEST-EV-0001",
                code_sha="0" * 40, freeze_hash="TEST-FREEZE-HASH")
            recorder.fail_label = fail_label
            record = engine.run_campaign(
                executor=executor, target=transport.target,
                hap_path=transport.hap_path, clock=FakeClock(auto_step_ms=10),
                recorder=recorder, operator_ready=True, mode="dryrun",
                capture_timing=TEST_TIMING)
            facts = record["steps"]["campaign_facts"]
            expect(len(facts["host_finally"]) == 10,
                   "finally_log 10 步齐全: %s" % facts["host_finally"])
            for op in ("ForceStop", "Uninstall", "RemoveStaging",
                       "BundleDump", "PidOfPost", "PidOfVpnPost",
                       "StagingProbe"):
                expect(op in record["hdc_audit_ops"], "cleanup 照跑: %s" % op)
            expect(any(fail_label in f for f in facts["step_failures"]),
                   "落盘失败登记 step_failures: %s" % facts["step_failures"])
            expect(record["sealed"] is True, "封签存在（不依赖 __del__）")
            files = verify_manifest(recorder.root)
            expect({"result.json", "state.json"} <= files
                   and os.path.isfile(os.path.join(recorder.root, "manifest.json")),
                   "finish 显式落盘（manifest 存在且清单可校验）: %s"
                   % sorted(files))
            expect(read_state_phase(recorder.root) == "complete",
                   "close_kind 已得 → complete 终态")


def test_tail_failure_seals_incomplete_and_reraises():
    """尾段（判定/组装/finish）失败 → 显式 incomplete 封签落盘 + 原异常外抛
    （不依赖 __del__ 兜底、不返回 sealed 成功记录）。"""
    with sandbox() as tmp:
        transport = FakeTimedTransport(fake.make_happy_path_scenario())
        executor = hdc.HdcExecutor(transport, target=transport.target,
                                   hap_path=transport.hap_path)
        recorder = FlakyFinishRecorder(
            os.path.join(tmp, "run-tailfail"), mode="dryrun",
            authorisation_id="TEST-AUTH-N1BDISC-0001",
            campaign_id="TEST-CAMPAIGN-0001", evidence_id="TEST-EV-0001",
            code_sha="0" * 40, freeze_hash="TEST-FREEZE-HASH")
        try:
            engine.run_campaign(
                executor=executor, target=transport.target,
                hap_path=transport.hap_path, clock=FakeClock(auto_step_ms=10),
                recorder=recorder, operator_ready=True, mode="dryrun",
                capture_timing=TEST_TIMING)
            raise AssertionError("尾段失败必须外抛（不声称成功）")
        except OSError as exc:
            expect("injected finish failure" in str(exc), "原异常外抛: %s" % exc)
        with open(os.path.join(recorder.root, "result.json"),
                  encoding="utf-8") as fh:
            doc = json.load(fh)
        expect(doc["terminal"] == "incomplete", "兜底终态 incomplete: %s"
               % doc["terminal"])
        expect(doc["result"]["verdict"] is None, "兜底记录不编造 verdict")
        expect(read_state_phase(recorder.root) == "incomplete", "state incomplete")
        verify_manifest(recorder.root)   # 封签清单逐文件校验（finish 真落盘）


# ==========================================================================
# 14. m1 端到端反例（真 subprocess 流）：rc=0 早退且无 POST/到点 → 静默不可判
# ==========================================================================

def test_e2e_rc0_early_eof_silence_indeterminate():
    """真 subprocess 夹具（fake_hdc_campaign）：die_after_lines → rc=0 干净
    EOF、无 POST、无任何到点标志 → capture_silence 为 None（不是 True），
    死亡分量落既有 marker-gap-indeterminate、F9 保持、protocol 语义不变。"""
    with sandbox() as tmp:
        exe_dir = os.path.join(tmp, "bin")
        os.makedirs(exe_dir)
        exe = os.path.join(exe_dir, "fake_hdc_campaign.py")
        shutil.copyfile(CAMPAIGN_FIXTURE_SRC, exe)
        os.chmod(exe, 0o755)
        stream_transport = fake.FakeHdc(fake.FakeScenario(markers=[
            fake.FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
            fake.FakeMarker("PRE", 1000,
                            {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                             "skip_summary": "none"}),
        ]))
        argv = hdc.build_argv("HilogStream", target=stream_transport.target,
                              hap_path=stream_transport.hap_path)
        lines = list(stream_transport.open_stream(argv))
        scene = {
            "bundle": hdc.BUNDLE,
            "model": "E2E-MODEL-1",
            "sw": "7.0.0.999",
            "vpn_pid": 24567,
            "ui_pid": 24566,
            "staging_root": hdc.STAGING_ROOT,
            "staging_sandbox": os.path.join(tmp, "staging-sandbox"),
            "fault_dir": os.path.join(tmp, "faultlogger-sandbox"),
            "crash_files": {},
            "stream": [{"after_ms": 120 + i * 120, "line": ln}
                       for i, ln in enumerate(lines)],
            "die_after_lines": True,
        }
        scene_path = os.path.join(tmp, "scene.json")
        state_path = os.path.join(tmp, "state.json")
        with open(scene_path, "w", encoding="utf-8") as fh:
            json.dump(scene, fh, ensure_ascii=False)
        os.environ[campaign_fixture.ENV_SCENE] = scene_path
        os.environ[campaign_fixture.ENV_STATE] = state_path
        try:
            transport = tr.RealHdcTransport(exe, call_timeout_s=20)
            target = "E2E-EOF-TGT"
            hap = _make_hap_file(tmp)
            executor = hdc.HdcExecutor(transport, target=target, hap_path=hap)
            recorder = make_recorder(tmp, "e2e-eof-run", mode="live")
            with wall_deadline(120):
                record = engine.run_campaign(
                    executor=executor, target=target, hap_path=hap,
                    clock=cap.MonoWallClock(), recorder=recorder,
                    operator_ready=True, mode="live",
                    capture_timing=cap.CaptureTiming(
                        allow_box_s=10, observation_window_s=10,
                        wallclock_fallback_s=30))
        finally:
            os.environ.pop(campaign_fixture.ENV_SCENE, None)
            os.environ.pop(campaign_fixture.ENV_STATE, None)
        facts = record["steps"]["campaign_facts"]
        expect(facts["capture"]["close_reason"] == "eof"
               and facts["capture"]["stream_exit_code"] == 0,
               "rc=0 干净 EOF 早退（无 POST/到点）: %s/%s"
               % (facts["capture"]["close_reason"],
                  facts["capture"]["stream_exit_code"]))
        expect(facts["capture"]["allow_deadline_reached"] is False
               and facts["capture"]["window_deadline_reached"] is False,
               "无到点标志")
        expect(facts["capture_silence"] is None,
               "silence None 而非 True: %r" % facts["capture_silence"])
        expect(record["evidence_vector"]["process_death_observed"]
               == "unobservable(cause=marker-gap-indeterminate)",
               "死亡分量不可判（既有闭集值）: %s"
               % record["evidence_vector"]["process_death_observed"])
        expect(facts["close_kind"] == "fail-f9-window-alive"
               and record["verdict"] == "fail",
               "F9 fail-closed: %s/%s"
               % (facts["close_kind"], record["verdict"]))
        expect("F9" in record["steps"]["gate13_verdict"]["gates"],
               "F9 命中: %s" % record["steps"]["gate13_verdict"]["gates"])
        expect(record["protocol"] == "pre-only",
               "protocol 语义不变（marker 面）: %s" % record["protocol"])
        expect(record["sealed"] is True, "封签完成")
        expect(leftover_fixture_procs(tmp) == [], "子进程组全部回收")


# ==========================================================================
# main runner（pytest 不在场时的一条命令入口）
# ==========================================================================

def main() -> int:
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
    print("n1bdisc engine selftests: tests=%d passed=%d failed=%d assertions=%d (%.0f ms)"
          % (len(tests), passed, failed, _ASSERTS, elapsed * 1000))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
