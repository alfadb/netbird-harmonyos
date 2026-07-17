#!/usr/bin/env bash
set -Eeuo pipefail

readonly EVIDENCE_ID="EV-E3-EMU24-20260717-0003"
readonly WORKSPACE="/home/worker/work/base/netbird-harmonyos"
readonly PROJECT="$WORKSPACE/spikes/e3-vpn-extension-hap"
readonly RAW="$WORKSPACE/docs/evidence/raw"
readonly PRODUCT_A_HAP="$PROJECT/entry/build/default/outputs/default/entry-default-unsigned.hap"
readonly PRODUCT_B_HAP="$PROJECT/entry/build/vpnB/outputs/default/entry-default-unsigned.hap"
readonly A_HAP_ARCHIVE="$RAW/$EVIDENCE_ID-application-a-hap.bin"
readonly B_HAP_ARCHIVE="$RAW/$EVIDENCE_ID-application-b-hap.bin"
readonly BUNDLE_A="cn.alfadb.netbird.e3vpna"
readonly BUNDLE_B="cn.alfadb.netbird.e3vpnb"
readonly MODULE="entry"
readonly ENTRY_ABILITY="EntryAbility"
readonly INSTANCE="netbird_api24_phone"
readonly TARGET="127.0.0.1:10000"
readonly STAGING="/data/local/tmp/e3-vpn-extension-20260717-0003"
readonly STABLE_TOOLS="/home/worker/harmonyos/command-line-tools/6.1.1.290"
readonly BETA_TOOLS="/home/worker/harmonyos/command-line-tools/26.0.0.461"
readonly HVIGOR="$STABLE_TOOLS/bin/hvigorw"
readonly OHPM="$STABLE_TOOLS/bin/ohpm"
readonly SDK="$BETA_TOOLS/sdk/default/openharmony"
readonly HDC="$SDK/toolchains/hdc"
readonly EMULATOR="$BETA_TOOLS/emulator/Emulator"
readonly CONNECT_HELPER="/home/worker/harmonyos/bin/emulator-connect"
readonly STOP_HELPER="/home/worker/harmonyos/bin/emulator-stop"
readonly QEMU_LOG="/home/worker/harmonyos/emulator-instances/netbird_api24_phone/Log/qemu.log"
readonly TRANSCRIPT="$RAW/$EVIDENCE_ID-transcript.log"
readonly CONSOLE="$RAW/$EVIDENCE_ID-emulator-console.log"
readonly BUILD_A_LOG="$RAW/$EVIDENCE_ID-build-a.log"
readonly BUILD_B_LOG="$RAW/$EVIDENCE_ID-build-b.log"
readonly TAG_HILOG="$RAW/$EVIDENCE_ID-hilog-tag.log"
readonly FAULT_LIST="$RAW/$EVIDENCE_ID-fault-list.log"
readonly SOURCE_MANIFEST="$RAW/$EVIDENCE_ID-source-manifest.txt"
readonly SOURCE_ARCHIVE="$RAW/$EVIDENCE_ID-source.tar"
readonly SETTINGS_TEXT="$RAW/$EVIDENCE_ID-settings-main-text.tsv"

mkdir -p "$RAW"
rm -f "$RAW/$EVIDENCE_ID"-*
exec > >(tee "$TRANSCRIPT") 2>&1

started_at="$(date --iso-8601=seconds)"
emulator_started=0
installed_a=0
installed_b=0
cleaned=0
result="blocked"
a_pids=()
CAPTURED_LAYOUT=''
CAPTURED_HILOG=''
STARTED_PID=''

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
  local bundle="$1"
  local output
  output="$(hdc shell "pidof $bundle" 2>/dev/null | tr -d '\r' || true)"
  if [[ "$output" =~ ^[[:space:]]*([0-9]+) ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
  fi
}

wait_for_pid() {
  local bundle="$1"
  local attempt pid
  for attempt in $(seq 1 30); do
    pid="$(get_pid "$bundle")"
    if [[ -n "$pid" ]]; then
      printf '%s\n' "$pid"
      return 0
    fi
    sleep 1
  done
  return 1
}

wait_for_absence() {
  local bundle="$1"
  local attempt
  for attempt in $(seq 1 30); do
    if [[ -z "$(get_pid "$bundle")" ]]; then
      return 0
    fi
    sleep 1
  done
  return 1
}

stop_emulator() {
  if (( emulator_started == 1 )); then
    HDC_PORT=10000 timeout 90 "$STOP_HELPER" || true
    for _ in $(seq 1 30); do
      if ! pgrep -f '/[e]mulator/Emulator.*netbird_api24_phone|[q]emu-system.*netbird_api24_phone' >/dev/null; then
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
    hdc shell "aa force-stop $BUNDLE_A" || true
    hdc shell "aa force-stop $BUNDLE_B" || true
    hdc shell "rm -rf $STAGING" || true
    if (( installed_a == 1 )); then
      timeout 120 "$HDC" -t "$TARGET" uninstall "$BUNDLE_A" || true
    fi
    if (( installed_b == 1 )); then
      timeout 120 "$HDC" -t "$TARGET" uninstall "$BUNDLE_B" || true
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

capture_layout() {
  local name="$1"
  local remote="$STAGING/$name-layout.json"
  local local_path="$RAW/$EVIDENCE_ID-$name-layout.json"
  hdc shell "uitest dumpLayout -p $remote -i"
  timeout 30 "$HDC" -t "$TARGET" file recv "$remote" "$local_path"
  hdc shell "rm -f $remote"
  run jq -e . "$local_path" >/dev/null
  CAPTURED_LAYOUT="$local_path"
}

capture_screen() {
  local name="$1"
  local remote="$STAGING/$name.png"
  local local_path="$RAW/$EVIDENCE_ID-$name.png"
  hdc shell "uitest screenCap -p $remote"
  timeout 30 "$HDC" -t "$TARGET" file recv "$remote" "$local_path"
  hdc shell "rm -f $remote"
  run file "$local_path"
  run sha256sum "$local_path"
}

click_node_id() {
  local layout="$1"
  local node_id="$2"
  local bounds x1 y1 x2 y2 x y
  bounds="$(jq -r --arg id "$node_id" \
    '.. | objects | .attributes? | select(.id == $id and .clickable == "true") | .bounds' \
    "$layout" | head -n 1)"
  [[ "$bounds" =~ ^\[([0-9]+),([0-9]+)\]\[([0-9]+),([0-9]+)\]$ ]] ||
    fail "clickable node $node_id not found in $layout"
  x1="${BASH_REMATCH[1]}"
  y1="${BASH_REMATCH[2]}"
  x2="${BASH_REMATCH[3]}"
  y2="${BASH_REMATCH[4]}"
  x=$(( (x1 + x2) / 2 ))
  y=$(( (y1 + y2) / 2 ))
  printf 'UI_CLICK_NODE id=%s bounds=%s center=%s,%s\n' "$node_id" "$bounds" "$x" "$y"
  hdc shell "uitest uiInput click $x $y"
}

click_id_prefix() {
  local layout="$1"
  local prefix="$2"
  local bounds x1 y1 x2 y2 x y
  bounds="$(jq -r --arg prefix "$prefix" \
    '.. | objects | .attributes? |
     select(((.id? // "") | startswith($prefix)) and .clickable == "true" and .visible == "true") | .bounds' \
    "$layout" | head -n 1)"
  [[ "$bounds" =~ ^\[([0-9]+),([0-9]+)\]\[([0-9]+),([0-9]+)\]$ ]] ||
    fail "visible clickable ID prefix $prefix not found in $layout"
  x1="${BASH_REMATCH[1]}"
  y1="${BASH_REMATCH[2]}"
  x2="${BASH_REMATCH[3]}"
  y2="${BASH_REMATCH[4]}"
  x=$(( (x1 + x2) / 2 ))
  y=$(( (y1 + y2) / 2 ))
  printf 'UI_CLICK_ID_PREFIX prefix=%s bounds=%s center=%s,%s\n' "$prefix" "$bounds" "$x" "$y"
  hdc shell "uitest uiInput click $x $y"
}

start_entry() {
  local bundle="$1"
  local output pid
  output="$(hdc shell "aa start -a $ENTRY_ABILITY -b $bundle -m $MODULE" 2>&1)"
  printf 'ENTRY_START bundle=%s output=%q\n' "$bundle" "$output"
  [[ "$output" == *'start ability successfully.'* && "$output" != *'Error Code:'* ]] ||
    fail "EntryAbility start failed for $bundle"
  pid="$(wait_for_pid "$bundle" || true)"
  [[ -n "$pid" ]] || fail "EntryAbility PID missing for $bundle"
  STARTED_PID="$pid"
}

force_stop_for_cleanup() {
  local bundle="$1"
  local reason="$2"
  printf 'FORCE_STOP_CLEANUP bundle=%s reason=%s notUsedAsRevoke=true\n' "$bundle" "$reason"
  hdc shell "aa force-stop $bundle"
  wait_for_absence "$bundle" || fail "process remained after cleanup force-stop: $bundle"
}

capture_hilog() {
  local name="$1"
  local full="$RAW/$EVIDENCE_ID-$name-hilog-full.log"
  hdc shell 'hilog -x -v year -v zone' >"$full"
  {
    printf '===== %s TAG HILOG =====\n' "$name"
    hdc shell 'hilog -x -T E3VpnGate -v year -v zone'
  } >>"$TAG_HILOG"
  run sha256sum "$full"
  CAPTURED_HILOG="$full"
}

capture_faults() {
  local name="$1"
  {
    printf '===== %s =====\n' "$name"
    hdc shell 'ls -la /data/log/faultlog/faultlogger 2>&1' || true
  } >>"$FAULT_LIST"
}

assert_missing_authorization_path() {
  local name="$1"
  local full="$2"
  local layout="$3"
  local bundle="$4"
  local ui_count promise_count oncreate_count ondestroy_count missing_count
  ui_count="$(grep -F -c "UI_START|bundle=$bundle|" "$full" || true)"
  promise_count="$(grep -F -c "START_PROMISE|bundle=$bundle|" "$full" || true)"
  oncreate_count="$(grep -F -c 'VPN_ONCREATE|' "$full" || true)"
  ondestroy_count="$(grep -F -c 'VPN_ONDESTROY|' "$full" || true)"
  missing_count="$(grep -F 'com.huawei.hmos.vpndialog' "$full" |
    grep -E -c 'bundle not exist|failed: com\.huawei\.hmos\.vpndialog|ExplicitQueryExtension size:0' || true)"
  printf '%s_COUNTS uiStart=%s promiseSettled=%s onCreate=%s onDestroy=%s missingDialogComponent=%s\n' \
    "$name" "$ui_count" "$promise_count" "$oncreate_count" "$ondestroy_count" "$missing_count"
  (( ui_count == 1 )) || fail "$name did not contain exactly one UI_START"
  (( promise_count == 0 )) || fail "$name start promise unexpectedly settled"
  (( oncreate_count == 0 && ondestroy_count == 0 )) || fail "$name unexpectedly entered VPN lifecycle"
  (( missing_count > 0 )) || fail "$name lacks system missing-VPN-dialog evidence"
  if jq -e '.. | objects | .attributes? | select(.bundleName == "com.huawei.hmos.vpndialog")' \
    "$layout" >/dev/null; then
    fail "$name unexpectedly displayed the VPN authorization component"
  fi
  printf '%s_VERDICT=blocked systemAuthorizationDialogAbsent=true promiseObserved=pending noOnCreate=true\n' "$name"
}

printf 'EVIDENCE_ID=%s\n' "$EVIDENCE_ID"
printf 'STARTED_AT=%s\n' "$started_at"
printf 'CLOCK_SOURCE=host_CLOCK_REALTIME_date_iso_8601_seconds_and_guest_HiLog\n'
printf 'TIMEZONE=%s\n' "$(date +%Z%:z)"
printf 'GIT_HEAD=%s\n' "$(git -C "$WORKSPACE" rev-parse HEAD)"
printf 'RECORD_SCOPE=E3_public_ordinary_third_party_VPN_Extension_authorization_gate\n'
printf 'TARGET_TUPLE=HarmonyOS_6.1.1(24),software_6.1.0.125,phone_Emulator,x86_64,SDK_API24,unsigned_debug_normal_apps\n'
printf 'PHYSICAL_DEVICE_USED=false\n'
printf 'GO_NETBIRD_PS4_TOUCHED=false\n'
printf 'MANAGE_VPN_USED=false\n'
printf 'SYSTEM_DEBUG_ENTERPRISE_BYPASS_USED=false\n'
printf 'PERMISSION_GRANT_COMMAND_USED=false\n'
printf 'VPN_CONNECTION_API_USED=false\n'
printf 'API24_VPN_OBSERVER_CLAIMED=false\n'

: >"$TAG_HILOG"
: >"$FAULT_LIST"

(
  cd "$PROJECT"
  "$HVIGOR" clean --mode module -p product=default -p module=entry@default \
    -p buildMode=debug --no-daemon
  "$HVIGOR" assembleHap --mode module -p product=default -p module=entry@default \
    -p buildMode=debug --no-daemon
) 2>&1 | tee "$BUILD_A_LOG"
[[ -f "$PRODUCT_A_HAP" ]] || fail "clean A build did not produce a HAP"
run install -m 0644 "$PRODUCT_A_HAP" "$A_HAP_ARCHIVE"
printf 'CLEAN_BUILD_A=pass\n'

(
  cd "$PROJECT"
  "$HVIGOR" clean --mode module -p product=vpnB -p module=entry@default \
    -p buildMode=debug --no-daemon
  "$HVIGOR" assembleHap --mode module -p product=vpnB -p module=entry@default \
    -p buildMode=debug --no-daemon
) 2>&1 | tee "$BUILD_B_LOG"
[[ -f "$PRODUCT_B_HAP" ]] || fail "clean B build did not produce a HAP"
run install -m 0644 "$PRODUCT_B_HAP" "$B_HAP_ARCHIVE"
printf 'CLEAN_BUILD_B=pass\n'

run sha256sum "$A_HAP_ARCHIVE" "$B_HAP_ARCHIVE" "$BUILD_A_LOG" "$BUILD_B_LOG"
for archive in "$A_HAP_ARCHIVE" "$B_HAP_ARCHIVE"; do
  run unzip -Z1 "$archive"
  if unzip -Z1 "$archive" | grep -E '(^|/)libs/|ohosTest|entry_test|libgo|netbird|ps4' >/dev/null; then
    fail "forbidden native, test, Go, NetBird, or PS4 member in $archive"
  fi
  module_json="$(unzip -p "$archive" module.json)"
  printf 'PACKAGED_MODULE_JSON=%s\n' "$module_json"
  printf '%s\n' "$module_json" | jq -e \
    '.module.requestPermissions == [{"name":"ohos.permission.INTERNET"}] and
     (.module.extensionAbilities | length == 1) and
     .module.extensionAbilities[0].type == "vpn" and
     .module.extensionAbilities[0].name == "E3VpnExtensionAbility" and
     .module.extensionAbilities[0].exported == false' >/dev/null ||
    fail "packaged manifest violates ordinary E3 scope: $archive"
done
[[ "$(unzip -p "$A_HAP_ARCHIVE" module.json | jq -r '.app.bundleName')" == "$BUNDLE_A" ]] ||
  fail "A bundle identity mismatch"
[[ "$(unzip -p "$B_HAP_ARCHIVE" module.json | jq -r '.app.bundleName')" == "$BUNDLE_B" ]] ||
  fail "B bundle identity mismatch"
printf 'PACKAGED_SCOPE_AUDIT=pass twoNormalBundlesVpnTypeInternetOnlyNoNativeMembers\n'

executable_inputs=(
  "$PROJECT/entry/src/main/module.json5"
  "$PROJECT/entry/src/main/ets/entryability/EntryAbility.ets"
  "$PROJECT/entry/src/main/ets/vpnextensionability/E3VpnExtensionAbility.ets"
  "$PROJECT/entry/src/main/ets/pages/Index.ets"
)
if rg -n 'MANAGE_VPN|createVpnConnection|createVpnObserver|\.create\(|\.protect\(|\.destroy\(' \
  "${executable_inputs[@]}"; then
  fail "forbidden permission, observer, or VpnConnection call in executable E3 inputs"
fi
printf 'FORBIDDEN_API_SOURCE_SCAN=pass\n'

source_inputs=(
  spikes/e3-vpn-extension-hap/e3-vpn-extension-emulator-run.sh
  spikes/e3-vpn-extension-hap/hvigorfile.ts
  spikes/e3-vpn-extension-hap/oh-package.json5
  spikes/e3-vpn-extension-hap/hvigor/hvigor-config.json5
  spikes/e3-vpn-extension-hap/build-profile.json5
  spikes/e3-vpn-extension-hap/AppScope/app.json5
  spikes/e3-vpn-extension-hap/AppScope/resources/base/element/string.json
  spikes/e3-vpn-extension-hap/AppScope/resources/base/media/app_icon.svg
  spikes/e3-vpn-extension-hap/entry/hvigorfile.ts
  spikes/e3-vpn-extension-hap/entry/build-profile.json5
  spikes/e3-vpn-extension-hap/entry/oh-package.json5
  spikes/e3-vpn-extension-hap/entry/src/main/module.json5
  spikes/e3-vpn-extension-hap/entry/src/main/ets/entryability/EntryAbility.ets
  spikes/e3-vpn-extension-hap/entry/src/main/ets/vpnextensionability/E3VpnExtensionAbility.ets
  spikes/e3-vpn-extension-hap/entry/src/main/ets/pages/Index.ets
  spikes/e3-vpn-extension-hap/entry/src/main/resources/base/profile/main_pages.json
  spikes/e3-vpn-extension-hap/entry/src/main/resources/base/element/color.json
  spikes/e3-vpn-extension-hap/entry/src/main/resources/base/element/string.json
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

printf 'TOOLCHAIN_HOST=%q\n' "$(uname -a)"
run "$HVIGOR" --version
run "$HDC" -v
run node --version
run "$OHPM" --version

"$HDC" kill || true
timeout 90 "$STOP_HELPER" || true
if pgrep -f '/[e]mulator/Emulator.*netbird_api24_phone|[q]emu-system.*netbird_api24_phone' >/dev/null; then
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
kill -0 "$emulator_pid" 2>/dev/null || fail "Emulator exited during startup"

connected=0
for attempt in $(seq 1 80); do
  HDC_PORT=10000 timeout 30 "$CONNECT_HELPER" || true
  shell_probe="$(hdc shell "echo e3-connect-$attempt" 2>&1 || true)"
  distribution="$(hdc shell 'param get const.product.os.dist.name' 2>&1 | tr -d '\r' || true)"
  printf 'CONNECTIVITY attempt=%s shell=%q distribution=%q\n' "$attempt" "$shell_probe" "$distribution"
  if [[ "$shell_probe" == "e3-connect-$attempt" && "$distribution" == *HarmonyOS* ]]; then
    connected=1
    break
  fi
  sleep 3
done
(( connected == 1 )) || fail "Emulator HDC connectivity blocked"

ready=0
for attempt in $(seq 1 180); do
  qemu_boot="$(tail -n +"$qemu_start_line" "$QEMU_LOG" |
    grep -F 'guest os boot completed.' | tail -n 1 || true)"
  shell_probe="$(hdc shell "echo e3-ready-$attempt" 2>&1 || true)"
  printf 'BOOT_READINESS attempt=%s qemu=%q shell=%q\n' "$attempt" "$qemu_boot" "$shell_probe"
  if [[ -n "$qemu_boot" && "$shell_probe" == "e3-ready-$attempt" ]]; then
    ready=1
    break
  fi
  sleep 1
done
(( ready == 1 )) || fail "Emulator did not reach current-boot readiness"
printf 'READINESS_VERDICT=pass\n'

for bundle in "$BUNDLE_A" "$BUNDLE_B"; do
  preinstall="$(hdc shell "bm dump -n $bundle" 2>&1 || true)"
  printf 'PREINSTALL bundle=%s result=%q\n' "$bundle" "$preinstall"
  [[ "$preinstall" == *'failed to get information'* ]] || fail "bundle pre-existed: $bundle"
done

hdc shell "rm -rf $STAGING && mkdir -p $STAGING/a $STAGING/b"
hdc file send "$A_HAP_ARCHIVE" "$STAGING/a/a.hap"
hdc file send "$B_HAP_ARCHIVE" "$STAGING/b/b.hap"
install_a="$(hdc shell "bm install -p $STAGING/a" 2>&1)"
printf 'INSTALL_A=%q\n' "$install_a"
[[ "$install_a" == *'install bundle successfully.'* ]] || fail "A install failed"
installed_a=1
install_b="$(hdc shell "bm install -p $STAGING/b" 2>&1)"
printf 'INSTALL_B=%q\n' "$install_b"
[[ "$install_b" == *'install bundle successfully.'* ]] || fail "B install failed"
installed_b=1

wakeup="$(hdc shell 'power-shell wakeup' 2>&1 | tr -d '\r')"
home="$(hdc shell 'uitest uiInput keyEvent Home' 2>&1 | tr -d '\r')"
swipe="$(hdc shell 'uitest uiInput swipe 660 2500 660 500 2000' 2>&1 | tr -d '\r')"
printf 'UNLOCK_SEQUENCE wakeup=%q home=%q swipe=%q\n' "$wakeup" "$home" "$swipe"
sleep 2

for bundle in "$BUNDLE_A" "$BUNDLE_B"; do
  dump="$(hdc shell "bm dump -n $bundle" | sed -E \
    -e 's/("accessTokenId": )[0-9]+/\1[RUNTIME_ACCESS_TOKEN_ID_REDACTED]/g' \
    -e 's/("accessTokenIdEx": )[0-9]+/\1[RUNTIME_ACCESS_TOKEN_ID_EX_REDACTED]/g')"
  printf 'BUNDLE_AUDIT_BEGIN bundle=%s\n%s\nBUNDLE_AUDIT_END\n' "$bundle" "$dump"
  [[ "$dump" == *'"appPrivilegeLevel": "normal"'* && "$dump" == *'"isSystemApp": false'* &&
     "$dump" == *'"mainElementName": "EntryAbility"'* && "$dump" == *'"type": 502'* ]] ||
    fail "installed bundle is not an ordinary app with VPN extension: $bundle"
done
printf 'INSTALLED_NORMAL_APP_AUDIT=pass\n'

dialog_bundle_query="$(hdc shell 'bm dump -n com.huawei.hmos.vpndialog' 2>&1 || true)"
printf 'VPN_DIALOG_BUNDLE_PRECHECK=%q\n' "$dialog_bundle_query"
[[ "$dialog_bundle_query" == *'failed to get information'* ]] ||
  fail "unexpected VPN dialog package state before measured scenarios"

hdc shell 'hilog -G 16M'
hilog_size="$(hdc shell 'hilog -g' 2>&1 | tr -d '\r')"
printf 'HILOG_BUFFER_QUERY=%q\n' "$hilog_size"
[[ "$hilog_size" == *'16.0M'* || "$hilog_size" == *'16M'* || "$hilog_size" == *'16777216'* ]] ||
  fail "HiLog buffer is not 16 MiB"
capture_faults 'BEFORE_SCENARIOS'

for round in 1 2 3; do
  hdc shell 'hilog -r'
  start_entry "$BUNDLE_A"
  pid="$STARTED_PID"
  for old_pid in "${a_pids[@]}"; do
    [[ "$pid" != "$old_pid" ]] || fail "A PID reused across blocked authorization attempts"
  done
  a_pids+=("$pid")
  printf 'A_ALLOW_ATTEMPT_%s_PID=%s\n' "$round" "$pid"
  sleep 2
  capture_layout "a-allow-attempt${round}-entry"
  entry_layout="$CAPTURED_LAYOUT"
  click_node_id "$entry_layout" 'start-vpn'
  sleep 10
  capture_layout "a-allow-attempt${round}-authorization-absent"
  blocked_layout="$CAPTURED_LAYOUT"
  capture_screen "a-allow-attempt${round}-authorization-absent"
  capture_hilog "a-allow-attempt${round}"
  full="$CAPTURED_HILOG"
  capture_faults "A_ALLOW_ATTEMPT_${round}"
  [[ "$(get_pid "$BUNDLE_A")" == "$pid" ]] || fail "A exited before blocked-path capture"
  assert_missing_authorization_path "A_ALLOW_ATTEMPT_${round}" "$full" "$blocked_layout" "$BUNDLE_A"
  force_stop_for_cleanup "$BUNDLE_A" "blocked-authorization-attempt-$round"
done
printf 'A_BLOCKED_PATH_PIDS=%s,%s,%s\n' "${a_pids[0]}" "${a_pids[1]}" "${a_pids[2]}"
printf 'ALLOW_REPEAT_REQUIREMENT=blocked no_first_allow_possible; missing_system_dialog_reproduced_three_distinct_PIDs\n'

hdc shell 'hilog -r'
start_entry "$BUNDLE_B"
b_pid="$STARTED_PID"
printf 'B_DENY_ATTEMPT_PID=%s freshIdentity=true\n' "$b_pid"
sleep 2
capture_layout 'b-deny-entry'
b_entry_layout="$CAPTURED_LAYOUT"
click_node_id "$b_entry_layout" 'start-vpn'
sleep 10
capture_layout 'b-deny-authorization-absent'
b_blocked_layout="$CAPTURED_LAYOUT"
capture_screen 'b-deny-authorization-absent'
capture_hilog 'b-deny'
b_full="$CAPTURED_HILOG"
capture_faults 'B_DENY'
assert_missing_authorization_path 'B_DENY' "$b_full" "$b_blocked_layout" "$BUNDLE_B"
printf 'B_DENY_CLICK=blocked no authorization dialog button existed; no refusal fabricated\n'
printf 'B_PROMISE_ACTUAL=pending_within_10_seconds; API24_observer_not_available_or_claimed\n'
force_stop_for_cleanup "$BUNDLE_B" 'blocked-denial-attempt'

hdc shell 'hilog -r'
start_entry "$BUNDLE_A"
stop_pid="$STARTED_PID"
printf 'STOP_OBSERVATION_PID=%s activeVpn=false\n' "$stop_pid"
sleep 2
capture_layout 'stop-without-active-entry'
stop_entry_layout="$CAPTURED_LAYOUT"
click_node_id "$stop_entry_layout" 'stop-vpn'
sleep 5
capture_layout 'stop-without-active-result'
capture_screen 'stop-without-active-result'
capture_hilog 'stop-without-active'
stop_full="$CAPTURED_HILOG"
capture_faults 'STOP_WITHOUT_ACTIVE'
stop_ui_count="$(grep -F -c "UI_STOP|bundle=$BUNDLE_A|" "$stop_full" || true)"
stop_promise_count="$(grep -F -c "STOP_PROMISE|bundle=$BUNDLE_A|" "$stop_full" || true)"
stop_oncreate_count="$(grep -F -c 'VPN_ONCREATE|' "$stop_full" || true)"
stop_ondestroy_count="$(grep -F -c 'VPN_ONDESTROY|' "$stop_full" || true)"
printf 'STOP_OBSERVATION_COUNTS uiStop=%s promiseSettled=%s onCreate=%s onDestroy=%s\n' \
  "$stop_ui_count" "$stop_promise_count" "$stop_oncreate_count" "$stop_ondestroy_count"
(( stop_ui_count == 1 )) || fail "normal UI stop request was not recorded"
(( stop_oncreate_count == 0 && stop_ondestroy_count == 0 )) ||
  fail "unexpected lifecycle during no-active stop observation"
grep -F 'STOP_PROMISE|' "$stop_full" || true
printf 'NORMAL_ACTIVE_STOP_VERDICT=blocked A could not become active; no onDestroy fabricated\n'
force_stop_for_cleanup "$BUNDLE_A" 'post-stop-observation'

hdc shell 'hilog -r'
start_entry "$BUNDLE_A"
conflict_a_pid="$STARTED_PID"
sleep 2
capture_layout 'conflict-a-entry'
conflict_a_layout="$CAPTURED_LAYOUT"
click_node_id "$conflict_a_layout" 'start-vpn'
sleep 5
[[ -n "$(get_pid "$BUNDLE_A")" ]] || fail "A process missing before B conflict request"
start_entry "$BUNDLE_B"
conflict_b_pid="$STARTED_PID"
sleep 2
capture_layout 'conflict-b-entry'
conflict_b_layout="$CAPTURED_LAYOUT"
click_node_id "$conflict_b_layout" 'start-vpn'
sleep 10
capture_layout 'conflict-result-no-active-vpn'
conflict_layout="$CAPTURED_LAYOUT"
capture_screen 'conflict-result-no-active-vpn'
capture_hilog 'conflict-no-active'
conflict_full="$CAPTURED_HILOG"
capture_faults 'CONFLICT_NO_ACTIVE'
conflict_ui_a="$(grep -F -c "UI_START|bundle=$BUNDLE_A|" "$conflict_full" || true)"
conflict_ui_b="$(grep -F -c "UI_START|bundle=$BUNDLE_B|" "$conflict_full" || true)"
conflict_oncreate="$(grep -F -c 'VPN_ONCREATE|' "$conflict_full" || true)"
conflict_promise="$(grep -F -c 'START_PROMISE|' "$conflict_full" || true)"
printf 'CONFLICT_OBSERVATION aPid=%s bPid=%s uiA=%s uiB=%s promisesSettled=%s onCreate=%s\n' \
  "$conflict_a_pid" "$conflict_b_pid" "$conflict_ui_a" "$conflict_ui_b" \
  "$conflict_promise" "$conflict_oncreate"
(( conflict_ui_a == 1 && conflict_ui_b == 1 && conflict_oncreate == 0 )) ||
  fail "conflict observation request/lifecycle counts mismatch"
printf 'SINGLE_INSTANCE_CONFLICT_VERDICT=blocked A never became active; E3 does not call create to force a create-level conflict\n'
force_stop_for_cleanup "$BUNDLE_A" 'post-conflict-observation'
force_stop_for_cleanup "$BUNDLE_B" 'post-conflict-observation'

hdc shell 'hilog -r'
hdc shell 'uitest uiInput keyEvent Home'
sleep 2
capture_layout 'settings-home'
home_layout="$CAPTURED_LAYOUT"
capture_screen 'settings-home'
click_id_prefix "$home_layout" 'AppIconCommonView_com.huawei.hmos.settings.'
sleep 3
capture_layout 'settings-main'
settings_layout="$CAPTURED_LAYOUT"
capture_screen 'settings-main'
if cmp -s "$home_layout" "$settings_layout"; then
  fail "Settings click left the exact Home layout unchanged"
fi
jq -e '.. | objects | .attributes? |
  select(.bundleName == "com.huawei.hmos.settings" and .visible == "true")' \
  "$settings_layout" >/dev/null || fail "Settings bundle window is absent after desktop icon click"
jq -e '.. | objects | .attributes? |
  select(.text == "Settings" and .visible == "true")' \
  "$settings_layout" >/dev/null || fail "Settings page title is absent after desktop icon click"
jq -r '.. | objects | .attributes? |
  select(.text? != null and .text != "") | [.text, .bounds, .id] | @tsv' \
  "$settings_layout" >"$SETTINGS_TEXT"
run sha256sum "$SETTINGS_TEXT"
printf 'SETTINGS_MAIN_TEXT_BEGIN\n'
run grep -E '^(Settings|WLAN|System|Apps & services|Accessibility|Storage|Battery)' "$SETTINGS_TEXT"
printf 'SETTINGS_MAIN_TEXT_END\n'
if grep -E -i '(^|[[:space:]])VPN([[:space:]]|$)|More connections|VPN management' "$SETTINGS_TEXT"; then
  fail "unexpected VPN management entry found; blocked runner cannot classify it without active A"
fi
capture_hilog 'settings-main'
settings_full="$CAPTURED_HILOG"
capture_faults 'SETTINGS_MAIN'
printf 'SETTINGS_UI_PATH=Home_icon_click_then_Settings_main ordinaryUserUi=true\n'
printf 'SETTINGS_VPN_MANAGEMENT_ENTRY=absent in complete Settings main layout\n'
printf 'USER_REVOKE_VERDICT=blocked no active A and no VPN management entry; no force-stop/uninstall used as revoke\n'
run sha256sum "$settings_full" "$settings_layout"

capture_faults 'AFTER_SCENARIOS'
run sha256sum "$TAG_HILOG" "$FAULT_LIST" "$CONSOLE" "$RAW/$EVIDENCE_ID"-*.png \
  "$RAW/$EVIDENCE_ID"-*-layout.json "$RAW/$EVIDENCE_ID"-*-hilog-full.log

hdc shell "rm -rf $STAGING"
timeout 120 "$HDC" -t "$TARGET" uninstall "$BUNDLE_A"
installed_a=0
timeout 120 "$HDC" -t "$TARGET" uninstall "$BUNDLE_B"
installed_b=0
for bundle in "$BUNDLE_A" "$BUNDLE_B"; do
  post="$(hdc shell "bm dump -n $bundle" 2>&1 || true)"
  printf 'POST_UNINSTALL bundle=%s result=%q\n' "$bundle" "$post"
  [[ "$post" == *'failed to get information'* ]] || fail "bundle remained after uninstall: $bundle"
done
printf 'UNINSTALL_ABSENCE=pass\n'

stop_emulator
"$HDC" kill || true
sleep 2
residual_processes="$(pgrep -af '/[e]mulator/Emulator.*netbird_api24_phone|[q]emu-system.*netbird_api24_phone|[h]dc -m -s' || true)"
residual_ports="$(ss -ltnp | grep -E ':(10000|5555|8710)[[:space:]]' || true)"
printf 'FINAL_RESIDUAL_PROCESSES=%q\n' "$residual_processes"
printf 'FINAL_RESIDUAL_PORTS=%q\n' "$residual_ports"
[[ -z "$residual_processes" && -z "$residual_ports" ]] || fail "host process or port residue"

run bash -n "$PROJECT/e3-vpn-extension-emulator-run.sh"
run git -C "$WORKSPACE" diff --check
run markdownlint-cli2 "$WORKSPACE/README.md" "$WORKSPACE/docs/**/*.md" "$PROJECT/README.md"
if rg -n -i -e 'authorization:[[:space:]]*bearer[[:space:]]+[A-Za-z0-9._~-]+' \
    -e '-----BEGIN ([A-Z ]+ )?PRIVATE KEY-----' \
    -e 'setup[_ -]?key[=:][[:space:]]*[A-Za-z0-9._~-]{8,}' \
    "$TRANSCRIPT" "$TAG_HILOG" "$FAULT_LIST" "$RAW/$EVIDENCE_ID"-*-hilog-full.log; then
  fail "high-confidence sensitive material pattern detected"
fi
printf 'SENSITIVE_SCAN=pass_high_confidence_patterns\n'

if rg -n 'MANAGE_VPN|createVpnConnection|createVpnObserver|\.create\(|\.protect\(|\.destroy\(' \
  "${executable_inputs[@]}"; then
  fail "post-run forbidden API scan failed"
fi
printf 'POST_RUN_FORBIDDEN_API_SCAN=pass\n'

result="blocked"
cleaned=1
ended_at="$(date --iso-8601=seconds)"
printf 'ENDED_AT=%s\n' "$ended_at"
printf 'AUTHORIZATION=blocked missing_system_authorization_component\n'
printf 'DENIAL=blocked no_system_dialog_or_deny_button\n'
printf 'STOP=blocked no_active_A_so_no_onDestroy\n'
printf 'REVOKE=blocked no_active_A_and_no_Settings_VPN_management_entry\n'
printf 'CONFLICT=blocked no_active_A_and_no_VpnConnection_create_in_E3\n'
printf 'RECORD_STATUS=collected\n'
printf 'VERDICT=blocked\n'
