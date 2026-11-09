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
# The runner's host probe requires io as well, and refuses to start without it.
test_should_fail_when_io_controller_is_not_delegated() {
  local name="test_should_fail_when_io_controller_is_not_delegated"
  local output status=0
  output="$(
    run_script \
      ENV=dev \
      STUB_CGROUP_CONTROLLERS='cpu memory pids' \
      bash "$VERIFY"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "missing io delegation passed"
  elif [[ "$output" != *"cgroup controller is not delegated: io"* ]]; then
    bad "$name" "$output"
  else
    ok "$name"
  fi
}
# A host with forwarding off runs a healthy-looking service that leases no
# allowlisted-egress work, so verification names it.
test_should_fail_when_ipv4_forwarding_is_off() {
  local name="test_should_fail_when_ipv4_forwarding_is_off"
  local output status=0
  output="$(run_script ENV=dev STUB_IP_FORWARD=0 bash "$VERIFY")" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a host with net.ipv4.ip_forward=0 passed verification"
  elif [[ "$output" != *"/proc/sys/net/ipv4/ip_forward reads '0'"* ]]; then
    bad "$name" "$output"
  elif ! grep -qF "cat '/proc/sys/net/ipv4/ip_forward'" "$calls"; then
    bad "$name" "verification never read the host's forwarding setting"
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
    TAILSCALE_FAIL_MATCH='/v1/runners/me' \
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
  elif ! grep -q 'Authorization: Bearer .*/v1/runners/me' "$calls"; then
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
test_should_fail_when_installed_stream_files_are_not_ready() {
  local boundary output status
  for boundary in 'stat -c %a /etc/default/agentsfleet-runner' 'test -r /etc/agentsfleet/runner-stream-trust.pem'; do
    status=0
    output="$(run_script ENV=dev TAILSCALE_FAIL_MATCH="$boundary" bash "$VERIFY")" || status=$?
    if [ "$status" -eq 0 ]; then
      bad "installed stream files" "$boundary failure passed verification: $output"
      return
    fi
  done
  ok "installed stream files and private unit env are checked"
}

test_should_fail_when_installed_stream_files_are_not_ready
test_should_fail_when_cpu_controller_is_not_delegated
test_should_fail_when_io_controller_is_not_delegated
test_should_fail_when_ipv4_forwarding_is_off
test_should_fail_verification_when_service_check_fails
test_should_fail_verification_when_service_is_not_enabled
test_should_fail_verification_when_runner_token_is_rejected
test_should_verify_without_reading_runner_token
test_should_pass_once_the_control_plane_clears_a_tunnel_reconnect
test_should_fail_and_name_the_status_when_the_control_plane_stays_down

# Synthetic configuration bytes only; no real WireGuard key enters this suite.
test_should_replace_dev_wireguard_only_after_stopping_old_config() {
  local output status=0 mode
  output="$(run_script ENV=dev STUB_EXECUTE_WIREGUARD=1 RUNNER_STREAM_WIREGUARD_CONFIG=first-config bash "$DEPLOY")" || status=$?
  output+="$(run_script ENV=dev STUB_EXECUTE_WIREGUARD=1 RUNNER_STREAM_WIREGUARD_CONFIG=first-config bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ] || [ "$(cat "$work_dir/wireguard/events")" != $'enable\nenable' ]; then
    bad "unchanged WireGuard configuration" "$output"
    return
  fi
  output="$(run_script ENV=dev STUB_EXECUTE_WIREGUARD=1 RUNNER_STREAM_WIREGUARD_CONFIG=second-config bash "$DEPLOY")" || status=$?
  mode="$(stat -c %a "$work_dir/wireguard/fly-dev.conf" 2>/dev/null || stat -f %Lp "$work_dir/wireguard/fly-dev.conf")"
  if [ "$status" -ne 0 ] || [ "$mode" != 600 ] ||
    [ "$(cat "$work_dir/wireguard/fly-dev.conf")" != second-config ] ||
    [ "$(cat "$work_dir/wireguard/stopped-config")" != first-config ] ||
    [ "$(cat "$work_dir/wireguard/events")" != $'enable\nenable\nstop\nenable' ]; then
    bad "changed WireGuard configuration" "$output"
  elif grep -Fq second-config "$calls" || [[ "$output" == *second-config* ]] ||
    find "$work_dir/wireguard" -name '.fly-dev.*' | grep . >/dev/null; then
    bad "WireGuard stdin confidentiality" "configuration reached argv/output or a staged copy survived"
  else
    ok "WireGuard uses stdin, protected files, and old PreDown before replacement"
  fi
}

test_should_skip_absent_and_production_wireguard() {
  local output status=0
  output="$(run_script ENV=dev STUB_MISSING_WIREGUARD=1 bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ] || grep -q '/etc/wireguard' "$calls"; then
    bad "absent WireGuard field" "$output"
    return
  fi
  output="$(run_script ENV=prod RUNNER_ITEM=prod-runner RUNNER_STREAM_WIREGUARD_CONFIG=ignored-prod-config bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ] || grep -Eq '/wireguard-config|/etc/wireguard' "$calls"; then
    bad "production WireGuard isolation" "$output"
  else
    ok "absent optional peer and production skip WireGuard"
  fi
}

test_should_probe_each_private_carrier_and_refuse_failures() {
  local output status=0 boundary
  output="$(run_script ENV=dev bash "$VERIFY")" || status=$?
  if [ "$status" -ne 0 ] || [[ "$output" != *'private runner h2:'*'private runner h3:'* ]] ||
    ! grep -Fq -- '-tls1_3 -alpn h2' "$calls" || ! grep -Fq -- '-quic -alpn h3' "$calls" ||
    ! grep -Fq -- '-groups X25519MLKEM768' "$calls" ||
    grep -Eq -- '-quic.*-tls1_3|-tls1_3.*-quic|-sess_out|-keylogfile' "$calls"; then
    bad "separate hybrid carrier probes" "$output"
    return
  fi
  for boundary in 'getent ahosts' 'http://agentsfleetd-dev.internal:3000/readyz' '-alpn h2' '-alpn h3'; do
    status=0
    output="$(run_script ENV=dev TAILSCALE_FAIL_MATCH="$boundary" bash "$VERIFY")" || status=$?
    if [ "$status" -eq 0 ]; then
      bad "private carrier failure" "$boundary passed"
      return
    fi
  done
  ok "private DNS, readiness, and both TLS carriers fail closed"
}

test_should_replace_dev_wireguard_only_after_stopping_old_config
test_should_skip_absent_and_production_wireguard
test_should_probe_each_private_carrier_and_refuse_failures
report_results
