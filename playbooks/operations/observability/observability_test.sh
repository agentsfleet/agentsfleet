#!/usr/bin/env bash

# Provisioning behaviour: the gate, the credentials, and what the apply and
# update paths send to Grafana. Asset CONTENT lives in its sibling,
# observability_assets_test.sh.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROVIDER_DIR="$SCRIPT_DIR/providers/grafana"
GATE="$SCRIPT_DIR/00_gate.sh"
# shellcheck source=observability_test_support.sh
source "$SCRIPT_DIR/observability_test_support.sh"

test_should_validate_assets() {
  local name="test_should_validate_assets"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(run_script bash "$PROVIDER_DIR/assets_check.sh")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  else
    ok "$name"
  fi
}

test_should_verify_prometheus_without_exposing_token() {
  local name="test_should_verify_prometheus_without_exposing_token"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(run_script bash "$GATE" check dev grafana)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif rg --quiet 'grafana-secret' "$calls"; then
    bad "$name" "Grafana token appeared in process arguments"
  else
    ok "$name"
  fi
}

test_should_reject_wrong_datasource_type() {
  local name="test_should_reject_wrong_datasource_type"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(
    run_script MOCK_PROM_TYPE=loki bash "$GATE" check dev grafana
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a non-Prometheus datasource passed"
  else
    ok "$name"
  fi
}

test_should_create_dashboard_and_folder() {
  local name="test_should_create_dashboard_and_folder"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(run_script MOCK_MODE=create bash "$PROVIDER_DIR/resources.sh")" ||
    status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(rg -c -- '--request POST' "$calls")" -ne 2 ]; then
    bad "$name" "expected one folder and one dashboard create"
  elif rg --quiet 'grafana-secret' "$calls"; then
    bad "$name" "Grafana token appeared in process arguments"
  else
    ok "$name"
  fi
}

test_should_create_alerts_with_source_threshold() {
  local name="test_should_create_alerts_with_source_threshold"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(run_script MOCK_MODE=create bash "$PROVIDER_DIR/alerts.sh")" ||
    status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(rg -c -- '--request POST' "$calls")" -ne 6 ]; then
    bad "$name" "expected six alert creates"
  elif ! rg --quiet \
    'min by \(runner_id\) \(time\(\) - agentsfleet_runner_last_seen_seconds\)\) > 90' \
    "$captures"; then
    bad "$name" "runner threshold was not an age derived from source"
  elif rg --quiet 'grafana-secret' "$calls"; then
    bad "$name" "Grafana token appeared in process arguments"
  else
    ok "$name"
  fi
}

test_should_update_existing_resources_with_versions() {
  local name="test_should_update_existing_resources_with_versions"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(run_script MOCK_MODE=update bash "$PROVIDER_DIR/resources.sh")" ||
    status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(rg -c -- '--request PUT' "$calls")" -ne 2 ]; then
    bad "$name" "expected one folder and one dashboard update"
  elif ! rg --quiet '"resourceVersion": "7"' "$captures"; then
    bad "$name" "dashboard update omitted the current resource version"
  else
    ok "$name"
  fi
}

test_should_update_existing_alerts_with_versions() {
  local name="test_should_update_existing_alerts_with_versions"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(run_script MOCK_MODE=update bash "$PROVIDER_DIR/alerts.sh")" ||
    status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(rg -c -- '--request PUT' "$calls")" -ne 6 ]; then
    bad "$name" "expected six alert updates"
  elif [ "$(rg -l 'resourceVersion.*7' \
    "$captures" | wc -l | tr -d ' ')" -ne 6 ]; then
    bad "$name" "an alert update omitted the current resource version"
  else
    ok "$name"
  fi
}

test_should_fail_when_grafana_rejects_a_write() {
  local name="test_should_fail_when_grafana_rejects_a_write"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(
    run_script MOCK_MODE=create MOCK_WRITE_STATUS=500 \
      bash "$PROVIDER_DIR/resources.sh"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a rejected Grafana write passed"
  else
    ok "$name"
  fi
}

test_should_require_write_approval() {
  local name="test_should_require_write_approval"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(
    run_script ALLOW_OBSERVABILITY_WRITES=0 bash "$GATE" apply dev grafana
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "Grafana writes ran without approval"
  else
    ok "$name"
  fi
}

test_should_reject_unknown_provider() {
  local name="test_should_reject_unknown_provider"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(run_script bash "$GATE" check dev elastic)" || status=$?
  if [ "$status" -ne 2 ]; then
    bad "$name" "an unsupported provider did not fail with usage status"
  elif [ -s "$calls" ]; then
    bad "$name" "an unsupported provider reached Grafana"
  else
    ok "$name"
  fi
}

test_should_reject_invalid_gate_inputs() {
  local name="test_should_reject_invalid_gate_inputs"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status arguments
  local cases=(
    ''
    'inspect dev grafana'
    'check staging grafana'
  )

  for arguments in "${cases[@]}"; do
    status=0
    read -r -a argv <<<"$arguments"
    # The empty case is the point of this test, and bash 3.2 treats an empty
    # array expansion as unbound under `set -u` — so the no-argument row has to
    # be spelled the way the audit scripts spell theirs.
    output="$(run_script bash "$GATE" ${argv[@]+"${argv[@]}"})" || status=$?
    if [ "$status" -ne 2 ]; then
      bad "$name" "invalid input '$arguments' did not fail with usage status"
      return
    fi
    if [ -s "$calls" ]; then
      bad "$name" "invalid input '$arguments' reached Grafana"
      return
    fi
  done
  ok "$name"
}

# Each test now runs in its own backgrounded subshell — safe since every
# test above shadows $calls/$captures with a fresh mktemp path, per the note
# by run_script. ok()/bad() still increment $passed/$failed, but those
# increments happen inside the subshell and vanish when it exits; pass/fail
# is instead read back from each test's own captured log (a bad() call always
# prints a line starting "FAIL ", which is the only reliable outcome signal
# a bash function running to completion under `set -uo pipefail` — never an
# explicit `exit`/`return` code — actually provides).

TEST_NAMES=(
  test_should_validate_assets
  test_should_verify_prometheus_without_exposing_token
  test_should_reject_wrong_datasource_type
  test_should_create_dashboard_and_folder
  test_should_create_alerts_with_source_threshold
  test_should_update_existing_resources_with_versions
  test_should_update_existing_alerts_with_versions
  test_should_fail_when_grafana_rejects_a_write
  test_should_require_write_approval
  test_should_reject_unknown_provider
  test_should_reject_invalid_gate_inputs
)

run_suite
