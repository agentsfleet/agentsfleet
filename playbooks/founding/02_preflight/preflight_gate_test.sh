#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GATE="$SCRIPT_DIR/00_gate.sh"
passed=0
failed=0
work_dir="$(mktemp -d)"
fixture_dir="$work_dir/preflight"
calls="$work_dir/calls"
mkdir -p "$fixture_dir"
trap 'rm -rf -- "$work_dir"' EXIT

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

cp "$GATE" "$fixture_dir/00_gate.sh"
for step in 01_tools_and_auth.sh 02_credentials.sh 03_vercel_envs.sh; do
  printf '%s\n' '#!/usr/bin/env bash' \
    'printf "%s|%s|%s\n" "$(basename "$0")" "$ENV" "$STAGE" >>"$PREFLIGHT_CALLS"' \
    >"$fixture_dir/$step"
  chmod +x "$fixture_dir/$step"
done

run_gate() {
  local environment="$1"
  local stage="$2"
  : >"$calls"
  env \
    PREFLIGHT_CALLS="$calls" \
    ENV="$environment" \
    STAGE="$stage" \
    bash "$fixture_dir/00_gate.sh" 2>&1
}

test_should_dispatch_bootstrap_steps_in_order() {
  local name="test_should_dispatch_bootstrap_steps_in_order"
  local output status=0
  local expected=$'01_tools_and_auth.sh|all|bootstrap\n02_credentials.sh|all|bootstrap'
  output="$(run_gate all bootstrap)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(< "$calls")" != "$expected" ]; then
    bad "$name" "bootstrap dispatch did not match the explicit order"
  else
    ok "$name"
  fi
}

test_should_dispatch_vercel_only_for_prod_scoped_deployment() {
  local name="test_should_dispatch_vercel_only_for_prod_scoped_deployment"
  local output status=0
  local expected=$'01_tools_and_auth.sh|all|deployment\n02_credentials.sh|all|deployment\n03_vercel_envs.sh|all|deployment'
  output="$(run_gate all deployment)" || status=$?
  if [ "$status" -ne 0 ] || [ "$(< "$calls")" != "$expected" ]; then
    bad "$name" "all-environment deployment dispatch was incomplete: $output"
    return
  fi
  status=0
  output="$(run_gate dev deployment)" || status=$?
  expected=$'01_tools_and_auth.sh|dev|deployment\n02_credentials.sh|dev|deployment'
  if [ "$status" -ne 0 ] || [ "$(< "$calls")" != "$expected" ]; then
    bad "$name" "development dispatch called the production Vercel check: $output"
  else
    ok "$name"
  fi
}

test_should_reject_unknown_stage_before_dispatch() {
  local name="test_should_reject_unknown_stage_before_dispatch"
  local output status=0
  output="$(run_gate all operations)" || status=$?
  if [ "$status" -ne 2 ] || [ -s "$calls" ]; then
    bad "$name" "unknown stage returned $status or executed a step: $output"
  else
    ok "$name"
  fi
}

test_should_reject_unknown_environment_before_dispatch() {
  local name="test_should_reject_unknown_environment_before_dispatch"
  local output status=0
  output="$(run_gate staging bootstrap)" || status=$?
  if [ "$status" -ne 2 ] || [ -s "$calls" ]; then
    bad "$name" "unknown environment returned $status or executed a step: $output"
  else
    ok "$name"
  fi
}

test_should_stop_when_an_explicit_step_is_not_executable() {
  local name="test_should_stop_when_an_explicit_step_is_not_executable"
  local output status=0
  chmod -x "$fixture_dir/02_credentials.sh"
  output="$(run_gate all bootstrap)" || status=$?
  chmod +x "$fixture_dir/02_credentials.sh"
  if [ "$status" -eq 0 ]; then
    bad "$name" "non-executable credential step passed: $output"
  elif [ "$(< "$calls")" != '01_tools_and_auth.sh|all|bootstrap' ]; then
    bad "$name" "dispatch continued after the non-executable step"
  else
    ok "$name"
  fi
}

test_should_dispatch_bootstrap_steps_in_order
test_should_dispatch_vercel_only_for_prod_scoped_deployment
test_should_reject_unknown_stage_before_dispatch
test_should_reject_unknown_environment_before_dispatch
test_should_stop_when_an_explicit_step_is_not_executable

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
