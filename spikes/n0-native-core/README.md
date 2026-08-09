# N0 native core — host build spike

N0(b) host-build part of the [N0 native client feasibility gate](../../docs/n0-native-client-feasibility.md):
a single native WireGuard core (BoringTun 0.7.1, `ffi-bindings` only) with a
narrow C ABI smoke, a thinnest C++ NAPI overlay, and a dual-ABI host build
against the official OHOS clang/sysroot.

This spike is **host-only**: it builds and verifies ELF artifacts. It does not
start an Emulator, does not run HDC, and does not claim any runtime loading.
The Emulator runtime evidence is produced by the dedicated runner
`n0-emulator-run.sh` (see below), which is the only N0(b) Emulator HDC entry
point.

## Scope and boundaries

| Item | Status |
| --- | --- |
| BoringTun | fixed `0.7.1`, `default-features = false`, `features = ["ffi-bindings"]` |
| `device` feature / socket2 patch | **forbidden** (socket2 0.4.10 `IovLen` is undefined on OHOS targets; not patched, per N0 stop condition 1) |
| management/ICE/relay/UI/VPN/TUN/protect | **not implemented** |
| x86_64 | full build + ELF verification; `libentry.so` is the HAP snapshot drop-in |
| aarch64 | **cross-compile only; no load claim** |
| Emulator / HDC / physical device | **not run by this spike**; the dedicated runner `n0-emulator-run.sh` is the only N0(b) Emulator HDC entry point (fixed `netbird_api24_phone` / `127.0.0.1:10000`); physical device HDC remains forbidden |
| Cargo.lock | authoritative (`--locked`); BoringTun crate checksum verified against cargo cache and lock; build is fully offline (`--offline`) |

## Layout

```text
Cargo.toml / Cargo.lock   Rust core crate (libn0core: cdylib + staticlib)
src/lib.rs                 narrow C ABI: n0_probe_version, n0_probe_smoke
                          (calls the real BoringTun x25519/new_tunnel/tick; nothing faked)
napi/n0_overlay.cpp        thinnest C++ NAPI overlay: runProbe() -> {version, key, smoke}
napi/types/                ArkTS type declarations (libentry.so)
napi/runProbeTest.ets      ArkTS test entry (asserts structured fields, fail-closed)
napi/ohosTest/             ohosTest test runner that actually calls runN0ProbeTest()
build.sh                   dual-ABI host build + ELF/symbol/checksum verification
                           (cargo --offline --locked; no network during build)
prepare-hap-snapshot.sh    stages the overlay into a temp r1-api24-hap snapshot copy
n0-emulator-run.sh         formal API 24 x86_64 Emulator evidence runner
                           (--selftest / --dry-run / formal run)
README.md                  this file
```

## Build

```bash
bash spikes/n0-native-core/build.sh
```

The script (all fail-closed):

1. verifies the official OHOS clang/sysroot, the OHOS `llvm-ar` and the
   rustup OHOS targets exist;
2. verifies the BoringTun 0.7.1 crate checksum
   (`15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939`) against
   the cargo cache and `Cargo.lock`;
3. builds the Rust core for `x86_64-unknown-linux-ohos` and
   `aarch64-unknown-linux-ohos` (release, `--offline --locked`, linker =
   official OHOS clang, `AR_*` = official OHOS `llvm-ar` for both targets;
   the build never touches the network);
4. builds the C++ overlay `libentry.so` for both ABIs against the OHOS sysroot,
   statically linking `libn0core.a` (the final `.so` also exports the real
   BoringTun C ABI symbols; the overlay explicitly includes `<string.h>` for
   the bounded `strnlen` read of the public key buffer);
5. verifies every artifact is an ELF shared object of the expected architecture,
   that `DT_NEEDED` contains **no `libc.so.6`** (glibc), and that the exported
   symbols include `n0_probe_version`, `n0_probe_smoke` and the BoringTun C ABI
   (`x25519_secret_key`, `x25519_public_key`, `x25519_key_to_base64`,
   `check_base64_encoded_x25519_key`, `new_tunnel`, `wireguard_tick`,
   `tunnel_free`). The `readelf` output is captured first and checked with bash
   string matching (a `readelf | grep -q` pipeline under `set -o pipefail`
   would fail spuriously).

Outputs:

```text
out/x86_64/libentry.so            x86_64 overlay (HAP snapshot drop-in)
out/aarch64/libentry.so           aarch64 overlay (cross-compile only)
out/hap-snapshot/x86_64/          libentry.so + libn0core.a + libn0core.so
target/<target>/release/          Rust core artifacts (libn0core.so / libn0core.a)
```

## Smoke semantics

`n0_probe_smoke` genuinely calls the BoringTun C ABI:

- `x25519_secret_key` → `x25519_public_key` → `x25519_key_to_base64` →
  `check_base64_encoded_x25519_key` (must return 1, base64 length 44);
- `new_tunnel` (same key pair, no preshared key, no keep-alive) →
  `wireguard_tick` (must not return `WIREGUARD_ERROR`) → `tunnel_free`.

Nothing is faked. `runProbe()` returns `{version, key, smoke:{ok, x25519Ok,
tunnelOk, tickOp, tickSize}}`; `smoke.ok` is the fail-closed verdict and any
marshaling error throws.

### `rc` / `ok` semantics (do not misread `rc=0 ok=0`)

In the C ABI, `ok == 0` means **PASS** (`n0_smoke_result.ok` is 0 when all
smoke checks passed; nonzero is fail-closed). `n0_probe_smoke` returns
`out->ok` by contract, so the return code and the struct field are the same
value: a host check reading `rc=0 ok=0` must read it as **pass**, not as a
contradiction. The C++ overlay enforces this invariant with an explicit
`rc == smoke.ok` consistency check (any disagreement throws) and maps the C
ABI value to ArkTS as `smoke.ok = (ok == 0)` — i.e. `rc=0 ok=0` becomes
`smoke.ok=true` in the ArkTS result.

## HAP snapshot + Emulator runner

`prepare-hap-snapshot.sh` stages the overlay into a temporary copy of the
**pinned** `spikes/r1-api24-hap` snapshot at commit
`2c567dc721c6582f93a15b241e843e3bbff3f7f3` (fixed constant, never dynamic
HEAD; verified to exist in the repository) via `git archive`, then:

- drops the prebuilt x86_64 `libentry.so` into `entry/libs/x86_64/`;
- adds the `libentry.so` types dependency to `entry/oh-package.json5` and
  keeps `entry/oh-package-lock.json5` in sync;
- overwrites the ohosTest test runner with the N0 runner
  (`napi/ohosTest/OpenHarmonyTestRunner.ets`) so the `aa test` Hypium runner
  **actually calls `runN0ProbeTest()`** (it is not left unreferenced);
- restricts `entry/build-profile.json5` `abiFilters` to `x86_64` so the HAP
  packages no arm64 artifacts (arm64 is cross-compile only, no load claim);
- prints the exact hvigor commands.

The test runner emits the machine-readable marker
`N0_CORE_PROBE_RESULT|verdict=PASS|...` on success and
`N0_CORE_PROBE_RESULT|verdict=FAIL|...` (plus a nonzero test result code) on
any assertion failure. The FAIL detail is cleaned of `|`, `\r`, `\n` and `;`
so the marker stays single-line and the runner's `[^;]*` extraction is never
truncated. No private key material is ever printed; the public key is only
reported as its length (44 chars), never its value.

### Runner

`n0-emulator-run.sh` is the formal N0(b) Emulator evidence runner:

```bash
bash spikes/n0-native-core/n0-emulator-run.sh --selftest  # pure host checks
bash spikes/n0-native-core/n0-emulator-run.sh --dry-run   # full pipeline rehearsal, no device, no evidence
bash spikes/n0-native-core/n0-emulator-run.sh             # formal run (evidence ID EV-N0-EMU24-20260810-0001)
```

Formal run pipeline (all fail-closed):

0. **preconditions (before no-clobber and before any evidence file)**: only
   the fixed Emulator instance `netbird_api24_phone` and HDC target
   `127.0.0.1:10000` are allowed; `PHYS_1_TARGET`, any non-fixed
   `TARGET`/`HDC_TARGET`/`EMULATOR_TARGET`/`HDC_PORT`/`EMULATOR_INSTANCE`
   request, and any `CONNECT_HELPER`/`STOP_HELPER` override are refused;
   `WORKSPACE` must be the script repository itself; `EVIDENCE_ID` must match
   `EV-<gate>-<target>-<YYYYMMDD>-<NNNN>` (this also makes the guest staging
   path shell-safe); the workspace must be a git repository containing the
   pinned snapshot commit; the working tree must be clean and HEAD must
   contain this runner. Dry-run records the state but explicitly exempts git
   clean / runner-in-HEAD. A formal run that fails a precondition leaves no
   evidence files behind.
1. **offline build**: the runner FORCES `DEVECO_SDK_HOME=$BUILD_TOOLS/sdk`
   (verified to resolve to the native SDK) before any build, then `build.sh`
   (cargo `--offline --locked`, OHOS llvm-ar for both targets) dual-ABI build,
   then `prepare-hap-snapshot.sh` stages the fixed r1 snapshot, then
   `ohpm install --all` + `hvigor clean` + `assembleHap` for the app and test
   HAPs;
2. **HAP identity**: both HAPs must package `libs/x86_64/libentry.so`
   byte-equal to `out/x86_64/libentry.so`, must contain no arm64 member, and
   the module.json must match the r1probe bundle / `entry` module /
   `entry_test` + `OpenHarmonyTestRunner` (unzip member lists are captured
   first, never piped into grep under pipefail);
3. **hashes**: source files, staged snapshot files, `Cargo.lock`, the
   BoringTun crate, toolchain versions and all artifacts are hashed into the
   source manifest; the stable build SDK (`command-line-tools/6.1.1.290`:
   hvigorw/ohpm/native) and the beta Emulator runtime
   (`command-line-tools/26.0.0.461`: Emulator binary + hdc) are recorded as
   two distinct inputs, never mixed; the real native SDK version/api are read
   from `oh-uni-package.json` (never hardcoded) and recorded in the header
   and manifest;
4. **device phase**: cold boot, wait ready, install app + test HAPs, clear
   HiLog, `aa test` (aa RC is captured but NOT gated here);
5. **judgment**: exactly one distinct `N0_CORE_PROBE_RESULT` marker is
   required; verdict PASS + version prefix + `keyLen=44` + `tickOp=0` +
   `tickSize=0` + NAPI HiLog `N0_RUNPROBE` `smokeOk=1/x25519Ok=1/tunnelOk=1` +
   host aa RC=0 + guest `TestFinished-ResultCode=0`. With no marker at all,
   a precise loader rejection of `libentry.so` in the collected aa/hilog logs
   is a measured blocked only when a line references `libentry.so` AND carries
   an exact signature on the SAME line: the complete phrase `initial-exec TLS
   resolves to dynamic definition`, `Error relocating` (symbol-missing
   variants excluded), or an explicit `dlopen ... libentry.so ... failed` /
   `libentry.so ... dlopen ... failed`. Lines carrying `not found` / `missing`
   / `cannot locate symbol` / `undefined symbol` / `symbol not found` are
   explicitly excluded FIRST (runner failures, never measured blocked). The
   aa RC check is applied after this classification;
6. **base manifest**: created immediately after the verdict with the
   schema-required `information_status` / `record_status` / `upstream_sha`
   and the dual axis; screenshot / cleanup / residual / sensitive results are
   appended; any later fail still seals the manifest and transcript via the
   EXIT trap, and the seal appends `final_exit_code` / `run_status` /
   `fail_reason` to the manifest so a post-verdict failure is never only in
   the transcript;
7. **cleanup**: main path and EXIT trap both force-stop / uninstall / stop
   the Emulator / `hdc kill`, and record residual process/port state
   (double cleanup in the formal device phase); before the device phase (a
   precondition failure) the teardown performs NO hdc kill / residual port
   cleanup — `device_phase_started` is set to 1 at the first Emulator/HDC
   action; `E8_STATUS=CLOSED`, `PHYSICAL_DEVICE_USED=false`.

Verdict dual-axis (machine-parseable):

- `RECORD_STATUS=collected` + `VERDICT=pass`: all pass criteria above.
- `RECORD_STATUS=collected` + `VERDICT=blocked`: measured platform/call
  failure, one of:
  - the aa test executed (host RC=0) and the guest emitted exactly one marker
    with `verdict=FAIL` whose detail starts with an anchored platform-rejection
    prefix: `detail=dlopen` | `detail=N0 runProbe smoke failed:` |
    `detail=N0 runProbe sub-check failed:` | `detail=n0_probe_smoke returned
    inconsistent status` | `detail=n0_probe_version returned null` |
    `detail=failed to build runProbe result`. `detail=N0 runProbe smoke
    missing` is NOT blocked (NAPI marshaling defect, runner fail);
  - no marker at all, but HAP member identity passed and the collected
    aa/hilog logs contain a precise loader rejection pointing at
    `libentry.so` on the same line (`initial-exec TLS resolves to dynamic
    definition` | `Error relocating` without symbol-missing | explicit
    `dlopen ... libentry.so ... failed`); not-found / symbol-missing lines
    are excluded first and are never measured blocked.
- anything else (runner defect, missing tool, build failure, emulator/install
  failure, marker missing without a precise loader rejection,
  marker duplicated, unexpected FAIL detail, host RC != 0 on a marker
  verdict, guest code != 0 on PASS) exits non-zero and is never recorded as
  measured blocked.

Evidence (raw under `docs/evidence/raw/EV-N0-EMU24-20260810-0001-*`):
`-transcript.log`, `-n0-build.log`, `-snapshot-prep.log`, `-build.log`,
`-aa-test.log`, `-hilog-tag.log`, `-hilog-app-full.log`,
`-emulator-console.log`, `-source-manifest.txt`, `-run1.png` (best-effort)
and `-manifest.txt` (base manifest written immediately after the verdict;
`manifest_sha256` is the sha256 of the manifest up to and including the
`transcript_final_sha256` line; the self-hash line itself is appended after
hashing; the seal also appends `final_exit_code` / `run_status` /
`fail_reason` before the hashes).

## Acceptance

- `bash spikes/n0-native-core/build.sh` exits 0 with all checks green;
- x86_64 and aarch64 `libentry.so` are ELF shared objects, `DT_NEEDED` has no
  `libc.so.6`, and the exported symbol set includes the narrow C ABI plus the
  real BoringTun C ABI;
- `Cargo.lock` is committed and `--offline --locked` builds reproduce it
  without network access;
- no `device` feature, no socket2 patch, no management/ICE/relay/UI/VPN/TUN/
  protect surface, no private patches;
- arm64 is cross-compile only; no load claim anywhere in this spike;
- `bash spikes/n0-native-core/n0-emulator-run.sh --selftest` exits 0 and
  covers: marker pass/blocked/fail classification (including anchored
  `detail=` prefixes, `smoke missing` → fail, `tickOp=0`/`tickSize=0` on
  PASS, marker first field must be `verdict=`), marker extraction,
  distinct-marker collection (three sources: same → 1, different → 2, empty
  → 0, plus three real prefixes → 1), NAPI field extraction, guest
  result-code extraction, no-marker loader-rejection classification (exact
  rejection → yes; not-found / not-found-with-dlopen / Error relocating
  symbol-not-found / symbol-wrapping / non-libentry / empty / missing file →
  no), target guards, no-clobber, manifest seal self-hash + `final_exit_code`
  / `run_status` / `fail_reason`, teardown no-op and teardown no-device-phase
  (no hdc kill / no residual scan);
- `bash spikes/n0-native-core/n0-emulator-run.sh --dry-run` exits 0 with no
  device action and no evidence file; a formal run from a dirty tree or with
  a malformed `EVIDENCE_ID`/`WORKSPACE` fails at the precondition stage and
  creates no evidence files.
