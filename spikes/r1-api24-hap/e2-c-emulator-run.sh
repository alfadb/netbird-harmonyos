#!/usr/bin/env bash
set -Eeuo pipefail

readonly EVIDENCE_ID="EV-E2-EMU24-20260717-0002"
readonly WORKSPACE="/home/worker/work/base/netbird-harmonyos"
readonly PROJECT="$WORKSPACE/spikes/r1-api24-hap"
readonly RAW="$WORKSPACE/docs/evidence/raw"
readonly APP_HAP="$PROJECT/entry/build/default/outputs/default/entry-default-unsigned.hap"
readonly BUNDLE="cn.alfadb.netbird.r1probe"
readonly MODULE="entry"
readonly ABILITY="EntryAbility"
readonly INSTANCE="netbird_api24_phone"
readonly TARGET="127.0.0.1:10000"
readonly HOST_TCP_PORT="39021"
readonly HOST_UDP_PORT="39022"
readonly STAGING="/data/local/tmp/e2-c-emu24-20260717-0002"
readonly STABLE_TOOLS="/home/worker/harmonyos/command-line-tools/6.1.1.290"
readonly BETA_TOOLS="/home/worker/harmonyos/command-line-tools/26.0.0.461"
readonly HVIGOR="$STABLE_TOOLS/bin/hvigorw"
readonly OHPM="$STABLE_TOOLS/bin/ohpm"
readonly SDK="$BETA_TOOLS/sdk/default/openharmony"
readonly NAPI_HEADER="$SDK/native/sysroot/usr/include/napi/native_api.h"
readonly EMULATOR="$BETA_TOOLS/emulator/Emulator"
readonly HDC="$SDK/toolchains/hdc"
readonly CONNECT_HELPER="/home/worker/harmonyos/bin/emulator-connect"
readonly STOP_HELPER="/home/worker/harmonyos/bin/emulator-stop"
readonly QEMU_LOG="/home/worker/harmonyos/emulator-instances/netbird_api24_phone/Log/qemu.log"
readonly HOST_BINARY="/tmp/$EVIDENCE_ID-host-server"
readonly TRANSCRIPT="$RAW/$EVIDENCE_ID-transcript.log"
readonly CONSOLE="$RAW/$EVIDENCE_ID-emulator-console.log"
readonly TAG_HILOG="$RAW/$EVIDENCE_ID-hilog-tag.log"
readonly BUILD_LOG="$RAW/$EVIDENCE_ID-build.log"
readonly HOST_LOG="$RAW/$EVIDENCE_ID-host-server.log"
readonly FAULT_LIST="$RAW/$EVIDENCE_ID-fault-list.log"
readonly SOURCE_MANIFEST="$RAW/$EVIDENCE_ID-source-manifest.txt"
readonly SOURCE_ARCHIVE="$RAW/$EVIDENCE_ID-source.tar"
readonly HAP_ARCHIVE="$RAW/$EVIDENCE_ID-application-hap.bin"
readonly E1_LIB_ARCHIVE="$RAW/$EVIDENCE_ID-libprobe-x86_64.bin"
readonly E2_LIB_ARCHIVE="$RAW/$EVIDENCE_ID-libe2network-x86_64.bin"
readonly HOST_BINARY_ARCHIVE="$RAW/$EVIDENCE_ID-host-server.bin"

mkdir -p "$RAW"
exec > >(tee "$TRANSCRIPT") 2>&1

started_at="$(date --iso-8601=seconds)"
emulator_started=0
host_started=0
host_completed=0
host_pid=''
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

stop_host_server() {
  if (( host_started == 1 && host_completed == 0 )) && [[ -n "$host_pid" ]]; then
    kill "$host_pid" 2>/dev/null || true
    wait "$host_pid" 2>/dev/null || true
  fi
  host_started=0
  rm -f "$HOST_BINARY"
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
    stop_host_server
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
printf 'RECORD_SCOPE=E2_C-network_ordinary_EntryAbility_with_E1_regression_no_TestRunner_no_Go_no_NetBird_no_PS4\n'
printf 'TARGET_TUPLE=HarmonyOS_6.1.1(24),software_6.1.0.125,phone_Emulator,x86_64,SDK_API24,Beta_HDC_3.2.0e,unsigned_debug_normal_app\n'
printf 'HOST_SCOPE=Pod_local_temporary_C_server_loopback_bind_only_via_Emulator_NAT\n'
printf 'TEST_RUNNER_USED=false\n'
printf 'PHYSICAL_DEVICE_USED=false\n'
printf 'GO_NETBIRD_PS4_TOUCHED=false\n'
printf 'PUBLIC_NETWORK_ALLOWED=false\n'

rm -f "$CONSOLE" "$TAG_HILOG" "$BUILD_LOG" "$HOST_LOG" "$FAULT_LIST" \
  "$SOURCE_MANIFEST" "$SOURCE_ARCHIVE" "$HAP_ARCHIVE" "$E1_LIB_ARCHIVE" \
  "$E2_LIB_ARCHIVE" "$HOST_BINARY_ARCHIVE" "$RAW/$EVIDENCE_ID"-run*.png \
  "$RAW/$EVIDENCE_ID"-run*-hilog-full.log

run cc -std=c17 -O2 -Wall -Wextra -Werror "$PROJECT/e2-host-server.c" -o "$HOST_BINARY"
run install -m 0755 "$HOST_BINARY" "$HOST_BINARY_ARCHIVE"
run file "$HOST_BINARY_ARCHIVE"
run sha256sum "$PROJECT/e2-host-server.c" "$HOST_BINARY_ARCHIVE"

stdbuf -oL -eL "$HOST_BINARY" >"$HOST_LOG" 2>&1 &
host_pid=$!
host_started=1
printf 'HOST_SERVER_PID=%s\n' "$host_pid"
host_ready=0
for attempt in $(seq 1 50); do
  if grep -F 'HOST_SERVER_READY|' "$HOST_LOG" >/dev/null 2>&1; then
    host_ready=1
    break
  fi
  if ! kill -0 "$host_pid" 2>/dev/null; then
    printf '%s\n' "$(<"$HOST_LOG")"
    fail "host server exited before readiness"
  fi
  sleep 0.1
done
(( host_ready == 1 )) || fail "host server readiness marker timeout"

host_tcp_listen="$(ss -ltnp | grep -E ":$HOST_TCP_PORT[[:space:]]" || true)"
host_udp_listen="$(ss -lunp | grep -E ":$HOST_UDP_PORT[[:space:]]" || true)"
printf 'HOST_TCP_LISTEN=%q\n' "$host_tcp_listen"
printf 'HOST_UDP_LISTEN=%q\n' "$host_udp_listen"
if [[ "$host_tcp_listen" != *"127.0.0.1:$HOST_TCP_PORT"* ||
      "$host_udp_listen" != *"127.0.0.1:$HOST_UDP_PORT"* ||
      "$host_tcp_listen" == *"0.0.0.0:$HOST_TCP_PORT"* ||
      "$host_udp_listen" == *"0.0.0.0:$HOST_UDP_PORT"* ]]; then
  fail "host service is not restricted to Pod loopback"
fi
printf 'HOST_LOOPBACK_BIND_AUDIT=pass\n'

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
unzip -p "$APP_HAP" libs/x86_64/libprobe.so >"$E1_LIB_ARCHIVE"
unzip -p "$APP_HAP" libs/x86_64/libe2network.so >"$E2_LIB_ARCHIVE"
run sha256sum "$APP_HAP" "$HAP_ARCHIVE" "$E1_LIB_ARCHIVE" "$E2_LIB_ARCHIVE" \
  "$HOST_BINARY_ARCHIVE" "$BUILD_LOG" "$NAPI_HEADER"
run stat -c '%n size=%s mtime=%y' "$APP_HAP" "$HAP_ARCHIVE" "$E1_LIB_ARCHIVE" \
  "$E2_LIB_ARCHIVE" "$HOST_BINARY_ARCHIVE" "$BUILD_LOG"
run unzip -Z1 "$APP_HAP"
if unzip -Z1 "$APP_HAP" | grep -E 'libgoprobe|libtls-|entry_test|ohosTest' >/dev/null; then
  fail "forbidden Go, TLS, or TestRunner member in application HAP"
fi
printf 'FORBIDDEN_HISTORICAL_MEMBER=false\n'
unzip -p "$APP_HAP" module.json

run file "$E1_LIB_ARCHIVE" "$E2_LIB_ARCHIVE"
run readelf -d "$E2_LIB_ARCHIVE"
for symbol in socket bind listen connect accept4 epoll_create1 epoll_ctl epoll_wait poll \
  getaddrinfo freeaddrinfo getifaddrs freeifaddrs send recv sendto recvfrom close; do
  if ! readelf -Ws "$E2_LIB_ARCHIVE" | grep -F "$symbol" >/dev/null; then
    fail "required E2 native symbol missing: $symbol"
  fi
done
printf 'PUBLIC_C_NETWORK_SYMBOL_AUDIT=pass\n'

source_inputs=(
  spikes/r1-api24-hap/e2-c-emulator-run.sh
  spikes/r1-api24-hap/e2-host-server.c
  spikes/r1-api24-hap/entry/src/main/cpp/CMakeLists.txt
  spikes/r1-api24-hap/entry/src/main/cpp/e1_c_probe.cpp
  spikes/r1-api24-hap/entry/src/main/cpp/e2_c_network.cpp
  spikes/r1-api24-hap/entry/src/main/cpp/types/libprobe/index.d.ts
  spikes/r1-api24-hap/entry/src/main/cpp/types/libe2network/index.d.ts
  spikes/r1-api24-hap/entry/src/main/cpp/types/libe2network/oh-package.json5
  spikes/r1-api24-hap/entry/src/main/ets/entryability/EntryAbility.ets
  spikes/r1-api24-hap/entry/src/main/ets/pages/Index.ets
  spikes/r1-api24-hap/entry/src/main/module.json5
  spikes/r1-api24-hap/entry/oh-package.json5
  spikes/r1-api24-hap/entry/oh-package-lock.json5
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
run "$OHPM" --version

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
  shell_probe="$(hdc shell "echo e2-c-connect-$attempt" 2>&1 || true)"
  distribution="$(hdc shell 'param get const.product.os.dist.name' 2>&1 | tr -d '\r' || true)"
  printf 'CONNECTIVITY attempt=%s targets=%q shell=%q distribution=%q\n' \
    "$attempt" "$targets" "$shell_probe" "$distribution"
  if [[ "$shell_probe" == "e2-c-connect-$attempt" && "$distribution" == *HarmonyOS* ]]; then
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
  shell_readiness="$(hdc shell "echo e2-c-readiness-$boot_attempt" 2>&1 || true)"
  printf 'BOOT_READINESS attempt=%s qemu.boot=%q shell=%q\n' \
    "$boot_attempt" "$qemu_boot_complete" "$shell_readiness"
  if [[ -n "$qemu_boot_complete" && "$shell_readiness" == "e2-c-readiness-$boot_attempt" ]]; then
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
  fail "bundle existed before E2 installation"
fi

guest_route="$(hdc shell 'cat /proc/net/route' 2>&1 | tr -d '\r')"
printf 'GUEST_PROC_NET_ROUTE_BEGIN\n%s\nGUEST_PROC_NET_ROUTE_END\n' "$guest_route"
if ! printf '%s\n' "$guest_route" | awk 'NR > 1 && $1 != "lo" && $2 != "00000000" && $8 != "00000000" { found=1 } END { exit !found }'; then
  fail "guest has no objective non-loopback directly connected route"
fi
printf 'GUEST_ROUTE_DISCOVERY=pass_source_proc_net_route_no_ip_command_dependency\n'

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

{
  printf '===== BEFORE MEASURED RUNS =====\n'
  hdc shell 'ls -la /data/log/faultlog/faultlogger 2>&1' || true
} >"$FAULT_LIST"
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
  for marker_attempt in $(seq 1 120); do
    tag_snapshot="$(hdc shell 'hilog -x -T R1Api24Probe -v year -v zone' 2>&1 || true)"
    e2_pass_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
      '$4 == pid && index($0, "E2_C_PROBE_RESULT|verdict=PASS") { count++ } END { print count + 0 }')"
    e2_fail_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
      '$4 == pid && index($0, "E2_C_PROBE_RESULT|verdict=FAIL") { count++ } END { print count + 0 }')"
    e1_fail_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
      '$4 == pid && index($0, "E1_C_PROBE_RESULT|verdict=FAIL") { count++ } END { print count + 0 }')"
    printf 'COLD_START_%s_MARKER_ATTEMPT=%s PID=%s E2_PASS=%s E2_FAIL=%s E1_FAIL=%s\n' \
      "$run_number" "$marker_attempt" "$pid" "$e2_pass_count" "$e2_fail_count" "$e1_fail_count"
    if (( e2_fail_count > 0 || e1_fail_count > 0 )); then
      printf '%s\n' "$tag_snapshot"
      fail "application reported E1 or E2 FAIL on run $run_number"
    fi
    if (( e2_pass_count == 1 )); then
      marker_ready=1
      break
    fi
    sleep 1
  done
  if (( marker_ready != 1 )); then
    printf '%s\n' "$tag_snapshot"
    fail "application E2 PASS marker timeout on run $run_number"
  fi

  e1_pass_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_C_PROBE_RESULT|verdict=PASS") { count++ } END { print count + 0 }')"
  e1_round_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_ROUND_RESULT|verdict=PASS") { count++ } END { print count + 0 }')"
  e1_buffer_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_BUFFER_SAMPLE|") { count++ } END { print count + 0 }')"
  e1_callback_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_CALLBACK_SAMPLE|") && index($0, "|valid=true") { count++ } END { print count + 0 }')"
  e1_fd_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E1_FD_RESULT|") { count++ } END { print count + 0 }')"
  e2_round_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E2_ROUND_RESULT|verdict=PASS") { count++ } END { print count + 0 }')"
  tcp_loop_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E2_TCP_LOOPBACK|verdict=PASS") { count++ } END { print count + 0 }')"
  udp_loop_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E2_UDP_LOOPBACK|verdict=PASS") { count++ } END { print count + 0 }')"
  udp_datagram_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E2_UDP_DATAGRAM|") && index($0, "|valid=true") { count++ } END { print count + 0 }')"
  host_route_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E2_HOST_ROUTE|verdict=PASS") { count++ } END { print count + 0 }')"
  host_tcp_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E2_HOST_TCP|verdict=PASS") { count++ } END { print count + 0 }')"
  host_udp_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E2_HOST_UDP|verdict=PASS") { count++ } END { print count + 0 }')"
  host_udp_datagram_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E2_HOST_UDP_DATAGRAM|") && index($0, "|valid=true") { count++ } END { print count + 0 }')"
  dns_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E2_DNS|verdict=PASS") && index($0, "|publicDnsUsed=false") { count++ } END { print count + 0 }')"
  event_count="$(printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && index($0, "E2_EVENT_PATHS|verdict=PASS") { count++ } END { print count + 0 }')"
  printf 'COLD_START_%s_COUNTS e1Pass=%s e1Rounds=%s e1Buffers=%s e1Callbacks=%s e1Fd=%s e2Rounds=%s tcpLoop=%s udpLoop=%s udpDatagrams=%s hostRoutes=%s hostTcp=%s hostUdp=%s hostUdpDatagrams=%s dns=%s events=%s\n' \
    "$run_number" "$e1_pass_count" "$e1_round_count" "$e1_buffer_count" "$e1_callback_count" \
    "$e1_fd_count" "$e2_round_count" "$tcp_loop_count" "$udp_loop_count" "$udp_datagram_count" \
    "$host_route_count" "$host_tcp_count" "$host_udp_count" "$host_udp_datagram_count" "$dns_count" "$event_count"
  if (( e1_pass_count != 1 || e1_round_count != 10 || e1_buffer_count != 1000 ||
        e1_callback_count != 1000 || e1_fd_count != 10 || e2_round_count != 10 ||
        tcp_loop_count != 10 || udp_loop_count != 10 || udp_datagram_count != 60 ||
        host_route_count != 10 || host_tcp_count != 10 || host_udp_count != 10 ||
        host_udp_datagram_count != 30 || dns_count != 10 || event_count != 10 )); then
    fail "per-PID E1 regression or E2 evidence count mismatch on run $run_number"
  fi
  if printf '%s\n' "$tag_snapshot" | awk -v pid="$pid" \
    '$4 == pid && /E2_ROUND_RESULT/ && ($0 !~ /fdLeak=false/ || $0 !~ /threadLeak=false/) { bad=1 } END { exit !bad }'; then
    fail "E2 resource leak marker detected on run $run_number"
  fi
  printf 'COLD_START_%s_E1_REGRESSION=pass\n' "$run_number"
  printf 'COLD_START_%s_E2_COUNTS=pass\n' "$run_number"

  host_tcp_pid_count="$(grep -F -c "HOST_TCP|" "$HOST_LOG" | tr -d ' ' || true)"
  host_tcp_this_pid="$(grep -F 'HOST_TCP|' "$HOST_LOG" | grep -F -c "|pid=$pid|" || true)"
  host_udp_this_pid="$(grep -F 'HOST_UDP|' "$HOST_LOG" | grep -F -c "|pid=$pid|" || true)"
  printf 'COLD_START_%s_HOST_COUNTS totalTcp=%s pidTcp=%s pidUdp=%s\n' \
    "$run_number" "$host_tcp_pid_count" "$host_tcp_this_pid" "$host_udp_this_pid"
  if (( host_tcp_this_pid != 10 || host_udp_this_pid != 30 )); then
    fail "host raw server count mismatch for PID $pid"
  fi

  {
    printf '===== RUN %s PID %s TAG HILOG =====\n' "$run_number" "$pid"
    printf '%s\n' "$tag_snapshot"
  } >>"$TAG_HILOG"
  full_hilog="$RAW/$EVIDENCE_ID-run${run_number}-hilog-full.log"
  hdc shell 'hilog -x -v year -v zone' >"$full_hilog"
  printf 'COLD_START_%s_FULL_HILOG_LINES=%s\n' "$run_number" "$(wc -l <"$full_hilog")"
  run sha256sum "$full_hilog"
  if grep -E "E1_C_PROBE_RESULT\\|verdict=FAIL|E2_C_PROBE_RESULT\\|verdict=FAIL|E2_NATIVE_ERROR" "$full_hilog" >/dev/null; then
    fail "failure marker exists in unfiltered HiLog on run $run_number"
  fi

  {
    printf '===== AFTER RUN %s PID %s =====\n' "$run_number" "$pid"
    hdc shell 'ls -la /data/log/faultlog/faultlogger 2>&1' || true
  } >>"$FAULT_LIST"

  if ! kill_probe="$(get_pid)" || [[ "$kill_probe" != "$pid" ]]; then
    fail "application process exited before screenshot on run $run_number"
  fi
  sleep 1
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
  printf 'COLD_START_%s_VISIBLE_E2_PASS_PAGE=pass_marker_bound_screenshot\n' "$run_number"
done

printf 'COLD_START_PIDS=%s,%s,%s\n' "${pids[0]}" "${pids[1]}" "${pids[2]}"

if wait "$host_pid"; then
  host_exit=0
else
  host_exit=$?
fi
host_completed=1
host_started=0
printf 'HOST_SERVER_EXIT=%s\n' "$host_exit"
if (( host_exit != 0 )); then
  printf '%s\n' "$(<"$HOST_LOG")"
  fail "host server exited with failure"
fi
host_result_count="$(grep -F -c 'HOST_SERVER_RESULT|verdict=PASS|tcp=30|udp=90|pids=3' "$HOST_LOG" || true)"
host_pid_summary_count="$(grep -F 'HOST_PID_SUMMARY|' "$HOST_LOG" | grep -F -c '|verdict=PASS' || true)"
host_error_count="$(grep -F -c 'HOST_SERVER_ERROR|' "$HOST_LOG" || true)"
printf 'HOST_SERVER_COUNTS result=%s pidSummaries=%s errors=%s tcp=%s udp=%s\n' \
  "$host_result_count" "$host_pid_summary_count" "$host_error_count" \
  "$(grep -F -c 'HOST_TCP|' "$HOST_LOG" || true)" "$(grep -F -c 'HOST_UDP|' "$HOST_LOG" || true)"
if (( host_result_count != 1 || host_pid_summary_count != 3 || host_error_count != 0 )); then
  fail "host server final evidence mismatch"
fi
printf 'HOST_SERVER_VERDICT=pass\n'

run sha256sum "$TAG_HILOG" "$HOST_LOG" "$FAULT_LIST" "$CONSOLE" "$RAW/$EVIDENCE_ID"-run*.png

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
  residual_ports="$(ss -ltnup | grep -E ":(10000|$HOST_TCP_PORT|$HOST_UDP_PORT|5555|8710)[[:space:]]" || true)"
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
  fail "residual Emulator, HDC, host service, or tested port after cleanup"
fi
printf 'FINAL_RESIDUAL_PROCESS=false\n'
printf 'FINAL_RESIDUAL_PORT=false\n'

run bash -n "$PROJECT/e2-c-emulator-run.sh"
run git -C "$WORKSPACE" diff --check
run markdownlint-cli2 "$WORKSPACE/README.md" "$WORKSPACE/docs/**/*.md" "$PROJECT/README.md"
if rg -n -i -e 'authorization:[[:space:]]*bearer[[:space:]]+[A-Za-z0-9._~-]+' \
    -e '-----BEGIN ([A-Z ]+ )?PRIVATE KEY-----' -e 'setup[_ -]?key[=:][[:space:]]*[A-Za-z0-9._~-]{8,}' \
    "$TRANSCRIPT" "$HOST_LOG" "$FAULT_LIST" "$TAG_HILOG" "$RAW/$EVIDENCE_ID"-run*-hilog-full.log; then
  fail "high-confidence sensitive material pattern detected"
fi
printf 'SENSITIVE_SCAN=pass_high_confidence_patterns\n'
if rg -n '(114\.114\.114\.114|8\.8\.8\.8|https?://)' \
    "$PROJECT/e2-c-emulator-run.sh" "$PROJECT/e2-host-server.c" \
    "$PROJECT/entry/src/main/cpp/e2_c_network.cpp"; then
  fail "public endpoint literal exists in E2 executable inputs"
fi
printf 'PUBLIC_ENDPOINT_SOURCE_SCAN=pass\n'

rm -f "$HOST_BINARY"
result="pass"
cleaned=1
ended_at="$(date --iso-8601=seconds)"
printf 'ENDED_AT=%s\n' "$ended_at"
printf 'VERDICT=pass\n'
printf 'RECORD_STATUS=collected\n'
