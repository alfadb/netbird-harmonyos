#!/usr/bin/env bash
set -Eeuo pipefail

readonly EVIDENCE_ID="EV-E1-EMU24-20260717-0005"
readonly WORKSPACE="/home/worker/work/base/netbird-harmonyos"
readonly PROJECT="$WORKSPACE/spikes/r1-api24-hap"
readonly RAW="$WORKSPACE/docs/evidence/raw"
readonly APP_HAP="$PROJECT/entry/build/default/outputs/default/entry-default-unsigned.hap"
readonly BUNDLE="cn.alfadb.netbird.r1probe"
readonly MODULE="entry"
readonly ABILITY="EntryAbility"
readonly INSTANCE="netbird_api24_phone"
readonly TARGET="127.0.0.1:10000"
readonly STAGING="/data/local/tmp/e1-c-emu24-20260717-0005"
readonly STABLE_TOOLS="/home/worker/harmonyos/command-line-tools/6.1.1.290"
readonly BETA_TOOLS="/home/worker/harmonyos/command-line-tools/26.0.0.461"
readonly HVIGOR="$STABLE_TOOLS/bin/hvigorw"
readonly SDK="$BETA_TOOLS/sdk/default/openharmony"
readonly NAPI_HEADER="$SDK/native/sysroot/usr/include/napi/native_api.h"
readonly EMULATOR="$BETA_TOOLS/emulator/Emulator"
readonly HDC="$SDK/toolchains/hdc"
readonly CONNECT_HELPER="/home/worker/harmonyos/bin/emulator-connect"
readonly STOP_HELPER="/home/worker/harmonyos/bin/emulator-stop"
readonly QEMU_LOG="/home/worker/harmonyos/emulator-instances/netbird_api24_phone/Log/qemu.log"
readonly TRANSCRIPT="$RAW/$EVIDENCE_ID-transcript.log"
readonly CONSOLE="$RAW/$EVIDENCE_ID-emulator-console.log"
readonly TAG_HILOG="$RAW/$EVIDENCE_ID-hilog-tag.log"
readonly BUILD_LOG="$RAW/$EVIDENCE_ID-build.log"
readonly SOURCE_MANIFEST="$RAW/$EVIDENCE_ID-source-manifest.txt"
readonly SOURCE_ARCHIVE="$RAW/$EVIDENCE_ID-source.tar"
readonly HAP_ARCHIVE="$RAW/$EVIDENCE_ID-application-hap.bin"
readonly LIB_ARCHIVE="$RAW/$EVIDENCE_ID-libprobe-x86_64.bin"

mkdir -p "$RAW"
exec > >(tee "$TRANSCRIPT") 2>&1

started_at="$(date --iso-8601=seconds)"
emulator_started=0
cleaned=0
installed=0
result="blocked"
pids=()

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
  timeout 30 "$HDC" -t "$TARGET" "$@"
}

fail() {
  result="fail"
  printf 'FAIL_REASON=%s\n' "$1"
  exit 1
}

get_pid() {
  local output
  output="$(hdc shell "pidof $BUNDLE" 2>/dev/null | tr -d '\r' || true)"
  if [[ "$output" =~ ^[[:space:]]*([0-9]+) ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
  fi
}

wait_for_process_absent() {
  local attempt pid
  for attempt in $(seq 1 30); do
    pid="$(get_pid)"
    if [[ -z "$pid" ]]; then
      return 0
    fi
    sleep 1
  done
  return 1
}

wait_for_process_present() {
  local attempt pid
  for attempt in $(seq 1 30); do
    pid="$(get_pid)"
    if [[ -n "$pid" ]]; then
      printf '%s\n' "$pid"
      return 0
    fi
    sleep 1
  done
  return 1
}

stop_emulator() {
  if (( emulator_started == 1 )); then
    HDC_PORT=10000 timeout 60 "$STOP_HELPER" || true
    for _ in $(seq 1 30); do
      if ! pgrep -f '/emulator/Emulator.*-start netbird_api24_phone|qemu-system.*netbird_api24_phone' >/dev/null; then
        break
      fi
      sleep 1
    done
    emulator_started=0
  fi
}

cleanup() {
  local exit_code=$?
  set +e
  if (( cleaned == 0 )); then
    hdc shell "aa force-stop $BUNDLE" || true
    hdc shell "rm -rf $STAGING" || true
    if (( installed == 1 )); then
      timeout 120 "$HDC" -t "$TARGET" uninstall "$BUNDLE" || true
    fi
    stop_emulator
    "$HDC" kill || true
    cleaned=1
  fi
  set -e
  printf 'TRAP_EXIT_CODE=%s RESULT=%s\n' "$exit_code" "$result"
  exit "$exit_code"
}
trap cleanup EXIT

printf 'EVIDENCE_ID=%s\n' "$EVIDENCE_ID"
printf 'STARTED_AT=%s\n' "$started_at"
printf 'CLOCK_SOURCE=host_CLOCK_REALTIME_date_iso_8601_seconds\n'
printf 'TIMEZONE=%s\n' "$(date +%Z%:z)"
printf 'GIT_HEAD=%s\n' "$(git -C "$WORKSPACE" rev-parse HEAD)"
printf 'RECORD_SCOPE=E1_C-only_ordinary_EntryAbility_no_TestRunner_no_Go_no_NetBird_no_PS4\n'
printf 'TARGET_TUPLE=HarmonyOS_6.1.1(24),software_6.1.0.125,phone_Emulator,x86_64,SDK_API24,Beta_HDC_3.2.0e,unsigned_debug_normal_app\n'
printf 'TEST_RUNNER_USED=false\n'
printf 'PHYSICAL_DEVICE_USED=false\n'
printf 'GO_NETBIRD_PS4_TOUCHED=false\n'

rm -f "$CONSOLE" "$TAG_HILOG" "$BUILD_LOG" "$SOURCE_MANIFEST" "$SOURCE_ARCHIVE" \
  "$HAP_ARCHIVE" "$LIB_ARCHIVE" "$RAW/$EVIDENCE_ID"-run*.png \
  "$RAW/$EVIDENCE_ID"-run*-hilog-full.log

(
  cd "$PROJECT"
  print_command "$HVIGOR" clean --mode module -p product=default -p module=entry@default \
    -p buildMode=debug --no-daemon
  "$HVIGOR" clean --mode module -p product=default -p module=entry@default \
    -p buildMode=debug --no-daemon
  print_command "$HVIGOR" assembleHap --mode module -p product=default -p module=entry@default \
    -p buildMode=debug --no-daemon
  "$HVIGOR" assembleHap --mode module -p product=default -p module=entry@default \
    -p buildMode=debug --no-daemon
) 2>&1 | tee "$BUILD_LOG"
[[ -f "$APP_HAP" ]] || fail "clean build did not produce application HAP"
printf 'CLEAN_BUILD_VERDICT=pass\n'

run install -m 0644 "$APP_HAP" "$HAP_ARCHIVE"
unzip -p "$APP_HAP" libs/x86_64/libprobe.so >"$LIB_ARCHIVE"
run sha256sum "$APP_HAP" "$HAP_ARCHIVE" "$LIB_ARCHIVE" "$BUILD_LOG" "$NAPI_HEADER"
run stat -c '%n size=%s mtime=%y' "$APP_HAP" "$HAP_ARCHIVE" "$LIB_ARCHIVE" "$BUILD_LOG"
run unzip -Z1 "$APP_HAP"
if unzip -Z1 "$APP_HAP" | grep -E 'libgoprobe|libtls-|entry_test|ohosTest' >/dev/null; then
  fail "forbidden Go, TLS, or TestRunner member in application HAP"
fi
printf 'FORBIDDEN_HISTORICAL_MEMBER=false\n'
unzip -p "$APP_HAP" module.json

run file "$LIB_ARCHIVE"
run readelf -d "$LIB_ARCHIVE"
for symbol in napi_create_threadsafe_function napi_call_threadsafe_function \
  napi_acquire_threadsafe_function napi_release_threadsafe_function pthread_create socketpair dup close; do
  if ! readelf -Ws "$LIB_ARCHIVE" | grep -F "$symbol" >/dev/null; then
    fail "required native symbol missing: $symbol"
  fi
done
printf 'PUBLIC_THREADSAFE_SYMBOL_AUDIT=pass\n'

source_inputs=(
  spikes/r1-api24-hap/e1-c-emulator-run.sh
  spikes/r1-api24-hap/entry/src/main/cpp/CMakeLists.txt
  spikes/r1-api24-hap/entry/src/main/cpp/e1_c_probe.cpp
  spikes/r1-api24-hap/entry/src/main/cpp/types/libprobe/index.d.ts
  spikes/r1-api24-hap/entry/src/main/ets/entryability/EntryAbility.ets
  spikes/r1-api24-hap/entry/src/main/ets/pages/Index.ets
  spikes/r1-api24-hap/entry/src/main/module.json5
  spikes/r1-api24-hap/entry/build-profile.json5
  spikes/r1-api24-hap/build-profile.json5
)
(
  cd "$WORKSPACE"
  sha256sum "${source_inputs[@]}"
) >"$SOURCE_MANIFEST"
(
  cd "$WORKSPACE"
  tar --sort=name --mtime='2026-07-17 00:00:00 +0800' --owner=0 --group=0 --numeric-owner \
    -cf "$SOURCE_ARCHIVE" "${source_inputs[@]}"
)
run sha256sum "$SOURCE_MANIFEST" "$SOURCE_ARCHIVE"
printf 'SOURCE_INPUT_COUNT=%s\n' "${#source_inputs[@]}"

printf 'TOOLCHAIN host=%q\n' "$(uname -a)"
run "$HVIGOR" --version
run "$HDC" -v
run node --version
run "$STABLE_TOOLS/bin/ohpm" --version

"$HDC" kill || true
timeout 60 "$STOP_HELPER" || true
if pgrep -f '/emulator/Emulator.*-start netbird_api24_phone|qemu-system.*netbird_api24_phone' >/dev/null; then
  fail "residual Emulator exists before cold boot"
fi
qemu_start_line=$(( $(wc -l <"$QEMU_LOG") + 1 ))
printf 'QEMU_CURRENT_BOOT_START_LINE=%s\n' "$qemu_start_line"

print_command "$EMULATOR" -start "$INSTANCE" -instancePath /home/worker/harmonyos/emulator-instances \
  -imageRoot /home/worker/harmonyos/emulator-images -bootMode coldboot -hdcport 10000
DISPLAY=:1 XAUTHORITY=/home/worker/.Xauthority XDG_RUNTIME_DIR=/tmp/runtime-worker \
  "$EMULATOR" -start "$INSTANCE" \
  -instancePath /home/worker/harmonyos/emulator-instances \
  -imageRoot /home/worker/harmonyos/emulator-images \
  -bootMode coldboot -hdcport 10000 >"$CONSOLE" 2>&1 &
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
  HDC_PORT=10000 timeout 30 "$CONNECT_HELPER" || true
  targets="$($HDC list targets -v || true)"
  shell_probe="$(hdc shell "echo e1-c-connect-$attempt" 2>&1 || true)"
  distribution="$(hdc shell 'param get const.product.os.dist.name' 2>&1 | tr -d '\r' || true)"
  printf 'CONNECTIVITY attempt=%s targets=%q shell=%q distribution=%q\n' \
    "$attempt" "$targets" "$shell_probe" "$distribution"
  if [[ "$shell_probe" == "e1-c-connect-$attempt" && "$distribution" == *HarmonyOS* ]]; then
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
  shell_readiness="$(hdc shell "echo e1-c-readiness-$boot_attempt" 2>&1 || true)"
  printf 'BOOT_READINESS attempt=%s qemu.boot=%q shell=%q\n' \
    "$boot_attempt" "$qemu_boot_complete" "$shell_readiness"
  if [[ -n "$qemu_boot_complete" && "$shell_readiness" == "e1-c-readiness-$boot_attempt" ]]; then
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

hdc shell "rm -rf $STAGING"
hdc shell "mkdir -p $STAGING"
hdc file send "$HAP_ARCHIVE" "$STAGING/entry-default-unsigned.hap"
install_output=''
for install_attempt in $(seq 1 30); do
  install_output="$(timeout 180 "$HDC" -t "$TARGET" shell "bm install -p $STAGING" 2>&1 || true)"
  printf 'INSTALL_ATTEMPT=%s OUTPUT=%q\n' "$install_attempt" "$install_output"
  if [[ "$install_output" == *"install bundle successfully"* ]]; then
    installed=1
    break
  fi
  sleep 2
done
if (( installed != 1 )); then
  fail "application HAP installation failed"
fi
printf 'INSTALL_VERDICT=pass\n'

wakeup_output="$(hdc shell 'power-shell wakeup' 2>&1 | tr -d '\r')"
home_output="$(hdc shell 'uitest uiInput keyEvent Home' 2>&1 | tr -d '\r')"
swipe_output="$(hdc shell 'uitest uiInput swipe 660 2500 660 500 2000' 2>&1 | tr -d '\r')"
printf 'UNLOCK_SEQUENCE wakeup=%q home=%q swipe=%q\n' "$wakeup_output" "$home_output" "$swipe_output"
sleep 2

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

hilog_buffer_output="$(hdc shell 'hilog -G 16M' 2>&1 | tr -d '\r')"
hilog_buffer_query="$(hdc shell 'hilog -g' 2>&1 | tr -d '\r')"
printf 'HILOG_BUFFER_SET=%q\n' "$hilog_buffer_output"
printf 'HILOG_BUFFER_QUERY=%q\n' "$hilog_buffer_query"
if [[ "$hilog_buffer_query" != *"16.0M"* && "$hilog_buffer_query" != *"16M"* &&
      "$hilog_buffer_query" != *"16777216"* ]]; then
  fail "HiLog buffer did not report the required 16 MiB capacity"
fi
printf 'HILOG_BUFFER_VERDICT=pass\n'

: >"$TAG_HILOG"
for run_number in 1 2 3; do
  if (( run_number > 1 )); then
    hdc shell "aa force-stop $BUNDLE"
    wait_for_process_absent || fail "process remained after force-stop before run $run_number"
  fi
  before_pid="$(get_pid)"
  printf 'COLD_START_%s_BEFORE_PID=%q\n' "$run_number" "$before_pid"
  [[ -z "$before_pid" ]] || fail "process existed before cold start $run_number"

  hdc shell 'hilog -r'
  aa_output="$(hdc shell "aa start -a $ABILITY -b $BUNDLE -m $MODULE" 2>&1)"
  printf 'COLD_START_%s_AA_OUTPUT=%q\n' "$run_number" "$aa_output"
  if [[ "$aa_output" == *"error:"* || "$aa_output" == *"Error Code:"* ||
        "$aa_output" != *"start ability successfully."* ]]; then
    fail "aa start semantic failure on run $run_number"
  fi
  printf 'COLD_START_%s_AA_SEMANTIC=pass\n' "$run_number"

  pid="$(wait_for_process_present || true)"
  [[ -n "$pid" ]] || fail "missing process PID on run $run_number"
  for old_pid in "${pids[@]}"; do
    [[ "$pid" != "$old_pid" ]] || fail "PID reused across cold starts: $pid"
  done
  pids+=("$pid")
  printf 'COLD_START_%s_PID=%s\n' "$run_number" "$pid"

  marker_ready=0
  for marker_attempt in $(seq 1 60); do
    tag_snapshot="$(hdc shell 'hilog -x -T R1Api24Probe -v year -v zone' 2>&1 || true)"
    pass_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
      '$4 == pid && index($0, "E1_C_PROBE_RESULT|verdict=PASS") { count++ } END { print count + 0 }')"
    fail_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
      '$4 == pid && index($0, "E1_C_PROBE_RESULT|verdict=FAIL") { count++ } END { print count + 0 }')"
    printf 'COLD_START_%s_MARKER_ATTEMPT=%s PID=%s PASS=%s FAIL=%s\n' \
      "$run_number" "$marker_attempt" "$pid" "$pass_count" "$fail_count"
    if (( fail_count > 0 )); then
      printf '%s\n' "$tag_snapshot"
      fail "application reported E1 FAIL on run $run_number"
    fi
    if (( pass_count == 1 )); then
      marker_ready=1
      break
    fi
    sleep 1
  done
  if (( marker_ready != 1 )); then
    printf '%s\n' "$tag_snapshot"
    fail "application E1 PASS marker timeout on run $run_number"
  fi

  warmup_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_TSFN_WARMUP|verdict=PASS|callbacks=100") { count++ } END { print count + 0 }')"
  warmup_callback_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_CALLBACK_WARMUP|") && index($0, "|valid=true") { count++ } END { print count + 0 }')"
  round_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_ROUND_RESULT|verdict=PASS") { count++ } END { print count + 0 }')"
  buffer_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_BUFFER_SAMPLE|") { count++ } END { print count + 0 }')"
  callback_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_CALLBACK_SAMPLE|") && index($0, "|valid=true") { count++ } END { print count + 0 }')"
  invalid_callback_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_CALLBACK_SAMPLE|") && index($0, "|valid=false") { count++ } END { print count + 0 }')"
  fd_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_FD_RESULT|") { count++ } END { print count + 0 }')"
  pthread_start_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_PTHREAD_START|") { count++ } END { print count + 0 }')"
  pthread_finish_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_PTHREAD_FINISH|") { count++ } END { print count + 0 }')"
  printf 'COLD_START_%s_COUNTS warmup=%s warmupCallbacks=%s rounds=%s buffers=%s callbacks=%s invalidCallbacks=%s fd=%s pthreadStart=%s pthreadFinish=%s\n' \
    "$run_number" "$warmup_count" "$warmup_callback_count" "$round_count" "$buffer_count" "$callback_count" \
    "$invalid_callback_count" "$fd_count" "$pthread_start_count" "$pthread_finish_count"
  if (( warmup_count != 1 || warmup_callback_count < 1 || round_count != 10 || buffer_count != 1000 ||
        callback_count != 1000 || invalid_callback_count != 0 || fd_count != 10 || pthread_start_count != 10 ||
        pthread_finish_count != 10 )); then
    fail "per-PID E1 evidence count mismatch on run $run_number"
  fi
  if printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && /E1_ROUND_RESULT/ && $0 !~ /monotonicIncrease=false/ { bad=1 } END { exit !bad }'; then
    fail "resource monotonic increase marker detected on run $run_number"
  fi
  printf 'COLD_START_%s_E1_COUNTS=pass\n' "$run_number"

  {
    printf '===== RUN %s PID %s TAG HILOG =====\n' "$run_number" "$pid"
    printf '%s\n' "$tag_snapshot"
  } >>"$TAG_HILOG"
  full_hilog="$RAW/$EVIDENCE_ID-run${run_number}-hilog-full.log"
  hdc shell 'hilog -x -v year -v zone' >"$full_hilog"
  printf 'COLD_START_%s_FULL_HILOG_LINES=%s\n' "$run_number" "$(wc -l <"$full_hilog")"
  run sha256sum "$full_hilog"

  if ! kill_probe="$(get_pid)" || [[ "$kill_probe" != "$pid" ]]; then
    fail "application process exited before screenshot on run $run_number"
  fi
  screen_output="$(hdc shell 'uitest screenCap' 2>&1 | tr -d '\r')"
  printf 'COLD_START_%s_SCREEN_OUTPUT=%q\n' "$run_number" "$screen_output"
  remote_screen="$(printf '%s\n' "$screen_output" | sed -n 's/^ScreenCap saved to //p' | tail -1)"
  [[ -n "$remote_screen" ]] || fail "screenshot path missing on run $run_number"
  local_screen="$RAW/$EVIDENCE_ID-run${run_number}.png"
  timeout 30 "$HDC" -t "$TARGET" file recv "$remote_screen" "$local_screen"
  hdc shell "rm -f $remote_screen"
  run file "$local_screen"
  run sha256sum "$local_screen"
  pixel_rgb="$(ffmpeg -v error -i "$local_screen" -vf 'crop=1:1:100:1000,format=rgb24' \
    -frames:v 1 -f rawvideo - | od -An -tu1 | xargs)"
  yavg="$(ffmpeg -hide_banner -i "$local_screen" -vf signalstats,metadata=print \
    -frames:v 1 -f null - 2>&1 | sed -n 's/.*lavfi.signalstats.YAVG=//p' | tail -1)"
  printf 'COLD_START_%s_PIXEL_RGB_100_1000=%s\n' "$run_number" "$pixel_rgb"
  printf 'COLD_START_%s_YAVG=%s\n' "$run_number" "$yavg"
  read -r red green blue <<<"$pixel_rgb"
  if [[ -z "$yavg" ]] || ! awk -v value="$yavg" 'BEGIN { exit !(value > 32.0) }'; then
    fail "screenshot is black or unreadable on run $run_number"
  fi
  if (( red < 235 || red > 249 || green < 239 || green > 252 || blue < 241 || blue > 254 )); then
    fail "expected application background pixel missing on run $run_number"
  fi
  printf 'COLD_START_%s_VISIBLE_PAGE=pass\n' "$run_number"
done

printf 'COLD_START_PIDS=%s,%s,%s\n' "${pids[0]}" "${pids[1]}" "${pids[2]}"
run sha256sum "$TAG_HILOG" "$CONSOLE" "$RAW/$EVIDENCE_ID"-run*.png

hdc shell "aa force-stop $BUNDLE"
wait_for_process_absent || fail "process remained after final force-stop"
printf 'POST_FORCE_STOP_PID=%q\n' "$(get_pid)"

hdc shell "rm -rf $STAGING"
timeout 120 "$HDC" -t "$TARGET" uninstall "$BUNDLE"
installed=0
post_uninstall="$(hdc shell "bm dump -n $BUNDLE" 2>&1 || true)"
printf 'POST_UNINSTALL_BM=%q\n' "$post_uninstall"
if [[ "$post_uninstall" != *'failed to get information'* ]]; then
  fail "bundle still present after uninstall"
fi
printf 'UNINSTALL_ABSENCE=pass\n'

stop_emulator
"$HDC" kill || true
residual_cleared=0
for cleanup_attempt in $(seq 1 10); do
  residual_processes="$(pgrep -af '/emulator/Emulator.*-start netbird_api24_phone|qemu-system.*netbird_api24_phone|hdc -m -s' || true)"
  residual_ports="$(ss -ltnp | grep -E ':(10000|5555|8710)[[:space:]]' || true)"
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

result="pass"
cleaned=1
ended_at="$(date --iso-8601=seconds)"
printf 'ENDED_AT=%s\n' "$ended_at"
printf 'VERDICT=pass\n'
printf 'RECORD_STATUS=collected\n'
