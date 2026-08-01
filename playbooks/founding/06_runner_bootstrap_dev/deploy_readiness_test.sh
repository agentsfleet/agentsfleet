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
  # Ordered before the generic subtree_control arm: the parent slice's
  # delegation is what the PRE-deploy probe reads, the service's own subtree is
  # what the POST-deploy check reads, and the two must be stubbable apart.
  # `${VAR-default}` not `${VAR:-default}`: an EMPTY controller set is the state
  # under test (a subtree the daemon never wrote), and `:-` would replace it
  # with the default and quietly assert nothing.
  *"/sys/fs/cgroup/cgroup.controllers"*) printf '%s\n' "${TEST_ROOT_CONTROLLERS-cpuset cpu io memory hugetlb pids rdma misc}" ;;
  *"/sys/fs/cgroup/system.slice/cgroup.subtree_control"*) printf '%s\n' "${TEST_SLICE_CONTROLLERS-cpu memory pids}" ;;
  *"cgroup.subtree_control"*) printf '%s\n' "${TEST_CONTROLLERS-cpu memory pids}" ;;
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

run_host_probe() {
  env PATH="$stub_dir:$PATH" \
    OP_READ_RETRIES=1 \
    OP_READ_MIN_INTERVAL_SECONDS=0 \
    REQUIRE_HOST_CGROUP_CAPABILITY=1 \
    "$@" \
    bash "$script_under_test" 2>&1
}

test_gate_reports_all_missing_controllers() {
  local name="test_gate_reports_all_missing_controllers"
  local output status=0 found=0 controller
  # A host whose delegated subtree is entirely empty — the exact state a runner
  # sits in before it has written subtree_control. All three must be named in
  # ONE run; the old gate returned on the first and reported only 'cpu'.
  output="$(run_readiness TEST_CONTROLLERS='')" || status=$?
  if [[ "$status" -eq 0 ]]; then
    bad "$name" "an empty delegated subtree passed: $output"
    return
  fi
  for controller in cpu memory pids; do
    [[ "$output" == *"controller '$controller' is not enabled"* ]] && found=$((found + 1))
  done
  if [[ "$found" -ne 3 ]]; then
    bad "$name" "named $found/3 missing controllers in one run: $output"
  else
    ok "$name"
  fi
}

test_pre_deploy_probe_rejects_incapable_host() {
  local name="test_pre_deploy_probe_rejects_incapable_host"
  local output status=0
  # A kernel booted hybrid/v1 offers no 'cpu' in the unified root.
  output="$(run_host_probe TEST_ROOT_CONTROLLERS='cpuset io memory pids')" || status=$?
  if [[ "$status" -eq 0 ]]; then
    bad "$name" "a kernel with no cpu controller passed the pre-deploy probe: $output"
  elif [[ "$output" != *"kernel offers no 'cpu' controller"* ]]; then
    bad "$name" "pre-deploy diagnostic did not name the missing controller: $output"
  else
    ok "$name"
  fi
}

test_pre_deploy_probe_rejects_undelegated_slice() {
  local name="test_pre_deploy_probe_rejects_undelegated_slice"
  local output status=0
  # Kernel is capable, but systemd does not delegate down to the slice — so no
  # unit beneath it could ever receive the controllers.
  output="$(run_host_probe TEST_SLICE_CONTROLLERS='memory pids')" || status=$?
  if [[ "$status" -eq 0 ]]; then
    bad "$name" "an undelegated parent slice passed: $output"
  elif [[ "$output" != *"system.slice does not delegate 'cpu'"* ]]; then
    bad "$name" "undelegated-slice diagnostic was absent: $output"
  else
    ok "$name"
  fi
}

test_pre_deploy_probe_accepts_capable_host() {
  local name="test_pre_deploy_probe_accepts_capable_host"
  local output status=0
  output="$(run_host_probe)" || status=$?
  if [[ "$status" -ne 0 ]]; then
    bad "$name" "a capable host failed the pre-deploy probe: $output"
  elif [[ "$output" != *"host cgroup capability: system.slice delegates"* ]]; then
    bad "$name" "capable host was not reported: $output"
  else
    ok "$name"
  fi
}

test_post_deploy_check_requires_delegated_subtree() {
  local name="test_post_deploy_check_requires_delegated_subtree"
  local output status=0
  # The pre-deploy probe passing must NOT excuse an empty delegated subtree:
  # a capable host still fails until the daemon has enabled its own controllers.
  output="$(run_readiness TEST_CONTROLLERS='' TEST_ROOT_CONTROLLERS='cpuset cpu io memory hugetlb pids rdma misc')" || status=$?
  if [[ "$status" -eq 0 ]]; then
    bad "$name" "a capable host with an empty delegated subtree passed: $output"
  else
    ok "$name"
  fi
}

test_should_accept_delegated_runner_cgroup
test_gate_reports_all_missing_controllers
test_pre_deploy_probe_rejects_incapable_host
test_pre_deploy_probe_rejects_undelegated_slice
test_pre_deploy_probe_accepts_capable_host
test_post_deploy_check_requires_delegated_subtree
test_should_reject_missing_delegation
test_should_reject_unexpected_delegation_subgroup
test_should_reject_unexpected_control_group
test_should_reject_missing_controller

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[[ "$failed" -eq 0 ]]
