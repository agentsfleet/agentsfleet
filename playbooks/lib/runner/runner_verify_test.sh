#!/usr/bin/env bash
# Regression tests for the runner verify lane.
#
#     bash playbooks/lib/runner/runner_verify_test.sh
#
# The readiness cases guard the Aug 20, 2026 dev-worker failure: the runner
# deployed and came up active, then the gate's single-shot /readyz curl caught
# Cloudflare mid tunnel-reconnect, took a 530, and failed the whole job on a
# box that was fine.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./runner_test_support.sh
source "$SCRIPT_DIR/runner_test_support.sh"

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
test_should_pass_once_the_control_plane_clears_a_tunnel_reconnect() {
  local name="test_should_pass_once_the_control_plane_clears_a_tunnel_reconnect"
  local output status=0
  output="$(run_script \
    ENV=dev \
    STUB_READYZ_STATUSES='530 530 200' \
    RUNNER_READYZ_RETRY_SECONDS=0 \
    bash "$VERIFY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "a control plane that answered on the third probe failed: $output"
  elif [[ "$output" != *"answered 530"* ]]; then
    bad "$name" "the retried 530 went unreported: $output"
  elif [[ "$output" != *"PASS: agentsfleet-dev-runner-ant is ready"* ]]; then
    bad "$name" "$output"
  else
    ok "$name"
  fi
}
test_should_fail_and_name_the_status_when_the_control_plane_stays_down() {
  local name="test_should_fail_and_name_the_status_when_the_control_plane_stays_down"
  local output status=0 probes
  output="$(run_script \
    ENV=dev \
    STUB_READYZ_STATUSES='530' \
    RUNNER_READYZ_ATTEMPTS=2 \
    RUNNER_READYZ_RETRY_SECONDS=0 \
    bash "$VERIFY")" || status=$?
  probes="$(grep -c '/readyz' "$calls")"
  if [ "$status" -eq 0 ]; then
    bad "$name" "an unreachable control plane passed"
  elif [[ "$output" != *"control plane unreachable"*"530"* ]]; then
    bad "$name" "the failure did not name the last HTTP status: $output"
  elif [ "$probes" -ne 2 ]; then
    bad "$name" "expected 2 readiness probes, got $probes"
  else
    ok "$name"
  fi
}
test_should_fail_when_cpu_controller_is_not_delegated
test_should_fail_verification_when_service_check_fails
test_should_fail_verification_when_service_is_not_enabled
test_should_fail_verification_when_runner_token_is_rejected
test_should_verify_without_reading_runner_token
test_should_pass_once_the_control_plane_clears_a_tunnel_reconnect
test_should_fail_and_name_the_status_when_the_control_plane_stays_down
report_results
