#!/usr/bin/env bash
set -Eeuo pipefail

readonly EVIDENCE_ID="EV-E0-EMU24-20260717-0001"
readonly WORKSPACE="/home/worker/work/base/netbird-harmonyos"
readonly PROJECT="$WORKSPACE/spikes/r1-api24-hap"
readonly RAW="$WORKSPACE/docs/evidence/raw"
readonly APP_HAP="$PROJECT/entry/build/default/outputs/default/entry-default-unsigned.hap"
readonly TEST_HAP="$PROJECT/entry/build/default/outputs/ohosTest/entry-ohosTest-unsigned.hap"
readonly BUNDLE="cn.alfadb.netbird.r1probe"
readonly MODULE="entry"
readonly ABILITY="EntryAbility"
readonly INSTANCE="netbird_api24_phone"
readonly TARGET="127.0.0.1:10000"
readonly STAGING="/data/local/tmp/e0-emu24-20260717-0001"
readonly EMULATOR="/home/worker/harmonyos/command-line-tools/26.0.0.461/emulator/Emulator"
readonly HDC="/home/worker/harmonyos/command-line-tools/26.0.0.461/sdk/default/openharmony/toolchains/hdc"
readonly CONNECT_HELPER="/home/worker/harmonyos/bin/emulator-connect"
readonly STOP_HELPER="/home/worker/harmonyos/bin/emulator-stop"
readonly QEMU_LOG="/home/worker/harmonyos/emulator-instances/netbird_api24_phone/Log/qemu.log"
readonly TRANSCRIPT="$RAW/$EVIDENCE_ID-transcript.log"
readonly CONSOLE="$RAW/$EVIDENCE_ID-emulator-console.log"
readonly FULL_HILOG="$RAW/$EVIDENCE_ID-hilog-app-full.log"
readonly TAG_HILOG="$RAW/$EVIDENCE_ID-hilog-tag.log"

mkdir -p "$RAW"
exec > >(tee "$TRANSCRIPT") 2>&1
exec 3>&2
BASH_XTRACEFD=3
export PS4='+ [$(date --iso-8601=seconds)] ${BASH_SOURCE##*/}:${LINENO}: '
set -x

started_at="$(date --iso-8601=seconds)"
emulator_started=0
emulator_pid=''
cleaned=0
installed=0
result="blocked"
pids=()

hdc() {
  timeout 30 "$HDC" -t "$TARGET" "$@"
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
  for attempt in $(seq 1 20); do
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
printf 'DISPLAY_TARGET=:1\n'
printf 'XAUTHORITY=/home/worker/.Xauthority\n'
printf 'TARGET_TUPLE=HarmonyOS_6.1.1(24),software_6.1.0.125,phone_Emulator,x86_64,SDK_API24,Beta_HDC_3.2.0e,unsigned_debug_normal_app\n'

[[ -f "$APP_HAP" && -f "$TEST_HAP" ]]
sha256sum "$APP_HAP" "$TEST_HAP"
stat -c '%n size=%s mtime=%y' "$APP_HAP" "$TEST_HAP"
unzip -Z1 "$APP_HAP"
unzip -Z1 "$TEST_HAP"
if { unzip -Z1 "$APP_HAP"; unzip -Z1 "$TEST_HAP"; } | grep -E 'libgoprobe|libtls-' >/dev/null; then
  printf 'FORBIDDEN_HISTORICAL_MEMBER=true\n'
  exit 1
fi
printf 'FORBIDDEN_HISTORICAL_MEMBER=false\n'
unzip -p "$APP_HAP" module.json

rm -f "$CONSOLE" "$FULL_HILOG" "$TAG_HILOG" "$RAW/$EVIDENCE_ID"-run*.png
"$HDC" kill || true
timeout 60 "$STOP_HELPER" || true
if pgrep -f '/emulator/Emulator.*-start netbird_api24_phone|qemu-system.*netbird_api24_phone' >/dev/null; then
  printf 'PREFLIGHT_RESIDUAL_EMULATOR=true\n'
  exit 1
fi
qemu_start_line=$(( $(wc -l <"$QEMU_LOG") + 1 ))
printf 'QEMU_CURRENT_BOOT_START_LINE=%s\n' "$qemu_start_line"

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
  printf 'EMULATOR_START_VERDICT=fail console=%q\n' "$(tail -20 "$CONSOLE")"
  exit 1
fi
printf 'EMULATOR_START_VERDICT=pass\n'

connected=0
for attempt in $(seq 1 80); do
  HDC_PORT=10000 timeout 30 "$CONNECT_HELPER" || true
  targets="$($HDC list targets -v || true)"
  shell_probe="$(hdc shell "echo e0-connect-$attempt" 2>&1 || true)"
  distribution="$(hdc shell 'param get const.product.os.dist.name' 2>&1 | tr -d '\r' || true)"
  printf 'CONNECTIVITY attempt=%s targets=%q shell=%q distribution=%q\n' \
    "$attempt" "$targets" "$shell_probe" "$distribution"
  if [[ "$shell_probe" == "e0-connect-$attempt" && "$distribution" == *HarmonyOS* ]]; then
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
  shell_readiness="$(hdc shell "echo e0-readiness-$boot_attempt" 2>&1 || true)"
  printf 'BOOT_READINESS attempt=%s qemu.boot=%q shell=%q\n' \
    "$boot_attempt" "$qemu_boot_complete" "$shell_readiness"
  if [[ -n "$qemu_boot_complete" && "$shell_readiness" == "e0-readiness-$boot_attempt" ]]; then
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
hdc file send "$APP_HAP" "$STAGING/entry-default-unsigned.hap"
hdc file send "$TEST_HAP" "$STAGING/entry-ohosTest-unsigned.hap"
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
  printf 'INSTALL_VERDICT=fail\n'
  exit 1
fi
printf 'INSTALL_VERDICT=pass\n'

wakeup_output="$(hdc shell 'power-shell wakeup' 2>&1 | tr -d '\r')"
home_output="$(hdc shell 'uitest uiInput keyEvent Home' 2>&1 | tr -d '\r')"
swipe_output="$(hdc shell 'uitest uiInput swipe 660 2500 660 500 2000' 2>&1 | tr -d '\r')"
printf 'UNLOCK_SEQUENCE wakeup=%q home=%q swipe=%q\n' "$wakeup_output" "$home_output" "$swipe_output"
sleep 2

hdc shell 'hilog -r'
for run in 1 2 3; do
  if (( run > 1 )); then
    hdc shell "aa force-stop $BUNDLE"
  fi
  wait_for_process_absent
  before_pid="$(get_pid)"
  printf 'COLD_START_%s_BEFORE_PID=%q\n' "$run" "$before_pid"
  aa_output="$(hdc shell "aa start -a $ABILITY -b $BUNDLE -m $MODULE" 2>&1)"
  printf 'COLD_START_%s_AA_OUTPUT=%q\n' "$run" "$aa_output"
  if [[ "$aa_output" == *"error:"* || "$aa_output" == *"Error Code:"* ||
        "$aa_output" != *"start ability successfully."* ]]; then
    printf 'COLD_START_%s_AA_SEMANTIC=fail\n' "$run"
    exit 1
  fi
  printf 'COLD_START_%s_AA_SEMANTIC=pass\n' "$run"
  pid="$(wait_for_process_present || true)"
  if [[ -z "$pid" ]]; then
    printf 'COLD_START_%s_PID=missing\n' "$run"
    exit 1
  fi
  for old_pid in "${pids[@]}"; do
    if [[ "$pid" == "$old_pid" ]]; then
      printf 'COLD_START_%s_PID_REUSED=%s\n' "$run" "$pid"
      exit 1
    fi
  done
  pids+=("$pid")
  printf 'COLD_START_%s_PID=%s\n' "$run" "$pid"

  marker_ready=0
  for marker_attempt in $(seq 1 20); do
    tag_snapshot="$(hdc shell 'hilog -x -T R1Api24Probe -v year -v zone' 2>&1 || true)"
    node_marker_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
      '$4 == pid && index($0, "Node-API result ping=pong version=r1-api24-probe/0.0.1") { count++ } END { print count + 0 }')"
    printf 'COLD_START_%s_MARKER_ATTEMPT=%s PID=%s PID_NODE_MARKER_COUNT=%s\n' \
      "$run" "$marker_attempt" "$pid" "$node_marker_count"
    if (( node_marker_count >= 1 )); then
      marker_ready=1
      break
    fi
    sleep 1
  done
  if (( marker_ready != 1 )); then
    printf 'COLD_START_%s_NODE_API_MARKER=missing\n' "$run"
    exit 1
  fi
  sleep 3
  screen_output="$(hdc shell 'uitest screenCap' 2>&1 | tr -d '\r')"
  printf 'COLD_START_%s_SCREEN_OUTPUT=%q\n' "$run" "$screen_output"
  remote_screen="$(printf '%s\n' "$screen_output" | sed -n 's/^ScreenCap saved to //p' | tail -1)"
  if [[ -z "$remote_screen" ]]; then
    printf 'COLD_START_%s_SCREEN=missing\n' "$run"
    exit 1
  fi
  local_screen="$RAW/$EVIDENCE_ID-run${run}.png"
  timeout 30 "$HDC" -t "$TARGET" file recv "$remote_screen" "$local_screen"
  hdc shell "rm -f $remote_screen"
  file "$local_screen"
  sha256sum "$local_screen"
  pixel_rgb="$(ffmpeg -v error -i "$local_screen" -vf 'crop=1:1:100:1000,format=rgb24' -frames:v 1 -f rawvideo - | od -An -tu1 | xargs)"
  yavg="$(ffmpeg -hide_banner -i "$local_screen" -vf signalstats,metadata=print -frames:v 1 -f null - 2>&1 | sed -n 's/.*lavfi.signalstats.YAVG=//p' | tail -1)"
  printf 'COLD_START_%s_PIXEL_RGB_100_1000=%s\n' "$run" "$pixel_rgb"
  printf 'COLD_START_%s_YAVG=%s\n' "$run" "$yavg"
  read -r red green blue <<<"$pixel_rgb"
  if [[ -z "$yavg" ]] || ! awk -v value="$yavg" 'BEGIN { exit !(value > 32.0) }'; then
    printf 'COLD_START_%s_NONBLACK=fail\n' "$run"
    exit 1
  fi
  if (( red < 235 || red > 249 || green < 239 || green > 252 || blue < 241 || blue > 254 )); then
    printf 'COLD_START_%s_APP_BACKGROUND_PIXEL=fail\n' "$run"
    exit 1
  fi
  printf 'COLD_START_%s_PIXEL_VERDICT=pass\n' "$run"
done

printf 'COLD_START_PIDS=%s,%s,%s\n' "${pids[0]}" "${pids[1]}" "${pids[2]}"
bm_dump="$(hdc shell "bm dump -n $BUNDLE" | sed -E \
  -e 's/("accessTokenId": )[0-9]+/\1[RUNTIME_ACCESS_TOKEN_ID_REDACTED]/g' \
  -e 's/("accessTokenIdEx": )[0-9]+/\1[RUNTIME_ACCESS_TOKEN_ID_EX_REDACTED]/g')"
printf '%s\n' "$bm_dump"
if [[ "$bm_dump" != *'"appPrivilegeLevel": "normal"'* ||
      "$bm_dump" != *'"isSystemApp": false'* ||
      "$bm_dump" != *'"mainElementName": "EntryAbility"'* ]]; then
  printf 'NORMAL_ENTRY_ABILITY_AUDIT=fail\n'
  exit 1
fi
printf 'NORMAL_ENTRY_ABILITY_AUDIT=pass\n'
hdc shell "aa force-stop $BUNDLE"
wait_for_process_absent
printf 'POST_FORCE_STOP_PID=%q\n' "$(get_pid)"

side_output="$(timeout 60 "$HDC" -t "$TARGET" shell "aa test -b $BUNDLE -m entry_test -s unittest /ets/testrunner/OpenHarmonyTestRunner -s timeout 15000" 2>&1)"
printf '%s\n' "$side_output"
if [[ "$side_output" != *'BASELINE_RESULT|functional=PASS|ping=pong|version=r1-api24-probe/0.0.1'* ||
      "$side_output" != *'TestFinished-ResultCode: 0'* ]]; then
  printf 'TEST_HAP_SIDECAR=fail\n'
  exit 1
fi
printf 'TEST_HAP_SIDECAR=pass_non_substitute\n'

hdc shell 'hilog -x -t app -v year -v zone' >"$FULL_HILOG"
hdc shell 'hilog -x -T R1Api24Probe -v year -v zone' >"$TAG_HILOG"
for marker in 'EntryAbility onCreate observed=true' 'EntryAbility windowStage onWindowStageCreate' \
  'EntryAbility foreground onForeground' 'Node-API result ping=pong version=r1-api24-probe/0.0.1'; do
  marker_count="$(grep -F -c "$marker" "$TAG_HILOG" || true)"
  printf 'MARKER_COUNT marker=%q count=%s\n' "$marker" "$marker_count"
  if (( marker_count < 3 )); then
    printf 'LIFECYCLE_MARKERS=fail\n'
    exit 1
  fi
done
printf 'LIFECYCLE_MARKERS=pass\n'

hdc shell "rm -rf $STAGING"
timeout 120 "$HDC" -t "$TARGET" uninstall "$BUNDLE"
installed=0
post_uninstall="$(hdc shell "bm dump -n $BUNDLE" 2>&1 || true)"
printf 'POST_UNINSTALL_BM=%q\n' "$post_uninstall"
if [[ "$post_uninstall" != *'failed to get information'* ]]; then
  printf 'UNINSTALL_ABSENCE=fail\n'
  exit 1
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
  printf 'FINAL_RESIDUAL_PROCESS=%s\n' "$([[ -n "$residual_processes" ]] && printf true || printf false)"
  printf 'FINAL_RESIDUAL_PORT=%s\n' "$([[ -n "$residual_ports" ]] && printf true || printf false)"
  exit 1
fi
printf 'FINAL_RESIDUAL_PROCESS=false\n'
printf 'FINAL_RESIDUAL_PORT=false\n'

result="pass"
cleaned=1
ended_at="$(date --iso-8601=seconds)"
printf 'ENDED_AT=%s\n' "$ended_at"
printf 'VERDICT=pass\n'
printf 'RECORD_STATUS=collected\n'
