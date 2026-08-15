#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../test_search.sh
source "$SCRIPT_DIR/../test_search.sh"
readonly PREPARE="$SCRIPT_DIR/prepare.sh"
readonly DEPLOY="$SCRIPT_DIR/deploy.sh"
readonly VERIFY="$SCRIPT_DIR/verify.sh"
readonly DEPLOY_SCRIPT="$SCRIPT_DIR/../../../deploy/baremetal/deploy.sh"
readonly REPO_ROOT="$SCRIPT_DIR/../../.."
passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
mkdir -p "$stub_dir"
trap 'rm -rf "$work_dir"' EXIT

ok() { printf 'ok   %s\n' "$1"; passed=$((passed + 1)); }
bad() { printf 'FAIL %s\n       %s\n' "$1" "$2" >&2; failed=$((failed + 1)); }

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
case "${1:-}" in
  whoami) printf 'stub-user\n' ;;
  read)
    case "${2:-}" in
      */tailscale-hostname) printf 'runner-host\n' ;;
      */deploy-user) printf 'runner-user\n' ;;
      */runner-token) printf '%s\n' "${STUB_TOKEN:-agt_rREAL_TOKEN}" ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB

cat >"$stub_dir/tailscale" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
if [ "${1:-}" = status ]; then
  printf '{"Self":{"Online":true}}\n'
  exit 0
fi
command="${*: -1}"
if [ -n "${TAILSCALE_FAIL_MATCH:-}" ] && [[ "$command" == *"$TAILSCALE_FAIL_MATCH"* ]]; then
  exit 9
fi
if [ "${STUB_EXECUTE_CGROUP_GATE:-0}" = 1 ] &&
  [[ "$command" == *"/sys/fs/cgroup/cgroup.controllers"* ]]
then
  command="${command//\/sys\/fs\/cgroup/$STUB_CGROUP_ROOT}"
  bash -c "$command"
  exit $?
fi
case "$command" in
  *"BWRAP_VERSION="*)
    printf '%s\n' \
      'BWRAP_VERSION=bwrap 0.11.0' \
      'BWRAP_INFO_FD=1' \
      'BWRAP_BLOCK_FD=1' \
      'NFT_VERSION=nftables v1.0' \
      'IP_VERSION=iproute2-6.0'
    ;;
  *"cgroup.subtree_control"*)
    printf '%s\n' \
      'root_controllers=cpu memory pids' \
      'root_subtree=cpu memory pids' \
      'slice_controllers=cpu memory pids' \
      'slice_subtree=cpu memory pids' \
      'service_controllers=cpu memory pids' \
      "service_subtree=${STUB_CGROUP_CONTROLLERS:-cpu memory pids}"
    ;;
esac
cat >/dev/null || true
STUB
chmod +x "$stub_dir/op" "$stub_dir/tailscale"
runner_binary="$work_dir/agentsfleet-runner"
printf 'runner\n' >"$runner_binary"
cgroup_fixture="$work_dir/cgroup"
mkdir -p "$cgroup_fixture"
run_script() {
  : >"$calls"
  # env -i: the child sees ONLY what this harness assigns plus the VAR=val
  # pairs each case passes before its command. Cases that mean to exercise a
  # variable pass it as an argument below, which still wins.
  #
  # The forcing case is AGENTSFLEET_API_URL: `common.sh` reads it as
  # `${AGENTSFLEET_API_URL:-$expected_api_url}` and refuses the deploy when it
  # disagrees with ENV's endpoint, so a developer shell pointed at api-dev
  # fails the ENV=prod case alone, on that machine only. Dropping that one
  # variable fixes that one bug; wiping the environment closes the whole class,
  # since every variable these scripts read is supplied by this harness or by
  # the case. PATH is composed before the wipe, so the stub dir still shadows
  # the real tools while the system tail keeps bash findable.
  env -i \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    RUNNER_BINARY="$runner_binary" \
    RUNNER_VERSION=test-build \
    ALLOW_VAULT_READS=1 \
    "$@" 2>&1
}
test_should_prepare_host_without_reading_runner_token() {
  local name="test_should_prepare_host_without_reading_runner_token"
  local output status=0
  output="$(
    run_script \
      ENV=dev \
      ALLOW_VAULT_READS=1 \
      ALLOW_RUNNER_HOST_PREPARE=1 \
      bash "$PREPARE"
  )" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif grep -q '/runner-token' "$calls"; then
    bad "$name" "host preparation read a runner token"
  elif ! grep -q 'apt-get install' "$calls"; then
    bad "$name" "host preparation did not install dependencies"
  elif grep -q '/opt/agentsfleet/deploy/deploy.sh runner' "$calls"; then
    bad "$name" "host preparation deployed a runner"
  else
    ok "$name"
  fi
}
test_should_require_host_prepare_approval() {
  local name="test_should_require_host_prepare_approval"
  local output status=0
  output="$(
    run_script \
      ENV=dev \
      ALLOW_VAULT_READS=1 \
      ALLOW_RUNNER_HOST_PREPARE=0 \
      bash "$PREPARE"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "host preparation ran without Human approval"
  else
    ok "$name"
  fi
}
test_should_refuse_host_without_required_cgroup_support() {
  local name="test_should_refuse_host_without_required_cgroup_support"
  local output status=0
  printf 'cpuset io\n' >"$cgroup_fixture/cgroup.controllers"
  output="$(
    run_script \
      ENV=dev \
      ALLOW_VAULT_READS=1 \
      ALLOW_RUNNER_HOST_PREPARE=1 \
      STUB_EXECUTE_CGROUP_GATE=1 \
      STUB_CGROUP_ROOT="$cgroup_fixture" \
      bash "$PREPARE"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "host preparation passed without required cgroup support"
    return
  elif ! grep -q '/sys/fs/cgroup/cgroup.controllers' "$calls"; then
    bad "$name" "host preparation did not check cgroup support"
    return
  elif ! grep -Fq 'for controller in cpu memory pids; do' "$calls" ||
    [[ "$output" != *"ERROR: required cgroup v2 controller unavailable: cpu"* ]]
  then
    bad "$name" "host preparation did not identify every required cgroup controller"
    return
  elif grep -Eq 'apt-get install|mkdir -p|chown -R' "$calls"; then
    bad "$name" "host preparation changed the host before the cgroup gate"
    return
  fi
  status=0
  output="$(
    run_script \
      ENV=dev \
      STUB_EXECUTE_CGROUP_GATE=1 \
      STUB_CGROUP_ROOT="$cgroup_fixture" \
      bash "$DEPLOY"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "runner deployment passed without required cgroup support"
  elif ! grep -q '/sys/fs/cgroup/cgroup.controllers' "$calls"; then
    bad "$name" "runner deployment did not check cgroup support"
  elif ! grep -Fq 'for controller in cpu memory pids; do' "$calls" ||
    [[ "$output" != *"ERROR: required cgroup v2 controller unavailable: cpu"* ]]
  then
    bad "$name" "runner deployment did not identify every required cgroup controller"
  elif grep -q '/opt/agentsfleet' "$calls"; then
    bad "$name" "runner deployment changed the host before the cgroup gate"
  else
    ok "$name"
  fi
}
test_should_deploy_development_without_secret_arguments() {
  local name="test_should_deploy_development_without_secret_arguments"
  local output status=0
  output="$(run_script ENV=dev bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif grep -q 'agt_rREAL_TOKEN' "$calls"; then
    bad "$name" "runner token appeared in a command argument"
  elif ! grep -q 'ZMB_CD_DEV/agentsfleet-dev-runner-ant/runner-token' "$calls"; then
    bad "$name" "development vault selection was not used"
  else
    ok "$name"
  fi
}
test_should_select_production_worker() {
  local name="test_should_select_production_worker"
  local output status=0
  output="$(run_script ENV=prod WORKER_ITEM=prod-worker bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif ! grep -q 'ZMB_CD_PROD/prod-worker/runner-token' "$calls"; then
    bad "$name" "production worker item was not used"
  else
    ok "$name"
  fi
}
test_should_not_install_packages_during_deploy() {
  local name="test_should_not_install_packages_during_deploy"
  local output status=0
  output="$(run_script ENV=dev bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif grep -q 'apt-get install' "$calls"; then
    bad "$name" "routine deployment attempted package installation"
  else
    ok "$name"
  fi
}
test_should_include_sbin_when_checking_host_tools() {
  local name="test_should_include_sbin_when_checking_host_tools"
  local output status=0
  output="$(run_script ENV=dev ALLOW_RUNNER_HOST_PREPARE=1 bash "$PREPARE")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
    return
  elif ! awk '
    $0 == "    export PATH=\"/usr/sbin:/sbin:$PATH\"" {
      getline
      if ($0 == "    test \"$(tailscale status --json | jq -r .Self.Online)\" = true") found=1
    }
    END { exit !found }
  ' "$calls"; then
    bad "$name" "host preparation did not expose Debian sbin tools"
    return
  fi

  status=0
  output="$(run_script ENV=dev bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif ! awk '
    $0 == "    export PATH=\"/usr/sbin:/sbin:$PATH\"" {
      getline
      if ($0 == "    test -d /opt/agentsfleet/bin") found=1
    }
    END { exit !found }
  ' "$calls"; then
    bad "$name" "runner deployment did not expose Debian sbin tools"
  else
    ok "$name"
  fi
}
test_should_reject_placeholder_token() {
  local name="test_should_reject_placeholder_token"
  local output status=0
  output="$(run_script ENV=dev STUB_TOKEN=agt_rFAKE_TOKEN bash "$DEPLOY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "placeholder token passed"
  elif [[ "$output" != *"placeholder runner token"* ]]; then
    bad "$name" "$output"
  else
    ok "$name"
  fi
}
test_should_use_canonical_unit_refresh() {
  local name="test_should_use_canonical_unit_refresh"
  local output status=0
  output="$(run_script ENV=dev bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif ! grep -q '/opt/agentsfleet/deploy/deploy.sh runner' "$calls"; then
    bad "$name" "runner deployment bypassed the canonical deploy path"
  elif ! grep -q '^sync_systemd_unit()' "$DEPLOY_SCRIPT"; then
    bad "$name" "canonical deploy path no longer refreshes the runner unit"
  elif ! grep -q 'systemctl daemon-reload' "$DEPLOY_SCRIPT"; then
    bad "$name" "canonical deploy path no longer reloads systemd"
  elif ! grep -q 'systemctl enable "$SERVICE_NAME"' "$DEPLOY_SCRIPT"; then
    bad "$name" "canonical deploy path no longer enables the runner service"
  else
    ok "$name"
  fi
}
test_should_reject_shell_unsafe_runner_inputs() {
  local name="test_should_reject_shell_unsafe_runner_inputs"
  local output status=0
  output="$(run_script ENV=dev STUB_TOKEN='agt_r;false' bash "$DEPLOY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "shell-unsafe runner token passed"
    return
  fi
  status=0
  output="$(run_script ENV=dev AGENTSFLEET_API_URL='https://api.example.test;false' bash "$DEPLOY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "shell-unsafe API URL passed"
  else
    ok "$name"
  fi
}
test_should_require_vault_read_approval() {
  local name="test_should_require_vault_read_approval"
  local output status=0
  output="$(run_script ENV=dev ALLOW_VAULT_READS=0 bash "$DEPLOY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "runner deployment read the vault without approval"
  elif [[ "$output" != *"vault read approval required"* ]]; then
    bad "$name" "runner deployment omitted the approval error: $output"
  elif grep -qE '^(read|ssh )' "$calls"; then
    bad "$name" "runner deployment reached the vault or host without approval"
  else
    ok "$name"
  fi
}
test_should_declare_workflow_vault_read_approval() {
  local name="test_should_declare_workflow_vault_read_approval"
  local workflow
  local -a workflows=(
    "$REPO_ROOT/.github/workflows/deploy-dev-worker.yml"
    "$REPO_ROOT/.github/workflows/release.yml"
  )
  for workflow in "${workflows[@]}"; do
    if ! rg --fixed-strings --quiet 'ALLOW_VAULT_READS: "1"' "$workflow"; then
      bad "$name" "$(basename "$workflow") omits runner vault-read approval"
      return
    fi
  done
  ok "$name"
}
test_should_fail_when_cpu_controller_is_not_delegated() {
  local name="test_should_fail_when_cpu_controller_is_not_delegated"
  local output status=0
  output="$(
    run_script \
      ENV=dev \
      STUB_CGROUP_CONTROLLERS='memory pids' \
      bash "$VERIFY"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "missing cpu delegation passed"
  elif [[ "$output" != *"cgroup controller is not delegated: cpu"* ]]; then
    bad "$name" "$output"
  else
    ok "$name"
  fi
}
test_should_fail_verification_when_service_check_fails() {
  local name="test_should_fail_verification_when_service_check_fails"
  local output status=0
  output="$(run_script \
    ENV=dev \
    TAILSCALE_FAIL_MATCH='systemctl is-active' \
    bash "$VERIFY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "failed remote service check passed"
  else
    ok "$name"
  fi
}
test_should_fail_verification_when_service_is_not_enabled() {
  local name="test_should_fail_verification_when_service_is_not_enabled"
  local output status=0
  output="$(run_script \
    ENV=dev \
    TAILSCALE_FAIL_MATCH='systemctl is-enabled' \
    bash "$VERIFY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "disabled remote service passed"
  else
    ok "$name"
  fi
}
test_should_fail_verification_when_runner_token_is_rejected() {
  local name="test_should_fail_verification_when_runner_token_is_rejected"
  local output status=0
  output="$(run_script \
    ENV=dev \
    TAILSCALE_FAIL_MATCH='agentsfleet-runner doctor' \
    bash "$VERIFY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "failed runner identity check passed"
  else
    ok "$name"
  fi
}
test_should_verify_without_reading_runner_token() {
  local name="test_should_verify_without_reading_runner_token"
  local output status=0
  output="$(run_script ENV=dev bash "$VERIFY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif grep -q '/runner-token' "$calls"; then
    bad "$name" "read-only verification read a runner token"
  elif ! grep -q 'systemctl is-enabled' "$calls"; then
    bad "$name" "verification did not check reboot enablement"
  elif ! grep -q 'agentsfleet-runner doctor' "$calls"; then
    bad "$name" "verification did not prove runner token validity"
  else
    ok "$name"
  fi
}
test_should_prepare_host_without_reading_runner_token
test_should_require_host_prepare_approval
test_should_refuse_host_without_required_cgroup_support
test_should_deploy_development_without_secret_arguments
test_should_select_production_worker
test_should_not_install_packages_during_deploy
test_should_include_sbin_when_checking_host_tools
test_should_reject_placeholder_token
test_should_use_canonical_unit_refresh
test_should_reject_shell_unsafe_runner_inputs
test_should_require_vault_read_approval
test_should_declare_workflow_vault_read_approval
test_should_fail_when_cpu_controller_is_not_delegated
test_should_fail_verification_when_service_check_fails
test_should_fail_verification_when_service_is_not_enabled
test_should_fail_verification_when_runner_token_is_rejected
test_should_verify_without_reading_runner_token
printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
