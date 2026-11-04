#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT
cp "$SCRIPT_DIR"/assets/*.json "$work_dir/"
passed=0

check_case() {
  local label="$1" mutation="$2" expected="$3" result=0
  local asset="${4:-dashboard-chat.json}"
  cp "$SCRIPT_DIR"/assets/*.json "$work_dir/"
  jq "$mutation" "$SCRIPT_DIR/assets/$asset" >"$work_dir/$asset"
  OBS_ASSETS_DIR="$work_dir" bash "$SCRIPT_DIR/01_assets_check.sh" >"$work_dir/output" 2>&1 || result=$?
  if { [ "$expected" = pass ] && [ "$result" -ne 0 ]; } ||
    { [ "$expected" = fail ] && [ "$result" -eq 0 ]; }; then
    echo "FAIL: $label"
    cat "$work_dir/output"
    exit 1
  fi
  passed=$((passed + 1))
  echo "PASS: $label"
}

check_case 'source assets with the Tempo variable' '.' pass
check_case 'foreign panel datasource refused' \
  '(.panels[] | select(.datasource.type == "tempo") | .datasource.uid) = "foreign"' fail
check_case 'foreign query datasource refused' \
  '(.panels[] | select(.datasource.type == "tempo") | .targets[0].datasource.uid) = "foreign"' fail
check_case 'a non-Tempo variable refused' \
  '(.templating.list[] | select(.name == "tempo") | .query) = "prometheus"' fail
check_case 'an unscoped query refused' \
  '(.panels[] | select(.datasource.type == "tempo") | .targets[0].query) = "{ name = \"fleet.delivery\" }"' fail
check_case 'missing segment panel refused' \
  '.panels |= map(select(.id != 42))' fail
check_case 'wrong percentile reducers refused' \
  '(.panels[] | select(.datasource.type == "tempo") | .transformations[1].options.reducers) = ["mean"]' fail
check_case 'wrong duration unit refused' \
  '(.panels[] | select(.id == 40) | .fieldConfig.defaults.unit) = "ms"' fail
check_case 'an unknown metric refused' \
  '.panels[0].targets[0].expr = "agentsfleet_unowned_probe"' fail

check_case 'an incorrect dashboard UID refused' '.uid = "__DASHBOARD_UID__"' fail

check_case 'an alert pointing at a missing panel refused' '.[0].panelId = 999' fail alerts.json
check_case 'an alert pointing at an unknown dashboard refused' '.[0].dashboardSuffix = "-foreign"' fail alerts.json

check_case 'an overview suffix alias refused' '.[0].dashboardSuffix = "overview" | .[0].panelId = 6' fail alerts.json

check_case 'a non-string alert dashboard suffix refused' '.[0].dashboardSuffix = 1' fail alerts.json

echo "$passed passed, 0 failed"
