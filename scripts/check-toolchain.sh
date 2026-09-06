#!/usr/bin/env bash
# Read-only, fail-closed check of the repo's active HarmonyOS toolchain baseline.
#
# Pinned baseline: $HOME/harmonyos/command-line-tools/26.0.0.821, where `current` must resolve.
# Read-only contract: no emulator start, no network, no cache writes, no host changes.
# hvigor/ohpm/SDK versions are parsed structurally from their JSON files with the bundled
# Node; the hvigorw/ohpm wrappers are never executed, so no build cache is touched.
# The fixed Rust baseline is checked against the rustup prefix /home/worker/rust with
# RUSTUP_HOME/CARGO_HOME/PATH set locally by this script (no interactive shell needed).
# Only `node -v`, `hdc -v`, `ldd` and the rustup/rustc/cargo version/target/cfg probes run.

set -Eeuo pipefail

readonly CLT_ROOT="$HOME/harmonyos/command-line-tools"
readonly CLT_VERSION="26.0.0.821"
readonly CLT_DIR="$CLT_ROOT/$CLT_VERSION"

readonly EXPECTED_HVIGOR="6.26.4"
readonly EXPECTED_OHPM="26.0.0.630"
readonly EXPECTED_NODE="v24.14.1"
readonly EXPECTED_HDC_TOKEN="3.2.0f"
readonly EXPECTED_SDK_STATUS="Release"
readonly EXPECTED_API_VERSION="26"
readonly EXPECTED_SDK_VERSION="26.0.0.105"

readonly NODE_BIN="$CLT_DIR/tool/node/bin/node"
readonly HDC_BIN="$CLT_DIR/sdk/default/openharmony/toolchains/hdc"

# Fixed Rust baseline: rustup-managed prefix under /home/worker/rust; rustup/rustc/cargo
# must resolve from $CARGO_HOME/bin and match these exact versions.
readonly RUSTUP_HOME="/home/worker/rust/rustup"
readonly CARGO_HOME="/home/worker/rust/cargo"
readonly EXPECTED_RUSTUP_VERSION="1.29.1"
readonly EXPECTED_RUSTC_VERSION="1.98.1"
readonly EXPECTED_CARGO_VERSION="1.98.1"
readonly REQUIRED_RUST_TARGETS=(
  "aarch64-unknown-linux-ohos"
  "x86_64-unknown-linux-ohos"
)
readonly AARCH64_OHOS_TARGET="aarch64-unknown-linux-ohos"
readonly REQUIRED_AARCH64_CFG=(
  'target_arch="aarch64"'
  'target_env="ohos"'
  'target_os="linux"'
)

readonly REQUIRED_FILES=(
  "bin/hvigorw"
  "hvigor/bin/hvigorw"
  "bin/ohpm"
  "ohpm/bin/ohpm"
  "tool/node/bin/node"
  "sdk/default/openharmony/toolchains/hdc"
)

# Structured JSON check run by the bundled Node (exits 1 on any mismatch or read error).
readonly JSON_CHECK_JS='
const fs = require("fs");
const [dir, expHvigor, expOhpm, expApi, expSdkVer, expStatus] = process.argv.slice(1);
let mismatches = 0;
const check = (name, expected, actual) => {
  const ok = actual === expected;
  if (!ok) { mismatches += 1; }
  console.log(`JSON_CHECK=${name} expected=${expected} actual=${String(actual)} result=${ok ? "ok" : "mismatch"}`);
};
const readJson = (rel) => JSON.parse(fs.readFileSync(`${dir}/${rel}`, "utf8"));
try {
  const sdk = readJson("sdk/default/sdk-pkg.json").data || {};
  const stage = typeof sdk.stage === "string" ? sdk.stage : "";
  const releaseType = typeof sdk.releaseType === "string" ? sdk.releaseType : "";
  const statusOk = stage === expStatus || releaseType === expStatus;
  if (!statusOk) { mismatches += 1; }
  console.log(`JSON_CHECK=sdk_release_status expected=${expStatus} actual=stage:${stage},releaseType:${releaseType} result=${statusOk ? "ok" : "mismatch"}`);
  check("sdk_apiVersion", expApi, sdk.apiVersion);
  check("sdk_version", expSdkVer, sdk.version);
  check("hvigor_version", expHvigor, readJson("hvigor/hvigor/package.json").version);
  check("ohpm_version", expOhpm, readJson("ohpm/package.json").version);
} catch (err) {
  console.log(`JSON_CHECK=exception expected=none actual=${err.message} result=error`);
  process.exit(1);
}
process.exit(mismatches === 0 ? 0 : 1);
'

fail() {
  printf 'CHECK=%s result=error %s\n' "$1" "$2" >&2
  exit 1
}

pass() {
  printf 'CHECK=%s result=ok %s\n' "$1" "$2"
}

check_ldd() {
  local name="$1" binary="$2" out missing
  if ! out="$(ldd "$binary" 2>&1)"; then
    fail "$name" "ldd exited nonzero binary=$binary output=${out//$'\n'/; }"
  fi
  if [[ "$out" == *'not found'* ]]; then
    missing="$(printf '%s\n' "$out" | grep 'not found' | tr '\n' ';')"
    fail "$name" "binary=$binary missing=$missing"
  fi
  pass "$name" "binary=$binary"
}

[[ -d "$CLT_DIR" ]] ||
  fail "version_dir" "missing pinned directory $CLT_DIR"
pass "version_dir" "path=$CLT_DIR"

[[ -L "$CLT_ROOT/current" ]] ||
  fail "current_symlink" "not a symlink: $CLT_ROOT/current"
current_target="$(readlink -f "$CLT_ROOT/current")" ||
  fail "current_symlink" "readlink failed for $CLT_ROOT/current"
[[ "$current_target" == "$CLT_DIR" ]] ||
  fail "current_symlink" "expected=$CLT_DIR actual=$current_target"
pass "current_symlink" "target=$current_target"

for rel in "${REQUIRED_FILES[@]}"; do
  [[ -x "$CLT_DIR/$rel" ]] ||
    fail "required_executables" "missing or not executable: $CLT_DIR/$rel"
done
pass "required_executables" "count=${#REQUIRED_FILES[@]}"

if ! node_out="$("$NODE_BIN" -v 2>&1)"; then
  fail "bundled_node_version" "node -v failed output=$node_out"
fi
[[ "$node_out" == "$EXPECTED_NODE" ]] ||
  fail "bundled_node_version" "expected=$EXPECTED_NODE actual=$node_out"
pass "bundled_node_version" "version=$node_out"

if ! hdc_out="$(timeout 30 "$HDC_BIN" -v 2>&1)"; then
  fail "hdc_version" "hdc -v failed output=${hdc_out//$'\n'/; }"
fi
[[ "$hdc_out" == *"$EXPECTED_HDC_TOKEN"* ]] ||
  fail "hdc_version" "expected token=$EXPECTED_HDC_TOKEN actual=${hdc_out//$'\n'/; }"
pass "hdc_version" "output=${hdc_out//$'\n'/; }"

if json_out="$("$NODE_BIN" -e "$JSON_CHECK_JS" \
  "$CLT_DIR" "$EXPECTED_HVIGOR" "$EXPECTED_OHPM" \
  "$EXPECTED_API_VERSION" "$EXPECTED_SDK_VERSION" "$EXPECTED_SDK_STATUS")"; then
  printf '%s\n' "$json_out"
else
  rc=$?
  if [[ -n "$json_out" ]]; then
    printf '%s\n' "$json_out" >&2
  fi
  fail "sdk_json" "structured JSON check failed node_exit=$rc"
fi

check_ldd "ldd_bundled_node" "$NODE_BIN"
check_ldd "ldd_hdc" "$HDC_BIN"

# ---- Fixed Rust baseline: rustup prefix /home/worker/rust (read-only probes) ----
[[ -d "$RUSTUP_HOME" ]] ||
  fail "rust_prefix_dirs" "missing RUSTUP_HOME directory $RUSTUP_HOME"
[[ -d "$CARGO_HOME" ]] ||
  fail "rust_prefix_dirs" "missing CARGO_HOME directory $CARGO_HOME"
pass "rust_prefix_dirs" "RUSTUP_HOME=$RUSTUP_HOME CARGO_HOME=$CARGO_HOME"

export RUSTUP_HOME CARGO_HOME
case ":$PATH:" in
  *":$CARGO_HOME/bin:"*) ;;
  *) export PATH="$CARGO_HOME/bin:$PATH" ;;
esac

for tool in rustup rustc cargo; do
  if ! tool_path="$(command -v "$tool" 2>/dev/null)"; then
    fail "rust_prefix_resolve" "not found on PATH: $tool"
  fi
  [[ "$tool_path" == "$CARGO_HOME/bin/$tool" ]] ||
    fail "rust_prefix_resolve" "tool=$tool expected=$CARGO_HOME/bin/$tool actual=$tool_path"
done
pass "rust_prefix_resolve" "prefix=$CARGO_HOME/bin"

for spec in "rustup:$EXPECTED_RUSTUP_VERSION" \
            "rustc:$EXPECTED_RUSTC_VERSION" \
            "cargo:$EXPECTED_CARGO_VERSION"; do
  tool="${spec%%:*}"
  expected="${spec#*:}"
  if ! full="$("$tool" --version 2>&1)"; then
    fail "rust_${tool}_version" "$tool --version failed output=${full//$'\n'/; }"
  fi
  read -r _ actual _ <<<"${full%%$'\n'*}"
  [[ "$actual" == "$expected" ]] ||
    fail "rust_${tool}_version" "expected=$expected actual=${actual:-none} output=${full//$'\n'/; }"
  pass "rust_${tool}_version" "version=$actual"
done

if ! targets_out="$(rustup target list --installed 2>&1)"; then
  fail "rust_targets" "rustup target list --installed failed output=${targets_out//$'\n'/; }"
fi
for target in "${REQUIRED_RUST_TARGETS[@]}"; do
  grep -qx "$target" <<<"$targets_out" ||
    fail "rust_targets" "missing installed target=$target installed=${targets_out//$'\n'/; }"
done
pass "rust_targets" "targets=${REQUIRED_RUST_TARGETS[*]}"

if ! cfg_out="$(rustc --print cfg --target "$AARCH64_OHOS_TARGET" 2>&1)"; then
  fail "rust_aarch64_cfg" "rustc --print cfg failed target=$AARCH64_OHOS_TARGET output=${cfg_out//$'\n'/; }"
fi
for kv in "${REQUIRED_AARCH64_CFG[@]}"; do
  grep -Fqx "$kv" <<<"$cfg_out" ||
    fail "rust_aarch64_cfg" "target=$AARCH64_OHOS_TARGET missing=$kv"
done
pass "rust_aarch64_cfg" "target=$AARCH64_OHOS_TARGET required=${REQUIRED_AARCH64_CFG[*]}"

printf 'TOOLCHAIN_CHECK=ok version_dir=%s current=%s rust_prefix=%s\n' "$CLT_DIR" "$current_target" "$CARGO_HOME"
exit 0
