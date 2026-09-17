#!/usr/bin/env bash

# Grader REFUSALS: each test breaks a copy of the assets one way and requires
# assets_check.sh to reject it by name. A guard nobody has watched bite is a
# guard nobody knows works.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROVIDER_DIR="$SCRIPT_DIR/providers/grafana"
# shellcheck source=observability_test_support.sh
source "$SCRIPT_DIR/observability_test_support.sh"

test_should_reject_an_epoch_read_without_subtraction() {
  local name="test_should_reject_an_epoch_read_without_subtraction"
  local dir output status=0
  dir="$(broken_assets)"
  jq '(.panels[] | select(.id == 23) | .targets[0].expr) =
      "max(agentsfleet_runner_last_seen_seconds)"' \
    "$dir/dashboard.json" >"$dir/patched.json"
  mv "$dir/patched.json" "$dir/dashboard.json"
  output="$(
    OBS_ASSETS_DIR="$dir" bash "$PROVIDER_DIR/assets_check.sh" 2>&1
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "the checker accepted an epoch compared as an age"
  elif ! printf '%s' "$output" | grep -q 'without subtracting it from time()'; then
    bad "$name" "wrong rejection: $output"
  else
    ok "$name"
  fi
}
test_should_reject_a_literal_alert_threshold() {
  local name="test_should_reject_a_literal_alert_threshold"
  local dir output status=0
  dir="$(broken_assets)"
  jq '(.[] | select(.name == "runner-silent") | .expr) =
      "max(min by (runner_id) (time() - agentsfleet_runner_last_seen_seconds)) > 90"' \
    "$dir/alerts.json" >"$dir/patched.json"
  mv "$dir/patched.json" "$dir/alerts.json"
  output="$(
    OBS_ASSETS_DIR="$dir" bash "$PROVIDER_DIR/assets_check.sh" 2>&1
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "the checker accepted a threshold nobody derived"
  elif ! printf '%s' "$output" | grep -q 'literal threshold'; then
    bad "$name" "wrong rejection: $output"
  else
    ok "$name"
  fi
}
test_should_reject_a_gap_panel_for_a_produced_family() {
  local name="test_should_reject_a_gap_panel_for_a_produced_family"
  local dir output status=0
  dir="$(broken_assets)"
  jq '(.panels[] | select(.type == "text") | .options.content) +=
      "\n| `agentsfleet_lease_polls_total` | invented reason |\n"' \
    "$dir/dashboard.json" >"$dir/patched.json"
  mv "$dir/patched.json" "$dir/dashboard.json"
  output="$(
    OBS_ASSETS_DIR="$dir" bash "$PROVIDER_DIR/assets_check.sh" 2>&1
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "the checker accepted a gap claim for a produced family"
  elif ! printf '%s' "$output" | grep -q 'UNPRODUCED ledger does not carry'; then
    bad "$name" "wrong rejection: $output"
  else
    ok "$name"
  fi
}
test_should_reject_an_error_panel_counting_successes() {
  local name="test_should_reject_an_error_panel_counting_successes"
  local dir output status=0
  dir="$(broken_assets)"
  jq '(.panels[] | select(.id == 30) | .targets[] |
       select(.expr | contains("library_read_outcome")) | .expr) =
      "sum(increase(agentsfleet_library_read_outcome_total[$__range])) or vector(0)"' \
    "$dir/dashboard.json" >"$dir/patched.json"
  mv "$dir/patched.json" "$dir/dashboard.json"
  output="$(OBS_ASSETS_DIR="$dir" bash "$PROVIDER_DIR/assets_check.sh" 2>&1)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "the checker accepted successful reads on an error panel"
  elif ! printf '%s' "$output" | grep -q 'library reads that succeeded'; then
    bad "$name" "wrong rejection: $output"
  else
    ok "$name"
  fi
}

TEST_NAMES=(
  test_should_reject_an_epoch_read_without_subtraction
  test_should_reject_a_literal_alert_threshold
  test_should_reject_a_gap_panel_for_a_produced_family
  test_should_reject_an_error_panel_counting_successes
)

run_suite
