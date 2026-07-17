# E1 C-only API 24 HAP Probe

This directory is a short-lived HarmonyOS research probe for the API 24 x86_64 Emulator gates. It must not evolve into a product shell.

## Current Scope

The current build graph is the E1 C-only ordinary-application probe:

- one unsigned API 24 Stage application HAP with bundle `cn.alfadb.netbird.r1probe`;
- an ordinary `EntryAbility`; the evidence runner does not build, install, or invoke the TestRunner sidecar;
- synchronous ArkTS-to-native guarded `Uint8Array` hashing over 100 distinct lengths per round;
- C pthread-to-ArkTS asynchronous delivery through the target SDK's public `napi_threadsafe_function` APIs;
- real `socketpair`, fd number handoff to ArkTS and return to native, `dup`, write/read, ownership close, and repeated-close `EBADF` checks;
- 10 complete rounds with per-round `/proc/self/fd` and `/proc/self/task` snapshots;
- a visible PID-specific `E1 C-only PASS` or FAIL page.

The current HAP contains no NetBird, Go runtime, TUN, VPN, DNS probe, Native Child invocation, `libgoprobe.so`, or `libtls-*` input. The CMake graph references only `e1_c_probe.cpp`; historical Go/TLS/PS4 sources and TestRunner execution are outside this probe.

Generated native libraries, HAPs, and build state are ignored. There is intentionally no signing configuration.

## Implementation

Each ordinary application PID first exercises the threadsafe function as an explicit `round=0` warmup. The measured runtime creates one persistent fd and one persistent thread on first delivery, changing the pre-warmup snapshot from `37/27` to the stable `38/28` baseline. The warmup's 100 callbacks are validated and recorded but excluded from the 10 measured rounds.

Every measured round then performs:

1. 100 synchronous buffers with lengths `0..99`, deterministic bytes, distinct guard sentinels, native FNV-1a, and ArkTS-side hash/guard checks.
2. One real fd transfer. Native creates fds `37/38`, returns the numbers to ArkTS, receives them back, duplicates fd `37` as `39`, writes and reads a deterministic payload, closes all ownership, then observes `EBADF(9)` for both repeated closes.
3. One new pthread that queues 100 ordered payload callbacks. ArkTS checks sequence, payload/hash, producer TID, and that the consumer TID equals the ordinary EntryAbility main ArkTS TID.
4. A joined producer and equal pre/post fd and thread counts. Any growth, callback error, fd error, timeout, crash, missing sample, or invalid page marker fails closed.

The native ELF uses the target SDK's public `napi_create_threadsafe_function`, `napi_call_threadsafe_function`, `napi_acquire_threadsafe_function`, and `napi_release_threadsafe_function` surface. It depends only on `libace_napi.z.so`, `libhilog_ndk.z.so`, `libc++_shared.so`, and `libc.so`.

## Clean Build

Use fixed tool paths rather than the floating `current` symlink:

```bash
cd /home/worker/work/base/netbird-harmonyos/spikes/r1-api24-hap
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw clean \
  --mode module -p product=default -p module=entry@default \
  -p buildMode=debug --no-daemon
/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap \
  --mode module -p product=default -p module=entry@default \
  -p buildMode=debug --no-daemon
```

Expected output:

```text
entry/build/default/outputs/default/entry-default-unsigned.hap
```

Hvigor's unsigned packaging is not byte-for-byte reproducible in this environment. The evidence runner archives the exact HAP before installation and binds all conclusions to that SHA-256.

## Emulator Run

The runner performs its own clean build, starts and stops the visible `netbird_api24_phone` instance, sets guest HiLog buffers to 16 MiB, installs only the application HAP, and executes three ordinary cold starts:

```bash
cd /home/worker/work/base/netbird-harmonyos
bash spikes/r1-api24-hap/e1-c-emulator-run.sh
```

It fails closed unless all of these conditions hold:

- the HAP has no historical Go/TLS/TestRunner member and the x86_64 ELF references every required public threadsafe/fd symbol;
- current qemu boot-complete and Beta HDC shell readiness both pass before installation;
- the installed package is a normal, non-system application whose main element is `EntryAbility`;
- all three cold starts have distinct nonempty PIDs;
- each PID has exactly one warmup summary, 10 round PASS lines, 1000 buffer samples, 1000 valid callback samples, 10 fd results, and 10 matched pthread starts/finishes;
- every measured resource line states no monotonic increase;
- each PID remains alive through a visible 1320x2856 PASS screenshot;
- complete unfiltered HiLog, tag HiLog, source inputs, native member, HAP, transcript, Emulator console, and screenshots are archived and hashed;
- force-stop, uninstall, bundle absence, Emulator/HDC shutdown, and tested-port cleanup all pass.

## Measured Result

`EV-E1-EMU24-20260717-0005` completed with `record_status: collected` and `verdict: pass`:

| Run | PID/main TID | Measured rounds | Buffers | Valid callbacks | fd transfers | Stable fd/thread | Screenshot |
| ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| 1 | 3148 | 10 | 1000 | 1000 | 10 | `38/28` | visible PASS |
| 2 | 3243 | 10 | 1000 | 1000 | 10 | `38/28` | visible PASS |
| 3 | 3405 | 10 | 1000 | 1000 | 10 | `38/28` | visible PASS |

The exact application HAP SHA-256 is `27f827ea7997759c65ec3a10f1e3fdcc728e918589000b66f3f5e798f9e273b9`; the executed x86_64 native member SHA-256 is `caed38130bef9987fff2f5c7be9281675aeeb52d2a3fef7c402d4b6a70c69fca`.

The evidence is awaiting independent review. It is only an E1 C-only sub-record: the latest formal NetBird baseline's official Go 1.25.12 loader path still fails, so E1 is not closed. E8 remains `CLOSED`, physical-device execution remains prohibited, and no R stage, arm64, VPN, channel, or product claim follows.

## Evidence

- [E1 C-only evidence record](../../docs/evidence/e1-c-bridge-api24-emulator-2026-07-17.md)
- [E0 ordinary-application evidence record](../../docs/evidence/e0-api24-emulator-2026-07-17.md)
- Exact HAP/native/source archives and hashes under `docs/evidence/raw/EV-E1-EMU24-20260717-0005-*`
- Complete replay transcript, tag HiLog, three unfiltered HiLogs, Emulator console, and three screenshots under the same prefix

Attempts `0001` through `0004` are retained and disclosed in the evidence record. They were rejected before collection acceptance for runner working-directory, pre-warmup baseline, or HiLog-capacity reasons and cannot substitute for `0005`.
