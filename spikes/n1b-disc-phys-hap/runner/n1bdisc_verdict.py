# -*- coding: utf-8 -*-
"""n1bdisc_verdict — N1BDISC host runner verdict 骨架（机器规则，fail-closed）。

权威规格：``docs/n1b-disc-gate-plan.md``「verdict 求值与聚合」（:1109-1203）、
「criteria-gap 处理」（:1369-1382）、D-W 节 cut-state 规则（:772-789）、
``dw_watchdog_killed`` 有序互斥表（:871-895）。

本模块实现：

1. **求值序**（:1115-1120 单一 if/else 链）：invalid → fail → blocked → pass；
2. **fail 闭集 F1-F9**（:1142-1152；F7 归 invalid 轴、不在 fail 闭集）；
3. **protocol 双值**（:1166-1198）：{``complete``, ``pre-only``}，由双 marker
   存在性唯一确定；PRE 缺 → F2；
4. **criteria-gap 两分法**（:1375-1379）：域外有效平台值 → ``unobservable(cause=
   value-outside-frozen-domain)`` 不 fail；解析域缺口 (2)/真值表缺口 (3)/未预注册
   cause (4) → fail；
5. **dw_return_class runner 重建 vs 探针 P12 派生比对**（不一致 F8(2)，:1159/:1559）；
6. **cut-state (A)(B)(C)** 与允许集闭表（:772-789，r22 TOCTOU 闭合）；
7. **join sticky / class sticky（RACEWIN 签名）例外**（:755/:760/:767）；
8. **``dw_watchdog_killed`` 有序互斥表①-⑤**（:874-894，先到先得命中即终止）。
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Dict, List, Mapping, Optional, Sequence, Tuple

import n1bdisc_core as core

#: protocol 取值域恰两值（:1132 r5 U7）。
PROTOCOL_VALUES: Tuple[str, ...] = ("complete", "pre-only")

_UNOBS = core.unobservable_value

# F 索引（:1143-1152；F7 归 invalid 轴、不判 fail）
F1, F2, F3, F4, F5, F6, F7, F8, F9 = (
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9")


# ---------------------------------------------------------------------------
# criteria-gap 两分法（:1375-1379）
# ---------------------------------------------------------------------------

def classify_criteria_gap(case: int, raw: str = "") -> Tuple[str, Optional[str]]:
    """判别方法 (1)-(4) 的两分法路由。

    返回 ``(axis, fail_gate)``：case 1 → ``("unobservable", None)``（域外有效平台值
    不 fail，落 ``value-outside-frozen-domain``）；case 2/3 → ``("fail", F8)``；
    case 4 → ``("fail", F4)``（未预注册 cause）。
    """
    if case == 1:
        return "unobservable:" + _UNOBS("value-outside-frozen-domain"), None
    if case == 2:
        return "fail", F8
    if case == 3:
        return "fail", F8
    if case == 4:
        return "fail", F4
    raise ValueError("criteria-gap case out of {1,2,3,4}: %r" % (case,))


# ---------------------------------------------------------------------------
# protocol 派生（:1166-1198）
# ---------------------------------------------------------------------------

def derive_protocol(pre_present: bool, post_present: bool) -> Optional[str]:
    """双 marker 存在性唯一确定 protocol；PRE 缺 → ``None``（F2 承载，:1133）。"""
    if pre_present and post_present:
        return "complete"
    if pre_present and not post_present:
        return "pre-only"
    return None


# ---------------------------------------------------------------------------
# dw_return_class 重建比对与 sticky 例外
# ---------------------------------------------------------------------------

#: POST ``dw_outcome`` 内层字段集（观察 (ii) 整改钉）。POST 内层布局判据未冻结
#（判据 :439 只冻结 POST 外层四字段集；内层全列形态明文「未冻结」）——CC 后实现
#: 契约 = 探针实际格式，即 ``probe/src/dw.rs:1008-1018`` ``post_emit`` 的 format!
#: 字面：内层以 ``;`` 分隔 k=v 对、字段集恰八项（强校验：缺失或多余键 → 解析失败
#: 返回空映射；探针发射位序固定仅为登记事实，解析不校验位序）（探针值域均不含 ``;``：
#: ``sanitize_marker_field`` 只转义 ``|``/控制符，util.rs:199-214）。
DW_OUTCOME_FIELDS: Tuple[str, ...] = (
    "class", "join", "watchdog", "dist",
    "poll_ret", "poll_errno", "poll_revents", "poll_elapsed_ms",
)

#: POST ``dw_outcome`` 内层 poll 四字段名 → 冻结闭表字段名（:776-777，
#: :data:`POLL_RAW_FIELDS` 闭表身份不变）。探针内层名无 ``dw_`` 前缀、elapsed 带
#: 内层后缀 ``_ms``（dw.rs:1009 字面），比对取值按本映射换算。
DW_OUTCOME_POLL_RAW_KEYS: Dict[str, str] = {
    "poll_ret": "dw_poll_ret",
    "poll_errno": "dw_poll_errno",
    "poll_revents": "dw_poll_revents",
    "poll_elapsed_ms": "dw_poll_return_elapsed_ms",
}


def parse_dw_outcome(dw_outcome: str) -> Dict[str, str]:
    """POST ``dw_outcome`` 全列的子字段拆解（探针实际格式：``;`` 分隔 k=v）。

    契约 = 探针实现字面（``probe/src/dw.rs:1008-1018`` ``post_emit``；判据未冻结
    该内层布局，见 :data:`DW_OUTCOME_FIELDS` 登记注）。fail-closed：任何结构违反
    （缺 ``=``、未知键、重复键、空值、值内出现 ``,``——旧 DryRun 逗号约定的域外
    字符、**键集合 ≠ 恰八字段集**——缺失或多余）→ 返回空映射，全列不可用；
    下游 P12 比对/cut-state 闭表按缺席落 F8(2)，
    不得宽松接受垃圾列（真机分隔符漂移必须 fail，不许静默产垃圾值）。
    """
    out: Dict[str, str] = {}
    for part in dw_outcome.split(";"):
        key, eq, value = part.partition("=")
        if (not eq or key not in DW_OUTCOME_FIELDS or key in out
                or not value or "," in value):
            return {}
        out[key] = value
    # 字段集强校验：恰八字段（缺失即失败）；位序不校验（见上方登记注）。
    if set(out) != set(DW_OUTCOME_FIELDS):
        return {}
    return out


def p12_class_consistency(rebuilt_class: str, post_class: Optional[str]) -> bool:
    """F8(2) 挂载比对：runner 重建值 vs P12 派生值（:1159/:1559）。"""
    if post_class is None:
        return False
    return rebuilt_class == post_class


def p12_join_consistency(rebuilt_join: Optional[str],
                         post_join: Optional[str],
                         post_class: Optional[str] = None) -> bool:
    """F8(2) 挂载比对（join 轴）：runner 重建值 vs POST 内层 ``join=`` 字面。

    B4-b 整改：**严格十值域解析**——``post_join`` 必须逐字 ∈
    :data:`core.DW_JOIN_RESULT_10`（:870/:443：D-W skip 形态由 dw.rs
    ``join_result_fallback`` 逐字携带 skip 编码，``pending`` 自探针修复后
    不再出现在任何合法发射格；旧「pending + skip 重建一致」的宽松接受随
    blocker 删除）。域外字面/缺字段 → 不一致（fail-closed，调用方挂 F8(2)）；
    ``post_class`` 形参保留仅为调用方签名兼容，不再参与判定。
    """
    if post_join is None or rebuilt_join is None:
        return False
    if post_join not in core.DW_JOIN_RESULT_10:
        return False
    return post_join == rebuilt_join


def racewin_sticky_signature(d6b_skip_present: bool,
                             racewin_present: bool) -> bool:
    """class sticky 例外签名（r21/r22 (B)(i)：``D6b skip ∧ DW_RACEWIN``、无 R 前件）。"""
    return d6b_skip_present and racewin_present


def apply_join_stickiness(jt_registered: bool, join_value: Optional[str]) -> Optional[str]:
    """join sticky（:755/:783）：JT 登记 → ``join-timeout``，不由迟到完成改写。"""
    if jt_registered:
        return "join-timeout"
    return join_value


# ---------------------------------------------------------------------------
# cut-state (A)(B)(C)（:772-789）
# ---------------------------------------------------------------------------

#: cut=false 格 class 允许域（(B)(ii)）。
CUT_FALSE_CLASSES: Tuple[str, ...] = (
    _UNOBS("poll-never-returned"), _UNOBS("flag-race-window-expired"))
#: cut=false 格 watchdog 允许集恰一值 = ⑤（cut-imputed，:778-779）。
CUT_FALSE_WATCHDOG_ALLOWED = _UNOBS("marker-gap-indeterminate")
#: poll raw 四字段（:776-777 允许集闭表作用域）。
POLL_RAW_FIELDS: Tuple[str, ...] = (
    "dw_poll_ret", "dw_poll_errno", "dw_poll_revents", "dw_poll_return_elapsed_ms")


def cut_state_b_violations(cut_false: bool, class_value: str,
                           poll_raw: Mapping[str, str],
                           watchdog_value: str,
                           racewin_present: bool) -> List[str]:
    """cut=false 格分支一致性校验（:777-788 闭表；违规列表非空 → F8(2) fail）。

    (i) ``RACEWIN`` 在 ⟺ cut=false ∧ class=flag-race（双向）；(ii) class ∈ 允许域；
    闭表字段值：poll raw 四字段与 watchdog 恰一允许值——迟到/决策前已发射的
    RETURN/EXIT 的 capture 存在性**不参与**（:780-782）。
    """
    violations: List[str] = []
    if not cut_false:
        return violations
    expected_racewin = class_value == _UNOBS("flag-race-window-expired")
    if racewin_present != expected_racewin:
        violations.append("(i) RACEWIN-in != (cut=false && class=flag-race)")
    if class_value not in CUT_FALSE_CLASSES:
        violations.append("(ii) class out of cut=false allowed domain: %r"
                          % (class_value,))
        return violations
    cause = class_value[len("unobservable(cause="):-1]
    allowed = _UNOBS(cause)
    for name in POLL_RAW_FIELDS:
        value = poll_raw.get(name)
        if value != allowed:
            violations.append("closed-table %s=%r not in {%r}" % (name, value, allowed))
    if watchdog_value != CUT_FALSE_WATCHDOG_ALLOWED:
        violations.append("closed-table watchdog=%r not in {%r}"
                          % (watchdog_value, CUT_FALSE_WATCHDOG_ALLOWED))
    return violations


def cut_state_a_violations(return_present: bool, exit_present: bool) -> List[str]:
    """cut=true 格（(A)）：RETURN 与 EXIT 必在 capture（缺任一 → F8(2)，:774）。"""
    violations: List[str] = []
    if not return_present:
        violations.append("(A) RETURN missing in capture")
    if not exit_present:
        violations.append("(A) EXIT missing in capture")
    return violations


# ---------------------------------------------------------------------------
# dw_watchdog_killed 有序互斥表①-⑤（:874-894）
# ---------------------------------------------------------------------------

def eval_watchdog_killed(*,
                         skip_cause: Optional[str] = None,
                         c_present: bool,
                         t_present: bool,
                         pidof_absent: bool,
                         exit_present: bool,
                         spawn_present: bool,
                         capture_silent_after_spawn: bool,
                         death_observed: bool,
                         inwait_confirmed: str,
                         death_site: Optional[str]) -> str:
    """有序互斥表（:874-894）：求值序①→⑤写死、先到先得命中即终止。

    skip 具名清单位于表外（:894）：skip 路径无 waiter 运行、不进入①-⑤求值。
    """
    if skip_cause is not None:
        return _UNOBS(skip_cause)
    # ① `_C` 在 ∧ pidof absent ∧ EXIT 缺 → destroy-terminal-candidate（:875）
    if c_present and pidof_absent and not exit_present:
        return _UNOBS("destroy-terminal-candidate")
    # ② `_T` 在、`_C` 缺、EXIT 缺 → call-boundary-incomplete 透传（:878）
    if t_present and not c_present and not exit_present:
        return _UNOBS("call-boundary-incomplete")
    # ③ 五合取（:881-886）：SPAWN ∧ EXIT 缺 ∧ 静默 ∧ 死亡分量 ∧ inwait 确认 ∧ 位点窗口
    if (not t_present and not c_present
            and spawn_present and not exit_present
            and capture_silent_after_spawn
            and death_observed
            and death_site in ("P8", "P9", "P10")
            and inwait_confirmed == "observed-true"):
        return "observed-true"
    # ④ EXIT 在 → observed-false（:890）
    if exit_present:
        return "observed-false"
    # ⑤ 其余一切输入（:891）
    return _UNOBS("marker-gap-indeterminate")


# ---------------------------------------------------------------------------
# verdict 主求值
# ---------------------------------------------------------------------------

@dataclass
class VerdictInput:
    """verdict 判定输入（runner 从 capture 全流 + runner 侧证据汇总）。

    判定输入仅限 marker 流（三形态关联后）、runner 侧证据与 freeze 哈希；平台事实
    三态字段永不进入 verdict 求值——唯一例外 ``probe_crash_signature_observed``
    经 F1 进入（:1113）。
    """

    # invalid 轴（:1124 + F7 :1149）
    hdc_whitelist_violations: List[str] = field(default_factory=list)
    freeze_integrity_failures: List[str] = field(default_factory=list)
    cross_attempt_splices: List[str] = field(default_factory=list)
    # blocked 轴（:1162）
    blocked_conditions: List[str] = field(default_factory=list)
    # 终态 marker 与 protocol
    pre_present: bool = False
    post_present: bool = False
    crash_signature: str = "observed-false"
    # F3：全序/时序破坏（含 SKIP 同现域门、_T/_C mono 逆序）
    order_violations: List[str] = field(default_factory=list)
    # F4：冻结字段缺项 / 增量落盘缺项 / 未预注册 cause
    frozen_field_missing: List[str] = field(default_factory=list)
    increment_gaps: List[str] = field(default_factory=list)
    unregistered_cause_hits: List[str] = field(default_factory=list)
    # F5：封签失败 / ledger 缺失或矛盾 / 同切点 digest 不符
    ledger_failures: List[str] = field(default_factory=list)
    sealing_failed: bool = False
    # F6
    d1_cmdline_ok: Optional[bool] = None
    # F8：解析域缺口 (2)（含 P12 派生不一致）/ 真值表缺口 (3) / 单调钟绝对域违反
    parse_domain_gaps: List[str] = field(default_factory=list)
    truth_table_gaps: List[str] = field(default_factory=list)
    monotonic_domain_violations: List[str] = field(default_factory=list)
    # F9：观测窗到点、仅 PRE 在、process_death_observed != observed-true
    window_expired_alive: bool = False
    # complete + 崩溃签名 → 观察登记（不驱动 verdict，:1136）
    abnormal_observations: List[str] = field(default_factory=list)


@dataclass
class VerdictResult:
    """verdict 结果：主判定 + 命中 gate 明细（MJ-13 单一 if/else 链的产出）。"""

    verdict: str                                    # invalid | fail | blocked | pass
    gates: List[str] = field(default_factory=list)  # 命中面（F1..F9 / invalid / blocked）
    details: Dict[str, List[str]] = field(default_factory=dict)

    def as_dict(self) -> Dict[str, Any]:
        return {"verdict": self.verdict, "gates": list(self.gates),
                "details": {k: list(v) for k, v in self.details.items()}}


def evaluate(inp: VerdictInput) -> VerdictResult:
    """求值序（:1115-1120）：invalid → fail → blocked → pass（单一 if/else 链）。"""
    invalid: List[str] = []
    invalid += ["hdc-whitelist: %s" % v for v in inp.hdc_whitelist_violations]
    invalid += ["freeze-integrity: %s" % v for v in inp.freeze_integrity_failures]
    invalid += ["cross-attempt-splice: %s" % v for v in inp.cross_attempt_splices]
    if invalid:
        return VerdictResult("invalid", ["invalid"], {"invalid": invalid})

    protocol = derive_protocol(inp.pre_present, inp.post_present)
    gates: List[str] = []
    details: Dict[str, List[str]] = {}

    # F1（:1143）：崩溃签名 ∧ protocol != complete；PRE 缺时 protocol 不可求值 →
    # 该输入由 F2 独立承载（:1133），F1 不进入求值。
    if protocol is not None and inp.crash_signature == "observed-true" \
            and protocol != "complete":
        gates.append(F1)
        details[F1] = ["crash signature observed-true with protocol=%s" % protocol]
    if protocol is None:
        gates.append(F2)
        details[F2] = ["N1BDISC_PRE missing (POST present=%s)" % inp.post_present]
    if inp.order_violations:
        gates.append(F3)
        details[F3] = list(inp.order_violations)
    if inp.frozen_field_missing or inp.increment_gaps or inp.unregistered_cause_hits:
        gates.append(F4)
        details[F4] = (["frozen-field-missing: %s" % v for v in inp.frozen_field_missing]
                       + ["increment-gap: %s" % v for v in inp.increment_gaps]
                       + ["unregistered-cause: %s" % v for v in inp.unregistered_cause_hits])
    if inp.sealing_failed or inp.ledger_failures:
        gates.append(F5)
        details[F5] = (["sealing-failed"] if inp.sealing_failed else []) \
            + list(inp.ledger_failures)
    if inp.d1_cmdline_ok is False:
        gates.append(F6)
        details[F6] = ["d1_cmdline not :vpn process"]
    if inp.parse_domain_gaps or inp.truth_table_gaps or inp.monotonic_domain_violations:
        gates.append(F8)
        details[F8] = (["parse-domain-gap: %s" % v for v in inp.parse_domain_gaps]
                       + ["truth-table-gap: %s" % v for v in inp.truth_table_gaps]
                       + ["monotonic-domain: %s" % v for v in inp.monotonic_domain_violations])
    if inp.window_expired_alive:
        gates.append(F9)
        details[F9] = ["window expired, only PRE present, "
                       "process_death_observed != observed-true"]

    if gates:
        return VerdictResult("fail", gates, details)

    if inp.blocked_conditions:
        return VerdictResult("blocked", ["blocked"],
                             {"blocked": list(inp.blocked_conditions)})
    return VerdictResult("pass", [], {})


__all__ = [
    "PROTOCOL_VALUES", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9",
    "classify_criteria_gap", "derive_protocol", "parse_dw_outcome",
    "DW_OUTCOME_FIELDS", "DW_OUTCOME_POLL_RAW_KEYS",
    "p12_class_consistency", "racewin_sticky_signature", "apply_join_stickiness",
    "p12_join_consistency",
    "CUT_FALSE_CLASSES", "CUT_FALSE_WATCHDOG_ALLOWED", "POLL_RAW_FIELDS",
    "cut_state_b_violations", "cut_state_a_violations",
    "eval_watchdog_killed", "VerdictInput", "VerdictResult", "evaluate",
]
