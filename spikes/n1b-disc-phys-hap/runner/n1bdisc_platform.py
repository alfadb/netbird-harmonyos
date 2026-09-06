# -*- coding: utf-8 -*-
"""n1bdisc_platform — N1BDISC host runner u1-u7 平台分量派生（纯函数，host-only）。

权威规格：``docs/n1b-disc-gate-plan.md``（只读冻结；本实现引用现行行号）。
分期项 9（gate 10 前置义务）：输入 = marker 事件序列 / chunk 重组结果 /
ledger 角色 / dw 派生输入，输出 = u1-u7 三态字段字典 + F8/F4 facet 列表
（facet 由调用方交给 verdict 承载，本模块不判 verdict）。

**gate 3 整改（B1/B2/B3/B6/B7/M1）——派生以真实 producer 的 marker 发射语义
为基准**（第一原则；上一轮实现者跳过了这一步，是全部 blocker 的根因）：

- **U1/U3（B1）**：``D4_READ|off=`` 只承载双偏移**可解析性**
  （``offsets_parsable``，d4.rs:94-95），不携带身份匹配结果——外来可解析
  IPv4 包同样产 ``off=0/4/both``，绝不构成 observed-true。匹配事实的唯一
  marker 载体 = ``u3hex`` chunk（d4.rs:186-187，恰在首个匹配帧上发射）与
  ``D4_END|u1=`` 终态；runner 对 chunk 帧按冻结身份（proto=17 + dst + dport
  47001 + 16 B payload 逐字节，d4.rs:97-125 / 规格 :537-539）在 offset-0/4
  独立复核。
- **U2（B2）**：``D5_RECV|src=`` 为「地址:端口」形态（d5.rs:83-91），payload
  逐字节身份不在任何 marker——身份判定的唯一载体 = ``D5_END|u2=``
  （d5.rs:117-130，E10 身份核对的探针侧收口）；身份未证实的 RECV 绝不折算
  observed-true（:581-584）。
- **U5（B3）**：``not_attempted`` 为 ets 真实发射字面（ets:232-252），其 u5
  落值按矩阵终局分派（:479 取值域闭合）：first-accept-lock 后 →
  ``protocol-first-accept-lock``、timeout 终止后 →
  ``matrix-terminated-on-create-timeout``——不得记 value-outside。
- **U4（B7）**：七子项逐项解析 ret/errno/fd/reuse（d6.rs:36-121 占位冻结、
  :977 不得空置）——缺字段 → F4，不得整包 observed-true。
- **B6**：删除全部判据无定义的 cause 字面（mb1-not-retained 等）；
  ``marker-gap-indeterminate`` 只保留在判据实际授权的位点（:657 u7——本模块
  内仅 u7 两处）；未预注册 cause 的缺口一律 ``unregistered-cause:`` 前缀
  facet → 调用方送 F4（:1373-1380 两分法）。
- **M1**：D7 ``start_mono_ms``/``elapsed_ms`` 有符号解析，负值/不可解析 →
  F8 单调钟/解析域分支（:829/:1150-1151），不归 F4 missing。

- **U1-U7 三态字段表**（:278-287）：全部字段三态
  ``observed-true`` / ``observed-false`` / ``unobservable``；
- 每分量 fail-closed：前提缺 → ``unobservable(cause=<预注册 cause>)``；
  域外有效平台值 → ``unobservable(cause=value-outside-frozen-domain)``
  （判别方法 (1)，:1375-1376 不 fail）；解析缺口/真值表缺口 → F8 facet 由
  verdict 承载（:1150）；未预注册 cause → F4 facet（:1379 (4)）。
- 纯函数：无 I/O、无时钟、无 hdc；模块导入零副作用。
"""

from __future__ import annotations

import re
from typing import Any, Dict, FrozenSet, List, Mapping, Optional, Sequence, Tuple

import n1bdisc_core as core
import n1bdisc_death as death
import n1bdisc_fsm as fsm

_UNOBS = core.unobservable_value

# ---------------------------------------------------------------------------
# 冻结常量（规格字面）
# ---------------------------------------------------------------------------

#: ``u1_match_offset`` 取值域（规格 :539：{0/4/both/none}，首个匹配帧可用 offset）。
U1_MATCH_OFFSETS: Tuple[str, ...] = ("0", "4", "both", "none")
#: D4 冻结目的地址（MR* 保留 / MB1 保留，规格 :537；producer net.rs dst_peer）。
D4_FROZEN_DST_MR = "10.99.0.2"
D4_FROZEN_DST_MB1 = "192.0.2.2"
#: D4 冻结 dport（规格 :537：目的端口 47001）。
D4_DPORT = 47001
#: D4 冻结 payload 身份族（d4.rs:52-58 net.rs d4_payload / 规格 :537）：
#: ``"N1DISCD4"(8B) | 0x01 | seq BE16 ∈ 1..20 | 0x5A×5``。
D4_MAGIC = b"N1DISCD4"
D4_PAYLOAD_LEN = 16
D4_SEQ_MAX = 20
#: D4/D5 冻结前提完整成功值（sendto n==16 / write n==44，规格 :540/:581）。
D4_PREMISE_LEN = 16
D5_PREMISE_LEN = 44
#: D5 冻结 src 地址（MR* 保留 / MB1 保留，规格 :576/:578；d5.rs:41 frozen_src）。
D5_FROZEN_SRC_MR = "10.99.0.2"
D5_FROZEN_SRC_MB1 = "192.0.2.2"
#: D5 身份端口（d5.rs:95-96：``from.sin_port == 47001``）。
D5_IDENTITY_PORT = 47001
#: D7 elapsed 接受域互斥区间（规格 :645-651 E5）。
D7_DURATION_MS = 20000
D7_GRACE_UPPER_MS = 25000
#: 矩阵条目 id 域（规格 :421；u5 逐候选键）。
MATRIX_ENTRY_IDS: Tuple[str, ...] = ("MR1", "MR1B", "MR2", "MR3", "MB1")
#: ``N1BDISC_D2_ENTRY|outcome=`` 冻结域（规格 :511；producer ets OUTCOME_*）。
D2_OUTCOME_DOMAIN: FrozenSet[str] = frozenset((
    "resolved", "rejected", "timeout", "late-resolved", "late-rejected",
    "indeterminate", "not_attempted",
))
#: 矩阵 timeout 终止族（规格 :474-475「timeout 及其后续结局」；
#: late-resolved 属保留、不入终止族）。
D2_TIMEOUT_TERMINATION: FrozenSet[str] = frozenset((
    "timeout", "late-rejected", "indeterminate"))

_INT_RE = re.compile(r"\A-?[0-9]+\Z")
#: D5 RECV src「地址:端口」形态（d5.rs:83-90 format! 字面）。
_RECV_SRC_RE = re.compile(r"\A\d{1,3}(?:\.\d{1,3}){3}:\d+\Z")

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


def _parse_int(text: Any) -> Optional[int]:
    """带符号十进制整数字面解析（M1：负值须可见；非整数字面 → None）。"""
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
# u1/u3 共用：D4 身份匹配证据（B1 —— 以真实 producer 载体为基准）
# ---------------------------------------------------------------------------

def _d4_frozen_dst(retained_entry: Optional[str]) -> str:
    return D4_FROZEN_DST_MB1 if retained_entry == "MB1" else D4_FROZEN_DST_MR


def _ipv4_udp_payload_at(frame: bytes, off: int
                         ) -> Optional[Tuple[int, str, int, bytes]]:
    """offset 处解析 IPv4+UDP（producer net.rs parse_ipv4_at :146-170 同构）。

    返回 ``(proto, dst, dport, udp_payload)``；不可解析 / proto≠17 / 长度不足
    → ``None``。UDP payload 取 ``off+28..off+44``（16 B 冻结身份，net.rs
    ``off + 20 + 8 + 16`` 上界切片的 runner 侧等价）。
    """
    if len(frame) < off + 20:
        return None
    if frame[off] >> 4 != 4 or frame[off] & 0x0F != 5:
        return None
    proto = frame[off + 9]
    if proto != 17:
        return None
    if len(frame) < off + 28:
        return None
    dst = ".".join(str(b) for b in frame[off + 16:off + 20])
    dport = (frame[off + 22] << 8) | frame[off + 23]
    payload = frame[off + 28:off + 28 + D4_PAYLOAD_LEN]
    return proto, dst, dport, payload


def _d4_payload_in_frozen_family(payload: bytes) -> bool:
    """16 B payload 是否 ∈ 冻结身份族（d4.rs:62 net.rs d4_payload；:537）。"""
    if len(payload) != D4_PAYLOAD_LEN:
        return False
    if payload[:8] != D4_MAGIC or payload[8] != 0x01:
        return False
    if any(b != 0x5A for b in payload[11:16]):
        return False
    seq = (payload[9] << 8) | payload[10]
    return 1 <= seq <= D4_SEQ_MAX


def d4_match_evidence(*, events: Sequence[Any], chunks: Any,
                      retained_entry: Optional[str]) -> Dict[str, Any]:
    """D4 身份匹配证据（B1；取代旧的「首个 off≠none 的 D4_READ」误读）。

    producer 语义（d4.rs）：``u3hex`` chunk（stream=u3hex,item=0）**恰在首个
    匹配受控包上发射**（:186-187，先于 break），是其前 64 字节十六进制；
    ``D4_READ|off=`` 只承载可解析性（:94-95），不承载匹配结果；外来包
    first-64 走 ``foreign`` 流（:127-131）。返回::

        {"match": bool, "frame": bytes|None, "hex_text": str|None,
         "offsets": frozenset({"0","4"}),   # 身份复核通过的 offset 集
         "f4": [...], "f8": [...], "raw": {...}}

    - chunk 在 → match=True，frame 按 :537 冻结身份在 offset-0/4 复核；
      两 offset 均不通过 → F8（chunk 声称匹配帧而身份不符，capture 自相矛盾）；
    - chunk 缺而 ``D4_END|u1=observed-true`` → F4（增量落盘缺项 :1128/:430①）；
    - chunk 在而 ``D4_END|u1=observed-false`` → F8（capture 自相矛盾）。
    """
    out: Dict[str, Any] = {"match": False, "frame": None, "hex_text": None,
                           "offsets": frozenset(), "f4": [], "f8": [],
                           "raw": {}}
    end = _first(events, "N1BDISC_D4_END")
    end_u1 = end.kv.get("u1") if end is not None else None
    hex_text = chunks.texts.get(("u3hex", 0)) if chunks is not None else None
    out["raw"]["d4_end_u1"] = end_u1
    if hex_text is None:
        if end_u1 == "observed-true":
            out["f4"].append("u3hex chunk group missing while D4_END|u1="
                             "observed-true (increment gap, :1128/:430-1)")
        return out
    out["match"] = True
    out["hex_text"] = hex_text
    try:
        frame = bytes.fromhex(hex_text)
    except ValueError:
        out["f8"].append("u3hex chunk not valid hex: %r" % (hex_text[:32],))
        return out
    out["frame"] = frame
    dst_frozen = _d4_frozen_dst(retained_entry)
    offsets = set()
    for off in (0, 4):
        parsed = _ipv4_udp_payload_at(frame, off)
        if parsed is None:
            continue
        proto, dst, dport, payload = parsed
        if (proto == 17 and dst == dst_frozen and dport == D4_DPORT
                and _d4_payload_in_frozen_family(payload)):
            offsets.add(str(off))
    out["offsets"] = frozenset(offsets)
    out["raw"]["frozen_dst"] = dst_frozen
    if not offsets:
        out["f8"].append(
            "u3hex frame fails frozen D4 identity at both offsets "
            "(proto=17+dst=%s+dport=%d+16B payload, d4.rs:97-125/:537)"
            % (dst_frozen, D4_DPORT))
    if end_u1 == "observed-false":
        out["f8"].append("capture self-contradiction: u3hex chunk present "
                         "(match, d4.rs:186-187) but D4_END|u1=observed-false")
    return out


# ---------------------------------------------------------------------------
# u1（D4 socket→tun 投递；规格 :534-547）
# ---------------------------------------------------------------------------

def _premise_state(rets: Sequence[Optional[int]], prem_len: int) -> str:
    """D4/D5 前提三分（:540/:581）。state ∈ {"holds","all-err","short-or-zero",
    "undecidable","no-marker"}（unparsable 字面混入且无完整成功 → 不可判）。"""
    if not rets:
        return "no-marker"
    known = [r for r in rets if r is not None]
    if any(r == prem_len for r in known):
        return "holds"
    if not known:
        return "undecidable"
    if all(r == -1 for r in known) and len(known) == len(rets):
        return "all-err"
    if all(r == -1 for r in known):
        return "undecidable"
    return "short-or-zero"


def _premise_cause(state: str, err_cause: str) -> Optional[str]:
    return {"all-err": err_cause,
            "short-or-zero": "short-or-zero-io"}.get(state)


def derive_u1(*, events: Sequence[Any],
              retained_entry: Optional[str],
              dw_skip: Optional[str] = None,
              chunks: Any = None) -> Dict[str, Any]:
    """u1_socket_to_tun_delivery + u1_match_offset + u1_no_route_control。

    判定载体（B1）：前提 = ``D4_SENT|ret=``（sendto n==16，:540）；匹配事实 =
    ``u3hex`` chunk（d4.rs:186-187 恰在首匹配帧发射）+ ``D4_END|u1=`` 终态
    （探针侧身份收口）；``D4_READ|off=`` 仅登记可解析性、不参与匹配（外来可
    解析包绝不产生 observed-true）。MB1 保留 → 主字段固定 ``mb1-no-route``、
    对照字段按同一前提/匹配门（:542-543）；MR* 保留 → 对照字段镜像主字段
    （producer d4.rs:159-168 同值发射）。``D4_END`` 缺（非 skip）→ F4
    （D 项终态 missing，:1128），不伪造 cause（B6）。
    """
    out: Dict[str, Any] = {
        "u1_socket_to_tun_delivery": None,
        "u1_match_offset": None,
        "u1_no_route_control": None,
        "raw": {},
    }
    f4: List[str] = []
    f8: List[str] = []
    skip = _skip_item_cause(events, "D4") or dw_skip
    if skip in ("no-live-fd", "dup-failed"):
        cause = _UNOBS(skip)
        out.update(u1_socket_to_tun_delivery=cause, u1_match_offset=cause,
                   u1_no_route_control=cause, raw={"branch": "skip"})
        return out

    if chunks is None:
        chunks = core.ChunkReassembly({}, (), ())
    ev = d4_match_evidence(events=events, chunks=chunks,
                           retained_entry=retained_entry)
    f4.extend(ev["f4"])
    f8.extend(ev["f8"])
    match = ev["match"]

    sent = _named(events, "N1BDISC_D4_SENT")
    rets: List[Optional[int]] = []
    unparsable: List[str] = []
    for e in sent:
        ret = _parse_int(e.kv.get("ret"))
        if ret is None and e.kv.get("ret") is not None:
            unparsable.append(e.raw)
        rets.append(ret)
    if unparsable:
        f8.append("D4_SENT ret unparsable: %s" % (unparsable,))
    end = _first(events, "N1BDISC_D4_END")
    end_present = end is not None
    end_u1 = ev["raw"]["d4_end_u1"]
    premise = _premise_state(rets, D4_PREMISE_LEN)
    out["raw"] = {
        "send_rets": [e.kv.get("ret") for e in sent],
        "d4_end_present": end_present,
        "d4_end_u1": end_u1,
        "match": match,
        "identity_offsets": sorted(ev["offsets"]),
    }

    def evaluated() -> Optional[str]:
        """前提 + 匹配 + 收口三态求值（u1 主字段 / MB1 对照字段同一门）。"""
        cause = _premise_cause(premise, "send-failed")
        if cause is not None:
            return _UNOBS(cause)
        if premise in ("no-marker", "undecidable"):
            # D4_SENT 全缺：D4_END 在而其 u1=send-failed（d4.rs:28-40 socket
            # 创建失败形态——无 SENT marker）→ 以 END 值为准；其余 → F4。
            if premise == "no-marker" and end_u1 == _UNOBS("send-failed"):
                return end_u1
            f4.append("D4 premise undecidable: no usable D4_SENT rets "
                      "(missing, :1128)")
            return None
        if match:
            return "observed-true"          # :541 任一 offset 匹配（chunk 载体）
        if end_u1 == "observed-true":
            # 身份判定的另一载体 = END 终态（d4.rs:171）；chunk 缺 = 增量落盘
            # 缺项（F4 已记于 d4_match_evidence），不否定匹配事实。
            return "observed-true"
        if end_present:
            return "observed-false"         # 窗口收口零匹配
        f4.append("D4 terminal missing: premise holds, no match, no D4_END "
                  "(missing, :1128)")
        return None

    if retained_entry == "MB1":
        # :542 主字段固定；对照字段按 E9 求值表（:543-547）。
        out["u1_socket_to_tun_delivery"] = _UNOBS("mb1-no-route")
        out["u1_no_route_control"] = evaluated()
    else:
        value = evaluated()
        out["u1_socket_to_tun_delivery"] = value
        # MR* 保留：对照字段镜像主字段（producer d4.rs:159-168 同值发射）。
        out["u1_no_route_control"] = value

    # u1_match_offset（:539 闭域 {0/4/both/none}）
    if match:
        if ev["offsets"]:
            offs = sorted(ev["offsets"])
            out["u1_match_offset"] = "both" if len(offs) == 2 else offs[0]
        # 身份复核未通过 / hex 不可解码 → F8 已记、值不落（域内无该格）。
    elif end_present:
        out["u1_match_offset"] = "none"                 # :539 零匹配显式载体
    else:
        f4.append("D4 terminal missing: no match, no D4_END for "
                  "u1_match_offset (missing, :1128)")

    # 探针终态 vs runner 重建交叉核对（MR* 比对重建值；MB1 比对固定主字段）。
    if end_u1 is not None:
        rebuilt = out["u1_socket_to_tun_delivery"]
        expected = _UNOBS("mb1-no-route") if retained_entry == "MB1" \
            else rebuilt
        if end_u1 != expected and expected is not None:
            f8.append("capture self-contradiction: D4_END|u1=%r != runner "
                      "rebuild %r" % (end_u1, expected))
    out["f4_facets"] = f4
    out["f8_facets"] = f8
    return out


# ---------------------------------------------------------------------------
# u2（D5 tun 写入 → sink；规格 :573-586）
# ---------------------------------------------------------------------------

def derive_u2(*, events: Sequence[Any],
              retained_entry: Optional[str],
              dw_skip: Optional[str] = None) -> Dict[str, Any]:
    """u2_tun_write_to_sink_delivery（:581-584；B2 完整身份语义）。

    判定载体：前提 = ``D5_WRITE|ret=``（write n==44）；**身份判定 = ``D5_END|u2=``**
    （d5.rs:117-130：src=冻结 src:47001 ∧ payload 与本轮 16 B 身份逐字节相等
    的探针侧收口——E10 的可判定等价在 marker 面只有该终态字段）。RECV 的
    ``src=`` 为「地址:端口」形态（d5.rs:83-91）：形态校验 + 与冻结
    ``地址:47001`` 的字面对照仅登记进 raw；**身份未证实的 RECV 绝不折算
    observed-true**（坏 payload 反例 = RECV 在而 END=observed-false）。
    ``D5_END`` 缺（非 skip）→ F4（D 项终态 missing，:1128），不伪造 cause。
    """
    out: Dict[str, Any] = {"u2_tun_write_to_sink_delivery": None, "raw": {}}
    f4: List[str] = []
    f8: List[str] = []
    skip = _skip_item_cause(events, "D5") or dw_skip
    if skip in ("no-live-fd", "dup-failed"):
        out["u2_tun_write_to_sink_delivery"] = _UNOBS(skip)
        out["raw"] = {"branch": "skip"}
        return out

    frozen_src_ip = D5_FROZEN_SRC_MB1 if retained_entry == "MB1" \
        else D5_FROZEN_SRC_MR
    frozen_src = "%s:%d" % (frozen_src_ip, D5_IDENTITY_PORT)
    writes = _named(events, "N1BDISC_D5_WRITE")
    rets: List[Optional[int]] = []
    unparsable: List[str] = []
    for e in writes:
        ret = _parse_int(e.kv.get("ret"))
        if ret is None and e.kv.get("ret") is not None:
            unparsable.append(e.raw)
        rets.append(ret)
    if unparsable:
        f8.append("D5_WRITE ret unparsable: %s" % (unparsable,))
    recvs = _named(events, "N1BDISC_D5_RECV")
    recv_srcs: List[str] = []
    src_form_matches: List[str] = []
    src_ip_only: List[str] = []
    for e in recvs:
        src = e.kv.get("src")
        recv_srcs.append(src)
        if not isinstance(src, str) or not _RECV_SRC_RE.match(src):
            f8.append("D5_RECV src not addr:port literal: %r (d5.rs:83-91)"
                      % (src,))
            continue
        if src == frozen_src:
            src_form_matches.append(src)
        elif src.split(":", 1)[0] == frozen_src_ip:
            src_ip_only.append(src)     # 地址符而端口≠47001 → 身份不成立
    end = _first(events, "N1BDISC_D5_END")
    end_present = end is not None
    end_u2 = end.kv.get("u2") if end is not None else None
    premise = _premise_state(rets, D5_PREMISE_LEN)
    out["raw"] = {
        "frozen_src": frozen_src,
        "write_rets": [e.kv.get("ret") for e in writes],
        "recv_srcs": recv_srcs,
        "recv_src_identity_form_matches": src_form_matches,
        "recv_src_ip_only_matches": src_ip_only,
        "d5_end_present": end_present,
        "d5_end_u2": end_u2,
        "identity_unconfirmed_recvs": bool(src_form_matches)
        and end_u2 == "observed-false",
    }
    cause = _premise_cause(premise, "write-failed")
    if cause is not None:
        value = _UNOBS(cause)
    elif premise in ("no-marker", "undecidable"):
        value = None
        f4.append("D5 premise undecidable: no usable D5_WRITE rets "
                  "(missing, :1128)")
    elif end_u2 in ("observed-true", "observed-false"):
        # 身份判定的唯一 marker 载体 = 探针 E10 收口（:582-583）。
        value = end_u2
    elif end_u2 is not None and end_u2.startswith("unobservable(cause="):
        # 探针侧前提类 cause（write-failed/short-or-zero-io，d5.rs:120-125）。
        value = end_u2
    elif end_u2 is not None:
        value = None
        f8.append("D5_END|u2=%r outside three-state closeout domain "
                  "(d5.rs:117-130)" % (end_u2,))
    else:
        value = None
        f4.append("D5 terminal missing: premise holds, no D5_END "
                  "(missing, :1128)")
    out["u2_tun_write_to_sink_delivery"] = value
    # 交叉核对：END 身份结论与前提矛盾（如全部 -1 却 observed-true）。
    if end_u2 in ("observed-true", "observed-false") and cause is not None:
        f8.append("capture self-contradiction: D5_END|u2=%r with premise %r"
                  % (end_u2, premise))
    out["f4_facets"] = f4
    out["f8_facets"] = f8
    return out


# ---------------------------------------------------------------------------
# u3（D4 首帧 dump；规格 :548-568）
# ---------------------------------------------------------------------------

def _ipv4_parsable(frame: bytes, offset: int) -> bool:
    """offset 处可解析 IPv4（version=4、IHL=5；producer net.rs parse_ipv4_at）。"""
    return (len(frame) >= offset + 4 and frame[offset] >> 4 == 4
            and frame[offset] & 0x0F == 5)


def _pi_header_enum(frame: bytes) -> Tuple[str, str]:
    """S5 互斥分区表（规格 :551-561；镜像 producer net.rs u3_prefix_class :193-211）。

    返回 ``(u3_pi_header_present, other_prefix_raw4)``（无前缀原文时 raw4 为空串）。
    tun_pi-like 判定沿 producer：前 2 字节 flags 均 0 且后 2 字节大端
    proto=0x0800（net.rs:203）。
    """
    p0 = _ipv4_parsable(frame, 0)
    p4 = _ipv4_parsable(frame, 4)
    if not p0 and not p4:
        return "unparsable", ""
    if p0 and not p4:
        return "no-prefix", ""
    if p4 and not p0:
        if len(frame) >= 4 and frame[0] == 0 and frame[1] == 0 \
                and frame[2] == 0x08 and frame[3] == 0x00:
            return "tun_pi-like", ""
        return "other-prefix", frame[:4].hex()
    return "ambiguous", ""       # :558 两可解析 → 无条件 ambiguous（不被截胡）


def _readlen_class(read_len: Optional[int], frame: bytes, at: int) -> str:
    """readlen vs total_length 五值分类（:562；镜像 net.rs readlen_vs_total）。

    read_len 不可判（帧恰 64 B 被 chunk 截断，n 只知 ≥64）→ ``unobservable``
    （:562 域成员）；offset 处不可解析 → ``unparsable``。
    """
    if read_len is None:
        return "unobservable"
    if not _ipv4_parsable(frame, at) or len(frame) < at + 4:
        return "unparsable"
    total = (frame[at + 2] << 8) | frame[at + 3]
    if read_len == total:
        return "equal"
    return "readlen>total_length" if read_len > total else "readlen<total_length"


def derive_u3(*, events: Sequence[Any],
              chunks: Any,
              retained_entry: Optional[str],
              dw_skip: Optional[str] = None) -> Dict[str, Any]:
    """u3_* 五字段 + off0 对照字段（规格 :548-568）。

    决定性数据（B1）= **首个匹配受控包**——其唯一 capture 载体 = ``u3hex``
    chunk（d4.rs:186-187 恰在首匹配帧发射，即 U1 匹配事实本身）；关联规则：
    chunk 在 → 匹配发生、帧字节可用；chunk 缺 → 零匹配（含窗口耗尽、MB1 保留
    零匹配）→ 全部 ``unobservable(cause=no-controlled-read)``（:548；MB1 保留
    且匹配发生时按 chunk 决定性数据求值——producer 对 MB1 匹配帧同样发射 chunk
    与 U3 字段，d4.rs:183-213；遗留偏差登记）。决定性 offset（:563 E4）：
    off=both → 4（producer d4.rs:189 可解析性驱动：o4 优先）；
    ``u3_readlen_vs_total_length_off0`` 并记（:564，producer d4.rs:191-212
    无条件并记）。
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
        out["f4_facets"] = []
        out["f8_facets"] = []
        return out

    f4: List[str] = []
    f8: List[str] = []
    ev = d4_match_evidence(events=events, chunks=chunks,
                           retained_entry=retained_entry)
    f4.extend(ev["f4"])
    f8.extend(ev["f8"])
    end_present = _first(events, "N1BDISC_D4_END") is not None

    if not ev["match"]:
        # 零匹配 → no-controlled-read（:548/:567）；D4_END 亦缺 → F4（B6）。
        if end_present:
            cause = _UNOBS("no-controlled-read")
            for key in ("u3_first_read_len", "u3_first64_hex",
                        "u3_pi_header_present", "u3_prefix_format",
                        "u3_readlen_vs_total_length",
                        "u3_readlen_vs_total_length_off0"):
                out[key] = cause
        else:
            f4.append("D4 terminal missing: no match, no D4_END for u3 "
                      "(missing, :1128)")
        out["raw"] = {"match": False}
        out["f4_facets"] = f4
        out["f8_facets"] = f8
        return out

    frame = ev["frame"]
    if frame is None:
        # chunk 在而 hex 不可解码（F8 已记）：字段不落值。
        out["raw"] = {"match": True, "hex_undecodable": True}
        out["f4_facets"] = f4
        out["f8_facets"] = f8
        return out

    out["u3_first64_hex"] = ev["hex_text"]
    # read_len 仅当整帧在 chunk 内（<64 B）可判；恰 64 B 时 n 只知 ≥64
    # → u3_first_read_len 不落值、readlen 比较字段落域内 unobservable（:562）。
    read_len: Optional[int] = len(frame) if len(frame) < 64 else None
    out["u3_first_read_len"] = read_len
    pi_enum, other_raw = _pi_header_enum(frame)
    out["u3_pi_header_present"] = pi_enum
    out["raw"]["other_prefix_raw4"] = other_raw   # :557 other-prefix 逐字登记原文
    prefix_map = {
        "tun_pi-like": "observed-true",
        "no-prefix": "observed-false",
        "other-prefix": "observed-false",
        "ambiguous": _UNOBS("prefix-ambiguous"),
        "unparsable": _UNOBS("frame-unparsable"),
    }
    out["u3_prefix_format"] = prefix_map[pi_enum]

    # 决定性 offset（:563 E4 / producer d4.rs:189：o4 优先，可解析性驱动）
    decisive = 4 if _ipv4_parsable(frame, 4) else 0
    out["u3_readlen_vs_total_length"] = _readlen_class(read_len, frame, decisive)
    out["u3_readlen_vs_total_length_off0"] = _readlen_class(read_len, frame, 0)
    out["raw"]["decisive_offset"] = decisive
    out["raw"]["frame_len"] = len(frame)
    out["raw"]["match"] = True
    out["f4_facets"] = f4
    out["f8_facets"] = f8
    return out


# ---------------------------------------------------------------------------
# u4（D6 destroy 后同步尝试；规格 :981-984 + pre-only 四支 :1185-1195）
# ---------------------------------------------------------------------------

def _parse_u4_result(subitem: str, marker: Any,
                     f4: List[str]) -> Optional[Dict[str, Any]]:
    """u4 子项 result marker 字段解析（B7：缺字段 → F4，不整包 observed-true）。

    S1-S6：``ret``/``errno`` 均须整数字面（d6.rs:36-111 恒双字段发射；:977
    占位冻结不得空置）；S7：``fd`` 整数 + ``reuse`` ∈ {true,false}
    （d6.rs:121）。解析失败 → 返回 ``None`` 并登记 F4 facet。
    """
    if subitem == "u4_dup_fd_reuse":
        raw_fd = marker.kv.get("fd")
        fd = _parse_int(raw_fd)
        reuse = marker.kv.get("reuse")
        if raw_fd is None or fd is None:
            f4.append("D6S7_R.fd missing/unparsable (frozen placeholder, :979)")
            return None
        if reuse not in ("true", "false"):
            f4.append("D6S7_R.reuse=%r outside {true,false} (d6.rs:121)"
                      % (reuse,))
            return None
        return {"fd": fd, "reuse": reuse == "true"}
    raw_ret = marker.kv.get("ret")
    raw_errno = marker.kv.get("errno")
    ret = _parse_int(raw_ret)
    errno = _parse_int(raw_errno)
    if raw_ret is None or ret is None:
        f4.append("%s.ret missing/unparsable (frozen placeholder, :977)"
                  % marker.name)
        return None
    if raw_errno is None or errno is None:
        f4.append("%s.errno missing/unparsable (frozen placeholder, :977)"
                  % marker.name)
        return None
    return {"ret": ret, "errno": errno}


def derive_u4(*, events: Sequence[Any],
              destroy_call_state: str,
              death_observed: bool,
              post_present: bool,
              dw_skip: Optional[str] = None) -> Dict[str, Any]:
    """u4 七子项 + 摘要（规格 :981-984；pre-only 覆盖 :1185-1195；B7 逐项解析）。

    - D-W 整体 skip → 全部 ``unobservable(cause=<skip cause>)``（:999-1000）；
    - ``SKIP|item=D6b``（join-timeout-abandoned）→ dup 面四子项
      ``unobservable(cause=d6b-skipped-join-timeout)``（:969）；
    - result marker 在**且字段齐** → ``observed-true``（:982「ret 已登记」；
      缺字段 → F4、值不落，B7）；
    - pre-only（POST 缺 ∧ 死亡分量 true）→ 未执行子项按 ``destroy_call_state``
      四支分岔（:1187-1193）；pre 在 + 死亡 + 零 result（complete）→
      ``observed-false``；
    - 其余（无 pre/result、进程活）→ F4（D 项终态 missing，:1128/:439-440），
      不伪造 cause（B6）。
    """
    out: Dict[str, Any] = {"subitems": {}, "raw": {}}
    f4: List[str] = []
    f8: List[str] = []
    d6b_skip_cause = _skip_item_cause(events, "D6b")
    pre_only = (not post_present) and death_observed
    branch_cause: Optional[str] = None
    if pre_only:
        branch_map = {
            "not-called": "destroy-not-called",                  # :1189 (1b)
            "not-reached": "destroy-not-reached",                # :1188 (1a)
            "call-boundary-incomplete": "call-boundary-incomplete",  # :1191
            "marker-contradiction": "marker-contradiction",      # F3 轴承载
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
            branch_cause = "destroy-not-reached"

    parsed_results: Dict[str, Dict[str, Any]] = {}
    # resolve 证据 = D6a result marker（ets:451 d6a 在 destroy resolve 回调内同栈
    # 执行——D6S1_R 在 ⟺ destroy 已 resolve）。
    resolve_evidence = any(_first(events, rname) is not None
                           for _, _, rname in U4_SUBITEM_MARKERS[:3])
    for subitem, pre_name, result_name in U4_SUBITEM_MARKERS:
        result_marker = _first(events, result_name)
        pre_marker = _first(events, pre_name)
        if dw_skip in ("no-live-fd", "dup-failed"):
            value = _UNOBS(dw_skip)                              # :999-1000
        elif d6b_skip_cause is not None and subitem in U4_DUP_SUBITEMS:
            value = _UNOBS("d6b-skipped-join-timeout")           # :969
        elif result_marker is not None:
            parsed = _parse_u4_result(subitem, result_marker, f4)
            if parsed is None:
                value = None                                     # B7：缺字段 → F4
            else:
                value = "observed-true"                          # :982/:1186
                parsed_results[subitem] = parsed
        elif pre_only:
            value = ("observed-false" if branch_cause is None
                     else _UNOBS(branch_cause))                  # :1192/(3) 支
        elif pre_marker is not None and death_observed:
            value = "observed-false"                             # :982
        elif (destroy_call_state == "call-returned"
              and not resolve_evidence and not death_observed):
            # complete ∧ ``_C`` 在 ∧ 无 resolve 证据（D6a 未跑）→ 沿 V2 (4) 同款
            # ``unobservable(cause=destroy-unresolved)``（:1193；物理事实同一：
            # 已调用未返回，destroy 10 s 盒到期跳过 D6a——ets:446-465/判据 :1046；
            # 与 d6_items D6S1-3 的 skipped(cause=destroy-unresolved) 投影一致，
            # producer dw.rs:686-687）。
            value = _UNOBS("destroy-unresolved")
        else:
            # B6/B7：marker 双缺且进程活 → D 项终态 missing → F4，不伪造 cause。
            value = None
            f4.append("%s terminal missing: no pre/result marker, process "
                      "alive (missing, :1128/:439-440)" % subitem)
        out["subitems"][subitem] = value

    # 摘要（:983-984 全序 unobservable < observed-false < observed-true）
    values = list(out["subitems"].values())

    def _rank(v: Optional[str]) -> int:
        if v == "observed-true":
            return 2
        if v == "observed-false":
            return 1
        return 0   # 一切 unobservable 字面同为最弱值；None（F4 已挂）亦同

    if any(v == "observed-true" for v in values):
        out["u4_post_destroy_sync_observable"] = "observed-true"
    elif all(_rank(v) == 1 for v in values):
        out["u4_post_destroy_sync_observable"] = "observed-false"
    elif all(_rank(v) == 0 and v is not None for v in values):
        out["u4_post_destroy_sync_observable"] = \
            values[0] if values else None
    else:
        out["u4_post_destroy_sync_observable"] = None
    out["raw"]["d6b_skip_cause"] = d6b_skip_cause
    out["raw"]["pre_only_branch"] = branch_cause if pre_only else None
    out["raw"]["result_fields"] = parsed_results
    out["f4_facets"] = f4
    out["f8_facets"] = f8
    return out


# ---------------------------------------------------------------------------
# u5（D2 矩阵逐候选；规格 :478-482）
# ---------------------------------------------------------------------------

def d2_final_outcomes(events: Sequence[Any]) -> Dict[str, str]:
    """同 id 多条 ``D2_ENTRY`` outcome 行 → 终值映射（后到者为准，m-07）。"""
    final: Dict[str, str] = {}
    for e in events:
        if e.name != "N1BDISC_D2_ENTRY":
            continue
        outcome = e.kv.get("outcome")
        if outcome is None:
            continue
        final[e.kv.get("id", "")] = outcome
    return final


def derive_u5(*, events: Sequence[Any]) -> Dict[str, Any]:
    """u5_routeinfo_acceptance 逐候选（规格 :479 取值域闭合；B3）。

    ``not_attempted`` 是 ets 真实发射字面（ets:232-252：first-accept 后对未执行
    条目显式发 not_attempted；matrix 终止后同理；MR1B 在 MR1 未拒时不执行亦发），
    其 u5 落值按矩阵终局分派——保留条目在 → ``protocol-first-accept-lock``
    （:481）、timeout 终止族在 → ``matrix-terminated-on-create-timeout``
    （:475/:479）；outcome 字面域外 → F8（producer 分类缺陷）；条目 outcome
    marker 全缺（capture 缺失）→ F4（B6：不伪造 cause）。
    """
    final = d2_final_outcomes(events)
    retained = retained_entry_id(events)
    terminated_on_timeout = any(
        v in D2_TIMEOUT_TERMINATION for v in final.values())
    outcome_map = {
        "resolved": "observed-true", "late-resolved": "observed-true",
        "rejected": "observed-false", "late-rejected": "observed-false",
        "indeterminate": _UNOBS("create-indeterminate"),   # :479 窗尽
        "timeout": _UNOBS("create-indeterminate"),         # 窗未收口的终值形态
        "not_attempted": None,                             # 按矩阵终局分派（B3）
    }
    f4: List[str] = []
    f8: List[str] = []
    out: Dict[str, Any] = {"candidates": {},
                           "raw": {"retained": retained,
                                   "terminated_on_timeout": terminated_on_timeout}}
    for entry_id in MATRIX_ENTRY_IDS:
        outcome = final.get(entry_id)
        if outcome is None:
            # producer 对每个条目（含未执行）恒发 outcome marker（ets:233/251）；
            # 全缺 = capture 缺失 → F4（B6）。
            out["candidates"][entry_id] = None
            f4.append("D2_ENTRY outcome marker missing for %s (missing, :1128)"
                      % entry_id)
            continue
        if outcome == "not_attempted":
            if retained is not None:
                out["candidates"][entry_id] = _UNOBS("protocol-first-accept-lock")
            elif terminated_on_timeout:
                out["candidates"][entry_id] = _UNOBS(
                    "matrix-terminated-on-create-timeout")
            else:
                out["candidates"][entry_id] = None
                f4.append("unregistered-cause: %s not_attempted without "
                          "matrix-termination evidence (:479 分派前件缺)"
                          % entry_id)
            continue
        if outcome not in D2_OUTCOME_DOMAIN:
            out["candidates"][entry_id] = None
            f8.append("D2_ENTRY outcome=%r outside frozen domain (spec :511, "
                      "ets OUTCOME_* producer literals)" % (outcome,))
            continue
        out["candidates"][entry_id] = outcome_map[outcome]
    out["f4_facets"] = f4
    out["f8_facets"] = f8
    return out


# ---------------------------------------------------------------------------
# u6（D2 步 2.4；规格 :492 + 派生表 :497-503）
# ---------------------------------------------------------------------------

def derive_u6(*, events: Sequence[Any],
              fd_roles_created: frozenset = frozenset(),
              dw_skip: Optional[str] = None) -> Dict[str, Any]:
    """u6_initial_flags_and_isblocking_effect + u6_nonblocking_initial（:492/:497-503）。

    ``D2_S4|u6=`` 域 = {``o_nonblock_present``, ``o_nonblock_absent``,
    ``unobservable``}（:492；producer d2.rs:62-84）。present/absent → 三态
    true/false；裸 ``unobservable``（producer F_GETFL 失败形态，d2.rs:62）：
    detail 字段逐字保真、主字段无预注册 cause（:501 分支表仅 no-live-fd/
    dup-failed/死亡路径）→ ``unregistered-cause:`` facet（B6 → F4）；S4 缺 →
    沿分支表（:501）；无 skip 而缺 → F4（B6）。
    """
    out: Dict[str, Any] = {
        "u6_initial_flags_and_isblocking_effect": None,
        "u6_nonblocking_initial": None,
        "raw": {},
    }
    f4: List[str] = []
    f8: List[str] = []
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
    elif literal == "unobservable":
        # :492 域成员（detail 字段逐字保真）；主字段 cause 无预注册（:501）→ B6。
        out["u6_initial_flags_and_isblocking_effect"] = "unobservable"
        f4.append("unregistered-cause: D2_S4 bare 'unobservable' without skip "
                  "branch (no preregistered cause, :501/:1379-4)")
    elif literal is not None:
        # S4 字面是探针分类（非平台原始值），域外 = producer 缺陷 → F8（:1377-2）。
        f8.append("D2_S4 u6=%r outside frozen literal set (spec :492, "
                  "producer d2.rs:62-84)" % (literal,))
    elif skip is not None:
        cause = _UNOBS(skip)                            # :501 沿分支表
        out["u6_initial_flags_and_isblocking_effect"] = cause
        out["u6_nonblocking_initial"] = cause
    elif dw_skip in ("no-live-fd", "dup-failed"):
        cause = _UNOBS(dw_skip)
        out["u6_initial_flags_and_isblocking_effect"] = cause
        out["u6_nonblocking_initial"] = cause
    else:
        # S4 缺 ∧ 无 skip ∧ 进程活 → F4（B6：D2 锁定序列未收口，不伪造 cause）。
        f4.append("D2_S4 marker missing without skip branch (missing, :1128)")
    out["f4_facets"] = f4
    out["f8_facets"] = f8
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
    skip 优先 :662、V4 tail-loss 约束 :663-666；M1 数值有符号解析）。

    求值序（先到先得）：
    (0) skip（``SKIP|item=D7`` 或 no-live-fd 全局分支）→ ``no-live-vpn``
        （:662/:996——无 live VPN 不得 true/false）；
    (1) 阶段未达（:658：死亡 ∧ ``D7_BEGIN`` 缺 ∧ ``last_visible_site`` ≤ P5T）→
        ``stage-not-reached``（start_mono/elapsed/iters 同 cause）；
    (2) ``D7_END`` 在：elapsed 接受域三区间（:645-651）——M1：``elapsed_ms``/
        ``start_mono_ms`` 有符号解析，负值 → F8 单调钟域门（:654/:829）、
        非整数字面 → F8 解析域缺口、键缺 → F4（:1150-1151 负值/不可解析
        不归 F4 missing）；``[0,20000)`` 矛盾 fail(F8) + ``d7-early-exit-anomaly``；
        ``[20000,25000]`` → ``observed-true``；``>25000`` →
        ``d7-elapsed-overshoot-beyond-grace`` 不 fail；
    (3) ``D7_END`` 缺 ∧ ``D7_BEGIN`` 在：死亡 ∧ BEGIN 后静默 →
        ``observed-false``，但位点 P6 ∧ ``possible-tail-loss`` →
        ``marker-tail-loss`` 不得 false（:663-666）；无死亡 →
        ``marker-gap-indeterminate``（:657 授权位点，B6 收窄核对）。
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
        # M1：start_mono_ms 有符号解析（:825 单调钟读数类；:829 非负性门）。
        raw_start = begin.kv.get("start_mono_ms")
        if raw_start is None:
            f4.append("D7_BEGIN.start_mono_ms missing")
        else:
            start = _parse_int(raw_start)
            if start is None:
                f8.append("D7 parse: D7_BEGIN.start_mono_ms=%r not an "
                          "integer literal (:1377-2)" % (raw_start,))
            elif start < 0:
                f8.append("monotonic-domain: D7 start_mono_ms=%d < 0 "
                          "(规格 :829)" % start)
                out["start_mono_ms"] = start
            else:
                out["start_mono_ms"] = start

    if end is not None:
        # (2) 接受域三区间（:645-651）；M1：elapsed 带符号解析（负值 → 单调钟域门
        # :654 先命中）；iters 计数器（非单调钟域门成员）：键缺 → F4、bogus → F8。
        raw_elapsed = end.kv.get("elapsed_ms")
        raw_iters = end.kv.get("iters")
        elapsed: Optional[int] = None
        if raw_elapsed is None:
            f4.append("D7_END.elapsed_ms missing")
        else:
            elapsed = _parse_int(raw_elapsed)
            if elapsed is None:
                f8.append("D7 parse: D7_END.elapsed_ms=%r not an integer "
                          "literal (:1377-2)" % (raw_elapsed,))
        if raw_iters is None:
            f4.append("D7_END.iters missing")
        else:
            iters = _parse_int(raw_iters)
            if iters is None or iters < 0:
                f8.append("D7 parse: D7_END.iters=%r not a non-negative "
                          "counter (d7.rs END emission)" % (raw_iters,))
            else:
                out["iters"] = iters
        out["elapsed_ms"] = elapsed if elapsed is not None else raw_elapsed
        if elapsed is None:
            out["u7_long_task_watchdog_behavior"] = None   # F8/F4 已挂
        elif elapsed < 0:
            # :654 单调钟域门（非负性）先命中（F8），标签沿 early-exit 同族
            f8.append("monotonic-domain: D7 elapsed_ms=%d < 0 (规格 :829)"
                      % elapsed)
            out["u7_long_task_watchdog_behavior"] = \
                _UNOBS("d7-early-exit-anomaly")
            out["d7_anomaly"] = "early-exit-with-end-marker"
        elif elapsed < D7_DURATION_MS:
            # :647/:653 矛盾输入 → fail(F8)（标签同时驱动 F8）
            f8.append("d7-early-exit-contradiction: D7_END present with "
                      "elapsed_ms=%d < 20000 (d7_anomaly="
                      "early-exit-with-end-marker); raw elapsed=%d"
                      % (elapsed, elapsed))
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
                       if e.line_no > begin.line_no
                       and e.name.startswith("N1BDISC_")]
        silent = not after_begin
        out["raw"]["silent_after_begin"] = silent
        if (death_observed and silent and last_site == "P6"
                and tail_state == "possible-tail-loss"):
            out["u7_long_task_watchdog_behavior"] = _UNOBS("marker-tail-loss")
        elif death_observed and silent:
            out["u7_long_task_watchdog_behavior"] = "observed-false"  # :655
        else:
            # :657 授权位点：仅有 marker 缺失而无死亡证据（B6 保留）。
            out["u7_long_task_watchdog_behavior"] = \
                _UNOBS("marker-gap-indeterminate")
    else:
        # BEGIN 缺、非阶段未达（位点 > P5T 或无死亡）：:657 授权位点（B6 保留）。
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
    （交 verdict 承载：F8 解析/真值表缺口 :1150、F4 增量落盘缺项 :1128；
    ``unregistered-cause:`` 前缀的 f4 facet 由调用方路由进
    ``unregistered_cause_hits``——B6）。
    """
    dw_skip = dw_skip_cause_of(events)
    retained = retained_entry_id(events)
    if destroy_call_state is None:
        destroy_call_state = death.eval_destroy_call_state(
            _skip_item_cause(events, "destroy") is not None,
            _first(events, "N1BDISC_DW_DESTROY_T") is not None,
            _first(events, "N1BDISC_DW_DESTROY_C") is not None)

    u1 = derive_u1(events=events, retained_entry=retained, dw_skip=dw_skip,
                   chunks=chunks)
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
    "U1_MATCH_OFFSETS", "D4_FROZEN_DST_MR", "D4_FROZEN_DST_MB1",
    "D5_FROZEN_SRC_MR", "D5_FROZEN_SRC_MB1",
    "D4_PREMISE_LEN", "D5_PREMISE_LEN", "D7_DURATION_MS", "D7_GRACE_UPPER_MS",
    "MATRIX_ENTRY_IDS", "D2_OUTCOME_DOMAIN", "U4_SUBITEM_MARKERS",
    "U4_DUP_SUBITEMS",
    "dw_skip_cause_of", "retained_entry_id", "d4_match_evidence",
    "d2_final_outcomes",
    "derive_u1", "derive_u2", "derive_u3", "derive_u4", "derive_u5",
    "derive_u6", "derive_u7", "fd_roles_created", "derive_platform_components",
]
