#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/test_search.sh
source "$SCRIPT_DIR/../../lib/test_search.sh"
GATE="$SCRIPT_DIR/00_gate.sh"
PROVIDER_DIR="$SCRIPT_DIR/providers/grafana"
passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
captures="$work_dir/captures"
mkdir -p "$stub_dir" "$captures"
trap 'rm -rf "$work_dir"' EXIT

ok() {
  printf 'ok   %s\n' "$1"
  passed=$((passed + 1))
}

bad() {
  printf 'FAIL %s\n       %s\n' "$1" "$2" >&2
  failed=$((failed + 1))
}

cat >"$stub_dir/op" <<'STUB'
#!/usr/bin/env bash
case "${1:-}" in
  whoami) printf 'stub-user\n' ;;
  read)
    case "${2:-}" in
      */grafana-url) printf 'https://grafana.test\n' ;;
      */grafana-sa-token) printf 'grafana-secret\n' ;;
      */grafana-namespace) printf 'default\n' ;;
      */prometheus-datasource-uid) printf 'prometheus-main\n' ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB

cat >"$stub_dir/curl" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
method=GET
output_file=""
input_file=""
write_status=0
previous=""
for argument in "$@"; do
  case "$previous" in
    --request) method="$argument" ;;
    --output) output_file="$argument" ;;
    --data-binary) input_file="${argument#@}" ;;
  esac
  [ "$argument" = "--write-out" ] && write_status=1
  previous="$argument"
done
url="${*: -1}"
status=200
body='{}'

case "$url" in
  */api/datasources/uid/prometheus-main)
    body="{\"uid\":\"prometheus-main\",\"type\":\"${MOCK_PROM_TYPE:-prometheus}\"}"
    ;;
  */api/datasources/proxy/uid/prometheus-main/api/v1/query)
    body='{"status":"success","data":{"result":[{"value":[1,"0"]}]}}'
    ;;
  */folders/agentsfleet-dev | */dashboards/agentsfleet-runtime-dev | */alertrules/*-dev)
    if [ "${MOCK_MODE:-create}" = "create" ]; then
      status=404
    else
      body='{"metadata":{"resourceVersion":"7"},"spec":{}}'
    fi
    ;;
  */folders | */dashboards | */alertrules)
    status=201
    body='{"metadata":{"resourceVersion":"1"}}'
    ;;
esac

if [ "$method" != "GET" ] && [ -n "${MOCK_WRITE_STATUS:-}" ]; then
  status="$MOCK_WRITE_STATUS"
fi

if [ "$method" != "GET" ] && [ -n "$input_file" ]; then
  capture="$CAPTURES/$(basename "$input_file").$method.json"
  cp "$input_file" "$capture"
fi
if [ -n "$output_file" ]; then
  printf '%s\n' "$body" >"$output_file"
else
  printf '%s\n' "$body"
fi
if [ "$write_status" -eq 1 ]; then
  printf '%s' "$status"
fi
STUB

cat >"$stub_dir/rg" <<'STUB'
#!/usr/bin/env bash
echo "ERROR: production playbooks must not require rg" >&2
exit 127
STUB

chmod +x "$stub_dir/op" "$stub_dir/curl" "$stub_dir/rg"

run_script() {
  : >"$calls"
  rm -f "$captures"/*
  env \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    CAPTURES="$captures" \
    OBS_ENV=dev \
    ALLOW_VAULT_READS=1 \
    ALLOW_OBSERVABILITY_WRITES=1 \
    "$@" 2>&1
}

# Every test below runs in its own backgrounded subshell (see the runner at
# the bottom of this file), and run_script's `: >"$calls"` / `rm -f
# "$captures"/*` operate on paths, not shell state — two tests sharing the
# file-scope $calls/$captures would genuinely race: one test's assertion
# reading a curl-argument log truncated mid-read by another test's run_script
# call. Each test declares its OWN local calls/captures below. bash resolves
# a free variable inside a called function by walking UP the call stack, and
# a command-substitution subshell inherits that whole call stack at fork time
# — so run_script, invoked from inside a test function that just did `local
# calls=...`, sees THAT test's path, never the file-scope default declared at
# the top of this file. The file-scope $calls/$captures become dead once
# every test shadows them; kept only so run_script has something to name if a
# future caller forgets to.
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

test_should_leave_the_census_untouched() {
  local name="test_should_leave_the_census_untouched"
  local changed
  changed="$(
    git -C "$SCRIPT_DIR/../../.." diff --name-only origin/main...HEAD \
      -- docs/metrics.census.tsv bench/baselines 2>/dev/null
  )"
  if [ -n "$changed" ]; then
    bad "$name" "this workstream changed files it must not: $changed"
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
  test_should_leave_the_census_untouched
  test_should_cover_slo_in_the_playbook
)

result_dir="$(mktemp -d)"
pids=()
for name in "${TEST_NAMES[@]}"; do
  ( "$name" ) >"$result_dir/$name.log" 2>&1 &
  pids+=("$!")
done
for pid in "${pids[@]}"; do wait "$pid"; done

for name in "${TEST_NAMES[@]}"; do
  cat "$result_dir/$name.log"
  if grep -q '^FAIL ' "$result_dir/$name.log"; then
    failed=$((failed + 1))
  else
    passed=$((passed + 1))
  fi
done
rm -rf -- "$result_dir"

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
