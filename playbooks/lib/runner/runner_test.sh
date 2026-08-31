#!/usr/bin/env bash
# Regression tests for the runner prepare and deploy lanes.
#
#     bash playbooks/lib/runner/runner_test.sh
#
# The verify lane has its own suite next door (runner_verify_test.sh); both run
# on the stub harness in runner_test_support.sh.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./runner_test_support.sh
source "$SCRIPT_DIR/runner_test_support.sh"

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
test_should_select_production_runner() {
  local name="test_should_select_production_runner"
  local output status=0
  output="$(run_script ENV=prod RUNNER_ITEM=prod-runner bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif ! grep -q 'ZMB_CD_PROD/prod-runner/runner-token' "$calls"; then
    bad "$name" "production runner item was not used"
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
    "$REPO_ROOT/.github/workflows/deploy-dev-metal.yml"
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
test_should_prepare_host_without_reading_runner_token
test_should_require_host_prepare_approval
test_should_refuse_host_without_required_cgroup_support
test_should_deploy_development_without_secret_arguments
test_should_select_production_runner
test_should_not_install_packages_during_deploy
test_should_include_sbin_when_checking_host_tools
test_should_reject_placeholder_token
test_should_use_canonical_unit_refresh
test_should_reject_shell_unsafe_runner_inputs
test_should_require_vault_read_approval
test_should_declare_workflow_vault_read_approval
report_results
