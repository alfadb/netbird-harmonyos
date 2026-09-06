# -*- coding: utf-8 -*-
"""n1bdisc_fsm — N1BDISC host runner 观测窗与门流程状态机（host-only 纯逻辑）。

权威规格：``docs/n1b-disc-gate-plan.md``（只读冻结，本实现以其实际行号为准）。
本模块实现：

1. 时间盒全量常量（规格 :1031-1049 冻结表，逐行与本文件常量一一对应）；
2. operator-ready 登记（规格 :1053：``p0_ready_mono_ms`` + 动作字面
   ``operator-ready-confirmed``，BL-4 非负门）；
3. Allow 300 s 盒（规格 :1033/:1063-1065：自 ``StartEntry`` 命令返回起算、
   终点 = 首个 N1BDISC marker；到点未出现 marker → 已消费 campaign 收口）；
4. HilogStream 先于 StartEntry 的位次约束（规格 :1062-1063）；
5. 观测窗 525 s（规格 :1058-1060：自首个 ``N1BDISC_`` marker 起算，467 + 58 推导冻结）；
6. 窗到点收口四分支（规格 :1066-1068）：POST 在 → complete 封签 / 仅 PRE + 死亡证据 →
   pre-only / 仅 PRE 无死亡 → F9 fail / PRE 缺 → fail；
7. ``late_marker_observed`` 登记（规格 :1066：窗到点后到达的 marker 不参与求值）。

时钟与 IO 全部经注入/显式实参传入，模块导入零副作用；本模块不做任何真实 hdc/
设备/网络访问。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Optional, Tuple

# ---------------------------------------------------------------------------
# 时间盒全量常量（规格 :1031-1049 冻结表，逐行对应；单位见各常量名后缀）
# ---------------------------------------------------------------------------

#: P0 Allow 等待（自 StartEntry 返回起，至 capture 出现首个 N1BDISC marker；:1033）。
ALLOW_BOX_S = 300
#: P1 dlopen+dlsym（:1034）。
P1_DLOPEN_BOX_S = 10
#: P2 每条 create()（:1035）。
P2_CREATE_BOX_S = 60
#: P2 迟到观察窗（全矩阵恰一次；:1036）。
P2_LATE_OBSERVATION_WINDOW_S = 60
#: P3 D4 窗口：总 10 s / 每 poll 500 ms（:1037）。
P3_D4_WINDOW_TOTAL_S = 10
P3_D4_POLL_MS = 500
#: P4 D5 窗口：总 10 s / 每轮 500 ms / ≤5 轮（:1038）。
P4_D5_WINDOW_TOTAL_S = 10
P4_D5_ROUND_MS = 500
P4_D5_MAX_ROUNDS = 5
#: P5 D8a 阶梯：每级 poll 500 ms + write（≤1 s/级，总 ≤10 s；:1039）。
P5_D8A_PER_LEVEL_POLL_MS = 500
P5_D8A_PER_LEVEL_MAX_S = 1
P5_D8A_TOTAL_S = 10
#: P5T N1BDISC_PRE 发射：即返（:1040）。
P5T_PRE_EMIT_S = 0
#: P6 D7 任务：20 s + 5 s 宽限（:1041）。
P6_D7_TASK_S = 20
P6_D7_GRACE_S = 5
#: P7 D8b storm：10 s / 4 MiB / 50 000 写（熔断；:1042）。
P7_D8B_STORM_S = 10
P7_D8B_STORM_BYTES = 4 * 1024 * 1024
P7_D8B_STORM_MAX_WRITES = 50_000
#: P8 drain：5 s（:1043）。
P8_DRAIN_BOX_S = 5
#: P8 barrier 等待：7 s（drain 盒 5 s + 调度裕量 2 s；:1044）。
P8_BARRIER_BOX_S = 7
#: P8 in-wait 证据采集：2 s（10 ms 间隔；:1045）。
P8_INWAIT_SAMPLE_S = 2
P8_INWAIT_SAMPLE_INTERVAL_MS = 10
#: P9 destroy()：10 s（:1046）。
P9_DESTROY_BOX_S = 10
#: P10 worker 终态轮询：8 s（10 ms 间隔；:1047）。
P10_TERMINAL_POLL_BOX_S = 8
P10_POLL_INTERVAL_MS = 10
#: P12 迟到竞态窗等待：1000 ms（:1048）。
P12_RACE_WINDOW_MS = 1000
#: D6 各步：即返 syscall（:1049）。
D6_STEP_IMMEDIATE = True
#: 唯一时间盒豁免 = pthread_join（:1029；join 不可有界，阻塞由观测窗到点 +
#: host finally 收口兜底）。
JOIN_UNBOUNDED = True

#: 时间盒表逐行快照（规格 :1031-1049 一致性断言用：(位点, 名称, 数值, 单位)）。
TIME_BOX_TABLE: Tuple[Tuple[str, str, int, str], ...] = (
    ("P0", "allow-wait", ALLOW_BOX_S, "s"),
    ("P1", "dlopen+dlsym", P1_DLOPEN_BOX_S, "s"),
    ("P2", "per-create", P2_CREATE_BOX_S, "s"),
    ("P2", "late-observation-window", P2_LATE_OBSERVATION_WINDOW_S, "s"),
    ("P3", "d4-window-total", P3_D4_WINDOW_TOTAL_S, "s"),
    ("P3", "d4-per-poll", P3_D4_POLL_MS, "ms"),
    ("P4", "d5-window-total", P4_D5_WINDOW_TOTAL_S, "s"),
    ("P4", "d5-per-round", P4_D5_ROUND_MS, "ms"),
    ("P4", "d5-max-rounds", P4_D5_MAX_ROUNDS, "count"),
    ("P5", "d8a-per-level-poll", P5_D8A_PER_LEVEL_POLL_MS, "ms"),
    ("P5", "d8a-per-level-max", P5_D8A_PER_LEVEL_MAX_S, "s"),
    ("P5", "d8a-total", P5_D8A_TOTAL_S, "s"),
    ("P5T", "pre-emit", P5T_PRE_EMIT_S, "s"),
    ("P6", "d7-task", P6_D7_TASK_S, "s"),
    ("P6", "d7-grace", P6_D7_GRACE_S, "s"),
    ("P7", "d8b-storm", P7_D8B_STORM_S, "s"),
    ("P7", "d8b-storm-bytes", P7_D8B_STORM_BYTES, "B"),
    ("P7", "d8b-storm-max-writes", P7_D8B_STORM_MAX_WRITES, "count"),
    ("P8", "drain", P8_DRAIN_BOX_S, "s"),
    ("P8", "barrier-wait", P8_BARRIER_BOX_S, "s"),
    ("P8", "inwait-sample", P8_INWAIT_SAMPLE_S, "s"),
    ("P8", "inwait-sample-interval", P8_INWAIT_SAMPLE_INTERVAL_MS, "ms"),
    ("P9", "destroy", P9_DESTROY_BOX_S, "s"),
    ("P10", "terminal-poll", P10_TERMINAL_POLL_BOX_S, "s"),
    ("P10", "terminal-poll-interval", P10_POLL_INTERVAL_MS, "ms"),
    ("P12", "race-window", P12_RACE_WINDOW_MS, "ms"),
    ("D6", "steps-immediate", 1 if D6_STEP_IMMEDIATE else 0, "flag"),
)

# ---------------------------------------------------------------------------
# 观测窗冻结值（规格 :1056-1065）
# ---------------------------------------------------------------------------

#: 求值观测窗时长（自首个 N1BDISC_ marker 起算；:1058 冻结值）。
OBSERVATION_WINDOW_S = 525
#: 主线串行时间盒上界之和（:1059 推导冻结）。
SERIAL_BOX_UPPER_BOUND_S = 467
#: 收尾裕量（:1059）。
CLOSE_MARGIN_S = 58
#: HilogStream 墙钟兜底上界（≥825 s = Allow 盒 + 求值窗；:1063，仅 runner 异常熔断）。
HILOG_WALLCLOCK_FALLBACK_S = 825

#: P0 operator-ready 动作字面（:1053 冻结）。
OPERATOR_READY_ACTION = "operator-ready-confirmed"

#: 收口结局闭集。
CLOSE_COMPLETE = "complete-seal"          # POST 在 → 正常封签（protocol=complete）
CLOSE_PRE_ONLY = "pre-only"               # 仅 PRE + 死亡证据（合法终态）
CLOSE_FAIL_F9 = "fail-f9-window-alive"    # 仅 PRE 无死亡 → F9（存活未完成）
CLOSE_FAIL_PRE_MISSING = "fail-pre-missing"  # PRE 缺 → F2（完整性失败）
CLOSE_FAIL_ALLOW_CONSUMED = "fail-allow-consumed"  # Allow 盒到点无 marker → 已消费收口


class FsmError(Exception):
    """状态机非法迁移（位次/域违反；属探针/runner 缺陷面）。"""


@dataclass(frozen=True)
class FsmEvent:
    """状态机迁移事件（kind + 伴随时刻/详情）。"""

    kind: str
    mono_ms: Optional[int] = None
    detail: str = ""


@dataclass
class ObservationWindowFSM:
    """观测窗与 P0 门流程状态机（纯逻辑；时刻全部由调用方注入的单调钟读数）。"""

    state: str = "idle"
    p0_ready_mono_ms: Optional[int] = None
    operator_ready_action: Optional[str] = None
    operator_ready_wall_ms: Optional[int] = None
    hilog_started_mono_ms: Optional[int] = None
    start_entry_mono_ms: Optional[int] = None
    first_marker_name: Optional[str] = None
    window_start_mono_ms: Optional[int] = None
    pre_seen: bool = False
    post_seen: bool = False
    post_mono_ms: Optional[int] = None
    closed: bool = False
    close_kind: Optional[str] = None
    close_mono_ms: Optional[int] = None
    late_markers: List[Tuple[str, int]] = field(default_factory=list)
    events: List[FsmEvent] = field(default_factory=list)

    # -- P0 前置 ------------------------------------------------------------

    def register_operator_ready(self, mono_ms: int, wall_ms: int) -> FsmEvent:
        """operator-ready 确认步（规格 :1053）：登记单调/墙钟双时刻 + 动作字面。

        ``p0_ready_mono_ms`` 受 BL-4 单调钟读数非负门约束（规格 :826/:828）。
        """
        self._require(self.state == "idle", "operator-ready 必须是首个状态迁移")
        self._require(isinstance(mono_ms, int) and mono_ms >= 0,
                      "p0_ready_mono_ms 违反单调钟读数非负门（BL-4）")
        self.state = "operator-ready"
        self.p0_ready_mono_ms = mono_ms
        self.operator_ready_action = OPERATOR_READY_ACTION
        self.operator_ready_wall_ms = wall_ms
        return self._emit(FsmEvent("operator-ready-confirmed", mono_ms))

    def start_hilog_stream(self, mono_ms: int) -> FsmEvent:
        """HilogStream 启动（规格 :1062：Live 序中先于 StartEntry 启动）。"""
        self._require(self.state == "operator-ready",
                      "HilogStream 启动前必须有 operator-ready 确认")
        self.state = "hilog-streaming"
        self.hilog_started_mono_ms = mono_ms
        return self._emit(FsmEvent("hilog-stream-started", mono_ms))

    def issue_start_entry(self, mono_ms: int) -> FsmEvent:
        """StartEntry 发出（规格 :1053/:1062：不得先于 HilogStream / operator-ready）。

        Allow 300 s 盒自本时刻起算（规格 :1033）。
        """
        self._require(self.state == "hilog-streaming",
                      "StartEntry 必须先于 HilogStream 之后发出（HilogStream 先于 StartEntry）")
        self.state = "allow-wait"
        self.start_entry_mono_ms = mono_ms
        return self._emit(FsmEvent("start-entry-issued", mono_ms))

    # -- 盒边界 -------------------------------------------------------------

    @property
    def allow_deadline_mono_ms(self) -> Optional[int]:
        if self.start_entry_mono_ms is None:
            return None
        return self.start_entry_mono_ms + ALLOW_BOX_S * 1000

    @property
    def window_deadline_mono_ms(self) -> Optional[int]:
        if self.window_start_mono_ms is None:
            return None
        return self.window_start_mono_ms + OBSERVATION_WINDOW_S * 1000

    # -- capture 喂入 -------------------------------------------------------

    def feed_marker(self, name: str, mono_ms: int) -> Optional[FsmEvent]:
        """capture 出现一枚 N1BDISC marker 时喂入。

        窗前首枚 marker 开启观测窗（规格 :1062）；已收口后到达的 marker 一律登记
        ``late_marker_observed``、不参与求值（规格 :1066）。Allow 盒到点后到达的
        首枚 marker 同样按已消费收口处理（观测窗从未开启）。
        """
        self._require(bool(name) and name.startswith("N1BDISC_"),
                      "feed_marker 只接受 N1BDISC_ marker")
        if self.closed:
            self.late_markers.append((name, mono_ms))
            return self._emit(FsmEvent("late_marker_observed", mono_ms, name))
        if self.state == "allow-wait" and self.allow_deadline_mono_ms is not None \
                and mono_ms >= self.allow_deadline_mono_ms:
            # 已消费 campaign 收口（规格 :1054/:1065）：观测窗从未开启。
            self._close(CLOSE_FAIL_ALLOW_CONSUMED, mono_ms)
            self.late_markers.append((name, mono_ms))
            return self._emit(FsmEvent("late_marker_observed", mono_ms, name))
        if self.state == "allow-wait":
            self.state = "window-open"
            self.first_marker_name = name
            self.window_start_mono_ms = mono_ms
            self._emit(FsmEvent("window-opened", mono_ms, name))
        if name == "N1BDISC_PRE":
            self.pre_seen = True
        elif name == "N1BDISC_POST":
            self.post_seen = True
            self.post_mono_ms = mono_ms
        return None

    # -- 收口 ---------------------------------------------------------------

    def expire(self, now_mono_ms: int, death_observed: Optional[bool]) -> FsmEvent:
        """观测窗到点（或 host finally 提前收口）时的四分支判定（规格 :1066-1068）。

        ``death_observed`` = 死亡分量 ``process_death_observed`` 的机器承载：
        ``True`` = observed-true（pre-only 合法终态前提）；``False``/``None``
        （observed-false / unobservable）在仅 PRE 在场时一律落 F9 fail
        （规格 :1067：进程仍存活或状态不可判 → 存活未完成不是平台事实）。
        """
        self._require(not self.closed, "状态机已收口，不得二次 expire")
        if self.state == "allow-wait":
            if self.allow_deadline_mono_ms is None \
                    or now_mono_ms < self.allow_deadline_mono_ms:
                raise FsmError("Allow 盒未到点，不得提前按已消费收口（用 expire 前先核对盒边界）")
            self._close(CLOSE_FAIL_ALLOW_CONSUMED, now_mono_ms)
        elif self.state == "window-open":
            self._require(self.window_deadline_mono_ms is not None
                          and now_mono_ms >= self.window_deadline_mono_ms,
                          "观测窗未到点（窗到点前收口由 host finally 路径显式调用 "
                          "close_by_host_finally）")
            self._close(self.decide_close_from_state(death_observed), now_mono_ms)
        else:
            raise FsmError("当前状态 %r 无收口面" % (self.state,))
        return self.events[-1]

    def close_by_host_finally(self, now_mono_ms: int,
                              death_observed: Optional[bool]) -> FsmEvent:
        """host finally 步骤 9 的终态判定（规格 :1658-1661）：窗到点前触发时走同一四分支。"""
        self._require(not self.closed, "状态机已收口，不得二次收口")
        self._require(self.state == "window-open",
                      "host finally 收口要求观测窗已开启（PRE 缺失场景由 expire 的 "
                      "allow-consumed 分支或窗到点覆盖）")
        self._close(self.decide_close_from_state(death_observed), now_mono_ms)
        return self.events[-1]

    @staticmethod
    def decide_close(pre_seen: bool, post_seen: bool,
                     death_observed: Optional[bool]) -> str:
        """四分支纯函数（规格 :1066-1068 逐字）：POST 在 / 仅 PRE+死亡 / 仅 PRE / PRE 缺。"""
        if pre_seen and post_seen:
            return CLOSE_COMPLETE
        if pre_seen and not post_seen:
            return CLOSE_PRE_ONLY if death_observed is True else CLOSE_FAIL_F9
        return CLOSE_FAIL_PRE_MISSING

    def decide_close_from_state(self, death_observed: Optional[bool]) -> str:
        """以实例当前 PRE/POST 存在性执行四分支（规格 :1066-1068）。"""
        return self.decide_close(self.pre_seen, self.post_seen, death_observed)

    # -- 内部 ---------------------------------------------------------------

    def _close(self, kind: str, mono_ms: int) -> None:
        self.closed = True
        self.close_kind = kind
        self.close_mono_ms = mono_ms
        self._emit(FsmEvent("closed", mono_ms, kind))

    def _emit(self, event: FsmEvent) -> FsmEvent:
        self.events.append(event)
        return event

    @staticmethod
    def _require(cond: bool, msg: str) -> None:
        if not cond:
            raise FsmError(msg)


# ---------------------------------------------------------------------------
# 冻结全序位点映射（规格 :1012-1027；marker → 位点）
# ---------------------------------------------------------------------------

#: 全序位点（:1012-1027 冻结全序；P5T 位于 P5 与 P6 之间，r5 U1）。
SITE_ORDER: Tuple[str, ...] = (
    "P1", "P2", "P3", "P4", "P5", "P5T", "P6",
    "P7", "P8", "P9", "P10", "P11", "P12",
)

#: 阶段 marker 前缀 → 位点（家族字面 ``D1_*`` 等按前缀匹配；逐字全名见 SITE_OF_MARKER）。
SITE_PREFIX_MARKER: Tuple[Tuple[str, str], ...] = (
    ("N1BDISC_D1_", "P1"), ("N1BDISC_D2_", "P2"), ("N1BDISC_D4_", "P3"),
    ("N1BDISC_D5_", "P4"), ("N1BDISC_D7_", "P6"),
)

#: 阶段 marker 逐字全名 → 位点（死亡位点判定 = capture 中最后一个成功发出的阶段
#: marker，规格 :1228；FD/SKIP/CHUNK 位点随分支漂移、不参与阶段序校验）。
SITE_OF_MARKER: dict = {}
for _name, _site in (
    ("N1BDISC_D8_MTU", "P5"), ("N1BDISC_PRE", "P5T"),
    ("N1BDISC_D8_STORM_BEGIN", "P7"), ("N1BDISC_D8_STORM_END", "P7"),
    ("N1BDISC_DW_SPAWN", "P8"), ("N1BDISC_DW_DRAIN", "P8"),
    ("N1BDISC_DW_BARRIER", "P8"), ("N1BDISC_DW_INWAIT", "P8"),
    ("N1BDISC_DW_DESTROY_T", "P9"), ("N1BDISC_DW_DESTROY_C", "P9"),
    ("N1BDISC_D6S1_B", "P9"), ("N1BDISC_D6S2_B", "P9"), ("N1BDISC_D6S3_B", "P9"),
    ("N1BDISC_D6S1_R", "P9"), ("N1BDISC_D6S2_R", "P9"), ("N1BDISC_D6S3_R", "P9"),
    ("N1BDISC_DW_RETURN", "P10"), ("N1BDISC_DW_EXIT", "P10"),
    ("N1BDISC_D6S4_B", "P10"), ("N1BDISC_D6S5_B", "P10"),
    ("N1BDISC_D6S6_B", "P10"), ("N1BDISC_D6S7_B", "P10"),
    ("N1BDISC_D6S4_R", "P10"), ("N1BDISC_D6S5_R", "P10"),
    ("N1BDISC_D6S6_R", "P10"), ("N1BDISC_D6S7_R", "P10"),
    ("N1BDISC_DW_RACEWIN", "P12"), ("N1BDISC_POST", "P12"),
):
    SITE_OF_MARKER[_name] = _site


def site_of(name: str) -> Optional[str]:
    """marker 字面 → 全序位点；FD/SKIP/CHUNK 等位点漂移 marker 返回 ``None``。"""
    if name in SITE_OF_MARKER:
        return SITE_OF_MARKER[name]
    for prefix, site in SITE_PREFIX_MARKER:
        if name.startswith(prefix):
            return site
    return None


def stage_order_violations(site_sequence: List[str]) -> List[str]:
    """观测位点序列相对冻结全序的破坏列表（空列表 = 全序保持）。

    规格口径：``protocol=complete`` 时校验 P1-P12 全序（:1127）；``pre-only`` 时
    死亡位点之前的已完成项 marker 有序（:1128）——两者均为「观测序列是冻结全序的
    子序列」检查（skip 分支使位点缺席合法，缺席不破坏全序，规格 :1010）。
    """
    violations: List[str] = []
    order_index = {s: i for i, s in enumerate(SITE_ORDER)}
    last = -1
    for site in site_sequence:
        idx = order_index.get(site)
        if idx is None:
            continue
        if idx < last:
            violations.append("site-order-violation: %s after index %d"
                              % (site, last))
        else:
            last = idx
    return violations


__all__ = [
    # 时间盒常量
    "ALLOW_BOX_S", "P1_DLOPEN_BOX_S", "P2_CREATE_BOX_S",
    "P2_LATE_OBSERVATION_WINDOW_S", "P3_D4_WINDOW_TOTAL_S", "P3_D4_POLL_MS",
    "P4_D5_WINDOW_TOTAL_S", "P4_D5_ROUND_MS", "P4_D5_MAX_ROUNDS",
    "P5_D8A_PER_LEVEL_POLL_MS", "P5_D8A_PER_LEVEL_MAX_S", "P5_D8A_TOTAL_S",
    "P5T_PRE_EMIT_S", "P6_D7_TASK_S", "P6_D7_GRACE_S",
    "P7_D8B_STORM_S", "P7_D8B_STORM_BYTES", "P7_D8B_STORM_MAX_WRITES",
    "P8_DRAIN_BOX_S", "P8_BARRIER_BOX_S", "P8_INWAIT_SAMPLE_S",
    "P8_INWAIT_SAMPLE_INTERVAL_MS", "P9_DESTROY_BOX_S",
    "P10_TERMINAL_POLL_BOX_S", "P10_POLL_INTERVAL_MS", "P12_RACE_WINDOW_MS",
    "D6_STEP_IMMEDIATE", "JOIN_UNBOUNDED", "TIME_BOX_TABLE",
    # 观测窗
    "OBSERVATION_WINDOW_S", "SERIAL_BOX_UPPER_BOUND_S", "CLOSE_MARGIN_S",
    "HILOG_WALLCLOCK_FALLBACK_S", "OPERATOR_READY_ACTION",
    # 收口闭集
    "CLOSE_COMPLETE", "CLOSE_PRE_ONLY", "CLOSE_FAIL_F9",
    "CLOSE_FAIL_PRE_MISSING", "CLOSE_FAIL_ALLOW_CONSUMED",
    # 状态机
    "FsmError", "FsmEvent", "ObservationWindowFSM",
    # 冻结全序位点
    "SITE_ORDER", "SITE_OF_MARKER", "SITE_PREFIX_MARKER", "site_of",
    "stage_order_violations",
]
