#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""fake_hdc_exec — 真 subprocess 测试夹具：可编程假 hdc 可执行文件（host-only）。

只被 ``selftests/test_transport_real.py`` 当作 transport 的 ``command`` 绝对路径
spawn——测试只用本夹具副本的绝对路径构造 transport，绝不指向真实 hdc，不改 PATH，
不触发设备/签名/网络。行为由环境变量 ``N1B_FAKE_HDC_EXEC_SCENE`` 指向的 JSON
剧本驱动：

- ``rules``：按 ``match``（argv 前缀逐 token 相等）取第一条命中规则；全部未命中
  （或无剧本）→ 把收到的 argv 以 JSON 行逐字 echo 到 stdout（供 argv 保真断言）；
- 规则字段：
  - ``echo_argv``：先把 argv 以 JSON 行写到 stdout；
  - ``chunks``：``{fd, data, repeat, delay_ms}`` 序列——``fd`` 1/2 上裸
    ``os.write``（无缓冲、写满才返回）、``repeat`` 重复 data、``delay_ms`` 先睡；
  - ``sleep_s``：chunks 之后滞留（模拟长驻流 / 超时场景）；
  - ``exit_code``：退出码（可为非零）。

一个纯 stdlib 短命 python 进程：零网络、零设备、不执行任何真实 hdc 命令。
"""

from __future__ import annotations

import json
import os
import sys
import time

#: 剧本 JSON 路径的环境变量名（测试经它注入，不经 PATH、不经 argv）。
ENV_SCENE = "N1B_FAKE_HDC_EXEC_SCENE"


def _write_all(fd: int, data: str) -> None:
    """裸写整个 data 到 fd（os.write 可能部分写，循环写满为止）。"""
    raw = data.encode("utf-8")
    while raw:
        raw = raw[os.write(fd, raw):]


def _load_scene() -> dict:
    path = os.environ.get(ENV_SCENE)
    if not path:
        return {}
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def _match(rule: dict, argv: list) -> bool:
    pattern = [str(t) for t in rule.get("match", ())]
    return argv[:len(pattern)] == pattern


def _run(rule: dict, argv: list) -> int:
    if rule.get("echo_argv"):
        _write_all(1, json.dumps(argv, ensure_ascii=False) + "\n")
    for chunk in rule.get("chunks", ()):
        delay_ms = chunk.get("delay_ms")
        if delay_ms:
            time.sleep(float(delay_ms) / 1000.0)
        data = str(chunk.get("data", ""))
        repeat = int(chunk.get("repeat", 0) or 0)
        if repeat > 1:
            data = data * repeat
        if data:
            _write_all(int(chunk.get("fd", 1)), data)
    sleep_s = float(rule.get("sleep_s", 0) or 0)
    if sleep_s > 0:
        time.sleep(sleep_s)
    return int(rule.get("exit_code", 0))


def main() -> int:
    argv = [str(t) for t in sys.argv[1:]]
    scene = _load_scene()
    for rule in scene.get("rules", ()):
        if _match(rule, argv):
            return _run(rule, argv)
    _write_all(1, json.dumps(argv, ensure_ascii=False) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
