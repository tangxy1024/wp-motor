#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

declare -A STATUS=()
declare -A NOTE=()

run_stage() {
    local key="$1"
    local cmd="$2"
    local log_file
    log_file="$(mktemp)"

    echo "==> Running: $cmd"
    if bash -lc "$cmd" >"$log_file" 2>&1; then
        STATUS["$key"]="PASS"
    else
        STATUS["$key"]="FAIL"
    fi

    case "$key" in
    "cargo check")
        local err_count
        err_count="$(grep -c '^error' "$log_file" || true)"
        NOTE["$key"]="errors=$err_count"
        ;;
    "cargo clippy")
        local warn_count
        warn_count="$(grep -c '^warning' "$log_file" || true)"
        NOTE["$key"]="warnings=$warn_count"
        ;;
    "cargo test")
        local summary
        summary="$(grep 'test result:' "$log_file" | tail -n 1 || true)"
        NOTE["$key"]="${summary:-no test summary}"
        ;;
    esac

    if [[ "${STATUS[$key]}" == "FAIL" ]]; then
        echo "--- ${key} output (tail) ---"
        tail -n 120 "$log_file"
        echo "--- end ${key} output ---"
    fi

    rm -f "$log_file"
}

run_stage "cargo check" "cargo check --workspace 2>&1"
run_stage "cargo clippy" "cargo clippy --workspace -- -D warnings 2>&1"
run_stage "cargo test" "cargo test --workspace 2>&1"

echo
echo "| Stage | Status | Note |"
echo "|------|------|------|"
echo "| cargo check | ${STATUS["cargo check"]} | ${NOTE["cargo check"]} |"
echo "| cargo clippy | ${STATUS["cargo clippy"]} | ${NOTE["cargo clippy"]} |"
echo "| cargo test | ${STATUS["cargo test"]} | ${NOTE["cargo test"]} |"

if [[ "${STATUS["cargo check"]}" == "FAIL" || "${STATUS["cargo clippy"]}" == "FAIL" || "${STATUS["cargo test"]}" == "FAIL" ]]; then
    exit 1
fi

