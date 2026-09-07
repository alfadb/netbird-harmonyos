#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc UI trigger selftests（host-only，只读源码，零设备）。

钉住 Index.ets 单次点击 → startVpnExtensionAbility → 系统 VPN 授权 →
N1BDiscVpnExtensionAbility.onCreate 启动链的冻结约束：

  T1. 启动 API 恰 1 个调用位点，且只位于 .onClick 处理器体内（点击门控）
  T2. Want 冻结值：bundle cn.alfadb.netbird.n1bdisc +
      abilityName N1BDiscVpnExtensionAbility，对象字面仅这两个键
  T3. 权限最小集：module.json5 requestPermissions == {ohos.permission.INTERNET}；
      vpn 扩展 exported: false / type: 'vpn'；无 ACL 字样
  T4. 无自动启动/无重试：无 aboutToAppear/onPageShow/onAppear/定时器/循环；
      单发闩锁（requestSent 同步置位 + enabled(!requestSent) + 提前 return）
  T5. 既有 tag/domain 不变、无新增 N1BDISC_ 前缀字面（A5 口径）

一条命令运行（pytest 与自研 main 双兼容）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_ui_trigger.py
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_ui_trigger.py
"""

from __future__ import annotations

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ETS_DIR = os.path.join(ROOT, "entry", "src", "main", "ets")
INDEX = os.path.join(ETS_DIR, "pages", "Index.ets")
MODULE_JSON5 = os.path.join(ROOT, "entry", "src", "main", "module.json5")
APP_JSON5 = os.path.join(ROOT, "AppScope", "app.json5")

FROZEN_BUNDLE = "cn.alfadb.netbird.n1bdisc"
FROZEN_ABILITY = "N1BDiscVpnExtensionAbility"
FROZEN_PERMISSION = "ohos.permission.INTERNET"

_ASSERTS = 0


def expect(cond, msg):
    """断言计数器（main 汇总每断言数；pytest 下等价 assert）。"""
    global _ASSERTS
    _ASSERTS += 1
    if not cond:
        raise AssertionError(msg)


def read(path):
    with open(path, "r", encoding="utf-8") as fh:
        return fh.read()


def strip_ets_comments(text):
    """去 // 与 /* */ 注释（跳过字符串字面内内容），返回注释剥离文本。"""
    out = []
    i, n = 0, len(text)
    quote = None
    while i < n:
        c = text[i]
        if quote is not None:
            out.append(c)
            if c == "\\" and i + 1 < n:
                out.append(text[i + 1])
                i += 2
                continue
            if c == quote:
                quote = None
            i += 1
            continue
        if c in ("'", '"', "`"):
            quote = c
            out.append(c)
            i += 1
            continue
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
            continue
        if text.startswith("/*", i):
            j = text.find("*/", i + 2)
            i = n if j < 0 else j + 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


def match_span(text, open_idx):
    """给定 text[open_idx] == '(' / '{' / '['，返回匹配闭括号下标。"""
    opens = {"(": ")", "{": "}", "[": "]"}
    close_ch = opens[text[open_idx]]
    depth = 0
    for i in range(open_idx, len(text)):
        c = text[i]
        if c in opens:
            depth += 1
        elif c in (")", "}", "]"):
            depth -= 1
            if depth == 0:
                expect(c == close_ch,
                       "unbalanced bracket at %d: expected %r got %r"
                       % (i, close_ch, c))
                return i
    raise AssertionError("no matching close bracket for index %d" % open_idx)


def ets_sources():
    paths = []
    for dirpath, dirnames, filenames in os.walk(ETS_DIR):
        dirnames.sort()
        for name in sorted(filenames):
            if name.endswith(".ets"):
                paths.append(os.path.join(dirpath, name))
    return paths


# --------------------------------------------------------------------------
# T1. 恰 1 个启动调用位点，且点击门控
# --------------------------------------------------------------------------

def test_single_click_gated_call_site():
    call_re = re.compile(r"vpnExtension\s*\.\s*startVpnExtensionAbility\s*\(")
    sites = []
    for path in ets_sources():
        rel = os.path.relpath(path, ROOT)
        text = strip_ets_comments(read(path))
        for m in call_re.finditer(text):
            sites.append((rel, m.start()))
    expect(len(sites) == 1,
           "startVpnExtensionAbility call sites across ets = %d (frozen 1): %s"
           % (len(sites), sites))
    rel, pos = sites[0]
    expect(rel == os.path.relpath(INDEX, ROOT),
           "sole call site must live in pages/Index.ets, got %s" % rel)

    text = strip_ets_comments(read(INDEX))
    # 门控链：onClick 处理器 → this.requestStart() → API。API 调用须位于
    # requestStart 函数体内，且 requestStart() 的唯一调用点位于 onClick 内。
    m = re.search(r"private\s+requestStart\s*\(\s*\)\s*:\s*void\s*\{", text)
    expect(m is not None, "requestStart() entry must exist")
    fn_open = text.index("{", m.end() - 1)
    fn_close = match_span(text, fn_open)
    expect(fn_open < pos < fn_close,
           "API call site must sit inside requestStart(), site=%d fn=%d-%d"
           % (pos, fn_open, fn_close))

    handlers = []
    for om in re.finditer(r"\.onClick\s*\(", text):
        end = match_span(text, om.end() - 1)
        handlers.append((om.start(), end))
    expect(len(handlers) >= 1, "Index.ets must register at least one onClick")
    dispatches = [d.start() for d in re.finditer(r"this\.requestStart\s*\(", text)]
    expect(len(dispatches) == 1,
           "requestStart() must be dispatched exactly once, got %d"
           % len(dispatches))
    inside = [h for h in handlers if h[0] < dispatches[0] < h[1]]
    expect(len(inside) == 1,
           "sole requestStart() dispatch must sit inside one onClick handler, "
           "handlers=%s" % (handlers,))
    # 生命周期/自动触发向量不存在（无 onAppear/onCreate 自动启动路径）
    for banned in ("aboutToAppear", "onPageShow", "onAppear", "onForeground"):
        expect(banned not in text,
               "Index.ets must not reference %s (no auto-start vector)" % banned)


# --------------------------------------------------------------------------
# T2. Want 冻结值
# --------------------------------------------------------------------------

def test_want_frozen_values():
    text = strip_ets_comments(read(INDEX))
    m = re.search(r"const\s+BUNDLE_NAME\s*:\s*string\s*=\s*'([^']+)'", text)
    expect(m is not None and m.group(1) == FROZEN_BUNDLE,
           "BUNDLE_NAME literal must equal frozen %r" % FROZEN_BUNDLE)
    m = re.search(r"const\s+EXTENSION_ABILITY\s*:\s*string\s*=\s*'([^']+)'", text)
    expect(m is not None and m.group(1) == FROZEN_ABILITY,
           "EXTENSION_ABILITY literal must equal frozen %r" % FROZEN_ABILITY)

    m = re.search(r"private\s+buildWant\s*\(\s*\)\s*:\s*Want\s*\{", text)
    expect(m is not None, "buildWant(): Want builder must exist")
    open_idx = text.index("{", m.end() - 1)
    close_idx = match_span(text, open_idx)
    body = text[open_idx:close_idx + 1]
    lit = re.search(r"return\s*\{(.*)\}\s*;?\s*$", body, re.S)
    expect(lit is not None, "buildWant body must be a single typed Want literal")
    keys = re.findall(r"(\w+)\s*:", lit.group(1))
    expect(sorted(keys) == ["abilityName", "bundleName"],
           "Want literal keys must be exactly [bundleName, abilityName], got %s"
           % sorted(keys))
    expect("BUNDLE_NAME" in lit.group(1) and "EXTENSION_ABILITY" in lit.group(1),
           "Want literal must bind the frozen consts (no inline drift)")

    app = read(APP_JSON5)
    m = re.search(r"bundleName\s*:\s*'([^']+)'", app)
    expect(m is not None and m.group(1) == FROZEN_BUNDLE,
           "AppScope bundleName must equal frozen %r" % FROZEN_BUNDLE)


# --------------------------------------------------------------------------
# T3. 权限最小集
# --------------------------------------------------------------------------

def test_module_permissions_minimal():
    text = read(MODULE_JSON5)
    m = re.search(r"requestPermissions\s*:\s*\[", text)
    expect(m is not None, "module.json5 must declare requestPermissions")
    open_idx = text.index("[", m.end() - 1)
    close_idx = match_span(text, open_idx)
    block = text[open_idx:close_idx + 1]
    names = re.findall(r"name\s*:\s*'([^']+)'", block)
    expect(names == [FROZEN_PERMISSION],
           "requestPermissions must be exactly [%r], got %s"
           % (FROZEN_PERMISSION, names))
    expect("ACL" not in text and "acl" not in text,
           "module.json5 must not carry ACL markers")
    expect(not re.search(r"ohos\.permission\.\w*VPN", text),
           "no VPN system permission allowed (E3 minimal set)")

    ext_m = re.search(r"extensionAbilities\s*:\s*\[", text)
    expect(ext_m is not None, "extensionAbilities block must exist")
    ext_open = text.index("[", ext_m.end() - 1)
    ext_block = text[ext_open:match_span(text, ext_open) + 1]
    expect("'%s'" % FROZEN_ABILITY in ext_block,
           "extension %s must stay registered" % FROZEN_ABILITY)
    expect(re.search(r"type\s*:\s*'vpn'", ext_block),
           "extension type must be 'vpn'")
    expect(re.search(r"exported\s*:\s*false", ext_block),
           "vpn extension must stay exported: false")


# --------------------------------------------------------------------------
# T4. 无自动启动/无重试：单发闩锁
# --------------------------------------------------------------------------

def test_single_shot_latch_no_retry():
    text = strip_ets_comments(read(INDEX))
    for banned in ("setTimeout", "setInterval", "Promise.race"):
        expect(banned not in text,
               "Index.ets must not contain %s (no retry machinery)" % banned)
    expect(not re.search(r"\b(while|for)\s*\(", text),
           "Index.ets must contain no loops (single-shot request path)")

    latch = re.findall(r"this\.requestSent\s*=\s*true", text)
    expect(len(latch) == 1,
           "requestSent must be latched true exactly once, got %d" % len(latch))
    expect(re.search(r"\.enabled\s*\(\s*!this\.requestSent\s*\)", text),
           "trigger button must be disabled once requestSent latches")

    m = re.search(r"private\s+requestStart\s*\(\s*\)\s*:\s*void\s*\{", text)
    expect(m is not None, "requestStart() entry must exist")
    open_idx = text.index("{", m.end() - 1)
    close_idx = match_span(text, open_idx)
    body = text[open_idx:close_idx + 1]
    guard = re.search(r"if\s*\(\s*this\.requestSent\s*\)\s*\{", body)
    expect(guard is not None, "requestStart must early-return on latched state")
    # guard 体内唯一动作是 return（不重发、不改闩锁、不发日志）；
    # body 相对索引换算回全文绝对索引后再取括号 span。
    guard_open_idx = text.index("{", open_idx + guard.end() - 1)
    guard_close = match_span(text, guard_open_idx)
    guard_text = text[open_idx + guard.end():guard_close]
    expect(re.search(r"\breturn\b", guard_text),
           "latched branch must return immediately")
    expect(not call_re().search(guard_text),
           "latched branch must never re-issue the start request")
    # 闩锁置位先于启动调用（同一次尝试请求发出后即禁重复点击）
    expect(body.index("this.requestSent = true") < body.index("startVpnExtensionAbility"),
           "latch must be set synchronously before issuing the request")


def call_re():
    return re.compile(r"vpnExtension\s*\.\s*startVpnExtensionAbility\s*\(")


# --------------------------------------------------------------------------
# T5. tag/domain 不变 + A5 口径（ETS 零 N1BDISC_ 字面）
# --------------------------------------------------------------------------

def test_tag_domain_and_a5_surface():
    text = read(INDEX)
    expect("const DOMAIN: number = 0x2900;" in text,
           "hilog DOMAIN must stay 0x2900")
    expect("const TAG: string = 'N1BDiscVpn';" in text,
           "hilog TAG must stay 'N1BDiscVpn'")
    for path in ets_sources():
        rel = os.path.relpath(path, ROOT)
        hits = re.findall(r"N1BDISC_[A-Z0-9_]*", read(path))
        expect(not hits, "A5: no N1BDISC_ literal in %s, got %s" % (rel, hits))


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
    print("n1bdisc ui-trigger selftests: tests=%d passed=%d failed=%d "
          "assertions=%d (%.0f ms)"
          % (len(tests), passed, failed, _ASSERTS, elapsed * 1000))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
