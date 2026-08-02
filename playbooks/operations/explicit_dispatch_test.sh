#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
passed=0
failed=0
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

run_case() {
  local name="$1"
  local source_gate="$2"
  local expected="$3"
  local case_dir="$work_dir/$name"
  local calls="$case_dir/calls"
  local output status=0 actual step first_step
  shift 3
  first_step="$1"

  mkdir -p "$case_dir"
  cp "$source_gate" "$case_dir/00_gate.sh"
  : >"$calls"

  for step in "$@" 09_unexpected.sh; do
    printf '%s\n' \
      '#!/usr/bin/env bash' \
      'printf '\''%s\n'\'' "${0##*/}" >>"$CALLS"' \
      >"$case_dir/$step"
    chmod +x "$case_dir/$step"
  done

  output="$(ENV=dev CALLS="$calls" bash "$case_dir/00_gate.sh" 2>&1)" ||
    status=$?
  actual="$(paste -sd ' ' "$calls")"
  if [ "$status" -ne 0 ]; then
    bad "${name}_uses_explicit_order" "$output"
  elif [ "$actual" != "$expected" ]; then
    bad "${name}_uses_explicit_order" "expected '$expected'; got '$actual'"
  else
    ok "${name}_uses_explicit_order"
  fi

  : >"$calls"
  chmod -x "$case_dir/$first_step"
  status=0
  output="$(ENV=dev CALLS="$calls" bash "$case_dir/00_gate.sh" 2>&1)" ||
    status=$?
  if [ "$status" -eq 0 ]; then
    bad "${name}_rejects_non_executable_step" \
      "$first_step ran without execute permission"
  elif [ -s "$calls" ]; then
    bad "${name}_rejects_non_executable_step" \
      "dispatch continued after $first_step failed"
  else
    ok "${name}_rejects_non_executable_step"
  fi
}

run_case credential_rotation \
  "$SCRIPT_DIR/credential_rotation/00_gate.sh" \
  '01_vault_sync.sh 02_service_health.sh' \
  01_vault_sync.sh 02_service_health.sh

run_case redis_teardown \
  "$SCRIPT_DIR/teardown/redis/00_gate.sh" \
  '01_credential_check.sh 02_teardown.sh 03_verify.sh' \
  01_credential_check.sh 02_teardown.sh 03_verify.sh

run_case database_teardown \
  "$SCRIPT_DIR/teardown/database/00_gate.sh" \
  '01_credential_check.sh 02_teardown.sh 03_verify.sh' \
  01_credential_check.sh 02_teardown.sh 03_verify.sh

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
