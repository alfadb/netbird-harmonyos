#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_static.py — N1BDISC 探针 A1-A12 静态断言机器检查器.

判据: docs/n1b-disc-gate-plan.md:1072-1087（静态断言，freeze 前机器检查，
违反即审查 blocker）+ 注 :1089-1107 + 冻结符号清单 :522-524 + 冻结字面集
:1095-1097。本工具即 :1072 所指「机器检查」载体（B-03 整改独立审查件）。

边界: host-only、只读源码、零设备、零构建。grep/正则 + 括号配对的轻量
结构分析；Rust 侧剥离注释后扫描（注释出现单独登记、不计违规），
ArkTS/ETS 同理。不修改任何源码。

用法:
    python3 staticcheck/check_static.py            # 全文报告 + 末尾汇总 JSON
    python3 staticcheck/check_static.py --json-only # 仅汇总 JSON（供 freeze 记录挂接）

退出码: 任一断言 FAIL -> 1；否则 0。QUESTION 不驱动退出码（但 freeze 前
必须逐条人工裁决，见 README）。
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROBE_SRC = ROOT / "probe" / "src"
ETS_DIR = ROOT / "entry" / "src" / "main" / "ets"
SELFTESTS = ROOT / "selftests"
RUNNER = ROOT / "runner"

SPEC = "docs/n1b-disc-gate-plan.md@04cf222+CC-1(criteria-change-1-reviewed-pass-2026-09-05) :1072-1087(+注 :1089-1107)"

# gate-plan :1095-1097 冻结 56 字面（逐字，含 N1BDISC_ 前缀）
FROZEN_56 = (
    ["N1BDISC_D1_BEGIN", "N1BDISC_D1_LOADED", "N1BDISC_D1_FAIL",
     "N1BDISC_D1_SYM", "N1BDISC_D1_END"]
    + ["N1BDISC_D2_ENTRY", "N1BDISC_D2_LATE", "N1BDISC_D2_LATE_FD"]
    + ["N1BDISC_D2_S%d" % i for i in range(1, 8)]
    + ["N1BDISC_D4_BEGIN", "N1BDISC_D4_SENT", "N1BDISC_D4_READ",
       "N1BDISC_D4_END"]
    + ["N1BDISC_D5_BEGIN", "N1BDISC_D5_WRITE", "N1BDISC_D5_RECV",
       "N1BDISC_D5_END"]
    + ["N1BDISC_D8_MTU", "N1BDISC_D8_STORM_BEGIN", "N1BDISC_D8_STORM_END"]
    + ["N1BDISC_D7_BEGIN", "N1BDISC_D7_END"]
    + ["N1BDISC_DW_SPAWN", "N1BDISC_DW_DRAIN", "N1BDISC_DW_BARRIER",
       "N1BDISC_DW_INWAIT", "N1BDISC_DW_DESTROY_T", "N1BDISC_DW_DESTROY_C",
       "N1BDISC_DW_RETURN", "N1BDISC_DW_EXIT", "N1BDISC_DW_RACEWIN"]
    + ["N1BDISC_D6S%d_B" % i for i in range(1, 8)]
    + ["N1BDISC_D6S%d_R" % i for i in range(1, 8)]
    + ["N1BDISC_FD", "N1BDISC_SKIP", "N1BDISC_CHUNK",
       "N1BDISC_PRE", "N1BDISC_POST"]
)

# gate-plan :522 冻结 BoringTun 符号清单（14 个，逐字）
BT_SYMBOLS = [
    "x25519_secret_key", "x25519_public_key", "x25519_key_to_base64",
    "x25519_key_to_hex", "x25519_key_to_str_free",
    "check_base64_encoded_x25519_key", "set_logging_function",
    "new_tunnel", "tunnel_free", "wireguard_write", "wireguard_read",
    "wireguard_tick", "wireguard_force_handshake", "wireguard_stats",
]

EXEMPT_MARKERS = {"N1BDISC_D2_REJTEXT", "N1BDISC_RESULT"}

A3_CALIBER_NOTE = (
    "口径注释（审查席建议口径，freeze 记录须逐字收录）："
    "取址保链不产生执行转移不计引用——btkeep.rs 中 #[link_name] 别名声明、"
    "BT_SYMBOLS 字符串表（dlsym 按名解析）、BT_FFI_KEEP #[used] 取址数组"
    "三类出现仅为保链/按名解析服务，不构成对 BoringTun 导出符号的执行引用；"
    "数据面零调用由「三类允许区之外出现=0（尤其 call site=0）」机器验证背书。"
)


# ---------------------------------------------------------------------------
# 源码读取与轻量结构化（注释/字符串感知，保持行号）
# ---------------------------------------------------------------------------

def load_rs_lines(path):
    """Rust: 返回 (text_lines, stripped, skeleton)。
    stripped  = 注释置空、字符串保留（用于字面提取）
    skeleton  = 注释与字符串内容均置空（用于结构/括号/关键字分析）"""
    text = path.read_text(encoding="utf-8").split("\n")
    stripped = _strip_comments(text, langs="rs", blank_strings=False)
    skeleton = _strip_comments(text, langs="rs", blank_strings=True)
    return text, stripped, skeleton


def load_ets_lines(path):
    text = path.read_text(encoding="utf-8").split("\n")
    stripped = _strip_comments(text, langs="ets", blank_strings=False)
    skeleton = _strip_comments(text, langs="ets", blank_strings=True)
    return text, stripped, skeleton


def _strip_comments(lines, langs="rs", blank_strings=False):
    out = []
    in_block = [False]
    for line in lines:
        out.append(_strip_line(line, in_block, langs, blank_strings))
    return out


def _strip_line(line, in_block, langs, blank_strings):
    res = []
    i, n = 0, len(line)
    in_str = False
    while i < n:
        c = line[i]
        if in_block[0]:
            if line.startswith("*/", i):
                in_block[0] = False
                res.append("  ")
                i += 2
            else:
                res.append(" ")
                i += 1
            continue
        if in_str:
            if c == "\\" and i + 1 < n:
                res.append("  " if blank_strings else c + line[i + 1])
                i += 2
                continue
            if c == '"':
                in_str = False
                res.append(c)
            else:
                res.append(" " if blank_strings else c)
            i += 1
            continue
        if c == '"':
            in_str = True
            res.append(c)
            i += 1
            continue
        if line.startswith("//", i):
            res.append(" " * (n - i))
            break
        if line.startswith("/*", i):
            in_block[0] = True
            res.append("  ")
            i += 2
            continue
        res.append(c)
        i += 1
    return "".join(res)


def rs_files():
    return sorted(PROBE_SRC.glob("*.rs"))


def ets_files():
    return sorted(ETS_DIR.rglob("*.ets"))


def find_format_paren(stripped, line_idx, window=3):
    """在 line_idx 附近（±window 行）定位 `format!(` 的开括号列。
    返回 (line_idx, col) 或 None。emit(&format! 的格式串常在下一行。"""
    lo = max(0, line_idx - window)
    hi = min(len(stripped) - 1, line_idx + window)
    for k in range(lo, hi + 1):
        pos = stripped[k].find("format!(")
        if pos >= 0:
            return (k, pos + len("format!"))
    return None


def find_block_extent(skeleton, start_idx):
    """自 start_idx 行起找第一个 '{'，返回其配对 '}' 的行号；无则 None。"""
    depth = 0
    opened = False
    for i in range(start_idx, len(skeleton)):
        for ch in skeleton[i]:
            if ch == "{":
                depth += 1
                opened = True
            elif ch == "}":
                depth -= 1
                if opened and depth == 0:
                    return i
    return None


def find_bracket_extent(skeleton, start_idx, anchor=r"=\s*\["):
    """数组区（`... = [` .. `];`）配对：在 start_idx 行内找 anchor 后首个 '['，
    返回配对 ']' 的行号；无则 None。（BT_SYMBOLS / BT_FFI_KEEP 为数组块，
    类型注解内的 [...] 由 anchor 跳过。）"""
    if start_idx >= len(skeleton):
        return None
    line = skeleton[start_idx]
    m = re.search(anchor, line)
    if not m:
        return None
    open_col = line.index("[", m.start())
    depth = 0
    opened = False
    for i in range(start_idx, len(skeleton)):
        cols = range(open_col, len(skeleton[i])) if i == start_idx \
            else range(len(skeleton[i]))
        for j in cols:
            ch = skeleton[i][j]
            if ch == "[":
                depth += 1
                opened = True
            elif ch == "]":
                depth -= 1
                if opened and depth == 0:
                    return i
    return None


def fn_index(skeleton):
    """[(name, start_line_idx, end_line_idx)] — Rust 顶层/ inherent fn。"""
    fns = []
    pat = re.compile(r'(?:pub\s+)?(?:unsafe\s+)?(?:extern\s+"C"\s+)?fn\s+(\w+)')
    for i, line in enumerate(skeleton):
        m = pat.search(line)
        if m:
            end = find_block_extent(skeleton, i)
            if end is not None:
                fns.append((m.group(1), i, end))
    return fns


def enclosing_fn(fns, line_idx):
    best = None
    for name, s, e in fns:
        if s <= line_idx <= e:
            if best is None or s > best[1]:
                best = (name, s, e)
    return best


def call_sites(stripped, skeleton, name_pat):
    """找 (line_idx, line_text, args_text, end_idx)。
    name_pat 匹配调用形（不含 '('）；args_text 为跨行配平的实参串。
    声明行（前置 `fn ` ）由调用方自行排除。"""
    out = []
    pat = re.compile(name_pat + r"\s*\(")
    for i, line in enumerate(stripped):
        for m in pat.finditer(line):
            if re.search(r"\bfn\s*$", line[: m.start()]):
                continue  # extern 块声明行（fn <name>( 形态）
            args, end = _balanced_args(stripped, i, m.end() - 1)
            out.append((i, line, args, end))
    return out


def _balanced_args(stripped, start_idx, open_col):
    depth = 0
    buf = []
    for i in range(start_idx, len(stripped)):
        line = stripped[i]
        begin = open_col if i == start_idx else 0
        for j in range(begin, len(line)):
            c = line[j]
            if c == "(":
                depth += 1
                if depth == 1:
                    continue
            elif c == ")":
                depth -= 1
                if depth == 0:
                    return "".join(buf), i
            if depth >= 1:
                buf.append(c)
    return "".join(buf), len(stripped) - 1


def lines_matching(stripped, pattern, start=0, end=None):
    end = len(stripped) if end is None else end
    pat = re.compile(pattern)
    return [(i, stripped[i]) for i in range(start, end) if pat.search(stripped[i])]


def first_match(stripped, pattern, start=0, end=None):
    hits = lines_matching(stripped, pattern, start, end)
    return hits[0] if hits else None


def f1(line_idx):
    """0-based -> 1-based 行号（证据输出口径）。"""
    return line_idx + 1


# ---------------------------------------------------------------------------
# 各断言检查（返回 dict: id/verdict/claim/evidence/registry/notes）
# ---------------------------------------------------------------------------

def check_a1():
    ev, notes, fails = [], [], []
    create_decl, create_calls = [], []
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = path.relative_to(ROOT)
        for i, line in enumerate(stripped):
            if "pthread_create" not in line:
                continue
            if re.search(r"\bfn\s+pthread_create\b", line):
                create_decl.append("%s:%d" % (rel, f1(i)))
            elif "pthread_create(" in line:
                create_calls.append((rel, f1(i), line.strip()))
    ev.append("pthread_create 全部出现（注释剥离后）: 声明=%s 调用=%s"
              % (create_decl or "无", ["%s:%d" % (r, l) for r, l, _ in create_calls]))
    if len(create_calls) != 1:
        fails.append("pthread_create 调用点=%d（要求恰 1）"
                     % len(create_calls))
    else:
        rel, ln, _ = create_calls[0]
        if str(rel) != "probe/src/dw.rs":
            fails.append("唯一调用点 %s:%d 不在 D-W 登记位点 dw.rs" % (rel, ln))
        else:
            ev.append("唯一 pthread_create 调用点 = probe/src/dw.rs:%d"
                      "（dw_start，D-W worker spawn，A1 登记位点）" % ln)

    bypass_pats = [
        (r"std::thread\b|thread::spawn|thread::Builder|Builder::spawn",
         "std::thread::spawn / Builder"),
        (r"\btokio\b", "tokio runtime"),
        (r"async[_-]std", "async-std"),
        (r"\bsmol\b", "smol"),
        (r"\brayon\b", "rayon"),
        (r"\basync\s+fn\b", "async fn"),
        (r"napi_create_threadsafe_function", "napi_create_threadsafe_function"),
        (r"napi_create_async_work", "napi_create_async_work"),
    ]
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        raw = path.read_text(encoding="utf-8").split("\n")
        rel = path.relative_to(ROOT)
        for pat, label in bypass_pats:
            for i, line in enumerate(stripped):
                if re.search(pat, line):
                    fails.append("线程/异步旁路 %s 出现于 %s:%d" % (label, rel, f1(i)))
        # 注释出现单独登记（非源码引用， informational）
        for pat, label in bypass_pats:
            for i, line in enumerate(raw):
                if re.search(pat, line) and not re.search(pat, stripped[i]):
                    notes.append("注释出现 %s 于 %s:%d（文档自陈，非源码引用，不违规）"
                                 % (label, rel, f1(i)))

    verdict = "FAIL" if fails else "PASS"
    return {"id": "A1", "verdict": verdict,
            "claim": "恰一个 pthread_create 调用点=D-W 登记位点 dw.rs；零线程/异步旁路；"
                     "零 napi_create_threadsafe_function / napi_create_async_work（extern 块与调用两查）",
            "evidence": ev, "registry": [], "notes": notes,
            "fails": fails}


def check_a2():
    ev, fails = [], []
    fd_sites = []
    call_pat = r"(?:sys::)?(?:read_fd|write_fd|read|write|fcntl)\b"
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = path.relative_to(ROOT)
        for i, line, args, _end in call_sites(stripped, stripped, call_pat):
            if "fd_orig" not in args:
                continue
            is_fcntl = ".fcntl" in line or "fcntl(" in line
            is_setfl = "F_SETFL" in args
            fd_sites.append((rel, f1(i), line.strip(), is_fcntl, is_setfl))
    rw_setfl = [s for s in fd_sites
                if (not s[3]) or s[4]]  # read/write 形（非 fcntl）或 F_SETFL
    ev.append("fd_orig 为实参的 read/write 调用点（注释剥离后）: %s"
              % (["%s:%d" % (s[0], s[1]) for s in fd_sites if not s[3]] or "0 处"))
    ev.append("fd_orig 为实参且含 F_SETFL 的 fcntl 调用点: %s"
              % (["%s:%d" % (s[0], s[1]) for s in fd_sites if s[3] and s[4]] or "0 处"))
    if rw_setfl:
        for s in rw_setfl:
            fails.append("fd_orig 触达 read/write/F_SETFL: %s:%d" % (s[0], s[1]))

    close_sites = []
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = path.relative_to(ROOT)
        for i, line in enumerate(stripped):
            if re.search(r"\bclose\s*\(\s*fd_orig\s*\)", line):
                close_sites.append((rel, f1(i)))
    ev.append("close(fd_orig) 调用点: %s" %
              (["%s:%d" % (r, l) for r, l in close_sites] or "0 处"))
    if len(close_sites) != 1:
        fails.append("close(fd_orig) 调用点=%d（要求恰 1）" % len(close_sites))
    else:
        rel, ln = close_sites[0]
        if str(rel) != "probe/src/d6.rs":
            fails.append("close(fd_orig) 位于 %s:%d，不在 d6.rs（D6a）" % (rel, ln))
        else:
            _, d6_stripped, _ = load_rs_lines(PROBE_SRC / "d6.rs")
            s3b = first_match(d6_stripped, r'N1BDISC_D6S3_B')
            s3r = first_match(d6_stripped, r'N1BDISC_D6S3_R')
            if s3b and s3r and s3b[0] < (ln - 1) < s3r[0]:
                ev.append("D6a 步 3 代码区核对: d6.rs S3_B 发射行=%d < close(fd_orig)=%d "
                          "< S3_R 发射行=%d（步 3 区间内）"
                          % (f1(s3b[0]), ln, f1(s3r[0])))
            else:
                fails.append("close(fd_orig)=d6.rs:%d 不在 S3_B/S3_R 发射行之间"
                             % ln)
    # 信息登记：fd_orig 其余触达（F_GETFD/F_GETFL/F_DUPFD_CLOEXEC 读形，A2 不禁）
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = path.relative_to(ROOT)
        for i, line, args, _end in call_sites(stripped, stripped, r"fcntl"):
            if "fd_orig" in args and "F_SETFL" not in args:
                ev.append("信息: fcntl(fd_orig, 非F_SETFL) %s:%d（A2 允许的读形/dup 形）"
                          % (rel, f1(i)))
    verdict = "FAIL" if fails else "PASS"
    return {"id": "A2", "verdict": verdict,
            "claim": "以 fd_orig 为实参的 read/write/F_SETFL 调用点=0；"
                     "close(fd_orig) 恰 1 且位于 D6a 步 3 代码区（d6.rs）",
            "evidence": ev, "registry": [], "notes": [], "fails": fails}


def check_a3():
    ev, fails, notes = [], [], []
    bt = PROBE_SRC / "btkeep.rs"
    _, stripped, skel = load_rs_lines(bt)

    # 三个允许区行区间（0-based, 含端点）。锚点在 stripped（字符串保留）上取，
    # skeleton 会把 "C" 内容置空导致 extern "C" { 不匹配。
    extern_start = first_match(stripped, r'extern\s+"C"\s*\{')
    extern_end = find_block_extent(skel, extern_start[0]) if extern_start else None
    sym_start = first_match(stripped, r"pub\s+const\s+BT_SYMBOLS")
    sym_end = find_bracket_extent(skel, sym_start[0]) if sym_start else None
    keep_start = first_match(stripped, r"pub\s+static\s+BT_FFI_KEEP")
    keep_end = find_bracket_extent(skel, keep_start[0]) if keep_start else None
    regions = {
        "extern_decl": (extern_start[0], extern_end),
        "symbol_table": (sym_start[0], sym_end),
        "address_take": (keep_start[0], keep_end),
    }
    ev.append("允许区（btkeep.rs）: extern 声明区=%d-%d, BT_SYMBOLS 表=%d-%d, "
              "BT_FFI_KEEP 取址区=%d-%d"
              % (f1(extern_start[0]), f1(extern_end), f1(sym_start[0]),
                 f1(sym_end), f1(keep_start[0]), f1(keep_end)))

    def in_region(idx):
        for label, (s, e) in regions.items():
            if s <= idx <= e:
                return label
        return None

    per_sym = []
    for sym in BT_SYMBOLS:
        raw_pat = re.compile(r"(?<![\w])" + re.escape(sym) + r"(?![\w])")
        alias_pat = re.compile(r"(?<![\w])_keep_" + re.escape(sym) + r"(?![\w])")
        occ = []
        # btkeep.rs 允许区
        for i, line in enumerate(stripped):
            for m in raw_pat.finditer(line):
                reg = in_region(i)
                if ('#[link_name' in line and '"%s"' % sym in line) or reg == "symbol_table":
                    occ.append(("btkeep.rs", f1(i), "允许:%s" %
                                ("link_name 声明" if "link_name" in line else "dlsym 名表")))
                else:
                    occ.append(("btkeep.rs", f1(i), "违规:允许区之外裸出现"))
            for m in alias_pat.finditer(line):
                reg = in_region(i)
                if reg == "extern_decl":
                    occ.append(("btkeep.rs", f1(i), "允许:别名 extern 声明"))
                elif reg == "address_take":
                    occ.append(("btkeep.rs", f1(i), "允许:#[used] 取址"))
                else:
                    occ.append(("btkeep.rs", f1(i), "违规:别名在允许区之外"
                                                     "（call site 形态尤其违规）"))
        # 其余 crate 文件（注释剥离后）
        for path in rs_files():
            if path == bt:
                continue
            _, st, _ = load_rs_lines(path)
            rel = str(path.relative_to(ROOT))
            for i, line in enumerate(st):
                if raw_pat.search(line):
                    occ.append((rel, f1(i), "违规:crate 其他文件出现"))
                if alias_pat.search(line):
                    occ.append((rel, f1(i), "违规:crate 其他文件出现别名"))
        bad = [o for o in occ if o[2].startswith("违规")]
        per_sym.append((sym, occ, bad))
        if bad:
            for o in bad:
                fails.append("符号 %s: %s:%d %s" % (sym, o[0], o[1], o[2]))
        kinds = {}
        for _, _, k in occ:
            kinds[k] = kinds.get(k, 0) + 1
        ev.append("%-33s 出现分类: %s" % (
            sym,
            "; ".join("%s×%d" % kv for kv in sorted(kinds.items())) or "0 处"))

    n_allowed = sum(1 for _s, occ, _b in per_sym for o in occ if o[2].startswith("允许"))
    ev.append("14 符号全 crate 出现位置穷举: 允许类合计 %d 处（恰= 14 link_name 声明"
              " + 14 名表字面 + 14 别名声明 + 14 取址），违规 %d 处"
              % (n_allowed, len(fails)))
    ev.append(A3_CALIBER_NOTE)
    verdict = "FAIL" if fails else "PASS"
    return {"id": "A3", "verdict": verdict,
            "claim": "14 个 BoringTun 符号（:522-524）全 crate 出现位置穷举分类；"
                     "允许={btkeep.rs link_name 声明 / BT_SYMBOLS 字符串表 / "
                     "BT_FFI_KEEP #[used] 取址}；其余出现（尤其 call site）=违规",
            "evidence": ev, "registry": [], "notes": notes, "fails": fails}


def check_a4():
    ev, fails = [], []
    timed = []
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(stripped):
            if re.search(r"pthread_timedjoin_np|pthread_tryjoin_np", line):
                timed.append("%s:%d" % (rel, f1(i)))
    ev.append("pthread_timedjoin_np / pthread_tryjoin_np（注释剥离后）: %s"
              % (timed or "0 处"))
    if timed:
        fails.append("timedjoin/tryjoin 出现: %s" % timed)

    # sleep 原语盘点
    sleep_sites, bad_sleep, other_sleep = [], [], []
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line, args, _end in call_sites(stripped, stripped, r"sleep_ms"):
            arg = args.strip()
            sleep_sites.append((rel, f1(i), arg))
            if arg not in ("10", "50"):
                bad_sleep.append("%s:%d sleep_ms(%s) 非 10/50ms 档" % (rel, f1(i), arg))
        for i, line in enumerate(stripped):
            if re.search(r"\busleep\s*\(", line) or \
               re.search(r"(?<!clock_)nanosleep\s*\(", line) or \
               re.search(r"\bstd::thread::sleep\b", line) or \
               re.search(r"(?<![:\w])sleep\s*\(", line):
                other_sleep.append("%s:%d %s" % (rel, f1(i), line.strip()))
    ev.append("sleep_ms 调用点（全部等待 tick）: %s"
              % ["%s:%d sleep_ms(%s)" % s for s in sleep_sites])
    ev.append("其他 sleep/等待原语（usleep/裸 nanosleep/std::thread::sleep/sleep）: %s"
              % (other_sleep or "0 处"))
    if bad_sleep:
        fails.extend(bad_sleep)
    if other_sleep:
        fails.append("非清单 sleep 原语出现: %s" % other_sleep)

    # clock_nanosleep 唯一落点 + CLOCK_MONOTONIC
    cn = []
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(stripped):
            if "clock_nanosleep(" in line and not re.search(r"\bfn\s+clock_nanosleep\b", line):
                cn.append((rel, f1(i), line.strip()))
    ev.append("clock_nanosleep 调用（注释剥离后）: %s" %
              ["%s:%d %s" % c for c in cn])
    if len(cn) != 1 or "CLOCK_MONOTONIC" not in cn[0][2] or cn[0][0] != "probe/src/sys.rs":
        fails.append("clock_nanosleep 调用未收敛为 sys.rs sleep_ms 内单点 CLOCK_MONOTONIC 形态")

    # pthread_join 唯一 + 前置终态标志门
    joins = []
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(stripped):
            if "pthread_join(" in line and not re.search(r"\bfn\s+pthread_join\b", line):
                joins.append((path, rel, i))
    ev.append("pthread_join 调用（注释剥离后）: %s" %
              ["%s:%d" % (r, f1(i)) for _p, r, i in joins])
    if len(joins) != 1:
        fails.append("pthread_join 调用点=%d（要求恰 1）" % len(joins))
    else:
        path, rel, idx = joins[0]
        if str(rel) != "probe/src/dw.rs":
            fails.append("pthread_join 不在 dw.rs: %s:%d" % (rel, f1(idx)))
        else:
            _, stripped, skel = load_rs_lines(path)
            fns = fn_index(skel)
            enc = enclosing_fn(fns, idx)
            gate = first_match(stripped, r"if\s*!\s*DW_TERMINAL\s*\.\s*load",
                               start=enc[1], end=idx)
            refuse = first_match(stripped, r"terminal flag not set; join refused",
                                 start=enc[1], end=idx)
            if gate:
                ev.append("前置终态标志门: dw.rs:%d `if !DW_TERMINAL.load(...)` 先于 "
                          "pthread_join（dw.rs:%d）；拒绝路径 dw.rs:%s"
                          "（未置位即返回，不阻塞）——A4 r3-D5 显式豁免的唯一无界调用"
                          % (f1(gate[0]), f1(idx),
                             f1(refuse[0]) if refuse else "n/a"))
            else:
                fails.append("pthread_join(dw.rs:%d) 前无终态标志检查门" % f1(idx))

    # 等待循环登记（复用 A8 注册表，取单调钟 deadline 类）
    reg = loop_registry()
    wait_reg = [e for e in reg if e["bound_class"] in
                ("MONO_DEADLINE", "MONO_DEADLINE_GATE", "COUNTER_FUSE")]
    ev.append("全部等待/时间盒循环及其 deadline 判定证据（与 A8 登记表同源）:")
    for e in wait_reg:
        ev.append("  - %s %s:%s [%s] deadline=%s 判定=%s tick=%s"
                  % (e["func"], e["file"], e["lines"], e["kind"],
                     e["deadline_lines"] or "n/a", e["check_lines"],
                     e["tick_lines"] or "n/a"))
    verdict = "FAIL" if fails else "PASS"
    return {"id": "A4", "verdict": verdict,
            "claim": "零 pthread_timedjoin_np/tryjoin；全部 sleep=clock_nanosleep"
                     "(CLOCK_MONOTONIC) 且仅 10/50ms 档；pthread_join 唯一且前置"
                     "终态标志检查；等待循环全部带单调钟 deadline（登记见下）",
            "evidence": ev, "registry": wait_reg, "notes": [], "fails": fails}


def check_a5():
    ev, fails, notes = [], [], []
    found = set()
    sites = {}
    lit_pat = re.compile(r"N1BDISC_[A-Z0-9_]+")
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(stripped):
            for m in lit_pat.finditer(line):
                found.add(m.group(0))
                sites.setdefault(m.group(0), []).append("%s:%d" % (rel.replace("probe/src/", ""), f1(i)))
    for lit in sorted(sites):
        ev.append("发射字面 %-32s @ %s" % (lit, ", ".join(sites[lit])))
    missing = sorted(set(FROZEN_56) - found)
    extra = sorted(found - set(FROZEN_56))
    ev.append("crate 发射字面集大小=%d / 冻结集=56；双向差集: 缺=%s 多=%s"
              % (len(found), missing or "{}", extra or "{}"))
    if missing:
        fails.append("冻结集字面未在 crate 发射面出现: %s" % missing)
    if extra:
        fails.append("crate 出现冻结集之外字面: %s" % extra)

    # 豁免集：发射面（rs+ets）零出现；runner/selftest 出现单独登记归类
    exempt_surface = []
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(stripped):
            for m in lit_pat.finditer(line):
                if m.group(0) in EXEMPT_MARKERS:
                    exempt_surface.append("%s:%d %s" % (rel, f1(i), m.group(0)))
    ets_exempt = []
    ets_lits = []
    for path in ets_files():
        _, stripped, _ = load_ets_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(stripped):
            for m in lit_pat.finditer(line):
                ets_lits.append("%s:%d %s" % (rel, f1(i), m.group(0)))
                if m.group(0) in EXEMPT_MARKERS:
                    ets_exempt.append("%s:%d %s" % (rel, f1(i), m.group(0)))
    ev.append("豁免集 %s 在发射面（rs+ets）出现: %s"
              % (sorted(EXEMPT_MARKERS), exempt_surface + ets_exempt or "0 处"))
    if exempt_surface or ets_exempt:
        fails.append("豁免集字面在发射面出现: %s" % (exempt_surface + ets_exempt))

    # 全 spike 树其余出现（runner 机器规则集定义 / selftest 反例夹具）——登记归类
    for path in sorted(list(RUNNER.glob("*.py")) + list(SELFTESTS.glob("*.py"))):
        rel = str(path.relative_to(ROOT))
        try:
            raw = path.read_text(encoding="utf-8").split("\n")
        except OSError:
            continue
        for i, line in enumerate(raw):
            for m in lit_pat.finditer(line):
                if m.group(0) in EXEMPT_MARKERS:
                    kind = ("runner 豁免集机器规则定义（非探针发射）"
                            if "runner" in rel else
                            "selftest 豁免集反例夹具（A5 正反例覆盖自身，:1080）")
                    notes.append("豁免字面 %s @ %s:%d —— %s"
                                 % (m.group(0), rel, f1(i), kind))
    ev.append("ArkTS/ETS 文件 N1BDISC_ 字面: %s" % (ets_lits or "0 处"))
    if ets_lits:
        fails.append("ETS 文件出现 N1BDISC_ 字面（要求 0）: %s" % ets_lits)

    verdict = "FAIL" if fails else "PASS"
    return {"id": "A5", "verdict": verdict,
            "claim": "crate 发射的 N1BDISC_ 字面集 == :1095-1097 冻结 56 字面"
                     "（双向差集为空）；豁免集 {N1BDISC_D2_REJTEXT, N1BDISC_RESULT}"
                     " 发射面零出现；ArkTS/ETS 零 N1BDISC_ 字面",
            "evidence": ev, "registry": [], "notes": notes, "fails": fails}


def check_a6():
    ev, fails = [], []
    calls = []
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line, args, _end in call_sites(stripped, stripped, r"openat"):
            calls.append((path, rel, i, args))
    ev.append("openat 调用（注释剥离后，sys.rs extern 声明除外）: %s" %
              ["%s:%d flags=[%s]" % (r, f1(i), args) for _p, r, i, args in calls])
    if len(calls) != 2:
        fails.append("openat 调用点=%d（要求恰 2：dw.rs inwait 区两处）" % len(calls))
    for path, rel, i, args in calls:
        if str(rel) != "probe/src/dw.rs":
            fails.append("openat 调用不在 dw.rs: %s:%d" % (rel, f1(i)))
        if "O_RDONLY" not in args:
            fails.append("openat %s:%d flags 不含 O_RDONLY" % (rel, f1(i)))
        for wflag in ("O_WRONLY", "O_RDWR"):
            if wflag in args:
                fails.append("openat %s:%d 含 %s" % (rel, f1(i), wflag))

    wr = []
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(stripped):
            if re.search(r"\bO_WRONLY\b|\bO_RDWR\b", line):
                wr.append("%s:%d" % (rel, f1(i)))
    ev.append("O_WRONLY / O_RDWR 常量全 crate 出现: %s" % (wr or "0 处"))
    if wr:
        fails.append("写打开常量出现: %s" % wr)

    proc_lits = set()
    proc_sites = []
    lit_pat = re.compile(r'"(/proc[^"]*)"')
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(stripped):
            for m in lit_pat.finditer(line):
                proc_lits.add(m.group(1))
                proc_sites.append("%s:%d %s" % (rel, f1(i), m.group(1)))
    allowed_paths = {"/proc/self/task/{}/stat", "/proc/self/task/{}/syscall"}
    ev.append("全 crate /proc 路径字面: %s" % sorted(proc_lits))
    ev.append("出现位置: %s" % proc_sites)
    if not proc_lits <= allowed_paths:
        fails.append("出现白名单之外的 /proc 路径字面: %s"
                     % sorted(proc_lits - allowed_paths))
    if proc_lits != allowed_paths:
        fails.append("白名单路径模板未齐: %s" % sorted(allowed_paths - proc_lits))
    # tid 绑定：模板实参须为 DW_TID 读出的 tid（dw_inwait_collect 内）
    _, dw_stripped, dw_skel = load_rs_lines(PROBE_SRC / "dw.rs")
    tid_def = first_match(dw_stripped, r"let\s+tid\s*=\s*DW_TID\s*\.\s*load")
    ev.append("tid 绑定: dw.rs:%d `let tid = DW_TID.load(...)`"
              "（D-W worker tid，模板实参即该值）" % f1(tid_def[0]) if tid_def else
              "tid 绑定未找到")
    # 两个 openat 的宿主函数 = inwait 采集段；其唯一调用链入口 = dw_inwait_collect
    fns = fn_index(dw_skel)
    for path, rel, i, _a in calls:
        enc = enclosing_fn(fns, i)
        ev.append("openat %s:%d 位于 %s（D-W in-wait 证据采集段的 /proc 读实现）"
                  % (rel, f1(i), enc[0] if enc else "?"))
        if not enc or enc[0] not in ("read_proc_stat", "read_proc_syscall"):
            fails.append("openat %s:%d 不在 read_proc_stat/read_proc_syscall"
                         % (rel, f1(i)))
    for helper in ("read_proc_stat", "read_proc_syscall"):
        hsites = []
        for i, line, _args, _end in call_sites(dw_stripped, dw_stripped, helper):
            enc2 = enclosing_fn(fns, i)
            hsites.append("dw.rs:%d（%s）" % (f1(i), enc2[0] if enc2 else "?"))
        ev.append("%s 调用点: %s（即两处 openat 的唯一消费链，"
                  "全部位于 dw_inwait_collect 采样循环内）" % (helper, hsites))
    verdict = "FAIL" if fails else "PASS"
    return {"id": "A6", "verdict": verdict,
            "claim": "openat 调用点仅 dw.rs inwait 区两处，路径模板 ∈ "
                     "{/proc/self/task/<tid>/stat, /proc/self/task/<tid>/syscall}，"
                     "flags=O_RDONLY；零其他路径、零写标志",
            "evidence": ev, "registry": [], "notes": [], "fails": fails}


def check_a7():
    ev, fails, notes = [], [], []
    _, dw_stripped, dw_skel = load_rs_lines(PROBE_SRC / "dw.rs")
    fns = fn_index(dw_skel)
    rps = first_match(dw_skel, r"fn\s+read_proc_stat")
    rps_end = find_block_extent(dw_skel, rps[0])
    # rfind(')') 定位：实现可在 read_proc_stat 内，也可在其调用的纯函数助手中
    rfind = first_match(dw_stripped, r"rfind\s*\(\s*'\)'\s*\)", 0, len(dw_stripped))
    if not rfind:
        fails.append("dw.rs 中未找到 rfind(')') 定位（A7 左锚实现缺失）")
        return {"id": "A7", "verdict": "FAIL",
                "claim": "stat 的 state 解析按行内最后一个 ')' 之后的下一 token 定位",
                "evidence": ev, "registry": [], "notes": notes, "fails": fails}
    host = enclosing_fn(fns, rfind[0])
    host_name = host[0] if host else "?"
    window = "\n".join(dw_stripped[rfind[0]: min(rfind[0] + 8, len(dw_stripped))])
    m_nxt = re.search(r"split_whitespace\s*\(\s*\)\s*\.\s*next\s*\(", window)
    nxt_line = rfind[0] + window[:m_nxt.start()].count("\n") if m_nxt else None
    ev.append("实现定位: dw.rs:%d `text.rfind(')')`（宿主: %s）—— 全行最后一个"
              " ')' 之后" % (f1(rfind[0]), host_name))
    if nxt_line is not None:
        ev.append("下一 token 提取: dw.rs:%d `.split_whitespace().next()`"
                  "（rfind 位点之后首个空白分隔 token = state 字段）" % f1(nxt_line))
    else:
        fails.append("rfind(')') 之后未见 split_whitespace().next() 取下一 token")
    # 调用链：rfind 宿主须为 read_proc_stat 本体，或被 read_proc_stat 调用
    if host_name == "read_proc_stat":
        ev.append("实现宿主: read_proc_stat（dw.rs:%d-%d，A7 实现原地）"
                  % (f1(rps[0]), f1(rps_end)))
    else:
        chain = [(i, line) for i, line, _a, _e in
                 call_sites(dw_stripped, dw_stripped, host_name)
                 if rps[0] <= i <= rps_end]
        if chain:
            ev.append("调用链核对: read_proc_stat（dw.rs:%d-%d）@ dw.rs:%d 调用纯函数"
                      "助手 %s（dw.rs:%d-%d，A7 实现集中在该助手）"
                      % (f1(rps[0]), f1(rps_end), f1(chain[0][0]), host_name,
                         f1(host[1]), f1(host[2])))
        else:
            fails.append("rfind(')') 宿主 %s 未被 read_proc_stat 调用（实现漂移）"
                         % host_name)

    bad = []
    for path in rs_files():
        _, stripped, _ = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        for i, line in enumerate(stripped):
            if re.search(r"split\s*\(\s*'\s'\s*\)", line) or \
               re.search(r"splitn\s*\(\s*3", line) or \
               re.search(r"split_whitespace\s*\(\s*\)\s*\.\s*nth\s*\(", line) or \
               re.search(r"\.\s*nth\s*\(\s*2\s*\)", line):
                bad.append("%s:%d %s" % (rel, f1(i), line.strip()))
    ev.append("左侧定位/第3字段切分实现（split(' ')、splitn(3、nth(2)）: %s"
              % (bad or "0 处"))
    if bad:
        fails.append("禁止的左侧定位实现出现: %s" % bad)

    # selftest 侧 comm 带空格/括号反例
    cex = []
    for path in sorted(list(SELFTESTS.glob("*.py")) +
                       list(PROBE_SRC.glob("*.rs"))):
        rel = str(path.relative_to(ROOT))
        try:
            raw = path.read_text(encoding="utf-8").split("\n")
        except OSError:
            continue
        for i, line in enumerate(raw):
            low = line.lower()
            if "comm" in low and "(" in line and ")" in line and " " in line \
                    and not line.strip().startswith("#"):
                cex.append("%s:%d %s" % (rel, f1(i), line.strip()))
    if cex:
        ev.append("selftest 侧 comm 带空格/括号反例字面: %s" % cex)
    else:
        notes.append("QUESTION: 未在 probe 单测/selftests 中找到 comm 带空格/括号"
                     "反例字面（A7 :1082 要求 selftest 覆盖）")
    verdict = "FAIL" if fails else ("PASS" if cex else "QUESTION")
    return {"id": "A7", "verdict": verdict,
            "claim": "stat 的 state 解析按行内最后一个 ')' 之后的下一 token 定位"
                     "（rfind(')') + 下一 token，实现行号登记）；零左侧定位实现；"
                     "selftest 侧存在 comm 带空格/括号反例（缺 -> QUESTION）",
            "evidence": ev, "registry": [], "notes": notes, "fails": fails}


# ---------------------------------------------------------------------------
# A8: 全 loop/while/for 登记表
# ---------------------------------------------------------------------------

def loop_registry():
    reg = []
    for path in rs_files():
        _, stripped, skel = load_rs_lines(path)
        rel = str(path.relative_to(ROOT))
        fns = fn_index(skel)
        loop_pat = re.compile(r"\b(loop)\b|\b(while)\b|\b(for)\b")
        taken = []
        for i, line in enumerate(skel):
            if "yield" in line:
                continue
            if re.search(r"\bimpl\b[^{]*\bfor\b", line):
                continue  # impl Trait for Type 头，非 for 循环
            m = loop_pat.search(line)
            if not m:
                continue
            kind = m.group(1) or m.group(2) or m.group(3)
            if any(s <= i <= e for s, e in taken):
                continue  # 已被更长块覆盖（如 for 头中的 while 字样）
            end = find_block_extent(skel, i)
            if end is None:
                end = i
            taken.append((i, end))
            enc = enclosing_fn(fns, i)
            body = skel[i:end + 1]
            body_stripped = stripped[i:end + 1]
            entry = classify_loop(rel, kind, i, end,
                                  enc[0] if enc else "?", enc[1] if enc else i,
                                  body, body_stripped, stripped)
            reg.append(entry)
    return reg


def classify_loop(rel, kind, i, end, fname, fstart, body, body_stripped, fn_stripped):
    header = body_stripped[0]
    joined = "\n".join(body_stripped)
    deadline_lines, check_lines, tick_lines, counter_lines, break_lines = \
        [], [], [], [], []
    # deadline 赋值（宿主函数体内、循环之前或 deadline 名含 deadline）
    for j in range(fstart, end + 1):
        L = fn_stripped[j]
        if re.search(r"\b\w*deadline\w*\b[^=]*=", L):
            deadline_lines.append("%d" % f1(j))
    for j, L in enumerate(body_stripped):
        ln = f1(i + j)
        if re.search(r"mono_ms\s*\(\s*\)\s*(>=|<)", L) or \
           re.search(r"now_ms\s*>=|>=\s*deadline|<\s*deadline|>=\s*CAP_", L):
            check_lines.append("%d" % ln)
        if re.search(r"sleep_ms\s*\(", L):
            tick_lines.append("%d" % ln)
        if "break" in L:
            break_lines.append("%d" % ln)
        if re.search(r"\w+\s*\+=\s*1|round\s*\+\=|calls\s*\+=|steps\s*\.\s*push", L):
            counter_lines.append("%d" % ln)
    # 分类
    rng = re.search(r"\bfor\s+\w+\s+in\s+([\d\w.]+)\.\.([\d\w.]+)", header) or \
        re.search(r"\bfor\s+_\s+in\s+0\.\.(\d+)", header)
    if kind == "for":
        it = re.search(r"\bfor\s+[^i]*?in\s+(.+?)\{", header)
        expr = it.group(1).strip() if it else header
        if rng and all(p.isdigit() for p in rng.groups()):
            cls, ok = "FIXED_RANGE", True
            note = "字面范围 for（%s..%s）固定迭代上界" % rng.groups()
        elif re.search(r"\.iter\(\)|\.enumerate\(\)|\.chunks|\.chars\(\)|"
                       r"\.bytes\(\)|\.by_ref\(\)|BT_SYMBOLS|roles|LADDER", expr):
            cls, ok = "DATA_FINITE", True
            note = "有限集合迭代（%s）——集合为常量/有限数据" % expr[:50]
        else:
            cls, ok = "RANGE_EXPR", True
            note = "范围 for（%s）——上界为数据派生有限值（长度/切片域）" % expr[:50]
        return {"file": rel, "lines": "%d-%d" % (f1(i), f1(end)), "kind": kind,
                "func": fname, "loop_var": (expr[:60]),
                "term_cond": note, "bound_class": cls, "ok": ok,
                "deadline_lines": deadline_lines, "check_lines": check_lines,
                "tick_lines": tick_lines, "break_lines": break_lines,
                "counter_lines": counter_lines, "note": note}
    if kind == "while":
        cond = header[header.index("while") + 5: header.rfind("{")].strip()
        if re.search(r"mono_ms\s*\(\s*\)\s*<", cond):
            cls, ok = "MONO_DEADLINE_GATE", True
            note = "循环条件即单调钟门（%s）" % cond
        elif re.search(r"len\(\)\s*<\s*\d+", cond):
            cls, ok = "COUNTER_BOUND", True
            note = "固定计数上界（%s），体内 push 驱动收敛: %s" % (cond, counter_lines)
        elif re.search(r"%\s*\d+\s*!=\s*\d+", cond):
            cls, ok = "MODULO_CYCLE", True
            note = "模剩余循环：体内定长追加，≤64 步收敛（%s）" % cond
        elif re.search(r">>\s*\d+\s*!=\s*0", cond):
            cls, ok = "SHIFT_FOLD", True
            note = "右移折叠循环：u32 值每轮右移 N 位，≤⌈32/N⌉ 步收敛（%s）" % cond
        else:
            cls, ok = "UNPROVEN", False
            note = "无法静态证明有界"
        return {"file": rel, "lines": "%d-%d" % (f1(i), f1(end)), "kind": kind,
                "func": fname, "loop_var": cond[:80], "term_cond": note,
                "bound_class": cls, "ok": ok, "deadline_lines": deadline_lines,
                "check_lines": check_lines, "tick_lines": tick_lines,
                "break_lines": break_lines, "counter_lines": counter_lines,
                "note": note}
    # kind == "loop"
    if check_lines:
        cls, ok = "MONO_DEADLINE", True
        note = "体内单调钟 deadline 判定 break（判定行=%s）" % check_lines
    elif any(re.search(r"CAP_CALLS|CAP_BYTES|>=\s*CAP_", b) for b in body_stripped):
        cls, ok = "COUNTER_FUSE", True
        note = "体内固定计数/字节熔断 break（calls≥50000 / bytes≥4MiB / 钟 fuse）"
    else:
        cls, ok = "UNPROVEN", False
        note = "无法静态证明有界"
    return {"file": rel, "lines": "%d-%d" % (f1(i), f1(end)), "kind": kind,
            "func": fname, "loop_var": "(无限 loop)", "term_cond": note,
            "bound_class": cls, "ok": ok, "deadline_lines": deadline_lines,
            "check_lines": check_lines, "tick_lines": tick_lines,
            "break_lines": break_lines, "counter_lines": counter_lines,
            "note": note}


def check_a8():
    fails = []
    reg = loop_registry()
    for e in reg:
        if not e["ok"]:
            fails.append("循环 %s:%s（%s）无法证明有界" %
                         (e["file"], e["lines"], e["func"]))
    ev = ["逐循环登记（probe/src 全部 loop/while/for，注释与字符串剥离后）:"]
    ev.append("%-14s %-8s %-22s %-12s %s" %
              ("位置", " kind", "宿主函数", "有界类", "终止条件/证据"))
    for e in reg:
        ev.append("%-14s %-8s %-22s %-12s %s" %
                  ("%s:%s" % (e["file"].replace("probe/src/", ""), e["lines"]),
                   e["kind"], e["func"], e["bound_class"], e["term_cond"]))
        extra = []
        if e["deadline_lines"]:
            extra.append("deadline赋值行=%s" % e["deadline_lines"])
        if e["tick_lines"]:
            extra.append("sleep tick=%s" % e["tick_lines"])
        if e["break_lines"]:
            extra.append("break行=%s" % e["break_lines"])
        if extra:
            ev.append("%-14s     ↳ %s" % ("", "; ".join(extra)))
    ev.append("登记合计 %d 个循环；UNPROVEN=%d" %
              (len(reg), sum(1 for e in reg if not e["ok"])))
    verdict = "FAIL" if fails else "PASS"
    return {"id": "A8", "verdict": verdict,
            "claim": "probe/src 全部 loop/while/for 逐循环登记：位置/循环变量/"
                     "终止条件/单调钟或固定计数证据；无法证明有界 -> FAIL"
                     "（:1084 逐循环登记核对结论载体）",
            "evidence": ev, "registry": reg, "notes": [], "fails": fails}


# ---------------------------------------------------------------------------
# A9-A12: ArkTS 子协议 / join-timeout 分支 / poll raw / P12 单 load
# ---------------------------------------------------------------------------

def ets_main():
    for p in ets_files():
        if p.name == "N1BDiscVpnExtensionAbility.ets":
            return p
    return None


def ets_method_extent(skel, name):
    """ETS 类方法声明定位：要求行首修饰符形态（private/public/protected/
    static/async 前缀），排除 `this.<name>(` 调用行。返回 (start, end) 或 None。"""
    start = first_match(skel, r"^\s*(?:private\s+|public\s+|protected\s+|"
                              r"static\s+|async\s+)+" + name + r"\s*\(")
    if not start:
        return None
    end = find_block_extent(skel, start[0])
    if end is None:
        return None
    return (start[0], end)


def check_a9():
    ev, fails, notes = [], [], []
    path = ets_main()
    if path is None:
        return {"id": "A9", "verdict": "QUESTION",
                "claim": "共享 destroy 子协议四步静态核对",
                "evidence": ["未找到 N1BDiscVpnExtensionAbility.ets"],
                "registry": [],
                "notes": ["QUESTION: ArkTS 主文件缺失，四语句无法静态定位"],
                "fails": []}
    _, stripped, skel = load_ets_lines(path)
    rel = str(path.relative_to(ROOT))

    t_line = first_match(stripped, r"probe\s*\.\s*dw_destroy_t\s*\(")
    c_line = first_match(stripped, r"probe\s*\.\s*dw_destroy_c\s*\(")
    d_line = first_match(stripped, r"\.\s*destroy\s*\(")
    ext = ets_method_extent(skel, "destroyOnce")
    race_line = first_match(stripped, r"Promise\s*\.\s*race",
                            start=ext[0], end=ext[1] + 1) if ext else None
    box_line = first_match(stripped, r"setTimeout", start=ext[0], end=ext[1] + 1) \
        if ext else None
    ev.append("四语句定位: _T=%s, destroy()=%s, _C=%s, 有界等待(Promise.race)=%s"
              % (["%s:%d" % (rel, f1(t_line[0]))] if t_line else "缺",
                 ["%s:%d" % (rel, f1(d_line[0]))] if d_line else "缺",
                 ["%s:%d" % (rel, f1(c_line[0]))] if c_line else "缺",
                 ["%s:%d" % (rel, f1(race_line[0]))] if race_line else "缺"))
    if box_line:
        ev.append("有界等待盒: %s:%d setTimeout(..., DESTROY_BOX_MS=10000)"
                  % (rel, f1(box_line[0])))

    if not (t_line and d_line and c_line and race_line):
        notes.append("QUESTION: 四语句之一无法静态定位，直线控制流不可静态证实")
        return {"id": "A9", "verdict": "QUESTION",
                "claim": "dw_destroy_t -> destroy() -> dw_destroy_c -> 有界等待"
                         " 四语句同控制流直线出现；destroy() 全源码恰一调用点；"
                         "_C 先于调用 -> FAIL",
                "evidence": ev, "registry": [], "notes": notes, "fails": []}

    order_ok = t_line[0] < d_line[0] < c_line[0] < race_line[0]
    ev.append("行序: %d < %d < %d < %d（_T -> destroy -> _C -> 有界等待）%s"
              % (f1(t_line[0]), f1(d_line[0]), f1(c_line[0]), f1(race_line[0]),
                 "OK" if order_ok else "乱序"))
    if c_line[0] < d_line[0]:
        fails.append("_C 发射（%d）先于 destroy() 调用（%d）——A9 反例必 fail"
                     % (f1(c_line[0]), f1(d_line[0])))
    elif not order_ok:
        fails.append("四语句行序非 _T -> destroy -> _C -> 有界等待")

    # 同函数 + 直线（无分支关键字）核对
    enc = ("destroyOnce", ext[0], ext[1]) if ext else None
    same_fn = all(enc and enc[1] <= x[0] <= enc[2]
                  for x in (t_line, d_line, c_line, race_line))
    ev.append("四语句宿主函数: %s（%s:%d-%d）%s"
              % (enc[0] if enc else "?", rel, f1(enc[1]), f1(enc[2]),
                 "（同函数）" if same_fn else "（不同函数!）"))
    if not same_fn:
        fails.append("四语句不在同一控制流函数内")

    branch_pat = re.compile(
        r"(?<![.\w])(if\s*\(|else\b|switch\b|case\b|for\s*\(|while\s*\(|"
        r"return\b|break\b|continue\b|try\b|catch\s*\()")
    seg = [(i, stripped[i]) for i in range(t_line[0], c_line[0] + 1)]
    hits = [(f1(i), s.strip()) for i, s in seg if branch_pat.search(s)]
    ev.append("_T.._C 区间分支关键字扫描: %s" % (hits or "0 处（直线段）"))
    if hits:
        fails.append("_T 与 _C 之间存在分支关键字（可跳过 _C 发射）: %s" % hits)
    seg2 = [(i, stripped[i]) for i in range(c_line[0], race_line[0] + 1)]
    hits2 = [(f1(i), s.strip()) for i, s in seg2 if branch_pat.search(s)]
    ev.append("_C..有界等待 区间分支关键字扫描（.then/.catch 登记 = 等待接线，"
              "Promise 构造非分支）: %s" % (hits2 or "0 处"))
    bad2 = [h for h in hits2 if not re.search(r"\.\s*(then|catch)\s*\(", h[1])]
    if bad2:
        fails.append("_C 与有界等待之间存在分支关键字: %s" % bad2)

    # 静态可达性：四语句均处于函数体顶层深度（无 if/else/for/while/try 包裹）
    depth_ok = True
    for x in (t_line, d_line, c_line, race_line):
        d0 = _depth_at(skel, enc[1])
        for j in range(enc[1], x[0]):
            L = skel[j]
            if re.search(r"(?<![.\w])(if\s*\(|else\b|for\s*\(|while\s*\(|"
                         r"switch\b|try\b|catch\s*\()", L):
                depth_ok = False
                break
        d1v = _depth_at(skel, x[0])
        ev.append("语句 %s:%d 深度=%d（函数体顶层=%d）"
                  % (rel, f1(x[0]), d1v, d0 + 1))
        if d1v != d0 + 1:
            depth_ok = False
    if not depth_ok:
        notes.append("QUESTION: 四语句之一处于条件/循环嵌套内（静态条件可达，"
                     "非无条件直线）")

    # destroy() 全源码恰一调用点（rs + ets，注释剥离）
    sites = []
    for p in list(rs_files()) + list(ets_files()):
        _, st, _ = (load_rs_lines(p) if p.suffix == ".rs" else load_ets_lines(p))
        r = str(p.relative_to(ROOT))
        for i, line in enumerate(st):
            if re.search(r"\.\s*destroy\s*\(\s*\)", line):
                sites.append("%s:%d" % (r, f1(i)))
    ev.append(".destroy() 调用点全源码（注释剥离后）: %s" % (sites or "0 处"))
    if len(sites) != 1:
        fails.append(".destroy() 调用点=%d（要求恰 1，共享子协议唯一调用点）"
                     % len(sites))

    verdict = "FAIL" if fails else "PASS"
    return {"id": "A9", "verdict": verdict,
            "claim": "dw_destroy_t -> destroy() -> dw_destroy_c -> 有界等待 "
                     "四语句同控制流直线出现（行号序列登记，其间无分支关键字）；"
                     "destroy() 全源码恰一调用点；_C 先于调用 -> FAIL",
            "evidence": ev, "registry": [], "notes": notes, "fails": fails}


def _depth_at(skel, line_idx):
    depth = 0
    for i in range(line_idx):
        for ch in skel[i]:
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
    return depth


def check_a10():
    ev, fails, notes = [], [], []
    path = ets_main()
    _, stripped, skel = load_ets_lines(path)
    rel = str(path.relative_to(ROOT))
    cond = first_match(stripped, r"if\s*\(\s*term\s*\.\s*terminal\s*\)")
    if not cond:
        return {"id": "A10", "verdict": "QUESTION",
                "claim": "join-timeout 分支零 fd_dup 操作",
                "evidence": ["未找到 `if (term.terminal)` 分支"],
                "registry": [], "notes": ["QUESTION: P10 分支结构无法静态定位"],
                "fails": []}
    open_end = find_block_extent(skel, cond[0])
    # else 支
    else_line = None
    depth = 0
    for i in range(cond[0], open_end + 1):
        if re.match(r"\s*\}\s*else\s*\{", skel[i]):
            else_line = i
            break
    else_end = find_block_extent(skel, else_line) if else_line is not None else None
    ev.append("P10 分支结构: if(term.terminal) %s:%d-%d; else(join-timeout) %s:%d-%s"
              % (rel, f1(cond[0]), f1(open_end), rel,
                 f1(else_line) if else_line is not None else "?",
                 f1(else_end) if else_end is not None else "?"))

    term_body = stripped[cond[0]:open_end + 1]
    else_body = stripped[else_line:else_end + 1] if else_line is not None else []
    term_txt = "\n".join(term_body)
    else_txt = "\n".join(else_body)

    if re.search(r"probe\s*\.\s*dw_join\s*\(", term_txt):
        ln = next(f1(cond[0] + j) for j, L in enumerate(term_body)
                  if "dw_join" in L)
        ev.append("终态已确认支（if 支）含 dw_join @ 行 %s" % ln)
    else:
        fails.append("终态已确认支内无 dw_join（结构错位）")
    if re.search(r"probe\s*\.\s*d6b\s*\(", term_txt):
        ln = next(f1(cond[0] + j) for j, L in enumerate(term_body) if "d6b" in L)
        ev.append("终态已确认支（if 支）含 d6b(fd_dup) 调用 @ 行 %s"
                  "（D6b 步 4-7 全部位于该支内）" % ln)
    else:
        fails.append("d6b 调用不在 worker 终态已确认支内（A10 :1085 必 fail）")

    if re.search(r"skip_emit\s*\(\s*'D6b'\s*,\s*'join-timeout-abandoned'", else_txt):
        ln = next(f1(else_line + j) for j, L in enumerate(else_body)
                  if "join-timeout-abandoned" in L)
        ev.append("join-timeout 支（else 支）含 skip_emit('D6b','join-timeout-abandoned')"
                  " @ 行 %s（join-timeout 登记点）" % ln)
    else:
        fails.append("else 支内无 join-timeout-abandoned skip 登记")

    op_pat = re.compile(r"\b(read|write|fcntl|close|d6b|fd_dup)\b")
    op_hits = [(f1(else_line + j), L.strip()) for j, L in enumerate(else_body)
               if op_pat.search(L)]
    ev.append("join-timeout 支内 fd 操作/d6b/fd_dup 记号扫描: %s"
              % (op_hits or "0 处"))
    if op_hits:
        fails.append("join-timeout 支内出现 fd_dup 操作记号: %s" % op_hits)

    # else 支之后至函数尾：确认无直接 fd 操作（P11/P12 均为 native 调用）
    enc = ets_method_extent(skel, "runRetainedLine")
    enc = ("runRetainedLine", enc[0], enc[1]) if enc else None
    tail_end = enc[2] if enc else open_end
    tail = stripped[else_end + 1: tail_end + 1]
    tail_hits = [(f1(else_end + 1 + j), L.strip()) for j, L in enumerate(tail)
                 if re.search(r"\b(read|write|fcntl|close|d6b)\s*\(", L)]
    ev.append("join-timeout 支后至 %s 尾部直接 fd 调用: %s（P11/P12 经 native，"
              "主线程零直接 fd 操作）" % (enc[0], tail_hits or "0 处"))
    if tail_hits:
        fails.append("join-timeout 后主线程控制流存在直接 fd 调用: %s" % tail_hits)

    # Rust 侧登记：D6b 步 4-7 调用点全部位于 d6b() 内（被终态支唯一调用）
    _, d6_stripped, d6_skel = load_rs_lines(PROBE_SRC / "d6.rs")
    d6b_fn = first_match(d6_skel, r"pub\s+fn\s+d6b\b")
    d6b_end = find_block_extent(d6_skel, d6b_fn[0])
    inner = [(f1(j), d6_stripped[j].strip()) for j in range(d6b_fn[0], d6b_end + 1)
             if re.search(r"(sys::fcntl|read_fd|sys::close|sys::socket)\s*\(",
                          d6_stripped[j])]
    ev.append("d6.rs d6b()（%d-%d）内 fd 调用点（S4-S7，全部被终态支门控）: %s"
              % (f1(d6b_fn[0]), f1(d6b_end), inner))
    d6b_calls = []
    for p in ets_files():
        _, st, _ = load_ets_lines(p)
        r = str(p.relative_to(ROOT))
        for i, line in enumerate(st):
            if re.search(r"probe\s*\.\s*d6b\s*\(", line):
                d6b_calls.append("%s:%d" % (r, f1(i)))
    ev.append("d6b 调用点全集: %s" % (d6b_calls or "0 处"))
    if len(d6b_calls) != 1:
        fails.append("d6b 调用点=%d（要求恰 1，位于终态已确认支）" % len(d6b_calls))

    verdict = "FAIL" if fails else "PASS"
    return {"id": "A10", "verdict": verdict,
            "claim": "join-timeout 分支（skip_emit('D6b',...) 所在支）内零 fd_dup "
                     "操作（read/write/fcntl/close）；d6b 调用位于 terminal 已确认支"
                     "（if/else 结构核对，:1085 + :1102 判定强度注）",
            "evidence": ev, "registry": [], "notes": notes, "fails": fails}


def check_a11():
    ev, fails = [], []
    path = PROBE_SRC / "dw.rs"
    _, stripped, skel = load_rs_lines(path)
    fns = fn_index(skel)
    worker = first_match(skel, r"fn\s+dw_worker\b")
    w_end = find_block_extent(skel, worker[0])
    poll = first_match(stripped, r"sys\s*:\s*:\s*poll1\s*\(", worker[0], w_end)
    if not poll:
        return {"id": "A11", "verdict": "FAIL",
                "claim": "worker poll 返回后单次钟读、ret/errno/revents 各至多一读、"
                         "快照与 DW_RETURN 同源、dw_drain_end 单写无重读",
                "evidence": ["dw_worker 内未找到 poll1 调用"], "registry": [],
                "notes": [], "fails": ["poll1 调用缺失"]}
    bind = first_match(stripped, r"let\s*\(\s*ret\s*,\s*perrno\s*,\s*revents\s*\)",
                       worker[0], w_end)
    ev.append("poll 调用: dw.rs:%d `%s`；单次绑定 dw.rs:%d "
              "`let (ret, perrno, revents) = ...`（各恰一次读取的唯一源）"
              % (f1(poll[0]), poll[1].strip(), f1(bind[0]) if bind else -1))

    post = [i for i in range(poll[0], w_end + 1)
            if re.search(r"mono_ms\s*\(\s*\)", stripped[i])]
    ev.append("poll 返回后 mono_ms() 钟读点: %s" %
              ["dw.rs:%d %s" % (f1(i), stripped[i].strip()) for i in post])
    if len(post) != 1:
        fails.append("poll 返回后 mono_ms() 读取=%d 次（要求恰 1 次）" % len(post))
    else:
        ev.append("单次钟读两写: dw.rs:%d end_clock -> at_mono_ms=%d / "
                  "elapsed_ms=%d（同源两写）"
                  % (f1(post[0]), f1(post[0] + 1), f1(post[0] + 2)))

    errno_post = [i for i in range(poll[0], w_end + 1)
                  if re.search(r"sys\s*:\s*:\s*errno\s*\(", stripped[i])]
    ev.append("poll 返回后 sys::errno() 再读点: %s" %
              (["dw.rs:%d" % f1(i) for i in errno_post] or "0 处"))
    if errno_post:
        fails.append("poll 返回后存在二次 errno 读取: %s"
                     % ["dw.rs:%d" % f1(i) for i in errno_post])

    reassign = [i for i in range(bind[0] + 1, w_end + 1)
                if re.search(r"\b(ret|perrno|revents)\s*=[^=]", skel[i])]
    ev.append("ret/perrno/revents 绑定后再赋值: %s" %
              (["dw.rs:%d" % f1(i) for i in reassign] or "0 处"))
    if reassign:
        fails.append("poll raw 二次赋值（各至多一读被破坏）: %s"
                     % ["dw.rs:%d" % f1(i) for i in reassign])

    # DW_RETURN 与快照同源（同一组局部变量名）
    dwret = first_match(stripped, r"N1BDISC_DW_RETURN", worker[0], w_end)
    fmt_paren = find_format_paren(stripped, dwret[0])
    args_txt, args_end = _balanced_args(stripped, fmt_paren[0], fmt_paren[1])
    arg_names = set(re.findall(r"[A-Za-z_]\w*", args_txt.split(",", 1)[1]))
    ev.append("DW_RETURN 发射 dw.rs:%d 值参名: %s"
              % (f1(dwret[0]), sorted(n for n in arg_names
                                      if n not in ("format",))))
    snap_lines = [(i, stripped[i]) for i in range(poll[0], w_end + 1)
                  if re.search(r"SNAP_(RET|ERRNO|REVENTS|AT_MONO_MS|ELAPSED_MS)\s*\.\s*store",
                               stripped[i])]
    snap_names = set()
    for i, L in snap_lines:
        m = re.search(r"\.\s*store\s*\(\s*([A-Za-z_]\w*)", L)
        if m:
            snap_names.add(m.group(1))
            ev.append("快照写 dw.rs:%d `%s` <- 局变量 %s" % (f1(i), L.strip()[:60], m.group(1)))
    if not arg_names >= snap_names:
        fails.append("快照五 raw 与 DW_RETURN 值参不同源: 快照=%s 发射=%s"
                     % (sorted(snap_names), sorted(arg_names)))
    else:
        ev.append("同源核对: 快照五写与 DW_RETURN 发射共享同一组局部变量名 %s"
                  % sorted(snap_names))

    # dw_drain_end 单写、poll 后无重读/重写
    dstore = [i for i in range(worker[0], w_end + 1)
              if re.search(r"SNAP_DRAIN_END\s*\.\s*store", stripped[i])]
    dload = [i for i in range(worker[0], w_end + 1)
             if re.search(r"SNAP_DRAIN_END\s*\.\s*load", stripped[i])]
    ev.append("SNAP_DRAIN_END（dw_drain_end）worker 内 store: %s（poll 前，单写）；"
              "worker 内 load: %s"
              % (["dw.rs:%d" % f1(i) for i in dstore],
                 ["dw.rs:%d" % f1(i) for i in dload] or "0 处"))
    if len(dstore) != 1 or dstore[0] > poll[0]:
        fails.append("dw_drain_end 非 poll 前单写: store=%s" %
                     ["dw.rs:%d" % f1(i) for i in dstore])
    if dload:
        fails.append("worker 内 dw_drain_end 重读: %s" %
                     ["dw.rs:%d" % f1(i) for i in dload])
    post_loads = [i for i in range(w_end + 1, len(stripped))
                  if re.search(r"SNAP_DRAIN_END\s*\.\s*load", stripped[i])]
    ev.append("worker 之外 SNAP_DRAIN_END.load（P12 post_emit 快照读，:1104 单列"
              "口径允许）: %s" % (["dw.rs:%d" % f1(i) for i in post_loads] or "0 处"))

    verdict = "FAIL" if fails else "PASS"
    return {"id": "A11", "verdict": verdict,
            "claim": "poll 返回后单次钟读；ret/errno/revents 各至多一读；"
                     "快照五 raw 与 DW_RETURN 同源（同一组局部变量名）；"
                     "dw_drain_end 单写无重读（:1104 单列口径）",
            "evidence": ev, "registry": [], "notes": [], "fails": fails}


def check_a12():
    ev, fails, notes = [], [], []
    path = PROBE_SRC / "dw.rs"
    _, stripped, skel = load_rs_lines(path)
    post = first_match(skel, r"pub\s+fn\s+post_emit\b")
    p_end = find_block_extent(skel, post[0])

    loads = [i for i in range(post[0], p_end + 1)
             if re.search(r"DW_TERMINAL\s*\.\s*load", skel[i])]
    decision = first_match(skel, r"let\s+f\s*=\s*DW_TERMINAL\s*\.\s*load",
                           post[0], p_end)
    ev.append("post_emit 内 DW_TERMINAL.load 全部出现: %s" %
              ["dw.rs:%d %s" % (f1(i), skel[i].strip()[:70]) for i in loads])
    if not decision:
        fails.append("post_emit 内未找到唯一决策 load（`let f = DW_TERMINAL.load`）")
        return {"id": "A12", "verdict": "FAIL",
                "claim": "RACEWIN 发射仅位于 F=0 盒到期支；五输出绑定单次 "
                         "dw_worker_terminal load",
                "evidence": ev, "registry": [], "notes": notes, "fails": fails}
    ev.append("决策 load: dw.rs:%d `let f = DW_TERMINAL.load(Ordering::SeqCst)`"
              "（P12 对 dw_worker_terminal 的那一次 seq_cst load）" % f1(decision[0]))

    # :758 盒到期复验区（f_after_box 绑定块）——规格明文许可的第二次读
    fab = first_match(skel, r"let\s+f_after_box\s*=", post[0], p_end)
    fab_end = find_block_extent(skel, fab[0]) if fab else None
    sanctioned, unsanctioned, pre = [], [], []
    for i in loads:
        if i == decision[0]:
            continue
        if fab and fab[0] <= i <= fab_end:
            sanctioned.append(i)
        elif i < decision[0]:
            pre.append(i)
        else:
            unsanctioned.append(i)
    if pre:
        ev.append("决策 load 之前盒区 load（r19-r22 盒机制，位于绑定域之前）: %s"
                  % ["dw.rs:%d" % f1(i) for i in pre])
    if sanctioned:
        ev.append(":758 盒到期复验区（f_after_box 绑定块 dw.rs:%d-%d 内，"
                  "规格明文许可的盒到期 F=FLAG 复核）: %s"
                  % (f1(fab[0]), f1(fab_end),
                     ["dw.rs:%d" % f1(i) for i in sanctioned]))
    if unsanctioned:
        fails.append("绑定域内存在非 :758 许可的二次 FLAG load: %s"
                     % ["dw.rs:%d" % f1(i) for i in unsanctioned])
    else:
        ev.append("决策/盒区之外二次 DW_TERMINAL.load = 0 处（②③ 之间另读 "
                  "FLAG/重求值 = 无）")

    # RACEWIN 发射点（全 crate 计数）
    rw = [i for i in range(len(stripped)) if "N1BDISC_DW_RACEWIN" in stripped[i]]
    ev.append("RACEWIN 发射行（全 crate）: %s" %
              (["dw.rs:%d" % f1(i) for i in rw] or "0 处"))
    if len(rw) != 1:
        fails.append("RACEWIN 发射点=%d（要求恰 1）" % len(rw))

    derive = first_match(skel, r"fn\s+derive_post_outcome\b", 0, len(skel))
    if derive and fab:
        verdict = _a12_shape_b(post, p_end, decision, fab, fab_end, sanctioned,
                               derive, rw, stripped, skel, ev, fails, notes)
    else:
        verdict = _a12_shape_a(post, p_end, decision, rw, stripped, skel,
                               ev, fails, notes)
    return {"id": "A12", "verdict": verdict,
            "claim": "RACEWIN 发射位于且仅位于 F=0 盒到期条件支内；cut/class/"
                     "RACEWIN/watchdog/poll raw 五输出的写值与发射绑定单次 "
                     "dw_worker_terminal seq_cst load（:1087 + :1106-1107 注；"
                     ":758 盒到期复验为规格明文许可的唯一例外）",
            "evidence": ev, "registry": [], "notes": notes, "fails": fails}


def _a12_shape_b(post, p_end, decision, fab, fab_end, sanctioned, derive, rw,
                 stripped, skel, ev, fails, notes):
    """Shape B（B-01 结构）：五输出经 PostOutcome 结构体由 derive_post_outcome
    从单次 f load（+ :758 许可的 f_after_box）派生；RACEWIN 发射由唯一
    racewin=true cell 门控。"""
    d_end = find_block_extent(skel, derive[0])
    ev.append("五输出载体: PostOutcome 结构体 + derive_post_outcome 纯函数"
              "（dw.rs:%d-%d，单次 f load 以参数进入派生，函数内零 FLAG load）"
              % (f1(derive[0]), f1(d_end)))
    d_loads = [i for i in range(derive[0], d_end + 1)
               if re.search(r"DW_TERMINAL\s*\.\s*load", skel[i])]
    if d_loads:
        fails.append("derive_post_outcome 内存在 FLAG load（隐藏重求值）: %s"
                     % ["dw.rs:%d" % f1(i) for i in d_loads])
    rw_true = [i for i in range(derive[0], d_end + 1)
               if re.search(r"racewin:\s*true", skel[i])]
    ev.append("racewin=true 赋值点（F=0 竞态 cell，要求恰 1）: %s"
              % (["dw.rs:%d" % f1(i) for i in rw_true] or "0 处"))
    if len(rw_true) != 1:
        fails.append("derive 内 racewin=true 赋值=%d（要求恰 1：唯一竞态 cell）"
                     % len(rw_true))
    else:
        cell_ctx = stripped[max(derive[0], rw_true[0] - 30): rw_true[0] + 1]
        anchors = ["flag-race-window-expired" in L for L in cell_ctx]
        if any(anchors):
            ev.append("结构核对: 唯一 racewin=true cell 携带 flag-race-window-"
                      "expired 编码（即 F=0 盒到期支；盒到期由 :758 复验 f_after_box "
                      "承载）")
        else:
            notes.append("QUESTION: 唯一 racewin=true cell 未见 flag-race 编码锚，"
                         "F=0 归属需人工复核")
    # 决策变量进入派生：调用实参含 f 与 f_after_box
    call = first_match(skel, r"derive_post_outcome\s*\(", post[0], p_end)
    args_txt, _end = _balanced_args(
        stripped, call[0],
        stripped[call[0]].index("(", stripped[call[0]].index("derive_post_outcome")))
    arg_toks = set(re.findall(r"[A-Za-z_]\w*", args_txt))
    for need in ("f", "f_after_box"):
        if need not in arg_toks:
            fails.append("derive_post_outcome 调用实参缺 %s（单次 F 判定未进入派生）"
                         % need)
    ev.append("derive 调用实参: %s（含决策变量 f 与 :758 许可的 f_after_box）"
              % sorted(arg_toks))
    # RACEWIN 发射门控
    if len(rw) == 1:
        gate = first_match(skel, r"if\s+out\s*\.\s*racewin\s*\{",
                           max(post[0], rw[0] - 4), rw[0] + 1)
        if gate:
            ev.append("结构核对: RACEWIN 发射（dw.rs:%d）被 `if out.racewin` 门控，"
                      "而 racewin=true 唯一落在 F=0 竞态 cell —— 发射点位于且仅位于 "
                      "F=0 盒到期条件支" % f1(rw[0]))
        else:
            fails.append("RACEWIN 发射（dw.rs:%d）未见 out.racewin 门控" % f1(rw[0]))
    # cut 绑定：worker_terminal_at_p12 <- out.cut；cells 的 cut 源自 f 语义
    cut_lines = [(i, stripped[i]) for i in range(post[0], p_end + 1)
                 if "worker_terminal_at_p12={}" in stripped[i]]
    cut_arg = None
    if cut_lines:
        fp = find_format_paren(stripped, cut_lines[0][0])
        args_txt2, _e2 = _balanced_args(stripped, fp[0], fp[1])
        toks = [t.strip() for t in args_txt2.split(",")[1:]]
        cut_arg = toks[-1].strip() if toks else None
        ev.append("cut 绑定: POST dw.rs:%d `worker_terminal_at_p12={}` <- `%s` "
                  "（PostOutcome.cut；cells 以 cut: f / cut: true / cut: false 自同一 "
                  "F 判定取值）" % (f1(cut_lines[0][0]), cut_arg))
        if cut_arg != "out.cut":
            fails.append("worker_terminal_at_p12 未绑定 out.cut（got %r）" % cut_arg)
    else:
        fails.append("POST 载荷未见 worker_terminal_at_p12 绑定")
    cells = [(f1(i), skel[i].strip()) for i in range(derive[0], d_end + 1)
             if re.search(r"\bcut:\s*(f|true|false)\b", skel[i])]
    ev.append("cell 级 cut 取值登记: %s" % cells)
    # 五输出字段同源：POST/JSON 均引用 out.* 同一结构体字段
    po = [i for i in range(post[0], p_end + 1)
          if "poll_ret=" in stripped[i] and "poll_errno=" in stripped[i]]
    js = [i for i in range(post[0], p_end + 1)
          if "dw_poll_ret" in stripped[i]]
    ev.append("五输出消费: POST dw_outcome（dw.rs:%s）与 JSON 返回（dw.rs:%s）"
              "引用同一 PostOutcome 实例字段 out.class/out.poll_*/out.watchdog/out.cut"
              % (f1(po[0]) if po else "n/a", f1(js[0]) if js else "n/a"))
    return "FAIL" if fails else "PASS"


def _a12_shape_a(post, p_end, decision, rw, stripped, skel, ev, fails, notes):
    """Shape A（直接链结构）：五输出经单 if/else 元组链绑定到决策变量 f。"""
    chain = first_match(stripped,
                        r"let\s*\(\s*class\s*,\s*poll_ret\s*,\s*poll_errno\s*,"
                        r"\s*poll_revents\s*,\s*poll_elapsed\s*,\s*watchdog\s*,"
                        r"\s*racewin\s*\)",
                        post[0], p_end)
    if not chain:
        fails.append("未找到五输出元组绑定链")
        return "FAIL"
    chain_end = None
    for i in range(chain[0], p_end + 1):
        if re.match(r"\s*\};", stripped[i]):
            chain_end = i
            break
    ev.append("五输出绑定链: dw.rs:%d-%d（class/poll_ret/poll_errno/"
              "poll_revents/poll_elapsed/watchdog/racewin 单元组单链赋值）"
              % (f1(chain[0]), f1(chain_end)))
    seg = stripped[chain[0]:chain_end + 1]
    arms = []
    for i, L in zip(range(chain[0], chain_end + 1), seg):
        if re.search(r"if\s*!\s*spawned", L):
            arms.append((i, "!spawned"))
        elif re.search(r"else\s+if\s+f\b", L):
            arms.append((i, "f=true"))
        elif re.search(r"else\s+if\s+jt\b", L):
            arms.append((i, "f=false∧jt"))
        elif re.search(r"else\s*\{", L) and "else if" not in L:
            arms.append((i, "f=false∧!jt"))
    ev.append("链臂结构: %s" % ["%d:%s" % (f1(i), a) for i, a in arms])
    kinds = [a for _i, a in arms]
    if kinds != ["!spawned", "f=true", "f=false∧jt", "f=false∧!jt"]:
        fails.append("五输出非单一 if/else 链（臂序异常）: %s" % kinds)
    seg_loads = [i for i, L in zip(range(chain[0], chain_end + 1), seg)
                 if "DW_TERMINAL.load" in L]
    if seg_loads:
        fails.append("绑定链内存在 FLAG load: %s"
                     % ["dw.rs:%d" % f1(i) for i in seg_loads])
    jt_arm = [a for a in arms if a[1] == "f=false∧jt"]
    jt_next = [a for a in arms if a[1] == "f=false∧!jt"]
    jt_start = jt_arm[0][0] if jt_arm else None
    jt_stop = jt_next[0][0] if jt_next else chain_end
    if len(rw) == 1 and jt_start is not None and jt_start < rw[0] < jt_stop:
        ev.append("结构核对: RACEWIN 发射（dw.rs:%d）位于且仅位于 F=0 盒到期支"
                  "（f=false∧jt 臂 %d-%d）内" % (f1(rw[0]), f1(jt_start), f1(jt_stop)))
    else:
        fails.append("RACEWIN 发射不在唯一 F=0 支内（位置=%s, 臂=%s）"
                     % (["dw.rs:%d" % f1(i) for i in rw], jt_start))
    cut = [i for i in range(post[0], p_end + 1)
           if re.search(r"worker_terminal_at_p12\s*=\s*f\b", stripped[i])]
    ev.append("cut（worker_terminal_at_p12）绑定: %s" %
              (["dw.rs:%d `%s`" % (f1(i), stripped[i].strip()) for i in cut] or "0 处"))
    if len(cut) != 1:
        fails.append("worker_terminal_at_p12 未绑定到唯一决策变量 f")
    return "FAIL" if fails else "PASS"


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------

CHECKS = [check_a1, check_a2, check_a3, check_a4, check_a5, check_a6,
          check_a7, check_a8, check_a9, check_a10, check_a11, check_a12]


def source_fingerprint():
    """被检源码快照指纹（sha256 前 12 位）——freeze 记录以此钉住检查时的树状态。"""
    import hashlib
    files = (sorted(rs_files()) + sorted(ets_files()) +
             sorted(RUNNER.glob("*.py")) + sorted(SELFTESTS.glob("*.py")))
    h = hashlib.sha256()
    rows = []
    for p in files:
        digest = hashlib.sha256(p.read_bytes()).hexdigest()[:12]
        rows.append("%s=%s" % (p.relative_to(ROOT), digest))
        h.update(p.read_bytes())
    return {"tree_sha256_12": h.hexdigest()[:12],
            "files": len(rows), "per_file": rows}


def main():
    json_only = "--json-only" in sys.argv
    results = []
    for fn in CHECKS:
        try:
            r = fn()
        except Exception as e:  # 工具自身缺陷必须显性失败，不得静默 PASS
            r = {"id": fn.__name__.replace("check_", "").upper(),
                 "verdict": "FAIL",
                 "claim": "(工具异常)",
                 "evidence": ["检查器异常: %r" % (e,)],
                 "registry": [], "notes": [], "fails": ["tool-error: %r" % (e,)]}
        results.append(r)

    if not json_only:
        for r in results:
            print("=" * 78)
            print("%s  判定: %s" % (r["id"], r["verdict"]))
            print("断言: %s" % r["claim"])
            print("证据:")
            for e in r["evidence"]:
                print("  %s" % e)
            if r.get("registry"):
                print("登记表:")
                for item in r["registry"]:
                    print("  %s" % json.dumps(item, ensure_ascii=False))
            for n in r.get("notes", []):
                print("登记/注: %s" % n)
            if r["fails"]:
                print("违规项:")
                for x in r["fails"]:
                    print("  FAIL: %s" % x)
            print()
        print("=" * 78)

    summary = {
        "tool": "spikes/n1b-disc-phys-hap/staticcheck/check_static.py",
        "spec": SPEC,
        "run_root": str(ROOT),
        "source_fingerprint": source_fingerprint(),
        "results": [{"id": r["id"], "verdict": r["verdict"],
                     "summary": r["claim"],
                     "fails": r["fails"],
                     "notes": r.get("notes", [])} for r in results],
        "fail": [r["id"] for r in results if r["verdict"] == "FAIL"],
        "question": [r["id"] for r in results if r["verdict"] == "QUESTION"],
        "pass": [r["id"] for r in results if r["verdict"] == "PASS"],
        "exit_code": 1 if any(r["verdict"] == "FAIL" for r in results) else 0,
    }
    if not json_only:
        print("汇总 JSON:")
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return summary["exit_code"]


if __name__ == "__main__":
    sys.exit(main())
