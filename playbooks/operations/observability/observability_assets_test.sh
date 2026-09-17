#!/usr/bin/env bash

# Asset CONTENT: what the assets must say and what the grader must refuse.
# Provisioning behaviour is in observability_test.sh.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROVIDER_DIR="$SCRIPT_DIR/providers/grafana"
GATE="$SCRIPT_DIR/00_gate.sh"
# shellcheck source=observability_test_support.sh
source "$SCRIPT_DIR/observability_test_support.sh"

# A mutated copy of the assets, so a negative test proves the checker rejects a
# defect without the risk of leaving the real asset broken on a failed run.
broken_assets() {
  local dir
  dir="$(mktemp -d -p "$work_dir")"
  cp "$PROVIDER_DIR/assets/dashboard.json" "$PROVIDER_DIR/assets/alerts.json" "$dir/"
  printf '%s' "$dir"
}

test_should_repair_the_heartbeat_readings() {
  local name="test_should_repair_the_heartbeat_readings"
  local raw
  raw="$(
    {
      jq -r '.panels[].targets[].expr' "$PROVIDER_DIR/assets/dashboard.json"
      jq -r '.[].expr' "$PROVIDER_DIR/assets/alerts.json"
    } | grep -F 'agentsfleet_runner_last_seen_seconds' |
      grep -vF 'time() - agentsfleet_runner_last_seen_seconds' || true
  )"
  if [ -n "$raw" ]; then
    bad "$name" "an expression still reads the epoch raw: $raw"
  elif ! jq -e '[.panels[] | select(.id == 6 or .id == 23) |
      .targets[].expr | contains("min by (runner_id)")] | all and length == 2' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "a heartbeat panel does not take the freshest reading per runner"
  else
    ok "$name"
  fi
}

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

test_should_guard_slo_ratios_against_zero() {
  local name="test_should_guard_slo_ratios_against_zero"
  if ! jq -e '[.panels[] | select(.id == 20 or .id == 21) | .targets[].expr |
      contains("clamp_min") and contains("or vector(0)")] | all and length == 2' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "an availability ratio can divide by an absent denominator"
  elif ! jq -e '[.panels[] | select(.id == 22) | .targets[].expr |
      contains("or vector(1)")] | all' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "the pickup-latency indicator reads no-data rather than healthy"
  else
    ok "$name"
  fi
}

test_should_derive_the_replay_floor_from_source() {
  local name="test_should_derive_the_replay_floor_from_source"
  local replay min_age interval expected actual
  replay="$SCRIPT_DIR/../../../rustd/crates/afd_runner/src/sweep/replay.rs"
  min_age="$(sed -n 's/^const MIN_AGE: Duration = Duration::from_secs(\([0-9]*\));/\1/p' "$replay")"
  interval="$(sed -n 's/^const INTERVAL: Duration = Duration::from_secs(\([0-9]*\));/\1/p' "$replay")"
  expected=$((min_age + interval))
  actual="$(
    OBS_ENV=dev bash -c "
      source '$PROVIDER_DIR/common.sh'
      OBS_REPO_ROOT='$SCRIPT_DIR/../../..'
      obs_admission_replay_floor_seconds
    "
  )"
  if [ "$actual" != "$expected" ]; then
    bad "$name" "derived floor $actual, expected $expected from replay.rs"
  elif ! jq -e '[.panels[] | select(.id == 22 or .id == 28) | .targets[].expr |
      contains("__ADMISSION_REPLAY_FLOOR_SECONDS__")] | any' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "the replay floor is not a substituted placeholder in the asset"
  else
    ok "$name"
  fi
}

test_should_mark_burn_rate_panels_unproven() {
  local name="test_should_mark_burn_rate_panels_unproven"
  if ! jq -e '[.panels[] | select(.id == 24 or .id == 25) |
      (.targets | length) >= 4] | all and length == 2' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "a burn-rate panel is not multiwindow"
  elif ! jq -e '[.panels[] | select(.id == 24 or .id == 25) |
      .description | contains("UNPROVEN")] | all and length == 2' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "a burn-rate panel does not say its target is unproven"
  else
    ok "$name"
  fi
}

test_should_keep_the_shipped_panels() {
  local name="test_should_keep_the_shipped_panels"
  local missing=""
  local id
  for id in 1 2 3 4 5 6 7 8 10; do
    jq -e --argjson id "$id" '[.panels[] | select(.id == $id)] | length == 1' \
      "$PROVIDER_DIR/assets/dashboard.json" >/dev/null || missing="$missing $id"
  done
  if [ -n "$missing" ]; then
    bad "$name" "shipped panel(s) lost:$missing"
  else
    ok "$name"
  fi
}

test_should_read_telemetry_loss() {
  local name="test_should_read_telemetry_loss"
  if ! jq -e '[.panels[].targets[].expr |
      contains("agentsfleet_otlp_entries_discarded_total")] | any' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "no panel reports telemetry discarded at the source"
  elif jq -e '[.panels[].targets[].expr |
      contains("agentsfleet_otlp_queue_depth")] | any' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "a panel still queries a family the UNPRODUCED ledger excuses"
  else
    ok "$name"
  fi
}

test_should_warn_about_the_shared_tenant() {
  local name="test_should_warn_about_the_shared_tenant"
  local content
  content="$(
    jq -r '.panels[] | select(.type == "text") | .options.content' \
      "$PROVIDER_DIR/assets/dashboard.json"
  )"
  if ! printf '%s' "$content" | grep -q 'deployment.environment'; then
    bad "$name" "the gap panel does not name the missing environment attribute"
  elif ! printf '%s' "$content" | grep -q 'same Grafana stack'; then
    bad "$name" "the gap panel does not warn that the two environments share a tenant"
  else
    ok "$name"
  fi
}

# The census USED to be guarded here too. It moved when the fleet-census work
# merged in — two produced families added, a never-incremented one retired —
# so the claim became false and the assertion had to go rather than be worked
# around.
# bench/baselines is a different thing: those are bench-lane measurements read
# by `make bench-compare`, never by Grafana, and no dashboard work has a reason
# to touch them.
test_should_leave_the_bench_baselines_untouched() {
  local name="test_should_leave_the_bench_baselines_untouched"
  local changed
  changed="$(
    git -C "$SCRIPT_DIR/../../.." diff --name-only origin/main...HEAD \
      -- bench/baselines 2>/dev/null
  )"
  if [ -n "$changed" ]; then
    bad "$name" "this workstream changed bench baselines it must not: $changed"
  else
    ok "$name"
  fi
}

test_should_cover_slo_in_the_playbook() {
  local name="test_should_cover_slo_in_the_playbook"
  local playbook="$SCRIPT_DIR/001_playbook.md"
  if ! grep -q 'Service Level Indicator panels render' "$playbook"; then
    bad "$name" "Acceptance does not name the Service Level Indicator panels"
  elif ! grep -q 'unproven marker' "$playbook"; then
    bad "$name" "Acceptance does not require the burn panels to stay marked"
  elif ! grep -q 'Read before applying to production' "$playbook"; then
    bad "$name" "the playbook does not warn about the shared tenant"
  else
    ok "$name"
  fi
}

test_should_match_the_alert_count_constant() {
  local name="test_should_match_the_alert_count_constant"
  local declared actual output status=0
  declared="$(
    sed -n 's/^EXPECTED_ALERTS=\([0-9]*\)$/\1/p' "$PROVIDER_DIR/assets_check.sh"
  )"
  actual="$(jq 'length' "$PROVIDER_DIR/assets/alerts.json")"
  if [ -z "$declared" ]; then
    bad "$name" "the grader carries no named alert-count constant"
    return
  fi
  if [ "$declared" != "$actual" ]; then
    bad "$name" "constant says $declared, alerts.json holds $actual"
    return
  fi
  # And the constant must be load-bearing: a rule added without moving it fails.
  local dir
  dir="$(broken_assets)"
  jq '. + [(.[0] | .name = "probe-extra-rule")]' "$dir/alerts.json" >"$dir/patched.json"
  mv "$dir/patched.json" "$dir/alerts.json"
  output="$(OBS_ASSETS_DIR="$dir" bash "$PROVIDER_DIR/assets_check.sh" 2>&1)" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a seventh rule passed a grader that declares $declared"
  else
    ok "$name"
  fi
}

# Named by the fleet-census workstream's Section 5, which specified dashboard
# work against this asset and left it here rather than duplicating the asset.
test_fleet_row_reads_both_families() {
  local name="test_fleet_row_reads_both_families"
  local output status=0
  if ! jq -e '[.panels[] | select(.id == 35) | .targets[].expr |
      contains("agentsfleet_fleets{status=")] | all and length == 5' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "no panel reads agentsfleet_fleets by its five closed statuses"
    return
  fi
  if ! jq -e '[.panels[] | select(.id == 36) | .targets[].expr |
      contains("sum by (kind) (rate(agentsfleet_fleet_runs_started_total")] | all' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "no panel reads agentsfleet_fleet_runs_started_total by kind"
    return
  fi
  output="$(run_script bash "$PROVIDER_DIR/assets_check.sh")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  else
    ok "$name"
  fi
}

test_pickup_ratio_guards_empty() {
  local name="test_pickup_ratio_guards_empty"
  local expr
  expr="$(
    jq -r '.panels[] | select(.id == 34) | .targets[0].expr' \
      "$PROVIDER_DIR/assets/dashboard.json"
  )"
  case "$expr" in
    *'or vector(1)'*) ;;
    *) bad "$name" "the pickup ratio has no empty-series fallback: $expr"; return ;;
  esac
  case "$expr" in
    *'agentsfleet_fleet_runs_started_total{kind="fresh"}'*) ;;
    *) bad "$name" "the numerator is not fresh-only; a reclaim would inflate it"; return ;;
  esac
  case "$expr" in
    *'kind="reclaimed"'*)
      bad "$name" "a reclaim counts toward the numerator and can push the ratio above 1"
      return
      ;;
  esac
  ok "$name"
}

test_prior_panels_survive() {
  local name="test_prior_panels_survive"
  local missing="" id
  # Every panel this asset carried before the census work merged.
  for id in 1 2 3 4 5 6 7 8 10 20 21 22 23 24 25 26 27 28 29 30 31 32 33; do
    jq -e --argjson id "$id" '[.panels[] | select(.id == $id)] | length == 1' \
      "$PROVIDER_DIR/assets/dashboard.json" >/dev/null || missing="$missing $id"
  done
  if [ -n "$missing" ]; then
    bad "$name" "panel(s) lost to the merge:$missing"
  else
    ok "$name"
  fi
}

TEST_NAMES=(
  test_should_repair_the_heartbeat_readings
  test_should_reject_an_epoch_read_without_subtraction
  test_should_reject_a_literal_alert_threshold
  test_should_reject_a_gap_panel_for_a_produced_family
  test_should_guard_slo_ratios_against_zero
  test_should_derive_the_replay_floor_from_source
  test_should_mark_burn_rate_panels_unproven
  test_should_keep_the_shipped_panels
  test_should_read_telemetry_loss
  test_should_warn_about_the_shared_tenant
  test_should_leave_the_bench_baselines_untouched
  test_should_cover_slo_in_the_playbook
  test_should_match_the_alert_count_constant
  test_fleet_row_reads_both_families
  test_pickup_ratio_guards_empty
  test_prior_panels_survive
)

run_suite
