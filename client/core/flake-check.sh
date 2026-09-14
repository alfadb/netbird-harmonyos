#!/usr/bin/env bash
# NetBird client core — flake pressure test (offline, no source changes).
# Runs the full test suite N rounds in a row and reports every failure's
# failing test names + assertion lines, then a round-level summary.
#
# Usage: bash client/core/flake-check.sh [rounds]     (default 20)
#
# Exit code: 0 = every round green; 1 = at least one round had failures
# (round-by-round detail is printed as it happens).
#
# Environment (frozen, mirrors build.sh):
#   RUSTUP_HOME=/home/worker/rust/rustup CARGO_HOME=/home/worker/rust/cargo
#   PATH=$CARGO_HOME/bin:$PATH
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export RUSTUP_HOME="${RUSTUP_HOME:-/home/worker/rust/rustup}"
export CARGO_HOME="${CARGO_HOME:-/home/worker/rust/cargo}"
export PATH="$CARGO_HOME/bin:$PATH"

ROUNDS="${1:-20}"
case "$ROUNDS" in
    ''|*[!0-9]*) printf '[flake-check] ERROR: rounds must be a positive integer, got: %s\n' "$ROUNDS" >&2; exit 1 ;;
esac

log() { printf '[flake-check] %s\n' "$*"; }

cd "$ROOT"

FAILED_ROUNDS=0
for round in $(seq 1 "$ROUNDS"); do
    OUT_FILE="$ROOT/target/flake-check-round-${round}.log"
    log "=== round ${round}/${ROUNDS} ==="
    # --color never: no ANSI codes in the captured log (keeps the grep
    # patterns exact)
    if cargo test --offline --locked --color never 2>&1 | tee "$OUT_FILE"; then
        rm -f "$OUT_FILE"
        log "round ${round}: PASS"
    else
        FAILED_ROUNDS=$((FAILED_ROUNDS + 1))
        log "round ${round}: FAIL — failing tests + assertion lines:"
        # failing test headers (libtest: "test <name> ... FAILED" / ": FAILED")
        grep -E '^test [^ ].* (FAILED|panicked)' "$OUT_FILE" || true
        grep -E ': FAILED$' "$OUT_FILE" || true
        # panic sites with their source location + the message that follows
        grep -A3 'panicked at' "$OUT_FILE" || true
        log "full round log kept at: $OUT_FILE"
    fi
done

log "summary: ${FAILED_ROUNDS}/${ROUNDS} rounds with failures"
[ "$FAILED_ROUNDS" -eq 0 ]
log "all rounds green"