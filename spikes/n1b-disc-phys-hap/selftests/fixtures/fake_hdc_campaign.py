#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""fake_hdc_campaign — 端到端测试夹具：按 argv + 状态文件模拟命令生命周期的假 hdc。

只被 ``selftests/test_engine.py`` 经 ``RealHdcTransport`` 当作 ``command`` 绝对
路径 spawn（run_campaign 端到端用例）——绝不指向真实 hdc，不改 PATH，无网络/
设备/签名。纯 stdlib python 进程（一次性命令短命；hilog 长驻或按剧本退出）。

注入（全部经环境变量，不经 PATH）：

- ``N1B_FAKE_HDC_CAMPAIGN_STATE``：JSON 状态文件路径（bundle 生命周期：staged /
  hap_sent / installed / started / ui_alive / vpn_alive / vpn_pid）；
- ``N1B_FAKE_HDC_CAMPAIGN_SCENE``：JSON 剧本（bundle / model / sw / vpn_pid /
  ui_pid / staging_root（冻结字面，仅作 argv 校验）/ staging_sandbox（本地替代
  目录——**冻结设备路径绝不真实创建/删除**）/ fault_dir（沙箱内 faultlogger 替
  代目录）/ crash_files: {name: text}——进程死亡时物化的条目 / stream:
  [{after_ms, line}] / die_after_lines: bool / fail_ops: [op 名]——命中即非零
  退出（CLI 单次性/中止路径注入；无重试面））。

命令语义（模拟真实 hdc 子命令形态；与 runner 白名单表一一对应）：

- ``version`` / ``param get ...``：探针字面输出；
- ``mkdir -p`` / ``file send`` / ``bm install -p``：沙箱 staging 推进（前序缺失
  → 非零退出，模拟真机失败面）；
- ``aa start``：installed → started + ui/vpn 存活（vpn_pid 入状态）；
- ``hilog -T ...``：长驻流——按 ``after_ms``（相对开流时刻）逐行输出剧本行
  （逐行 flush），随后按剧本三态：``die_after_lines`` = 进程死亡模拟（vpn/ui
  转 absent、crash 条目物化、退出码 0 = 干净 EOF）/ ``die_after_lines_keep_
  stream`` = 进程死亡但流长驻（真实设备形态：hilog 不随应用死亡，捕获由
  runner 时间盒收口）/ 缺省长驻（sleep 到被 SIGKILL）；
- ``pidof <bundle>[:vpn]``：存活 → pid 字面 / absent → 空输出（exit 0）；
- ``find <faultlogger> ... -print``：列出 fault_dir 内命中文件；
- ``file recv <remote> <host>``：条目内容写到 host 路径（真实 file recv 语义）；
- ``aa force-stop``：vpn/ui 转 absent + crash 条目物化；
- ``bm uninstall`` / ``rm -rf`` / ``ls -ld`` / ``bm dump``：清理与 absent 探针。

状态每次变更原子重写（临时文件 + os.replace）；全部副作用限制在测试沙箱目录。
"""

from __future__ import annotations

import json
import os
import sys
import time

ENV_STATE = "N1B_FAKE_HDC_CAMPAIGN_STATE"
ENV_SCENE = "N1B_FAKE_HDC_CAMPAIGN_SCENE"


def _write_all(data: str) -> None:
    raw = data.encode("utf-8")
    while raw:
        raw = raw[sys.stdout.buffer.write(raw):]
    sys.stdout.buffer.flush()


def _load_json(name: str) -> dict:
    path = os.environ.get(name)
    if not path or not os.path.exists(path):
        return {}
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def _save_state(state: dict) -> None:
    path = os.environ.get(ENV_STATE)
    if not path:
        return
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(state, fh, ensure_ascii=False, sort_keys=True)
    os.replace(tmp, path)


def _staging_root(scene: dict) -> str:
    return scene.get("staging_root", "/data/local/tmp/netbird-n1bdisc")


def _sandbox(scene: dict) -> str:
    return scene["staging_sandbox"]


def _fault_dir(scene: dict) -> str:
    return scene.get("fault_dir", os.path.join(_sandbox(scene), "faultlogger"))


def _materialize_crash_files(scene: dict) -> None:
    """进程死亡：crash 条目物化进 fault 目录（快照差分可见的新增文件）。"""
    fault_dir = _fault_dir(scene)
    os.makedirs(fault_dir, exist_ok=True)
    for name, text in (scene.get("crash_files") or {}).items():
        with open(os.path.join(fault_dir, name), "w", encoding="utf-8") as fh:
            fh.write(text)


def _mutate_vpn(state: dict, scene: dict, alive: bool) -> None:
    state["vpn_alive"] = alive
    state["ui_alive"] = alive
    if not alive:
        _materialize_crash_files(scene)
    _save_state(state)


def _fail(message: str) -> int:
    sys.stderr.write(message + "\n")
    return 1


def _match_op(core: list, bundle: str) -> str:
    """core argv → runner 白名单操作名（fixture 侧最小前缀判定；仅 fail_ops 用）。"""
    if core == ["version"]:
        return "Version"
    if len(core) >= 4 and core[:3] == ["shell", "param", "get"]:
        return "ParamModel" if core[3] == "const.product.model" \
            else "ParamSoftwareVersion"
    if len(core) >= 2 and core[:2] == ["shell", "mkdir"]:
        return "MkdirStaging"
    if len(core) >= 2 and core[:2] == ["file", "send"]:
        return "SendHap"
    if len(core) >= 2 and core[:2] == ["shell", "bm"] and "install" in core:
        return "InstallHap"
    if len(core) >= 3 and core[:2] == ["shell", "aa"] and core[2] == "start":
        return "StartEntry"
    if len(core) >= 2 and core[:2] == ["shell", "hilog"]:
        return "HilogStream"
    if len(core) >= 3 and core[:2] == ["shell", "pidof"]:
        return "PidOfVpn" if core[2] == bundle + ":vpn" else "PidOf"
    if len(core) >= 2 and core[:2] == ["shell", "find"]:
        return "FaultProbe"
    if len(core) >= 2 and core[:2] == ["file", "recv"]:
        return "FaultRecv"
    if len(core) >= 3 and core[:2] == ["shell", "aa"] and core[2] == "force-stop":
        return "ForceStop"
    if len(core) >= 2 and core[:2] == ["shell", "bm"] and "uninstall" in core:
        return "Uninstall"
    if len(core) >= 3 and core[:2] == ["shell", "rm"] and "-rf" in core:
        return "RemoveStaging"
    if len(core) >= 2 and core[:2] == ["shell", "ls"] and "-ld" in core:
        return "StagingProbe"
    if len(core) >= 2 and core[:2] == ["shell", "bm"] and "dump" in core:
        return "BundleDump"
    return ""


def _run_stream(scene: dict) -> int:
    """长驻 hilog 流：相对开流时刻按 after_ms 逐行输出；终态按剧本三态。

    ``die_after_lines`` = 行尽即进程死亡 + 干净 EOF（exit 0）；``die_after_lines_
    keep_stream`` = 进程死亡但流长驻（真实设备形态：hilog 不随应用死亡）——
    捕获由 runner 时间盒收口；两者皆否 = 纯长驻。
    """
    start = time.monotonic()
    lines = sorted(scene.get("stream", ()), key=lambda e: e.get("after_ms", 0))
    for entry in lines:
        delay = float(entry.get("after_ms", 0)) / 1000.0 - (time.monotonic() - start)
        if delay > 0:
            time.sleep(delay)
        _write_all(str(entry.get("line", "")) + "\n")
    if scene.get("die_after_lines") or scene.get("die_after_lines_keep_stream"):
        state = _load_json(ENV_STATE)
        _mutate_vpn(state, scene, alive=False)   # 探针进程死亡（absent + 物化）
        if scene.get("die_after_lines"):
            return 0                              # 干净 EOF（exit 0）
    time.sleep(600)                               # 长驻：由 runner 停流回收
    return 0


def main() -> int:
    argv = [str(t) for t in sys.argv[1:]]
    scene = _load_json(ENV_SCENE)
    state = _load_json(ENV_STATE)
    bundle = scene.get("bundle", "cn.alfadb.netbird.n1bdisc")
    staging = _staging_root(scene)
    sandbox = _sandbox(scene)
    hap_remote = staging + "/hap/n1bdisc.hap"
    hap_dir_remote = staging + "/hap"

    # 真实 hdc 形态：除 `version` 外均带 `-t <target>` 前缀——剥掉前缀后再匹配。
    core = argv[2:] if len(argv) >= 2 and argv[0] == "-t" else argv

    # fail_ops 注入（CLI 单次性/中止路径测试辅助）：命中操作名即非零退出，
    # 不推进任何生命周期状态（无副作用、无重试面）。
    fail_ops = scene.get("fail_ops") or ()
    matched_op = _match_op(core, bundle)
    if matched_op and matched_op in fail_ops:
        return _fail("injected failure: %s" % matched_op)

    if argv == ["version"]:
        _write_all("FakeHdc campaign fixture 1.0\n")
        return 0
    if len(core) >= 3 and core[:3] == ["shell", "param", "get"]:
        key = core[3]
        if key == "const.product.model":
            _write_all(scene.get("model", "FAKE-MODEL-CAMPAIGN") + "\n")
            return 0
        if key == "const.product.software.version":
            _write_all(scene.get("sw", "7.0.0.999") + "\n")
            return 0
        return _fail("unknown param key")
    if len(core) >= 4 and core[:2] == ["shell", "mkdir"] and "-p" in core:
        path = core[core.index("-p") + 1]
        if path != hap_dir_remote:
            return _fail("mkdir: unexpected path")
        os.makedirs(os.path.join(sandbox, "hap"), exist_ok=True)
        state["staged"] = True
        _save_state(state)
        return 0
    if len(core) == 4 and core[0] == "file" and core[1] == "send":
        if core[3] != hap_remote:
            return _fail("file send: unexpected remote path")
        if not state.get("staged"):
            return _fail("file send failed: no staging dir")
        if not os.path.isfile(core[2]):
            return _fail("file send failed: hap missing on host")
        with open(core[2], "rb") as src, \
                open(os.path.join(sandbox, "hap", "n1bdisc.hap"), "wb") as dst:
            dst.write(src.read())
        state["hap_sent"] = True
        _save_state(state)
        _write_all("FileTransfer finish\n")
        return 0
    if len(core) >= 2 and core[0] == "shell" and core[1] == "bm" \
            and "install" in core:
        if hap_dir_remote not in core:
            return _fail("bm install: unexpected path")
        if not state.get("hap_sent") or \
                not os.path.isfile(os.path.join(sandbox, "hap", "n1bdisc.hap")):
            return _fail("bm install failed: hap missing")
        state["installed"] = True
        _save_state(state)
        _write_all("install bundle successfully\n")
        return 0
    if len(core) >= 3 and core[:2] == ["shell", "aa"] and core[2] == "start":
        if not state.get("installed"):
            return _fail("aa start failed: not installed")
        state["started"] = True
        state["ui_alive"] = True
        state["vpn_alive"] = True
        state["vpn_pid"] = int(scene.get("vpn_pid", 24567))
        _save_state(state)
        _write_all("start ability successfully\n")
        return 0
    if len(core) >= 2 and core[0] == "shell" and core[1] == "hilog":
        return _run_stream(scene)
    if len(core) >= 3 and core[:2] == ["shell", "pidof"]:
        name = core[2]
        if name == bundle + ":vpn":
            if state.get("vpn_alive"):
                _write_all(str(state.get("vpn_pid", 24567)) + "\n")
            return 0
        if name == bundle:
            if state.get("ui_alive"):
                _write_all(str(scene.get("ui_pid", 24566)) + "\n")
            return 0
        return _fail("pidof: unknown process name")
    if len(core) >= 2 and core[0] == "shell" and core[1] == "find":
        fault_dir = _fault_dir(scene)
        names = sorted(os.listdir(fault_dir)) if os.path.isdir(fault_dir) else []
        for name in names:
            if bundle in name and os.path.isfile(os.path.join(fault_dir, name)):
                _write_all(os.path.join(fault_dir, name) + "\n")
        return 0
    if len(core) == 4 and core[0] == "file" and core[1] == "recv":
        name = core[2].rsplit("/", 1)[-1]
        source = os.path.join(_fault_dir(scene), name)
        if not os.path.isfile(source):
            return _fail("file recv failed: %s" % name)
        with open(source, "rb") as fh, open(core[3], "wb") as dst:
            dst.write(fh.read())
        return 0
    if len(core) >= 3 and core[:2] == ["shell", "aa"] \
            and core[2] == "force-stop":
        _mutate_vpn(state, scene, alive=False)
        return 0
    if len(core) >= 2 and core[0] == "shell" and core[1] == "bm" \
            and "uninstall" in core:
        state["installed"] = False
        _save_state(state)
        return 0
    if len(core) >= 3 and core[0] == "shell" and core[1] == "rm" \
            and "-rf" in core:
        targets = core[core.index("-rf") + 1:]
        if targets != [staging]:
            return _fail("rm: unexpected path")
        if os.path.isdir(sandbox):
            for root, dirs, files in os.walk(sandbox, topdown=False):
                for name in files:
                    os.remove(os.path.join(root, name))
                for name in dirs:
                    os.rmdir(os.path.join(root, name))
            os.rmdir(sandbox)
        state["staged"] = False
        state["hap_sent"] = False
        _save_state(state)
        return 0
    if len(core) >= 3 and core[0] == "shell" and core[1] == "ls" \
            and "-ld" in core:
        if core[core.index("-ld") + 1] != staging:
            return _fail("ls: unexpected path")
        if os.path.isdir(sandbox):
            _write_all("drwxrwxrwx ... %s\n" % staging)
            return 0
        return _fail("ls: %s: No such file or directory" % staging)
    if len(core) >= 2 and core[0] == "shell" and core[1] == "bm" \
            and "dump" in core:
        if state.get("installed"):
            _write_all("BundleName: %s\nAppStates: IS_INSTALLED=true\n" % bundle)
            return 0
        return _fail("bm dump failed: not installed")
    return _fail("unknown command argv")


if __name__ == "__main__":
    sys.exit(main())
