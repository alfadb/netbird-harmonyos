#!/usr/bin/env bash
set -Eeuo pipefail

# ============================================================================
# e1-stock-go-replay.sh
#
# E1 stock Go loader/runtime replay for the NetBird v0.76.3 baseline on the
# fixed API 24 x86_64 phone Emulator target 127.0.0.1:10000.
#
# This is a resumable entry point. On a host without the same-tuple
# Emulator/Go/ffmpeg/SSH worker it must be run with --preflight, which only
# verifies host paths, versions, the 34d5125 runGoProbe source snapshot and
# the current repository state, then records a host-preflight blocked
# evidence. On the historical Linux worker the same command performs the full
# replay: stock Go 1.25.12 libgoprobe.so build, ELF verification, snapshot
# HAP build, dual-HAP install, aa test, directed HiLog capture, judgment and
# cleanup.
#
# Usage:
#   bash spikes/r1-api24-hap/e1-stock-go-replay.sh [--preflight|--selftest]
#
# Environment (all optional; defaults target the historical Linux worker):
#   WORKSPACE, EVIDENCE_ROOT, EVIDENCE_ID, STABLE_TOOLS, BETA_TOOLS, GO_BIN,
#   EMULATOR_INSTANCE, EMULATOR_HDC_PORT, GO_TOOLCHAIN_MODE, NETBIRD_SOURCE_DIR,
#   GH_BIN, EMULATOR_INSTANCE_PATH, EMULATOR_IMAGE_ROOT, EMULATOR_DISPLAY,
#   EMULATOR_XAUTHORITY, EMULATOR_XDG_RUNTIME_DIR, CONNECT_HELPER, STOP_HELPER,
#   QEMU_LOG
#
# Forbidden inputs: PHYS_1_TARGET and any non-127.0.0.1:10000 emulator target.
# ============================================================================

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_DIR="$SCRIPT_DIR"
readonly DEFAULT_WORKSPACE="$(cd -- "$PROJECT_DIR/../.." && pwd -P)"

# --- fixed baseline constants ----------------------------------------------
readonly BASELINE_TAG="v0.76.3"
readonly BASELINE_COMMIT="f65f7b347ee4e7de6d98c488d3d894cd018b02b6"
readonly BASELINE_GO_DIRECTIVE="go 1.25.5"
readonly BASELINE_TOOLCHAIN_DIRECTIVE="toolchain go1.25.12"
readonly SNAPSHOT_COMMIT="34d512541ca8047f8e3796abd6d85ef94cc13559"
readonly GO_VERSION_PREFIX="go version go1.25.12 "
readonly TARGET_TUPLE="HarmonyOS_6.1.1(24),API24,x86_64,phone_Emulator"
# Capture any external EMULATOR_TARGET request BEFORE the fixed constant is
# assigned: the readonly constant below would otherwise silently clobber the
# environment value and the guard could never detect a non-fixed request.
readonly EMULATOR_TARGET_REQUEST="${EMULATOR_TARGET:-}"
readonly EMULATOR_TARGET="127.0.0.1:10000"
readonly BUNDLE="cn.alfadb.netbird.r1probe"
readonly TEST_MODULE="entry_test"
readonly TEST_RUNNER="/ets/testrunner/OpenHarmonyTestRunner"
readonly HOST_PREFLIGHT_EVIDENCE_ID="EV-E1-EMU24HOST-20260809-0001"
readonly DEFAULT_EVIDENCE_ID="EV-E1-EMU24-20260809-0003"
readonly EXPECTED_LOADER_REJECTION="initial-exec TLS resolves to dynamic definition"

# --- parameterized environment ----------------------------------------------
WORKSPACE="${WORKSPACE:-$DEFAULT_WORKSPACE}"
EVIDENCE_ROOT="${EVIDENCE_ROOT:-$WORKSPACE/docs/evidence/raw}"
EVIDENCE_ID="${EVIDENCE_ID:-$DEFAULT_EVIDENCE_ID}"
STABLE_TOOLS="${STABLE_TOOLS:-/home/worker/harmonyos/command-line-tools/6.1.1.290}"
BETA_TOOLS="${BETA_TOOLS:-/home/worker/harmonyos/command-line-tools/26.0.0.461}"
GO_BIN="${GO_BIN:-/home/worker/go/bin/go}"
EMULATOR_INSTANCE="${EMULATOR_INSTANCE:-netbird_api24_phone}"
EMULATOR_HDC_PORT="${EMULATOR_HDC_PORT:-10000}"
GO_TOOLCHAIN_MODE="${GO_TOOLCHAIN_MODE:-local}"
NETBIRD_SOURCE_DIR="${NETBIRD_SOURCE_DIR:-}"
GH_BIN="${GH_BIN:-gh}"
EMULATOR_INSTANCE_PATH="${EMULATOR_INSTANCE_PATH:-/home/worker/harmonyos/emulator-instances}"
EMULATOR_IMAGE_ROOT="${EMULATOR_IMAGE_ROOT:-/home/worker/harmonyos/emulator-images}"
EMULATOR_DISPLAY="${EMULATOR_DISPLAY:-:1}"
EMULATOR_XAUTHORITY="${EMULATOR_XAUTHORITY:-/home/worker/.Xauthority}"
EMULATOR_XDG_RUNTIME_DIR="${EMULATOR_XDG_RUNTIME_DIR:-/tmp/runtime-worker}"
CONNECT_HELPER="${CONNECT_HELPER:-/home/worker/harmonyos/bin/emulator-connect}"
STOP_HELPER="${STOP_HELPER:-/home/worker/harmonyos/bin/emulator-stop}"
QEMU_LOG="${QEMU_LOG:-$EMULATOR_INSTANCE_PATH/$EMULATOR_INSTANCE/Log/qemu.log}"

# --- derived paths ----------------------------------------------------------
readonly HVIGOR="$STABLE_TOOLS/bin/hvigorw"
readonly OHPM="$STABLE_TOOLS/bin/ohpm"
readonly SDK="$BETA_TOOLS/sdk/default/openharmony"
readonly HDC="$SDK/toolchains/hdc"
readonly EMULATOR="$BETA_TOOLS/emulator/Emulator"
readonly NATIVE_HOME="$STABLE_TOOLS/sdk/default/openharmony/native"
readonly GO_PROBE_DIR="$PROJECT_DIR/go-probe"
readonly GO_PROBE_OUTPUT_DIR="$PROJECT_DIR/entry/libs/x86_64"
readonly GO_SO="$GO_PROBE_OUTPUT_DIR/libgoprobe.so"
readonly APP_HAP_REL="entry/build/default/outputs/default/entry-default-unsigned.hap"
readonly TEST_HAP_REL="entry/build/default/outputs/ohosTest/entry-ohosTest-unsigned.hap"
readonly STAGING="/data/local/tmp/e1-stock-go-20260809"

# --- runtime state ----------------------------------------------------------
PREFLIGHT=0
SELFTEST=0
case "${1:-}" in
  --preflight) PREFLIGHT=1 ;;
  --selftest) SELFTEST=1 ;;
  "") ;;
  *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
esac

result="blocked"
emulator_started=0
installed=0
snapshot_dir=""
app_member=""
test_member=""
started_at=""
ended_at=""

TRANSCRIPT=""
TAG_HILOG=""
APP_HILOG=""
MANIFEST=""
CONSOLE=""
BUILD_LOG=""
GO_BUILD_LOG=""
BASELINE_VERIFY_LOG=""
AA_TEST_LOG=""

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
  timeout 30 "$HDC" -t "$EMULATOR_TARGET" "$@"
}

fail() {
  result="fail"
  printf 'FAIL_REASON=%s\n' "$1"
  exit 1
}

# Reliable EXIT teardown: any fail/interrupt after startup/install first
# deletes the guest staging directory while the Emulator is still online, then
# uninstalls, stops the Emulator, kills host hdc and records cleanup. It never
# touches anything outside the fixed Emulator target.
teardown() {
  if (( SELFTEST == 1 )); then
    return 0
  fi
  printf 'CLEANUP_BEGIN=teardown\n'
  # 1. While the Emulator is still online, delete the guest staging directory
  #    first: it lives on /data/local/tmp and is unreachable after stop.
  if (( PREFLIGHT == 1 )) || [[ ! -x "$HDC" ]]; then
    printf 'CLEANUP_STAGING=skipped-no-hdc\n'
  elif (( emulator_started == 1 )); then
    hdc shell "rm -rf $STAGING" >/dev/null 2>&1 || true
    printf 'CLEANUP_STAGING=cleared\n'
  else
    printf 'CLEANUP_STAGING=skipped-emulator-not-started\n'
  fi
  # 2. Uninstall the bundle while the Emulator is still online.
  if (( installed == 1 )); then
    timeout 120 "$HDC" -t "$EMULATOR_TARGET" uninstall "$BUNDLE" >/dev/null 2>&1 || true
    installed=0
    printf 'CLEANUP_UNINSTALL=done\n'
  fi
  # 3. Stop the Emulator.
  if (( emulator_started == 1 )); then
    HDC_PORT="$EMULATOR_HDC_PORT" timeout 60 "$STOP_HELPER" >/dev/null 2>&1 || true
    for _ in $(seq 1 30); do
      if ! pgrep -f '/emulator/Emulator.*-start '"$EMULATOR_INSTANCE"'|qemu-system.*'"$EMULATOR_INSTANCE" >/dev/null 2>&1; then
        break
      fi
      sleep 1
    done
    emulator_started=0
    printf 'CLEANUP_EMULATOR=stopped\n'
  fi
  # 4. Kill the host hdc daemon.
  if (( PREFLIGHT == 1 )) || [[ ! -x "$HDC" ]]; then
    printf 'CLEANUP_HDC=skipped-no-hdc\n'
  else
    "$HDC" kill >/dev/null 2>&1 || true
    printf 'CLEANUP_HDC=kill-issued\n'
  fi
  rm -rf "$snapshot_dir" "$app_member" "$test_member" 2>/dev/null || true
  printf 'CLEANUP_TEMP=removed\n'
  printf 'CLEANUP_END=teardown-complete\n'
  seal_and_finalize
  return 0
}
trap teardown EXIT

# Close the transcript log stream and wait for tee so the transcript file is
# final before any seal hash is computed.
seal_transcript() {
  exec 1>&3 2>&3
  if [[ -n "${TEE_PID:-}" ]]; then
    wait "$TEE_PID" 2>/dev/null || true
  fi
}

# Called at the end of teardown, after all teardown output has been flushed
# into the transcript: seal the stream, then append the final transcript hash
# (and a self-hash of the manifest) so no mid-stream hash is ever presented
# as final and the teardown log is inside the sealed hash.
seal_and_finalize() {
  if [[ -z "${TEE_PID:-}" ]]; then
    return 0
  fi
  seal_transcript
  if [[ -n "$MANIFEST" && -f "$MANIFEST" ]]; then
    local transcript_hash manifest_hash
    transcript_hash="$(sha256sum "$TRANSCRIPT" | awk '{print $1}')" || true
    printf 'transcript_final_sha256=%s\n' "$transcript_hash" >>"$MANIFEST"
    manifest_hash="$(sha256sum "$MANIFEST" | awk '{print $1}')" || true
    printf 'manifest_sha256=%s\n' "$manifest_hash" >>"$MANIFEST"
  fi
}

# Refuse to overwrite existing fixed evidence (no-clobber).
check_no_clobber() {
  local f
  for f in "$TRANSCRIPT" "$TAG_HILOG" "$APP_HILOG" "$MANIFEST" "$CONSOLE" "$BUILD_LOG" "$GO_BUILD_LOG" "$BASELINE_VERIFY_LOG" "$AA_TEST_LOG"; do
    if [[ -n "$f" && -e "$f" ]]; then
      printf 'REFUSE_OVERWRITE=evidence file already exists: %s; refusing to overwrite fixed evidence (use a fresh EVIDENCE_ROOT)\n' "$f" >&2
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
  if [[ -n "$request" && "$request" != "$EMULATOR_TARGET" ]]; then
    return 1
  fi
  return 0
}
guard_hdc_port() {
  [[ "$EMULATOR_HDC_PORT" == "10000" ]]
}
guard_emulator_instance() {
  [[ "$EMULATOR_INSTANCE" == "netbird_api24_phone" ]]
}

# --- judgment: classify a GO_SPIKE_RESULT line ------------------------------
# Field-level judgment: split the pipe-separated GO_SPIKE_RESULT line into
# key=value fields and read only the verdict/ok/dlopenLoaded/loaderError
# fields. A phrase appearing anywhere else (e.g. inside detail) must never
# influence the classification.
judge_spike_line() {
  local spike_line="$1"
  local verdict="" ok="" dlopen_loaded="" loader_error=""
  local -a fields
  local field
  IFS='|' read -ra fields <<<"$spike_line"
  for field in "${fields[@]}"; do
    case "$field" in
      detail=*) break ;;  # detail is the trailing field; ignore it and any pipe fragments inside it
      verdict=*) verdict="${field#verdict=}" ;;
      ok=*) ok="${field#ok=}" ;;
      dlopenLoaded=*) dlopen_loaded="${field#dlopenLoaded=}" ;;
      loaderError=*) loader_error="${field#loaderError=}" ;;
    esac
  done
  if [[ "$verdict" == "PASS" && "$ok" == "true" ]]; then
    printf 'pass\n'
  elif [[ "$dlopen_loaded" == "false" && "$loader_error" == *"$EXPECTED_LOADER_REJECTION"* ]]; then
    printf 'blocked\n'
  else
    printf 'fail\n'
  fi
}

# --- ELF program-header PT_TLS detection -----------------------------------
# readelf -lW prints the program-header Type column as "TLS" (never the
# literal "PT_TLS"), so a whole-output grep for "PT_TLS" is a false negative
# on a real TLS-bearing shared object. Only rows between "Program Headers:"
# and "Section to Segment mapping:" are authoritative; section-level ".tbss"
# text below the mapping line must never count. Diagnostic output prints the
# full LOAD/TLS program-header rows; the function returns 0 iff a row's first
# field is exactly "TLS".
pt_tls_diag() {
  local readelf_l="$1"
  local in_headers=0
  local found=1
  local line type
  while IFS= read -r line; do
    if [[ "$line" == "Program Headers:" ]]; then
      in_headers=1
      continue
    fi
    # readelf prints the mapping header with a leading space
    # (" Section to Segment mapping:"), so match it anywhere in the line.
    if [[ "$line" == *"Section to Segment mapping:"* ]]; then
      break
    fi
    if (( in_headers == 1 )); then
      read -r type _ <<<"$line"
      case "$type" in
        LOAD|TLS) printf '%s\n' "$line" ;;
      esac
      if [[ "$type" == "TLS" ]]; then
        found=0
      fi
    fi
  done <<<"$readelf_l"
  return $found
}

# --- selftest: pure host checks, no network/HDC/Emulator, no evidence -------
selftest_run() {
  local rc=0
  local tmpdir
  local spike_blocked spike_pass spike_unknown spike_detail_phrase spike_detail_pass spike_detail_pipe v
  local pt_tls_diag_out
  local pt_tls_positive pt_tls_negative
  local existing newfile
  tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/e1-selftest.XXXXXX")"
  trap 'rm -rf "$tmpdir"' RETURN

  printf 'SELFTEST_BEGIN=e1-stock-go-replay.sh\n'
  printf 'SELFTEST_MODE=pure-host no-network no-hdc no-emulator no-evidence\n'

  spike_blocked='GO_SPIKE_RESULT|verdict=FAIL|ok=false|pid=1234|preCreatedBeforeDlopen=true|dlopenLoaded=false|postCreatedAfterDlopen=false|pre=true|post=false|stage=dlopen|loaderErrno=0|loaderError=initial-exec TLS resolves to dynamic definition|detail=dlopen libgoprobe.so failed: initial-exec TLS resolves to dynamic definition'
  spike_pass='GO_SPIKE_RESULT|verdict=PASS|ok=true|pid=1234|preCreatedBeforeDlopen=true|dlopenLoaded=true|postCreatedAfterDlopen=true|pre=true|post=true|stage=complete|loaderErrno=0|loaderError=|detail=pre-dlopen and post-dlopen threads passed all Go callbacks'
  spike_unknown='GO_SPIKE_RESULT|verdict=DRIFT|ok=false|pid=1234|preCreatedBeforeDlopen=true|dlopenLoaded=false|postCreatedAfterDlopen=false|pre=true|post=false|stage=drift|loaderErrno=0|loaderError=environment drift|detail=environment drift'
  v="$(judge_spike_line "$spike_blocked")"
  if [[ "$v" != "blocked" ]]; then
    printf 'SELFTEST FAIL judge blocked: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_spike_line "$spike_pass")"
  if [[ "$v" != "pass" ]]; then
    printf 'SELFTEST FAIL judge pass: got %s\n' "$v"
    rc=1
  fi
  v="$(judge_spike_line "$spike_unknown")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge unknown: got %s\n' "$v"
    rc=1
  fi
  # counterexample: the frozen rejection phrase appears only inside detail,
  # while the loaderError field is empty -> must be fail, not blocked
  spike_detail_phrase='GO_SPIKE_RESULT|verdict=FAIL|ok=false|pid=1234|preCreatedBeforeDlopen=true|dlopenLoaded=false|postCreatedAfterDlopen=false|pre=true|post=false|stage=dlopen|loaderErrno=0|loaderError=|detail=dlopen libgoprobe.so failed: initial-exec TLS resolves to dynamic definition'
  v="$(judge_spike_line "$spike_detail_phrase")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge detail phrase: got %s\n' "$v"
    rc=1
  fi
  # counterexample: verdict=PASS/ok=true appear only inside detail, while the
  # real verdict field is FAIL -> must be fail, not pass
  spike_detail_pass='GO_SPIKE_RESULT|verdict=FAIL|ok=false|pid=1234|preCreatedBeforeDlopen=true|dlopenLoaded=false|postCreatedAfterDlopen=false|pre=true|post=false|stage=dlopen|loaderErrno=0|loaderError=|detail=verdict=PASS ok=true but the real fields are FAIL'
  v="$(judge_spike_line "$spike_detail_pass")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge detail pass phrase: got %s\n' "$v"
    rc=1
  fi
  # counterexample: pipe-separated verdict=PASS/ok=true fragments appear only
  # inside the trailing detail field -> must be fail, not pass
  spike_detail_pipe='GO_SPIKE_RESULT|verdict=FAIL|ok=false|pid=1234|preCreatedBeforeDlopen=true|dlopenLoaded=false|postCreatedAfterDlopen=false|pre=true|post=false|stage=dlopen|loaderErrno=0|loaderError=|detail=inner log|verdict=PASS|ok=true'
  v="$(judge_spike_line "$spike_detail_pipe")"
  if [[ "$v" != "fail" ]]; then
    printf 'SELFTEST FAIL judge detail pipe fragments: got %s\n' "$v"
    rc=1
  fi
  printf 'SELFTEST judgment=pass\n'

  # PT_TLS detection: the positive case is a real readelf -lW program-header
  # table with a TLS row; the negative case has only section-level ".tbss"
  # text below the mapping line and must not be misjudged as PT_TLS. Both
  # fixtures carry a fabricated "TLS"-leading line below the mapping header:
  # if the interval cutoff at "Section to Segment mapping:" ever failed to
  # stop the scan, the negative case would be misjudged as PT_TLS and the
  # selftest would fail.
  pt_tls_positive='Elf file type is DYN (Shared object file)
Entry point 0x0
There are 12 program headers, starting at offset 64

Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  PHDR           0x000040 0x0000000000000040 0x0000000000000040 0x0002a0 0x0002a0 R   0x8
  LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x068f04 0x068f04 R   0x1000
  TLS            0x132480 0x0000000000133480 0x0000000000133480 0x000000 0x000008 R   0x8
  DYNAMIC        0x207530 0x0000000000209530 0x0000000000209530 0x000180 0x000180 RW  0x8

 Section to Segment mapping:
  Segment Sections...
   05     .tbss
  TLS            0x999999 0x0000000000999999 0x0000000000999999 0x000000 0x000008 R   0x8
'
  pt_tls_negative='Elf file type is DYN (Shared object file)
Entry point 0x0
There are 12 program headers, starting at offset 64

Program Headers:
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  PHDR           0x000040 0x0000000000000040 0x0000000000000040 0x0002a0 0x0002a0 R   0x8
  LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x068f04 0x068f04 R   0x1000
  DYNAMIC        0x207530 0x0000000000209530 0x0000000000209530 0x000180 0x000180 RW  0x8

 Section to Segment mapping:
  Segment Sections...
   05     .tbss
  TLS            0x999999 0x0000000000999999 0x0000000000999999 0x000000 0x000008 R   0x8
'
  if ! ( pt_tls_diag "$pt_tls_positive" >/dev/null 2>&1 ); then
    printf 'SELFTEST FAIL pt_tls positive\n'
    rc=1
  fi
  if ( pt_tls_diag "$pt_tls_negative" >/dev/null 2>&1 ); then
    printf 'SELFTEST FAIL pt_tls negative\n'
    rc=1
  fi
  pt_tls_diag_out="$(pt_tls_diag "$pt_tls_positive" 2>/dev/null || true)"
  if [[ "$pt_tls_diag_out" != *"LOAD"* || "$pt_tls_diag_out" != *"TLS"* ]]; then
    printf 'SELFTEST FAIL pt_tls positive diag output\n'
    rc=1
  fi
  if [[ "$pt_tls_diag_out" != *"0x0000000000133480"* ]]; then
    printf 'SELFTEST FAIL pt_tls positive diag full TLS row\n'
    rc=1
  fi
  if [[ "$pt_tls_diag_out" == *"0x999999"* ]]; then
    printf 'SELFTEST FAIL pt_tls positive diag below-mapping leak\n'
    rc=1
  fi
  pt_tls_diag_out="$(pt_tls_diag "$pt_tls_negative" 2>/dev/null || true)"
  if [[ "$pt_tls_diag_out" == *"TLS"* ]]; then
    printf 'SELFTEST FAIL pt_tls negative diag output\n'
    rc=1
  fi
  printf 'SELFTEST pt_tls=pass\n'

  if ( PHYS_1_TARGET=1; guard_physical_target ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard PHYS_1_TARGET\n'
    rc=1
  fi
  if ( TARGET=192.168.1.1:10000; guard_emulator_target ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard TARGET\n'
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
  if ( EMULATOR_HDC_PORT=5555; guard_hdc_port ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard EMULATOR_HDC_PORT\n'
    rc=1
  fi
  if ( EMULATOR_INSTANCE=other_instance; guard_emulator_instance ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guard EMULATOR_INSTANCE\n'
    rc=1
  fi
  if ! ( EMULATOR_HDC_PORT=10000; EMULATOR_INSTANCE=netbird_api24_phone; unset TARGET HDC_TARGET PHYS_1_TARGET; \
         guard_physical_target && guard_emulator_target && guard_hdc_port && guard_emulator_instance ) >/dev/null 2>&1; then
    printf 'SELFTEST FAIL guards positive\n'
    rc=1
  fi
  printf 'SELFTEST guards=pass\n'

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

  emulator_started=0
  installed=0
  if ! teardown >/dev/null 2>&1; then
    printf 'SELFTEST FAIL teardown no-op\n'
    rc=1
  fi
  printf 'SELFTEST cleanup=pass\n'

  if (( rc == 0 )); then
    printf 'SELFTEST_RESULT=PASS\n'
  else
    printf 'SELFTEST_RESULT=FAIL\n'
  fi
  return $rc
}

# --- forbidden target guards (before any evidence file is created) ----------
if (( SELFTEST != 1 )); then
  guard_physical_target || fail "PHYS_1_TARGET is set; this runner is Emulator-only and must never target a physical device"
  guard_emulator_target || fail "TARGET/HDC_TARGET/EMULATOR_TARGET is not the fixed $EMULATOR_TARGET Emulator target"
  guard_hdc_port || fail "EMULATOR_HDC_PORT=$EMULATOR_HDC_PORT; only the fixed 10000 Emulator HDC port is allowed"
  guard_emulator_instance || fail "EMULATOR_INSTANCE=$EMULATOR_INSTANCE; only the fixed netbird_api24_phone Emulator instance is allowed"
fi

# --- evidence setup (selftest writes no evidence) ---------------------------
if (( SELFTEST == 1 )); then
  :
else
  mkdir -p "$EVIDENCE_ROOT"
  if (( PREFLIGHT == 1 )); then
    TRANSCRIPT="$EVIDENCE_ROOT/$HOST_PREFLIGHT_EVIDENCE_ID-transcript.log"
  else
    TRANSCRIPT="$EVIDENCE_ROOT/$EVIDENCE_ID-transcript.log"
    TAG_HILOG="$EVIDENCE_ROOT/$EVIDENCE_ID-hilog-tag.log"
    APP_HILOG="$EVIDENCE_ROOT/$EVIDENCE_ID-hilog-app-full.log"
    MANIFEST="$EVIDENCE_ROOT/$EVIDENCE_ID-manifest.txt"
    CONSOLE="$EVIDENCE_ROOT/$EVIDENCE_ID-emulator-console.log"
    BUILD_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-build.log"
    GO_BUILD_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-go-build.log"
    BASELINE_VERIFY_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-baseline-verify.log"
    AA_TEST_LOG="$EVIDENCE_ROOT/$EVIDENCE_ID-aa-test.log"
  fi
  check_no_clobber || exit 2
  exec 3>&1
  exec > >(tee "$TRANSCRIPT") 2>&1
  TEE_PID=$!
fi

# --- selftest mode: run and exit before any evidence/HDC/Emulator work ------
if (( SELFTEST == 1 )); then
  selftest_run
  exit $?
fi

# --- fixed baseline header --------------------------------------------------
printf 'RUNNER=e1-stock-go-replay.sh\n'
printf 'BASELINE_TAG=%s\n' "$BASELINE_TAG"
printf 'BASELINE_COMMIT=%s\n' "$BASELINE_COMMIT"
printf 'BASELINE_GO_DIRECTIVE=%s\n' "$BASELINE_GO_DIRECTIVE"
printf 'BASELINE_TOOLCHAIN_DIRECTIVE=%s\n' "$BASELINE_TOOLCHAIN_DIRECTIVE"
printf 'SNAPSHOT_COMMIT=%s\n' "$SNAPSHOT_COMMIT"
printf 'GO_VERSION_PREFIX=%s\n' "$GO_VERSION_PREFIX"
printf 'TARGET_TUPLE=%s\n' "$TARGET_TUPLE"
printf 'EMULATOR_TARGET=%s\n' "$EMULATOR_TARGET"
printf 'BUNDLE=%s\n' "$BUNDLE"
printf 'TEST_MODULE=%s\n' "$TEST_MODULE"
printf 'TEST_RUNNER=%s\n' "$TEST_RUNNER"
printf 'PHYSICAL_DEVICE_USED=false\n'
if (( PREFLIGHT == 1 )); then
  printf 'HDC_RUN=false\n'
  printf 'EVIDENCE_ID=%s\n' "$HOST_PREFLIGHT_EVIDENCE_ID"
else
  printf 'HDC_RUN=true\n'
  printf 'EVIDENCE_ID=%s\n' "$EVIDENCE_ID"
fi
printf 'WORKSPACE=%s\n' "$WORKSPACE"
printf 'EVIDENCE_ROOT=%s\n' "$EVIDENCE_ROOT"
printf 'STABLE_TOOLS=%s\n' "$STABLE_TOOLS"
printf 'BETA_TOOLS=%s\n' "$BETA_TOOLS"
printf 'GO_BIN=%s\n' "$GO_BIN"
printf 'EMULATOR_INSTANCE=%s\n' "$EMULATOR_INSTANCE"
printf 'GO_TOOLCHAIN_MODE=%s\n' "$GO_TOOLCHAIN_MODE"
printf 'NETBIRD_SOURCE_DIR=%s\n' "${NETBIRD_SOURCE_DIR:-<unset>}"
printf 'GH_BIN=%s\n' "$GH_BIN"

# --- repository state -------------------------------------------------------
if [[ ! -d "$WORKSPACE/.git" ]]; then
  fail "WORKSPACE is not a git repository: $WORKSPACE"
fi
code_sha="$(git -C "$WORKSPACE" rev-parse HEAD)"
printf 'GIT_HEAD=%s\n' "$code_sha"
printf 'GIT_BRANCH=%s\n' "$(git -C "$WORKSPACE" rev-parse --abbrev-ref HEAD)"
if ! git -C "$WORKSPACE" cat-file -e "$SNAPSHOT_COMMIT^{commit}" 2>/dev/null; then
  fail "snapshot commit $SNAPSHOT_COMMIT is not present in the repository"
fi
printf 'SNAPSHOT_COMMIT_PRESENT=pass\n'

# --- snapshot source verification (in git, no extraction) --------------------
verify_snapshot_in_git() {
  local probe_cpp
  probe_cpp="$(git -C "$WORKSPACE" show "$SNAPSHOT_COMMIT:spikes/r1-api24-hap/entry/src/main/cpp/probe.cpp" 2>/dev/null || true)"
  if [[ -z "$probe_cpp" ]]; then
    printf 'SNAPSHOT_SOURCE_VERIFY=fail reason=probe.cpp missing at snapshot\n'
    return 1
  fi
  if [[ "$probe_cpp" != *"runGoProbe"* ]]; then
    printf 'SNAPSHOT_SOURCE_VERIFY=fail reason=runGoProbe missing in snapshot probe.cpp\n'
    return 1
  fi
  if [[ "$probe_cpp" != *"GO_SPIKE_RESULT"* ]]; then
    printf 'SNAPSHOT_SOURCE_VERIFY=fail reason=GO_SPIKE_RESULT missing in snapshot probe.cpp\n'
    return 1
  fi
  local test_runner
  test_runner="$(git -C "$WORKSPACE" show "$SNAPSHOT_COMMIT:spikes/r1-api24-hap/entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets" 2>/dev/null || true)"
  if [[ "$test_runner" != *"runGoProbe"* || "$test_runner" != *"GO_SPIKE_RESULT"* || "$test_runner" != *"BASELINE_RESULT"* ]]; then
    printf 'SNAPSHOT_SOURCE_VERIFY=fail reason=TestRunner snapshot missing runGoProbe/GO_SPIKE_RESULT/BASELINE_RESULT\n'
    return 1
  fi
  if [[ "$test_runner" != *"R1Api24ProbeTest"* ]]; then
    printf 'SNAPSHOT_SOURCE_VERIFY=fail reason=TestRunner snapshot missing R1Api24ProbeTest hilog tag\n'
    return 1
  fi
  local index_dts
  index_dts="$(git -C "$WORKSPACE" show "$SNAPSHOT_COMMIT:spikes/r1-api24-hap/entry/src/main/cpp/types/libprobe/index.d.ts" 2>/dev/null || true)"
  if [[ "$index_dts" != *"runGoProbe"* ]]; then
    printf 'SNAPSHOT_SOURCE_VERIFY=fail reason=index.d.ts snapshot missing runGoProbe\n'
    return 1
  fi
  printf 'SNAPSHOT_SOURCE_VERIFY=pass\n'
  return 0
}
verify_snapshot_in_git || fail "snapshot source verification failed"

# --- baseline verification (gh or injected source; fail closed) -------------
verify_baseline() {
  if [[ -n "$NETBIRD_SOURCE_DIR" ]]; then
    if [[ ! -d "$NETBIRD_SOURCE_DIR/.git" ]]; then
      printf 'BASELINE_VERIFY=fail reason=NETBIRD_SOURCE_DIR is not a git repository\n'
      return 1
    fi
    local head_sha
    head_sha="$(git -C "$NETBIRD_SOURCE_DIR" rev-parse HEAD 2>/dev/null || true)"
    if [[ "$head_sha" != "$BASELINE_COMMIT" ]]; then
      printf 'BASELINE_VERIFY=fail reason=injected source HEAD %s != %s\n' "$head_sha" "$BASELINE_COMMIT"
      return 1
    fi
    if ! grep -Fxq "$BASELINE_GO_DIRECTIVE" "$NETBIRD_SOURCE_DIR/go.mod" 2>/dev/null; then
      printf 'BASELINE_VERIFY=fail reason=injected go.mod missing exact line %s\n' "$BASELINE_GO_DIRECTIVE"
      return 1
    fi
    if ! grep -Fxq "$BASELINE_TOOLCHAIN_DIRECTIVE" "$NETBIRD_SOURCE_DIR/go.mod" 2>/dev/null; then
      printf 'BASELINE_VERIFY=fail reason=injected go.mod missing exact line %s\n' "$BASELINE_TOOLCHAIN_DIRECTIVE"
      return 1
    fi
    printf 'BASELINE_VERIFY=pass mode=injected-source\n'
    return 0
  fi

  if ! command -v "$GH_BIN" >/dev/null 2>&1; then
    printf 'BASELINE_VERIFY=fail reason=gh unavailable and NETBIRD_SOURCE_DIR unset\n'
    return 1
  fi

  local tag_sha commit_sha go_mod_b64 go_mod
  if ! tag_sha="$("$GH_BIN" api "repos/netbirdio/netbird/git/ref/tags/$BASELINE_TAG" --jq '.object.sha' 2>/dev/null)"; then
    printf 'BASELINE_VERIFY=fail reason=gh could not resolve tag %s (network or API failure)\n' "$BASELINE_TAG"
    return 1
  fi
  if commit_sha="$("$GH_BIN" api "repos/netbirdio/netbird/git/tags/$tag_sha" --jq '.object.sha' 2>/dev/null)"; then
    : # annotated tag; commit_sha is the peeled commit
  else
    commit_sha="$tag_sha" # lightweight tag; the ref object is the commit itself
  fi
  if [[ "$commit_sha" != "$BASELINE_COMMIT" ]]; then
    printf 'BASELINE_VERIFY=fail reason=tag %s points to %s, expected %s\n' "$BASELINE_TAG" "$commit_sha" "$BASELINE_COMMIT"
    return 1
  fi
  if ! go_mod_b64="$("$GH_BIN" api "repos/netbirdio/netbird/contents/go.mod?ref=$BASELINE_COMMIT" --jq '.content' 2>/dev/null)"; then
    printf 'BASELINE_VERIFY=fail reason=gh could not fetch go.mod at %s\n' "$BASELINE_COMMIT"
    return 1
  fi
  if ! go_mod="$(printf '%s' "$go_mod_b64" | tr -d '\n' | base64 -d 2>/dev/null)"; then
    printf 'BASELINE_VERIFY=fail reason=go.mod base64 decode failed at %s\n' "$BASELINE_COMMIT"
    return 1
  fi
  if ! grep -Fxq "$BASELINE_GO_DIRECTIVE" <<<"$go_mod"; then
    printf 'BASELINE_VERIFY=fail reason=go.mod at %s missing exact line %s\n' "$BASELINE_COMMIT" "$BASELINE_GO_DIRECTIVE"
    return 1
  fi
  if ! grep -Fxq "$BASELINE_TOOLCHAIN_DIRECTIVE" <<<"$go_mod"; then
    printf 'BASELINE_VERIFY=fail reason=go.mod at %s missing exact line %s\n' "$BASELINE_COMMIT" "$BASELINE_TOOLCHAIN_DIRECTIVE"
    return 1
  fi
  printf 'BASELINE_VERIFY=pass mode=gh tag=%s commit=%s\n' "$BASELINE_TAG" "$commit_sha"
  return 0
}
if (( PREFLIGHT == 1 )); then
  verify_baseline || printf 'BASELINE_VERIFY=blocked (network or injected source unavailable; fail closed)\n'
else
  baseline_output="$(verify_baseline 2>&1 || true)"
  printf '%s\n' "$baseline_output" | tee "$BASELINE_VERIFY_LOG"
  if [[ "$baseline_output" != *"BASELINE_VERIFY=pass"* ]]; then
    fail "baseline verification failed (network unavailable or injected source mismatch)"
  fi
fi

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
check_tool hvigor-ohos-plugin "$STABLE_TOOLS/hvigor/hvigor-ohos-plugin" || true
check_tool emulator "$EMULATOR" || true
check_tool hdc "$HDC" || true
check_tool go "$GO_BIN" || true
check_tool ohos-x86_64-clang "$NATIVE_HOME/llvm/bin/x86_64-unknown-linux-ohos-clang" || true
check_command ffmpeg ffmpeg || true
check_command gh "$GH_BIN" || true
check_command bash bash || true
check_command readelf readelf || true
check_command file file || true
check_command unzip unzip || true
check_command ss ss || true
check_command git git || true
check_command tar tar || true
check_command base64 base64 || true
check_command timeout timeout || true
check_command pgrep pgrep || true
check_command mktemp mktemp || true
check_command sha256sum sha256sum || true
check_command awk awk || true
if command -v shellcheck >/dev/null 2>&1; then
  printf 'HOST_CHECK shellcheck=pass command=shellcheck\n'
else
  printf 'HOST_CHECK shellcheck=fail command=shellcheck (external bash -n required)\n'
fi

# --- preflight mode: stop here, no emulator, no HDC -------------------------
if (( PREFLIGHT == 1 )); then
  started_at="$(date --iso-8601=seconds)"
  printf 'STARTED_AT=%s\n' "$started_at"
  printf 'CLOCK_SOURCE=host_CLOCK_REALTIME_date_iso_8601_seconds\n'
  printf 'TIMEZONE=%s\n' "$(date +%Z%:z)"
  printf 'HOST_PREFLIGHT_EVIDENCE_ID=%s\n' "$HOST_PREFLIGHT_EVIDENCE_ID"
  printf 'EXECUTION=not-run-host-preflight\n'
  printf 'RECORD_STATUS=collected\n'
  printf 'VERDICT=blocked\n'
  printf 'HOST_PREFLIGHT_VERDICT=blocked\n'
  printf 'HOST_PREFLIGHT_REASON=host lacks the same-tuple Emulator/Go/ffmpeg/SSH worker; no runtime verdict is produced\n'
  printf 'HOST_PREFLIGHT_MISSING_COUNT=%s\n' "$host_missing"
  printf 'HOST_PREFLIGHT_NOTE=Windows DevEco Studio Emulator is not equivalent to the Linux worker Emulator\n'
  printf 'HOST_PREFLIGHT_NOTE=old freeze superseded; E3 physical-device HDC prohibition unchanged\n'
  printf 'HOST_PREFLIGHT_NOTE=this evidence ID is independent and does not occupy a future runtime evidence ID\n'
  printf 'HOST_PREFLIGHT_NOTE=no platform conclusion follows from this host-only record\n'
  ended_at="$(date --iso-8601=seconds)"
  printf 'ENDED_AT=%s\n' "$ended_at"
  exit 0
fi

# ============================================================================
# Full replay mode
# ============================================================================
started_at="$(date --iso-8601=seconds)"
printf 'STARTED_AT=%s\n' "$started_at"
printf 'CLOCK_SOURCE=host_CLOCK_REALTIME_date_iso_8601_seconds\n'
printf 'TIMEZONE=%s\n' "$(date +%Z%:z)"
printf 'RECORD_SCOPE=E1_stock_Go_loader_runtime_replay_v0.76.3_baseline_API24_x86_64\n'
printf 'TEST_RUNNER_USED=true\n'
printf 'GO_NETBIRD_PS4_TOUCHED=false\n'

if (( host_missing > 0 )); then
  fail "missing required host tools ($host_missing); run --preflight for a host-only blocked record"
fi

# --- 1. stock Go libgoprobe.so build ----------------------------------------
rm -f "$GO_SO" "$GO_PROBE_OUTPUT_DIR/libgoprobe.h"
if ! (
  cd "$PROJECT_DIR"
  print_command env HARMONYOS_NATIVE_HOME="$NATIVE_HOME" GO_BIN="$GO_BIN" \
    GO_PROBE_OUTPUT_DIR="$GO_PROBE_OUTPUT_DIR" GO_TOOLCHAIN_MODE="$GO_TOOLCHAIN_MODE" \
    bash "$GO_PROBE_DIR/build.sh"
  env HARMONYOS_NATIVE_HOME="$NATIVE_HOME" GO_BIN="$GO_BIN" \
    GO_PROBE_OUTPUT_DIR="$GO_PROBE_OUTPUT_DIR" GO_TOOLCHAIN_MODE="$GO_TOOLCHAIN_MODE" \
    bash "$GO_PROBE_DIR/build.sh"
) 2>&1 | tee "$GO_BUILD_LOG"; then
  fail "stock Go build failed (see $GO_BUILD_LOG)"
fi
[[ -f "$GO_SO" ]] || fail "stock Go build did not produce libgoprobe.so"
printf 'GO_BUILD_VERDICT=pass\n'

# --- 2. ELF verification ----------------------------------------------------
go_version="$(GOTOOLCHAIN="$GO_TOOLCHAIN_MODE" "$GO_BIN" version)"
case "$go_version" in
  "$GO_VERSION_PREFIX"*) printf 'GO_VERSION_VERIFY=pass version=%s\n' "$go_version" ;;
  *) fail "Go version mismatch: $go_version" ;;
esac
file_output="$(file "$GO_SO")"
printf 'GO_SO_FILE=%s\n' "$file_output"
case "$file_output" in
  *"ELF 64-bit"*"x86-64"*) printf 'GO_SO_ELF_VERIFY=pass\n' ;;
  *) fail "libgoprobe.so is not an x86_64 ELF: $file_output" ;;
esac
readelf_l="$(readelf -lW "$GO_SO")"
pt_tls_diag "$readelf_l" || fail "libgoprobe.so has no PT_TLS segment"
printf 'GO_SO_PT_TLS_VERIFY=pass\n'
readelf_d="$(readelf -dW "$GO_SO")"
printf '%s\n' "$readelf_d" | grep -E 'FLAGS|FLAGS_1|NEEDED' | head -10 || true
if ! printf '%s\n' "$readelf_d" | grep -q 'STATIC_TLS'; then
  fail "libgoprobe.so has no STATIC_TLS flag"
fi
printf 'GO_SO_STATIC_TLS_VERIFY=pass\n'
readelf_r="$(readelf -rW "$GO_SO")"
if ! printf '%s\n' "$readelf_r" | grep -q 'R_X86_64_TPOFF64'; then
  fail "libgoprobe.so has no R_X86_64_TPOFF64 relocation"
fi
printf 'GO_SO_TPOFF64_VERIFY=pass\n'
for symbol in Hello RuntimeProbe NetDialProbe; do
  if ! readelf -Ws "$GO_SO" | grep -F "$symbol" >/dev/null; then
    fail "libgoprobe.so missing exported symbol: $symbol"
  fi
done
printf 'GO_SO_EXPORT_VERIFY=pass\n'
run sha256sum "$GO_SO"
go_so_sha="$(sha256sum "$GO_SO" | awk '{print $1}')"
printf 'GO_SO_SHA256=%s\n' "$go_so_sha"

# --- 3. snapshot extraction and HAP build -----------------------------------
snapshot_dir="$(mktemp -d "${TMPDIR:-/tmp}/e1-stock-go-snapshot.XXXXXX")"
if ! (
  cd "$WORKSPACE"
  print_command git archive "$SNAPSHOT_COMMIT" spikes/r1-api24-hap
  git archive "$SNAPSHOT_COMMIT" spikes/r1-api24-hap | tar -x -C "$snapshot_dir"
); then
  fail "snapshot git archive/tar extraction failed"
fi
snapshot_project="$snapshot_dir/spikes/r1-api24-hap"
[[ -f "$snapshot_project/entry/src/main/cpp/probe.cpp" ]] || fail "snapshot extraction missing probe.cpp"
grep -q "runGoProbe" "$snapshot_project/entry/src/main/cpp/probe.cpp" || fail "snapshot probe.cpp missing runGoProbe"
grep -q "GO_SPIKE_RESULT" "$snapshot_project/entry/src/main/cpp/probe.cpp" || fail "snapshot probe.cpp missing GO_SPIKE_RESULT"
grep -q "runGoProbe" "$snapshot_project/entry/src/main/cpp/types/libprobe/index.d.ts" || fail "snapshot index.d.ts missing runGoProbe"
grep -q "GO_SPIKE_RESULT" "$snapshot_project/entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets" || fail "snapshot TestRunner missing GO_SPIKE_RESULT"
printf 'SNAPSHOT_EXTRACT_VERIFY=pass\n'
printf 'SNAPSHOT_DIR=%s\n' "$snapshot_dir"

# the snapshot lock file must be the snapshot's own (libprobe only), not the
# working tree's (which also carries later e2 libe2network entries)
[[ -f "$snapshot_project/entry/oh-package-lock.json5" ]] || fail "snapshot extraction missing oh-package-lock.json5"
grep -q 'libprobe.so@src/main/cpp/types/libprobe' "$snapshot_project/entry/oh-package-lock.json5" || fail "snapshot lock file missing libprobe specifier"
if grep -q 'libe2network' "$snapshot_project/entry/oh-package-lock.json5"; then
  fail "snapshot lock file contains working-tree e2 contamination"
fi
printf 'SNAPSHOT_LOCK_VERIFY=pass\n'

plugin_path="$(grep -oP "file:\K[^']+" "$snapshot_project/hvigor/hvigor-config.json5" | head -1 || true)"
if [[ "$plugin_path" != "$STABLE_TOOLS/hvigor/hvigor-ohos-plugin" ]]; then
  fail "snapshot hvigor plugin path $plugin_path != $STABLE_TOOLS/hvigor/hvigor-ohos-plugin"
fi
[[ -d "$plugin_path" ]] || fail "hvigor plugin directory missing: $plugin_path"
printf 'HVIGOR_PLUGIN_VERIFY=pass\n'

# place the stock Go library into the snapshot project for Hvigor packaging
mkdir -p "$snapshot_project/entry/libs/x86_64"
cp "$GO_SO" "$snapshot_project/entry/libs/x86_64/libgoprobe.so"
snapshot_go_so="$snapshot_project/entry/libs/x86_64/libgoprobe.so"
run sha256sum "$snapshot_go_so"

if ! (
  cd "$snapshot_project"
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
app_hap="$snapshot_project/$APP_HAP_REL"
test_hap="$snapshot_project/$TEST_HAP_REL"
[[ -f "$app_hap" ]] || fail "clean build did not produce application HAP"
[[ -f "$test_hap" ]] || fail "clean build did not produce test HAP"
printf 'HAP_BUILD_VERDICT=pass\n'

# --- 4. HAP member identity -------------------------------------------------
app_member="$(mktemp "${TMPDIR:-/tmp}/e1-stock-go-app-member.XXXXXX")"
test_member="$(mktemp "${TMPDIR:-/tmp}/e1-stock-go-test-member.XXXXXX")"
if ! unzip -p "$app_hap" libs/x86_64/libgoprobe.so >"$app_member"; then
  fail "unzip failed to extract libgoprobe.so from application HAP"
fi
if ! unzip -p "$test_hap" libs/x86_64/libgoprobe.so >"$test_member"; then
  fail "unzip failed to extract libgoprobe.so from test HAP"
fi
[[ -s "$app_member" ]] || fail "application HAP has no libgoprobe.so member"
[[ -s "$test_member" ]] || fail "test HAP has no libgoprobe.so member"
app_member_sha="$(sha256sum "$app_member" | awk '{print $1}')"
test_member_sha="$(sha256sum "$test_member" | awk '{print $1}')"
printf 'APP_MEMBER_SHA256=%s\n' "$app_member_sha"
printf 'TEST_MEMBER_SHA256=%s\n' "$test_member_sha"
if [[ "$app_member_sha" != "$go_so_sha" || "$test_member_sha" != "$go_so_sha" ]]; then
  fail "HAP libgoprobe.so member is not byte-equal to the built input"
fi
printf 'HAP_MEMBER_IDENTITY=pass\n'
if unzip -Z1 "$app_hap" | grep -E 'libtls-|libneededprobe|libtlsprobe' >/dev/null ||
   unzip -Z1 "$test_hap" | grep -E 'libtls-|libneededprobe|libtlsprobe' >/dev/null; then
  fail "forbidden historical TLS member in application or test HAP"
fi
printf 'FORBIDDEN_HISTORICAL_MEMBER=false\n'
run sha256sum "$app_hap" "$test_hap"

# --- 5. Emulator boot -------------------------------------------------------
"$HDC" kill || true
HDC_PORT="$EMULATOR_HDC_PORT" timeout 60 "$STOP_HELPER" || true
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
  -bootMode coldboot -hdcport "$EMULATOR_HDC_PORT" >"$CONSOLE" 2>&1 &
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
  HDC_PORT="$EMULATOR_HDC_PORT" timeout 30 "$CONNECT_HELPER" || true
  shell_probe="$(hdc shell "echo e1-stock-go-connect-$attempt" 2>&1 || true)"
  distribution="$(hdc shell 'param get const.product.os.dist.name' 2>&1 | tr -d '\r' || true)"
  printf 'CONNECTIVITY attempt=%s shell=%q distribution=%q\n' \
    "$attempt" "$shell_probe" "$distribution"
  if [[ "$shell_probe" == "e1-stock-go-connect-$attempt" && "$distribution" == *HarmonyOS* ]]; then
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
  shell_readiness="$(hdc shell "echo e1-stock-go-readiness-$boot_attempt" 2>&1 || true)"
  printf 'BOOT_READINESS attempt=%s qemu.boot=%q shell=%q\n' \
    "$boot_attempt" "$qemu_boot_complete" "$shell_readiness"
  if [[ -n "$qemu_boot_complete" && "$shell_readiness" == "e1-stock-go-readiness-$boot_attempt" ]]; then
    ready=1
    break
  fi
  sleep 1
done
if (( ready != 1 )); then
  printf 'READINESS_VERDICT=blocked\n'
  exit 1
fi
printf 'READINESS_VERDICT=pass\n'

# --- 6. install and aa test -------------------------------------------------
hdc shell "rm -rf $STAGING" || fail "guest staging rm failed"
hdc shell "mkdir -p $STAGING" || fail "guest staging mkdir failed"
timeout 180 "$HDC" -t "$EMULATOR_TARGET" file send "$app_hap" "$STAGING/entry-default-unsigned.hap" || fail "app HAP file send failed"
timeout 180 "$HDC" -t "$EMULATOR_TARGET" file send "$test_hap" "$STAGING/entry-ohosTest-unsigned.hap" || fail "test HAP file send failed"
install_output=''
for install_attempt in $(seq 1 30); do
  install_output="$(timeout 180 "$HDC" -t "$EMULATOR_TARGET" shell "bm install -p $STAGING" 2>&1 || true)"
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

hilog_buffer_output="$(hdc shell 'hilog -G 16M' 2>&1 | tr -d '\r' || true)"
hilog_buffer_query="$(hdc shell 'hilog -g' 2>&1 | tr -d '\r' || true)"
printf 'HILOG_BUFFER_SET=%q\n' "$hilog_buffer_output"
printf 'HILOG_BUFFER_QUERY=%q\n' "$hilog_buffer_query"
if [[ "$hilog_buffer_query" != *"16.0M"* && "$hilog_buffer_query" != *"16M"* &&
      "$hilog_buffer_query" != *"16777216"* ]]; then
  fail "HiLog buffer did not report the required 16 MiB capacity"
fi
printf 'HILOG_BUFFER_VERDICT=pass\n'
hdc shell 'hilog -r' || fail "hilog clear failed"
set +e
aa_output="$(timeout 60 "$HDC" -t "$EMULATOR_TARGET" shell "aa test -b $BUNDLE -m $TEST_MODULE -s unittest $TEST_RUNNER -s timeout 15000" 2>&1)"
aa_rc=$?
set -e
printf 'AA_TEST_RC=%s\n' "$aa_rc"
printf 'AA_TEST_OUTPUT=%q\n' "$aa_output"
printf '%s\n' "$aa_output" >"$AA_TEST_LOG"
if (( aa_rc != 0 )); then
  fail "aa test exited non-zero (rc=$aa_rc)"
fi

# --- 7. directed HiLog capture ----------------------------------------------
: >"$TAG_HILOG"
{
  printf '===== TAG HILOG R1Api24ProbeTest =====\n'
  hdc shell 'hilog -x -T R1Api24ProbeTest -v year -v zone' 2>&1 || true
} >>"$TAG_HILOG"
hdc shell 'hilog -x -t app -v year -v zone' >"$APP_HILOG" 2>&1 || true
printf 'TAG_HILOG_LINES=%s\n' "$(wc -l <"$TAG_HILOG")"
printf 'APP_HILOG_LINES=%s\n' "$(wc -l <"$APP_HILOG")"
run sha256sum "$TAG_HILOG" "$APP_HILOG"

# --- 8. judgment ------------------------------------------------------------
# Prefer the directed TAG HiLog; fall back to the aa test output, then the
# full app HiLog, so a missed tag capture never fabricates a verdict. The
# source of each judged line is printed explicitly; only when all three
# sources are empty does the runner fail.
baseline_line=""
baseline_source=""
for f in "$TAG_HILOG" "$AA_TEST_LOG" "$APP_HILOG"; do
  if [[ -n "$f" && -f "$f" ]]; then
    candidate="$(grep -F 'BASELINE_RESULT' "$f" | tail -1 || true)"
    if [[ -n "$candidate" ]]; then
      baseline_line="$candidate"
      baseline_source="$f"
      break
    fi
  fi
done
spike_line=""
spike_source=""
for f in "$TAG_HILOG" "$AA_TEST_LOG" "$APP_HILOG"; do
  if [[ -n "$f" && -f "$f" ]]; then
    candidate="$(grep -F 'GO_SPIKE_RESULT' "$f" | tail -1 || true)"
    if [[ -n "$candidate" ]]; then
      spike_line="$candidate"
      spike_source="$f"
      break
    fi
  fi
done
printf 'BASELINE_RESULT_SOURCE=%s\n' "$baseline_source"
printf 'GO_SPIKE_RESULT_SOURCE=%s\n' "$spike_source"
printf 'BASELINE_RESULT_LINE=%s\n' "$baseline_line"
printf 'GO_SPIKE_RESULT_LINE=%s\n' "$spike_line"
if [[ -z "$baseline_line" ]]; then
  fail "BASELINE_RESULT missing from all HiLog sources (tag/aa-test/app)"
fi
if [[ "$baseline_line" != *"functional=PASS"* ]]; then
  fail "Node-API baseline did not pass: $baseline_line"
fi
printf 'BASELINE_JUDGMENT=pass\n'
if [[ -z "$spike_line" ]]; then
  fail "GO_SPIKE_RESULT missing from all HiLog sources (tag/aa-test/app)"
fi
case "$(judge_spike_line "$spike_line")" in
  pass)
    result="pass"
    printf 'MEASURED_VERDICT=pass\n'
    printf 'MEASURED_VERDICT_NOTE=unexpected stock Go load success; requires independent review before any conclusion\n'
    ;;
  blocked)
    result="blocked"
    printf 'MEASURED_VERDICT=blocked\n'
    printf 'MEASURED_VERDICT_NOTE=expected stock Go 1.25.12 initial-exec TLS loader rejection; measured blocked, not runner failure\n'
    ;;
  fail)
    fail "unexpected GO_SPIKE_RESULT: $spike_line"
    ;;
esac

# --- 9. cleanup -------------------------------------------------------------
hdc shell "rm -rf $STAGING" || true
if (( installed == 1 )); then
  timeout 120 "$HDC" -t "$EMULATOR_TARGET" uninstall "$BUNDLE" || true
  installed=0
fi
if (( emulator_started == 1 )); then
  HDC_PORT="$EMULATOR_HDC_PORT" timeout 60 "$STOP_HELPER" || true
  for _ in $(seq 1 30); do
    if ! pgrep -f '/emulator/Emulator.*-start '"$EMULATOR_INSTANCE"'|qemu-system.*'"$EMULATOR_INSTANCE" >/dev/null; then
      break
    fi
    sleep 1
  done
  emulator_started=0
fi
"$HDC" kill || true
residual_cleared=0
for cleanup_attempt in $(seq 1 10); do
  residual_processes="$(pgrep -af '/emulator/Emulator.*-start '"$EMULATOR_INSTANCE"'|qemu-system.*'"$EMULATOR_INSTANCE"'|hdc -m -s' || true)"
  residual_ports="$(ss -ltnp | grep -E ":($EMULATOR_HDC_PORT|5555|8710)[[:space:]]" || true)"
  printf 'FINAL_CLEANUP_ATTEMPT=%s processes=%q ports=%q\n' \
    "$cleanup_attempt" "$residual_processes" "$residual_ports"
  if [[ -z "$residual_processes" && -z "$residual_ports" ]]; then
    residual_cleared=1
    break
  fi
  "$HDC" kill || true
  sleep 1
done
if (( residual_cleared != 1 )); then
  fail "residual Emulator, HDC, or tested port after cleanup"
fi
printf 'FINAL_RESIDUAL_PROCESS=false\n'
printf 'FINAL_RESIDUAL_PORT=false\n'

# --- 10. manifest -----------------------------------------------------------
# The transcript hash is intentionally NOT written here: the transcript is
# still being appended to by tee (and later by teardown output). The final
# transcript hash is appended by seal_and_finalize after the stream is closed.
ended_at="$(date --iso-8601=seconds)"
{
  printf '=== E1 stock Go replay manifest ===\n'
  printf 'evidence_id=%s\n' "$EVIDENCE_ID"
  printf 'code_sha=%s\n' "$code_sha"
  printf 'baseline_tag=%s\n' "$BASELINE_TAG"
  printf 'baseline_commit=%s\n' "$BASELINE_COMMIT"
  printf 'snapshot_commit=%s\n' "$SNAPSHOT_COMMIT"
  printf 'target_tuple=%s\n' "$TARGET_TUPLE"
  printf 'emulator_target=%s\n' "$EMULATOR_TARGET"
  printf 'started_at=%s\n' "$started_at"
  printf 'ended_at=%s\n' "$ended_at"
  printf 'verdict=%s\n' "$result"
  printf 'go_so_sha256=%s\n' "$go_so_sha"
  printf 'app_hap_sha256=%s\n' "$(sha256sum "$app_hap" | awk '{print $1}')"
  printf 'test_hap_sha256=%s\n' "$(sha256sum "$test_hap" | awk '{print $1}')"
  printf 'tag_hilog_sha256=%s\n' "$(sha256sum "$TAG_HILOG" | awk '{print $1}')"
  printf 'app_hilog_sha256=%s\n' "$(sha256sum "$APP_HILOG" | awk '{print $1}')"
  printf 'build_log_sha256=%s\n' "$(sha256sum "$BUILD_LOG" | awk '{print $1}')"
  printf 'go_build_log_sha256=%s\n' "$(sha256sum "$GO_BUILD_LOG" | awk '{print $1}')"
  printf 'baseline_verify_log_sha256=%s\n' "$(sha256sum "$BASELINE_VERIFY_LOG" | awk '{print $1}')"
  printf 'console_sha256=%s\n' "$(sha256sum "$CONSOLE" | awk '{print $1}')"
  printf 'aa_test_log_sha256=%s\n' "$(sha256sum "$AA_TEST_LOG" | awk '{print $1}')"
  # E1-scoped verdict only: this record never changes E8 status on its own.
  printf 'e8_status=CLOSED\n'
} >"$MANIFEST"
printf 'ENDED_AT=%s\n' "$ended_at"
printf 'VERDICT=%s\n' "$result"
printf 'RECORD_STATUS=collected\n'
