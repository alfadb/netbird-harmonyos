#!/usr/bin/env python3
# Diagnostic (read-only) verification of the freshly built unsigned HAP.
#
# This is NOT a gate/authorization record: it only reads the artifacts produced
# by `DEVECO_SDK_HOME=<CLT> bash client/build.sh` and prints machine-readable
# facts. Method follows the project's existing caliber (client/build.sh step 4):
#   * the packaged member sha256 == the llvm-strip'd copy of the module .so
#     (hvigor's DoNativeStrip strips prebuilt .so members during packaging);
#   * member is ELF64 little-endian ET_DYN AArch64;
#   * pack.info apiVersion target/compatible == 26.
# Additional (this record): the raw unstripped .so hash, the new NAPI entry
# string literals inside the packaged member, and the build self-identification
# digest recomputed independently from the working tree (core/build.sh step 7).

import hashlib
import json
import os
import struct
import subprocess
import sys
import tempfile
import zipfile

CLT = "/home/worker/harmonyos/command-line-tools/current"
REPO = "/home/worker/work/base/netbird-harmonyos"
LLVM_BIN = os.path.join(CLT, "sdk/default/openharmony/native/llvm/bin")

HAP = os.path.join(REPO, "client/entry/build/default/outputs/default/entry-default-unsigned.hap")
SO_RAW = os.path.join(REPO, "client/entry/libs/arm64-v8a/libnetbird_core.so")
SO_CORE = os.path.join(REPO, "client/core/target/aarch64-unknown-linux-ohos/release/libnetbird_core.so")
MEMBER = "libs/arm64-v8a/libnetbird_core.so"

NEW_ENTRIES = ["n2_cd_switch_identified", "n2_ring_ack"]
# Named refusal / answer tokens that only the new n2_ring.rs parser and
# n2_cd.rs switch path define (proves the module logic is in the packaged bytes,
# not just the registration string).
NEW_TOKENS = ["session-mismatch", "seq-beyond-tail", "not-armed", "not-configured",
              "invalid-session", "switch-identity-required"]


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def elf_facts(data):
    facts = {}
    facts["elf_magic_ok"] = data[:4] == b"\x7fELF"
    facts["ei_class"] = data[4]          # 2 = ELF64
    facts["ei_data"] = data[5]           # 1 = little-endian
    etype, machine = struct.unpack_from("<HH", data, 16)
    facts["e_type"] = etype              # 3 = ET_DYN
    facts["e_machine"] = machine         # 183 = AArch64
    facts["is_elf64_le_dyn_aarch64"] = (
        facts["elf_magic_ok"] and facts["ei_class"] == 2 and facts["ei_data"] == 1
        and etype == 3 and machine == 183)
    return facts


def recompute_build_id():
    """Independent shell-equivalent recomputation of core/build.sh step 7."""
    header = b"n2h-build-id-v1\n"
    files = ["Cargo.toml", "Cargo.lock", "build.rs"]
    for root, dirs, names in os.walk(os.path.join(REPO, "client/core/proto")):
        del dirs
        for n in names:
            files.append(os.path.relpath(os.path.join(root, n), os.path.join(REPO, "client/core")))
    for root, dirs, names in os.walk(os.path.join(REPO, "client/core/src")):
        del dirs
        for n in names:
            files.append(os.path.relpath(os.path.join(root, n), os.path.join(REPO, "client/core")))
    files = sorted(set(files), key=lambda s: s.encode())
    h = hashlib.sha256()
    h.update(header)
    for rel in files:
        h.update(rel.encode() + b"\n")
        with open(os.path.join(REPO, "client/core", rel), "rb") as f:
            h.update(f.read())
        h.update(b"\0")
    return h.hexdigest()


def llvm(prog, *args):
    return subprocess.run([os.path.join(LLVM_BIN, prog), *args],
                          capture_output=True, text=True, check=False)


def main():
    out = {"source_kind": "diagnostic-host-build", "is_evidence": False}
    problems = []

    out["hap"] = {"path": HAP, "exists": os.path.isfile(HAP)}
    for p in (HAP, SO_RAW, SO_CORE):
        if not os.path.isfile(p):
            problems.append(f"missing artifact: {p}")
    out["hap"]["sha256"] = sha256_file(HAP)
    out["hap"]["bytes"] = os.path.getsize(HAP)
    out["hap"]["mtime"] = subprocess.run(["stat", "-c", "%y", HAP], capture_output=True,
                                         text=True).stdout.strip()

    out["so_module_raw"] = {"path": SO_RAW, "sha256": sha256_file(SO_RAW),
                            "bytes": os.path.getsize(SO_RAW)}
    out["so_core_release"] = {"path": SO_CORE, "sha256": sha256_file(SO_CORE),
                              "bytes": os.path.getsize(SO_CORE)}
    out["so_copy_identical_to_core_release"] = (
        out["so_module_raw"]["sha256"] == out["so_core_release"]["sha256"])

    raw = open(SO_RAW, "rb").read()
    out["so_module_raw"]["elf"] = elf_facts(raw)
    if not out["so_module_raw"]["elf"]["is_elf64_le_dyn_aarch64"]:
        problems.append("raw module .so is not ELF64 LE ET_DYN AArch64")

    # llvm-readelf evidence on the raw artifact (same tool the core build uses).
    rh = llvm("llvm-readelf", "-h", SO_RAW).stdout
    out["so_module_raw"]["llvm_readelf_h"] = [
        line.strip() for line in rh.splitlines()
        if any(k in line for k in ("Class:", "Data:", "Type:", "Machine:"))]
    needed = llvm("llvm-readelf", "-d", SO_RAW).stdout
    out["so_module_raw"]["needed"] = sorted({line.split("[")[1].split("]")[0]
                                             for line in needed.splitlines() if "NEEDED" in line})
    if "libc.so.6" in out["so_module_raw"]["needed"]:
        problems.append("raw module .so NEEDED contains libc.so.6 (glibc)")

    # Packaged member identity (project caliber).
    with zipfile.ZipFile(HAP) as z:
        names = z.namelist()
        out["hap"]["member_present"] = MEMBER in names
        if MEMBER not in names:
            problems.append(f"{MEMBER} not in HAP")
            member = b""
        else:
            member = z.read(MEMBER)
        out["hap"]["member_names_sample"] = [n for n in names if n.startswith("libs/") or
                                             n in ("pack.info", "module.json", "ets/modules.abc")]
        out["hap"]["signature_entries"] = [n for n in names if n.startswith("META-INF/")]
        pack = json.loads(z.read("pack.info").decode("utf-8"))
        out["pack_info"] = pack
        api = pack["summary"]["modules"][0].get("apiVersion", {})
        out["hap"]["apiVersion"] = api
        if api.get("target") != 26 or api.get("compatible") != 26:
            problems.append(f"pack.info apiVersion != 26: {api}")
        if out["hap"]["signature_entries"]:
            problems.append("HAP unexpectedly contains META-INF/ signature entries")

    out["hap"]["member_sha256"] = sha256_bytes(member)
    out["hap"]["member_bytes"] = len(member)
    with tempfile.TemporaryDirectory() as td:
        stripped = os.path.join(td, "stripped.so")
        with open(stripped, "wb") as f:
            f.write(raw)
        llvm("llvm-strip", stripped)
        out["so_module_llvm_stripped"] = {"sha256": sha256_file(stripped),
                                          "bytes": os.path.getsize(stripped)}
    out["member_matches_llvm_stripped_module_so"] = (
        out["hap"]["member_sha256"] == out["so_module_llvm_stripped"]["sha256"])
    if not out["member_matches_llvm_stripped_module_so"]:
        problems.append("packaged member sha256 != llvm-strip'd module .so sha256")
    out["hap"]["member_elf"] = elf_facts(member)
    if not out["hap"]["member_elf"]["is_elf64_le_dyn_aarch64"]:
        problems.append("packaged member is not ELF64 LE ET_DYN AArch64")

    # New NAPI entry surface inside the packaged bytes.
    out["new_entry_strings"] = {name: (name.encode() in member) for name in NEW_ENTRIES}
    out["new_token_strings"] = {tok: (tok.encode() in member) for tok in NEW_TOKENS}
    for k, v in {**out["new_entry_strings"], **out["new_token_strings"]}.items():
        if not v:
            problems.append(f"string not found in packaged member: {k}")

    # The compiled ArkTS bytecode packaged in the same HAP: the shell-side
    # channels of the same two entries must be inside ets/modules.abc (this is
    # the "build input inclusion" evidence for the .ets side).
    with zipfile.ZipFile(HAP) as z:
        abc = z.read("ets/modules.abc") if "ets/modules.abc" in z.namelist() else b""
        module_json = json.loads(z.read("module.json").decode("utf-8")) if "module.json" in z.namelist() else {}
    out["arkts_abc"] = {"bytes": len(abc),
                        "n2_cd_switch_identified": b"n2_cd_switch_identified" in abc,
                        "n2_ring_ack": b"n2_ring_ack" in abc,
                        "N2_RING_ACK_PARAM": b"N2_RING_ACK_PARAM" in abc,
                        "N2_RING_ACK_SHELL": b"N2_RING_ACK_SHELL" in abc,
                        "n2_cd_identity": b"n2_cd_identity" in abc}
    for k in ("n2_cd_switch_identified", "n2_ring_ack", "N2_RING_ACK_PARAM", "N2_RING_ACK_SHELL"):
        if not out["arkts_abc"][k]:
            problems.append(f"ArkTS bytecode missing shell channel string: {k}")
    out["module_json"] = {
        "module": module_json.get("module", {}).get("name"),
        "type": module_json.get("module", {}).get("type"),
        "mainElement": module_json.get("module", {}).get("mainElement"),
        "extensionAbilities": [e.get("name") for e in
                               module_json.get("module", {}).get("extensionAbilities", [])],
        "app": module_json.get("app"),
    }

    # Build self-identification (I7) recomputed from the working tree.
    ident = recompute_build_id()
    out["build_self_identification"] = {
        "n2h_build_sha256_recomputed": ident,
        "present_in_raw_so": ident.encode() in raw,
        "present_in_packaged_member": ident.encode() in member,
    }
    if not out["build_self_identification"]["present_in_raw_so"]:
        problems.append("N2H_BUILD_SHA256 not found in raw .so")

    out["result"] = "pass" if not problems else "fail"
    out["problems"] = problems
    print(json.dumps(out, indent=2, ensure_ascii=False))
    return 0 if not problems else 1


if __name__ == "__main__":
    sys.exit(main())
