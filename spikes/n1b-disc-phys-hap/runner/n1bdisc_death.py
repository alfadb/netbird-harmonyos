# -*- coding: utf-8 -*-
"""n1bdisc_death — N1BDISC host runner 死亡事实（七分量证据向量，host-only）。

权威规格：``docs/n1b-disc-gate-plan.md``「死亡事实记录（证据向量）」（:1205-1367）
与「criteria-gap 处理」（:1369-1382）。

本模块实现：

1. **快照差分**（:1213-1215）：campaign 时间窗归属**仅且仅由快照文件集合差分**决定，
   不使用任何墙钟比较；fault 条目内时间字段只作观察数据逐字落盘（唯一例外
   ``marker_tail_state`` 的静默跨度比较，两端同取设备侧时钟）。
2. **fault 条目解析契约**（:1234-1242）：字段按行首字面匹配、值取该行 ``:`` 之后
   首个非空白 token 起至行尾；``Fault_Type`` 归一化（去 ``_``/``-`` 大写）+ 冻结候选集
   恰三项 {``APPFREEZE``, ``CPPCRASH``, ``JSRAWERROR``}，域外记 ``other:<原值逐字>``；
   ``Signal`` 三段（探针 crash / 平台终止 / 其余）；时间字段仅观察。
3. **多条目聚合**（:1244-1255）：全窗聚合单值、文件名字节序聚合、正向优先不稀释、
   ``FaultRecv`` 失败/字段不可解析的 unknown 支、raw 级正向谓词 (1)(2)(3)。
4. **七分量证据向量**（:1264-1347）：逐项落盘互不推导；``probe_crash_signature_observed``
   三支闭集按支求值 + true > unknown > false 聚合；``marker_tail_state`` 五值
   （``T_tail = 25000 ms`` 严格大于）；``destroy_call_state`` 五态 + SKIP 同现域门。
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any, Dict, List, Mapping, Optional, Sequence, Tuple

import n1bdisc_core as core
import n1bdisc_fsm as fsm

# ---------------------------------------------------------------------------
# 冻结常量
# ---------------------------------------------------------------------------

#: ``Fault_Type`` 归一化候选集（:1236 恰三项，逐字冻结）。
FAULT_TYPE_CANDIDATES: Tuple[str, ...] = ("APPFREEZE", "CPPCRASH", "JSRAWERROR")
#: 正向崩溃类型（F1 第 1 支，:1314）。
CRASH_FAULT_TYPES: Tuple[str, ...] = ("CPPCRASH", "JSRAWERROR")
#: 探针 crash 段信号（:1239）。
CRASH_SIGNALS: Tuple[str, ...] = ("SIGSEGV", "SIGABRT", "SIGBUS", "SIGFPE")
#: 平台终止段信号（:1240；仅 ``signal_observed`` 记录、不构成崩溃签名）。
PLATFORM_TERMINATION_SIGNALS: Tuple[str, ...] = ("SIGKILL", "SIGTERM")
#: ``marker_tail_state`` 阈值（:1359：D7 20000 + 宽限 5000；严格大于）。
T_TAIL_MS = 25000
#: 冻结短时步集（:1231：D2 保留条目锁定序列 2.1-2.7 的即返 syscall；P1 已由 R5 移出）。
SHORT_STEP_SITES = frozenset({"P2"})

_TIMESTAMP_RE = re.compile(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}")

#: unknown cause 冻结优先序（:1343）。
UNKNOWN_CAUSE_PRIORITY: Tuple[str, ...] = (
    "faultrecv-unavailable", "fault-type-unparsable",
    "signal-unparsable", "no-visible-marker",
)


def unobs(cause: str) -> str:
    """``unobservable(cause=<cause>)`` 冻结字面（沿 core 同一拼法门）。"""
    return core.unobservable_value(cause)


def cause_of(value: str) -> Optional[str]:
    """从 ``unobservable(cause=x)`` 字面取 cause；非该字面形态返回 ``None``。"""
    if isinstance(value, str) and value.startswith("unobservable(cause=") \
            and value.endswith(")"):
        return value[len("unobservable(cause="):-1]
    return None


# ---------------------------------------------------------------------------
# 1) 快照差分（窗界唯一判据）
# ---------------------------------------------------------------------------

def snapshot_diff(snapshot_names: Sequence[str],
                  current_names: Sequence[str]) -> List[str]:
    """窗界 = 文件集合差分（:1214）；返回新增文件名（文件名字节序，:1254）。"""
    snapshot = set(snapshot_names)
    return sorted(n for n in current_names if n not in snapshot)


# ---------------------------------------------------------------------------
# 2) fault 条目解析契约
# ---------------------------------------------------------------------------

def _extract_field(text: str, field_name: str) -> Optional[str]:
    """行首字面匹配取字段值：``:`` 之后首个非空白 token 起、至行尾（:1234）。"""
    for line in text.splitlines():
        if not line.startswith(field_name):
            continue
        rest = line[len(field_name):]
        if not rest.lstrip().startswith(":"):
            return None
        return rest.lstrip()[1:].lstrip()
    return None


def normalize_fault_type(raw: str) -> Tuple[str, str]:
    """归一化（去全部 ``_``/``-`` 后统一大写）+ 候选集比对（:1235-1237）。

    返回 ``(kind, value)``：``kind ∈ {"candidate", "other"}``；候选集命中时
    ``value`` = 归一化字面，域外时 ``value`` = 归一化**前**的逐字原文（供
    ``other:<原值逐字>``）。
    """
    normalized = raw.replace("_", "").replace("-", "").upper()
    if normalized in FAULT_TYPE_CANDIDATES:
        return "candidate", normalized
    return "other", raw


def classify_signal(raw: str) -> Tuple[str, str]:
    """``Signal`` 三段（:1238-1241）：crash / platform / other（原文逐字入档）。"""
    if raw in CRASH_SIGNALS:
        return "crash", raw
    if raw in PLATFORM_TERMINATION_SIGNALS:
        return "platform", raw
    return "other", raw


@dataclass(frozen=True)
class FaultEntryParse:
    """单条目解析结果（字段缺失 = ``None`` → 分量 unobservable 贡献，:1278 免责域）。"""

    file_name: str
    fault_type_raw: Optional[str]       # None = 字段行缺失/值无法提取
    fault_type_kind: Optional[str]      # candidate | other | None
    fault_type_value: Optional[str]     # 归一化字面 | 原始原文（other）
    signal_raw: Optional[str]
    signal_kind: Optional[str]          # crash | platform | other | None
    signal_value: Optional[str]
    timestamp: Optional[str]            # 首个 YYYY-MM-DD hh:mm:ss.mmm（仅观察）
    raw: str

    @property
    def time_missing(self) -> bool:
        """时间字段不可解析 → criteria-gap (2) 解析域缺口（F8 fail 面，:1242）。"""
        return self.timestamp is None


def parse_fault_entry(file_name: str, text: str) -> FaultEntryParse:
    """按证据规则 5 契约解析一条 faultlogger 文本条目（:1234-1242）。"""
    ft_raw = _extract_field(text, "Fault_Type")
    sig_raw = _extract_field(text, "Signal")
    if ft_raw is None:
        ft_kind, ft_value = None, None
    else:
        ft_kind, ft_value = normalize_fault_type(ft_raw)
    if sig_raw is None:
        sig_kind, sig_value = None, None
    else:
        sig_kind, sig_value = classify_signal(sig_raw)
    match = _TIMESTAMP_RE.search(text)
    return FaultEntryParse(
        file_name=file_name,
        fault_type_raw=ft_raw, fault_type_kind=ft_kind, fault_type_value=ft_value,
        signal_raw=sig_raw, signal_kind=sig_kind, signal_value=sig_value,
        timestamp=match.group(0) if match else None,
        raw=text,
    )


# ---------------------------------------------------------------------------
# 3) 多条目聚合（保守正向优先）
# ---------------------------------------------------------------------------

@dataclass
class FaultComponents:
    """两事件分量聚合值 + raw 档 + unknown cause 档（多条目时多值不丢，:1245-1246）。"""

    fault_type_observed: str
    signal_observed: str
    raw_predicates: Tuple[bool, bool, bool]        # raw 级正向谓词 (1)(2)(3)，:1248
    out_of_domain_literals: Tuple[str, ...]        # 域外 Fault_Type 原值全体
    signal_literals: Tuple[str, ...]               # 全部信号原文
    unknown_causes: Tuple[str, ...]                # 命中的全部 unknown cause
    time_gap_files: Tuple[str, ...]                # 时间字段不可解析的条目（F8(2) 面）

    @property
    def fault_type_unobserved(self) -> bool:
        return cause_of(self.fault_type_observed) is not None

    @property
    def signal_unobserved(self) -> bool:
        return cause_of(self.signal_observed) is not None


def aggregate_fault_components(
    parses: Sequence[FaultEntryParse],
    failed_files: Sequence[str] = (),
    fault_probe_failed: bool = False,
    last_visible_site: Optional[str] = None,
    marker_seq_ok: bool = True,
) -> FaultComponents:
    """全窗聚合单值（:1244-1255 + r13 前置/r15 raw 谓词）。

    求值序（每分量独立、跨分量不传导）：正向谓词 → 取回失败/字段不可解析 unknown 支 →
    无条目 ``observed-false`` → 域外/平台终止兜底。聚合序 = faultlogger 文件名字节序。
    """
    ordered = sorted(parses, key=lambda p: p.file_name)
    failed = set(failed_files)
    p1 = any(p.fault_type_kind == "candidate"
             and p.fault_type_value in CRASH_FAULT_TYPES for p in ordered)
    p2 = any(p.signal_kind == "crash" for p in ordered)
    p3 = (any(p.fault_type_kind == "candidate"
              and p.fault_type_value == "APPFREEZE" for p in ordered)
          and last_visible_site in SHORT_STEP_SITES and marker_seq_ok)
    out_of_domain = tuple(p.fault_type_value for p in ordered
                          if p.fault_type_kind == "other")
    signal_literals = tuple(p.signal_raw for p in ordered if p.signal_raw is not None)
    time_gaps = tuple(p.file_name for p in ordered if p.time_missing)
    unknown: List[str] = []

    # 前置·全局失败（r13，:1329）：FaultProbe 命令失败 ∨ 命中文件全部取回失败
    if fault_probe_failed or (failed and not ordered):
        cause = unobs("faultrecv-unavailable")
        unknown.append("faultrecv-unavailable")
        return FaultComponents(cause, cause, (p1, p2, p3), out_of_domain,
                               signal_literals, tuple(unknown), time_gaps)

    # ---- fault_type_observed 侧（:1245 基序 + r15 raw 谓词 + :1252 求值序）----
    # 序：正向 (1) → 位点守卫 APPFREEZE (3)（不被失败稀释）→ unknown 支（先于
    # APPFREEZE 支，:1252）→ 基则 APPFREEZE 支 → other: 兜底 → 无条目 false。
    has_appfreeze = any(p.fault_type_kind == "candidate"
                        and p.fault_type_value == "APPFREEZE" for p in ordered)
    if p1:
        hit = next(p for p in ordered
                   if p.fault_type_kind == "candidate"
                   and p.fault_type_value in CRASH_FAULT_TYPES)
        fault_value = hit.fault_type_value
    elif p3:
        fault_value = "APPFREEZE"
    elif any(p.fault_type_raw is None for p in ordered) or failed:
        # 不可解析条目/取回失败不被正向稀释、也不洗成 false（:1251-1252）；
        # 取回失败（整条证据缺失）优先于字段不可解析（:1331）。
        cause = "faultrecv-unavailable" if failed else "fault-type-unparsable"
        unknown.append(cause)
        fault_value = unobs(cause)
    elif has_appfreeze:
        fault_value = "APPFREEZE"               # 基则（:1245）：无正向但有 APPFREEZE
    elif not ordered:
        fault_value = "observed-false"          # 无条目 ≠ 不可求值（:1338）
    else:
        fault_value = "other:%s" % out_of_domain[0]

    # ---- signal_observed 侧（raw 谓词 (2) 本侧，:1246/:1249） ------------
    if p2:
        hit = next(p for p in ordered if p.signal_kind == "crash")
        signal_value = hit.signal_value
    elif any(p.signal_raw is None for p in ordered) or failed:
        cause = "faultrecv-unavailable" if failed else "signal-unparsable"
        unknown.append(cause)
        signal_value = unobs(cause)
    elif not ordered:
        signal_value = "observed-false"
    elif any(p.signal_kind == "platform" for p in ordered):
        signal_value = next(p.signal_value for p in ordered
                            if p.signal_kind == "platform")
    else:
        signal_value = "observed-false"          # 其余段信号仅 raw 入档（:1246）

    return FaultComponents(fault_value, signal_value, (p1, p2, p3),
                           out_of_domain, signal_literals, tuple(unknown),
                           time_gaps)


# ---------------------------------------------------------------------------
# 4) 七分量证据向量
# ---------------------------------------------------------------------------

def eval_process_death(positive_baseline: bool, pidof_absent: Optional[bool],
                       capture_silent: Optional[bool]) -> str:
    """``process_death_observed`` 三态（:1276-1282；r12 唯一充分条件 = (a)）。"""
    if not positive_baseline:
        return unobs("pidofvpn-no-positive-baseline")
    if pidof_absent is True and capture_silent is True:
        return "observed-true"
    if pidof_absent is False:
        return "observed-false"
    # absent 而静默不可判 / 采样本身不可判 → 宁缺勿误
    return unobs("marker-gap-indeterminate")


def eval_last_visible_site(site_sequence: Sequence[str]) -> str:
    """``last_visible_site``：capture 中最后一个成功发出的阶段 marker 位点（:1228）。"""
    known = [s for s in site_sequence if s in fsm.SITE_ORDER]
    if not known:
        return unobs("no-visible-marker")
    return known[-1]


def eval_destroy_call_state(skip_destroy: bool, t_present: bool,
                            c_present: bool) -> str:
    """``destroy_call_state`` 五态 + SKIP 同现域门（:1286-1297）。

    域门先于五态表：SKIP 与调用锚同现 → ``marker-contradiction``（verdict 轴走 F3）。
    """
    if skip_destroy and (t_present or c_present):
        return unobs("marker-contradiction")     # 同现域门（:1296）
    if c_present and not t_present:
        return unobs("marker-contradiction")     # 五态表第 5 行（verdict 走 F3）
    if skip_destroy:
        return "not-called"
    if not t_present and not c_present:
        return "not-reached"
    if t_present and not c_present:
        return unobs("call-boundary-incomplete")
    return "call-returned"


def eval_marker_tail_state(post_present: bool, death_evidence_present: bool,
                           death_wall_ms: Optional[int],
                           last_marker_wall_ms: Optional[int]) -> str:
    """``marker_tail_state`` 五值闭合枚举（:1349-1360；``T_tail`` 严格大于）。"""
    if post_present:
        return "tail-complete"
    if not death_evidence_present:
        return "no-death-evidence"
    if death_wall_ms is None or last_marker_wall_ms is None:
        return unobs("tail-clock-unresolvable")
    if death_wall_ms - last_marker_wall_ms > T_TAIL_MS:
        return "possible-tail-loss"
    return "tail-loss-not-indicated"


@dataclass
class CrashSignatureResult:
    """``probe_crash_signature_observed`` 求值结果（r12 两步，:1323-1345）。"""

    value: str                                        # observed-true/false | unobservable
    branch_values: Tuple[str, str, str] = ("false", "false", "false")
    missing_inputs: Tuple[str, ...] = ()
    unknown_causes: Tuple[str, ...] = ()


def eval_probe_crash_signature(
    fault_type_observed: str,
    signal_observed: str,
    last_visible_site: str,
    marker_seq_ok: bool = True,
) -> CrashSignatureResult:
    """三支闭集按支求值 + true > unknown > false 聚合（:1312-1345）。

    ``observed-true`` 当且仅当任一支成立；无 true 且任一支 unknown → unobservable
    （cause 沿冻结优先序取首个命中者，全部命中 cause 逐字入 raw 档）。
    """
    missing: List[str] = []
    causes: List[str] = []

    def _evaluable(value: str) -> bool:
        return cause_of(value) is None

    # 支 1：只依赖 fault_type_observed
    if _evaluable(fault_type_observed):
        b1 = "true" if fault_type_observed in CRASH_FAULT_TYPES else "false"
    else:
        b1 = "unknown"
        missing.append("fault_type_observed")
        causes.append(cause_of(fault_type_observed))
    # 支 2：只依赖 signal_observed
    if _evaluable(signal_observed):
        b2 = "true" if signal_observed in CRASH_SIGNALS else "false"
    else:
        b2 = "unknown"
        missing.append("signal_observed")
        causes.append(cause_of(signal_observed))
    # 支 3：fault_type ∧ last_visible_site ∧ marker 序列无矛盾
    site_ok = _evaluable(last_visible_site)
    if _evaluable(fault_type_observed) and site_ok and marker_seq_ok is not None:
        # marker 序列自相矛盾属可判定输入——守卫不成立 → 支 3 false（非 unknown，:1337）
        b3 = "true" if (marker_seq_ok
                        and fault_type_observed == "APPFREEZE"
                        and last_visible_site in SHORT_STEP_SITES) else "false"
    else:
        b3 = "unknown"
        if not _evaluable(fault_type_observed) and "fault_type_observed" not in missing:
            missing.append("fault_type_observed")
            causes.append(cause_of(fault_type_observed))
        if not site_ok:
            missing.append("last_visible_site")
            causes.append(cause_of(last_visible_site))
    branches = (b1, b2, b3)
    if "true" in branches:
        return CrashSignatureResult("observed-true", branches,
                                    tuple(missing), tuple(causes))
    if "unknown" in branches:
        present = [c for c in UNKNOWN_CAUSE_PRIORITY if c in causes]
        value = unobs(present[0]) if present else unobs("marker-gap-indeterminate")
        return CrashSignatureResult(value, branches, tuple(missing), tuple(causes))
    return CrashSignatureResult("observed-false", branches,
                                tuple(missing), tuple(causes))


# ---------------------------------------------------------------------------
# 5) D8b 死亡收口三分流（r12：BEGIN-only / 阶段未达两支 + 存活未完成 F9 面）
# ---------------------------------------------------------------------------

#: D8b BEGIN-only 死亡收口 cause（规格 :604：一条 cause 覆盖 END 承载五字段 +
#: ``window_end_monotonic``）。
STORM_INCOMPLETE_CAUSE = "storm-incomplete-pre-only"
#: D8b 阶段未达死亡收口 cause（规格 :610：七个落盘字段全列；u7 支复用同字面，
#: 规格 :659——同域同义跨字段共用）。
STAGE_NOT_REACHED_CAUSE = "stage-not-reached"

#: ``N1BDISC_D8_STORM_END`` 冻结载荷字段（规格 :616）：END 承载的 5 个落盘字段。
STORM_END_FIELDS: Tuple[str, ...] = (
    "eagain", "partial", "bytes", "calls", "caps_hit")
#: D8b 全部落盘字段（规格 :610 七字段全列；两支收口的管辖域）。
D8B_FIELDS: Tuple[str, ...] = (
    "window_start_monotonic", "window_end_monotonic",
    "eagain_observed", "partial_write_observed",
    "bytes_written_total", "write_calls", "caps_hit")
#: END 载荷字段 → D8b 落盘字段名（规格 :599/:616 同一承载）。
_STORM_END_TO_FIELD: Dict[str, str] = {
    "eagain": "eagain_observed", "partial": "partial_write_observed",
    "bytes": "bytes_written_total", "calls": "write_calls", "caps_hit": "caps_hit",
}


_INT_SRE = re.compile(r"\A-?[0-9]+\Z")


def _parse_int(text: Any) -> Optional[int]:
    """带符号十进制整数字面解析（M1：ws/we/D7 数值须有符号解析，负值才可见）。"""
    if isinstance(text, str) and _INT_SRE.match(text):
        return int(text)
    return None


#: ``eagain``/``partial`` 三态闭域（producer d8.rs:156-157 只发这两值；域外 →
#: F8 解析域缺口，不得整包照登——B5）。
STORM_THREE_STATE = frozenset({"observed-true", "observed-false"})
#: ``caps_hit`` 闭域（producer d8.rs:145-155：``none`` 或 {bytes,calls,time} 按
#: push 序的字面子集组合，恰 8 个字面——B5）。
STORM_CAPS_HIT_DOMAIN = frozenset({
    "none", "bytes", "calls", "time",
    "bytes,calls", "bytes,time", "calls,time", "bytes,calls,time",
})


def eval_d8b_closure(*, begin_kv: Optional[Mapping[str, str]] = None,
                     end_kv: Optional[Mapping[str, str]] = None,
                     death_observed: bool,
                     death_site_before_storm: bool,
                     death_wall_ms: Optional[int] = None,
                     begin_capture_wall_ms: Optional[int] = None,
                     skip_cause: Optional[str] = None) -> Dict[str, Any]:
    """D8b（storm）收口三分流（规格 :603-615 + :608/:615 对照 + F9 :1152）。

    求值序（先到先得）：

    0. skip 表指派（:995：D8a/D8b skip → 全字段
       ``unobservable(cause=no-live-fd)``/``dup-failed``）——no-live-fd /
       dup-failed 分支下 storm 从未计划执行；
    1. BEGIN ∧ END 在 → 正常重建（B5 逐字段闭域验证：ws/we 有符号解析——M1 负值/不可解析归 F8 单调钟/解析域面、不归 F4 missing；eagain/partial 三态闭域、bytes/calls 非负整数、caps_hit 8 字面闭域——bogus 值 → f8、键缺 → f4；``window_start=ws``、``window_end=we``、
       五字段自 END 载荷逐字登记；``ws > we`` 或负值 → f8 facet（单调钟域门，
       规格 :830/:829）；END 载荷字段缺 → f4 面（字段缺项）。
    2. BEGIN 在 ∧ END 缺 ∧ 死亡分量 true → BEGIN-only 收口（:603-607）：
       ``window_end`` 与 END 承载五字段各记
       ``unobservable(cause=storm-incomplete-pre-only)``；``window_start`` 自
       ``ws`` 照常重建（不受 END 缺失影响）；置
       ``storm_incomplete_pre_only=True`` 并登记跨度
       ``storm_incomplete_pre_only_span = 死亡证据墙钟 − BEGIN capture 墙钟``
       （同侧墙钟相减，``ws`` 为单调钟禁止跨钟相减，:607；B5：**:607 强制
       span**——任一墙钟不可求值时不得静默省略登记，该缺口无预注册 cause，
       按 criteria-gap 两分法 (4) 落 f4 面）。
    3. BEGIN 缺 ∧ 死亡分量 true ∧ 死亡位点 < P7 → 阶段未达收口（:609-614）：
       七字段全列各记 ``unobservable(cause=stage-not-reached)``；
       ``storm_incomplete_pre_only=False``、不登记跨度。
    4. 无死亡分量（含 BEGIN 在 END 缺而进程仍活的对照，:608/:615）→ 存活未完成：
       字段缺项沿 F4 面（missing 列表），布尔 False、无跨度——真未完成不是平台
       终态（verdict 由观测窗到点收口规则 F9 独立出）。
    5. BEGIN 缺 ∧ 死亡 ∧ 位点不早于 P7 → 真值表未覆盖 → f8 facet（fail-closed；
       B6：不伪造未预注册 cause，字段值留 None）。
    """
    out: Dict[str, Any] = {
        "branch": None,
        "window_start_monotonic": None,
        "window_end_monotonic": None,
        "eagain_observed": None, "partial_write_observed": None,
        "bytes_written_total": None, "write_calls": None, "caps_hit": None,
        "storm_incomplete_pre_only": False,
        "storm_incomplete_pre_only_span": None,
        "raw": {},
        "f4_missing": [],
        "f8_facets": [],
    }
    begin_present = begin_kv is not None
    end_present = end_kv is not None

    if skip_cause in ("no-live-fd", "dup-failed"):
        # (0) skip 表指派（:995）：D8b 整体 skip，全字段同 cause 编码
        out["branch"] = "skip"
        cause = unobs(skip_cause)
        for field_name in D8B_FIELDS:
            out[field_name] = cause
        out["raw"]["skip_cause"] = skip_cause
        return out
    if begin_present and end_present:
        out["branch"] = "rebuilt"
        # ws/we 有符号解析（M1）：负值 → F8 单调钟域门；非整数字面 → F8 解析域
        # 缺口；键缺 → F4 字段缺项——负值/不可解析不归 F4 missing。
        for key, field in (("ws", "window_start_monotonic"),
                           ("we", "window_end_monotonic")):
            raw = (begin_kv if key == "ws" else end_kv).get(key)
            if raw is None:
                out["f4_missing"].append("D8_STORM_%s.%s"
                                         % ("BEGIN" if key == "ws" else "END", key))
                continue
            value = _parse_int(raw)
            if value is None:
                out["f8_facets"].append("storm-parse: D8b %s=%r not an integer "
                                        "literal (规格 :616/:1377(2))" % (key, raw))
                continue
            if value < 0:
                out["f8_facets"].append("monotonic-domain: D8b %s=%d < 0 (规格 :829)"
                                        % (key, value))
                continue
            out[field] = value
        ws = out["window_start_monotonic"]
        we = out["window_end_monotonic"]
        if isinstance(ws, int) and isinstance(we, int) and ws > we:
            out["f8_facets"].append(
                "monotonic-domain: window_start %d > window_end %d (规格 :830)" % (ws, we))
        # 五字段逐项闭域验证（B5：bogus 值 → F8，不整包照登；键缺 → F4）
        for key in ("eagain", "partial"):
            value = end_kv.get(key)
            field = _STORM_END_TO_FIELD[key]
            if value is None:
                out["f4_missing"].append("D8_STORM_END.%s" % key)
            elif value not in STORM_THREE_STATE:
                out["f8_facets"].append(
                    "storm-parse: D8_STORM_END.%s=%r outside three-state domain "
                    "{observed-true, observed-false} (producer d8.rs:156-157)"
                    % (key, value))
            else:
                out[field] = value
        for key in ("bytes", "calls"):
            value = end_kv.get(key)
            field = _STORM_END_TO_FIELD[key]
            if value is None:
                out["f4_missing"].append("D8_STORM_END.%s" % key)
                continue
            parsed = _parse_int(value)
            if parsed is None or parsed < 0:
                out["f8_facets"].append(
                    "storm-parse: D8_STORM_END.%s=%r not a non-negative integer "
                    "(producer d8.rs:164 u64)" % (key, value))
            else:
                out[field] = value
        value = end_kv.get("caps_hit")
        if value is None:
            out["f4_missing"].append("D8_STORM_END.caps_hit")
        elif value not in STORM_CAPS_HIT_DOMAIN:
            out["f8_facets"].append(
                "storm-parse: D8_STORM_END.caps_hit=%r outside closed domain "
                "{none, bytes/calls/time subsets} (producer d8.rs:145-155)" % (value,))
        else:
            out["caps_hit"] = value
        return out

    if begin_present and not end_present and death_observed:
        # BEGIN-only 死亡收口（:603-607）
        out["branch"] = "storm-incomplete-pre-only"
        raw_ws = begin_kv.get("ws")
        ws = _parse_int(raw_ws)
        if raw_ws is None:
            out["f4_missing"].append("D8_STORM_BEGIN.ws")
        elif ws is None:
            out["f8_facets"].append("storm-parse: D8b ws=%r not an integer literal "
                                    "(规格 :616/:1377(2))" % (raw_ws,))
        elif ws < 0:
            out["f8_facets"].append("monotonic-domain: D8b ws=%d < 0 (规格 :829)" % ws)
        else:
            out["window_start_monotonic"] = ws   # ws 照常重建（:604）
            out["raw"]["storm_begin_ws"] = ws
        cause = unobs(STORM_INCOMPLETE_CAUSE)
        out["window_end_monotonic"] = cause
        for field_name in ("eagain_observed", "partial_write_observed",
                           "bytes_written_total", "write_calls", "caps_hit"):
            out[field_name] = cause
        out["storm_incomplete_pre_only"] = True
        # 跨度（:607 强制登记——B5）：死亡证据墙钟与 BEGIN capture 墙钟相减（同侧
        # 墙钟；禁止跨钟）。任一墙钟不可求值 → 不得静默省略：:607 未预注册该缺口
        # 的 unobservable cause，按 criteria-gap 两分法 (4) 落 F4 面。
        if death_wall_ms is not None and begin_capture_wall_ms is not None:
            out["storm_incomplete_pre_only_span"] = \
                death_wall_ms - begin_capture_wall_ms
        else:
            out["f4_missing"].append(
                "storm_incomplete_pre_only_span: :607 mandatory span undecidable "
                "(death_wall_ms=%r, begin_capture_wall_ms=%r)"
                % (death_wall_ms, begin_capture_wall_ms))
        out["raw"].update({
            "death_wall_ms": death_wall_ms,
            "begin_capture_wall_ms": begin_capture_wall_ms,
        })
        return out

    if not begin_present and death_observed and death_site_before_storm:
        # 阶段未达死亡收口（:609-614）
        out["branch"] = "stage-not-reached"
        cause = unobs(STAGE_NOT_REACHED_CAUSE)
        for field_name in D8B_FIELDS:
            out[field_name] = cause
        out["storm_incomplete_pre_only"] = False   # :614：不登记跨度
        return out

    if not death_observed:
        # 对照（:608/:615）：无死亡证据 = 存活未完成——字段缺项沿 F4 面、F9 独立出
        out["branch"] = "alive-incomplete"
        out["f4_missing"].extend("D8b.%s" % f for f in D8B_FIELDS)
        return out

    # BEGIN 缺 ∧ 死亡 ∧ 位点不早于 P7：真值表未覆盖（fail-closed，:1377 (3)）
    # B6：不伪造未预注册 cause（marker-gap-indeterminate 不在本支授权位），
    # 字段值留 None，F8 facet 承载。
    out["branch"] = "uncovered"
    out["f8_facets"].append(
        "d8b-closure-truth-table-gap: death without BEGIN and last_visible_site "
        "not before P7")
    return out


@dataclass
class EvidenceVector:
    """七分量证据向量（:1264-1274：逐项有值、互不推导、不合成单一死因标签）。"""

    process_death_observed: str
    last_visible_site: str
    fault_type_observed: str
    signal_observed: str
    destroy_call_state: str
    marker_tail_state: str
    probe_crash_signature_observed: str
    raw: Dict[str, Any] = field(default_factory=dict)

    def as_dict(self) -> Dict[str, Any]:
        return {
            "process_death_observed": self.process_death_observed,
            "last_visible_site": self.last_visible_site,
            "fault_type_observed": self.fault_type_observed,
            "signal_observed": self.signal_observed,
            "destroy_call_state": self.destroy_call_state,
            "marker_tail_state": self.marker_tail_state,
            "probe_crash_signature_observed": self.probe_crash_signature_observed,
            "raw": self.raw,
        }


__all__ = [
    "FAULT_TYPE_CANDIDATES", "CRASH_FAULT_TYPES", "CRASH_SIGNALS",
    "PLATFORM_TERMINATION_SIGNALS", "T_TAIL_MS", "SHORT_STEP_SITES",
    "UNKNOWN_CAUSE_PRIORITY",
    "STORM_INCOMPLETE_CAUSE", "STAGE_NOT_REACHED_CAUSE",
    "STORM_END_FIELDS", "D8B_FIELDS",
    "snapshot_diff", "normalize_fault_type", "classify_signal",
    "FaultEntryParse", "parse_fault_entry",
    "FaultComponents", "aggregate_fault_components",
    "eval_process_death", "eval_last_visible_site", "eval_destroy_call_state",
    "eval_marker_tail_state", "CrashSignatureResult",
    "eval_probe_crash_signature", "eval_d8b_closure",
    "EvidenceVector", "unobs", "cause_of",
]
