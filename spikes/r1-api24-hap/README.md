# E0 API 24 HAP Probe

This directory is a short-lived HarmonyOS research probe for the API 24 x86_64 Emulator gates. It must not evolve into a product shell.

## Current Scope

The current build graph is the E0 ordinary-application probe:

- one API 24 Stage application HAP and one `entry_test` sidecar HAP;
- bundle `cn.alfadb.netbird.r1probe` and ordinary `EntryAbility`;
- minimal C++ Node-API `ping()` and `version()` implementation in `e0_probe.cpp`;
- visible ArkUI text `E0 API 24` and `EntryAbility running`;
- lifecycle HiLog for `onCreate`, `onWindowStageCreate`, and `onForeground`;
- three bounded cold starts, screenshots, force-stop, sidecar baseline, uninstall, and host cleanup.

The current HAPs contain no NetBird, Go runtime, TUN, VPN, DNS probe, Native Child invocation, `libgoprobe.so`, or `libtls-*` input. The current CMake graph does not reference historical 0010 `probe.cpp`; it has been removed from the current source tree. Historical reproduction must check out the corresponding historical commit and must not retain old source as a current fallback. See [the R1 evidence record](../../docs/evidence/r1-go-abi-preflight-2026-07-16.md) for those completed experiments.

Generated native libraries, HAPs, and build state are ignored. There is intentionally no signing configuration.

## Clean Build

Use fixed tool paths rather than the floating `current` symlink:

```bash
cd /home/worker/work/base/netbird-harmonyos/spikes/r1-api24-hap
rm -rf entry/libs/x86_64
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw clean \
  --mode module -p product=default -p module=entry@default \
  -p buildMode=debug --no-daemon
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap \
  --mode module -p product=default -p module=entry@default \
  -p buildMode=debug --no-daemon
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap \
  --mode module -p product=default -p module=entry@ohosTest \
  -p buildMode=debug --no-daemon
```

Expected outputs:

```text
entry/build/default/outputs/default/entry-default-unsigned.hap
entry/build/default/outputs/ohosTest/entry-ohosTest-unsigned.hap
```

Hvigor's unsigned packaging is not byte-for-byte reproducible for identical source in this environment. Record each HAP SHA-256 before installation and never infer artifact identity from source identity alone.

## Emulator Run

The runner starts and stops the visible `netbird_api24_phone` instance itself. It requires X11 display `:1`, Beta HDC 3.2.0e, the fixed Emulator image, `ffmpeg`, and both prebuilt HAPs:

```bash
cd /home/worker/work/base/netbird-harmonyos
./spikes/r1-api24-hap/e0-emulator-run.sh
```

The script fails closed unless all of these conditions hold:

- no historical Go/TLS HAP member is present;
- the Emulator process starts, and both the current qemu boot segment and a Beta HDC shell probe report readiness;
- only after that readiness gate do both unsigned HAPs install together;
- the lock screen is dismissed with a no-credential wake/Home/swipe sequence;
- three `aa start` calls return semantic success, including `start ability successfully.`, after complete force-stop cycles;
- all three application PIDs are nonempty and distinct;
- each run reaches the Node-API result and produces a visible 1320x2856 screenshot;
- the installed application is normal, non-system, and exposes `EntryAbility` as the main element;
- the test HAP runs only afterward as a non-substituting baseline;
- all four lifecycle/result markers occur three times;
- uninstall and package-absence checks pass;
- no Emulator, qemu, HDC server, or tested port remains.

`aa`, HDC, and test-framework host exit codes are not sufficient by themselves. The runner checks output semantics, guest process state, HiLog counts, image pixels, and final cleanup state.

## Measured E0 Result

`EV-E0-EMU24-20260717-0001` passed the functional E0 criteria on the recorded API 24 x86_64 target:

| Run | PID | Node-API | Screenshot pixel `(100,1000)` | YAVG |
| ---: | ---: | --- | --- | ---: |
| 1 | 2907 | `ping=pong`, `version=r1-api24-probe/0.0.1` | `242 246 248` | 225.599 |
| 2 | 2966 | same | `242 246 248` | 225.599 |
| 3 | 3123 | same | `242 246 248` | 225.599 |

`onCreate observed=true`, `onWindowStageCreate`, `onForeground`, and the Node-API result each occurred exactly three times. Final transcript markers were `VERDICT=pass`, `TRAP_EXIT_CODE=0 RESULT=pass`, `FINAL_RESIDUAL_PROCESS=false`, and `FINAL_RESIDUAL_PORT=false`.

The evidence record is `record_status: reviewed-pass`; E0 is closed and passed for its exact evidence scope. E8 remains `CLOSED`, the physical-device prohibition remains in force, and E1 is the next execution gate. This does not establish Go, VPN, formal R-stage, arm64, or device success.

## Evidence

- [E0 evidence record](../../docs/evidence/e0-api24-emulator-2026-07-17.md)
- Immutable measured application/test HAP archives: `docs/evidence/raw/EV-E0-EMU24-20260717-0001-{application,test}-hap.bin`
- Replayable sanitized transcript: `docs/evidence/raw/EV-E0-EMU24-20260717-0001-transcript.log`
- Full app HiLog and tag-filtered HiLog under the same evidence prefix
- Three guest screenshots under the same evidence prefix
- Measured source-and-driver manifest under the same evidence prefix
- Post-measurement clean-build support log under the same evidence prefix

This result is limited to the exact unsigned HAP hashes and target tuple in the evidence record. It does not validate arm64 runtime behavior, a named physical device, Huawei commercial HarmonyOS, NetBird, Go, VPN APIs, signing, channel acceptance, product support, or any R-stage exit.
