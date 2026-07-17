# R1 API 24 HAP Probe

This directory contains short-lived HarmonyOS R1 research probes for the API 24 x86_64 TLS loader boundary. It must not evolve into a product shell.

## Current Scope

- Builds one API 24 Stage application HAP and one `entry_test` HAP with bundle name `cn.alfadb.netbird.r1probe`.
- Retains the Tier1 pure-C TLS sources and records as immutable 0009 evidence; `runDynamicTlsProbe` remains available for that scope.
- The current Tier2 Runner calls `runGoProbe`, which creates one waiting C pthread before `dlopen("libgoprobe.so", RTLD_NOW | RTLD_LOCAL)` and one C pthread after a successful load. Each thread performs its own `dlsym` and requires `Hello=42`, goroutine/channel/timer/allocation `RuntimeProbe=4194304`, and loopback `NetDialProbe=0`.
- `go-probe/build.sh` requires an explicit Go launcher and checks the reported version prefix. Its default `GOTOOLCHAIN=local` prevents a supplied candidate launcher from silently selecting a cached stock toolchain.
- The measured 0010 stock control uses Go 1.25.12 and retains `TPOFF64` plus `STATIC_TLS`; direct PS4 `5f5911fabb3af7b5662ebc17ff7fa4f881df903a` emits TLSDESC with no `TPOFF` or `STATIC_TLS` and passed ten fresh TestRunner processes.
- The only approved official-Go follow-up was a mechanical `git apply --3way` of the unmodified PS4 diff to Go 1.25.12. It produced conflicts and is stopped as high-maintenance; no semantic Go edit, extra runtime or argv repair, third iteration, NetBird patch, SDK patch, or product conclusion is authorized.
- Contains no NetBird, TUN, VPN, DNS lookup, public-network operation, Native Child invocation, Hypium, signing material, or committed Go source patch.

Generated Go libraries, headers, HAPs, and native inputs are ignored. The evidence record for the completed Tier2 stop is `EV-R1-EMU24-20260717-0010`.

## Native Probe Build

Set fixed tool and output paths explicitly; do not use the floating `current` symlink.

```bash
cd /home/worker/work/base/netbird-harmonyos/spikes/r1-api24-hap
HARMONYOS_NATIVE_HOME=/home/worker/harmonyos/command-line-tools/6.1.1.290/sdk/default/openharmony/native \
NATIVE_PROBE_OUTPUT_DIR=/home/worker/work/base/netbird-harmonyos/spikes/r1-api24-hap/entry/libs/x86_64 \
./native-probes/build.sh
```

`HOST_CC` may override `/usr/bin/gcc`, but the measured 0009 inputs use GCC 14.2.0. The generated shared objects and temporary object directory are ignored; the script removes its temporary object directory on exit.

## ELF Verification

```bash
cd /home/worker/work/base/netbird-harmonyos/spikes/r1-api24-hap
for so in entry/libs/x86_64/libtls-{ie,gd,desc,ld}.so; do
  file "$so"
  readelf -lWrdsW "$so"
  /home/worker/harmonyos/command-line-tools/6.1.1.290/sdk/default/openharmony/native/llvm/bin/llvm-objdump -dr --no-show-raw-insn "$so"
done
```

The final model requirements are strict:

- `libtls-ie.so`: `PT_TLS`, `R_X86_64_TPOFF64`, and `STATIC_TLS` must exist.
- `libtls-gd.so`: `R_X86_64_DTPMOD64`, `R_X86_64_DTPOFF64`, and classic `__tls_get_addr` calls must exist; `TPOFF`, `TLSDESC`, and `STATIC_TLS` must be absent.
- `libtls-desc.so`: final `R_X86_64_TLSDESC` and descriptor calls must exist; `TPOFF`, classic `__tls_get_addr`, and `STATIC_TLS` must be absent.
- `libtls-ld.so`: final `R_X86_64_DTPMOD64` and `__tls_get_addr` calls must exist; `TPOFF`, `TLSDESC`, and `STATIC_TLS` must be absent.

`EV-R1-EMU24-20260717-0009` met all four requirements. The exact input SHA-256 values are IE `b8dee0046cc580339b8a09a79673c81bc75218f90819fdd9069b6b82fb08a674`, GD `c00f4b8861f8c7598f76851cf6d4b90d95a0b03e6e51f58da5213586f1480946`, TLSDESC `336357920dd412db0613d88d223229751f31f3e34cd2b85f051162f75e614834`, and LD `093fa560431152d21a0323fe9f96bc49edfec7a9983e69cc19e95461f54bec54`.

## Cold HAP Build

Run one clean and then build both unsigned HAPs.

```bash
cd /home/worker/work/base/netbird-harmonyos/spikes/r1-api24-hap
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/ohpm install --all
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw clean --mode module -p product=default -p module=entry@default -p buildMode=debug --no-daemon
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap --mode module -p product=default -p module=entry@default -p buildMode=debug --no-daemon
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap --mode module -p product=default -p module=entry@ohosTest -p buildMode=debug --no-daemon
```

The expected outputs are `entry/build/default/outputs/default/entry-default-unsigned.hap` and `entry/build/default/outputs/ohosTest/entry-ohosTest-unsigned.hap`. The build intentionally has no signing configuration.

The module profile excludes only the four measured TLS inputs from native stripping. Both HAPs must contain all four inputs byte-for-byte, the application HAP `libprobe.so` member must equal the final Hvigor stripped output, and neither HAP may contain historical Go or needed-wrapper objects.

## Emulator Run

Use Beta HDC 3.2.0e for every target operation and install both HAPs together.

```bash
HDC=/home/worker/harmonyos/command-line-tools/26.0.0.461/sdk/default/openharmony/toolchains/hdc
TARGET=127.0.0.1:10000
REMOTE=/data/local/tmp/r1-dynamic-tls-0009
APP_HAP=/home/worker/work/base/netbird-harmonyos/spikes/r1-api24-hap/entry/build/default/outputs/default/entry-default-unsigned.hap
TEST_HAP=/home/worker/work/base/netbird-harmonyos/spikes/r1-api24-hap/entry/build/default/outputs/ohosTest/entry-ohosTest-unsigned.hap
"$HDC" -t "$TARGET" shell rm -rf "$REMOTE"
"$HDC" -t "$TARGET" shell mkdir "$REMOTE"
"$HDC" -t "$TARGET" file send "$APP_HAP" "$REMOTE"
"$HDC" -t "$TARGET" file send "$TEST_HAP" "$REMOTE"
"$HDC" -t "$TARGET" shell bm install -p "$REMOTE"
"$HDC" -t "$TARGET" shell hilog -r
"$HDC" -t "$TARGET" shell aa test -b cn.alfadb.netbird.r1probe -m entry_test -s unittest /ets/testrunner/OpenHarmonyTestRunner -s timeout 15000
"$HDC" -t "$TARGET" shell hilog -x -t app -v year -v zone
```

`aa test` can return host code 0 independently of the framework verdict. Require `BASELINE_RESULT|functional=PASS`, expected IE `BLOCKED` with no environment drift, `TestFinished-ResultCode: 0`, and aggregate `TLS_SUITE_RESULT|verdict=PASS`.

Clean guest and bundle state after capture.

```bash
"$HDC" -t "$TARGET" shell rm -rf "$REMOTE"
"$HDC" -t "$TARGET" shell aa force-stop cn.alfadb.netbird.r1probe
"$HDC" -t "$TARGET" uninstall cn.alfadb.netbird.r1probe
```

## Measured Result

`EV-R1-EMU24-20260717-0009` produced baseline `PASS`, expected IE loader `BLOCKED`, classic GD `PASS`, TLSDESC `PASS`, and local-dynamic `PASS`; Tier1 is `PASS` because both gating dynamic models passed.

| Model | Final ELF identity | Loader | Main thread | Pre-dlopen thread | Post-dlopen thread | Gate |
| --- | --- | --- | --- | --- | --- | --- |
| IE | `PT_TLS` + `TPOFF64` + `STATIC_TLS` | Expected rejection | Not called | Waiting before rejection | Not created | Control passed |
| Classic GD | `DTPMOD64` + `DTPOFF64` + `__tls_get_addr` | PASS | `42`, value `2001`, 100, reset `42` | `42`, value `2002`, 100, reset `42` | `42`, value `2003`, 100, reset `42` | PASS |
| TLSDESC gnu2 | `R_X86_64_TLSDESC` | PASS | `42`, value `3001`, 100, reset `42` | `42`, value `3002`, 100, reset `42` | `42`, value `3003`, 100, reset `42` | PASS |
| Local dynamic | `DTPMOD64` + `__tls_get_addr` | PASS | `42`, value `4001`, 100, reset `42` | `42`, value `4002`, 100, reset `42` | `42`, value `4003`, 100, reset `42` | Non-gating PASS |

No thread error, cross-thread value leak, unexpected IE load, or process crash occurred. The complete app buffer records Runner PID 2950 exiting normally after `finishTest`.

For successful `dlopen` calls the structured output retains an ambient `loaderErrno=22` while `loaderError` is empty; errno is undefined after successful `dlopen` and is not a failure field or verdict input. The IE failure has a nonempty loader error and meaningful errno 2.

## Evidence

- Replayable transcript: `docs/evidence/raw/EV-R1-EMU24-20260717-0009-dynamic-tls-loader.log`, SHA-256 `51f86fa723383288f96eaf604e3d3691f95464aee8fbfd4257ff8dd112f725cc`.
- Emulator console: `docs/evidence/raw/EV-R1-EMU24-20260717-0009-emulator-console.log`, SHA-256 `6ee2d8cd6e97a07465ae87a9a9a68656dad6ca9f4dba7746bbd9343e2b20d3b5`.
- Full unfiltered app HiLog: `docs/evidence/raw/EV-R1-EMU24-20260717-0009-hilog-app-full.log.gz.base64`, SHA-256 `e44b94bda8e4465ab6135d0c8db3cb81f9940d99b36c84cf4b31aacb15bcba5d`; decoded raw SHA-256 `010db21b82de5e0bcc8e4722c2d94f96c2ed0638068b813c8a977b2b7d1522b1`.

Decode the full HiLog without filtering or content changes:

```bash
base64 -d docs/evidence/raw/EV-R1-EMU24-20260717-0009-hilog-app-full.log.gz.base64 | gzip -dc
```

## Boundary And Next Gate

Go issue `#71953`, Go CL `644975`, Go CL `696635`, and Go PR `75048` were all still open or `NEW`, unmerged, and unreleased when 0009 was recorded. They are references for the next separately authorized Tier2 Go/toolchain feasibility gate only; none is an input, patch, merged baseline, or released capability in this probe.

This PASS proves only that the exact API 24 x86_64 TestRunner loader and libc path accept these verified C dynamic TLS models for the measured late-load and thread lifecycle. It does not prove that Go 1.25.12 emits or supports these models, does not authorize a Go/runtime/toolchain patch, and does not validate arm64, a named physical device, commercial Huawei HarmonyOS, NetBird, VPN, signing, channel acceptance, product support, or stage exit.

R0, R1, and R2 remain unexited; arm64 and named physical devices remain provisional; upstream adaptation patch count remains zero.
