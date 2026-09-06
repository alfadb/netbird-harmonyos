# -*- coding: utf-8 -*-
"""n1bdisc_platform — N1BDISC host runner u1-u7 平台分量派生（纯函数，host-only）。

权威规格：``docs/n1b-disc-gate-plan.md``（只读冻结；本实现引用现行行号）。
分期项 9（gate 10 前置义务）：输入 = marker 事件序列 / chunk 重组结果 /
ledger 角色 / dw 派生输入，输出 = u1-u7 三态字段字典 + F8/F4 facet 列表
（facet 由调用方交给 verdict 承载，本模块不判 verdict）。

- **U1-U7 三态字段表**（:278-287）：全部字段三态
  ``observed-true`` / ``observed-false`` / ``unobservable``；
- 每分量 fail-closed：前提缺 → ``unobservable(cause=<预注册 cause>)``；
  域外值 → ``unobservable(cause=value-outside-frozen-domain)``（判别方法 (1)，
  :1375-1376 不 fail）；解析缺口/真值表缺口 → F8 facet 由 verdict 承载（:1150）；
- 纯函数：无 I/O、无时钟、无 hdc；模块导入零副作用。
"""

from __future__ import annotations

import re
from typing import Any, Dict, List, Mapping, Optional, Sequence, Tuple

import n1bdisc_core as core
import n1bdisc_death as death
import n1bdisc_fsm as fsm

_UNOBS = core.unobservable_value

# ---------------------------------------------------------------------------
# 冻结常量（规格字面）
# ---------------------------------------------------------------------------

#: ``u1_match_offset`` 取值域（规格 :539：{0/4/both/none}，首个匹配帧可用 offset）。
U1_MATCH_OFFSETS: Tuple[str, ...] = ("0", "4", "both", "none")
#: D5 冻结 src 地址（MR* 保留 / MB1 保留，规格 :576/:578）。
D5_FROZEN_SRC_MR = "10.99.0.2"
D5_FROZEN_SRC_MB1 = "192.0.2.2"
#: D4/D5 冻结前提完整成功值（sendto n==16 / write n==44，规格 :540/:581）。
D4_PREMISE_LEN = 16
D5_PREMISE_LEN = 44
#: D7 elapsed 接受域互斥区间（规格 :645-651 E5）。
D7_DURATION_MS = 20000
D7_GRACE_UPPER_MS = 25000
#: 矩阵条目 id 域（规格 :421；u5 逐候选键）。
MATRIX_ENTRY_IDS: Tuple[str, ...] = ("MR1", "MR1B", "MR2", "MR3", "MB1")

_UINT_RE = re.compile(r"\A[0-9]+\Z")

#: u4 七子项 ↔ D6 pre/result marker（规格 :959-979 表 + :981 逐子项清单）。
U4_SUBITEM_MARKERS: Tuple[Tuple[str, str, str], ...] = (
    ("u4_orig_getfd", "N1BDISC_D6S1_B", "N1BDISC_D6S1_R"),
    ("u4_orig_getfl", "N1BDISC_D6S2_B", "N1BDISC_D6S2_R"),
    ("u4_orig_close", "N1BDISC_D6S3_B", "N1BDISC_D6S3_R"),
    ("u4_dup_getfd", "N1BDISC_D6S4_B", "N1BDISC_D6S4_R"),
    ("u4_dup_read", "N1BDISC_D6S5_B", "N1BDISC_D6S5_R"),
    ("u4_dup_close", "N1BDISC_D6S6_B", "N1BDISC_D6S6_R"),
    ("u4_dup_fd_reuse", "N1BDISC_D6S7_B", "N1BDISC_D6S7_R"),
)
#: D6b 整段 skip 管辖的四个 dup 面子项（规格 :969 r18 重裁）。
U4_DUP_SUBITEMS = frozenset({
    "u4_dup_getfd", "u4_dup_read", "u4_dup_close", "u4_dup_fd_reuse"})


def _parse_uint(text: Any) -> Optional[int]:
    if isinstance(text, str) and _UINT_RE.match(text):
        return int(text)
    return None


_INT_RE = re.compile(r"\A-?[0-9]+\Z")


def _parse_int(text: Any) -> Optional[int]:
    """带符号十进制整数字面解析（errno/ret 载体含 ``-1``；非整数字面 → None）。"""
    if isinstance(text, str) and _INT_RE.match(text):
        return int(text)
    return None


def _named(events: Sequence[Any], name: str) -> List[Any]:
    return [e for e in events if e.name == name]


def _first(events: Sequence[Any], name: str) -> Optional[Any]:
    return next((e for e in events if e.name == name), None)


def _skip_item_cause(events: Sequence[Any], item: str) -> Optional[str]:
    """首个 ``N1BDISC_SKIP|item=<item>`` 的 cause 字面；缺 → ``None``。"""
    for e in events:
        if e.name == "N1BDISC_SKIP" and e.kv.get("item") == item:
            return e.kv.get("cause", "")
    return None


def dw_skip_cause_of(events: Sequence[Any]) -> Optional[str]:
    """D-W 整体 skip cause（``SKIP|item=D-W``；规格 :997-1000 skip 表指派载体）。"""
    return _skip_item_cause(events, "D-W")


def retained_entry_id(events: Sequence[Any]) -> Optional[str]:
    """first-accept 保留条目 id（规格 :449：首个 resolved/late-resolved 条目；
    结局终值按 m-07 后到者为准）。无保留条目 → ``None``。"""
    first_accept_line: Optional[Tuple[int, str]] = None
    for e in events:
        if e.name != "N1BDISC_D2_ENTRY":
            continue
        outcome = e.kv.get("outcome")
        if outcome in ("resolved", "late-resolved"):
            key = (e.line_no, e.kv.get("id", ""))
            if first_accept_line is None or key < first_accept_line:
                first_accept_line = key
    return first_accept_line[1] if first_accept_line else None


# ---------------------------------------------------------------------------
# u1（D4 socket→tun 投递；规格 :534-547）+ u3 共用的首匹配帧提取
# ---------------------------------------------------------------------------

def first_matching_d4_read(events: Sequence[Any]
                           ) -> Tuple[Optional[Any], List[Mapping[str, str]], bool]:
    """首个匹配受控包 read（runner 载体 = ``D4_READ|off=`` 域内非 none 字面）。

    返回 ``(read_marker, out_of_domain_offs, none_seen)``：``off`` 落在
    :data:`U1_MATCH_OFFSETS` 之外的字面逐条登记（域外值不参与匹配、原值保留，
    规格 :539/:1375-1376）；``none_seen`` = 域内 ``off=none`` read 是否在
    （零匹配的显式载体，:539 取值域成员）。
    """
    out_of_domain: List[Mapping[str, str]] = []
    none_seen = False
    for e in _named(events, "N1BDISC_D4_READ"):
        off = e.kv.get("off")
        if off in U1_MATCH_OFFSETS:
            if off != "none":
                return e, out_of_domain, none_seen
            none_seen = True
            continue
        out_of_domain.append({"off": off, "raw": e.raw})
    return None, out_of_domain, none_seen


def derive_u1(*, events: Sequence[Any],
              retained_entry: Optional[str],
              dw_skip: Optional[str] = None) -> Dict[str, Any]:
    """u1_socket_to_tun_delivery + u1_match_offset（规格 :539-547）。

    前提 = 至少一次 ``sendto`` 返回 ``n==16``（marker 载体
    ``D4_SENT|n=<k>|ret=<n>|errno=<e>`` 的 ``ret``）；全部 ``-1`` →
    ``send-failed``；存在 ``0``/部分但从无完整成功 → ``short-or-zero-io``；
    MB1 保留 → 主字段固定 ``mb1-no-route``、对照字段 ``u1_no_route_control``
    按 E9 求值表（:543-547）承载同一流。窗口耗尽零匹配 → ``observed-false``；
    任一 offset 匹配即 ``observed-true``。
    """
    out: Dict[str, Any] = {
        "u1_socket_to_tun_delivery": None,
        "u1_match_offset": None,
        "u1_no_route_control": None,
        "raw": {},
    }
    skip = _skip_item_cause(events, "D4") or dw_skip
    if skip in ("no-live-fd", "dup-failed"):
        cause = _UNOBS(skip)
        out.update(u1_socket_to_tun_delivery=cause, u1_match_offset=cause,
                   u1_no_route_control=cause, raw={"branch": "skip"})
        return out

    sent = _named(events, "N1BDISC_D4_SENT")
    rets: List[Optional[int]] = []
    unparsable: List[str] = []
    for e in sent:
        ret = _parse_int(e.kv.get("ret"))
        if ret is None:
            unparsable.append(e.raw)
        rets.append(ret)
    has_full = any(r == D4_PREMISE_LEN for r in rets)
    match, out_of_domain, none_seen = first_matching_d4_read(events)
    end_present = _first(events, "N1BDISC_D4_END") is not None
    out["raw"] = {
        "send_rets": [e.kv.get("ret") for e in sent],
        "out_of_domain_offs": out_of_domain,
        "d4_end_present": end_present,
    }

    if retained_entry == "MB1":
        # 规格 :542：MB1 保留 → 主字段固定 mb1-no-route（零匹配不构成「不投递」）；
        # 对照字段 u1_no_route_control 按 E9 求值表承载同一流（:543-547）。
        out["u1_socket_to_tun_delivery"] = _UNOBS("mb1-no-route")
        out["u1_no_route_control"] = _premise_gated_value(
            rets, has_full, match, end_present, unparsable)
        out["u1_match_offset"] = _match_offset_value(match, out_of_domain,
                                                     none_seen)
        return out

    # MR* 保留：对照字段无执行域（判据未规定 MR* 保留时的落值字面——偏差登记，
    # 取 marker-gap-indeterminate 纯记录、不驱动 verdict）。
    out["u1_no_route_control"] = _UNOBS("mb1-not-retained")
    if unparsable and not has_full:
        out["u1_match_offset"] = _match_offset_value(match, out_of_domain,
                                                     none_seen)
        out["u1_socket_to_tun_delivery"] = _UNOBS("value-outside-frozen-domain")
        out["raw"]["unparsable_send_rets"] = unparsable
        return out
    out["u1_match_offset"] = _match_offset_value(match, out_of_domain, none_seen)
    out["u1_socket_to_tun_delivery"] = _premise_gated_value(
        rets, has_full, match, end_present, unparsable)
    return out


def _match_offset_value(match: Optional[Any],
                        out_of_domain: Sequence[Mapping[str, str]],
                        none_seen: bool = False) -> str:
    if match is not None:
        return str(match.kv.get("off"))
    if out_of_domain:
        return _UNOBS("value-outside-frozen-domain")
    if none_seen:
        return "none"                       # :539 取值域成员（零匹配显式载体）
    return _UNOBS("marker-gap-indeterminate")


def _premise_gated_value(rets: Sequence[Optional[int]], has_full: bool,
                         match: Optional[Any], end_present: bool,
                         unparsable: Sequence[str]) -> str:
    """前提三分 + 窗口二分（u1 主字段 / u1_no_route_control 同一门，:540-541/:543-547）。"""
    if not rets:
        return _UNOBS("marker-gap-indeterminate")   # 前提标记全缺（偏差登记 cause）
    if has_full:
        if match is not None:
            return "observed-true"
        if end_present:
            return "observed-false"
        return _UNOBS("marker-gap-indeterminate")   # 窗口未收口、不得赋 false
    if all(r == -1 for r in rets if r is not None) and \
            all(r is not None for r in rets):
        return _UNOBS("send-failed")                # :546 全部 -1
    return _UNOBS("short-or-zero-io")               # :545 有 0/部分、从无完整成功


# ---------------------------------------------------------------------------
# u2（D5 tun 写入 → sink；规格 :573-586）
# ---------------------------------------------------------------------------

def derive_u2(*, events: Sequence[Any],
              retained_entry: Optional[str],
              dw_skip: Optional[str] = None) -> Dict[str, Any]:
    """u2_tun_write_to_sink_delivery（规格 :581-584）。

    前提 = 至少一轮 ``write`` 返回 ``n==44``（``D5_WRITE|round|r=|ret=|errno=``）；
    全部 ``-1`` → ``write-failed``、短写从无完整成功 → ``short-or-zero-io``；
    前提成立时任一轮 ``D5_RECV|round|src=`` 的 src = 冻结 src（身份匹配的 runner
    可见载体——16 B 载荷逐字节身份由探针侧 E10 施行，marker 域仅承载
    ``src=<addr>``/``round``，偏差登记）→ ``observed-true``；轮数与窗口耗尽零
    收到 → ``observed-false``。
    """
    out: Dict[str, Any] = {"u2_tun_write_to_sink_delivery": None, "raw": {}}
    skip = _skip_item_cause(events, "D5") or dw_skip
    if skip in ("no-live-fd", "dup-failed"):
        out["u2_tun_write_to_sink_delivery"] = _UNOBS(skip)
        out["raw"] = {"branch": "skip"}
        return out
    frozen_src = D5_FROZEN_SRC_MB1 if retained_entry == "MB1" else D5_FROZEN_SRC_MR
    writes = _named(events, "N1BDISC_D5_WRITE")
    rets = [_parse_int(e.kv.get("ret")) for e in writes]
    has_full = any(r == D5_PREMISE_LEN for r in rets)
    recvs = _named(events, "N1BDISC_D5_RECV")
    matched = [e for e in recvs if e.kv.get("src") == frozen_src]
    end_present = _first(events, "N1BDISC_D5_END") is not None
    out["raw"] = {
        "frozen_src": frozen_src,
        "write_rets": [e.kv.get("ret") for e in writes],
        "recv_srcs": [e.kv.get("src") for e in recvs],
        "d5_end_present": end_present,
    }
    if not writes:
        out["u2_tun_write_to_sink_delivery"] = _UNOBS("marker-gap-indeterminate")
        return out
    if has_full:
        if matched:
            out["u2_tun_write_to_sink_delivery"] = "observed-true"
        elif end_present:
            out["u2_tun_write_to_sink_delivery"] = "observed-false"
        else:
            out["u2_tun_write_to_sink_delivery"] = _UNOBS("marker-gap-indeterminate")
        return out
    if all(r == -1 for r in rets if r is not None) and \
            all(r is not None for r in rets):
        out["u2_tun_write_to_sink_delivery"] = _UNOBS("write-failed")
    else:
        out["u2_tun_write_to_sink_delivery"] = _UNOBS("short-or-zero-io")
    return out


# ---------------------------------------------------------------------------
# u3（D4 首帧 dump；规格 :548-568）
# ---------------------------------------------------------------------------

def _ipv4_parsable(frame: bytes, offset: int) -> bool:
    """offset 处可解析 IPv4（version=4、IHL=5 且 total_length 字段可读）。"""
    return (len(frame) >= offset + 4 and frame[offset] >> 4 == 4
            and frame[offset] & 0x0F == 5)


def _pi_header_enum(frame: bytes) -> Tuple[str, str]:
    """S5 互斥分区表（规格 :551-561）：先分区、再定类，``ambiguous`` 无条件。

    返回 ``(u3_pi_header_present, other_prefix_raw4)``（无前缀原文时 raw4 为空串）。
    """
    p0 = _ipv4_parsable(frame, 0)
    p4 = _ipv4_parsable(frame, 4)
    if not p0 and not p4:
        return "unparsable", ""
    if p0 and not p4:
        return "no-prefix", ""
    if p4 and not p0:
        if len(frame) >= 4 and frame[2] == 0x08 and frame[3] == 0x00:
            return "tun_pi-like", ""
        return "other-prefix", frame[:4].hex()
    return "ambiguous", ""      # :558 两可解析 → 无条件 ambiguous


def derive_u3(*, events: Sequence[Any],
              chunks: Any,
              retained_entry: Optional[str],
              dw_skip: Optional[str] = None) -> Dict[str, Any]:
    """u3_* 五字段 + off0 对照字段（规格 :548-568）。

    决定性数据 = 首个匹配受控包 read（:548）；帧字节载体 = ``u3hex`` chunk
    （item=0，前 64 字节十六进制）。零匹配（含窗口耗尽、MB1 保留）→ 全部
    ``unobservable(cause=no-controlled-read)``。``off=both`` 时决定性 offset = 4
    并另记 ``u3_readlen_vs_total_length_off0``（:563-564）。
    """
    out: Dict[str, Any] = {
        "u3_first_read_len": None,
        "u3_first64_hex": None,
        "u3_pi_header_present": None,
        "u3_prefix_format": None,
        "u3_readlen_vs_total_length": None,
        "u3_readlen_vs_total_length_off0": None,
        "raw": {},
    }
    skip = _skip_item_cause(events, "D4") or dw_skip
    if skip in ("no-live-fd", "dup-failed"):
        cause = _UNOBS(skip)
        for key in ("u3_first_read_len", "u3_first64_hex", "u3_pi_header_present",
                    "u3_prefix_format", "u3_readlen_vs_total_length",
                    "u3_readlen_vs_total_length_off0"):
            out[key] = cause
        out["raw"] = {"branch": "skip"}
        return out

    match, out_of_domain, none_seen = first_matching_d4_read(events)
    if retained_entry == "MB1" or match is None:
        # 零匹配（含窗口耗尽、MB1 保留）→ 全部 no-controlled-read（:548/:567）
        cause = _UNOBS("no-controlled-read")
        for key in ("u3_first_read_len", "u3_first64_hex", "u3_pi_header_present",
                    "u3_prefix_format", "u3_readlen_vs_total_length",
                    "u3_readlen_vs_total_length_off0"):
            out[key] = cause
        out["raw"] = {"out_of_domain_offs": out_of_domain}
        return out

    f4: List[str] = []
    f8: List[str] = []
    read_len = _parse_uint(match.kv.get("len"))
    if read_len is None:
        f8.append("u3: D4_READ len unparsable: %s" % (match.raw,))
        out["u3_first_read_len"] = _UNOBS("value-outside-frozen-domain")
    else:
        out["u3_first_read_len"] = read_len

    hex_text = chunks.texts.get(("u3hex", 0)) if chunks is not None else None
    frame = b""
    if hex_text is None:
        # D 项完成 marker 在而 (stream,item) chunk 组缺失 → 增量落盘缺项（:1128 F4 面）
        f4.append("u3hex chunk group missing while matching D4_READ present")
        out["u3_first64_hex"] = _UNOBS("marker-gap-indeterminate")
        pi_enum, other_raw = "unparsable", ""
    else:
        try:
            frame = bytes.fromhex(hex_text)
        except ValueError:
            f8.append("u3: u3hex chunk not valid hex: %r" % (hex_text[:32],))
            out["u3_first64_hex"] = hex_text
            pi_enum, other_raw = "unparsable", ""
        else:
            out["u3_first64_hex"] = hex_text
            pi_enum, other_raw = _pi_header_enum(frame)
    out["u3_pi_header_present"] = pi_enum
    out["raw"]["other_prefix_raw4"] = other_raw   # :557 other-prefix 逐字登记原文

    # u3_prefix_format 三态主字段派生表（:565-568）
    prefix_map = {
        "tun_pi-like": "observed-true",
        "no-prefix": "observed-false",
        "other-prefix": "observed-false",
        "ambiguous": _UNOBS("prefix-ambiguous"),
        "unparsable": _UNOBS("frame-unparsable"),
    }
    out["u3_prefix_format"] = prefix_map[pi_enum]

    # readlen vs total_length（决定性 offset：off=both → 4，:563-564）
    off = str(match.kv.get("off"))
    decisive = 4 if off == "both" else int(off)

    def _readlen_cmp(at: int) -> str:
        if read_len is None or not _ipv4_parsable(frame, at):
            return "unparsable" if _parse_uint(match.kv.get("len")) is not None \
                else "unobservable"
        total = (frame[at + 2] << 8) | frame[at + 3]
        if read_len == total:
            return "equal"
        return "readlen>total_length" if read_len > total else "readlen<total_length"

    if hex_text is None or (read_len is not None and not frame):
        out["u3_readlen_vs_total_length"] = "unobservable"
    else:
        out["u3_readlen_vs_total_length"] = _readlen_cmp(decisive)
    if off == "both":
        out["u3_readlen_vs_total_length_off0"] = _readlen_cmp(0)
    out["raw"]["decisive_offset"] = decisive
    out["f4_facets"] = f4
    out["f8_facets"] = f8
    return out


# ---------------------------------------------------------------------------
# u4（D6 destroy 后同步尝试；规格 :981-984 + pre-only 四支 :1185-1195）
# ---------------------------------------------------------------------------

def derive_u4(*, events: Sequence[Any],
              destroy_call_state: str,
              death_observed: bool,
              post_present: bool,
              dw_skip: Optional[str] = None) -> Dict[str, Any]:
    """u4 七子项 + 摘要（规格 :981-984；pre-only 覆盖 :1185-1195）。

    - D-W 整体 skip → 全部 ``unobservable(cause=<skip cause>)``（:999-1000）；
    - ``SKIP|item=D6b``（join-timeout-abandoned）→ dup 面四子项
      ``unobservable(cause=d6b-skipped-join-timeout)``（:969）；
    - result marker 在（ret 已登记）→ ``observed-true``（记录值保持，:1186）；
    - POST 在（或无死亡证据不进 pre-only，:1194）→ 按 D6 节原三态：
      pre 在 + 死亡 + 零 result → ``observed-false``，其余 ``unobservable``；
    - pre-only（POST 缺 ∧ 死亡分量 true）→ 未执行子项按 ``destroy_call_state``
      四支分岔（:1187-1193）；
    - 摘要 = 任一 true → true，否则全序 ``unobservable < observed-false <
      observed-true`` 下的最小值（:983-984）。
    """
    out: Dict[str, Any] = {"subitems": {}, "raw": {}}
    d6b_skip_cause = _skip_item_cause(events, "D6b")
    pre_only = (not post_present) and death_observed
    branch_cause: Optional[str] = None
    if pre_only:
        branch_map = {
            "not-called": "destroy-not-called",                  # :1189 (1b)
            "not-reached": "destroy-not-reached",                # :1188 (1a)
            "call-boundary-incomplete": "call-boundary-incomplete",  # :1191
            "marker-contradiction": "marker-contradiction",      # F3 轴承载（偏差登记）
        }
        if destroy_call_state in branch_map:
            branch_cause = branch_map[destroy_call_state]
        elif destroy_call_state == "call-returned":
            # (3)/(4)：resolve/D6a 证据 → observed-false；无 → destroy-unresolved
            resolve_evidence = any(
                _first(events, rname) is not None
                for _, _, rname in U4_SUBITEM_MARKERS[:3])
            branch_cause = None if resolve_evidence else "destroy-unresolved"
            out["raw"]["destroy_resolved"] = resolve_evidence
        else:
            branch_cause = "marker-gap-indeterminate"

    for subitem, pre_name, result_name in U4_SUBITEM_MARKERS:
        result_marker = _first(events, result_name)
        pre_marker = _first(events, pre_name)
        if dw_skip in ("no-live-fd", "dup-failed"):
            value = _UNOBS(dw_skip)                              # :999-1000
        elif d6b_skip_cause is not None and subitem in U4_DUP_SUBITEMS:
            value = _UNOBS("d6b-skipped-join-timeout")           # :969
        elif result_marker is not None:
            value = "observed-true"                              # :982/:1186
        elif pre_only:
            value = ("observed-false" if branch_cause is None
                     else _UNOBS(branch_cause))                  # :1192/(3) 支
        elif pre_marker is not None and death_observed:
            value = "observed-false"                             # :982
        else:
            value = _UNOBS("marker-gap-indeterminate")           # :982
        out["subitems"][subitem] = value

    # 摘要（:983-984 全序 unobservable < observed-false < observed-true）
    values = list(out["subitems"].values())

    def _rank(v: str) -> int:
        if v == "observed-true":
            return 2
        if v == "observed-false":
            return 1
        return 0   # 一切 unobservable 字面同为最弱值

    if any(v == "observed-true" for v in values):
        out["u4_post_destroy_sync_observable"] = "observed-true"
    elif all(_rank(v) == 1 for v in values):
        out["u4_post_destroy_sync_observable"] = "observed-false"
    else:
        out["u4_post_destroy_sync_observable"] = values[0] if values else \
            _UNOBS("marker-gap-indeterminate")
    out["raw"]["d6b_skip_cause"] = d6b_skip_cause
    out["raw"]["pre_only_branch"] = branch_cause if pre_only else None
    return out


# ---------------------------------------------------------------------------
# u5（D2 矩阵逐候选；规格 :478-482）
# ---------------------------------------------------------------------------

def derive_u5(*, events: Sequence[Any]) -> Dict[str, Any]:
    """u5_routeinfo_acceptance 逐候选（规格 :479 取值域闭合）。

    终值按 m-07「后到者为准」；``not_attempted``/无 marker 条目按矩阵终局分派：
    accept-lock 后 → ``protocol-first-accept-lock``、timeout/indeterminate 终止后
    → ``matrix-terminated-on-create-timeout``、无任何矩阵终局证据 →
    ``marker-gap-indeterminate``（偏差登记：判据未规定「条目 marker 全缺」格）。
    """
    final: Dict[str, str] = {}
    for e in events:
        if e.name != "N1BDISC_D2_ENTRY":
            continue
        outcome = e.kv.get("outcome")
        if outcome is None:
            continue
        final[e.kv.get("id", "")] = outcome
    retained = retained_entry_id(events)
    terminated_on_timeout = any(
        v in ("timeout", "indeterminate") for v in final.values())
    outcome_map = {
        "resolved": "observed-true", "late-resolved": "observed-true",
        "rejected": "observed-false", "late-rejected": "observed-false",
        "indeterminate": _UNOBS("create-indeterminate"),
        "timeout": _UNOBS("create-indeterminate"),
    }
    out: Dict[str, Any] = {"candidates": {}, "raw": {"retained": retained}}
    for entry_id in MATRIX_ENTRY_IDS:
        outcome = final.get(entry_id)
        if outcome is not None:
            out["candidates"][entry_id] = outcome_map.get(
                outcome, _UNOBS("value-outside-frozen-domain"))
        elif retained is not None:
            out["candidates"][entry_id] = _UNOBS("protocol-first-accept-lock")
        elif terminated_on_timeout:
            out["candidates"][entry_id] = _UNOBS(
                "matrix-terminated-on-create-timeout")
        else:
            out["candidates"][entry_id] = _UNOBS("marker-gap-indeterminate")
    return out


# ---------------------------------------------------------------------------
# u6（D2 步 2.4；规格 :492 + 派生表 :497-503）
# ---------------------------------------------------------------------------

def derive_u6(*, events: Sequence[Any],
              fd_roles_created: frozenset = frozenset(),
              dw_skip: Optional[str] = None) -> Dict[str, Any]:
    """u6_initial_flags_and_isblocking_effect + u6_nonblocking_initial（:492/:497-503）。

    ``D2_S4|u6=`` 字面：``o_nonblock_present`` → ``observed-true``、
    ``o_nonblock_absent`` → ``observed-false``；S4 缺 → 沿「无 fd / dup 失败」
    表（:501）``no-live-fd``/``dup-failed``；域外字面 →
    ``value-outside-frozen-domain``；marker 内裸 ``unobservable`` →
    ``marker-gap-indeterminate``（偏差登记：marker 未携带 cause 字面）。
    """
    out: Dict[str, Any] = {
        "u6_initial_flags_and_isblocking_effect": None,
        "u6_nonblocking_initial": None,
        "raw": {},
    }
    skip: Optional[str] = None
    if "fd_orig" not in fd_roles_created:
        skip = "no-live-fd"
    elif "fd_dup" not in fd_roles_created:
        skip = "dup-failed"
    s4 = _first(events, "N1BDISC_D2_S4")
    literal = s4.kv.get("u6") if s4 is not None else None
    out["raw"] = {"s4_literal": literal, "skip": skip or dw_skip}
    if literal in ("o_nonblock_present", "o_nonblock_absent"):
        value = "observed-true" if literal == "o_nonblock_present" \
            else "observed-false"
        out["u6_initial_flags_and_isblocking_effect"] = value
        out["u6_nonblocking_initial"] = value
        return out
    if literal == "unobservable":
        cause = _UNOBS("marker-gap-indeterminate")
    elif literal is not None:
        cause = _UNOBS("value-outside-frozen-domain")   # 域外字面（:1375）
    elif skip is not None:
        cause = _UNOBS(skip)                            # :501 沿分支表
    else:
        cause = _UNOBS("marker-gap-indeterminate")      # S4 缺且矩阵已执行
    out["u6_initial_flags_and_isblocking_effect"] = cause
    out["u6_nonblocking_initial"] = cause
    return out


# ---------------------------------------------------------------------------
# u7（D7 live watchdog；规格 :645-669）
# ---------------------------------------------------------------------------

def _site_index(site: Optional[str]) -> Optional[int]:
    if site in fsm.SITE_ORDER:
        return fsm.SITE_ORDER.index(site)
    return None


def derive_u7(*, events: Sequence[Any],
              death_observed: bool,
              last_site: Optional[str],
              tail_state: Optional[str],
              dw_skip: Optional[str] = None) -> Dict[str, Any]:
    """u7_long_task_watchdog_behavior（:645-669；含 r13 阶段未达支 :658-661、
    skip 优先 :662、V4 tail-loss 约束 :663-666）。

    求值序（先到先得）：
    (0) skip（``SKIP|item=D7`` 或 no-live-fd 全局分支）→ ``no-live-vpn``
        （:662/:996——无 live VPN 不得 true/false）；
    (1) 阶段未达（:658：死亡 ∧ ``D7_BEGIN`` 缺 ∧ ``last_visible_site`` ≤ P5T）→
        ``stage-not-reached``（start_mono/elapsed/iters 同 cause）；
    (2) ``D7_END`` 在：elapsed 接受域三区间（:645-651）——``[0,20000)`` 矛盾
        fail(F8) + ``d7-early-exit-anomaly``；``[20000,25000]`` →
        ``observed-true``；``>25000`` → ``d7-elapsed-overshoot-beyond-grace``
        不 fail；负值 → 单调钟域门先命中（:654）同挂 F8；
    (3) ``D7_END`` 缺 ∧ ``D7_BEGIN`` 在：死亡 ∧ BEGIN 后静默 →
        ``observed-false``，但位点 P6 ∧ ``possible-tail-loss`` →
        ``marker-tail-loss`` 不得 false（:663-666）；无死亡 →
        ``marker-gap-indeterminate``（:657 禁止以 marker 缺失单独推断被杀）。
    """
    out: Dict[str, Any] = {
        "u7_long_task_watchdog_behavior": None,
        "start_mono_ms": None,
        "elapsed_ms": None,
        "iters": None,
        "d7_anomaly": None,
        "raw": {},
    }
    f4: List[str] = []
    f8: List[str] = []
    begin = _first(events, "N1BDISC_D7_BEGIN")
    end = _first(events, "N1BDISC_D7_END")
    d7_skip = _skip_item_cause(events, "D7")

    # (0) skip 分支优先（:662；skip 表 D7 行 :996 仅 no-live-fd 分支 skip D7）
    if d7_skip is not None or dw_skip == "no-live-fd":
        cause = _UNOBS("no-live-vpn")
        out.update(u7_long_task_watchdog_behavior=cause,
                   start_mono_ms=cause, elapsed_ms=cause, iters=cause,
                   raw={"branch": "skip", "skip_cause": d7_skip or dw_skip})
        out["f4_facets"] = f4
        out["f8_facets"] = f8
        return out

    # (1) r13 阶段未达支（:658-661）
    last_idx = _site_index(last_site)
    if death_observed and begin is None and last_idx is not None \
            and last_idx <= _site_index("P5T"):
        cause = _UNOBS(death.STAGE_NOT_REACHED_CAUSE)
        out.update(u7_long_task_watchdog_behavior=cause,
                   start_mono_ms=cause, elapsed_ms=cause, iters=cause,
                   raw={"branch": "stage-not-reached", "last_site": last_site})
        out["f4_facets"] = f4
        out["f8_facets"] = f8
        return out

    if begin is not None:
        start = _parse_uint(begin.kv.get("start_mono_ms"))
        if start is None:
            f4.append("D7_BEGIN.start_mono_ms missing/unparsable")
        else:
            out["start_mono_ms"] = start

    if end is not None:
        # (2) 接受域三区间（:645-651）；elapsed 带符号解析（负值 → 单调钟域门 :654）
        elapsed = _parse_int(end.kv.get("elapsed_ms"))
        iters = _parse_uint(end.kv.get("iters"))
        if elapsed is None:
            f4.append("D7_END.elapsed_ms missing/unparsable")
        if iters is None:
            f4.append("D7_END.iters missing/unparsable")
        out["elapsed_ms"] = elapsed if elapsed is not None \
            else end.kv.get("elapsed_ms")
        out["iters"] = iters if iters is not None else end.kv.get("iters")
        if elapsed is None:
            out["u7_long_task_watchdog_behavior"] = \
                _UNOBS("marker-gap-indeterminate")
        elif elapsed < 0:
            # :654 单调钟域门（非负性）先命中（F8），标签沿 early-exit 同族
            f8.append("monotonic-domain: D7 elapsed_ms=%d < 0" % elapsed)
            out["u7_long_task_watchdog_behavior"] = \
                _UNOBS("d7-early-exit-anomaly")
            out["d7_anomaly"] = "early-exit-with-end-marker"
        elif elapsed < D7_DURATION_MS:
            # :647/:653 矛盾输入 → fail(F8)（标签同时驱动 F8）
            f8.append("d7-early-exit-contradiction: D7_END present with "
                      "elapsed_ms=%d < 20000 (d7_anomaly="
                      "early-exit-with-end-marker); raw elapsed=%d" % (elapsed, elapsed))
            out["u7_long_task_watchdog_behavior"] = \
                _UNOBS("d7-early-exit-anomaly")
            out["d7_anomaly"] = "early-exit-with-end-marker"
        elif elapsed <= D7_GRACE_UPPER_MS:
            out["u7_long_task_watchdog_behavior"] = "observed-true"   # :648/:652
        else:
            # :649/:654 具名三态观察、不判 fail
            out["u7_long_task_watchdog_behavior"] = \
                _UNOBS("d7-elapsed-overshoot-beyond-grace")
            out["d7_anomaly"] = "elapsed-overshoot-observed"
    elif begin is not None:
        # (3) END 缺、BEGIN 在（:655-657 + V4 约束 :663-666）
        after_begin = [e for e in events
                       if e.line_no > begin.line_no and e.name.startswith("N1BDISC_")]
        silent = not after_begin
        out["raw"]["silent_after_begin"] = silent
        if (death_observed and silent and last_site == "P6"
                and tail_state == "possible-tail-loss"):
            out["u7_long_task_watchdog_behavior"] = _UNOBS("marker-tail-loss")
        elif death_observed and silent:
            out["u7_long_task_watchdog_behavior"] = "observed-false"  # :655
        else:
            out["u7_long_task_watchdog_behavior"] = \
                _UNOBS("marker-gap-indeterminate")                    # :657
    else:
        # BEGIN 缺、非阶段未达（位点 > P5T 或无死亡）：:657 禁止推断
        out["u7_long_task_watchdog_behavior"] = _UNOBS("marker-gap-indeterminate")
    out["f4_facets"] = f4
    out["f8_facets"] = f8
    return out


# ---------------------------------------------------------------------------
# 顶层组装
# ---------------------------------------------------------------------------

def fd_roles_created(events: Sequence[Any]) -> frozenset:
    """已 create 的 fd 角色（含 ``role#k`` 形态剥离）；供 u6 分支判定。"""
    roles = set()
    for e in _named(events, "N1BDISC_FD"):
        if e.kv.get("action") == "create":
            roles.add(str(e.kv.get("role", "")).partition("#")[0])
    return frozenset(roles)


def derive_platform_components(*, events: Sequence[Any],
                               chunks: Any,
                               death_observed: bool,
                               last_site: Optional[str],
                               tail_state: Optional[str],
                               post_present: bool,
                               destroy_call_state: Optional[str] = None,
                               ) -> Dict[str, Any]:
    """u1-u7 全量派生（分期项 9 接线入口；输入 = capture 全流 + 死亡事实）。

    返回字典：``u1``..``u7`` 各分量子字典 + 顶层 ``f8_facets`` / ``f4_facets``
    （交 verdict 承载：F8 解析/真值表缺口 :1150、F4 增量落盘缺项 :1128）。
    """
    dw_skip = dw_skip_cause_of(events)
    retained = retained_entry_id(events)
    if destroy_call_state is None:
        destroy_call_state = death.eval_destroy_call_state(
            _skip_item_cause(events, "destroy") is not None,
            _first(events, "N1BDISC_DW_DESTROY_T") is not None,
            _first(events, "N1BDISC_DW_DESTROY_C") is not None)

    u1 = derive_u1(events=events, retained_entry=retained, dw_skip=dw_skip)
    u2 = derive_u2(events=events, retained_entry=retained, dw_skip=dw_skip)
    u3 = derive_u3(events=events, chunks=chunks, retained_entry=retained,
                   dw_skip=dw_skip)
    u4 = derive_u4(events=events, destroy_call_state=destroy_call_state,
                   death_observed=death_observed, post_present=post_present,
                   dw_skip=dw_skip)
    u5 = derive_u5(events=events)
    u6 = derive_u6(events=events, fd_roles_created=fd_roles_created(events),
                   dw_skip=dw_skip)
    u7 = derive_u7(events=events, death_observed=death_observed,
                   last_site=last_site, tail_state=tail_state, dw_skip=dw_skip)

    f8: List[str] = []
    f4: List[str] = []
    for component in (u1, u2, u3, u4, u5, u6, u7):
        f8.extend(component.get("f8_facets", ()))
        f4.extend(component.get("f4_facets", ()))
    return {
        "u1": u1, "u2": u2, "u3": u3, "u4": u4, "u5": u5, "u6": u6, "u7": u7,
        "retained_entry": retained,
        "dw_skip_cause": dw_skip,
        "destroy_call_state": destroy_call_state,
        "f8_facets": f8,
        "f4_facets": f4,
    }


__all__ = [
    "U1_MATCH_OFFSETS", "D5_FROZEN_SRC_MR", "D5_FROZEN_SRC_MB1",
    "D4_PREMISE_LEN", "D5_PREMISE_LEN", "D7_DURATION_MS", "D7_GRACE_UPPER_MS",
    "MATRIX_ENTRY_IDS", "U4_SUBITEM_MARKERS", "U4_DUP_SUBITEMS",
    "dw_skip_cause_of", "retained_entry_id", "first_matching_d4_read",
    "derive_u1", "derive_u2", "derive_u3", "derive_u4", "derive_u5",
    "derive_u6", "derive_u7", "fd_roles_created", "derive_platform_components",
]
