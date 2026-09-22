#!/usr/bin/env bash

# Provisioning behaviour: the gate, the credentials, and what the apply and
# update paths send to Grafana. Asset CONTENT lives in its sibling,
# observability_assets_test.sh.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROVIDER_DIR="$SCRIPT_DIR"
GATE="$SCRIPT_DIR/00_gate.sh"
# shellcheck source=observability_test_support.sh
source "$SCRIPT_DIR/observability_test_support.sh"

test_should_validate_assets() {
  local name="test_should_validate_assets"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(run_script bash "$PROVIDER_DIR/01_assets_check.sh")" || status=$?
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
  output="$(run_script ACTION=check ENV=dev bash "$GATE")" || status=$?
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
    run_script MOCK_PROM_TYPE=loki ACTION=check ENV=dev bash "$GATE"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a non-Prometheus datasource passed"
  else
    ok "$name"
  fi
}

test_should_reject_wrong_loki_datasource_type() {
  local name="test_should_reject_wrong_loki_datasource_type"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(
    run_script MOCK_LOKI_TYPE=prometheus ACTION=check ENV=dev bash "$GATE"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a non-Loki datasource passed"
  else
    ok "$name"
  fi
}

test_should_create_every_resource_in_one_apply() {
  local name="test_should_create_every_resource_in_one_apply"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  # Seven: the folder, the dashboard, and five alert rules. Counted as one
  # number because they are one apply — a dashboard written without its rules
  # is half a deploy, and the old split let each half pass while the other
  # never ran.
  output="$(run_script MOCK_MODE=create bash "$PROVIDER_DIR/03_apply.sh")" ||
    status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(rg -c -- '--request POST' "$calls")" -ne 7 ]; then
    bad "$name" "expected one folder, one dashboard and five alert creates"
  # Three facts in one pattern: the reading is an AGE (time() minus the epoch
  # family), it is grouped per runner so the page names who died, and 90 is the
  # number `afd_core::timing` derives rather than one somebody typed. The clamp
  # keeps a replica whose clock ran ahead from reporting a negative age.
  elif ! rg --quiet \
    'min by \(runner_id\) \(clamp_min\(time\(\) - agentsfleet_runner_last_seen_seconds, 0\)\) > 90' \
    "$captures"; then
    bad "$name" "runner threshold was not a per-runner age derived from source"
  elif rg --quiet 'grafana-secret' "$calls"; then
    bad "$name" "Grafana token appeared in process arguments"
  else
    ok "$name"
  fi
}

test_should_update_every_resource_with_its_version() {
  local name="test_should_update_every_resource_with_its_version"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status=0
  output="$(run_script MOCK_MODE=update bash "$PROVIDER_DIR/03_apply.sh")" ||
    status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(rg -c -- '--request PUT' "$calls")" -ne 7 ]; then
    bad "$name" "expected one folder, one dashboard and five alert updates"
  elif ! rg --quiet '"resourceVersion": "7"' "$captures"; then
    bad "$name" "dashboard update omitted the current resource version"
  # Every alert carries its own version too: an update that drops one is a
  # lost-update race the API cannot refuse.
  elif [ "$(rg -l 'resourceVersion.*7' "$captures" | wc -l | tr -d ' ')" -lt 6 ]; then
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
      bash "$PROVIDER_DIR/03_apply.sh"
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
    run_script ALLOW_OBSERVABILITY_WRITES=0 ACTION=apply ENV=dev bash "$GATE"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "Grafana writes ran without approval"
  else
    ok "$name"
  fi
}


test_should_reject_invalid_gate_inputs() {
  local name="test_should_reject_invalid_gate_inputs"
  local calls="$(mktemp -p "$work_dir")"
  local captures="$(mktemp -d -p "$work_dir")"
  local output status entry action env_value
  # ENV unset is the row that matters most: it is the one an operator hits by
  # reflex, and "all" is the value they reach for next.
  local cases=(
    'check|'
    'check|all'
    'check|staging'
    'inspect|dev'
  )

  for entry in "${cases[@]}"; do
    action="${entry%%|*}"
    env_value="${entry##*|}"
    status=0
    output="$(run_script ACTION="$action" ENV="$env_value" bash "$GATE")" || status=$?
    if [ "$status" -ne 2 ]; then
      bad "$name" "invalid input ACTION=$action ENV=$env_value did not fail with usage status"
      return
    fi
    if [ -s "$calls" ]; then
      bad "$name" "invalid input ACTION=$action ENV=$env_value reached Grafana"
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


# ========================================================================
# Asset CONTENT: what the assets must say.
# ========================================================================

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

test_should_refuse_to_invent_health_from_absent_data() {
  local name="test_should_refuse_to_invent_health_from_absent_data"
  # Inverted deliberately. This once required clamp_min(denominator, 1) and a
  # zero fallback, which is what made a fleet with NO traffic and a fleet with
  # NO TELEMETRY both report 100% availability. An undefined ratio renders N/A,
  # and N/A is the honest answer to a question nothing was asked.
  if ! jq -e '[.panels[] | select(.id == 20 or .id == 21 or .id == 22 or .id == 34) |
      .targets[].expr |
      (contains("clamp_min") or contains("or vector(1)") or contains("or vector(0)")) | not] |
      all and length >= 4' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "an indicator still invents a passing value when its denominator is absent"
  elif ! jq -e '[.panels[] | (.targets // [])[].expr |
      contains("or vector(0)") | not] | all' \
    "$PROVIDER_DIR/assets/dashboard.json" >/dev/null; then
    bad "$name" "a panel still reads an absent series as zero"
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
      source '$PROVIDER_DIR/lib.sh'
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
    sed -n 's/^EXPECTED_ALERTS=\([0-9]*\)$/\1/p' "$PROVIDER_DIR/01_assets_check.sh"
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
  output="$(OBS_ASSETS_DIR="$dir" bash "$PROVIDER_DIR/01_assets_check.sh" 2>&1)" || status=$?
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
  output="$(run_script bash "$PROVIDER_DIR/01_assets_check.sh")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  else
    ok "$name"
  fi
}

test_pickup_ratio_is_honest_about_no_admissions() {
  local name="test_pickup_ratio_is_honest_about_no_admissions"
  local expr
  expr="$(
    jq -r '.panels[] | select(.id == 34) | .targets[0].expr' \
      "$PROVIDER_DIR/assets/dashboard.json"
  )"
  case "$expr" in
    *'or vector(1)'* | *'clamp_min'*)
      bad "$name" "the ratio still invents a pass when nothing was admitted: $expr"
      return
      ;;
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
  # Every panel this asset carried before the census work merged, minus the two
  # roll-ups folded into the tables deliberately. 30 and 31 are now the failures
  # and activity tables; 32 and 33 were four-wall tile grids whose families moved
  # into those two, so the coverage check still sees every one of them. Removing
  # an id from this list is a decision to state, never a way to make it pass.
  for id in 1 2 3 4 5 6 7 8 10 20 21 22 23 24 25 26 27 28; do
    jq -e --argjson id "$id" '[.panels[] | select(.id == $id)] | length == 1' \
      "$PROVIDER_DIR/assets/dashboard.json" >/dev/null || missing="$missing $id"
  done
  if [ -n "$missing" ]; then
    bad "$name" "panel(s) lost to the merge:$missing"
  else
    ok "$name"
  fi
}

# ========================================================================
# Grader REFUSALS: break a copy one way, require rejection by name.
# A guard nobody has watched bite is a guard nobody knows works.
# ========================================================================



test_should_reject_an_epoch_read_without_subtraction() {
  local name="test_should_reject_an_epoch_read_without_subtraction"
  local dir output status=0
  dir="$(broken_assets)"
  jq '(.panels[] | select(.id == 23) | .targets[0].expr) =
      "max(agentsfleet_runner_last_seen_seconds)"' \
    "$dir/dashboard.json" >"$dir/patched.json"
  mv "$dir/patched.json" "$dir/dashboard.json"
  output="$(
    OBS_ASSETS_DIR="$dir" bash "$PROVIDER_DIR/01_assets_check.sh" 2>&1
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
    OBS_ASSETS_DIR="$dir" bash "$PROVIDER_DIR/01_assets_check.sh" 2>&1
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "the checker accepted a threshold nobody derived"
  elif ! printf '%s' "$output" | grep -q 'literal threshold'; then
    bad "$name" "wrong rejection: $output"
  else
    ok "$name"
  fi
}

test_should_fail_a_declared_name_with_no_function() {
  local name="test_should_fail_a_declared_name_with_no_function"
  local output
  # A fresh bash, because `run_suite` is already running in this one and is not
  # re-entrant. The phantom name is the whole fixture: before the guard, a name
  # in TEST_NAMES with no function behind it logged "command not found", which
  # carries no FAIL line, so the tally counted it GREEN. Two refusal guards
  # reported success that way for an edit that had deleted their bodies.
  output="$(
    bash -c '
      source "$1/observability_test_support.sh"
      TEST_NAMES=(a_name_nothing_defines)
      passed=0
      failed=0
      run_suite
    ' _ "$SCRIPT_DIR" 2>&1
  )" || true
  if ! printf '%s' "$output" | grep -q '^FAIL a_name_nothing_defines'; then
    bad "$name" "a declared name with no function did not fail: $output"
  elif ! printf '%s' "$output" | grep -q 'no such function'; then
    bad "$name" "the failure did not say why: $output"
  elif ! printf '%s' "$output" | grep -q '0 passed, 1 failed'; then
    bad "$name" "the tally still counted the phantom as passing: $output"
  else
    ok "$name"
  fi
}


TEST_NAMES=(
  test_should_validate_assets
  test_should_verify_prometheus_without_exposing_token
  test_should_reject_wrong_datasource_type
  test_should_reject_wrong_loki_datasource_type
  test_should_fail_when_grafana_rejects_a_write
  test_should_create_every_resource_in_one_apply
  test_should_update_every_resource_with_its_version
  test_should_require_write_approval
  test_should_reject_invalid_gate_inputs
  test_should_fail_a_declared_name_with_no_function
  test_should_repair_the_heartbeat_readings
  test_should_refuse_to_invent_health_from_absent_data
  test_should_derive_the_replay_floor_from_source
  test_should_mark_burn_rate_panels_unproven
  test_should_keep_the_shipped_panels
  test_should_read_telemetry_loss
  test_should_leave_the_bench_baselines_untouched
  test_should_cover_slo_in_the_playbook
  test_should_match_the_alert_count_constant
  test_fleet_row_reads_both_families
  test_pickup_ratio_is_honest_about_no_admissions
  test_prior_panels_survive
  test_should_reject_an_epoch_read_without_subtraction
  test_should_reject_a_literal_alert_threshold
)

run_suite
