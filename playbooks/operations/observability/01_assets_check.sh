#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
# The assets under test. Overridable so a negative test can point this checker
# at a deliberately broken copy without editing the real ones; nothing that
# WRITES to Grafana reads this variable, so a wrong value can only make the
# check look at the wrong file, never make an apply push one.
OBS_ASSETS_DIR="${OBS_ASSETS_DIR:-$SCRIPT_DIR/assets}"
DASHBOARDS=("$OBS_ASSETS_DIR"/dashboard*.json)
ALERTS="$OBS_ASSETS_DIR/alerts.json"

# The six focused dashboards have an exact, reviewed panel inventory.
# A target may draw a constant reference line instead of querying — the replay
# floor beside the backlog age is one. It names no metric, so the ownership rule
# above has nothing to check and the shape rule admits it explicitly rather than
# leaving a reader to wonder why the pattern has a hole.

# The alert set is closed. A rule added without moving this number is a rule
# nobody decided to add.
#
# Four rules are declared in alerts.json. Indy approved correcting this
# stale count on Nov 04, 2026; the alert set itself is unchanged.
EXPECTED_ALERTS=4

# The family that reports a Unix epoch rather than an age. Every reader of it
# must subtract from evaluation time, or it draws tens of thousands of years.
EPOCH_FAMILY='agentsfleet_runner_last_seen_seconds'


fail() {
  echo "ERROR: $1" >&2
  exit 1
}

check_parses() {
  local asset
  for asset in "${DASHBOARDS[@]}" "$ALERTS"; do
    jq -e . "$asset" >/dev/null || fail "$asset is not valid JSON"
  done
}

check_dashboard_shape() {
  local dashboard expected expected_uid
  [ "${#DASHBOARDS[@]}" -eq 6 ] || fail "expected six dashboard definitions"
  [ "$(jq -r .uid "${DASHBOARDS[@]}" | sort -u | wc -l)" -eq 6 ] || fail "dashboard UIDs must be unique"
  for dashboard in "${DASHBOARDS[@]}"; do
    case "${dashboard##*/}" in
      dashboard.json) expected='[3,6,7,8,20,28,35,37,101,106]' ;;
      dashboard-chat.json) expected='[38,40,41,42,43,44,45,46,47,103,104,105]' ;;
      dashboard-api-db.json) expected='[1,2,110,111,112,113,114,115]' ;;
      dashboard-scheduler.json) expected='[120,121,122,123,124,126]' ;;
      dashboard-models.json) expected='[130,131,132,133,134,135,136,137,138,139,140,141]' ;;
      dashboard-sandbox.json) expected='[150,151,152,153,154,155,156,157]' ;;
      *) fail "unknown dashboard definition: $dashboard" ;;
    esac
    expected_uid="__DASHBOARD_UID__${dashboard##*/dashboard}"
    expected_uid="${expected_uid%.json}"
    jq -e --arg uid "$expected_uid" --argjson expected "$expected" '
      .uid == $uid and
      ([.panels[].id] | sort) == ($expected | sort) and
      ([.panels[].id] | length == (unique | length)) and
      (.links | length == 6) and
      ([.templating.list[] | select(.name == "tempo" and .type == "datasource" and .query == "tempo" and .multi == false and .includeAll == false)] | length == 1) and
      ([.templating.list[] | select(.name == "fleet_id" and .type == "textbox")] | length == 1) and
      ([.templating.list[].name] | length == (unique | length)) and
      all(.panels[];
        has("targets") and
        (if .datasource.uid == "__PROMETHEUS_UID__" then
          all(.targets[]; (.expr | contains("agentsfleet_") or contains("gen_ai_") or test("^vector\\([0-9_A-Z]+\\)$")))
        else
          .datasource == {type:"tempo",uid:"${tempo}"} and
          (.targets | length == 1) and
          all(.targets[];
            .datasource == {type:"tempo",uid:"${tempo}"} and
            .queryType == "traceql" and .tableType == "spans" and .limit == 1000 and .spss == 1 and
            (.query | startswith("{ name = \"fleet.delivery\" && span.\"gen_ai.agent.id\" = \"$fleet_id\" }"))) and
          (.transformations | length == 2) and
          .transformations[0].id == "filterFieldsByName" and
          .transformations[1] == {id:"reduce",options:{reducers:["p95","p99"],mode:"reduceFields"}} and
          .fieldConfig.defaults.displayName == "${__field.labels.reducer}" and
          (if .id == 40 then .fieldConfig.defaults.unit == "ns" else .fieldConfig.defaults.unit == "ms" end)
        end)) and
      (if (.uid | endswith("-chat")) then
        ([.panels[] | select(.datasource.uid == "${tempo}") | .transformations[0].options.include.names[0]] | sort) ==
        (["duration", "agentsfleet.delivery.queue_wait_ms", "agentsfleet.delivery.memory_ms", "agentsfleet.delivery.model_ms", "agentsfleet.delivery.tools_ms", "agentsfleet.delivery.sandbox_wait_ms", "agentsfleet.delivery.settle_ms", "agentsfleet.delivery.complete_ms"] | sort)
       else all(.panels[]; .datasource.uid == "__PROMETHEUS_UID__") end)
    ' "$dashboard" >/dev/null || fail "dashboard panels drifted: $dashboard"
  done
}

check_alert_shape() {
  jq -e \
    --argjson expected "$EXPECTED_ALERTS" \
    '
      length == $expected and
      all(.[]; (.dashboardSuffix | type) == "string" and (.panelId | type) == "number") and
      ([.[].name] | length == (unique | length)) and
      ([.[].expr] | all(contains("agentsfleet_"))) and
      ([.[] | select(.name == "runner-silent")] | length == 1)
    ' "$ALERTS" >/dev/null ||
    fail "alert rules drifted: count, names, expressions, or runner-silent"
}

check_alert_links() {
  local name panel asset
  while IFS=$'\t' read -r name panel; do
    case "$name" in
      dashboard.json | dashboard-chat.json | dashboard-api-db.json | dashboard-scheduler.json | dashboard-models.json | dashboard-sandbox.json) ;;
      *) fail "alert links an unknown dashboard" ;;
    esac
    asset="$OBS_ASSETS_DIR/$name"
    jq -e --argjson panel "$panel" 'any(.panels[]; .id == $panel)' "$asset" >/dev/null ||
      fail "alert links a missing panel: $asset / $panel"
  done < <(jq -r '.[] | [("dashboard" + (.dashboardSuffix // "") + ".json"), .panelId] | @tsv' "$ALERTS")
}

# An epoch compared against a threshold is true forever; an epoch plotted as a
# duration renders as geological time. Both shipped once. The subtraction is
# what makes the reading an age, so it is required rather than reviewed.
check_epoch_readers_subtract() {
  local expr
  while IFS= read -r expr; do
    [ -n "$expr" ] || continue
    case "$expr" in
      *"time() - $EPOCH_FAMILY"*) ;;
      *) fail "expression reads $EPOCH_FAMILY without subtracting it from time(): $expr" ;;
    esac
  done < <(
    {
      jq -r '.panels[].targets[].expr' "${DASHBOARDS[@]}"
      jq -r '.[].expr' "$ALERTS"
    } | grep -F "$EPOCH_FAMILY" || true
  )
}

# A threshold belongs to source, not to this file. `> 0` stays legal because it
# asks whether anything happened at all; any larger bound is a number somebody
# typed, and it drifts the moment the constant behind it moves.
check_thresholds_are_derived() {
  local expr
  while IFS= read -r expr; do
    [ -n "$expr" ] || continue
    fail "alert expression compares against a literal threshold; substitute it instead: $expr"
  done < <(
    jq -r '.[].expr' "$ALERTS" |
      grep -E '[<>]=?[[:space:]]*[0-9]{2,}' || true
  )
}





# Every metric an asset names must be one this repository owns.
#
# Checked against the census rather than by grepping the crates, because an
# OTLP family is spelled with dots at the source and with underscores on the
# wire: `agentsfleet.billing.credit.consumed` is declared that way in Rust and
# arrives in the store as `agentsfleet_billing_credit_consumed`. Grepping for
# the wire spelling could never find the declaration. The census is the right
# authority anyway — a Rust registry test grades it against the registry in
# BOTH directions, so a name in the census is a name the daemon produces or
# the build does not compile.
check_metrics_are_source_owned() {
  local census metric base
  census="$(
    awk -F'\t' '!/^#/ && NF > 1 && $1 != "name" {print $1}' \
      "$REPO_ROOT/docs/metrics.census.tsv" "$REPO_ROOT/docs/metrics.runner.census.tsv" | tr '.' '_' | sort -u
  )"
  # Matched in-shell, not through `printf | grep -Fxq`. Under `set -o pipefail`
  # that pipeline reports the WRITER's status too, and `grep -q` exits the
  # moment it matches — so a metric near the top of the sorted census leaves
  # `printf` writing into a closed pipe, and the SIGPIPE (141) becomes the
  # pipeline's status even though the lookup SUCCEEDED. The `||` then fires and
  # a declared metric is reported as undeclared. It is a race on whether the
  # writer drains first, so it passes locally and fails on a loaded runner:
  # `agentsfleet_admission_replays_total` sorts 3rd of 62 and lost it in CI.
  # A `case` glob over the newline-delimited list asks the same question with
  # no second process to outlive.
  while IFS= read -r metric; do
    [ -n "$metric" ] || continue
    base="${metric%_count}"
    base="${base%_bucket}"
    base="${base%_sum}"
    case $'\n'"$census"$'\n' in
      *$'\n'"$base"$'\n'*) ;;
      *) fail "Grafana asset references a metric no census row declares: $metric" ;;
    esac
  done < <(
    jq -r '.. | strings' "${DASHBOARDS[@]}" "$ALERTS" |
      grep -oE '(agentsfleet|gen_ai)_[a-z0-9_]+' |
      grep -vE '^gen_ai_(request_model|provider_name|token_type)$' | sort -u
  )
}


check_parses
check_dashboard_shape
check_alert_shape
check_alert_links
check_epoch_readers_subtract
check_thresholds_are_derived
check_metrics_are_source_owned

echo "PASS: Grafana assets are valid and reference source-owned metrics"
