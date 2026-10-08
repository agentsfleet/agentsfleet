#!/usr/bin/env bash
# Regression tests for what the runner deploy lane requires of a host and
# sets on it.
#
#     bash playbooks/lib/runner/runner_host_test.sh
#
# Runs on the stub harness in runner_test_support.sh, as runner_test.sh does.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./runner_test_support.sh
source "$SCRIPT_DIR/runner_test_support.sh"

# The runner's host probe and the controllers it refuses to start without.
readonly PROBE_RS="$REPO_ROOT/rustd/crates/afr_sandbox/src/probe.rs"
readonly HOST_DEPLOY_DIR="/opt/agentsfleet/deploy"
readonly FORWARDING_LINE="net.ipv4.ip_forward = 1"

# A host prepared before forwarding was part of preparation still gains it,
# because every deploy turns it on before it hands the host its runner.
test_should_turn_on_ipv4_forwarding_during_deploy() {
  local name="test_should_turn_on_ipv4_forwarding_during_deploy"
  local output status=0 forwarded deployed
  output="$(run_script ENV=dev bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
    return
  fi
  forwarded="$(grep -nF "$FORWARDING_LINE" "$calls" | head -1 | cut -d: -f1)"
  deployed="$(grep -nF "$HOST_DEPLOY_DIR/deploy.sh runner" "$calls" | head -1 | cut -d: -f1)"
  if [ -z "$forwarded" ]; then
    bad "$name" "runner deployment never turned on IPv4 forwarding"
  elif ! grep -qF 'sudo sysctl -q -p' "$calls"; then
    bad "$name" "runner deployment wrote the setting without applying it"
  elif [ -z "$deployed" ] || [ "$forwarded" -ge "$deployed" ]; then
    bad "$name" "IPv4 forwarding is turned on after the runner is deployed, not before"
  else
    ok "$name"
  fi
}

# The host's deploy.sh sources its siblings from its own directory, so one the
# lane does not copy fails every deploy on the host.
test_should_copy_every_file_the_host_deploy_sources() {
  local name="test_should_copy_every_file_the_host_deploy_sources"
  local output status=0 sourced file
  output="$(run_script ENV=dev bash "$DEPLOY")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
    return
  fi
  sourced="$(sed -n 's|^source "$(dirname "${BASH_SOURCE\[0\]}")/\(.*\)"$|\1|p' "$DEPLOY_SCRIPT")"
  if [ -z "$sourced" ]; then
    bad "$name" "no sibling source line read from $DEPLOY_SCRIPT — the scan is broken"
    return
  fi
  for file in $sourced; do
    if ! grep -qF "$HOST_DEPLOY_DIR/$file" "$calls"; then
      bad "$name" "deploy.sh sources $file, which never reached $HOST_DEPLOY_DIR"
      return
    fi
  done
  ok "$name"
}

# The cgroup gate checks exactly what the runner's probe requires, so a host
# the gate passes is one the runner can start on.
test_should_gate_on_every_controller_the_runner_probe_requires() {
  local name="test_should_gate_on_every_controller_the_runner_probe_requires"
  local output status=0 required gated
  required="$(
    sed -n 's/^pub const REQUIRED_CONTROLLERS: \[&str; [0-9]*\] = \[\(.*\)\];$/\1/p' "$PROBE_RS" |
      tr -d '",'
  )"
  output="$(run_script ENV=dev bash "$DEPLOY")" || status=$?
  gated="$(sed -n 's/^ *for controller in \(.*\); do$/\1/p' "$calls" | head -1)"
  if [ -z "$required" ]; then
    bad "$name" "REQUIRED_CONTROLLERS unreadable in $PROBE_RS"
  elif [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$gated" != "$required" ]; then
    bad "$name" "the cgroup gate checks [$gated], the runner's probe requires [$required]"
  else
    ok "$name"
  fi
}

test_should_turn_on_ipv4_forwarding_during_deploy
test_should_copy_every_file_the_host_deploy_sources
test_should_gate_on_every_controller_the_runner_probe_requires
report_results
