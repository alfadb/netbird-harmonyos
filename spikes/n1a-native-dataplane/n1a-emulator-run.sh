#!/usr/bin/env bash
set -Eeuo pipefail

# ============================================================================
# n1a-emulator-run.sh
#
# N1a formal API 24 x86_64 Emulator evidence runner for the native WireGuard
# data-plane pump probe (BoringTun 0.7.1 ffi-bindings, dual reciprocal
# tunnels, loopback UDP channel; spikes/n1a-native-dataplane).
#
# Frozen criteria (docs/n1a-gate-plan.md, criteria-frozen-r2): C1-C9 and the
# aggregation rule are implemented literally and CANNOT be amended after
# measurement starts. Structural mirror of the N0 runner
# (spikes/n0-native-core/n0-emulator-run.sh): same fixed Emulator
# instance/HDC target, same guards, same staging/build/install/aa-test flow,
# same teardown/seal semantics. N1a differences are confined to:
#   - the marker set (N1A_RESULT four-field frozen form + the ohosTest
#     N1A_PROBE_TEST_RESULT cross-check marker);
#   - the frozen judgment table (verdict/c5/throughput floor);
#   - the C9 result-page phase (phase B cold start, see below);
#   - the terminal-state semantics (pass | blocked | fail are all sealed
#     measured/infra states; runner defects exit non-zero).
#
# Scope (docs/n1a-gate-plan.md):
#   - fixed Emulator instance netbird_api24_phone, HDC 127.0.0.1:10000 only;
#   - NO physical device, NO public endpoint, NO VPN/TUN/protect,
#     NO management/signal/relay/ICE surface;
#   - arm64 is cross-compile only; the HAP packages x86_64 only.
#
# Usage:
#   bash spikes/n1a-native-dataplane/n1a-emulator-run.sh             # formal
#   bash spikes/n1a-native-dataplane/n1a-emulator-run.sh --selftest  # host
#   bash spikes/n1a-native-dataplane/n1a-emulator-run.sh --dry-run   # host
#                                                            # pipeline
#                                                            # rehearsal: no
#                                                            # device action,
#                                                            # no evidence
#
# Two-phase formal flow (C9 page clause):
#   Phase A (machine judgment, n0-mirror aa test): install app+test HAPs,
#     `aa test` runs runN1aProbeTest() -> the probe executes once; the NAPI
#     overlay emits the single-line N1A_RESULT marker (frozen four-field
#     form) and the ohosTest runner emits N1A_PROBE_TEST_RESULT after its
#     own fail-closed assertions. Judgment: exactly one distinct N1A_RESULT
#     in the phase-A capture (tag hilog / aa-test log / app hilog), frozen
#     field checks, ohosTest consistency, host aa RC and guest code.
#   Phase B (C9 visible result page, only after phase A judged pass): clear
#     HiLog, cold-start the ordinary EntryAbility; the staged pages/Index
#     runs the probe once and renders PASS/FAIL from the same probe result
#     object that emitted the phase-B N1A_RESULT marker (page/marker
#     consistency by construction, E2 precedent). The runner captures the
#     phase-B window separately (hilog was cleared), requires exactly one
#     distinct N1A_RESULT there with verdict=PASS, and gates on a
#     non-empty, non-black (ffmpeg YAVG > 32.0) screenshot.
#   Marker-window scoping note (interpretation, reported to the main
#   session): the frozen "exactly one marker / duplicate marker = fail"
#   rule is evaluated PER capture window (phase A for the judgment, phase B
#   for the page clause). Two probe executions exist in the campaign (one
#   per phase); both must report verdict=PASS and a c5 in the allowed set,
#   and both markers are recorded in the manifest. The frozen criteria text
#   does not define multi-phase windows; this scoping is the runner's
#   documented interpretation and is flagged in the run header.
#
# Verdict dual-axis (machine-parseable; frozen aggregation):
#   RECORD_STATUS=collected + VERDICT=pass: phase A judged pass (exactly one
#     N1A_RESULT, verdict=PASS, c5 in {induced, not-triggered},
#     throughput_mibps >= 5.00, ohosTest marker verdict=PASS, host aa RC=0,
#     guest TestFinished-ResultCode=0) AND phase B page clause pass
#     (exactly one N1A_RESULT in the phase-B window with verdict=PASS and
#     c5 in {induced, not-triggered}, non-black screenshot, page text agrees
#     by construction).
#   VERDICT=blocked: environment-class only (frozen list): Emulator start
#     failure, HDC connectivity/readiness degradation (bounded short-loop
#     non-recovery, the E1-recorded 25-minute degradation mode), build input
#     drift (BoringTun crate/Cargo.lock checksum, --offline --locked,
#     SDK/tool failure). Measured criteria violations are NEVER blocked.
#   VERDICT=fail: measured criteria violation (marker verdict=FAIL, c5=fail,
#     throughput below floor, malformed/duplicate/missing marker, ohosTest
#     inconsistency, guest code != 0 on a PASS marker, phase-B page clause
#     violation).
#   Runner defects (precondition failures, no-clobber refusals, missing
#     evidence inputs) exit non-zero WITHOUT a measured verdict and are
#   never recorded as pass/blocked/fail evidence.
#
# No private key material and no key value is ever printed.
# ============================================================================

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_DIR="$SCRIPT_DIR"
readonly DEFAULT_WORKSPACE="$(cd -- "$PROJECT_DIR/../.." && pwd -P)"

# --- fixed baseline constants ----------------------------------------------
readonly DEFAULT_EVIDENCE_ID="EV-N1A-EMU24-20260830-0001"
readonly SNAPSHOT_HEAD="2c567dc721c6582f93a15b241e843e3bbff3f7f3"
readonly BORINGTUN_VERSION="0.7.1"
readonly BORINGTUN_CRATE_SHA256="15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939"
readonly EXPECTED_VERSION_PREFIX="n1a-native-dataplane/"
readonly MARKER_PREFIX="N1A_RESULT"
readonly TEST_MARKER_PREFIX="N1A_PROBE_TEST_RESULT"
readonly APP_TAG="N1aProbe"
readonly TEST_TAG="N1aProbeTest"
readonly THROUGHPUT_FLOOR_MIBPS="5.00"
readonly TARGET_TUPLE="HarmonyOS_6.1.1(24),API24,x86_64,phone_Emulator"

# Capture any external device-target request BEFORE the fixed constants are
# assigned (mirror of the N0 guard rationale).
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

# --- toolchain paths (two distinct recorded inputs; never mixed) -------------
readonly BUILD_TOOLS="/home/worker/harmonyos/command-line-tools/6.1.1.290"
readonly EMULATOR_TOOLS="/home/worker/harmonyos/command-line-tools/26.0.0.461"
readonly HVIGOR="$BUILD_TOOLS/bin/hvigorw"
readonly OHPM="$BUILD_TOOLS/bin/ohpm"
readonly NATIVE_HOME="$BUILD_TOOLS/sdk/default/openharmony/native"
readonly NAPI_HEADER="$NATIVE_HOME/sysroot/usr/include/napi/native_api.h"
readonly HDC="$EMULATOR_TOOLS/sdk/default/openharmony/toolchains/hdc"
readonly EMULATOR="$EMULATOR_TOOLS/emulator/Emulator"

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
readonly ABILITY="EntryAbility"
readonly MODULE="entry"
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

result="defect"
fail_reason=""
blocked_reason=""
sealed=0
emulator_started=0
installed=0
device_phase_started=0
app_started=0
snapshot_dir=""
app_member=""
test_member=""
started_at=""
ended_at=""
code_sha=""
git_branch=""
git_clean=0
runner_in_head=0
built_sha=""
aarch64_sha=""
crate_sha=""
app_member_sha=""
test_member_sha=""
DRY_TMP=""

TRANSCRIPT=""
TAG_HILOG=""
APP_HILOG=""
PAGE_HILOG=""
MANIFEST=""
CONSOLE=""
BUILD_LOG=""
N1A_BUILD_LOG=""
SNAPSHOT_LOG=""
AA_TEST_LOG=""
AA_START_LOG=""
SOURCE_MANIFEST=""
SCREENSHOT_A=""
SCREENSHOT_B=""
PROBE_DETAIL=""
BFREEZE_LOG=""
OVERLAY_DIAG=""

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
  # stdin from /dev/null (N0 rationale: the hdc daemon must never inherit the
  # runner's stdin/stdout-transcript).
  timeout 30 "$HDC" -t "$EMULATOR_TARGET" "$@" </dev/null
}

# Runner defect: never a measured verdict; exits non-zero. Before evidence
# setup (preconditions) no evidence file exists; after it, the EXIT trap
# seals whatever was recorded with run_status=defect.
defect() {
  result="defect"
  fail_reason="$1"
  printf 'DEFECT_REASON=%s\n' "$1"
  exit 1
}

# Environment-class terminal state (frozen blocked list): sealed evidence,
# exit 0.
blocked_env() {
  result="blocked"
  blocked_reason="$1"
  printf 'BLOCKED_REASON=%s\n' "$1"
  exit 0
}

# Measured criteria violation: sealed evidence, exit 0 (a valid measured
# terminal state per the frozen aggregation).
measured_fail() {
  result="fail"
  fail_reason="$1"
  printf 'FAIL_REASON=%s\n' "$1"
  exit 0
}

# Reliable EXIT teardown (N0 mirror; see the N0 header for the full
# rationale): staging rm while the Emulator is online -> uninstall -> stop
# Emulator -> kill host hdc (device phase only) -> residual state record ->
# temp cleanup -> seal.
teardown() {
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
      printf 'DRY_RUN_VERDICT=defect\n'
    fi
    set -e
    printf 'TRAP_EXIT_CODE=%s RESULT=%s\n' "$exit_code" "$result"
    exit "$exit_code"
  fi
  if (( app_started == 1 )); then
    hdc shell "aa force-stop $BUNDLE" >/dev/null 2>&1 </dev/null || true
    app_started=0
    printf 'CLEANUP_APP_STOP=done\n'
  fi
  if (( emulator_started == 1 )); then
    hdc shell "rm -rf '$STAGING'" >/dev/null 2>&1 </dev/null || true
    printf 'CLEANUP_STAGING=cleared\n'
  else
    printf 'CLEANUP_STAGING=skipped-emulator-not-started\n'
  fi
  if (( installed == 1 )); then
    timeout 120 "$HDC" -t "$EMULATOR_TARGET" uninstall "$BUNDLE" >/dev/null 2>&1 </dev/null || true
    installed=0
    printf 'CLEANUP_UNINSTALL=done\n'
  fi
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
  if (( device_phase_started == 1 )); then
    "$HDC" kill >/dev/null 2>&1 </dev/null || true
    printf 'CLEANUP_HDC=kill-issued\n'
  else
    printf 'CLEANUP_HDC=skipped-no-device-phase\n'
  fi
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
  # Defect-1 invariant (EV-N1A-EMU24-20260830-0001): raw and manifest must
  # exist together or not at all. A runner defect that strikes BEFORE the
  # base manifest is created (pre-measurement: build/snapshot/verify
  # failures) is not a measurement - the partial raw files are removed.
  if (( SELFTEST == 0 && DRY_RUN == 0 )) && [[ "$result" == "defect" ]] \
      && { [[ -z "$MANIFEST" ]] || [[ ! -f "$MANIFEST" ]]; }; then
    local f
    for f in "$TRANSCRIPT" "$BUILD_LOG" "$N1A_BUILD_LOG" "$SNAPSHOT_LOG" \
             "$SOURCE_MANIFEST" "$CONSOLE"; do
      [[ -n "$f" && -f "$f" ]] && rm -f "$f"
    done
    printf 'CLEANUP_UNMEASURED_RAW=removed\n'
  fi
  printf 'CLEANUP_TEMP=removed\n'
  printf 'CLEANUP_END=teardown-complete\n'
  printf 'TRAP_EXIT_CODE=%s RESULT=%s\n' "$exit_code" "$result"
  seal_and_finalize "$exit_code"
  set -e
  exit "$exit_code"
}
trap teardown EXIT

# Base manifest: all run-identity facts that exist BEFORE any measurement
# (defect 1 of EV-N1A-EMU24-20260830-0001: measured_fail short-circuited
# before the old post-judgment manifest block, leaving fail/blocked terminal
# states with raw but no manifest/seal). Written once, immediately before
# the device phase starts (all preconditions passed, builds verified); every
# later phase APPENDS. seal_and_finalize then seals fail/blocked/pass alike.
write_base_manifest() {
  {
    printf '=== N1a Emulator evidence manifest ===\n'
    printf 'evidence_id=%s\n' "$EVIDENCE_ID"
    printf 'information_status=current-measured\n'
    printf 'record_status=collected\n'
    printf 'stage_or_gate=N1a\n'
    printf 'campaign_id=N1A-EMU24-20260830-0001\n'
    printf 'attempt=initial\n'
    printf 'criteria_revision=frozen-r3 (docs/n1a-gate-plan.md)\n'
    printf 'code_sha=%s\n' "$code_sha"
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
    printf 'clock_source=host_CLOCK_REALTIME_date_iso_8601_seconds\n'
    printf 'timezone=%s\n' "$(date +%Z%:z)"
    printf 'working_directory=%s\n' "$WORKSPACE"
    printf 'command=bash spikes/n1a-native-dataplane/n1a-emulator-run.sh\n'
    printf 'throughput_floor_mibps=%s\n' "$THROUGHPUT_FLOOR_MIBPS"
    printf 'e8_status=CLOSED\n'
    printf 'physical_device_used=false\n'
    printf 'libentry_x86_64_sha256=%s\n' "$built_sha"
    printf 'libentry_aarch64_sha256=%s\n' "$aarch64_sha"
    printf 'cargo_lock_sha256=%s\n' "$(sha256sum "$PROJECT_DIR/Cargo.lock" | awk '{print $1}')"
    printf 'two_phase_flow=phase-A aa-test judgment + phase-B C9 result page; per-window duplicate rule; C1 two-window binding; B second full probe; c5/throughput dual-recorded (criteria-holder ruling 2026-08-30)\n'
    printf 'reviewer=pending-independent-review\n'
    printf 'manifest_self_hash_semantics=manifest_sha256 is the sha256 of this file up to and including the transcript_final_sha256 line; the manifest_sha256 line itself is appended after hashing and is not part of its own hash; transcript_final_sha256 is the sha256 of the first transcript_final_bytes bytes of the transcript (recomputed with head -c transcript_final_bytes)\n'
  } >>"$MANIFEST"
}

# Close the transcript log stream: switch stdout/stderr to /dev/null so
# nothing after this point can append (O_APPEND; no tee, no fd 3; the seal
# never waits on a child holding the stream open).
seal_transcript() {
  exec >/dev/null 2>&1
}

seal_and_finalize() {
  # Idempotent: the trap teardown and the explicit call sites may both fire;
  # a second call must never append a second final block.
  if (( sealed == 1 )); then
    return 0
  fi
  sealed=1
  local exit_code="${1:-0}"
  seal_transcript
  if [[ -n "$MANIFEST" && -f "$MANIFEST" ]]; then
    local transcript_bytes transcript_hash manifest_hash
    printf 'final_exit_code=%s\n' "$exit_code" >>"$MANIFEST"
    printf 'run_status=%s\n' "$result" >>"$MANIFEST"
    printf 'fail_reason=%s\n' "${fail_reason:-}" >>"$MANIFEST"
    printf 'blocked_reason=%s\n' "${blocked_reason:-}" >>"$MANIFEST"
    printf 'console_sha256=%s\n' "$(sha256sum "$CONSOLE" 2>/dev/null | awk '{print $1}')" >>"$MANIFEST"
    printf 'aa_start_log_sha256=%s\n' "$(sha256sum "$AA_START_LOG" 2>/dev/null | awk '{print $1}')" >>"$MANIFEST"
    transcript_bytes="$(stat -c %s "$TRANSCRIPT" 2>/dev/null || true)"
    printf 'transcript_final_bytes=%s\n' "$transcript_bytes" >>"$MANIFEST"
    transcript_hash="$(head -c "${transcript_bytes:-0}" "$TRANSCRIPT" 2>/dev/null | sha256sum | awk '{print $1}')" || true
    printf 'transcript_final_sha256=%s\n' "$transcript_hash" >>"$MANIFEST"
    manifest_hash="$(sha256sum "$MANIFEST" | awk '{print $1}')" || true
    printf 'manifest_sha256=%s\n' "$manifest_hash" >>"$MANIFEST"
  fi
}

# Refuse to overwrite existing fixed evidence (no-clobber; runs before any
# raw file is created and before any device action).
check_no_clobber() {
  local f
  for f in "$TRANSCRIPT" "$TAG_HILOG" "$APP_HILOG" "$PAGE_HILOG" "$MANIFEST" \
    "$CONSOLE" "$BUILD_LOG" "$N1A_BUILD_LOG" "$SNAPSHOT_LOG" "$AA_TEST_LOG" \
    "$AA_START_LOG" "$SOURCE_MANIFEST" "$PROBE_DETAIL" "$OVERLAY_DIAG" "$BFREEZE_LOG" "$SCREENSHOT_A" "$SCREENSHOT_B"; do
    if [[ -n "$f" && -e "$f" ]]; then
      printf 'REFUSE_OVERWRITE=evidence file already exists: %s; refusing to overwrite fixed evidence (use a fresh EVIDENCE_ID or EVIDENCE_ROOT)\n' "$f" >&2
      return 1
    fi
  done
  return 0
}

# --- forbidden target guards (N0 mirror) --------------------------------------
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
# Extract the N1A_RESULT marker content from a raw line. Handles the hilog
# prefix ("...: N1A_RESULT|...") and the aa-test printSync /
# TestFinished-ResultMsg line (marker terminated by ';'). The marker fields
# never contain ';' or '|' by construction (four frozen fields, sanitized
# values).
extract_app_marker() {
  local line="$1"
  printf '%s' "$line" | sed -n "s/.*\(${MARKER_PREFIX}[^;]*\).*/\1/p" | sed 's/\r$//'
}

# Extract the ohosTest cross-check marker (N1A_PROBE_TEST_RESULT|...).
extract_test_marker() {
  local line="$1"
  printf '%s' "$line" | sed -n "s/.*\(${TEST_MARKER_PREFIX}[^;]*\).*/\1/p" | sed 's/\r$//'
}

# Collect every marker line of the given prefix from the given files, extract
# the marker content, dedupe identical markers (the same marker legitimately
# appears in the tag HiLog, the aa-test printSync output and the app HiLog)
# and print the distinct markers one per line. Prints nothing when no marker
# exists. (N0 collect_distinct_markers semantics, parameterized by prefix.)
collect_distinct_markers() {
  local prefix="$1"
  shift
  local f line extracted seen d
  local -a marker_lines=()
  local -a distinct_markers=()
  for f in "$@"; do
    if [[ -n "$f" && -f "$f" ]]; then
      while IFS= read -r line; do
        marker_lines+=("$line")
      done < <(grep -F "$prefix" "$f" || true)
    fi
  done
  for line in "${marker_lines[@]}"; do
    if [[ "$prefix" == "$MARKER_PREFIX" ]]; then
      extracted="$(extract_app_marker "$line")"
    else
      extracted="$(extract_test_marker "$line")"
    fi
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
  if (( ${#distinct_markers[@]} == 0 )); then
    return 0
  fi
  printf '%s\n' "${distinct_markers[@]}"
}

# Extract a pipe-separated marker field (k=v).
marker_field() {
  printf '%s' "$1" | awk -F'|' -v name="$2" \
    '{ for (i = 1; i <= NF; i++) if ($i ~ ("^" name "=")) { sub("^" name "=", "", $i); print $i } }'
}

# Defect 2 (EV-N1A-EMU24-20260830-0001): reassemble the chunked probe
# detail JSON. The ohosTest entry emits
#   N1A_JSON|part=<i>|total=<n>|sha256=<first16-of-whole-json-sha256>|data=<chunk>
# lines (384-byte chunks; the 0001 evidence measured the hilog message
# truncation at ~488 bytes). This function collects the lines from the
# phase-A HiLog/aa-test capture files, deduplicates identical lines, validates
# the part/total/sha contract, reassembles byte-exactly and verifies the
# digest. Prints the full 64-hex sha on stdout on success; returns 1 on any
# transport failure (caller treats it as a runner DEFECT, not a measured
# fail). The JSON document itself is unchanged - transport only.
N1A_JSON_CHUNK_MAX=320
reassemble_probe_json() {
  local out_path="$1"; shift
  local f line m part total sha16 data
  local declared_total="" declared_sha=""
  local -A seen_line=() seen_part=() part_data=()
  local -a parts=()
  for f in "$@"; do
    [[ -n "$f" && -f "$f" ]] || continue
    while IFS= read -r line; do
      [[ -n "$line" ]] || continue
      # Collapse identical re-emitted lines across capture sources.
      [[ -n "${seen_line[$line]:-}" ]] && continue
      seen_line["$line"]=1
      if [[ "$line" =~ N1A_JSON\|part=([0-9]+)\|total=([0-9]+)\|sha256=([0-9a-f]{16})\|data=(.*)$ ]]; then
        part="${BASH_REMATCH[1]}"
        total="${BASH_REMATCH[2]}"
        sha16="${BASH_REMATCH[3]}"
        data="${BASH_REMATCH[4]}"
        if [[ -z "$declared_total" ]]; then
          declared_total="$total"
          declared_sha="$sha16"
        elif [[ "$total" != "$declared_total" || "$sha16" != "$declared_sha" ]]; then
          return 1
        fi
        if [[ -n "${seen_part[$part]:-}" ]]; then
          return 1
        fi
        seen_part["$part"]=1
        part_data["$part"]="$data"
        parts+=("$part")
      else
        # A line that greps as a chunk but does not parse is corruption.
        return 1
      fi
    done < <(grep -F 'N1A_JSON|part=' "$f" || true)
  done
  [[ -n "$declared_total" ]] || return 1
  (( declared_total > 0 )) || return 1
  (( ${#parts[@]} == declared_total )) || return 1
  local i
  for ((i = 0; i < declared_total; i++)); do
    [[ -n "${part_data[$i]:-}" || ${#part_data[$i]} -eq 0 ]] || return 1
    (( ${#part_data[$i]} <= N1A_JSON_CHUNK_MAX )) || return 1
  done
  # Reassemble in part order and verify the whole-document digest.
  local reassembled reasm_sha
  reassembled=""
  for ((i = 0; i < declared_total; i++)); do
    reassembled+="${part_data[$i]}"
  done
  printf '%s' "$reassembled" >"$out_path"
  reasm_sha="$(sha256sum "$out_path" | awk '{print $1}')"
  [[ "${reasm_sha:0:16}" == "$declared_sha" ]] || { rm -f "$out_path"; return 1; }
  printf '%s' "$reasm_sha"
  return 0
}


# Defect #3 (EV-N1A-EMU24-20260831-0001): extract all N1A_DIAG lines and
# N1A_DIAG_JSON blocks from the hilog sources into a single overlay-diag
# artifact. Pure diagnostic channel -- never participates in the verdict.
# The extraction is best-effort: the file is always created (even if empty)
# so the manifest can bind its hash; absence of N1A_DIAG lines means the
# overlay never hit a ThrowError path (healthy) or the hilog was lost.
extract_overlay_diag() {
  local out_file="$1"; shift
  local f
  : >"$out_file"
  for f in "$@"; do
    [[ -n "$f" && -f "$f" ]] || continue
    grep -F 'N1A_DIAG' "$f" 2>/dev/null >>"$out_file" || true
    grep -F 'N1A_CATCH_FALLBACK' "$f" 2>/dev/null >>"$out_file" || true
    # 0008 ruling prerequisite #3: C5 shape short marker (diagnostic only).
    grep -F 'N1A_C5|' "$f" 2>/dev/null >>"$out_file" || true
  done
  # Deduplicate identical lines (hilog tag + app logs may duplicate).
  if [[ -s "$out_file" ]]; then
    local tmp; tmp="$(mktemp "${TMPDIR:-/tmp}/n1a-diag-dedup.XXXXXX")"
    sort -u "$out_file" >"$tmp"
    mv "$tmp" "$out_file"
  fi
  printf 'OVERLAY_DIAG_LINES=%s\n' "$(wc -l <"$out_file")" >&2
}

# Extract the guest TestFinished-ResultCode from the aa-test log (last match).
extract_guest_code() {
  local log="$1"
  grep -o 'TestFinished-ResultCode: [0-9]*' "$log" 2>/dev/null | tail -1 | awk '{print $2}' || true
}

# --- judgment: the frozen N1A marker table -----------------------------------
# Parse the frozen four-field form
#   N1A_RESULT|verdict=<PASS|FAIL>|c5=<induced|not-triggered|fail>|throughput_mibps=<x.xx>
# and classify it. Prints one of:
#   pass   verdict=PASS, c5 in {induced, not-triggered}, throughput >= floor,
#          exactly the four frozen fields.
#   fail   measured criteria violation: verdict=FAIL, c5=fail, malformed
#          field set/value, throughput below the floor.
# The c5 aggregation token vs marker value distinction is registered in the
# frozen plan header: the marker uses the C9 enumeration (induced /
# not-triggered / fail); the aggregation token pass-induced maps from
# c5=induced. not-triggered never fails alone and never counts as
# backpressure-verified.
judge_app_marker() {
  local marker_line="$1"
  local -a fields
  local verdict c5 throughput
  IFS='|' read -ra fields <<<"$marker_line"
  # Frozen C9: the field set is pinned to exactly four fields.
  if (( ${#fields[@]} != 4 )); then
    printf 'fail\n'
    return 0
  fi
  if [[ "${fields[0]}" != "$MARKER_PREFIX" ]]; then
    printf 'fail\n'
    return 0
  fi
  if [[ "${fields[1]}" != "verdict="* || "${fields[2]}" != "c5="* || \
        "${fields[3]}" != "throughput_mibps="* ]]; then
    printf 'fail\n'
    return 0
  fi
  verdict="${fields[1]#verdict=}"
  c5="${fields[2]#c5=}"
  throughput="${fields[3]#throughput_mibps=}"
  if [[ "$verdict" != "PASS" && "$verdict" != "FAIL" ]]; then
    printf 'fail\n'
    return 0
  fi
  if [[ "$c5" != "induced" && "$c5" != "not-triggered" && "$c5" != "fail" ]]; then
    printf 'fail\n'
    return 0
  fi
  if ! awk -v v="$throughput" 'BEGIN { exit !(v ~ /^[0-9]+\.[0-9][0-9]$/) }'; then
    printf 'fail\n'
    return 0
  fi
  if [[ "$verdict" == "FAIL" ]]; then
    printf 'fail\n'
    return 0
  fi
  if [[ "$c5" == "fail" ]]; then
    printf 'fail\n'
    return 0
  fi
  if ! awk -v v="$throughput" -v floor="$THROUGHPUT_FLOOR_MIBPS" \
       'BEGIN { exit !(v + 0 >= floor + 0) }'; then
    printf 'fail\n'
    return 0
  fi
  printf 'pass\n'
}

# Extract the ohosTest verdict (PASS/FAIL) from a N1A_PROBE_TEST_RESULT line.
judge_test_marker() {
  local marker_line="$1"
  local -a fields
  local verdict
  IFS='|' read -ra fields <<<"$marker_line"
  if (( ${#fields[@]} < 2 )); then
    printf 'malformed\n'
    return 0
  fi
  if [[ "${fields[0]}" != "$TEST_MARKER_PREFIX" || "${fields[1]}" != "verdict="* ]]; then
    printf 'malformed\n'
    return 0
  fi
  verdict="${fields[1]#verdict=}"
  if [[ "$verdict" == "PASS" || "$verdict" == "FAIL" ]]; then
    printf '%s\n' "$verdict"
    return 0
  fi
  printf 'malformed\n'
}

# Screenshot non-black check (E2 precedent): ffmpeg signalstats YAVG over the
# first frame must exceed 32.0 and the file must be a non-empty PNG.
screenshot_yavg() {
  local f="$1"
  ffmpeg -hide_banner -i "$f" -vf signalstats,metadata=print \
    -frames:v 1 -f null - 2>&1 | sed -n 's/.*lavfi.signalstats.YAVG=//p' | tail -1 || true
}

screenshot_nonblack() {
  local f="$1"
  local yavg
  [[ -n "$f" && -s "$f" ]] || return 1
  file "$f" | grep -q 'PNG' || return 1
  yavg="$(screenshot_yavg "$f")"
  [[ -n "$yavg" ]] || return 1
  awk -v value="$yavg" 'BEGIN { exit !(value + 0 > 32.0) }'
}

# --- selftest: pure host checks, no network/HDC/Emulator, no evidence -------
selftest_run() {
  local rc=0
  local tmpdir
  local v
  local marker_pass marker_pass_nt marker_fail_verdict marker_fail_c5 \
    marker_fail_throughput marker_malformed_fields5 marker_malformed_fields3 \
    marker_malformed_verdict marker_malformed_c5 marker_malformed_throughput \
    marker_c5_fail marker_polluted
  local test_marker_pass test_marker_fail test_marker_malformed
  local hilog_line resultmsg_line raw_line e1 e2 e3
  local src_a src_b src_c n1 n2 n0
  local guest_log guest_code
  local existing newfile
  local seal_tmp expected_manifest_hash actual_manifest_hash
  local seal_pid seal_done seal_bytes seal_hash recomputed_hash whole_hash
  local teardown_output
  local yavg_file
  tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/n1a-emu24-selftest.XXXXXX")"
  trap 'rm -rf "$tmpdir"' RETURN

  printf 'SELFTEST_BEGIN=n1a-emulator-run.sh\n'
  printf 'SELFTEST_MODE=pure-host no-network no-hdc no-emulator no-evidence\n'

  # --- frozen marker table: pass / fail classification ----------------------
  marker_pass='N1A_RESULT|verdict=PASS|c5=induced|throughput_mibps=48.71'
  marker_pass_nt='N1A_RESULT|verdict=PASS|c5=not-triggered|throughput_mibps=5.00'
  marker_fail_verdict='N1A_RESULT|verdict=FAIL|c5=induced|throughput_mibps=48.71'
  marker_fail_c5='N1A_RESULT|verdict=PASS|c5=fail|throughput_mibps=48.71'
  marker_fail_throughput='N1A_RESULT|verdict=PASS|c5=induced|throughput_mibps=4.99'
  marker_c5_fail='N1A_RESULT|verdict=FAIL|c5=fail|throughput_mibps=0.00'
  # counterexamples: field-set violations must fail (frozen C9 pins exactly
  # four fields)
  marker_malformed_fields5='N1A_RESULT|verdict=PASS|c5=induced|throughput_mibps=48.71|extra=1'
  marker_malformed_fields3='N1A_RESULT|verdict=PASS|c5=induced'
  marker_malformed_verdict='N1A_RESULT|verdict=MAYBE|c5=induced|throughput_mibps=48.71'
  marker_malformed_c5='N1A_RESULT|verdict=PASS|c5=pass-induced|throughput_mibps=48.71'
  marker_malformed_throughput='N1A_RESULT|verdict=PASS|c5=induced|throughput_mibps=fast'
  # counterexample: a verdict=PASS fragment appearing inside a trailing extra
  # field must not rescue a malformed marker
  marker_polluted='N1A_RESULT|verdict=FAIL|c5=induced|throughput_mibps=48.71|verdict=PASS'

  v="$(judge_app_marker "$marker_pass")"
  [[ "$v" == "pass" ]] || { printf 'SELFTEST FAIL judge pass: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_pass_nt")"
  [[ "$v" == "pass" ]] || { printf 'SELFTEST FAIL judge pass not-triggered at floor: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_fail_verdict")"
  [[ "$v" == "fail" ]] || { printf 'SELFTEST FAIL judge verdict=FAIL: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_fail_c5")"
  [[ "$v" == "fail" ]] || { printf 'SELFTEST FAIL judge c5=fail: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_fail_throughput")"
  [[ "$v" == "fail" ]] || { printf 'SELFTEST FAIL judge throughput below floor: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_c5_fail")"
  [[ "$v" == "fail" ]] || { printf 'SELFTEST FAIL judge c5=fail verdict=FAIL: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_malformed_fields5")"
  [[ "$v" == "fail" ]] || { printf 'SELFTEST FAIL judge 5 fields must fail: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_malformed_fields3")"
  [[ "$v" == "fail" ]] || { printf 'SELFTEST FAIL judge 3 fields must fail: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_malformed_verdict")"
  [[ "$v" == "fail" ]] || { printf 'SELFTEST FAIL judge bad verdict enum: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_malformed_c5")"
  [[ "$v" == "fail" ]] || { printf 'SELFTEST FAIL judge bad c5 enum: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_malformed_throughput")"
  [[ "$v" == "fail" ]] || { printf 'SELFTEST FAIL judge bad throughput format: got %s\n' "$v"; rc=1; }
  v="$(judge_app_marker "$marker_polluted")"
  [[ "$v" == "fail" ]] || { printf 'SELFTEST FAIL judge polluted extra field: got %s\n' "$v"; rc=1; }
  printf 'SELFTEST marker-parser=pass\n'

  # --- ohosTest cross-check marker ------------------------------------------
  test_marker_pass='N1A_PROBE_TEST_RESULT|verdict=PASS|N1a probe PASS version=n1a-native-dataplane/0.1.0+boringtun-0.7.1 verifiedPackets=2000'
  test_marker_fail='N1A_PROBE_TEST_RESULT|verdict=FAIL|detail=N1a probe c4 failed: fail'
  test_marker_malformed='N1A_PROBE_TEST_RESULT|foo=PASS'
  v="$(judge_test_marker "$test_marker_pass")"
  [[ "$v" == "PASS" ]] || { printf 'SELFTEST FAIL test marker PASS: got %s\n' "$v"; rc=1; }
  v="$(judge_test_marker "$test_marker_fail")"
  [[ "$v" == "FAIL" ]] || { printf 'SELFTEST FAIL test marker FAIL: got %s\n' "$v"; rc=1; }
  v="$(judge_test_marker "$test_marker_malformed")"
  [[ "$v" == "malformed" ]] || { printf 'SELFTEST FAIL test marker malformed: got %s\n' "$v"; rc=1; }
  printf 'SELFTEST test-marker-parser=pass\n'

  # --- marker extraction: hilog prefix / printSync / ResultMsg --------------
  hilog_line="CST 2026-08-30 22:00:00.000  1234  1234 I A02900/cn.alfadb.netbird.r1probe/N1aProbe: $marker_pass"
  resultmsg_line="TestFinished-ResultMsg: $marker_pass; user test finished."
  raw_line="$marker_pass"
  e1="$(extract_app_marker "$hilog_line")"
  e2="$(extract_app_marker "$resultmsg_line")"
  e3="$(extract_app_marker "$raw_line")"
  if [[ "$e1" != "$marker_pass" || "$e2" != "$marker_pass" || "$e3" != "$marker_pass" ]]; then
    printf 'SELFTEST FAIL extract app marker: e1=%q e2=%q e3=%q\n' "$e1" "$e2" "$e3"
    rc=1
  fi
  printf 'SELFTEST marker-extract=pass\n'

  # --- distinct marker collection (app prefix) ------------------------------
  src_a="$tmpdir/src-a.log"; src_b="$tmpdir/src-b.log"; src_c="$tmpdir/src-c.log"
  printf '%s\n' "$marker_pass" >"$src_a"
  printf '%s\n' "$marker_pass" >"$src_b"
  printf '%s\n' "$marker_pass" >"$src_c"
  n1="$(collect_distinct_markers "$MARKER_PREFIX" "$src_a" "$src_b" "$src_c" | wc -l | tr -d ' ')"
  [[ "$n1" == "1" ]] || { printf 'SELFTEST FAIL distinct markers same-source: %s\n' "$n1"; rc=1; }
  printf '%s\n' "$marker_pass_nt" >"$src_b"
  n2="$(collect_distinct_markers "$MARKER_PREFIX" "$src_a" "$src_b" "$src_c" | wc -l | tr -d ' ')"
  [[ "$n2" == "2" ]] || { printf 'SELFTEST FAIL distinct markers different-source: %s\n' "$n2"; rc=1; }
  : >"$src_a"; : >"$src_b"; : >"$src_c"
  n0="$(collect_distinct_markers "$MARKER_PREFIX" "$src_a" "$src_b" "$src_c" | wc -l | tr -d ' ')"
  [[ "$n0" == "0" ]] || { printf 'SELFTEST FAIL distinct markers empty: %s\n' "$n0"; rc=1; }
  printf 'SELFTEST distinct-markers=pass\n'

  # --- three real prefixes collapse to one distinct marker -------------------
  printf '%s\n' "$hilog_line" >"$src_a"
  printf '%s\n' "$resultmsg_line" >"$src_b"
  printf '%s\n' "$raw_line" >"$src_c"
  n1="$(collect_distinct_markers "$MARKER_PREFIX" "$src_a" "$src_b" "$src_c" | wc -l | tr -d ' ')"
  [[ "$n1" == "1" ]] || { printf 'SELFTEST FAIL distinct markers real prefixes: %s\n' "$n1"; rc=1; }
  printf 'SELFTEST distinct-markers-real-prefixes=pass\n'

  # --- guest result code extraction -----------------------------------------
  guest_log="$tmpdir/aa-test.log"
  printf 'TestFinished-ResultCode: 0\n' >"$guest_log"
  guest_code="$(extract_guest_code "$guest_log")"
  [[ "$guest_code" == "0" ]] || { printf 'SELFTEST FAIL guest code extraction: %s\n' "$guest_code"; rc=1; }
  printf 'TestFinished-ResultCode: 1\n' >>"$guest_log"
  guest_code="$(extract_guest_code "$guest_log")"
  [[ "$guest_code" == "1" ]] || { printf 'SELFTEST FAIL guest code last-match: %s\n' "$guest_code"; rc=1; }
  : >"$guest_log"
  guest_code="$(extract_guest_code "$guest_log")"
  [[ -z "$guest_code" ]] || { printf 'SELFTEST FAIL guest code empty: %s\n' "$guest_code"; rc=1; }
  printf 'SELFTEST guest-code=pass\n'

  # --- screenshot non-black check --------------------------------------------
  yavg_file="$tmpdir/black.png"
  ffmpeg -hide_banner -loglevel error -f lavfi -i color=c=black:s=64x64 \
    -frames:v 1 "$yavg_file" -y >/dev/null 2>&1 || true
  if screenshot_nonblack "$yavg_file"; then
    printf 'SELFTEST FAIL black screenshot must fail the non-black check\n'
    rc=1
  fi
  ffmpeg -hide_banner -loglevel error -f lavfi -i color=c=white:s=64x64 \
    -frames:v 1 "$tmpdir/white.png" -y >/dev/null 2>&1 || true
  if ! screenshot_nonblack "$tmpdir/white.png"; then
    printf 'SELFTEST FAIL white screenshot must pass the non-black check\n'
    rc=1
  fi
  : >"$tmpdir/empty.png"
  if screenshot_nonblack "$tmpdir/empty.png"; then
    printf 'SELFTEST FAIL empty file must fail the non-black check\n'
    rc=1
  fi
  printf 'SELFTEST screenshot-nonblack=pass\n'

  # --- guards (N0 mirror) ----------------------------------------------------
  if ( PHYS_1_TARGET=1; guard_physical_target ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard PHYS_1_TARGET\n'; rc=1
  fi
  if ( TARGET=192.168.1.1:10000; guard_emulator_target ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard TARGET\n'; rc=1
  fi
  if ( HDC_TARGET=192.168.1.1:10000; guard_emulator_target ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard HDC_TARGET\n'; rc=1
  fi
  if ( guard_emulator_target "192.168.1.1:10000" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard EMULATOR_TARGET request\n'; rc=1
  fi
  if ! ( guard_emulator_target "" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard EMULATOR_TARGET request positive\n'; rc=1
  fi
  if ( HDC_PORT=5555; guard_hdc_port ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard HDC_PORT\n'; rc=1
  fi
  if ( guard_emulator_instance "other_instance" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard EMULATOR_INSTANCE\n'; rc=1
  fi
  if ! ( guard_emulator_instance "netbird_api24_phone" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard EMULATOR_INSTANCE positive\n'; rc=1
  fi
  if ( guard_helpers "/tmp/evil-connect" "" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard CONNECT_HELPER override\n'; rc=1
  fi
  if ( guard_helpers "" "/tmp/evil-stop" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard STOP_HELPER override\n'; rc=1
  fi
  if ! ( guard_helpers "" "" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard helpers positive\n'; rc=1
  fi
  if ! ( unset PHYS_1_TARGET TARGET HDC_TARGET HDC_PORT; \
         guard_physical_target && guard_emulator_target "" && guard_hdc_port && \
         guard_emulator_instance "" && guard_helpers "" "" ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guards positive\n'; rc=1
  fi
  printf 'SELFTEST guards=pass\n'

  # --- no-clobber ------------------------------------------------------------
  existing="$tmpdir/existing.log"
  newfile="$tmpdir/new.log"
  : >"$existing"
  if ( TRANSCRIPT="$existing"; check_no_clobber ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL no-clobber existing\n'; rc=1
  fi
  if ! ( TRANSCRIPT="$newfile"; check_no_clobber ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL no-clobber new\n'; rc=1
  fi
  printf 'SELFTEST no-clobber=pass\n'

  # --- manifest seal (live child holding the transcript fd; N0 mirror) ------
  seal_tmp="$tmpdir/seal"
  mkdir -p "$seal_tmp"
  : >"$seal_tmp/transcript.log"
  printf 'evidence_id=EV-N1A-EMU24-20260830-0001\n' >"$seal_tmp/manifest.txt"
  printf 'verdict=pass\n' >>"$seal_tmp/manifest.txt"
  (
    exec >>"$seal_tmp/transcript.log" 2>&1
    printf 'SELFTEST_SEAL_TRANSCRIPT_LINE=1\n'
    sleep 30 &
    printf '%s\n' "$!" >"$seal_tmp/holder.pid"
    TRANSCRIPT="$seal_tmp/transcript.log"
    MANIFEST="$seal_tmp/manifest.txt"
    result="pass"
    seal_and_finalize
    seal_rc=$?
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
  for f in '^transcript_final_sha256=' '^manifest_sha256=' '^final_exit_code=0$' \
           '^run_status=' '^fail_reason=' '^blocked_reason=' '^transcript_final_bytes='; do
    if ! grep -q "$f" "$seal_tmp/manifest.txt"; then
      printf 'SELFTEST FAIL seal field missing: %s\n' "$f"
      rc=1
    fi
  done
  seal_bytes="$(grep '^transcript_final_bytes=' "$seal_tmp/manifest.txt" | cut -d= -f2)"
  seal_hash="$(grep '^transcript_final_sha256=' "$seal_tmp/manifest.txt" | cut -d= -f2)"
  recomputed_hash="$(head -c "$seal_bytes" "$seal_tmp/transcript.log" | sha256sum | awk '{print $1}')"
  if [[ "$recomputed_hash" != "$seal_hash" ]]; then
    printf 'SELFTEST FAIL seal transcript hash recompute mismatch\n'
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
    printf 'SELFTEST FAIL seal manifest self-hash mismatch\n'
    rc=1
  fi
  printf 'SELFTEST manifest-seal=pass\n'

  # --- defect 1: fail path must seal (idempotent, fields complete) ----------
  # Simulate: base manifest exists, result=fail; seal_and_finalize must run
  # TWICE without duplicating the final block, and must record run_status=fail
  # plus all six final fields. No Emulator involved.
  local failseal_tmp failseal_manifest failseal_transcript
  failseal_tmp="$(mktemp -d "${TMPDIR:-/tmp}/n1a-selftest-failseal.XXXXXX")"
  failseal_manifest="$failseal_tmp/manifest.txt"
  failseal_transcript="$failseal_tmp/transcript.log"
  printf 'evidence_id=EV-N1A-SELFTEST-FAILSEAL\n' >"$failseal_manifest"
  printf 'transcript-line-1\n' >"$failseal_transcript"
  (
    sealed=0
    MANIFEST="$failseal_manifest"
    TRANSCRIPT="$failseal_transcript"
    result="fail"
    fail_reason="selftest simulated criterion violation"
    blocked_reason=""
    seal_and_finalize 0
    seal_and_finalize 0
  )
  local failseal_final_rows failseal_field_ok=0
  failseal_final_rows="$(grep -c '^final_exit_code=' "$failseal_manifest")"
  for f in final_exit_code run_status fail_reason blocked_reason \
           transcript_final_bytes transcript_final_sha256 manifest_sha256; do
    grep -q "^${f}=" "$failseal_manifest" || failseal_field_ok=1
  done
  grep -q '^run_status=fail$' "$failseal_manifest" || failseal_field_ok=1
  grep -q '^fail_reason=selftest simulated criterion violation$' "$failseal_manifest" || failseal_field_ok=1
  if (( failseal_final_rows != 1 || failseal_field_ok != 0 )); then
    printf 'SELFTEST FAIL fail-path-seal: rows=%s field_ok=%s\n' \
      "$failseal_final_rows" "$failseal_field_ok"
    rc=1
  fi
  rm -rf "$failseal_tmp"
  printf 'SELFTEST fail-path-seal=pass\n'

  # --- defect 2: chunked JSON reassembly (ordered/shuffled/dup/missing/sha/oversize/total) ---
  local rj_tmp rj_out rj_file rj_json rj_sha rj_sha16 rj_i rj_total rj_data
  rj_tmp="$(mktemp -d "${TMPDIR:-/tmp}/n1a-selftest-reasm.XXXXXX")"
  rj_out="$rj_tmp/detail.json"
  # Fixture must span MULTIPLE chunks so the shuffled/dup case can build a
  # meaningful (total-1, total-2) prefix (a single-chunk fixture degenerates
  # to part=-1 and the reassembler rightly rejects the polluted line).
  rj_json='{"version":"n1a-native-dataplane/0.1.0+boringtun-0.7.1","ok":false,"criteria":{"c1":"pass","c2":"pass","c3":"pass","c4":"pass","c5":"not-triggered","c6":"pass","c7":"pass","c8":"pass","c9":"pass"},"handshake":{"established":true,"elapsed_ms":1173.276,"attempts":1},"integrity":{"packets_total":4000,"verified":4000,"mismatch":0,"lost":0,"extra":0,"byte_accounting":"exact"},"throughput_mibps":93.10,"backpressure":{"induced":false,"kernel_queue_drops":43435,"delivered":512,"corrupted":0},"tick":{"gaps":3,"keepalives":6,"session_alive":true},"resources":{"process_model":"testrunner","fd_t0":11,"fd_t3":11,"task_t0":8,"task_t3":8,"rss_kb_t0":94208,"rss_kb_t3":95104}}'
  rj_sha="$(printf '%s' "$rj_json" | sha256sum | awk '{print $1}')"
  rj_sha16="${rj_sha:0:16}"
  rj_total=$(( ( ${#rj_json} + N1A_JSON_CHUNK_MAX - 1 ) / N1A_JSON_CHUNK_MAX ))

  emit_test_chunks() {
    local out_file="$1" order="$2"
    local i start data
    : >"$out_file"
    for i in $order; do
      start=$(( i * N1A_JSON_CHUNK_MAX ))
      data="${rj_json:$start:$N1A_JSON_CHUNK_MAX}"
      printf 'N1A_JSON|part=%s|total=%s|sha256=%s|data=%s\n' \
        "$i" "$rj_total" "$rj_sha16" "$data" >>"$out_file"
    done
  }

  # (a) ordered -> pass, digest matches
  emit_test_chunks "$rj_tmp/a.log" "$(seq 0 $(( rj_total - 1 )))"
  if probe_json_sha="$(reassemble_probe_json "$rj_out" "$rj_tmp/a.log")" \
      && [[ "$probe_json_sha" == "$rj_sha" ]] \
      && [[ "$(cat "$rj_out")" == "$rj_json" ]]; then
    printf 'SELFTEST reassemble-ordered=pass\n'
  else
    printf 'SELFTEST FAIL reassemble-ordered\n'
    rc=1
  fi

  # (b) shuffled + identical duplicated lines across two sources -> pass
  emit_test_chunks "$rj_tmp/b1.log" "$(( rj_total - 1 )) $(( rj_total - 2 ))"
  emit_test_chunks "$rj_tmp/b2.log" "$(seq 0 $(( rj_total - 2 )))"
  cat "$rj_tmp/b1.log" >>"$rj_tmp/b2.log"
  if probe_json_sha="$(reassemble_probe_json "$rj_out" "$rj_tmp/b1.log" "$rj_tmp/b2.log")" \
      && [[ "$probe_json_sha" == "$rj_sha" ]]; then
    printf 'SELFTEST reassemble-shuffled-dup=pass\n'
  else
    printf 'SELFTEST FAIL reassemble-shuffled-dup\n'
    rc=1
  fi

  # (c) missing part -> fail
  emit_test_chunks "$rj_tmp/c.log" "$(seq 0 $(( rj_total - 2 )))"
  if reassemble_probe_json "$rj_out" "$rj_tmp/c.log" >/dev/null 2>&1; then
    printf 'SELFTEST FAIL reassemble-missing-part must fail\n'
    rc=1
  else
    printf 'SELFTEST reassemble-missing-part=pass\n'
  fi

  # (d) sha16 mismatch -> fail
  emit_test_chunks "$rj_tmp/d.log" "$(seq 0 $(( rj_total - 1 )))"
  sed -i "1s/sha256=${rj_sha16}/sha256=0000000000000000/" "$rj_tmp/d.log"
  if reassemble_probe_json "$rj_out" "$rj_tmp/d.log" >/dev/null 2>&1; then
    printf 'SELFTEST FAIL reassemble-sha-mismatch must fail\n'
    rc=1
  else
    printf 'SELFTEST reassemble-sha-mismatch=pass\n'
  fi

  # (e) oversize chunk data -> fail
  emit_test_chunks "$rj_tmp/e.log" "$(seq 0 $(( rj_total - 1 )))"
  printf 'N1A_JSON|part=0|total=%s|sha256=%s|data=%s\n' \
    "$rj_total" "$rj_sha16" "$(head -c $(( N1A_JSON_CHUNK_MAX + 1 )) /dev/zero | tr '\0' 'x')" \
    >>"$rj_tmp/e.log"
  if reassemble_probe_json "$rj_out" "$rj_tmp/e.log" >/dev/null 2>&1; then
    printf 'SELFTEST FAIL reassemble-oversize must fail\n'
    rc=1
  else
    printf 'SELFTEST reassemble-oversize=pass\n'
  fi

  # (f) inconsistent total across lines -> fail
  emit_test_chunks "$rj_tmp/f.log" "$(seq 0 $(( rj_total - 1 )))"
  printf 'N1A_JSON|part=%s|total=999|sha256=%s|data=x\n' \
    "$rj_total" "$rj_sha16" >>"$rj_tmp/f.log"
  if reassemble_probe_json "$rj_out" "$rj_tmp/f.log" >/dev/null 2>&1; then
    printf 'SELFTEST FAIL reassemble-total-inconsistent must fail\n'
    rc=1
  else
    printf 'SELFTEST reassemble-total-inconsistent=pass\n'
  fi

  rm -rf "$rj_tmp"

  # --- overlay-diag extraction (defect #3) ------------------------------------
  local od_tmp od_out
  od_tmp="$(mktemp -d "${TMPDIR:-/tmp}/n1a-selftest-diag.XXXXXX")"
  od_out="$od_tmp/diag.log"
  # (a) Normal N1A_DIAG lines from multiple sources + dedup of identical lines.
  printf 'N1A_DIAG|stage=verdict-integrity-mismatch|ok=0|v=1999\n' >"$od_tmp/a.log"
  printf 'N1A_DIAG_JSON_BEG|sha=abc123\n' >>"$od_tmp/a.log"
  printf 'N1A_CATCH_FALLBACK|native_throw=true\n' >>"$od_tmp/a.log"
  printf 'N1A_DIAG|stage=verdict-integrity-mismatch|ok=0|v=1999\n' >"$od_tmp/b.log" # dup of a.log's first
  printf 'unrelated hilog line\n' >>"$od_tmp/b.log"
  extract_overlay_diag "$od_out" "$od_tmp/a.log" "$od_tmp/b.log" 2>/dev/null
  local od_lines
  od_lines="$(wc -l <"$od_out" | tr -d ' ')"
  if [[ "$od_lines" -eq 3 ]] && grep -qF 'N1A_DIAG|stage=' "$od_out" \
      && grep -qF 'N1A_DIAG_JSON_BEG' "$od_out" \
      && grep -qF 'N1A_CATCH_FALLBACK' "$od_out" \
      && ! grep -qF 'unrelated' "$od_out"; then
    printf 'SELFTEST diag-extract=pass\n'
  else
    printf 'SELFTEST FAIL diag-extract (lines=%s expected 3)\n' "$od_lines"
    rc=1
  fi
  # (b) Empty sources -> empty but existing file.
  : >"$od_tmp/empty.log"
  extract_overlay_diag "$od_out" "$od_tmp/empty.log" 2>/dev/null
  if [[ -f "$od_out" ]] && [[ ! -s "$od_out" ]]; then
    printf 'SELFTEST diag-empty=pass\n'
  else
    printf 'SELFTEST FAIL diag-empty\n'
    rc=1
  fi
  # (c) null-probe diag line (defect #3 overlay null path).
  printf 'N1A_DIAG|stage=probe-null|probe=null\n' >"$od_tmp/c.log"
  extract_overlay_diag "$od_out" "$od_tmp/c.log" 2>/dev/null
  if grep -qF 'stage=probe-null|probe=null' "$od_out"; then
    printf 'SELFTEST diag-null-probe=pass\n'

  # 0008 ruling prerequisite #3: N1A_C5 line extraction from mixed sources.
  local c5_tmp
  c5_tmp="$(mktemp "${TMPDIR:-/tmp}/n1a-st-c5.XXXXXX")"
  : >"$c5_tmp"
  printf 'N1A_DIAG|stage=ok|ok=0\n' >>"$c5_tmp"
  printf 'N1A_C5|delivered=512|corrupted=0|rounds=128|elapsed_ms=200.5|eagain=0|error=bp timebox exceeded (deadlock): delivered=400 of 512\n' >>"$c5_tmp"
  printf 'N1A_CATCH_FALLBACK|native_throw=true\n' >>"$c5_tmp"
  extract_overlay_diag "$od_out" "$c5_tmp" "$c5_tmp" >/dev/null 2>&1
  if grep -qF 'N1A_C5|delivered=512' "$od_out" && \
     grep -qF 'N1A_DIAG|stage=ok' "$od_out" && \
     grep -qF 'N1A_CATCH_FALLBACK' "$od_out"; then
    printf 'SELFTEST diag-n1a-c5-extraction=pass\n'
  else
    printf 'SELFTEST FAIL diag-n1a-c5-extraction\n'; rc=1
  fi

  # 0008 ruling prerequisite #4: bfreeze path is declared and the no-clobber
  # list covers it (selftest mode keeps it empty — the formal path sets it
  # from EVIDENCE_ROOT; the check here verifies the variable exists and the
  # formal-path assignment is in the source).
  if [[ -n "${BFREEZE_LOG+x}" ]] && \
     grep -q 'BFREEZE_LOG.*EVIDENCE_ID' "$0" 2>/dev/null; then
    printf 'SELFTEST bfreeze-path=pass\n'
  else
    printf 'SELFTEST FAIL bfreeze-path (not declared or no formal assignment)\n'; rc=1
  fi
  else
    printf 'SELFTEST FAIL diag-null-probe\n'
    rc=1
  fi
  rm -rf "$od_tmp"

  # --- teardown no-op --------------------------------------------------------
  emulator_started=0
  installed=0
  if ! teardown >/dev/null 2>&1; then
    printf 'SELFTEST FAIL teardown no-op\n'
    rc=1
  fi
  printf 'SELFTEST cleanup=pass\n'

  # --- teardown no-device-phase: no hdc kill / no residual port scan --------
  teardown_output="$(SELFTEST=0 DRY_RUN=0 device_phase_started=0 emulator_started=0 installed=0 app_started=0 \
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
if (( SELFTEST != 1 )); then
  guard_physical_target || defect "PHYS_1_TARGET is set; this runner is Emulator-only and must never target a physical device"
  guard_emulator_target || defect "TARGET/HDC_TARGET/EMULATOR_TARGET is not the fixed $EMULATOR_TARGET Emulator target"
  guard_hdc_port || defect "HDC_PORT=${HDC_PORT:-<unset>}; only the fixed 10000 Emulator HDC port is allowed"
  guard_emulator_instance || defect "EMULATOR_INSTANCE request is not the fixed netbird_api24_phone Emulator instance"
  guard_helpers || defect "CONNECT_HELPER/STOP_HELPER override is forbidden; helpers are hardcoded"
  if [[ "$WORKSPACE" != "$DEFAULT_WORKSPACE" ]]; then
    defect "WORKSPACE override is not the script repository: $WORKSPACE (expected $DEFAULT_WORKSPACE)"
  fi
  if [[ ! "$EVIDENCE_ID" =~ ^EV-[A-Z0-9]+-[A-Z0-9]+-[0-9]{8}-[0-9]{4}$ ]]; then
    defect "EVIDENCE_ID does not match EV-<gate>-<target>-<YYYYMMDD>-<NNNN>: $EVIDENCE_ID"
  fi
  if [[ ! -d "$WORKSPACE/.git" ]]; then
    defect "WORKSPACE is not a git repository: $WORKSPACE"
  fi
  code_sha="$(git -C "$WORKSPACE" rev-parse HEAD)"
  git_branch="$(git -C "$WORKSPACE" rev-parse --abbrev-ref HEAD)"
  if ! git -C "$WORKSPACE" cat-file -e "$SNAPSHOT_HEAD^{commit}" 2>/dev/null; then
    defect "snapshot commit $SNAPSHOT_HEAD is not present in the repository"
  fi
  git_clean=0
  if git -C "$WORKSPACE" diff --quiet && git -C "$WORKSPACE" diff --cached --quiet && \
     [[ -z "$(git -C "$WORKSPACE" ls-files --others --exclude-standard)" ]]; then
    git_clean=1
  fi
  runner_in_head=0
  if git -C "$WORKSPACE" cat-file -e "HEAD:spikes/n1a-native-dataplane/n1a-emulator-run.sh" 2>/dev/null; then
    runner_in_head=1
  fi
  if (( DRY_RUN != 1 )); then
    if (( git_clean != 1 )); then
      defect "working tree is not clean; formal evidence must map to a committed code_sha"
    fi
    if (( runner_in_head != 1 )); then
      defect "HEAD does not contain spikes/n1a-native-dataplane/n1a-emulator-run.sh; commit the runner before a formal run"
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
  PAGE_HILOG="$EVIDENCE_ROOT/$EVIDENCE_ID-hilog-page.log"
  MANIFEST="$EVIDENCE_ROOT/$EVIDENCE_ID-manifest.txt"
  CONSOLE="$EVIDENCE_ROOT/$EVIDENCE_ID-emulator-console.log"
  BUILD_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-build.log"
  N1A_BUILD_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-n1a-build.log"
  SNAPSHOT_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-snapshot-prep.log"
  AA_TEST_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-aa-test.log"
  AA_START_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-aa-start.log"
  SOURCE_MANIFEST="$EVIDENCE_ROOT/$EVIDENCE_ID-source-manifest.txt"
  PROBE_DETAIL="$EVIDENCE_ROOT/$EVIDENCE_ID-probe-detail.json"
  OVERLAY_DIAG="$EVIDENCE_ROOT/$EVIDENCE_ID-overlay-diag.log"
  SCREENSHOT_A="$EVIDENCE_ROOT/$EVIDENCE_ID-phase-a.png"
  SCREENSHOT_B="$EVIDENCE_ROOT/$EVIDENCE_ID-phase-b-page.png"
  BFREEZE_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-bfreeze.log"
  check_no_clobber || exit 2
  if (( DRY_RUN == 1 )); then
    DRY_TMP="$(mktemp -d "${TMPDIR:-/tmp}/n1a-emu24-dryrun.XXXXXX")"
    BUILD_LOG="$DRY_TMP/build.log"
    N1A_BUILD_LOG="$DRY_TMP/n1a-build.log"
    SNAPSHOT_LOG="$DRY_TMP/snapshot-prep.log"
    AA_TEST_LOG="$DRY_TMP/aa-test.log"
    AA_START_LOG="$DRY_TMP/aa-start.log"
    SOURCE_MANIFEST="$DRY_TMP/source-manifest.txt"
    PROBE_DETAIL=""
    OVERLAY_DIAG=""
    TRANSCRIPT=""
    TAG_HILOG=""
    APP_HILOG=""
    PAGE_HILOG=""
    MANIFEST=""
    CONSOLE=""
    SCREENSHOT_A=""
    SCREENSHOT_B=""
  else
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
printf 'RUNNER=n1a-emulator-run.sh\n'
printf 'EVIDENCE_ID=%s\n' "$EVIDENCE_ID"
printf 'STARTED_AT=%s\n' "$started_at"
printf 'CLOCK_SOURCE=host_CLOCK_REALTIME_date_iso_8601_seconds\n'
printf 'TIMEZONE=%s\n' "$(date +%Z%:z)"
printf 'RECORD_SCOPE=N1a_native_WireGuard_data_plane_pump_BoringTun_0.7.1_ffi-bindings_API24_x86_64_Emulator\n'
printf 'CRITERIA=frozen-r2 docs/n1a-gate-plan.md criteria-frozen-r2 (not amendable after measurement)\n'
printf 'TARGET_TUPLE=%s\n' "$TARGET_TUPLE"
printf 'EMULATOR_INSTANCE=%s\n' "$EMULATOR_INSTANCE"
printf 'EMULATOR_TARGET=%s\n' "$EMULATOR_TARGET"
printf 'BUNDLE=%s\n' "$BUNDLE"
printf 'TEST_MODULE=%s\n' "$TEST_MODULE"
printf 'TEST_RUNNER=%s\n' "$TEST_RUNNER"
printf 'SNAPSHOT_COMMIT=%s\n' "$SNAPSHOT_HEAD"
printf 'BORINGTUN_VERSION=%s\n' "$BORINGTUN_VERSION"
printf 'BORINGTUN_CRATE_SHA256=%s\n' "$BORINGTUN_CRATE_SHA256"
printf 'MARKER_FROZEN_FORM=N1A_RESULT|verdict=<PASS|FAIL>|c5=<induced|not-triggered|fail>|throughput_mibps=<x.xx>\n'
printf 'THROUGHPUT_FLOOR_MIBPS=%s\n' "$THROUGHPUT_FLOOR_MIBPS"
printf 'TWO_PHASE_FLOW=phase-A aa-test judgment + phase-B C9 result page (registrable interpretation, criteria-holder ruled 2026-08-30); per-window duplicate rule: A judgment window and B page window are isolated via hilog -r (isolation failure = environment blocked); C1 binding: A proves only the N0 same-path (aa test/ohosTest cross-check), B proves only the ordinary-EntryAbility dlopen - neither window substitutes for the other; overall pass iff both windows independently PASS; B is a second FULL probe run (force-stop then aa start, independent UIAbility process); c5 and throughput recorded for BOTH windows (phase_a_c5/phase_b_c5, no collapsing to the more favorable value; backpressure-verified claims allowed only for an induced execution); same-window distinct collapses identical texts only, two different texts = fail\n'
printf 'PHYSICAL_DEVICE_USED=false\n'
printf 'E8_STATUS=CLOSED\n'
printf 'TEST_RUNNER_USED=true\n'
printf 'PUBLIC_NETWORK_ALLOWED=false\n'
printf 'WORKSPACE=%s\n' "$WORKSPACE"
printf 'EVIDENCE_ROOT=%s\n' "$EVIDENCE_ROOT"
printf 'BUILD_TOOLCHAIN=command-line-tools/6.1.1.290 (stable; hvigorw/ohpm/native SDK for build)\n'
printf 'EMULATOR_RUNTIME=command-line-tools/26.0.0.461 (beta; Emulator binary + hdc)\n'
printf 'NATIVE_SDK_VERSION=%s\n' "$native_sdk_version"
printf 'NATIVE_SDK_API=%s\n' "$native_sdk_api"
printf 'NATIVE_SDK_RELEASE_TYPE=%s\n' "$native_sdk_release"
if (( DRY_RUN == 1 )); then
  printf 'EXECUTION=dry-run\n'
fi

# --- repository state -------------------------------------------------------
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
resolved_native_sdk="$(readlink -f "$DEVECO_SDK_HOME/default/openharmony/native" 2>/dev/null || true)"
if [[ -z "$resolved_native_sdk" || ! -d "$resolved_native_sdk" ]]; then
  printf 'HOST_CHECK deveco-sdk-home=fail path=%s\n' "$DEVECO_SDK_HOME"
  host_missing=$((host_missing + 1))
else
  printf 'HOST_CHECK deveco-sdk-home=pass path=%s\n' "$resolved_native_sdk"
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
check_command ffmpeg ffmpeg || true
check_command cargo cargo || true
check_command rustc rustc || true
if command -v shellcheck >/dev/null 2>&1; then
  printf 'HOST_CHECK shellcheck=pass command=shellcheck\n'
else
  printf 'HOST_CHECK shellcheck=fail command=shellcheck (external bash -n required)\n'
fi
if (( host_missing > 0 )); then
  defect "missing required host tools ($host_missing)"
fi

# --- 1. offline dual-ABI build (build.sh: cargo --offline --locked) ---------
if ! (
  cd "$PROJECT_DIR"
  print_command bash "$PROJECT_DIR/build.sh"
  bash "$PROJECT_DIR/build.sh"
) 2>&1 | tee "$N1A_BUILD_LOG"; then
  blocked_env "N1a offline dual-ABI build failed (see $N1A_BUILD_LOG); build-input drift is environment-class per the frozen aggregation"
fi
[[ -f "$PROJECT_DIR/out/x86_64/libentry.so" ]] || blocked_env "build did not produce out/x86_64/libentry.so"
[[ -f "$PROJECT_DIR/out/aarch64/libentry.so" ]] || blocked_env "build did not produce out/aarch64/libentry.so"
printf 'N1A_BUILD_VERDICT=pass\n'

# --- 2. fixed r1 snapshot preparation ---------------------------------------
snapshot_dir="$(mktemp -d "${TMPDIR:-/tmp}/n1a-emu24-snapshot.XXXXXX")"
if ! (
  print_command bash "$PROJECT_DIR/prepare-hap-snapshot.sh" "$snapshot_dir"
  bash "$PROJECT_DIR/prepare-hap-snapshot.sh" "$snapshot_dir"
) 2>&1 | tee "$SNAPSHOT_LOG"; then
  blocked_env "snapshot preparation failed (see $SNAPSHOT_LOG)"
fi
[[ -f "$snapshot_dir/entry/src/main/cpp/n1a_overlay.cpp" ]] || defect "staged snapshot missing n1a_overlay.cpp"
[[ -f "$snapshot_dir/entry/src/main/cpp/types/libentry/index.d.ts" ]] || defect "staged snapshot missing libentry types"
[[ -f "$snapshot_dir/entry/src/main/ets/pages/runN1aProbeTest.ets" ]] || defect "staged snapshot missing runN1aProbeTest.ets"
[[ -f "$snapshot_dir/entry/src/main/ets/pages/Index.ets" ]] || defect "staged snapshot missing the staged C9 result page"
grep -q "runN1aProbeTest" "$snapshot_dir/entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets" || defect "staged test runner missing runN1aProbeTest"
grep -q "$TEST_MARKER_PREFIX" "$snapshot_dir/entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets" || defect "staged test runner missing $TEST_MARKER_PREFIX"
grep -q "runN1aProbe" "$snapshot_dir/entry/src/main/ets/pages/Index.ets" || defect "staged Index page does not call runN1aProbe"
grep -q "libentry.so" "$snapshot_dir/entry/oh-package.json5" || defect "staged oh-package.json5 missing libentry.so dependency"
grep -q "libentry.so" "$snapshot_dir/entry/oh-package-lock.json5" || defect "staged oh-package-lock.json5 missing libentry.so entry"
grep -q "x86_64" "$snapshot_dir/entry/build-profile.json5" || defect "staged build-profile.json5 missing x86_64 abiFilter"
if grep -q "arm64-v8a" "$snapshot_dir/entry/build-profile.json5"; then
  defect "staged build-profile.json5 still contains arm64-v8a abiFilter"
fi
[[ -f "$snapshot_dir/entry/libs/x86_64/libentry.so" ]] || defect "staged snapshot missing libs/x86_64/libentry.so"
printf 'SNAPSHOT_STAGE_VERIFY=pass\n'
printf 'SNAPSHOT_DIR=%s\n' "$snapshot_dir"

plugin_path="$(grep -oP "file:\K[^']+" "$snapshot_dir/hvigor/hvigor-config.json5" | head -1 || true)"
if [[ "$plugin_path" != "$BUILD_TOOLS/hvigor/hvigor-ohos-plugin" ]]; then
  defect "snapshot hvigor plugin path $plugin_path != $BUILD_TOOLS/hvigor/hvigor-ohos-plugin"
fi
[[ -d "$plugin_path" ]] || defect "hvigor plugin directory missing: $plugin_path"
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
  blocked_env "HAP build failed (see $BUILD_LOG); build-input drift is environment-class per the frozen aggregation"
fi
app_hap="$snapshot_dir/$APP_HAP_REL"
test_hap="$snapshot_dir/$TEST_HAP_REL"
[[ -f "$app_hap" ]] || blocked_env "clean build did not produce application HAP"
[[ -f "$test_hap" ]] || blocked_env "clean build did not produce test HAP"
printf 'HAP_BUILD_VERDICT=pass\n'

# --- 4. HAP member identity -------------------------------------------------
app_member="$(mktemp "${TMPDIR:-/tmp}/n1a-emu24-app-member.XXXXXX")"
test_member="$(mktemp "${TMPDIR:-/tmp}/n1a-emu24-test-member.XXXXXX")"
if ! unzip -p "$app_hap" libs/x86_64/libentry.so >"$app_member"; then
  defect "unzip failed to extract libentry.so from application HAP"
fi
if ! unzip -p "$test_hap" libs/x86_64/libentry.so >"$test_member"; then
  defect "unzip failed to extract libentry.so from test HAP"
fi
[[ -s "$app_member" ]] || defect "application HAP has no libs/x86_64/libentry.so member"
[[ -s "$test_member" ]] || defect "test HAP has no libs/x86_64/libentry.so member"
built_sha="$(sha256sum "$PROJECT_DIR/out/x86_64/libentry.so" | awk '{print $1}')"
app_member_sha="$(sha256sum "$app_member" | awk '{print $1}')"
test_member_sha="$(sha256sum "$test_member" | awk '{print $1}')"
printf 'APP_MEMBER_SHA256=%s\n' "$app_member_sha"
printf 'TEST_MEMBER_SHA256=%s\n' "$test_member_sha"
if [[ "$app_member_sha" != "$built_sha" || "$test_member_sha" != "$built_sha" ]]; then
  blocked_env "HAP libentry.so member is not byte-equal to out/x86_64/libentry.so; build-input drift is environment-class"
fi
printf 'HAP_MEMBER_IDENTITY=pass\n'
app_list="$(unzip -Z1 "$app_hap" 2>&1 || true)"
test_list="$(unzip -Z1 "$test_hap" 2>&1 || true)"
if grep -E 'libs/arm64-v8a/' <<<"$app_list" >/dev/null || \
   grep -E 'libs/arm64-v8a/' <<<"$test_list" >/dev/null; then
  defect "arm64 member present in application or test HAP"
fi
printf 'ARM64_MEMBER=false\n'
if grep -E 'libgoprobe|libtls-|libneededprobe|libfdprobe' <<<"$app_list" >/dev/null || \
   grep -E 'libgoprobe|libtls-|libneededprobe|libfdprobe' <<<"$test_list" >/dev/null; then
  defect "forbidden historical member in application or test HAP"
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
  defect "application HAP identity mismatch (expected r1probe bundle, entry module, EntryAbility)"
fi
if [[ "$test_module_json" != *'"name":"entry_test"'* ||
      "$test_module_json" != *'"name":"OpenHarmonyTestRunner"'* ]]; then
  defect "test HAP identity mismatch (expected entry_test module with OpenHarmonyTestRunner)"
fi
printf 'HAP_IDENTITY=pass\n'
run sha256sum "$app_hap" "$test_hap"

# --- 6. source / lock / crate / toolchain / artifact hashes ------------------
source_inputs=(
  spikes/n1a-native-dataplane/Cargo.toml
  spikes/n1a-native-dataplane/Cargo.lock
  spikes/n1a-native-dataplane/src/lib.rs
  spikes/n1a-native-dataplane/src/pump.rs
  spikes/n1a-native-dataplane/napi/n1a_overlay.cpp
  spikes/n1a-native-dataplane/napi/types/index.d.ts
  spikes/n1a-native-dataplane/napi/types/oh-package.json5
  spikes/n1a-native-dataplane/napi/runN1aProbeTest.ets
  spikes/n1a-native-dataplane/napi/pages/Index.ets
  spikes/n1a-native-dataplane/napi/ohosTest/OpenHarmonyTestRunner.ets
  spikes/n1a-native-dataplane/build.sh
  spikes/n1a-native-dataplane/prepare-hap-snapshot.sh
  spikes/n1a-native-dataplane/n1a-emulator-run.sh
  spikes/n1a-native-dataplane/README.md
)
(
  cd "$WORKSPACE"
  sha256sum "${source_inputs[@]}"
) >"$SOURCE_MANIFEST"
staged_inputs=(
  entry/src/main/cpp/n1a_overlay.cpp
  entry/src/main/cpp/types/libentry/index.d.ts
  entry/src/main/cpp/types/libentry/oh-package.json5
  entry/src/main/ets/pages/runN1aProbeTest.ets
  entry/src/main/ets/pages/Index.ets
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
[[ -n "$CRATE_FILE" ]] || blocked_env "boringtun-${BORINGTUN_VERSION}.crate not found in cargo cache"
crate_sha="$(sha256sum "$CRATE_FILE" | awk '{print $1}')"
[[ "$crate_sha" == "$BORINGTUN_CRATE_SHA256" ]] || blocked_env "boringtun crate checksum mismatch: got $crate_sha; build-input drift is environment-class"
printf 'BORINGTUN_CRATE_SHA256_VERIFY=pass\n'
lock_sha="$(grep -A3 'name = "boringtun"' "$PROJECT_DIR/Cargo.lock" | grep checksum | awk '{print $3}' | tr -d '"')"
[[ "$lock_sha" == "$BORINGTUN_CRATE_SHA256" ]] || blocked_env "Cargo.lock boringtun checksum mismatch: got $lock_sha; build-input drift is environment-class"
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
readelf_diag="$(readelf -d "$PROJECT_DIR/out/x86_64/libentry.so" || true)"
while IFS= read -r line; do
  if [[ "$line" == *'NEEDED'* ]]; then
    printf '%s\n' "$line"
  fi
done <<<"$readelf_diag"
printf 'ARTIFACT_HASH_VERIFY=pass\n'

# --- public endpoint source scan (also in dry-run; fail-closed) -----------
set +e
scan_output="$(rg -n '(114\.114\.114\.114|8\.8\.8\.8|https?://)' \
    "$PROJECT_DIR/n1a-emulator-run.sh" "$PROJECT_DIR/build.sh" \
    "$PROJECT_DIR/prepare-hap-snapshot.sh" "$PROJECT_DIR/src/lib.rs" \
    "$PROJECT_DIR/src/pump.rs" "$PROJECT_DIR/napi/n1a_overlay.cpp" \
    "$PROJECT_DIR/napi/runN1aProbeTest.ets" "$PROJECT_DIR/napi/pages/Index.ets" \
    "$PROJECT_DIR/napi/ohosTest/OpenHarmonyTestRunner.ets" 2>&1)"
scan_rc=$?
set -e
if (( scan_rc != 1 )); then
  if (( scan_rc == 0 )); then
    printf '%s\n' "$scan_output"
    defect "public endpoint literal exists in N1a executable inputs"
  fi
  defect "rg public endpoint source scan failed (rc=$scan_rc)"
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

# Defect 1: the base manifest exists BEFORE any measurement can happen, so
# every terminal state (pass/blocked/fail) has raw+manifest together and
# seal_and_finalize always has a manifest to seal.
write_base_manifest
printf 'BASE_MANIFEST=written\n'

# --- 7. Emulator cold boot (N0 mirror) --------------------------------------
device_phase_started=1
"$HDC" kill >/dev/null 2>&1 </dev/null || true
HDC_PORT="$EMULATOR_HDC_PORT" timeout 60 "$STOP_HELPER" >/dev/null 2>&1 </dev/null || true
if pgrep -f '/emulator/Emulator.*-start '"$EMULATOR_INSTANCE"'|qemu-system.*'"$EMULATOR_INSTANCE" >/dev/null; then
  blocked_env "residual Emulator exists before cold boot (environment-class per the frozen aggregation)"
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
  blocked_env "Emulator process exited during startup (environment-class per the frozen aggregation)"
fi
printf 'EMULATOR_START_VERDICT=pass\n'

connected=0
for attempt in $(seq 1 80); do
  HDC_PORT="$EMULATOR_HDC_PORT" timeout 30 "$CONNECT_HELPER" >/dev/null 2>&1 </dev/null || true
  shell_probe="$(hdc shell "echo n1a-emu24-connect-$attempt" 2>&1 || true)"
  distribution="$(hdc shell 'param get const.product.os.dist.name' 2>&1 | tr -d '\r' || true)"
  printf 'CONNECTIVITY attempt=%s shell=%q distribution=%q\n' \
    "$attempt" "$shell_probe" "$distribution"
  if [[ "$shell_probe" == "n1a-emu24-connect-$attempt" && "$distribution" == *HarmonyOS* ]]; then
    connected=1
    break
  fi
  sleep 3
done
if (( connected != 1 )); then
  blocked_env "HDC connectivity to the fixed Emulator did not recover in the bounded short loop (E1-recorded degradation mode); environment-class per the frozen aggregation"
fi
printf 'CONNECTIVITY_VERDICT=pass\n'

ready=0
for boot_attempt in $(seq 1 180); do
  qemu_boot_complete="$(tail -n +"$qemu_start_line" "$QEMU_LOG" | grep -F 'guest os boot completed.' | tail -1 || true)"
  shell_readiness="$(hdc shell "echo n1a-emu24-readiness-$boot_attempt" 2>&1 || true)"
  printf 'BOOT_READINESS attempt=%s qemu.boot=%q shell=%q\n' \
    "$boot_attempt" "$qemu_boot_complete" "$shell_readiness"
  if [[ -n "$qemu_boot_complete" && "$shell_readiness" == "n1a-emu24-readiness-$boot_attempt" ]]; then
    ready=1
    break
  fi
  sleep 1
done
if (( ready != 1 )); then
  blocked_env "Emulator boot readiness did not recover in the bounded short loop; environment-class per the frozen aggregation"
fi
boot_completed="$(hdc shell 'param get boot.completed' 2>&1 | tr -d '\r' || true)"
wms_ready="$(hdc shell 'param get bootevent.wms.ready' 2>&1 | tr -d '\r' || true)"
lockscreen_ready="$(hdc shell 'param get bootevent.lockscreen.ready' 2>&1 | tr -d '\r' || true)"
printf 'READINESS boot.completed=%q wms.ready=%q lockscreen.ready=%q qemu.boot=%q\n' \
  "$boot_completed" "$wms_ready" "$lockscreen_ready" "$qemu_boot_complete"
printf 'READINESS_VERDICT=pass\n'

preexisting_bundle="$(timeout 30 "$HDC" -t "$EMULATOR_TARGET" shell "bm dump -n $BUNDLE" 2>&1 || true)"
printf 'PREINSTALL_BUNDLE_QUERY=%q\n' "$preexisting_bundle"
if [[ "$preexisting_bundle" != *'failed to get information'* ]]; then
  blocked_env "bundle existed before N1a installation (environment state; not a measured N1a outcome)"
fi

# --- 8. install app + test HAPs (N0 mirror) ----------------------------------
hdc shell "rm -rf '$STAGING'" >/dev/null 2>&1 || blocked_env "guest staging rm failed"
hdc shell "mkdir -p '$STAGING'" >/dev/null 2>&1 || blocked_env "guest staging mkdir failed"
timeout 180 "$HDC" -t "$EMULATOR_TARGET" file send "$app_hap" "$STAGING/entry-default-unsigned.hap" </dev/null || blocked_env "app HAP file send failed"
timeout 180 "$HDC" -t "$EMULATOR_TARGET" file send "$test_hap" "$STAGING/entry-ohosTest-unsigned.hap" </dev/null || blocked_env "test HAP file send failed"
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
  blocked_env "dual-HAP installation failed (environment-class per the frozen aggregation)"
fi
printf 'INSTALL_VERDICT=pass\n'

bm_dump="$(timeout 30 "$HDC" -t "$EMULATOR_TARGET" shell "bm dump -n $BUNDLE" | sed -E \
  -e 's/("accessTokenId": )[0-9]+/\1[RUNTIME_ACCESS_TOKEN_ID_REDACTED]/g' \
  -e 's/("accessTokenIdEx": )[0-9]+/\1[RUNTIME_ACCESS_TOKEN_ID_EX_REDACTED]/g')"
printf '%s\n' "$bm_dump"
if [[ "$bm_dump" != *'"appPrivilegeLevel": "normal"'* ||
      "$bm_dump" != *'"isSystemApp": false'* ||
      "$bm_dump" != *'"mainElementName": "EntryAbility"'* ]]; then
  blocked_env "installed package is not an ordinary EntryAbility application (environment-class)"
fi
printf 'NORMAL_ENTRY_ABILITY_AUDIT=pass\n'

# --- 9. clear hilog + aa test (phase A) ----------------------------------------
hilog_buffer_output="$(hdc shell 'hilog -G 16M' 2>&1 | tr -d '\r' || true)"
hilog_buffer_query="$(hdc shell 'hilog -g' 2>&1 | tr -d '\r' || true)"
printf 'HILOG_BUFFER_SET=%q\n' "$hilog_buffer_output"
printf 'HILOG_BUFFER_QUERY=%q\n' "$hilog_buffer_query"
if [[ "$hilog_buffer_query" != *"16.0M"* && "$hilog_buffer_query" != *"16M"* &&
      "$hilog_buffer_query" != *"16777216"* ]]; then
  blocked_env "HiLog buffer did not report the required 16 MiB capacity"
fi
printf 'HILOG_BUFFER_VERDICT=pass\n'
hdc shell 'hilog -r' >/dev/null 2>&1 || blocked_env "hilog clear failed"
# aa RC is NOT a gate by itself here: the deferred check mirrors the N0
# discipline (a PASS marker requires host aa RC=0 and guest code=0).
set +e
aa_output="$(timeout 300 "$HDC" -t "$EMULATOR_TARGET" shell "aa test -b $BUNDLE -m $TEST_MODULE -s unittest $TEST_RUNNER -s timeout 90000" 2>&1 </dev/null)"
aa_rc=$?
set -e
printf 'AA_TEST_RC=%s\n' "$aa_rc"
printf 'AA_TEST_OUTPUT=%q\n' "$aa_output"
printf '%s\n' "$aa_output" >"$AA_TEST_LOG"

# --- 10. directed HiLog capture (phase A) -------------------------------------
: >"$TAG_HILOG"
{
  printf '===== TAG HILOG %s =====\n' "$APP_TAG"
  hdc shell "hilog -x -T $APP_TAG -v year -v zone" 2>&1 || true
  printf '===== TAG HILOG %s =====\n' "$TEST_TAG"
  hdc shell "hilog -x -T $TEST_TAG -v year -v zone" 2>&1 || true
} >>"$TAG_HILOG"
hdc shell 'hilog -x -t app -v year -v zone' >"$APP_HILOG" 2>&1 || true
printf 'TAG_HILOG_LINES=%s\n' "$(wc -l <"$TAG_HILOG")"
printf 'APP_HILOG_LINES=%s\n' "$(wc -l <"$APP_HILOG")"
run sha256sum "$TAG_HILOG" "$APP_HILOG"

# --- 10b. overlay diagnostic extraction (defect #3) ------------------------
# Collect N1A_DIAG lines from all phase-A hilog sources into the overlay-diag
# artifact. Best-effort: always creates the file for manifest binding.
extract_overlay_diag "$OVERLAY_DIAG" "$TAG_HILOG" "$APP_HILOG" "$AA_TEST_LOG"

# --- 11. phase-A judgment (frozen) ---------------------------------------------
mapfile -t distinct_app_markers < <(collect_distinct_markers "$MARKER_PREFIX" "$TAG_HILOG" "$AA_TEST_LOG" "$APP_HILOG")
printf 'MARKER_DISTINCT_COUNT=%s\n' "${#distinct_app_markers[@]}"
mapfile -t distinct_test_markers < <(collect_distinct_markers "$TEST_MARKER_PREFIX" "$TAG_HILOG" "$AA_TEST_LOG" "$APP_HILOG")
printf 'TEST_MARKER_DISTINCT_COUNT=%s\n' "${#distinct_test_markers[@]}"

guest_code="$(extract_guest_code "$AA_TEST_LOG")"
printf 'GUEST_RESULT_CODE=%s\n' "$guest_code"

phase_a_verdict=""
phase_a_marker=""
phase_a_c5=""
phase_a_throughput=""

if (( ${#distinct_app_markers[@]} == 0 )); then
  # Frozen C9: a missing marker is a measured fail (never blocked).
  printf 'N1A_RESULT_LINE=<missing>\n'
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "N1A_RESULT missing from all phase-A HiLog sources (tag/aa-test/app); frozen C9 classifies a missing marker as fail"
elif (( ${#distinct_app_markers[@]} > 1 )); then
  for d in "${distinct_app_markers[@]}"; do
    printf 'MARKER_DUPLICATE_LINE=%s\n' "$d"
  done
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "N1A_RESULT emitted more than one distinct marker in the phase-A window; frozen C9 duplicate-marker rule"
else
  marker_line="${distinct_app_markers[0]}"
  phase_a_marker="$marker_line"

  # Defect 2: with exactly one app marker the probe ran to completion, so
  # the chunked N1A_JSON transport must reassemble byte-exactly. A transport
  # failure here is a runner DEFECT (never a measured fail): the probe's own
  # verdict is judged from the marker below.
  probe_detail_sha=""
  if ! probe_detail_sha="$(reassemble_probe_json "$PROBE_DETAIL" \
      "$TAG_HILOG" "$APP_HILOG" "$AA_TEST_LOG")"; then
    defect "probe detail JSON reassembly failed (chunked N1A_JSON transport): $PROBE_DETAIL"
  fi
  printf 'PROBE_DETAIL_JSON_SHA256=%s\n' "$probe_detail_sha"
  printf 'PROBE_DETAIL_JSON_BYTES=%s\n' "$(stat -c %s "$PROBE_DETAIL")"
  printf 'probe_detail_json_sha256=%s\n' "$probe_detail_sha" >>"$MANIFEST"
  printf 'probe_detail_json_bytes=%s\n' "$(stat -c %s "$PROBE_DETAIL")" >>"$MANIFEST"
  printf 'probe_detail_json_sha16=%s\n' "${probe_detail_sha:0:16}" >>"$MANIFEST"
  # Overlay diagnostic artifact hash (defect #3): always bound, even if empty.
  if [[ -n "$OVERLAY_DIAG" && -f "$OVERLAY_DIAG" ]]; then
    printf 'overlay_diag_sha256=%s\n' "$(sha256sum "$OVERLAY_DIAG" | awk '{print $1}')" >>"$MANIFEST"
      printf 'overlay_diag_lines=%s\n' "$(wc -l <"$OVERLAY_DIAG")" >>"$MANIFEST"
  else
    printf 'overlay_diag_sha256=absent\n' >>"$MANIFEST"
  fi

  printf 'N1A_RESULT_LINE=%s\n' "$marker_line"
  phase_a_verdict="$(marker_field "$marker_line" verdict)"
  phase_a_c5="$(marker_field "$marker_line" c5)"
  phase_a_throughput="$(marker_field "$marker_line" throughput_mibps)"
  printf 'PHASE_A_VERDICT_FIELD=%s C5_FIELD=%s THROUGHPUT_FIELD=%s\n' \
    "$phase_a_verdict" "$phase_a_c5" "$phase_a_throughput"
  case "$(judge_app_marker "$marker_line")" in
    pass) phase_a_verdict="pass" ;;
    fail)
      printf 'MEASURED_VERDICT=fail\n'
      measured_fail "unexpected N1A_RESULT marker (frozen table violation): $marker_line"
      ;;
  esac
fi

# ohosTest cross-check: exactly one distinct N1A_PROBE_TEST_RESULT whose
# verdict must agree with the app marker.
if (( ${#distinct_test_markers[@]} != 1 )); then
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "expected exactly one distinct $TEST_MARKER_PREFIX marker in the phase-A window, got ${#distinct_test_markers[@]}"
fi
test_marker_line="${distinct_test_markers[0]}"
printf 'N1A_PROBE_TEST_RESULT_LINE=%s\n' "$test_marker_line"
test_verdict="$(judge_test_marker "$test_marker_line")"
printf 'OHOS_TEST_VERDICT_FIELD=%s\n' "$test_verdict"
if [[ "$test_verdict" != "PASS" ]]; then
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "ohosTest cross-check marker is not verdict=PASS: $test_marker_line"
fi
if [[ "$phase_a_verdict" == "pass" ]] && (( aa_rc != 0 )); then
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "marker PASS but host aa test exited non-zero (rc=$aa_rc)"
fi
if [[ "$phase_a_verdict" == "pass" ]] && [[ "$guest_code" != "0" ]]; then
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "guest test result code is not 0: ${guest_code:-<missing>}"
fi
printf 'PHASE_A_JUDGMENT=pass\n'

# Phase-A screenshot (post-aa-test screen state; recorded, YAVG-checked, and
# kept as phase-A raw; NOT the C9 page evidence — the C9 page clause is
# phase B below).
screen_output="$(hdc shell 'uitest screenCap' 2>&1 | tr -d '\r' || true)"
printf 'PHASE_A_SCREEN_OUTPUT=%q\n' "$screen_output"
remote_screen="$(printf '%s\n' "$screen_output" | sed -n 's/^ScreenCap saved to //p' | tail -1)"
if [[ -n "$remote_screen" ]]; then
  if timeout 30 "$HDC" -t "$EMULATOR_TARGET" file recv "$remote_screen" "$SCREENSHOT_A" </dev/null && [[ -s "$SCREENSHOT_A" ]]; then
    hdc shell "rm -f '$remote_screen'" >/dev/null 2>&1 || true
    run file "$SCREENSHOT_A"
    run sha256sum "$SCREENSHOT_A"
    printf 'PHASE_A_SCREENSHOT=collected\n'
  else
    rm -f "$SCREENSHOT_A"
    printf 'PHASE_A_SCREENSHOT=skipped-capture-failed\n'
  fi
else
  printf 'PHASE_A_SCREENSHOT=skipped-no-capture\n'
fi

# --- 12. post-measurement manifest rows (APPENDED to the base manifest) -------
# Defect 1: the base manifest already exists (written before the device
# phase); this block now only appends the phase results and the hashes of
# the artifacts measured so far.
ended_at="$(date --iso-8601=seconds)"
{
  printf 'phase_a_ended_at=%s\n' "$ended_at"
  printf 'phase_a_marker=%s\n' "$phase_a_marker"
  printf 'phase_a_verdict=%s\n' "$phase_a_verdict"
  printf 'phase_a_c5=%s\n' "$phase_a_c5"
  printf 'phase_a_throughput_mibps=%s\n' "$phase_a_throughput"
  printf 'phase_a_ohos_test_marker=%s\n' "$test_marker_line"
  printf 'phase_a_aa_rc=%s\n' "$aa_rc"
  printf 'phase_a_guest_result_code=%s\n' "$guest_code"
  printf 'app_hap_sha256=%s\n' "$(sha256sum "$app_hap" | awk '{print $1}')"
  printf 'test_hap_sha256=%s\n' "$(sha256sum "$test_hap" | awk '{print $1}')"
  printf 'app_member_sha256=%s\n' "$app_member_sha"
  printf 'test_member_sha256=%s\n' "$test_member_sha"
  printf 'source_manifest_sha256=%s\n' "$(sha256sum "$SOURCE_MANIFEST" | awk '{print $1}')"
  printf 'n1a_build_log_sha256=%s\n' "$(sha256sum "$N1A_BUILD_LOG" | awk '{print $1}')"
  printf 'snapshot_log_sha256=%s\n' "$(sha256sum "$SNAPSHOT_LOG" | awk '{print $1}')"
  printf 'build_log_sha256=%s\n' "$(sha256sum "$BUILD_LOG" | awk '{print $1}')"
  printf 'aa_test_log_sha256=%s\n' "$(sha256sum "$AA_TEST_LOG" | awk '{print $1}')"
  # aa_start_log_sha256 is now bound at seal time in seal_and_finalize
  # (record-review minor-A removed the phase-A-time pending placeholder)
  printf 'tag_hilog_sha256=%s\n' "$(sha256sum "$TAG_HILOG" | awk '{print $1}')"
  printf 'app_hilog_sha256=%s\n' "$(sha256sum "$APP_HILOG" | awk '{print $1}')"
  printf 'page_hilog_sha256=%s\n' "$(sha256sum "$PAGE_HILOG" | awk '{print $1}' 2>/dev/null || printf pending)"
} >>"$MANIFEST"
printf 'PHASE_A_ENDED_AT=%s\n' "$ended_at"
printf 'RECORD_STATUS=collected\n'

# ============================================================================
# Phase B: C9 visible result page (frozen C9 page clause)
# ============================================================================

# Clear HiLog so the phase-B window contains ONLY the page-run markers (the
# documented per-window scoping of the duplicate-marker rule).
hdc shell 'hilog -r' >/dev/null 2>&1 || blocked_env "phase-B hilog clear failed"

# Interpretation-ruling qualifier 3: phase B must be an INDEPENDENT ordinary
# UIAbility process - force-stop the bundle first so no TestRunner/ability
# residue survives, then aa start. A TestRunner-hosted page never satisfies
# the C1 main clause.
hdc shell "aa force-stop $BUNDLE" >/dev/null 2>&1 </dev/null || true
printf 'PHASE_B_FORCE_STOP=done\n'

# bfreeze early-fail-path absent binding (record-review new-minor-C:
# early phase-B failures like aa-start semantic failure never reach the
# post-poll binding; always emit the key so the manifest is complete)
if [[ -n "$BFREEZE_LOG" && -f "$BFREEZE_LOG" ]]; then
  printf 'bfreeze_log_sha256=%s\n' "$(sha256sum "$BFREEZE_LOG" | awk '{print $1}')" >>"$MANIFEST"
else
  printf 'bfreeze_log_sha256=absent\n' >>"$MANIFEST"
fi

set +e
aa_start_output="$(timeout 60 "$HDC" -t "$EMULATOR_TARGET" shell "aa start -a $ABILITY -b $BUNDLE -m $MODULE" 2>&1 </dev/null)"
aa_start_rc=$?
set -e
app_started=1
printf 'AA_START_RC=%s\n' "$aa_start_rc"
printf 'AA_START_OUTPUT=%q\n' "$aa_start_output"
printf '%s\n' "$aa_start_output" >"$AA_START_LOG"
if [[ "$aa_start_output" != *"start ability successfully"* ]]; then
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "aa start did not report success for the phase-B result page (semantic failure), rc=$aa_start_rc output=$aa_start_output"
fi

# Bounded poll for the phase-B N1A_RESULT marker (the page probe runs
# synchronously and takes several seconds: handshake + pump + tick gaps +
# backpressure). Bounded short loop: non-recovery is environment-class
# (frozen aggregation), a malformed/duplicate/FAIL marker is measured fail.
page_marker_found=""
# 0008 ruling prerequisite #4: bounded AppFreeze/jank sampling during the
# phase-B poll loop — every 3rd poll (~9s) captures the core hilog buffer
# (AppFreeze/ANR/jank traces) into the bfreeze evidence file. Bounded by
# the same 60-attempt poll limit (180s); the file is manifest-bound.
: >"$BFREEZE_LOG" 2>/dev/null || true
for page_attempt in $(seq 1 60); do
  hdc shell "hilog -x -T $APP_TAG -v year -v zone" >"$PAGE_HILOG" 2>&1 || true
  if grep -qF "$MARKER_PREFIX" "$PAGE_HILOG"; then
    page_marker_found="yes"
    printf 'PAGE_MARKER_ATTEMPT=%s\n' "$page_attempt"
    break
  fi
  if (( page_attempt % 3 == 0 )) && [[ -n "$BFREEZE_LOG" ]]; then
    printf '=== BFREEZE SAMPLE attempt=%s ===\n' "$page_attempt" >>"$BFREEZE_LOG"
    timeout 10 "$HDC" -t "$EMULATOR_TARGET" shell 'hilog -x -t core -v year -v zone' \
      >>"$BFREEZE_LOG" 2>&1 || true
  fi
  printf 'PAGE_MARKER_POLL attempt=%s no-marker-yet\n' "$page_attempt"
  sleep 3
done

# bfreeze manifest binding at poll completion (moved from the phase-A
# block; the early-fail binding above may have already written the key)
if grep -q '^bfreeze_log_sha256=' "$MANIFEST" 2>/dev/null; then
  : # already bound by the early-fail path; do not duplicate
elif [[ -n "$BFREEZE_LOG" && -f "$BFREEZE_LOG" ]]; then
  printf 'bfreeze_log_sha256=%s\n' "$(sha256sum "$BFREEZE_LOG" | awk '{print $1}')" >>"$MANIFEST"
  printf 'bfreeze_log_lines=%s\n' "$(wc -l <"$BFREEZE_LOG")" >>"$MANIFEST"
else
  printf 'bfreeze_log_sha256=absent\n' >>"$MANIFEST"
fi
if [[ "$page_marker_found" != "yes" ]]; then
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "phase-B page window never produced $MARKER_PREFIX in the bounded poll; frozen C9 missing-marker rule (page clause)"
fi
# Small settle delay so the page has rendered the verdict before the shot.
sleep 3
hdc shell "hilog -x -T $APP_TAG -v year -v zone" >"$PAGE_HILOG" 2>&1 || true
hdc shell "hilog -x -T $TEST_TAG -v year -v zone" >>"$PAGE_HILOG" 2>&1 || true
printf 'PAGE_HILOG_LINES=%s\n' "$(wc -l <"$PAGE_HILOG")"
run sha256sum "$PAGE_HILOG"

# Phase-B diagnostic extraction (defect #3): append any phase-B N1A_DIAG
# lines to the overlay-diag artifact (phase-A lines are already there).
if [[ -n "$OVERLAY_DIAG" && -f "$OVERLAY_DIAG" ]]; then
  grep -F 'N1A_DIAG' "$PAGE_HILOG" 2>/dev/null >>"$OVERLAY_DIAG" || true
  sort -u "$OVERLAY_DIAG" -o "$OVERLAY_DIAG" 2>/dev/null || true
fi

mapfile -t distinct_page_markers < <(collect_distinct_markers "$MARKER_PREFIX" "$PAGE_HILOG")
printf 'PAGE_MARKER_DISTINCT_COUNT=%s\n' "${#distinct_page_markers[@]}"
if (( ${#distinct_page_markers[@]} != 1 )); then
  for d in "${distinct_page_markers[@]}"; do
    printf 'PAGE_MARKER_LINE=%s\n' "$d"
  done
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "expected exactly one distinct $MARKER_PREFIX in the phase-B window, got ${#distinct_page_markers[@]}; frozen C9 page clause"
fi
page_marker_line="${distinct_page_markers[0]}"
printf 'PAGE_RESULT_LINE=%s\n' "$page_marker_line"
case "$(judge_app_marker "$page_marker_line")" in
  pass) ;;
  *)
    printf 'MEASURED_VERDICT=fail\n'
    measured_fail "phase-B page marker violates the frozen table: $page_marker_line"
    ;;
esac
page_c5="$(marker_field "$page_marker_line" c5)"
page_throughput="$(marker_field "$page_marker_line" throughput_mibps)"
printf 'PHASE_B_C5_FIELD=%s THROUGHPUT_FIELD=%s\n' "$page_c5" "$page_throughput"

# Cross-phase consistency: both executions must agree on the verdict (both
# PASS) and both c5 values must be in the allowed set. A c5 difference
# between the two executions (e.g. induced vs not-triggered) is recorded and
# does not fail the campaign by itself; the frozen text pins the per-marker
# c5 enumeration, not cross-phase equality.
printf 'CROSS_PHASE_VERDICT_CONSISTENT=yes\n'
printf 'CROSS_PHASE_C5_PHASE_A=%s PHASE_B=%s\n' "$phase_a_c5" "$page_c5"

# C9 screenshot: non-empty PNG with ffmpeg YAVG > 32.0 (E2 precedent).
screen_output="$(hdc shell 'uitest screenCap' 2>&1 | tr -d '\r' || true)"
printf 'PHASE_B_SCREEN_OUTPUT=%q\n' "$screen_output"
remote_screen="$(printf '%s\n' "$screen_output" | sed -n 's/^ScreenCap saved to //p' | tail -1)"
if [[ -z "$remote_screen" ]]; then
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "phase-B screenshot capture did not report a saved file; frozen C9 requires a non-empty page screenshot"
fi
if ! timeout 30 "$HDC" -t "$EMULATOR_TARGET" file recv "$remote_screen" "$SCREENSHOT_B" </dev/null || [[ ! -s "$SCREENSHOT_B" ]]; then
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "phase-B screenshot could not be received or is empty; frozen C9 non-empty-page clause"
fi
hdc shell "rm -f '$remote_screen'" >/dev/null 2>&1 || true
run file "$SCREENSHOT_B"
run sha256sum "$SCREENSHOT_B"
if ! screenshot_nonblack "$SCREENSHOT_B"; then
  printf 'MEASURED_VERDICT=fail\n'
  measured_fail "phase-B screenshot is black or unreadable (YAVG <= 32.0 or not a PNG); frozen C9 non-black-page clause"
fi
printf 'C9_PAGE_SCREENSHOT_YAVG=%s\n' "$(screenshot_yavg "$SCREENSHOT_B")"
# Page/marker consistency: by construction the page renders the verdict from
# the same probe result object that emitted the phase-B marker (E2
# precedent); the machine evidence is the single PASS marker in the phase-B
# window plus the non-black page screenshot.
printf 'C9_PAGE_MARKER_CONSISTENCY=pass_by_construction_single_pass_marker_plus_nonblack_page\n'
printf 'C9_PAGE_CLAUSE=pass\n'

# ============================================================================
# Final verdict + cleanup
# ============================================================================
result="pass"
ended_at="$(date --iso-8601=seconds)"
printf 'ENDED_AT=%s\n' "$ended_at"
printf 'RECORD_STATUS=collected\n'
printf 'VERDICT=pass\n'

hdc shell "aa force-stop $BUNDLE" >/dev/null 2>&1 || true
app_started=0
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
  blocked_env "residual Emulator, HDC, or tested port after cleanup (environment-class per the frozen aggregation)"
fi
printf 'FINAL_RESIDUAL_PROCESS=false\n'
printf 'FINAL_RESIDUAL_PORT=false\n'
printf 'final_residual_process=false\n' >>"$MANIFEST"
printf 'final_residual_port=false\n' >>"$MANIFEST"
printf 'phase_b_marker=%s\n' "$page_marker_line" >>"$MANIFEST"
printf 'phase_b_verdict=pass\n' >>"$MANIFEST"
printf 'phase_b_c5=%s\n' "$page_c5" >>"$MANIFEST"
printf 'phase_b_throughput_mibps=%s\n' "$page_throughput" >>"$MANIFEST"
printf 'phase_b_page_hilog_sha256=%s\n' "$(sha256sum "$PAGE_HILOG" | awk '{print $1}')" >>"$MANIFEST"
printf 'phase_b_screenshot_sha256=%s\n' "$(sha256sum "$SCREENSHOT_B" | awk '{print $1}')" >>"$MANIFEST"
printf 'phase_a_screenshot_sha256=%s\n' "$(sha256sum "$SCREENSHOT_A" 2>/dev/null | awk '{print $1}' || printf skipped)" >>"$MANIFEST"
printf 'ended_at=%s\n' "$ended_at" >>"$MANIFEST"

# --- sensitive material scan (fail-closed) -------------------------------------
set +e
sensitive_output="$(rg -n -i -e 'authorization:[[:space:]]*bearer[[:space:]]+[A-Za-z0-9._~-]+' \
    -e '-----BEGIN ([A-Z ]+ )?PRIVATE KEY-----' -e 'setup[_ -]?key[=:][[:space:]]*[A-Za-z0-9._~-]{8,}' \
    "$TRANSCRIPT" "$TAG_HILOG" "$APP_HILOG" "$PAGE_HILOG" "$AA_TEST_LOG" "$AA_START_LOG" \
    "$BUILD_LOG" "$N1A_BUILD_LOG" "$SNAPSHOT_LOG" "$CONSOLE" "$SOURCE_MANIFEST" 2>&1)"
sensitive_rc=$?
set -e
if (( sensitive_rc != 1 )); then
  if (( sensitive_rc == 0 )); then
    printf '%s\n' "$sensitive_output"
    printf 'SENSITIVE_SCAN=fail\n'
    printf 'sensitive_scan=fail_high_confidence_pattern_detected\n' >>"$MANIFEST"
    result="fail"
    fail_reason="high-confidence sensitive material pattern detected"
    defect "high-confidence sensitive material pattern detected"
  fi
  defect "rg sensitive material scan failed (rc=$sensitive_rc)"
fi
printf 'SENSITIVE_SCAN=pass_high_confidence_patterns\n'
printf 'sensitive_scan=pass_high_confidence_patterns\n' >>"$MANIFEST"

printf 'N1A_CAMPAIGN_RESULT mode=formal verdict=pass fail_reason=None\n'
exit 0