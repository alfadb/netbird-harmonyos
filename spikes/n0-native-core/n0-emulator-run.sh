#!/usr/bin/env bash
set -Eeuo pipefail

# ============================================================================
# n0-emulator-run.sh
#
# N0(b) formal API 24 x86_64 Emulator evidence runner for the single native
# WireGuard core (BoringTun 0.7.1 ffi-bindings via the N0 C++ NAPI overlay).
#
# Scope (docs/n0-native-client-feasibility.md, N0(b)):
#   - fixed Emulator instance netbird_api24_phone, HDC 127.0.0.1:10000 only;
#   - NO physical device, NO public endpoint, NO VPN/TUN/protect,
#     NO management/ICE/relay/UI surface;
#   - arm64 is cross-compile only; the HAP packages x86_64 only.
#
# Usage:
#   bash spikes/n0-native-core/n0-emulator-run.sh            # formal run
#   bash spikes/n0-native-core/n0-emulator-run.sh --selftest # pure host checks
#   bash spikes/n0-native-core/n0-emulator-run.sh --dry-run  # full pipeline
#                                                            # rehearsal: no
#                                                            # device action,
#                                                            # no evidence file
#
# Formal run preconditions (fail-closed, ALL evaluated BEFORE no-clobber and
# before any evidence file is created):
#   - WORKSPACE is the script repository itself (no override to another repo);
#   - EVIDENCE_ID matches EV-<gate>-<target>-<YYYYMMDD>-<NNNN> (also makes the
#     guest staging path shell-safe);
#   - WORKSPACE is a git repository, the pinned snapshot commit exists in it,
#     the working tree is clean and HEAD contains this runner (dry-run records
#     the state but explicitly exempts git clean / runner-in-HEAD);
#   - the fixed Emulator instance/HDC target are the only allowed device
#     targets (environment overrides to any other target are refused);
#   - no evidence file for the fixed EVIDENCE_ID exists (no-clobber).
#
# Verdict dual-axis (machine-parseable):
#   - RECORD_STATUS=collected + VERDICT=pass: the guest emitted exactly one
#     N0_CORE_PROBE_RESULT marker with verdict=PASS, version prefix
#     n0-native-core/, keyLen=44, tickOp=0, tickSize=0, the NAPI HiLog
#     N0_RUNPROBE line shows smokeOk=1/x25519Ok=1/tunnelOk=1, host aa RC=0 and
#     guest TestFinished-ResultCode=0.
#   - RECORD_STATUS=collected + VERDICT=blocked: measured platform/call
#     failure, one of:
#       (a) the aa test executed (host RC=0) and the guest emitted exactly one
#           N0_CORE_PROBE_RESULT marker with verdict=FAIL whose detail field
#           starts with a platform-rejection prefix: 'detail=dlopen'
#           (libentry.so loader rejection), 'detail=N0 runProbe smoke failed:'
#           / 'detail=N0 runProbe sub-check failed:' (native smoke sub-checks
#           failed on the platform), 'detail=n0_probe_smoke returned
#           inconsistent status' / 'detail=n0_probe_version returned null' /
#           'detail=failed to build runProbe result' (NAPI-layer native
#           failure). 'detail=N0 runProbe smoke missing' is NOT blocked.
#       (b) no marker at all, but HAP member identity passed and the collected
#           aa/hilog logs contain a precise loader rejection pointing at
#           libentry.so ('Error relocating', 'initial-exec TLS resolves to
#           dynamic definition', or an explicit dlopen load failure).
#   - anything else (runner defect, missing tool, build failure, emulator or
#     install failure, marker missing without a precise loader rejection,
#     duplicated marker, unexpected FAIL detail, host aa RC != 0 on a marker
#     verdict, guest code != 0 on a PASS marker) exits non-zero and is NEVER
#     recorded as measured blocked.
#
# No private key material and no public key value is ever printed: the guest
# only reports the public key length (keyLen=44), never the key itself, and
# this runner never extracts or prints a key value.
# ============================================================================

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_DIR="$SCRIPT_DIR"
readonly DEFAULT_WORKSPACE="$(cd -- "$PROJECT_DIR/../.." && pwd -P)"

# --- fixed baseline constants ----------------------------------------------
readonly DEFAULT_EVIDENCE_ID="EV-N0-EMU24-20260810-0002"
readonly SNAPSHOT_HEAD="2c567dc721c6582f93a15b241e843e3bbff3f7f3"
readonly BORINGTUN_VERSION="0.7.1"
readonly BORINGTUN_CRATE_SHA256="15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939"
readonly EXPECTED_VERSION_PREFIX="n0-native-core/"
readonly EXPECTED_KEY_LENGTH="44"
readonly TARGET_TUPLE="HarmonyOS_6.1.1(24),API24,x86_64,phone_Emulator"

# Capture any external device-target request BEFORE the fixed constants are
# assigned: the readonly constants below would otherwise silently clobber the
# environment value and the guard could never detect a non-fixed request.
readonly PHYS_1_TARGET_REQUEST="${PHYS_1_TARGET:-}"
readonly EMULATOR_TARGET_REQUEST="${EMULATOR_TARGET:-}"
readonly EMULATOR_INSTANCE_REQUEST="${EMULATOR_INSTANCE:-}"
readonly CONNECT_HELPER_REQUEST="${CONNECT_HELPER:-}"
readonly STOP_HELPER_REQUEST="${STOP_HELPER:-}"

# --- fixed device targets (never overridable by environment) ----------------
readonly EMULATOR_INSTANCE="netbird_api24_phone"
readonly EMULATOR_TARGET="127.0.0.1:10000"
readonly EMULATOR_HDC_PORT="10000"
readonly CONNECT_HELPER="/home/worker/harmonyos/bin/emulator-connect"
readonly STOP_HELPER="/home/worker/harmonyos/bin/emulator-stop"
readonly EMULATOR_INSTANCE_PATH="/home/worker/harmonyos/emulator-instances"
readonly EMULATOR_IMAGE_ROOT="/home/worker/harmonyos/emulator-images"
readonly EMULATOR_DISPLAY=":1"
readonly EMULATOR_XAUTHORITY="/home/worker/.Xauthority"
readonly EMULATOR_XDG_RUNTIME_DIR="/tmp/runtime-worker"
readonly QEMU_LOG="$EMULATOR_INSTANCE_PATH/$EMULATOR_INSTANCE/Log/qemu.log"

# --- parameterized environment (evidence identity only; never device targets)
WORKSPACE="${WORKSPACE:-$DEFAULT_WORKSPACE}"
EVIDENCE_ROOT="${EVIDENCE_ROOT:-$WORKSPACE/docs/evidence/raw}"
EVIDENCE_ID="${EVIDENCE_ID:-$DEFAULT_EVIDENCE_ID}"
readonly EVIDENCE_ID
readonly STAGING="/data/local/tmp/$EVIDENCE_ID"

# --- toolchain paths --------------------------------------------------------
# Two DISTINCT recorded inputs, never mixed:
#   BUILD_TOOLS (stable 6.1.1.290): hvigorw/ohpm and the native SDK used to
#     build the Rust core and the C++ overlay. The runner FORCES
#     DEVECO_SDK_HOME to $BUILD_TOOLS/sdk before any build (build.sh would
#     otherwise default to command-line-tools/current, a symlink to
#     6.1.1.290), so the runner's native checks and the build use the SAME
#     SDK.
#   EMULATOR_TOOLS (beta 26.0.0.461): the Emulator binary and its hdc. The
#     Emulator runtime is a separate recorded input from the build SDK; the
#     two are recorded independently in the header and the manifest.
readonly BUILD_TOOLS="/home/worker/harmonyos/command-line-tools/6.1.1.290"
readonly EMULATOR_TOOLS="/home/worker/harmonyos/command-line-tools/26.0.0.461"
readonly HVIGOR="$BUILD_TOOLS/bin/hvigorw"
readonly OHPM="$BUILD_TOOLS/bin/ohpm"
readonly NATIVE_HOME="$BUILD_TOOLS/sdk/default/openharmony/native"
readonly NAPI_HEADER="$NATIVE_HOME/sysroot/usr/include/napi/native_api.h"
readonly HDC="$EMULATOR_TOOLS/sdk/default/openharmony/toolchains/hdc"
readonly EMULATOR="$EMULATOR_TOOLS/emulator/Emulator"

# Force the stable build SDK for the whole run: build.sh resolves
# DEVECO_SDK_HOME (defaulting to command-line-tools/current, a symlink to
# 6.1.1.290) — pinning it to the SAME stable SDK recorded here means the
# native checks and the build use one recorded input, never a drift.
export DEVECO_SDK_HOME="$BUILD_TOOLS/sdk"

# Read the real native SDK version/api from oh-uni-package.json (recorded in
# the header and manifest; never hardcoded).
SDK_UNI_PACKAGE="$NATIVE_HOME/oh-uni-package.json"
native_sdk_version=""
native_sdk_api=""
native_sdk_release=""
if [[ -f "$SDK_UNI_PACKAGE" ]]; then
  native_sdk_version="$(grep -oP '"version":\s*"\K[^"]+' "$SDK_UNI_PACKAGE" | head -1 || true)"
  native_sdk_api="$(grep -oP '"apiVersion":\s*"\K[^"]+' "$SDK_UNI_PACKAGE" | head -1 || true)"
  native_sdk_release="$(grep -oP '"releaseType":\s*"\K[^"]+' "$SDK_UNI_PACKAGE" | head -1 || true)"
fi

# --- HAP / test identity ----------------------------------------------------
readonly BUNDLE="cn.alfadb.netbird.r1probe"
readonly TEST_MODULE="entry_test"
readonly TEST_RUNNER="/ets/testrunner/OpenHarmonyTestRunner"
readonly APP_HAP_REL="entry/build/default/outputs/default/entry-default-unsigned.hap"
readonly TEST_HAP_REL="entry/build/default/outputs/ohosTest/entry-ohosTest-unsigned.hap"

# --- runtime state ----------------------------------------------------------
SELFTEST=0
DRY_RUN=0
case "${1:-}" in
  --selftest) SELFTEST=1 ;;
  --dry-run) DRY_RUN=1 ;;
  "") ;;
  *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
esac

result="fail"
fail_reason=""
emulator_started=0
installed=0
device_phase_started=0
snapshot_dir=""
app_member=""
test_member=""
started_at=""
ended_at=""
code_sha=""
built_sha=""
aarch64_sha=""
crate_sha=""
app_member_sha=""
test_member_sha=""
DRY_TMP=""

TRANSCRIPT=""
TAG_HILOG=""
APP_HILOG=""
MANIFEST=""
CONSOLE=""
BUILD_LOG=""
N0_BUILD_LOG=""
SNAPSHOT_LOG=""
AA_TEST_LOG=""
SOURCE_MANIFEST=""
SCREENSHOT=""

# --- helper functions -------------------------------------------------------
print_command() {
  printf '$'
  printf ' %q' "$@"
  printf '\n'
}

run() {
  print_command "$@"
  "$@"
}

hdc() {
  # stdin from /dev/null: the hdc client can spawn a daemon, and the daemon
  # must never inherit the runner's stdin or depend on the runner's stdout
  # (the transcript). Callers that need output capture it via $(...) or a
  # file, never via the inherited transcript stream.
  timeout 30 "$HDC" -t "$EMULATOR_TARGET" "$@" </dev/null
}

fail() {
  result="fail"
  fail_reason="$1"
  printf 'FAIL_REASON=%s\n' "$1"
  exit 1
}

# Reliable EXIT teardown: any fail/interrupt after startup/install first
# deletes the guest staging directory while the Emulator is still online, then
# uninstalls, stops the Emulator, kills host hdc, records residual
# process/port state and seals the manifest. It never touches anything outside
# the fixed Emulator target. Before the device phase (a precondition failure)
# no hdc kill / residual port cleanup is performed: device_phase_started is
# set to 1 at the first Emulator/HDC action.
teardown() {
  # Capture the exit status FIRST: any command before this (including the
  # SELFTEST check below) would reset $? and lose the real exit code.
  local exit_code=$?
  if (( SELFTEST == 1 )); then
    return 0
  fi
  set +e
  printf 'CLEANUP_BEGIN=teardown\n'
  if (( DRY_RUN == 1 )); then
    printf 'CLEANUP_DRY_RUN=no-device-no-evidence\n'
    rm -rf "$DRY_TMP" "$snapshot_dir" "$app_member" "$test_member" 2>/dev/null || true
    printf 'CLEANUP_TEMP=removed\n'
    printf 'CLEANUP_END=teardown-complete\n'
    if (( exit_code != 0 )); then
      printf 'DRY_RUN_VERDICT=fail\n'
    fi
    set -e
    printf 'TRAP_EXIT_CODE=%s RESULT=%s\n' "$exit_code" "$result"
    exit "$exit_code"
  fi
  # 1. While the Emulator is still online, delete the guest staging directory
  #    first: it lives on /data/local/tmp and is unreachable after stop. The
  #    EVIDENCE_ID is format-validated (EV-<gate>-<target>-<YYYYMMDD>-<NNNN>)
  #    and the path is single-quoted inside the guest shell command.
  if (( emulator_started == 1 )); then
    hdc shell "rm -rf '$STAGING'" >/dev/null 2>&1 </dev/null || true
    printf 'CLEANUP_STAGING=cleared\n'
  else
    printf 'CLEANUP_STAGING=skipped-emulator-not-started\n'
  fi
  # 2. Uninstall the bundle while the Emulator is still online.
  if (( installed == 1 )); then
    timeout 120 "$HDC" -t "$EMULATOR_TARGET" uninstall "$BUNDLE" >/dev/null 2>&1 </dev/null || true
    installed=0
    printf 'CLEANUP_UNINSTALL=done\n'
  fi
  # 3. Stop the Emulator.
  if (( emulator_started == 1 )); then
    HDC_PORT="$EMULATOR_HDC_PORT" timeout 60 "$STOP_HELPER" >/dev/null 2>&1 </dev/null || true
    for _ in $(seq 1 30); do
      if ! pgrep -f '/emulator/Emulator.*-start '"$EMULATOR_INSTANCE"'|qemu-system.*'"$EMULATOR_INSTANCE" >/dev/null 2>&1; then
        break
      fi
      sleep 1
    done
    emulator_started=0
    printf 'CLEANUP_EMULATOR=stopped\n'
  fi
  # 4. Kill the host hdc daemon (device phase only: a precondition failure
  #    never touches hdc).
  if (( device_phase_started == 1 )); then
    "$HDC" kill >/dev/null 2>&1 </dev/null || true
    printf 'CLEANUP_HDC=kill-issued\n'
  else
    printf 'CLEANUP_HDC=skipped-no-device-phase\n'
  fi
  # 5. Record residual process/port state (device phase only). The Emulator
  #    processes are matched by their fixed command line; the hdc daemon is
  #    matched by its exact process name (pgrep -ax hdc), never by a loose
  #    pattern.
  if (( device_phase_started == 1 )); then
    residual_emulator="$(pgrep -af '/emulator/Emulator.*-start '"$EMULATOR_INSTANCE"'|qemu-system.*'"$EMULATOR_INSTANCE" || true)"
    residual_hdc="$(pgrep -ax hdc || true)"
    residual_ports="$(ss -ltnp | grep -E ":($EMULATOR_HDC_PORT|5555|8710)[[:space:]]" || true)"
    printf 'RESIDUAL_EMULATOR_PROCESS=%q\n' "$residual_emulator"
    printf 'RESIDUAL_HDC_PROCESS=%q\n' "$residual_hdc"
    printf 'RESIDUAL_PORT=%q\n' "$residual_ports"
  else
    printf 'RESIDUAL_STATE=skipped-no-device-phase\n'
  fi
  rm -rf "$snapshot_dir" "$app_member" "$test_member" 2>/dev/null || true
  printf 'CLEANUP_TEMP=removed\n'
  printf 'CLEANUP_END=teardown-complete\n'
  # TRAP_EXIT/RESULT must be inside the transcript BEFORE the stream is sealed
  # (seal_transcript switches stdout to /dev/null, so anything printed after
  # the seal would not be part of the transcript hash).
  printf 'TRAP_EXIT_CODE=%s RESULT=%s\n' "$exit_code" "$result"
  seal_and_finalize "$exit_code"
  set -e
  exit "$exit_code"
}
trap teardown EXIT

# Close the transcript log stream: switch the runner's stdout/stderr to
# /dev/null so nothing after this point can append to the transcript, then
# hash the now-final transcript. There is no tee process and no fd 3: the
# transcript is appended directly with O_APPEND, so the seal never waits on
# a child holding the stream open and always completes in bounded time.
seal_transcript() {
  exec >/dev/null 2>&1
}

# Called at the end of teardown, after all teardown output has been flushed
# into the transcript: seal the stream, then append the final exit code, run
# status, fail reason, the final transcript size and its sha256 (over the
# first transcript_final_bytes bytes, recomputed with head -c N) and a
# self-hash of the manifest so no mid-stream hash is ever presented as final
# and the teardown log is inside the sealed hash. Runs whenever the manifest
# exists, so any later fail (cleanup/residual/sensitive) still seals.
seal_and_finalize() {
  local exit_code="${1:-0}"
  seal_transcript
  if [[ -n "$MANIFEST" && -f "$MANIFEST" ]]; then
    local transcript_bytes transcript_hash manifest_hash
    # A failure after the verdict (cleanup/residual/sensitive) must be
    # recorded in the manifest, not only in the transcript: append the final
    # exit code, the run status and the fail reason BEFORE the hashes so they
    # are inside the sealed hash.
    printf 'final_exit_code=%s\n' "$exit_code" >>"$MANIFEST"
    printf 'run_status=%s\n' "$result" >>"$MANIFEST"
    printf 'fail_reason=%s\n' "${fail_reason:-}" >>"$MANIFEST"
    # The transcript is final (stdout is /dev/null): record its size and the
    # sha256 of the first transcript_final_bytes bytes, recomputed with
    # head -c N so a transcript that grows after the stat can never be
    # silently re-hashed.
    transcript_bytes="$(stat -c %s "$TRANSCRIPT" 2>/dev/null || true)"
    printf 'transcript_final_bytes=%s\n' "$transcript_bytes" >>"$MANIFEST"
    transcript_hash="$(head -c "${transcript_bytes:-0}" "$TRANSCRIPT" 2>/dev/null | sha256sum | awk '{print $1}')" || true
    printf 'transcript_final_sha256=%s\n' "$transcript_hash" >>"$MANIFEST"
    manifest_hash="$(sha256sum "$MANIFEST" | awk '{print $1}')" || true
    printf 'manifest_sha256=%s\n' "$manifest_hash" >>"$MANIFEST"
  fi
}

# Refuse to overwrite existing fixed evidence (no-clobber). Runs before any
# raw file is created and before any device action.
check_no_clobber() {
  local f
  for f in "$TRANSCRIPT" "$TAG_HILOG" "$APP_HILOG" "$MANIFEST" "$CONSOLE" \
    "$BUILD_LOG" "$N0_BUILD_LOG" "$SNAPSHOT_LOG" "$AA_TEST_LOG" \
    "$SOURCE_MANIFEST" "$SCREENSHOT"; do
    if [[ -n "$f" && -e "$f" ]]; then
      printf 'REFUSE_OVERWRITE=evidence file already exists: %s; refusing to overwrite fixed evidence (use a fresh EVIDENCE_ID or EVIDENCE_ROOT)\n' "$f" >&2
      return 1
    fi
  done
  return 0
}

# --- forbidden target guards (silent; return 1 when the input is forbidden) --
guard_physical_target() {
  [[ -z "${PHYS_1_TARGET:-}" ]]
}
guard_emulator_target() {
  local request="${1:-$EMULATOR_TARGET_REQUEST}"
  local var
  for var in TARGET HDC_TARGET; do
    if [[ -n "${!var:-}" && "${!var}" != "$EMULATOR_TARGET" ]]; then
      return 1
    fi
  done
  [[ -z "$request" || "$request" == "$EMULATOR_TARGET" ]]
}
guard_hdc_port() {
  [[ -z "${HDC_PORT:-}" || "$HDC_PORT" == "$EMULATOR_HDC_PORT" ]]
}
guard_emulator_instance() {
  local request="${1:-$EMULATOR_INSTANCE_REQUEST}"
  [[ -z "$request" || "$request" == "$EMULATOR_INSTANCE" ]]
}
guard_helpers() {
  local connect="${1:-$CONNECT_HELPER_REQUEST}" stop="${2:-$STOP_HELPER_REQUEST}"
  [[ -z "$connect" && -z "$stop" ]]
}

# --- marker extraction ------------------------------------------------------
# Extract the N0_CORE_PROBE_RESULT marker content from a raw line. Handles the
# hilog prefix ("...: N0_CORE_PROBE_RESULT|..."), the aa-test printSync line
# and the TestFinished-ResultMsg line (marker terminated by ';'). The marker
# summary/detail never contains ';' by construction (the guest cleans '|' and
# line breaks from error details; the PASS summary is fixed-format).
extract_marker() {
  local line="$1"
  printf '%s' "$line" | sed -n 's/.*\(N0_CORE_PROBE_RESULT[^;]*\).*/\1/p' | sed 's/\r$//'
}

# Collect every N0_CORE_PROBE_RESULT line from the given files, extract the
# marker content, dedupe identical lines (the same marker legitimately appears
# in the tag HiLog, the aa-test printSync output and the app HiLog) and print
# the distinct markers one per line. Prints nothing when no marker exists.
collect_distinct_markers() {
  local f line extracted seen d
  local -a marker_lines=()
  local -a distinct_markers=()
  for f in "$@"; do
    if [[ -n "$f" && -f "$f" ]]; then
      while IFS= read -r line; do
        marker_lines+=("$line")
      done < <(grep -F 'N0_CORE_PROBE_RESULT' "$f" || true)
    fi
  done
  for line in "${marker_lines[@]}"; do
    extracted="$(extract_marker "$line")"
    [[ -n "$extracted" ]] || continue
    seen=0
    for d in "${distinct_markers[@]}"; do
      if [[ "$d" == "$extracted" ]]; then
        seen=1
        break
      fi
    done
    if (( seen == 0 )); then
      distinct_markers+=("$extracted")
    fi
  done
  # Print nothing when no distinct marker exists (a bare printf of an empty
  # array would emit one empty line).
  if (( ${#distinct_markers[@]} == 0 )); then
    return 0
  fi
  printf '%s\n' "${distinct_markers[@]}"
}

# Extract a pipe-separated NAPI HiLog field (N0_RUNPROBE|smokeOk=1|...).
napi_field() {
  printf '%s' "$1" | awk -F'|' -v name="$2" \
    '{ for (i = 1; i <= NF; i++) if ($i ~ ("^" name "=")) { sub("^" name "=", "", $i); print $i } }'
}

# Extract the guest TestFinished-ResultCode from the aa-test log (last match).
# The trailing `|| true` keeps the no-match case (empty output) from tripping
# `set -euo pipefail` on the grep pipeline.
extract_guest_code() {
  local log="$1"
  grep -o 'TestFinished-ResultCode: [0-9]*' "$log" 2>/dev/null | tail -1 | awk '{print $2}' || true
}

# Precise no-marker loader rejection: returns 0 only when a log line
# references libentry.so AND carries an exact loader-rejection signature on
# the SAME line. Lines carrying a not-found / symbol-missing phrase are
# explicitly excluded FIRST (they are runner failures, never measured
# blocked), and the exclusion wins even when the same line also contains a
# signature. Allowed positives: the complete phrase 'initial-exec TLS resolves
# to dynamic definition', 'Error relocating' (symbol-missing variants already
# excluded), or an explicit dlopen load failure naming libentry.so
# ('dlopen ... libentry.so ... failed' / 'libentry.so ... dlopen ... failed').
is_loader_rejection() {
  local f="$1"
  [[ -n "$f" && -f "$f" ]] || return 1
  awk '
    /not found|missing|cannot locate symbol|undefined symbol|symbol not found/ { next }
    /libentry\.so/ && /initial-exec TLS resolves to dynamic definition|Error relocating|dlopen.*failed/ { found = 1; exit }
    END { exit (found ? 0 : 1) }
  ' "$f"
}

# --- judgment: classify a single N0_CORE_PROBE_RESULT line ------------------
# Field-level judgment: split the pipe-separated marker into fields and read
# only the verdict field (field 2) and the trailing summary/detail. A phrase
# appearing anywhere else (e.g. inside the FAIL detail) must never influence
# the classification.
judge_marker() {
  local marker_line="$1"
  local verdict="" summary="" detail="" field
  local -a fields
  IFS='|' read -ra fields <<<"$marker_line"
  if (( ${#fields[@]} < 2 )); then
    printf 'fail\n'
    return 0
  fi
  # The marker's first field must be the verdict field (verdict=...); a
  # marker whose first field is anything else is malformed -> fail.
  if [[ "${fields[1]}" != "verdict="* ]]; then
    printf 'fail\n'
    return 0
  fi
  verdict="${fields[1]#verdict=}"
  if [[ "$verdict" != "PASS" && "$verdict" != "FAIL" ]]; then
    printf 'fail\n'
    return 0
  fi
  if [[ "$verdict" == "PASS" ]]; then
    summary="${fields[2]:-}"
    local version="" keylen="" tickop="" ticksize=""
    version="$(printf '%s' "$summary" | sed -n 's/.*version=\([^ ]*\).*/\1/p')"
    keylen="$(printf '%s' "$summary" | sed -n 's/.*keyLen=\([0-9]*\).*/\1/p')"
    tickop="$(printf '%s' "$summary" | sed -n 's/.*tickOp=\([0-9]*\).*/\1/p')"
    ticksize="$(printf '%s' "$summary" | sed -n 's/.*tickSize=\([0-9]*\).*/\1/p')"
    if [[ "$version" == "$EXPECTED_VERSION_PREFIX"* && "$keylen" == "$EXPECTED_KEY_LENGTH" && \
          "$tickop" == "0" && "$ticksize" == "0" ]]; then
      printf 'pass\n'
    else
      printf 'fail\n'
    fi
    return 0
  fi
  # verdict=FAIL: measured blocked iff the detail field starts with a complete
  # platform/call rejection PREFIX (anchored, not a bare substring):
  #   detail=dlopen...                          libentry.so loader rejection
  #   detail=N0 runProbe smoke failed:...       native smoke sub-checks failed
  #   detail=N0 runProbe sub-check failed:...   native sub-check failed
  #   detail=n0_probe_smoke returned inconsistent status...
  #   detail=n0_probe_version returned null...
  #   detail=failed to build runProbe result... NAPI-layer native failure
  # Anything else — including 'detail=N0 runProbe smoke missing' (a NAPI
  # marshaling defect, not a platform rejection) and 'detail=N0 runProbe key
  # invalid' / 'version invalid' — is an unexpected marker (fail).
  detail="${fields[2]:-}"
  case "$detail" in
    "detail=dlopen"*) printf 'blocked\n' ;;
    "detail=N0 runProbe smoke failed:"*) printf 'blocked\n' ;;
    "detail=N0 runProbe sub-check failed:"*) printf 'blocked\n' ;;
    "detail=n0_probe_smoke returned inconsistent status"*) printf 'blocked\n' ;;
    "detail=n0_probe_version returned null"*) printf 'blocked\n' ;;
    "detail=failed to build runProbe result"*) printf 'blocked\n' ;;
    *) printf 'fail\n' ;;
  esac
}

# --- selftest: pure host checks, no network/HDC/Emulator, no evidence -------
selftest_run() {
  local rc=0
  local tmpdir
  local v
  local marker_pass marker_fail_dlopen marker_fail_smoke marker_fail_subcheck \
    marker_fail_version marker_detail_pollution marker_detail_pipe \
    marker_bad_version marker_bad_keylen marker_pass_polluted \
    marker_fail_smoke_missing marker_fail_key marker_bad_tickop marker_bad_ticksize \
    marker_bad_first_field
  local hilog_line resultmsg_line raw_line e1 e2 e3
  local teardown_output
  local existing newfile
  local seal_tmp expected_manifest_hash actual_manifest_hash
  local seal_pid seal_done seal_bytes seal_hash recomputed_hash
  local src_a src_b src_c n1 n2 n0
  local napi_line napi_smoke napi_x25519 napi_tunnel napi_tickop napi_ticksize
  local guest_log guest_code
  local rej_file
  tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/n0-emu24-selftest.XXXXXX")"
  trap 'rm -rf "$tmpdir"' RETURN

  printf 'SELFTEST_BEGIN=n0-emulator-run.sh\n'
  printf 'SELFTEST_MODE=pure-host no-network no-hdc no-emulator no-evidence\n'

  # --- marker parser: pass / blocked / fail classification -----------------
  marker_pass='N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0'
  marker_fail_dlopen='N0_CORE_PROBE_RESULT|verdict=FAIL|detail=dlopen libentry.so failed: cannot locate symbol n0_probe_version'
  marker_fail_smoke='N0_CORE_PROBE_RESULT|verdict=FAIL|detail=N0 runProbe smoke failed: smoke.ok=false x25519Ok=false tunnelOk=false'
  marker_fail_subcheck='N0_CORE_PROBE_RESULT|verdict=FAIL|detail=N0 runProbe sub-check failed: x25519Ok=false'
  marker_fail_version='N0_CORE_PROBE_RESULT|verdict=FAIL|detail=N0 runProbe version invalid: got n0-native-core/9.9.9'
  # counterexample: verdict=PASS and keyLen=44 phrases appear only inside the
  # FAIL detail -> must be fail, not pass
  marker_detail_pollution='N0_CORE_PROBE_RESULT|verdict=FAIL|detail=N0 runProbe version invalid: verdict=PASS keyLen=44 but the real fields are FAIL'
  # counterexample: pipe-separated verdict=PASS/keyLen=44 fragments appear
  # only inside the trailing detail field -> must be fail, not pass
  marker_detail_pipe='N0_CORE_PROBE_RESULT|verdict=FAIL|detail=inner log|verdict=PASS|keyLen=44'
  # counterexample: verdict=PASS with a wrong version or key length -> fail
  marker_bad_version='N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=wrong/1.0 keyLen=44 tickOp=0 tickSize=0'
  marker_bad_keylen='N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=43 tickOp=0 tickSize=0'
  # counterexample: verdict=PASS with a nonzero tickOp or tickSize -> fail
  marker_bad_tickop='N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=1 tickSize=0'
  marker_bad_ticksize='N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=16'
  # counterexample: 'smoke missing' is a NAPI marshaling defect, NOT a
  # platform rejection -> must be fail, not blocked (the blocked signature is
  # anchored to the full 'smoke failed:' prefix)
  marker_fail_smoke_missing='N0_CORE_PROBE_RESULT|verdict=FAIL|detail=N0 runProbe smoke missing'
  # counterexample: key invalid reports only keyLen (never the key value) and
  # is an unexpected marker -> fail
  marker_fail_key='N0_CORE_PROBE_RESULT|verdict=FAIL|detail=N0 runProbe key invalid: keyLen=43'
  # counterexample: verdict=PASS whose summary contains a FAIL-looking phrase
  # -> must still be pass (only field 2 decides the verdict)
  marker_pass_polluted='N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0 verdict=FAIL inside summary'
  # counterexample: marker whose first field is not verdict= -> malformed fail
  marker_bad_first_field='N0_CORE_PROBE_RESULT|foo=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0'

  v="$(judge_marker "$marker_pass")"
  if [[ "$v" != "pass" ]]; then
    printf 'SELFTEST FAIL judge pass: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_fail_dlopen")"
  if [[ "$v" != "blocked" ]]; then
    printf 'SELFTEST FAIL judge dlopen blocked: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_fail_smoke")"
  if [[ "$v" != "blocked" ]]; then
    printf 'SELFTEST FAIL judge smoke blocked: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_fail_subcheck")"
  if [[ "$v" != "blocked" ]]; then
    printf 'SELFTEST FAIL judge sub-check blocked: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_fail_version")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge version-invalid fail: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_detail_pollution")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge detail pollution: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_detail_pipe")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge detail pipe fragments: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_bad_version")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge bad version: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_bad_keylen")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge bad keyLen: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_bad_tickop")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge bad tickOp: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_bad_ticksize")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge bad tickSize: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_fail_smoke_missing")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge smoke-missing must fail: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_fail_key")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge key-invalid fail: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_pass_polluted")"
  if [[ "$v" != "pass" ]]; then
    printf 'SELFTEST FAIL judge pass polluted summary: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_marker "$marker_bad_first_field")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge marker first field must be verdict=: got %s\n' "$v"
    rc=1
  fi
  printf 'SELFTEST marker-parser=pass\n'

  # --- marker extraction: hilog prefix / printSync / ResultMsg -------------
  hilog_line='CST 2026-08-09 17:43:07.509  2814  2814 I A02900/N0NativeCoreTest: N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0'
  resultmsg_line='TestFinished-ResultMsg: N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0; user test finished.'
  raw_line='N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0'
  e1="$(extract_marker "$hilog_line")"
  e2="$(extract_marker "$resultmsg_line")"
  e3="$(extract_marker "$raw_line")"
  if [[ "$e1" != "$marker_pass" || "$e2" != "$marker_pass" || "$e3" != "$marker_pass" ]]; then
    printf 'SELFTEST FAIL extract marker: e1=%q e2=%q e3=%q\n' "$e1" "$e2" "$e3"
    rc=1
  fi
  printf 'SELFTEST marker-extract=pass\n'

  # --- distinct marker collection: three sources ---------------------------
  # same marker in all three sources -> 1 distinct; two different markers -> 2;
  # no marker anywhere -> 0.
  src_a="$tmpdir/src-a.log"
  src_b="$tmpdir/src-b.log"
  src_c="$tmpdir/src-c.log"
  printf '%s\n' "$marker_pass" >"$src_a"
  printf '%s\n' "$marker_pass" >"$src_b"
  printf '%s\n' "$marker_pass" >"$src_c"
  n1="$(collect_distinct_markers "$src_a" "$src_b" "$src_c" | wc -l | tr -d ' ')"
  if [[ "$n1" != "1" ]]; then
    printf 'SELFTEST FAIL distinct markers same-source: got %s want 1\n' "$n1"
    rc=1
  fi
  printf '%s\n' "$marker_fail_smoke" >"$src_b"
  n2="$(collect_distinct_markers "$src_a" "$src_b" "$src_c" | wc -l | tr -d ' ')"
  if [[ "$n2" != "2" ]]; then
    printf 'SELFTEST FAIL distinct markers different-source: got %s want 2\n' "$n2"
    rc=1
  fi
  : >"$src_a"
  : >"$src_b"
  : >"$src_c"
  n0="$(collect_distinct_markers "$src_a" "$src_b" "$src_c" | wc -l | tr -d ' ')"
  if [[ "$n0" != "0" ]]; then
    printf 'SELFTEST FAIL distinct markers empty: got %s want 0\n' "$n0"
    rc=1
  fi
  printf 'SELFTEST distinct-markers=pass\n'

  # --- distinct marker collection: three real prefixes ----------------------
  # The same marker appears with three different real prefixes (HiLog tag
  # line, aa-test printSync line, TestFinished-ResultMsg line); extraction
  # must collapse them to exactly one distinct marker.
  printf '%s\n' "$hilog_line" >"$src_a"
  printf '%s\n' "$resultmsg_line" >"$src_b"
  printf '%s\n' "$raw_line" >"$src_c"
  n1="$(collect_distinct_markers "$src_a" "$src_b" "$src_c" | wc -l | tr -d ' ')"
  if [[ "$n1" != "1" ]]; then
    printf 'SELFTEST FAIL distinct markers three real prefixes: got %s want 1\n' "$n1"
    rc=1
  fi
  printf 'SELFTEST distinct-markers-real-prefixes=pass\n'

  # --- NAPI field extraction -----------------------------------------------
  napi_line='N0_RUNPROBE|version=n0-native-core/0.1.0+boringtun-0.7.1|smokeOk=1|x25519Ok=1|tunnelOk=1|tickOp=0|tickSize=0'
  napi_smoke="$(napi_field "$napi_line" smokeOk)"
  napi_x25519="$(napi_field "$napi_line" x25519Ok)"
  napi_tunnel="$(napi_field "$napi_line" tunnelOk)"
  napi_tickop="$(napi_field "$napi_line" tickOp)"
  napi_ticksize="$(napi_field "$napi_line" tickSize)"
  if [[ "$napi_smoke" != "1" || "$napi_x25519" != "1" || "$napi_tunnel" != "1" || \
        "$napi_tickop" != "0" || "$napi_ticksize" != "0" ]]; then
    printf 'SELFTEST FAIL napi field extraction: smoke=%s x25519=%s tunnel=%s tickOp=%s tickSize=%s\n' \
      "$napi_smoke" "$napi_x25519" "$napi_tunnel" "$napi_tickop" "$napi_ticksize"
    rc=1
  fi
  printf 'SELFTEST napi-field=pass\n'

  # --- guest result code extraction ----------------------------------------
  guest_log="$tmpdir/aa-test.log"
  printf 'TestFinished-ResultCode: 0\n' >"$guest_log"
  guest_code="$(extract_guest_code "$guest_log")"
  if [[ "$guest_code" != "0" ]]; then
    printf 'SELFTEST FAIL guest code extraction: got %s want 0\n' "$guest_code"
    rc=1
  fi
  printf 'TestFinished-ResultCode: 1\n' >>"$guest_log"
  guest_code="$(extract_guest_code "$guest_log")"
  if [[ "$guest_code" != "1" ]]; then
    printf 'SELFTEST FAIL guest code last-match extraction: got %s want 1\n' "$guest_code"
    rc=1
  fi
  : >"$guest_log"
  guest_code="$(extract_guest_code "$guest_log")"
  if [[ -n "$guest_code" ]]; then
    printf 'SELFTEST FAIL guest code empty extraction: got %q want empty\n' "$guest_code"
    rc=1
  fi
  printf 'SELFTEST guest-code=pass\n'

  # --- no-marker loader rejection classification ---------------------------
  rej_file="$tmpdir/loader.log"
  # positive: complete 'initial-exec TLS resolves to dynamic definition'
  # phrase on a line referencing libentry.so
  printf 'libentry.so: initial-exec TLS resolves to dynamic definition\n' >"$rej_file"
  if ! is_loader_rejection "$rej_file"; then
    printf 'SELFTEST FAIL loader rejection initial-exec TLS\n'
    rc=1
  fi
  # positive: 'Error relocating' naming libentry.so without a symbol-missing
  # phrase (symbol-missing variants are excluded below)
  printf 'Error relocating libentry.so: n0_probe_version: relocation truncated to fit: R_X86_64_TPOFF64\n' >"$rej_file"
  if ! is_loader_rejection "$rej_file"; then
    printf 'SELFTEST FAIL loader rejection Error relocating\n'
    rc=1
  fi
  # positive: explicit dlopen load failure naming libentry.so
  printf 'dlopen failed: cannot open libentry.so\n' >"$rej_file"
  if ! is_loader_rejection "$rej_file"; then
    printf 'SELFTEST FAIL loader rejection dlopen\n'
    rc=1
  fi
  # counterexample: not-found error that also contains dlopen -> excluded
  printf 'dlopen failed: library "libentry.so" not found\n' >"$rej_file"
  if is_loader_rejection "$rej_file"; then
    printf 'SELFTEST FAIL loader rejection must reject not-found-with-dlopen\n'
    rc=1
  fi
  # counterexample: 'Error relocating' with a symbol-not-found phrase ->
  # excluded (symbol-missing is never measured blocked)
  printf 'Error relocating libentry.so: n0_probe_version: symbol not found\n' >"$rej_file"
  if is_loader_rejection "$rej_file"; then
    printf 'SELFTEST FAIL loader rejection must reject Error relocating symbol-not-found\n'
    rc=1
  fi
  printf 'libentry.so: cannot open shared object file: No such file or directory\n' >"$rej_file"
  if is_loader_rejection "$rej_file"; then
    printf 'SELFTEST FAIL loader rejection must reject not-found\n'
    rc=1
  fi
  printf 'libentry.so: undefined symbol: n0_probe_version\n' >"$rej_file"
  if is_loader_rejection "$rej_file"; then
    printf 'SELFTEST FAIL loader rejection must reject symbol-wrapping error\n'
    rc=1
  fi
  printf 'Error relocating someother.so: symbol not found\n' >"$rej_file"
  if is_loader_rejection "$rej_file"; then
    printf 'SELFTEST FAIL loader rejection must reject non-libentry line\n'
    rc=1
  fi
  : >"$rej_file"
  if is_loader_rejection "$rej_file"; then
    printf 'SELFTEST FAIL loader rejection must reject empty log\n'
    rc=1
  fi
  if is_loader_rejection "$tmpdir/does-not-exist.log"; then
    printf 'SELFTEST FAIL loader rejection must reject missing file\n'
    rc=1
  fi
  printf 'SELFTEST loader-rejection=pass\n'

  # --- guards: physical target / emulator target / port / instance / helpers
  if ( PHYS_1_TARGET=1; guard_physical_target ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard PHYS_1_TARGET\n'
    rc=1
  fi
  if ( TARGET=192.168.1.1:10000; guard_emulator_target ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard TARGET\n'
    rc=1
  fi
  if ( HDC_TARGET=192.168.1.1:10000; guard_emulator_target ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard HDC_TARGET\n'
    rc=1
  fi
  if ( guard_emulator_target "192.168.1.1:10000" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard EMULATOR_TARGET request\n'
    rc=1
  fi
  if ! ( guard_emulator_target "" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard EMULATOR_TARGET request positive\n'
    rc=1
  fi
  if ( HDC_PORT=5555; guard_hdc_port ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard HDC_PORT\n'
    rc=1
  fi
  if ( guard_emulator_instance "other_instance" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard EMULATOR_INSTANCE\n'
    rc=1
  fi
  if ! ( guard_emulator_instance "netbird_api24_phone" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard EMULATOR_INSTANCE positive\n'
    rc=1
  fi
  if ( guard_helpers "/tmp/evil-connect" "" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard CONNECT_HELPER override\n'
    rc=1
  fi
  if ( guard_helpers "" "/tmp/evil-stop" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard STOP_HELPER override\n'
    rc=1
  fi
  if ! ( guard_helpers "" "" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard helpers positive\n'
    rc=1
  fi
  if ! ( unset PHYS_1_TARGET TARGET HDC_TARGET HDC_PORT; \
         guard_physical_target && guard_emulator_target "" && guard_hdc_port && \
         guard_emulator_instance "" && guard_helpers "" "" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guards positive\n'
    rc=1
  fi
  printf 'SELFTEST guards=pass\n'

  # --- no-clobber -----------------------------------------------------------
  existing="$tmpdir/existing.log"
  newfile="$tmpdir/new.log"
  : >"$existing"
  if ( TRANSCRIPT="$existing"; check_no_clobber ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL no-clobber existing\n'
    rc=1
  fi
  if ! ( TRANSCRIPT="$newfile"; check_no_clobber ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL no-clobber new\n'
    rc=1
  fi
  printf 'SELFTEST no-clobber=pass\n'

  # --- manifest seal (real coverage: live child holding the transcript fd) --
  # The transcript is appended with O_APPEND; a child process that inherits
  # the transcript fd and stays alive must NOT block the seal (there is no tee
  # to wait for). The seal must complete in bounded time, record all six seal
  # fields (final_exit_code / run_status / fail_reason / transcript_final_bytes
  # / transcript_final_sha256 / manifest_sha256), and the transcript hash must
  # be recomputable from the first transcript_final_bytes bytes. The holder
  # sleep is killed in every path: no sleep is left behind.
  seal_tmp="$tmpdir/seal"
  mkdir -p "$seal_tmp"
  : >"$seal_tmp/transcript.log"
  printf 'evidence_id=EV-N0-EMU24-20260810-0002\n' >"$seal_tmp/manifest.txt"
  printf 'verdict=pass\n' >>"$seal_tmp/manifest.txt"
  (
    exec >>"$seal_tmp/transcript.log" 2>&1
    printf 'SELFTEST_SEAL_TRANSCRIPT_LINE=1\n'
    sleep 30 &
    printf '%s\n' "$!" >"$seal_tmp/holder.pid"
    TRANSCRIPT="$seal_tmp/transcript.log"
    MANIFEST="$seal_tmp/manifest.txt"
    seal_and_finalize
    seal_rc=$?
    # Simulate a post-seal append to the transcript (a stray child writing
    # after the seal): the recorded first-N-bytes hash must still recompute,
    # while the whole-file hash must differ.
    printf 'SELFTEST_POST_SEAL_APPEND=1\n' >>"$seal_tmp/transcript.log"
    holder_pid="$(cat "$seal_tmp/holder.pid")"
    kill "$holder_pid" 2>/dev/null || true
    wait "$holder_pid" 2>/dev/null || true
    printf '%s\n' "$seal_rc" >"$seal_tmp/seal.done"
    exit "$seal_rc"
  ) &
  seal_pid=$!
  seal_done=0
  for _ in $(seq 1 100); do
    if [[ -f "$seal_tmp/seal.done" ]]; then
      seal_done=1
      break
    fi
    sleep 0.1
  done
  if (( seal_done != 1 )); then
    kill "$seal_pid" 2>/dev/null || true
    if [[ -f "$seal_tmp/holder.pid" ]]; then
      kill "$(cat "$seal_tmp/holder.pid")" 2>/dev/null || true
    fi
    printf 'SELFTEST FAIL seal must complete in bounded time with a live child holding the transcript fd\n'
    rc=1
  fi
  wait "$seal_pid" 2>/dev/null || true
  if ! grep -q '^transcript_final_sha256=' "$seal_tmp/manifest.txt"; then
    printf 'SELFTEST FAIL seal transcript hash\n'
    rc=1
  fi
  if ! grep -q '^manifest_sha256=' "$seal_tmp/manifest.txt"; then
    printf 'SELFTEST FAIL seal manifest hash\n'
    rc=1
  fi
  if ! grep -q '^final_exit_code=0$' "$seal_tmp/manifest.txt"; then
    printf 'SELFTEST FAIL seal final_exit_code\n'
    rc=1
  fi
  if ! grep -q '^run_status=' "$seal_tmp/manifest.txt"; then
    printf 'SELFTEST FAIL seal run_status\n'
    rc=1
  fi
  if ! grep -q '^fail_reason=' "$seal_tmp/manifest.txt"; then
    printf 'SELFTEST FAIL seal fail_reason\n'
    rc=1
  fi
  if ! grep -q '^transcript_final_bytes=' "$seal_tmp/manifest.txt"; then
    printf 'SELFTEST FAIL seal transcript_final_bytes\n'
    rc=1
  fi
  # transcript_final_sha256 must equal the sha256 of the first
  # transcript_final_bytes bytes (head -c N recompute) even after the
  # simulated post-seal append; the whole-file hash must differ (the append
  # is real and the recorded first-N-bytes hash stays valid).
  seal_bytes="$(grep '^transcript_final_bytes=' "$seal_tmp/manifest.txt" | cut -d= -f2)"
  seal_hash="$(grep '^transcript_final_sha256=' "$seal_tmp/manifest.txt" | cut -d= -f2)"
  recomputed_hash="$(head -c "$seal_bytes" "$seal_tmp/transcript.log" | sha256sum | awk '{print $1}')"
  if [[ "$recomputed_hash" != "$seal_hash" ]]; then
    printf 'SELFTEST FAIL seal transcript hash recompute mismatch: expected %s got %s\n' \
      "$seal_hash" "$recomputed_hash"
    rc=1
  fi
  whole_hash="$(sha256sum "$seal_tmp/transcript.log" | awk '{print $1}')"
  if [[ "$whole_hash" == "$seal_hash" ]]; then
    printf 'SELFTEST FAIL post-seal append must change the whole-file hash\n'
    rc=1
  fi
  expected_manifest_hash="$(grep -v '^manifest_sha256=' "$seal_tmp/manifest.txt" | sha256sum | awk '{print $1}')"
  actual_manifest_hash="$(grep '^manifest_sha256=' "$seal_tmp/manifest.txt" | cut -d= -f2)"
  if [[ "$expected_manifest_hash" != "$actual_manifest_hash" ]]; then
    printf 'SELFTEST FAIL seal manifest self-hash mismatch: expected %s got %s\n' \
      "$expected_manifest_hash" "$actual_manifest_hash"
    rc=1
  fi
  printf 'SELFTEST manifest-seal=pass\n'

  # --- teardown no-op -------------------------------------------------------
  emulator_started=0
  installed=0
  if ! teardown >/dev/null 2>&1; then
    printf 'SELFTEST FAIL teardown no-op\n'
    rc=1
  fi
  printf 'SELFTEST cleanup=pass\n'

  # --- teardown no-device-phase: no hdc kill / no residual port scan --------
  # A precondition failure (device_phase_started=0) must never issue hdc kill
  # or scan residual ports; only the formal device phase performs the full
  # cleanup.
  teardown_output="$(SELFTEST=0 DRY_RUN=0 device_phase_started=0 emulator_started=0 installed=0 \
    MANIFEST= TRANSCRIPT= teardown 2>&1 || true)"
  if [[ "$teardown_output" != *'CLEANUP_HDC=skipped-no-device-phase'* ]]; then
    printf 'SELFTEST FAIL teardown no-device-phase must skip hdc kill\n'
    rc=1
  fi
  if [[ "$teardown_output" == *'CLEANUP_HDC=kill-issued'* ]]; then
    printf 'SELFTEST FAIL teardown no-device-phase must not issue hdc kill\n'
    rc=1
  fi
  if [[ "$teardown_output" != *'RESIDUAL_STATE=skipped-no-device-phase'* ]]; then
    printf 'SELFTEST FAIL teardown no-device-phase must skip residual scan\n'
    rc=1
  fi
  printf 'SELFTEST teardown-no-device-phase=pass\n'

  if (( rc == 0 )); then
    printf 'SELFTEST_RESULT=PASS\n'
  else
    printf 'SELFTEST_RESULT=FAIL\n'
  fi
  return $rc
}

# --- preconditions: guards + repository state (BEFORE any evidence file) ---
# All non-provable preconditions are evaluated before no-clobber and before
# any raw file is created: a formal run that fails a precondition leaves no
# evidence files behind. Dry-run records the state but explicitly exempts git
# clean / runner-in-HEAD (it is a rehearsal that may run from an uncommitted
# tree; no evidence is made).
if (( SELFTEST != 1 )); then
  guard_physical_target || fail "PHYS_1_TARGET is set; this runner is Emulator-only and must never target a physical device"
  guard_emulator_target || fail "TARGET/HDC_TARGET/EMULATOR_TARGET is not the fixed $EMULATOR_TARGET Emulator target"
  guard_hdc_port || fail "HDC_PORT=${HDC_PORT:-<unset>}; only the fixed 10000 Emulator HDC port is allowed"
  guard_emulator_instance || fail "EMULATOR_INSTANCE request is not the fixed netbird_api24_phone Emulator instance"
  guard_helpers || fail "CONNECT_HELPER/STOP_HELPER override is forbidden; helpers are hardcoded"
  # WORKSPACE must be the script repository itself (no override to another repo).
  if [[ "$WORKSPACE" != "$DEFAULT_WORKSPACE" ]]; then
    fail "WORKSPACE override is not the script repository: $WORKSPACE (expected $DEFAULT_WORKSPACE)"
  fi
  # EVIDENCE_ID must match the schema format; this also makes the guest
  # staging path /data/local/tmp/$EVIDENCE_ID shell-safe.
  if [[ ! "$EVIDENCE_ID" =~ ^EV-[A-Z0-9]+-[A-Z0-9]+-[0-9]{8}-[0-9]{4}$ ]]; then
    fail "EVIDENCE_ID does not match EV-<gate>-<target>-<YYYYMMDD>-<NNNN>: $EVIDENCE_ID"
  fi
  if [[ ! -d "$WORKSPACE/.git" ]]; then
    fail "WORKSPACE is not a git repository: $WORKSPACE"
  fi
  code_sha="$(git -C "$WORKSPACE" rev-parse HEAD)"
  git_branch="$(git -C "$WORKSPACE" rev-parse --abbrev-ref HEAD)"
  if ! git -C "$WORKSPACE" cat-file -e "$SNAPSHOT_HEAD^{commit}" 2>/dev/null; then
    fail "snapshot commit $SNAPSHOT_HEAD is not present in the repository"
  fi
  git_clean=0
  if git -C "$WORKSPACE" diff --quiet && git -C "$WORKSPACE" diff --cached --quiet && \
     [[ -z "$(git -C "$WORKSPACE" ls-files --others --exclude-standard)" ]]; then
    git_clean=1
  fi
  runner_in_head=0
  if git -C "$WORKSPACE" cat-file -e "HEAD:spikes/n0-native-core/n0-emulator-run.sh" 2>/dev/null; then
    runner_in_head=1
  fi
  if (( DRY_RUN != 1 )); then
    # Formal run: the evidence must map to a committed code_sha and the runner
    # itself must be part of that commit.
    if (( git_clean != 1 )); then
      fail "working tree is not clean; formal evidence must map to a committed code_sha"
    fi
    if (( runner_in_head != 1 )); then
      fail "HEAD does not contain spikes/n0-native-core/n0-emulator-run.sh; commit the runner before a formal run"
    fi
  fi
fi

# --- evidence setup (selftest writes no evidence) ---------------------------
if (( SELFTEST == 1 )); then
  :
else
  mkdir -p "$EVIDENCE_ROOT"
  TRANSCRIPT="$EVIDENCE_ROOT/$EVIDENCE_ID-transcript.log"
  TAG_HILOG="$EVIDENCE_ROOT/$EVIDENCE_ID-hilog-tag.log"
  APP_HILOG="$EVIDENCE_ROOT/$EVIDENCE_ID-hilog-app-full.log"
  MANIFEST="$EVIDENCE_ROOT/$EVIDENCE_ID-manifest.txt"
  CONSOLE="$EVIDENCE_ROOT/$EVIDENCE_ID-emulator-console.log"
  BUILD_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-build.log"
  N0_BUILD_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-n0-build.log"
  SNAPSHOT_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-snapshot-prep.log"
  AA_TEST_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-aa-test.log"
  SOURCE_MANIFEST="$EVIDENCE_ROOT/$EVIDENCE_ID-source-manifest.txt"
  SCREENSHOT="$EVIDENCE_ROOT/$EVIDENCE_ID-run1.png"
  check_no_clobber || exit 2
  if (( DRY_RUN == 1 )); then
    # dry-run: no evidence files; build logs go to a temp dir, transcript is
    # plain stdout.
    DRY_TMP="$(mktemp -d "${TMPDIR:-/tmp}/n0-emu24-dryrun.XXXXXX")"
    BUILD_LOG="$DRY_TMP/build.log"
    N0_BUILD_LOG="$DRY_TMP/n0-build.log"
    SNAPSHOT_LOG="$DRY_TMP/snapshot-prep.log"
    AA_TEST_LOG="$DRY_TMP/aa-test.log"
    SOURCE_MANIFEST="$DRY_TMP/source-manifest.txt"
    TRANSCRIPT=""
    TAG_HILOG=""
    APP_HILOG=""
    MANIFEST=""
    CONSOLE=""
    SCREENSHOT=""
  else
    # Formal run: create the empty transcript (no-clobber already verified)
    # and append the whole run to it with O_APPEND. There is no tee process
    # and no fd 3: the transcript is written directly, so the seal never
    # waits on a child holding the stream open and always completes in
    # bounded time.
    : >"$TRANSCRIPT"
    exec >>"$TRANSCRIPT" 2>&1
  fi
fi

# --- selftest mode: run and exit before any evidence/HDC/Emulator work ------
if (( SELFTEST == 1 )); then
  selftest_run
  exit $?
fi

# --- fixed baseline header --------------------------------------------------
started_at="$(date --iso-8601=seconds)"
printf 'RUNNER=n0-emulator-run.sh\n'
printf 'EVIDENCE_ID=%s\n' "$EVIDENCE_ID"
printf 'STARTED_AT=%s\n' "$started_at"
printf 'CLOCK_SOURCE=host_CLOCK_REALTIME_date_iso_8601_seconds\n'
printf 'TIMEZONE=%s\n' "$(date +%Z%:z)"
printf 'RECORD_SCOPE=N0_b_single_native_WireGuard_core_BoringTun_0.7.1_ffi-bindings_API24_x86_64_Emulator_load_smoke\n'
printf 'TARGET_TUPLE=%s\n' "$TARGET_TUPLE"
printf 'EMULATOR_INSTANCE=%s\n' "$EMULATOR_INSTANCE"
printf 'EMULATOR_TARGET=%s\n' "$EMULATOR_TARGET"
printf 'BUNDLE=%s\n' "$BUNDLE"
printf 'TEST_MODULE=%s\n' "$TEST_MODULE"
printf 'TEST_RUNNER=%s\n' "$TEST_RUNNER"
printf 'SNAPSHOT_COMMIT=%s\n' "$SNAPSHOT_HEAD"
printf 'BORINGTUN_VERSION=%s\n' "$BORINGTUN_VERSION"
printf 'BORINGTUN_CRATE_SHA256=%s\n' "$BORINGTUN_CRATE_SHA256"
printf 'PHYSICAL_DEVICE_USED=false\n'
printf 'E8_STATUS=CLOSED\n'
printf 'TEST_RUNNER_USED=true\n'
printf 'PUBLIC_NETWORK_ALLOWED=false\n'
printf 'WORKSPACE=%s\n' "$WORKSPACE"
printf 'EVIDENCE_ROOT=%s\n' "$EVIDENCE_ROOT"
# Two distinct recorded inputs: the stable build SDK and the beta Emulator
# runtime are never mixed (see the toolchain paths section).
printf 'BUILD_TOOLCHAIN=command-line-tools/6.1.1.290 (stable; hvigorw/ohpm/native SDK for build)\n'
printf 'EMULATOR_RUNTIME=command-line-tools/26.0.0.461 (beta; Emulator binary + hdc)\n'
# Real native SDK version/api read from oh-uni-package.json (never hardcoded).
printf 'NATIVE_SDK_VERSION=%s\n' "$native_sdk_version"
printf 'NATIVE_SDK_API=%s\n' "$native_sdk_api"
printf 'NATIVE_SDK_RELEASE_TYPE=%s\n' "$native_sdk_release"
if (( DRY_RUN == 1 )); then
  printf 'EXECUTION=dry-run\n'
fi

# --- repository state (evaluated before evidence setup; recorded here) ------
printf 'GIT_HEAD=%s\n' "$code_sha"
printf 'GIT_BRANCH=%s\n' "$git_branch"
printf 'SNAPSHOT_COMMIT_PRESENT=pass\n'
printf 'GIT_CLEAN=%s\n' "$([ "$git_clean" = 1 ] && printf yes || printf no)"
printf 'RUNNER_IN_HEAD=%s\n' "$([ "$runner_in_head" = 1 ] && printf yes || printf no)"

# --- host tool checks -------------------------------------------------------
host_missing=0
check_tool() {
  local name="$1" path="$2"
  if [[ -x "$path" ]]; then
    printf 'HOST_CHECK %s=pass path=%s\n' "$name" "$path"
    return 0
  fi
  printf 'HOST_CHECK %s=fail path=%s\n' "$name" "$path"
  host_missing=$((host_missing + 1))
  return 1
}

check_command() {
  local name="$1" cmd="$2"
  if command -v "$cmd" >/dev/null 2>&1; then
    printf 'HOST_CHECK %s=pass command=%s\n' "$name" "$cmd"
    return 0
  fi
  printf 'HOST_CHECK %s=fail command=%s\n' "$name" "$cmd"
  host_missing=$((host_missing + 1))
  return 1
}

check_tool hvigorw "$HVIGOR" || true
check_tool ohpm "$OHPM" || true
check_tool hvigor-ohos-plugin "$BUILD_TOOLS/hvigor/hvigor-ohos-plugin" || true
check_tool emulator "$EMULATOR" || true
check_tool hdc "$HDC" || true
check_tool ohos-x86_64-clang "$NATIVE_HOME/llvm/bin/x86_64-unknown-linux-ohos-clang" || true
if [[ -f "$NAPI_HEADER" ]]; then
  printf 'HOST_CHECK napi-header=pass path=%s\n' "$NAPI_HEADER"
else
  printf 'HOST_CHECK napi-header=fail path=%s\n' "$NAPI_HEADER"
  host_missing=$((host_missing + 1))
fi
# DEVECO_SDK_HOME is forced to the stable SDK above; verify the resolved
# native SDK path and that the real version/api were read from
# oh-uni-package.json.
resolved_native_sdk="$(readlink -f "$DEVECO_SDK_HOME/default/openharmony/native" 2>/dev/null || true)"
if [[ -z "$resolved_native_sdk" || ! -d "$resolved_native_sdk" ]]; then
  printf 'HOST_CHECK dev eco-sdk-home=fail path=%s\n' "$DEVECO_SDK_HOME"
  host_missing=$((host_missing + 1))
else
  printf 'HOST_CHECK dev eco-sdk-home=pass path=%s\n' "$resolved_native_sdk"
fi
if [[ -z "$native_sdk_version" || -z "$native_sdk_api" ]]; then
  printf 'HOST_CHECK oh-uni-package=fail path=%s\n' "$SDK_UNI_PACKAGE"
  host_missing=$((host_missing + 1))
else
  printf 'HOST_CHECK oh-uni-package=pass version=%s api=%s release=%s\n' \
    "$native_sdk_version" "$native_sdk_api" "$native_sdk_release"
fi
check_command bash bash || true
check_command readelf readelf || true
check_command file file || true
check_command unzip unzip || true
check_command ss ss || true
check_command git git || true
check_command tar tar || true
check_command timeout timeout || true
check_command pgrep pgrep || true
check_command mktemp mktemp || true
check_command sha256sum sha256sum || true
check_command awk awk || true
check_command sed sed || true
check_command grep grep || true
check_command rg rg || true
check_command node node || true
if command -v shellcheck >/dev/null 2>&1; then
  printf 'HOST_CHECK shellcheck=pass command=shellcheck\n'
else
  printf 'HOST_CHECK shellcheck=fail command=shellcheck (external bash -n required)\n'
fi
if (( host_missing > 0 )); then
  fail "missing required host tools ($host_missing)"
fi

# --- 1. offline dual-ABI build (build.sh: cargo --offline --locked) ---------
if ! (
  cd "$PROJECT_DIR"
  print_command bash "$PROJECT_DIR/build.sh"
  bash "$PROJECT_DIR/build.sh"
) 2>&1 | tee "$N0_BUILD_LOG"; then
  fail "N0 offline dual-ABI build failed (see $N0_BUILD_LOG)"
fi
[[ -f "$PROJECT_DIR/out/x86_64/libentry.so" ]] || fail "build did not produce out/x86_64/libentry.so"
[[ -f "$PROJECT_DIR/out/aarch64/libentry.so" ]] || fail "build did not produce out/aarch64/libentry.so"
printf 'N0_BUILD_VERDICT=pass\n'

# --- 2. fixed r1 snapshot preparation ---------------------------------------
snapshot_dir="$(mktemp -d "${TMPDIR:-/tmp}/n0-emu24-snapshot.XXXXXX")"
if ! (
  print_command bash "$PROJECT_DIR/prepare-hap-snapshot.sh" "$snapshot_dir"
  bash "$PROJECT_DIR/prepare-hap-snapshot.sh" "$snapshot_dir"
) 2>&1 | tee "$SNAPSHOT_LOG"; then
  fail "snapshot preparation failed (see $SNAPSHOT_LOG)"
fi
[[ -f "$snapshot_dir/entry/src/main/cpp/n0_overlay.cpp" ]] || fail "staged snapshot missing n0_overlay.cpp"
[[ -f "$snapshot_dir/entry/src/main/ets/pages/runProbeTest.ets" ]] || fail "staged snapshot missing runProbeTest.ets"
[[ -f "$snapshot_dir/entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets" ]] || fail "staged snapshot missing N0 test runner"
grep -q "runN0ProbeTest" "$snapshot_dir/entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets" || fail "staged test runner missing runN0ProbeTest"
grep -q "N0_CORE_PROBE_RESULT" "$snapshot_dir/entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets" || fail "staged test runner missing N0_CORE_PROBE_RESULT"
grep -q "libentry.so" "$snapshot_dir/entry/oh-package.json5" || fail "staged oh-package.json5 missing libentry.so dependency"
grep -q "libentry.so" "$snapshot_dir/entry/oh-package-lock.json5" || fail "staged oh-package-lock.json5 missing libentry.so entry"
grep -q "x86_64" "$snapshot_dir/entry/build-profile.json5" || fail "staged build-profile.json5 missing x86_64 abiFilter"
if grep -q "arm64-v8a" "$snapshot_dir/entry/build-profile.json5"; then
  fail "staged build-profile.json5 still contains arm64-v8a abiFilter"
fi
[[ -f "$snapshot_dir/entry/libs/x86_64/libentry.so" ]] || fail "staged snapshot missing libs/x86_64/libentry.so"
printf 'SNAPSHOT_STAGE_VERIFY=pass\n'
printf 'SNAPSHOT_DIR=%s\n' "$snapshot_dir"

plugin_path="$(grep -oP "file:\K[^']+" "$snapshot_dir/hvigor/hvigor-config.json5" | head -1 || true)"
if [[ "$plugin_path" != "$BUILD_TOOLS/hvigor/hvigor-ohos-plugin" ]]; then
  fail "snapshot hvigor plugin path $plugin_path != $BUILD_TOOLS/hvigor/hvigor-ohos-plugin"
fi
[[ -d "$plugin_path" ]] || fail "hvigor plugin directory missing: $plugin_path"
printf 'HVIGOR_PLUGIN_VERIFY=pass\n'

# --- 3. ohpm install + hvigor clean + assembleHap (app + test) --------------
if ! (
  cd "$snapshot_dir"
  print_command "$OHPM" install --all
  "$OHPM" install --all
  print_command "$HVIGOR" clean --mode module -p product=default -p module=entry@default \
    -p buildMode=debug --no-daemon
  "$HVIGOR" clean --mode module -p product=default -p module=entry@default \
    -p buildMode=debug --no-daemon
  print_command "$HVIGOR" assembleHap --mode module -p product=default -p module=entry@default \
    -p buildMode=debug --no-daemon
  "$HVIGOR" assembleHap --mode module -p product=default -p module=entry@default \
    -p buildMode=debug --no-daemon
  print_command "$HVIGOR" assembleHap --mode module -p product=default -p module=entry@ohosTest \
    -p buildMode=debug --no-daemon
  "$HVIGOR" assembleHap --mode module -p product=default -p module=entry@ohosTest \
    -p buildMode=debug --no-daemon
) 2>&1 | tee "$BUILD_LOG"; then
  fail "HAP build failed (see $BUILD_LOG)"
fi
app_hap="$snapshot_dir/$APP_HAP_REL"
test_hap="$snapshot_dir/$TEST_HAP_REL"
[[ -f "$app_hap" ]] || fail "clean build did not produce application HAP"
[[ -f "$test_hap" ]] || fail "clean build did not produce test HAP"
printf 'HAP_BUILD_VERDICT=pass\n'

# --- 4. HAP member identity -------------------------------------------------
app_member="$(mktemp "${TMPDIR:-/tmp}/n0-emu24-app-member.XXXXXX")"
test_member="$(mktemp "${TMPDIR:-/tmp}/n0-emu24-test-member.XXXXXX")"
if ! unzip -p "$app_hap" libs/x86_64/libentry.so >"$app_member"; then
  fail "unzip failed to extract libentry.so from application HAP"
fi
if ! unzip -p "$test_hap" libs/x86_64/libentry.so >"$test_member"; then
  fail "unzip failed to extract libentry.so from test HAP"
fi
[[ -s "$app_member" ]] || fail "application HAP has no libs/x86_64/libentry.so member"
[[ -s "$test_member" ]] || fail "test HAP has no libs/x86_64/libentry.so member"
built_sha="$(sha256sum "$PROJECT_DIR/out/x86_64/libentry.so" | awk '{print $1}')"
app_member_sha="$(sha256sum "$app_member" | awk '{print $1}')"
test_member_sha="$(sha256sum "$test_member" | awk '{print $1}')"
printf 'APP_MEMBER_SHA256=%s\n' "$app_member_sha"
printf 'TEST_MEMBER_SHA256=%s\n' "$test_member_sha"
if [[ "$app_member_sha" != "$built_sha" || "$test_member_sha" != "$built_sha" ]]; then
  fail "HAP libentry.so member is not byte-equal to out/x86_64/libentry.so"
fi
printf 'HAP_MEMBER_IDENTITY=pass\n'
# Capture the unzip member lists FIRST (no pipefail-sensitive unzip|grep
# pipelines), then check the captured lists.
app_list="$(unzip -Z1 "$app_hap" 2>&1 || true)"
test_list="$(unzip -Z1 "$test_hap" 2>&1 || true)"
if grep -E 'libs/arm64-v8a/' <<<"$app_list" >/dev/null || \
   grep -E 'libs/arm64-v8a/' <<<"$test_list" >/dev/null; then
  fail "arm64 member present in application or test HAP"
fi
printf 'ARM64_MEMBER=false\n'
if grep -E 'libgoprobe|libtls-|libneededprobe' <<<"$app_list" >/dev/null || \
   grep -E 'libgoprobe|libtls-|libneededprobe' <<<"$test_list" >/dev/null; then
  fail "forbidden historical member in application or test HAP"
fi
printf 'FORBIDDEN_HISTORICAL_MEMBER=false\n'

# --- 5. HAP identity (module.json) ------------------------------------------
app_module_json="$(unzip -p "$app_hap" module.json || true)"
test_module_json="$(unzip -p "$test_hap" module.json || true)"
printf 'APP_HAP_MODULE_JSON=%q\n' "$app_module_json"
printf 'TEST_HAP_MODULE_JSON=%q\n' "$test_module_json"
if [[ "$app_module_json" != *'"bundleName":"cn.alfadb.netbird.r1probe"'* ||
      "$app_module_json" != *'"name":"entry"'* ||
      "$app_module_json" != *'"mainElement":"EntryAbility"'* ]]; then
  fail "application HAP identity mismatch (expected r1probe bundle, entry module, EntryAbility)"
fi
if [[ "$test_module_json" != *'"name":"entry_test"'* ||
      "$test_module_json" != *'"name":"OpenHarmonyTestRunner"'* ]]; then
  fail "test HAP identity mismatch (expected entry_test module with OpenHarmonyTestRunner)"
fi
printf 'HAP_IDENTITY=pass\n'
run sha256sum "$app_hap" "$test_hap"

# --- 6. source / lock / crate / toolchain / artifact hashes -----------------
source_inputs=(
  spikes/n0-native-core/Cargo.toml
  spikes/n0-native-core/Cargo.lock
  spikes/n0-native-core/src/lib.rs
  spikes/n0-native-core/napi/n0_overlay.cpp
  spikes/n0-native-core/napi/types/index.d.ts
  spikes/n0-native-core/napi/types/oh-package.json5
  spikes/n0-native-core/napi/runProbeTest.ets
  spikes/n0-native-core/napi/ohosTest/OpenHarmonyTestRunner.ets
  spikes/n0-native-core/build.sh
  spikes/n0-native-core/prepare-hap-snapshot.sh
  spikes/n0-native-core/n0-emulator-run.sh
  spikes/n0-native-core/README.md
)
(
  cd "$WORKSPACE"
  sha256sum "${source_inputs[@]}"
) >"$SOURCE_MANIFEST"
staged_inputs=(
  entry/src/main/cpp/n0_overlay.cpp
  entry/src/main/cpp/types/libentry/index.d.ts
  entry/src/main/cpp/types/libentry/oh-package.json5
  entry/src/main/ets/pages/runProbeTest.ets
  entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets
  entry/oh-package.json5
  entry/oh-package-lock.json5
  entry/build-profile.json5
  entry/libs/x86_64/libentry.so
)
(
  cd "$snapshot_dir"
  sha256sum "${staged_inputs[@]}"
) >>"$SOURCE_MANIFEST"
run sha256sum "$SOURCE_MANIFEST"
printf 'SOURCE_INPUT_COUNT=%s\n' "$(( ${#source_inputs[@]} + ${#staged_inputs[@]} ))"

CRATE_FILE="$(find "${CARGO_HOME:-$HOME/.cargo}/registry/cache" -name "boringtun-${BORINGTUN_VERSION}.crate" 2>/dev/null | head -n1)"
[[ -n "$CRATE_FILE" ]] || fail "boringtun-${BORINGTUN_VERSION}.crate not found in cargo cache"
crate_sha="$(sha256sum "$CRATE_FILE" | awk '{print $1}')"
[[ "$crate_sha" == "$BORINGTUN_CRATE_SHA256" ]] || fail "boringtun crate checksum mismatch: got $crate_sha"
printf 'BORINGTUN_CRATE_SHA256_VERIFY=pass\n'
lock_sha="$(grep -A3 'name = "boringtun"' "$PROJECT_DIR/Cargo.lock" | grep checksum | awk '{print $3}' | tr -d '"')"
[[ "$lock_sha" == "$BORINGTUN_CRATE_SHA256" ]] || fail "Cargo.lock boringtun checksum mismatch: got $lock_sha"
printf 'CARGO_LOCK_BORINGTUN_VERIFY=pass\n'

printf 'TOOLCHAIN host=%q\n' "$(uname -a)"
run "$HVIGOR" --version
run "$HDC" -v
run node --version
run "$OHPM" --version
run rustc --version
run cargo --version
run "$NATIVE_HOME/llvm/bin/clang" --version

aarch64_sha="$(sha256sum "$PROJECT_DIR/out/aarch64/libentry.so" | awk '{print $1}')"
printf 'LIBENTRY_X86_64_SHA256=%s\n' "$built_sha"
printf 'LIBENTRY_AARCH64_SHA256=%s\n' "$aarch64_sha"
run file "$PROJECT_DIR/out/x86_64/libentry.so"
# readelf output is captured FIRST and the NEEDED lines are printed from the
# captured output: a `readelf | grep` pipeline under `set -o pipefail` would
# fail spuriously (grep -q exits early on a match and SIGPIPEs readelf).
readelf_diag="$(readelf -d "$PROJECT_DIR/out/x86_64/libentry.so" || true)"
while IFS= read -r line; do
  if [[ "$line" == *'NEEDED'* ]]; then
    printf '%s\n' "$line"
  fi
done <<<"$readelf_diag"
printf 'ARTIFACT_HASH_VERIFY=pass\n'

# --- public endpoint source scan (also in dry-run; fail-closed) -----------
# rg exit codes: 0 = match found, 1 = no match, 2 = error. ONLY rc=1 (clean
# scan) passes; every other rc fails the run.
set +e
scan_output="$(rg -n '(114\.114\.114\.114|8\.8\.8\.8|https?://)' \
    "$PROJECT_DIR/n0-emulator-run.sh" "$PROJECT_DIR/build.sh" \
    "$PROJECT_DIR/prepare-hap-snapshot.sh" "$PROJECT_DIR/src/lib.rs" \
    "$PROJECT_DIR/napi/n0_overlay.cpp" "$PROJECT_DIR/napi/runProbeTest.ets" \
    "$PROJECT_DIR/napi/ohosTest/OpenHarmonyTestRunner.ets" 2>&1)"
scan_rc=$?
set -e
if (( scan_rc != 1 )); then
  if (( scan_rc == 0 )); then
    printf '%s\n' "$scan_output"
    fail "public endpoint literal exists in N0 executable inputs"
  fi
  fail "rg public endpoint source scan failed (rc=$scan_rc)"
fi
printf 'PUBLIC_ENDPOINT_SOURCE_SCAN=pass\n'

# --- dry-run: stop here, no device action, no evidence file -----------------
if (( DRY_RUN == 1 )); then
  result="pass"
  printf 'DRY_RUN_VERDICT=pass\n'
  printf 'DRY_RUN_NOTE=no device action, no evidence file created; formal run additionally requires git clean and runner in HEAD\n'
  exit 0
fi

# ============================================================================
# Formal run: device phase
# ============================================================================

# --- 7. Emulator cold boot --------------------------------------------------
# First Emulator/HDC action: from here on the device phase is active and the
# EXIT-trap teardown performs the full hdc kill / residual port cleanup.
device_phase_started=1
"$HDC" kill >/dev/null 2>&1 </dev/null || true
HDC_PORT="$EMULATOR_HDC_PORT" timeout 60 "$STOP_HELPER" >/dev/null 2>&1 </dev/null || true
if pgrep -f '/emulator/Emulator.*-start '"$EMULATOR_INSTANCE"'|qemu-system.*'"$EMULATOR_INSTANCE" >/dev/null; then
  fail "residual Emulator exists before cold boot"
fi
if [[ -f "$QEMU_LOG" ]]; then
  qemu_start_line=$(( $(wc -l <"$QEMU_LOG") + 1 ))
else
  mkdir -p "$(dirname "$QEMU_LOG")"
  : >"$QEMU_LOG"
  qemu_start_line=1
fi
printf 'QEMU_CURRENT_BOOT_START_LINE=%s\n' "$qemu_start_line"

print_command env DISPLAY="$EMULATOR_DISPLAY" XAUTHORITY="$EMULATOR_XAUTHORITY" \
  XDG_RUNTIME_DIR="$EMULATOR_XDG_RUNTIME_DIR" "$EMULATOR" -start "$EMULATOR_INSTANCE" \
  -instancePath "$EMULATOR_INSTANCE_PATH" -imageRoot "$EMULATOR_IMAGE_ROOT" \
  -bootMode coldboot -hdcport "$EMULATOR_HDC_PORT"
DISPLAY="$EMULATOR_DISPLAY" XAUTHORITY="$EMULATOR_XAUTHORITY" XDG_RUNTIME_DIR="$EMULATOR_XDG_RUNTIME_DIR" \
  "$EMULATOR" -start "$EMULATOR_INSTANCE" \
  -instancePath "$EMULATOR_INSTANCE_PATH" \
  -imageRoot "$EMULATOR_IMAGE_ROOT" \
  -bootMode coldboot -hdcport "$EMULATOR_HDC_PORT" >"$CONSOLE" 2>&1 </dev/null &
emulator_pid=$!
emulator_started=1
printf 'EMULATOR_PID=%s\n' "$emulator_pid"
sleep 2
if ! kill -0 "$emulator_pid" 2>/dev/null; then
  fail "Emulator process exited during startup"
fi
printf 'EMULATOR_START_VERDICT=pass\n'

connected=0
for attempt in $(seq 1 80); do
  HDC_PORT="$EMULATOR_HDC_PORT" timeout 30 "$CONNECT_HELPER" >/dev/null 2>&1 </dev/null || true
  shell_probe="$(hdc shell "echo n0-emu24-connect-$attempt" 2>&1 || true)"
  distribution="$(hdc shell 'param get const.product.os.dist.name' 2>&1 | tr -d '\r' || true)"
  printf 'CONNECTIVITY attempt=%s shell=%q distribution=%q\n' \
    "$attempt" "$shell_probe" "$distribution"
  if [[ "$shell_probe" == "n0-emu24-connect-$attempt" && "$distribution" == *HarmonyOS* ]]; then
    connected=1
    break
  fi
  sleep 3
done
if (( connected != 1 )); then
  printf 'CONNECTIVITY_VERDICT=blocked\n'
  exit 1
fi
printf 'CONNECTIVITY_VERDICT=pass\n'

ready=0
for boot_attempt in $(seq 1 180); do
  qemu_boot_complete="$(tail -n +"$qemu_start_line" "$QEMU_LOG" | grep -F 'guest os boot completed.' | tail -1 || true)"
  shell_readiness="$(hdc shell "echo n0-emu24-readiness-$boot_attempt" 2>&1 || true)"
  printf 'BOOT_READINESS attempt=%s qemu.boot=%q shell=%q\n' \
    "$boot_attempt" "$qemu_boot_complete" "$shell_readiness"
  if [[ -n "$qemu_boot_complete" && "$shell_readiness" == "n0-emu24-readiness-$boot_attempt" ]]; then
    ready=1
    break
  fi
  sleep 1
done
if (( ready != 1 )); then
  printf 'READINESS_VERDICT=blocked\n'
  exit 1
fi
boot_completed="$(hdc shell 'param get boot.completed' 2>&1 | tr -d '\r' || true)"
wms_ready="$(hdc shell 'param get bootevent.wms.ready' 2>&1 | tr -d '\r' || true)"
lockscreen_ready="$(hdc shell 'param get bootevent.lockscreen.ready' 2>&1 | tr -d '\r' || true)"
printf 'READINESS boot.completed=%q wms.ready=%q lockscreen.ready=%q qemu.boot=%q\n' \
  "$boot_completed" "$wms_ready" "$lockscreen_ready" "$qemu_boot_complete"
printf 'READINESS_VERDICT=pass\n'

preexisting_bundle="$(hdc shell "bm dump -n $BUNDLE" 2>&1 || true)"
printf 'PREINSTALL_BUNDLE_QUERY=%q\n' "$preexisting_bundle"
if [[ "$preexisting_bundle" != *'failed to get information'* ]]; then
  fail "bundle existed before N0 installation"
fi

# --- 8. install app + test HAPs ---------------------------------------------
hdc shell "rm -rf '$STAGING'" >/dev/null 2>&1 || fail "guest staging rm failed"
hdc shell "mkdir -p '$STAGING'" >/dev/null 2>&1 || fail "guest staging mkdir failed"
timeout 180 "$HDC" -t "$EMULATOR_TARGET" file send "$app_hap" "$STAGING/entry-default-unsigned.hap" </dev/null || fail "app HAP file send failed"
timeout 180 "$HDC" -t "$EMULATOR_TARGET" file send "$test_hap" "$STAGING/entry-ohosTest-unsigned.hap" </dev/null || fail "test HAP file send failed"
install_output=''
for install_attempt in $(seq 1 30); do
  install_output="$(timeout 180 "$HDC" -t "$EMULATOR_TARGET" shell "bm install -p '$STAGING'" 2>&1 </dev/null || true)"
  printf 'INSTALL_ATTEMPT=%s OUTPUT=%q\n' "$install_attempt" "$install_output"
  if [[ "$install_output" == *"install bundle successfully"* ]]; then
    installed=1
    break
  fi
  sleep 2
done
if (( installed != 1 )); then
  fail "dual-HAP installation failed"
fi
printf 'INSTALL_VERDICT=pass\n'

bm_dump="$(hdc shell "bm dump -n $BUNDLE" | sed -E \
  -e 's/("accessTokenId": )[0-9]+/\1[RUNTIME_ACCESS_TOKEN_ID_REDACTED]/g' \
  -e 's/("accessTokenIdEx": )[0-9]+/\1[RUNTIME_ACCESS_TOKEN_ID_EX_REDACTED]/g')"
printf '%s\n' "$bm_dump"
if [[ "$bm_dump" != *'"appPrivilegeLevel": "normal"'* ||
      "$bm_dump" != *'"isSystemApp": false'* ||
      "$bm_dump" != *'"mainElementName": "EntryAbility"'* ]]; then
  fail "installed package is not an ordinary EntryAbility application"
fi
printf 'NORMAL_ENTRY_ABILITY_AUDIT=pass\n'

# --- 9. clear hilog + aa test ------------------------------------------------
hilog_buffer_output="$(hdc shell 'hilog -G 16M' 2>&1 | tr -d '\r' || true)"
hilog_buffer_query="$(hdc shell 'hilog -g' 2>&1 | tr -d '\r' || true)"
printf 'HILOG_BUFFER_SET=%q\n' "$hilog_buffer_output"
printf 'HILOG_BUFFER_QUERY=%q\n' "$hilog_buffer_query"
if [[ "$hilog_buffer_query" != *"16.0M"* && "$hilog_buffer_query" != *"16M"* &&
      "$hilog_buffer_query" != *"16777216"* ]]; then
  fail "HiLog buffer did not report the required 16 MiB capacity"
fi
printf 'HILOG_BUFFER_VERDICT=pass\n'
hdc shell 'hilog -r' >/dev/null 2>&1 || fail "hilog clear failed"
# aa RC is NOT a gate here: a static-import loader rejection makes the test
# runner crash (aa RC != 0, no marker). The aa RC check is deferred until
# after the marker/loader-rejection classification in the judgment section.
set +e
aa_output="$(timeout 60 "$HDC" -t "$EMULATOR_TARGET" shell "aa test -b $BUNDLE -m $TEST_MODULE -s unittest $TEST_RUNNER -s timeout 15000" 2>&1 </dev/null)"
aa_rc=$?
set -e
printf 'AA_TEST_RC=%s\n' "$aa_rc"
printf 'AA_TEST_OUTPUT=%q\n' "$aa_output"
printf '%s\n' "$aa_output" >"$AA_TEST_LOG"

# --- 10. directed HiLog capture ----------------------------------------------
: >"$TAG_HILOG"
{
  printf '===== TAG HILOG N0NativeCoreTest =====\n'
  hdc shell 'hilog -x -T N0NativeCoreTest -v year -v zone' 2>&1 || true
  printf '===== TAG HILOG N0NativeCore =====\n'
  hdc shell 'hilog -x -T N0NativeCore -v year -v zone' 2>&1 || true
} >>"$TAG_HILOG"
hdc shell 'hilog -x -t app -v year -v zone' >"$APP_HILOG" 2>&1 || true
printf 'TAG_HILOG_LINES=%s\n' "$(wc -l <"$TAG_HILOG")"
printf 'APP_HILOG_LINES=%s\n' "$(wc -l <"$APP_HILOG")"
run sha256sum "$TAG_HILOG" "$APP_HILOG"

# --- 11. judgment ------------------------------------------------------------
# Collect every N0_CORE_PROBE_RESULT line from all three sources, extract the
# marker content, dedupe identical lines (the same marker legitimately appears
# in the tag HiLog, the aa-test printSync output and the app HiLog) and
# require exactly one distinct marker.
mapfile -t distinct_markers < <(collect_distinct_markers "$TAG_HILOG" "$AA_TEST_LOG" "$APP_HILOG")
printf 'MARKER_DISTINCT_COUNT=%s\n' "${#distinct_markers[@]}"

# NAPI HiLog cross-check (N0NativeCore tag): the overlay logs N0_RUNPROBE with
# the native smoke fields; a PASS marker must agree with it.
n0_runprobe_line="$(grep -F 'N0_RUNPROBE|' "$TAG_HILOG" | tail -1 | sed -n 's/.*\(N0_RUNPROBE.*\)/\1/p' | sed 's/\r$//' || true)"
printf 'N0_RUNPROBE_LINE=%s\n' "$n0_runprobe_line"
smoke_ok="$(napi_field "$n0_runprobe_line" smokeOk)"
x25519_ok="$(napi_field "$n0_runprobe_line" x25519Ok)"
tunnel_ok="$(napi_field "$n0_runprobe_line" tunnelOk)"
printf 'NAPI_SMOKE_OK=%s NAPI_X25519_OK=%s NAPI_TUNNEL_OK=%s\n' "$smoke_ok" "$x25519_ok" "$tunnel_ok"

guest_code="$(extract_guest_code "$AA_TEST_LOG")"
printf 'GUEST_RESULT_CODE=%s\n' "$guest_code"

# Classification order: marker present? -> judge it; no marker -> precise
# loader rejection? -> measured blocked, else runner fail. The aa RC check is
# applied AFTER this classification: a static-import loader rejection
# legitimately makes aa RC != 0 with no marker, while a marker verdict
# requires host aa RC=0 (the aa test executed).
if (( ${#distinct_markers[@]} == 0 )); then
  # No marker at all: only a precise loader rejection pointing at libentry.so
  # (with HAP member identity already passed above) is a measured blocked;
  # missing library / member / not-found / symbol-wrapping errors / no
  # precise line are all runner failures.
  if is_loader_rejection "$AA_TEST_LOG" || is_loader_rejection "$TAG_HILOG" || is_loader_rejection "$APP_HILOG"; then
    result="blocked"
    printf 'MEASURED_VERDICT=blocked\n'
    printf 'MEASURED_VERDICT_RULE=no marker; HAP member identity passed; precise loader rejection of libentry.so in aa/hilog logs (initial-exec TLS resolves to dynamic definition|Error relocating without symbol-missing|explicit dlopen ... libentry.so ... failed)\n'
    printf 'MEASURED_VERDICT_AA_RC=%s\n' "$aa_rc"
  else
    fail "N0_CORE_PROBE_RESULT missing from all HiLog sources (tag/aa-test/app) and no precise libentry.so loader rejection in the collected logs"
  fi
elif (( ${#distinct_markers[@]} > 1 )); then
  for d in "${distinct_markers[@]}"; do
    printf 'MARKER_DUPLICATE_LINE=%s\n' "$d"
  done
  fail "N0_CORE_PROBE_RESULT emitted more than one distinct marker"
else
  marker_line="${distinct_markers[0]}"
  printf 'N0_CORE_PROBE_RESULT_LINE=%s\n' "$marker_line"
  case "$(judge_marker "$marker_line")" in
    pass)
      if (( aa_rc != 0 )); then
        fail "marker PASS but host aa test exited non-zero (rc=$aa_rc)"
      fi
      if [[ "$smoke_ok" != "1" || "$x25519_ok" != "1" || "$tunnel_ok" != "1" ]]; then
        fail "marker PASS but NAPI HiLog smoke fields disagree: $n0_runprobe_line"
      fi
      if [[ "$guest_code" != "0" ]]; then
        fail "guest test result code is not 0: ${guest_code:-<missing>}"
      fi
      result="pass"
      printf 'MEASURED_VERDICT=pass\n'
      ;;
    blocked)
      if (( aa_rc != 0 )); then
        fail "marker verdict=FAIL blocked but host aa test exited non-zero (rc=$aa_rc)"
      fi
      result="blocked"
      printf 'MEASURED_VERDICT=blocked\n'
      printf 'MEASURED_VERDICT_RULE=marker verdict=FAIL with anchored platform-rejection detail prefix (detail=dlopen|detail=N0 runProbe smoke failed:|detail=N0 runProbe sub-check failed:|detail=n0_probe_smoke returned inconsistent status|detail=n0_probe_version returned null|detail=failed to build runProbe result)\n'
      ;;
    fail)
      fail "unexpected N0_CORE_PROBE_RESULT: $marker_line"
      ;;
  esac
fi

# --- 12. base manifest (created immediately after the verdict) ---------------
# The base manifest is written as soon as the verdict is determined so that
# ANY later fail (screenshot/cleanup/residual/sensitive) can still seal the
# manifest and transcript via the EXIT trap. Subsequent results are appended
# below. The transcript hash is intentionally NOT written here: the transcript
# is still being appended to by the runner's stdout (O_APPEND) and later by
# teardown output; the final transcript size/hash is appended by
# seal_and_finalize after the stream is closed.
ended_at="$(date --iso-8601=seconds)"
{
  printf '=== N0 Emulator evidence manifest ===\n'
  printf 'evidence_id=%s\n' "$EVIDENCE_ID"
  printf 'information_status=current-measured\n'
  printf 'record_status=collected\n'
  printf 'stage_or_gate=N0(b)\n'
  printf 'code_sha=%s\n' "$code_sha"
  printf 'upstream_sha=f65f7b347ee4e7de6d98c488d3d894cd018b02b6 (NetBird v0.76.3 fixed baseline per N0 resolution); BoringTun 0.7.1 crate sha256 recorded as boringtun_crate_sha256\n'
  printf 'snapshot_commit=%s\n' "$SNAPSHOT_HEAD"
  printf 'boringtun_version=%s\n' "$BORINGTUN_VERSION"
  printf 'boringtun_crate_sha256=%s\n' "$crate_sha"
  printf 'target_tuple=%s\n' "$TARGET_TUPLE"
  printf 'emulator_instance=%s\n' "$EMULATOR_INSTANCE"
  printf 'emulator_target=%s\n' "$EMULATOR_TARGET"
  printf 'build_toolchain=command-line-tools/6.1.1.290 (stable; hvigorw/ohpm/native SDK for build)\n'
  printf 'emulator_runtime=command-line-tools/26.0.0.461 (beta; Emulator binary + hdc)\n'
  printf 'native_sdk_version=%s\n' "$native_sdk_version"
  printf 'native_sdk_api=%s\n' "$native_sdk_api"
  printf 'native_sdk_release_type=%s\n' "$native_sdk_release"
  printf 'started_at=%s\n' "$started_at"
  printf 'ended_at=%s\n' "$ended_at"
  printf 'clock_source=host_CLOCK_REALTIME_date_iso_8601_seconds\n'
  printf 'timezone=%s\n' "$(date +%Z%:z)"
  printf 'working_directory=%s\n' "$WORKSPACE"
  printf 'command=bash spikes/n0-native-core/n0-emulator-run.sh\n'
  printf 'verdict=%s\n' "$result"
  printf 'e8_status=CLOSED\n'
  printf 'physical_device_used=false\n'
  printf 'libentry_x86_64_sha256=%s\n' "$built_sha"
  printf 'libentry_aarch64_sha256=%s\n' "$aarch64_sha"
  printf 'app_hap_sha256=%s\n' "$(sha256sum "$app_hap" | awk '{print $1}')"
  printf 'test_hap_sha256=%s\n' "$(sha256sum "$test_hap" | awk '{print $1}')"
  printf 'app_member_sha256=%s\n' "$app_member_sha"
  printf 'test_member_sha256=%s\n' "$test_member_sha"
  printf 'cargo_lock_sha256=%s\n' "$(sha256sum "$PROJECT_DIR/Cargo.lock" | awk '{print $1}')"
  printf 'source_manifest_sha256=%s\n' "$(sha256sum "$SOURCE_MANIFEST" | awk '{print $1}')"
  printf 'n0_build_log_sha256=%s\n' "$(sha256sum "$N0_BUILD_LOG" | awk '{print $1}')"
  printf 'snapshot_log_sha256=%s\n' "$(sha256sum "$SNAPSHOT_LOG" | awk '{print $1}')"
  printf 'build_log_sha256=%s\n' "$(sha256sum "$BUILD_LOG" | awk '{print $1}')"
  printf 'aa_test_log_sha256=%s\n' "$(sha256sum "$AA_TEST_LOG" | awk '{print $1}')"
  printf 'tag_hilog_sha256=%s\n' "$(sha256sum "$TAG_HILOG" | awk '{print $1}')"
  printf 'app_hilog_sha256=%s\n' "$(sha256sum "$APP_HILOG" | awk '{print $1}')"
  printf 'console_sha256=%s\n' "$(sha256sum "$CONSOLE" | awk '{print $1}')"
  printf 'reviewer=pending-independent-review\n'
  printf 'manifest_self_hash_semantics=manifest_sha256 is the sha256 of this file up to and including the transcript_final_sha256 line; the manifest_sha256 line itself is appended after hashing and is not part of its own hash; transcript_final_sha256 is the sha256 of the first transcript_final_bytes bytes of the transcript (recomputed with head -c transcript_final_bytes)\n'
} >"$MANIFEST"
printf 'ENDED_AT=%s\n' "$ended_at"
printf 'RECORD_STATUS=collected\n'
printf 'VERDICT=%s\n' "$result"

# --- 13. screenshot (best-effort; never a gate) ------------------------------
screen_output="$(hdc shell 'uitest screenCap' 2>&1 | tr -d '\r' || true)"
printf 'SCREEN_OUTPUT=%q\n' "$screen_output"
remote_screen="$(printf '%s\n' "$screen_output" | sed -n 's/^ScreenCap saved to //p' | tail -1)"
if [[ -n "$remote_screen" ]]; then
  if timeout 30 "$HDC" -t "$EMULATOR_TARGET" file recv "$remote_screen" "$SCREENSHOT" </dev/null && [[ -s "$SCREENSHOT" ]]; then
    hdc shell "rm -f '$remote_screen'" >/dev/null 2>&1 || true
    run file "$SCREENSHOT"
    run sha256sum "$SCREENSHOT"
    printf 'SCREENSHOT=collected\n'
    printf 'screenshot_sha256=%s\n' "$(sha256sum "$SCREENSHOT" | awk '{print $1}')" >>"$MANIFEST"
  else
    rm -f "$SCREENSHOT"
    printf 'SCREENSHOT=skipped-capture-failed\n'
    printf 'screenshot=skipped-capture-failed\n' >>"$MANIFEST"
  fi
else
  printf 'SCREENSHOT=skipped-no-capture\n'
  printf 'screenshot=skipped-no-capture\n' >>"$MANIFEST"
fi

# --- 14. cleanup -------------------------------------------------------------
hdc shell "aa force-stop $BUNDLE" >/dev/null 2>&1 || true
hdc shell "rm -rf '$STAGING'" >/dev/null 2>&1 || true
if (( installed == 1 )); then
  timeout 120 "$HDC" -t "$EMULATOR_TARGET" uninstall "$BUNDLE" >/dev/null 2>&1 </dev/null || true
  installed=0
fi
if (( emulator_started == 1 )); then
  HDC_PORT="$EMULATOR_HDC_PORT" timeout 60 "$STOP_HELPER" >/dev/null 2>&1 </dev/null || true
  for _ in $(seq 1 30); do
    if ! pgrep -f '/emulator/Emulator.*-start '"$EMULATOR_INSTANCE"'|qemu-system.*'"$EMULATOR_INSTANCE" >/dev/null; then
      break
    fi
    sleep 1
  done
  emulator_started=0
fi
"$HDC" kill >/dev/null 2>&1 </dev/null || true
residual_cleared=0
for cleanup_attempt in $(seq 1 10); do
  # Emulator processes are matched by their fixed command line; the hdc
  # daemon is matched by its exact process name (pgrep -ax hdc), never by a
  # loose pattern.
  residual_emulator="$(pgrep -af '/emulator/Emulator.*-start '"$EMULATOR_INSTANCE"'|qemu-system.*'"$EMULATOR_INSTANCE" || true)"
  residual_hdc="$(pgrep -ax hdc || true)"
  residual_ports="$(ss -ltnp | grep -E ":($EMULATOR_HDC_PORT|5555|8710)[[:space:]]" || true)"
  printf 'FINAL_CLEANUP_ATTEMPT=%s emulator=%q hdc=%q ports=%q\n' \
    "$cleanup_attempt" "$residual_emulator" "$residual_hdc" "$residual_ports"
  if [[ -z "$residual_emulator" && -z "$residual_hdc" && -z "$residual_ports" ]]; then
    residual_cleared=1
    break
  fi
  "$HDC" kill >/dev/null 2>&1 </dev/null || true
  sleep 1
done
if (( residual_cleared != 1 )); then
  fail "residual Emulator, HDC, or tested port after cleanup"
fi
printf 'FINAL_RESIDUAL_PROCESS=false\n'
printf 'FINAL_RESIDUAL_PORT=false\n'
printf 'final_residual_process=false\n' >>"$MANIFEST"
printf 'final_residual_port=false\n' >>"$MANIFEST"

# --- 15. sensitive material scan (fail-closed) --------------------------------
# rg exit codes: 0 = match found, 1 = no match, 2 = error. ONLY rc=1 (clean
# scan) passes; every other rc fails the run.
set +e
sensitive_output="$(rg -n -i -e 'authorization:[[:space:]]*bearer[[:space:]]+[A-Za-z0-9._~-]+' \
    -e '-----BEGIN ([A-Z ]+ )?PRIVATE KEY-----' -e 'setup[_ -]?key[=:][[:space:]]*[A-Za-z0-9._~-]{8,}' \
    "$TRANSCRIPT" "$TAG_HILOG" "$APP_HILOG" "$AA_TEST_LOG" "$BUILD_LOG" \
    "$N0_BUILD_LOG" "$SNAPSHOT_LOG" "$CONSOLE" "$SOURCE_MANIFEST" 2>&1)"
sensitive_rc=$?
set -e
if (( sensitive_rc != 1 )); then
  if (( sensitive_rc == 0 )); then
    printf '%s\n' "$sensitive_output"
    fail "high-confidence sensitive material pattern detected"
  fi
  fail "rg sensitive material scan failed (rc=$sensitive_rc)"
fi
printf 'SENSITIVE_SCAN=pass_high_confidence_patterns\n'
printf 'sensitive_scan=pass_high_confidence_patterns\n' >>"$MANIFEST"
