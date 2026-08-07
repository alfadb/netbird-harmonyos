#!/usr/bin/env bash
set -Eeuo pipefail

readonly WORKSPACE="/home/worker/work/base/netbird-harmonyos"
readonly PROJECT="$WORKSPACE/spikes/e3-vpn-extension-physical-preflight-hap"
readonly PROFILE="$PROJECT/build-profile.json5"
readonly APP_SCOPE="$PROJECT/AppScope/app.json5"
readonly MODULE_PROFILE="$PROJECT/entry/build-profile.json5"
readonly ENTRY_PACKAGE="$PROJECT/entry/oh-package.json5"
readonly MANIFEST="$PROJECT/entry/src/main/module.json5"
readonly ETS_ROOT="$PROJECT/entry/src/main/ets"
readonly UI="$ETS_ROOT/pages/Index.ets"
readonly EXTENSION="$ETS_ROOT/vpnextensionability/E3PhysicalVpnExtensionAbility.ets"
readonly CPP_ROOT="$PROJECT/entry/src/main/cpp"
readonly CMAKE="$CPP_ROOT/CMakeLists.txt"
readonly NATIVE_C="$CPP_ROOT/fd_probe.c"
readonly NATIVE_TYPES="$CPP_ROOT/types/libfdprobe/index.d.ts"
readonly NATIVE_PACKAGE="$CPP_ROOT/types/libfdprobe/oh-package.json5"
readonly HVIGOR="/home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw"
readonly SDK_PKG="/home/worker/harmonyos/command-line-tools/6.1.1.290/sdk/default/sdk-pkg.json"
readonly HAP_A="$PROJECT/entry/build/default/outputs/default/entry-default-unsigned.hap"
readonly HAP_B="$PROJECT/entry/build/vpnB/outputs/default/entry-default-unsigned.hap"
readonly LIB_A="$PROJECT/entry/build/default/intermediates/stripped_native_libs/default/arm64-v8a/libfdprobe.so"
readonly LIB_B="$PROJECT/entry/build/vpnB/intermediates/stripped_native_libs/default/arm64-v8a/libfdprobe.so"
readonly BUNDLE_A="cn.alfadb.netbird.e3physvpna"
readonly BUNDLE_B="cn.alfadb.netbird.e3physvpnb"
readonly HISTORICAL="spikes/e3-vpn-extension-hap"
readonly RAW_EVIDENCE="docs/evidence/raw"
readonly EXPECTED_HISTORICAL_TREE="c1af639568c7bc299c3164f1bc0f56cf2c5cdeda"
readonly EXPECTED_RAW_TREE="608d33f902b3a3f356a27d3f433aa46613df36d2"
readonly NATIVE_MEMBER="libs/arm64-v8a/libfdprobe.so"

fail() {
  printf 'AUDIT_FAIL=%s\n' "$1" >&2
  exit 1
}

require_count() {
  local literal="$1"
  local expected="$2"
  local file="$3"
  local count
  count="$(rg -F -o "$literal" "$file" | wc -l)"
  [[ "$count" -eq "$expected" ]] ||
    fail "count mismatch file=$file literal=$literal expected=$expected actual=$count"
}

require_marker() {
  local marker="$1"
  local file="$2"
  rg -F -q "$marker" "$file" || fail "missing marker $marker in $file"
}

assert_historical_unchanged() {
  local historical_tree raw_tree status
  historical_tree="$(git -C "$WORKSPACE" rev-parse "HEAD:$HISTORICAL")"
  raw_tree="$(git -C "$WORKSPACE" rev-parse "HEAD:$RAW_EVIDENCE")"
  [[ "$historical_tree" == "$EXPECTED_HISTORICAL_TREE" ]] ||
    fail "historical E3 HEAD tree changed: $historical_tree"
  [[ "$raw_tree" == "$EXPECTED_RAW_TREE" ]] ||
    fail "raw evidence HEAD tree changed: $raw_tree"
  status="$(git -C "$WORKSPACE" status --porcelain=v1 --untracked-files=all -- \
    "$HISTORICAL" "$RAW_EVIDENCE")"
  [[ -z "$status" ]] || fail "historical E3 or raw evidence worktree is dirty: $status"
  printf 'HISTORICAL_TREE_AUDIT=pass e3=%s raw=%s\n' "$historical_tree" "$raw_tree"
}

assert_identity_and_profiles() {
  require_count "name: 'default'" 2 "$PROFILE"
  require_count "name: 'vpnB'" 1 "$PROFILE"
  require_count "bundleName: '$BUNDLE_A'" 1 "$PROFILE"
  require_count "bundleName: '$BUNDLE_B'" 1 "$PROFILE"
  require_count "bundleName: '$BUNDLE_A'" 1 "$APP_SCOPE"
  require_count 'bundleName:' 1 "$APP_SCOPE"
  require_count 'bundleName:' 2 "$PROFILE"
  require_count "compileSdkVersion: '6.1.1(24)'" 2 "$PROFILE"
  require_count "compatibleSdkVersion: '6.1.0(23)'" 2 "$PROFILE"
  require_count "targetSdkVersion: '6.1.0(23)'" 2 "$PROFILE"
  require_count "arguments: '-DOHOS_STL=none'" 1 "$MODULE_PROFILE"
  require_count "'arm64-v8a'" 1 "$MODULE_PROFILE"
  require_count "'fdprobe'" 1 "$MODULE_PROFILE"
  require_count "'libfdprobe.so': 'file:./src/main/cpp/types/libfdprobe'" 1 "$ENTRY_PACKAGE"

  if rg -n -F "bundleName: 'cn.alfadb.netbird.e3physvpn'" "$APP_SCOPE" "$PROFILE"; then
    fail 'misleading generic AppScope/product bundleName found'
  fi
  if rg -n 'cn\.alfadb\.' "$ETS_ROOT" "$MANIFEST" "$MODULE_PROFILE" "$ENTRY_PACKAGE"; then
    fail 'hard-coded bundle identity found outside AppScope/product configuration'
  fi
  if rg -n -i 'x86|armeabi-v7a' "$MODULE_PROFILE" "$ENTRY_PACKAGE" "$CPP_ROOT"; then
    fail 'non-arm64 native configuration found'
  fi
  printf 'IDENTITY_PROFILE_AUDIT=pass appScope=%s products=2 api=compile24_target23 abi=arm64-v8a\n' "$BUNDLE_A"
}

assert_native_source() {
  local native_sources
  native_sources="$(find "$CPP_ROOT" -type f \( -name '*.c' -o -name '*.cc' -o -name '*.cpp' -o -name '*.cxx' \) -print)"
  [[ "$native_sources" == "$NATIVE_C" ]] || fail "unexpected native source set: $native_sources"

  require_count 'project(e3_fdprobe LANGUAGES C)' 1 "$CMAKE"
  require_count 'add_library(fdprobe SHARED fd_probe.c)' 1 "$CMAKE"
  require_count 'add_library(' 1 "$CMAKE"
  require_count 'libace_napi.z.so' 1 "$CMAKE"
  require_count '#include <errno.h>' 1 "$NATIVE_C"
  require_count '#include <fcntl.h>' 1 "$NATIVE_C"
  require_count '#include <stdbool.h>' 1 "$NATIVE_C"
  require_count '#include <napi/native_api.h>' 1 "$NATIVE_C"
  require_count '#include' 4 "$NATIVE_C"
  require_count 'fcntl(fd, F_GETFD)' 1 "$NATIVE_C"
  require_count 'fcntl(' 1 "$NATIVE_C"
  require_count 'F_GETFD' 1 "$NATIVE_C"
  require_count 'return NULL;' 3 "$NATIVE_C"
  require_count 'napi_throw_error(env, NULL,' 3 "$NATIVE_C"
  if rg -F -q 'napi_throw_type_error' "$NATIVE_C"; then
    fail 'N-API NULL return path must use napi_throw_error'
  fi
  local missing_napi_error
  missing_napi_error="$(awk '/return NULL;/ { if (previous !~ /napi_throw_error\(env, NULL,/) print NR } { previous = $0 }' "$NATIVE_C")"
  [[ -z "$missing_napi_error" ]] ||
    fail "N-API NULL return without preceding napi_throw_error at lines: $missing_napi_error"
  require_count '"status"' 1 "$NATIVE_C"
  require_count 'export const status' 1 "$NATIVE_TYPES"
  require_count 'export const ' 1 "$NATIVE_TYPES"
  require_count "name: 'libfdprobe.so'" 1 "$NATIVE_PACKAGE"

  if rg -n -i 'LANGUAGES[[:space:]]+CXX|cxx_std|libc\+\+|hilog|pthread|thread|socket|network' \
    "$CMAKE" "$NATIVE_C"; then
    fail 'C++/logging/thread/socket/network native path found'
  fi
  if rg -n '(^|[^[:alnum:]_])(close|dup|dup2|dup3|read|write|pread|pwrite|socket|socketpair|connect|bind|listen|accept|send|recv|shutdown|poll|select|epoll_create|pthread_create|thrd_create)[[:space:]]*\(' \
    "$NATIVE_C"; then
    fail 'forbidden fd ownership, I/O, socket, or thread call found in native probe'
  fi
  if rg -n '#include[[:space:]]*[<"](unistd|sys/socket|pthread|threads|arpa|netinet)' "$NATIVE_C"; then
    fail 'forbidden native header found'
  fi
  if rg -n 'F_DUPFD|F_SETFL|F_SETFD' "$NATIVE_C"; then
    fail 'fd mutation command found in native probe'
  fi
  printf 'NATIVE_SOURCE_AUDIT=pass language=C exports=status syscall=fcntl_F_GETFD_only\n'
}

assert_arkts_source() {
  require_count "name: 'ohos.permission.INTERNET'" 1 "$MANIFEST"
  require_count "type: 'vpn'" 1 "$MANIFEST"
  require_count 'exported: false' 1 "$MANIFEST"

  if rg -n -i \
    'MANAGE_VPN|@ohos\.enterprise|automaticAuthorization|authorizationObserver|createVpnObserver|VpnObserver|generateVpnId|protectProcessNet|\.protect[[:space:]]*\(|@ohos\.net\.(socket|http)|https?://|wireguard|background(Service|Task|Mode)|ServiceExtensionAbility' \
    "$ETS_ROOT" "$MANIFEST"; then
    fail 'forbidden observer, protection, endpoint, integration, or background source path'
  fi
  if rg -n '\.(close|dup|read|write)[[:space:]]*\(' "$ETS_ROOT"; then
    fail 'fd ownership or I/O call found in ArkTS'
  fi
  if rg -n 'Atomics\.wait|Promise\.race|while[[:space:]]*\(|for[[:space:]]*\([[:space:]]*;|setInterval' "$ETS_ROOT"; then
    fail 'busy-wait or race found in ArkTS lifecycle'
  fi
  if rg -n 'setTimeout|clearTimeout' "$EXTENSION"; then
    fail 'timer found outside the UI bounded-release guard'
  fi
  require_count 'setTimeout(' 1 "$UI"
  require_count 'clearTimeout(' 1 "$UI"
  local timer_api
  timer_api="$(rg --no-filename -o 'setTimeout|clearTimeout|setInterval|clearInterval' "$ETS_ROOT" | LC_ALL=C sort -u | paste -sd, -)"
  [[ "$timer_api" == 'clearTimeout,setTimeout' ]] ||
    fail "non-minimal timer API set in ArkTS: $timer_api"
  local fs_api
  fs_api="$(rg --no-filename -o 'fs\.[A-Za-z]+' "$ETS_ROOT" | LC_ALL=C sort -u | paste -sd, -)"
  [[ "$fs_api" == 'fs.OpenMode,fs.closeSync,fs.openSync,fs.readSync,fs.unlinkSync,fs.writeSync' ]] ||
    fail "non-minimal fs API set in ArkTS: $fs_api"
  if rg -n 'VPN_DESTROY_ISSUED' "$ETS_ROOT"; then
    fail 'VPN_DESTROY_ISSUED terminal marker must not appear in ArkTS'
  fi
  if rg -n '\.destroy[[:space:]]*\([^[:space:])]' "$ETS_ROOT"; then
    fail 'argument-bearing destroy call found'
  fi
  if rg -n 'createVpnConnection|\.create[[:space:]]*\(|\.destroy[[:space:]]*\(' "$UI"; then
    fail 'UI owns or operates a VpnConnection'
  fi
  if rg -n 'routes|dnsAddresses|searchDomains|mtu|isIPv4Accepted|isIPv6Accepted|isInternal|isBlocking|trustedApplications|blockedApplications|vpnId|endpoint' "$EXTENSION"; then
    fail 'VpnConfig or source exceeds the minimal local address scope'
  fi

  require_count 'createVpnConnection(this.context)' 1 "$EXTENSION"
  require_count 'connection.create(config)' 1 "$EXTENSION"
  require_count 'connection.destroy()' 1 "$EXTENSION"
  require_count 'fdprobe.status(fd)' 1 "$EXTENSION"
  require_count "address: '192.0.2.1'" 1 "$EXTENSION"
  require_count 'prefixLength: 32' 1 "$EXTENSION"
  require_count "this.logFdSnapshot('post-create'" 1 "$EXTENSION"
  require_count "this.logFdSnapshot('pre-destroy'" 1 "$EXTENSION"
  require_count "this.logFdSnapshot('post-destroy-resolved'" 1 "$EXTENSION"
  require_count "this.logFdSnapshot('post-destroy-rejected'" 1 "$EXTENSION"
  require_count '.enabled(' 2 "$UI"
  require_count '.enabled(!this.operationPending && this.activeRequestId.length === 0)' 1 "$UI"
  require_count '.enabled(!this.operationPending)' 1 "$UI"
  require_count "Button('Start')" 1 "$UI"
  require_count "Button('Stop')" 1 "$UI"
  require_count 'this.nextRequestId()' 1 "$UI"
  require_count 'if (this.activeRequestId.length > 0) {' 1 "$UI"
  require_count 'UI_START_SKIPPED|bundle=%{public}s|requestId=%{public}s|reason=active-request' 1 "$UI"
  require_count 'this.activeRequestId = requestId;' 1 "$UI"
  require_count "this.activeRequestId = '';" 4 "$UI"
  require_count 'const code: number | undefined = (error as BusinessFailure).code;' 1 "$UI"
  require_count 'if (code === 16000001 && this.activeRequestId === requestId) {' 1 "$UI"
  require_count 'STOP_SESSION_RELEASED|bundle=%{public}s|requestId=%{public}s|reason=ability-not-found' 1 "$UI"
  if ! rg -U -q "if \\(code === 16000001 && this\\.activeRequestId === requestId\\) \\{\\n        this\\.activeRequestId = '';" "$UI"; then
    fail 'only BusinessFailure code 16000001 may release a rejected Stop session'
  fi

  local marker
  for marker in \
    UI_START UI_START_SKIPPED START_PROMISE_RESOLVED START_PROMISE_REJECTED START_PROMISE_LATE_RESOLVED \
    START_PROMISE_LATE_REJECTED START_PENDING_RELEASED LEDGER_PERSISTED LEDGER_WRITE_REJECTED \
    UI_STOP UI_STOP_SKIPPED STOP_PROMISE_RESOLVED STOP_PROMISE_REJECTED \
    STOP_SESSION_RELEASED; do
    require_marker "$marker" "$UI"
  done
  require_count 'START_PENDING_RELEASED|bundle=%{public}s|requestId=%{public}s|reason=bounded-timeout' 1 "$UI"
  require_count 'this.writeRequestLedger(requestId);' 1 "$UI"
  require_count 'fs.openSync(ledgerPath,' 1 "$UI"
  require_count 'fs.writeSync(fd, payload);' 1 "$UI"
  require_count 'fs.closeSync(fd);' 1 "$UI"
  require_count 'this.startGeneration++;' 2 "$UI"
  for marker in \
    VPN_ONCREATE VPN_CONNECTION_CREATED VPN_CREATE_BEGIN VPN_CREATE_RESOLVED VPN_CREATE_REJECTED \
    VPN_CREATE_INVALID_FD VPN_FD_SNAPSHOT VPN_FD_PROBE_REJECTED \
    CREATE_ACCEPTED CREATE_INVALID_DESTROY_REQUIRED \
    VPN_ONDESTROY VPN_DESTROY_WAIT_CREATE VPN_DESTROY_BEGIN VPN_DESTROY_RESOLVED \
    VPN_DESTROY_REJECTED VPN_DESTROY_SKIPPED VPN_DESTROY_ALREADY \
    PRE_DESTROY_OPEN PRE_DESTROY_NOT_OPEN FD_CLOSED_CONFIRMED FD_STILL_OPEN \
    FD_NOT_OPEN_AFTER_DESTROY FD_STATE_UNCONFIRMED \
    LEDGER_READ_RESOLVED LEDGER_READ_REJECTED LEDGER_CONSUME_RESOLVED LEDGER_CONSUME_REJECTED \
    LEDGER_MISSING LEDGER_REQUESTID_MISMATCH LEDGER_AGE_REJECTED \
    VPN_REQUESTID_INVALID \
    post-create pre-destroy post-destroy-resolved post-destroy-rejected; do
    require_marker "$marker" "$EXTENSION"
  done
  require_count 'requestSource=%{public}s' 1 "$EXTENSION"
  require_count 'fs.openSync(ledgerPath,' 1 "$EXTENSION"
  require_count 'fs.readSync(fd, buffer);' 1 "$EXTENSION"
  require_count 'fs.closeSync(fd);' 1 "$EXTENSION"
  require_count 'fs.unlinkSync(ledgerPath);' 1 "$EXTENSION"
  require_count 'this.consumeRequestLedger();' 1 "$EXTENSION"
  require_count 'LEDGER_MAX_FUTURE_MS' 1 "$EXTENSION"
  require_count 'LEDGER_MAX_AGE_MS' 1 "$EXTENSION"
  printf 'ARKTS_SOURCE_AUDIT=pass config=minimal ownership=platform_destroy singleFlight=true markers=complete uiSerialized=true ledger=sandbox timer=bounded-single fs=open-write-read-close\n'
}

clean_build() {
  local product="$1"
  printf 'BUILD_BEGIN product=%s clean=true\n' "$product"
  (
    cd "$PROJECT"
    "$HVIGOR" clean --mode module -p "product=$product" -p module=entry@default \
      -p buildMode=debug --no-daemon
    "$HVIGOR" assembleHap --mode module -p "product=$product" -p module=entry@default \
      -p buildMode=debug --no-daemon
  )
  printf 'BUILD_END product=%s result=pass\n' "$product"
}

assemble_without_clean() {
  local product="$1"
  printf 'FINAL_ASSEMBLE_BEGIN product=%s reason=retain-both-product-outputs\n' "$product"
  (
    cd "$PROJECT"
    "$HVIGOR" assembleHap --mode module -p "product=$product" -p module=entry@default \
      -p buildMode=debug --no-daemon
  )
  printf 'FINAL_ASSEMBLE_END product=%s result=pass\n' "$product"
}

assert_elf() {
  local product="$1"
  local lib="$2"
  local needed
  [[ -f "$lib" ]] || fail "missing native intermediate for $product: $lib"
  readelf -h "$lib" | rg -q 'Class:[[:space:]]+ELF64' || fail "non-ELF64 library for $product"
  readelf -h "$lib" | rg -q 'Machine:[[:space:]]+AArch64' || fail "non-AArch64 library for $product"
  readelf -d "$lib" | rg -q 'SONAME.*\[libfdprobe\.so\]' || fail "wrong SONAME for $product"

  needed="$(readelf -d "$lib" | awk '/NEEDED/ { line=$0; sub(/^.*\[/, "", line); sub(/\].*$/, "", line); print line }' | LC_ALL=C sort)"
  [[ "$needed" == $'libace_napi.z.so\nlibc.so' ]] ||
    fail "unexpected ELF dependencies for $product: $needed"
  if readelf -Ws "$lib" | rg -n 'UND.*(^|[^[:alnum:]_])(close|dup|dup2|dup3|read|write|socket|connect|pthread_create)(@|$)'; then
    fail "forbidden undefined symbol in $product libfdprobe"
  fi
  nm -D --defined-only "$lib" | rg -q 'RegisterFdProbeModule' ||
    fail "missing Node-API registration symbol for $product"
  if nm -D --defined-only "$lib" | rg -n '[[:space:]](Status|MakeStatus|Init)$'; then
    fail "internal Node-API callback unexpectedly exported for $product"
  fi
  printf 'ELF_AUDIT product=%s class=ELF64 machine=AArch64 needed=libace_napi.z.so,libc.so\n' "$product"
}

assert_hap() {
  local product="$1"
  local hap="$2"
  local bundle="$3"
  local lib="$4"
  local module_json pack_info members native_members archive_hash lib_hash
  [[ -f "$hap" ]] || fail "missing HAP for $product: $hap"
  [[ "$(basename "$hap")" == 'entry-default-unsigned.hap' ]] ||
    fail "unexpected signed or renamed HAP for $product"

  members="$(unzip -Z1 "$hap")"
  if printf '%s\n' "$members" | rg -n '(^|/)META-INF/'; then
    fail "signature member found in $product HAP"
  fi
  if printf '%s\n' "$members" | rg -n -i 'x86|armeabi-v7a'; then
    fail "non-arm64 archive path found in $product HAP"
  fi
  native_members="$(printf '%s\n' "$members" | rg '(^|/)libs/|\.so$|\.(a|o)$' || true)"
  [[ "$native_members" == "$NATIVE_MEMBER" ]] ||
    fail "unexpected native member set in $product HAP: $native_members"

  archive_hash="$(unzip -p "$hap" "$NATIVE_MEMBER" | sha256sum | awk '{print $1}')"
  lib_hash="$(sha256sum "$lib" | awk '{print $1}')"
  [[ "$archive_hash" == "$lib_hash" ]] || fail "packaged/intermediate native hash mismatch for $product"
  assert_elf "$product" "$lib"

  module_json="$(unzip -p "$hap" module.json)"
  printf '%s\n' "$module_json" | jq -e --arg bundle "$bundle" '
    .app.bundleName == $bundle and
    .app.bundleType == "app" and
    .app.compileSdkVersion == "6.1.1.125" and
    .app.targetAPIVersion == 60100023 and
    .app.minAPIVersion == 60100023 and
    .module.type == "entry" and
    .module.requestPermissions == [{"name":"ohos.permission.INTERNET"}] and
    (.module.extensionAbilities | length) == 1 and
    .module.extensionAbilities[0].name == "E3PhysicalVpnExtensionAbility" and
    .module.extensionAbilities[0].type == "vpn" and
    .module.extensionAbilities[0].exported == false
  ' >/dev/null || fail "packaged module metadata mismatch for $product"

  pack_info="$(unzip -p "$hap" pack.info)"
  printf '%s\n' "$pack_info" | jq -e --arg bundle "$bundle" '
    .summary.app.bundleName == $bundle and
    .summary.app.bundleType == "app" and
    .summary.app.version == {"code":1,"name":"0.0.1"} and
    (.summary.modules | length) == 1 and
    .summary.modules[0].distro.moduleType == "entry" and
    .summary.modules[0].distro.moduleName == "entry" and
    .summary.modules[0].apiVersion.compatible == 23 and
    .summary.modules[0].apiVersion.target == 23 and
    .summary.modules[0].apiVersion.releaseType == "Release" and
    (.summary.modules[0].extensionAbilities | length) == 1 and
    .summary.modules[0].extensionAbilities[0].name == "E3PhysicalVpnExtensionAbility" and
    .packages == [{"deviceType":["phone"],"moduleType":"entry","deliveryWithInstall":true,"name":"entry-default"}]
  ' >/dev/null || fail "pack.info metadata mismatch for $product"

  printf 'HAP_AUDIT product=%s bundle=%s compile=6.1.1.125 target=60100023 compatible=60100023 unsigned=true nativeMember=%s nativeSha256=%s\n' \
    "$product" "$bundle" "$NATIVE_MEMBER" "$archive_hash"
  sha256sum "$hap"
}

printf 'AUDIT_BEGIN=E3-PHYS-PREFLIGHT\n'
[[ -x "$HVIGOR" ]] || fail "Stable Hvigor missing: $HVIGOR"
[[ "$(jq -r '.data.apiVersion' "$SDK_PKG")" == '24' ]] || fail 'Stable SDK API is not 24'
[[ "$(jq -r '.data.displayName' "$SDK_PKG")" == 'HarmonyOS 6.1.1' ]] ||
  fail 'Stable SDK display name mismatch'
printf 'STABLE_SDK_AUDIT=pass api=24 display=HarmonyOS_6.1.1 hvigor=%s\n' "$($HVIGOR --version)"

assert_historical_unchanged
assert_identity_and_profiles
assert_native_source
assert_arkts_source
clean_build default
assert_hap vpnA "$HAP_A" "$BUNDLE_A" "$LIB_A"
clean_build vpnB
assert_hap vpnB "$HAP_B" "$BUNDLE_B" "$LIB_B"
assemble_without_clean default
assert_hap vpnA "$HAP_A" "$BUNDLE_A" "$LIB_A"
assert_hap vpnB "$HAP_B" "$BUNDLE_B" "$LIB_B"
assert_historical_unchanged

printf 'HAP_A=%s\n' "$HAP_A"
printf 'HAP_B=%s\n' "$HAP_B"
printf 'DEVICE_OPERATION=false HDC=false SIGNING=false INSTALL=false LOGIN=false KEY_GENERATION=false\n'
printf 'AUDIT_RESULT=pass\n'
