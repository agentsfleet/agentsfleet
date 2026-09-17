#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../../.." && pwd)"
# The assets under test. Overridable so a negative test can point this checker
# at a deliberately broken copy without editing the real ones; nothing that
# WRITES to Grafana reads this variable, so a wrong value can only make the
# check look at the wrong file, never make an apply push one.
OBS_ASSETS_DIR="${OBS_ASSETS_DIR:-$SCRIPT_DIR/assets}"
DASHBOARD="$OBS_ASSETS_DIR/dashboard.json"
ALERTS="$OBS_ASSETS_DIR/alerts.json"
LEDGER="$REPO_ROOT/rustd/crates/afd_observability/src/metrics/produced.rs"

# The asset carries every panel this dashboard is expected to render. Stated
# rather than implied: a panel silently dropped by a bad edit leaves a valid
# JSON file, and only a count notices.
MINIMUM_PANELS=20

# A target may draw a constant reference line instead of querying — the replay
# floor beside the backlog age is one. It names no metric, so the ownership rule
# above has nothing to check and the shape rule admits it explicitly rather than
# leaving a reader to wonder why the pattern has a hole.

# The alert set is closed. A rule added without moving this number is a rule
# nobody decided to add.
EXPECTED_ALERTS=6

# The family that reports a Unix epoch rather than an age. Every reader of it
# must subtract from evaluation time, or it draws tens of thousands of years.
EPOCH_FAMILY='agentsfleet_runner_last_seen_seconds'

fail() {
  echo "ERROR: $1" >&2
  exit 1
}

check_parses() {
  local asset
  for asset in "$DASHBOARD" "$ALERTS"; do
    jq -e . "$asset" >/dev/null || fail "$asset is not valid JSON"
  done
}

check_dashboard_shape() {
  jq -e \
    --argjson minimum "$MINIMUM_PANELS" \
    '
      (.panels | length) >= $minimum and
      ([.panels[].id] | length == (unique | length)) and
      ([.panels[] | has("targets")] | all) and
      ([.panels[].targets[].expr] | all(contains("agentsfleet_") or contains("gen_ai_") or test("^vector\\([0-9_A-Z]+\\)$"))) and
      ([.panels[].datasource.uid] | all(. == "__PROMETHEUS_UID__"))
    ' "$DASHBOARD" >/dev/null ||
    fail "dashboard panels drifted: count, ids, targets, or datasource"
}

check_alert_shape() {
  jq -e \
    --argjson expected "$EXPECTED_ALERTS" \
    '
      length == $expected and
      ([.[].name] | length == (unique | length)) and
      ([.[].expr] | all(contains("agentsfleet_"))) and
      ([.[] | select(.name == "runner-silent")] | length == 1)
    ' "$ALERTS" >/dev/null ||
    fail "alert rules drifted: count, names, expressions, or runner-silent"
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
      jq -r '.panels[].targets[].expr' "$DASHBOARD"
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

# The wire names the UNPRODUCED ledger excuses.
#
# Resolved rather than pattern-matched: the ledger holds Rust constants
# (`fleet::FLEET_TRIGGERED_TOTAL.wire_name()`), and the string each one stands
# for lives beside its declaration. Reading both is what keeps this check
# honest through a rename that touches only one of them.
unproduced_wire_names() {
  local declared="$REPO_ROOT/rustd/crates/afd_observability/src/metrics/declared"
  local symbol
  grep -oE '[a-z]+::[A-Z_0-9]+\.wire_name\(\)' "$LEDGER" |
    sed -E 's/.*::([A-Z_0-9]+)\..*/\1/' | sort -u |
    while IFS= read -r symbol; do
      grep -A2 -- "pub const $symbol:" "$declared"/*.rs |
        grep -oE '"(agentsfleet|gen_ai)[a-z_.]*"' | head -1 | tr -d '"'
    done
}

# Every family named as a declared gap must be one the ledger actually excuses.
# The check runs in that direction on purpose: when a family gains a producer,
# its ledger row leaves, and this fails until the gap panel leaves too.
check_gap_panel_cites_the_ledger() {
  local content family excused
  content="$(jq -r '.panels[] | select(.type == "text") | .options.content' "$DASHBOARD")"
  [ -n "$content" ] || fail "no declared-gap panel found in the dashboard"
  excused="$(unproduced_wire_names)"
  [ -n "$excused" ] || fail "could not resolve any UNPRODUCED family from $LEDGER"
  while IFS= read -r family; do
    [ -n "$family" ] || continue
    printf '%s\n' "$excused" | grep -Fxq -- "$family" ||
      fail "declared-gap panel names $family, which the UNPRODUCED ledger does not carry"
  done < <(
    # The TABLE ROWS only. A gap is CLAIMED by a row in the table; prose that
    # names a family in passing — "the retired X left the census with it" — is
    # history, and failing it would push the panel into naming things it cannot
    # spell. A bogus table row, the failure this exists to catch, still fails.
    printf '%s\n' "$content" |
      grep -E '^\| `(agentsfleet|gen_ai)[._a-z0-9]+`' |
      grep -oE '`(agentsfleet|gen_ai)[._a-z0-9]+`' |
      tr -d '`' | sort -u
  )
}

# Every family the census declares AND this build produces must appear on the
# dashboard somewhere.
#
# The direction matters. Checking that every panel names a real family catches
# a typo; checking that every produced family reaches a panel catches the thing
# that actually happens — a family ships, nobody adds a panel, and it is
# invisible for a year. The roll-up panels are generated from the census's own
# `category` column precisely so this check stays satisfiable without anyone
# maintaining a list by hand.
check_every_produced_family_is_panelled() {
  local panelled excused family missing=0
  panelled="$(
    jq -r '.panels[] | (.targets // [])[].expr' "$DASHBOARD" |
      grep -oE '(agentsfleet|gen_ai)_[a-z0-9_]+' |
      sed 's/_count$//' | sort -u
  )"
  excused="$(unproduced_wire_names | tr '.' '_')"
  while IFS= read -r family; do
    [ -n "$family" ] || continue
    printf '%s\n' "$excused" | grep -Fxq -- "$family" && continue
    printf '%s\n' "$panelled" | grep -Fxq -- "$family" && continue
    echo "  unpanelled produced family: $family" >&2
    missing=$((missing + 1))
  done < <(
    awk -F'\t' '!/^#/ && NF > 1 && $1 != "name" {print $1}' \
      "$REPO_ROOT/docs/metrics.census.tsv" | tr '.' '_' | sort -u
  )
  [ "$missing" -eq 0 ] ||
    fail "$missing produced census family(ies) reach no panel"
}

# A family in the `errors` roll-up whose label set includes a SUCCESS member
# must exclude it, or the panel reports successes in red.
#
# `agentsfleet_library_read_outcome_total` counts every read by outcome, and
# `ok` is one of them. The roll-up summed the family and put 44 successful
# reads on a panel titled "Every error family". The census says what to read:
# "non-`ok` outcomes per surface".
check_error_rollup_excludes_successes() {
  jq -e '
    [.panels[] | select(.id == 30) | .targets[].expr
     | select(contains("agentsfleet_library_read_outcome_total"))
     | contains("outcome!=\"ok\"")] | all and length > 0
  ' "$DASHBOARD" >/dev/null ||
    fail "the error roll-up counts library reads that succeeded"
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
  local census metric
  census="$(
    awk -F'\t' '!/^#/ && NF > 1 && $1 != "name" {print $1}' \
      "$REPO_ROOT/docs/metrics.census.tsv" | tr '.' '_' | sort -u
  )"
  while IFS= read -r metric; do
    [ -n "$metric" ] || continue
    printf '%s\n' "$census" | grep -Fxq -- "${metric%_count}" ||
      fail "Grafana asset references a metric no census row declares: $metric"
  done < <(
    jq -r '.. | strings' "$DASHBOARD" "$ALERTS" |
      grep -oE '(agentsfleet|gen_ai)_[a-z0-9_]+' | sort -u
  )
}

check_parses
check_dashboard_shape
check_alert_shape
check_epoch_readers_subtract
check_thresholds_are_derived
check_gap_panel_cites_the_ledger
check_every_produced_family_is_panelled
check_error_rollup_excludes_successes
check_metrics_are_source_owned

echo "PASS: Grafana assets are valid and reference source-owned metrics"
