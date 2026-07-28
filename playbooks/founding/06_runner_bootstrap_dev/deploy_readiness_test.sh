#!/usr/bin/env bash
# Regression tests for delegated runner cgroup deployment readiness.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly script_under_test="$script_dir/03_deploy_readiness.sh"

passed=0
failed=0

ok() { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

work_dir="$(mktemp -d)"
readonly work_dir
readonly stub_dir="$work_dir/bin"
mkdir -p "$stub_dir"
cleanup() { rm -rf "$work_dir"; }
trap cleanup EXIT

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
case "${2:-}" in
  */ssh-private-key) printf 'stub-private-key\n' ;;
  */tailscale-hostname) printf 'stub-host\n' ;;
  */deploy-user) printf 'stub-user\n' ;;
  *) exit 1 ;;
esac
STUB

cat >"$stub_dir/ssh" <<'STUB'
#!/usr/bin/env bash
command="${*: -1}"
case "$command" in
  'echo ok') printf 'ok\n' ;;
  *"/opt/agentsfleet/.env"*) printf '600 regular file\n' ;;
  *"stat -c"*) printf '755 regular file\n' ;;
  *"test -x"*) printf 'executable\n' ;;
  *"--property=Delegate --value"*) printf '%s\n' "${TEST_DELEGATE:-yes}" ;;
  *"--property=DelegateSubgroup --value"*) printf '%s\n' "${TEST_SUBGROUP:-runner}" ;;
  *"--property=ControlGroup --value"*) printf '%s\n' "${TEST_CGROUP_PATH:-/system.slice/agentsfleet-runner.service}" ;;
  *"cgroup.subtree_control"*) printf '%s\n' "${TEST_CONTROLLERS:-cpu memory pids}" ;;
  *"BWRAP_VERSION="*)
    printf '%s\n' 'BWRAP_VERSION=bwrap 0.11.0' 'BWRAP_INFO_FD=1' 'BWRAP_BLOCK_FD=1' 'NFT_VERSION=nftables v1.0' 'IP_VERSION=iproute2-6.0'
    ;;
  *) printf 'unexpected SSH command: %s\n' "$command" >&2; exit 1 ;;
esac
STUB

chmod +x "$stub_dir/op" "$stub_dir/ssh"

run_readiness() {
  env PATH="$stub_dir:$PATH" \
    OP_READ_RETRIES=1 \
    OP_READ_MIN_INTERVAL_SECONDS=0 \
    REQUIRE_RUNNER_CGROUP_DELEGATION=1 \
    "$@" \
    bash "$script_under_test" 2>&1
}

test_should_accept_delegated_runner_cgroup() {
  local name="test_should_accept_delegated_runner_cgroup"
  local output status=0
  output="$(run_readiness)" || status=$?
  if [[ "$status" -ne 0 ]]; then
    bad "$name" "valid delegated unit failed: $output"
  elif [[ "$output" != *"runner cgroup delegation: /system.slice/agentsfleet-runner.service"* ]]; then
    bad "$name" "valid delegated unit was not reported: $output"
  else
    ok "$name"
  fi
}

test_should_reject_missing_delegation() {
  local name="test_should_reject_missing_delegation"
  local output status=0
  output="$(run_readiness TEST_DELEGATE=no)" || status=$?
  if [[ "$status" -eq 0 ]]; then
    bad "$name" "missing delegation passed: $output"
  elif [[ "$output" != *"Delegate=no"* ]]; then
    bad "$name" "missing delegation diagnostic was absent: $output"
  else
    ok "$name"
  fi
}

test_should_reject_unexpected_delegation_subgroup() {
  local name="test_should_reject_unexpected_delegation_subgroup"
  local output status=0
  output="$(run_readiness TEST_SUBGROUP=other)" || status=$?
  if [[ "$status" -eq 0 ]]; then
    bad "$name" "unexpected subgroup passed: $output"
  elif [[ "$output" != *"DelegateSubgroup=other"* ]]; then
    bad "$name" "unexpected subgroup diagnostic was absent: $output"
  else
    ok "$name"
  fi
}

test_should_reject_unexpected_control_group() {
  local name="test_should_reject_unexpected_control_group"
  local output status=0
  output="$(run_readiness TEST_CGROUP_PATH=/system.slice/other.service)" || status=$?
  if [[ "$status" -eq 0 ]]; then
    bad "$name" "unexpected control group passed: $output"
  elif [[ "$output" != *"unexpected control group '/system.slice/other.service'"* ]]; then
    bad "$name" "unexpected control-group diagnostic was absent: $output"
  else
    ok "$name"
  fi
}

test_should_reject_missing_controller() {
  local name="test_should_reject_missing_controller"
  local output status=0
  output="$(run_readiness TEST_CONTROLLERS='cpu memory')" || status=$?
  if [[ "$status" -eq 0 ]]; then
    bad "$name" "missing pids controller passed: $output"
  elif [[ "$output" != *"controller 'pids' is not enabled"* ]]; then
    bad "$name" "missing controller diagnostic was absent: $output"
  else
    ok "$name"
  fi
}

test_should_accept_delegated_runner_cgroup
test_should_reject_missing_delegation
test_should_reject_unexpected_delegation_subgroup
test_should_reject_unexpected_control_group
test_should_reject_missing_controller

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
