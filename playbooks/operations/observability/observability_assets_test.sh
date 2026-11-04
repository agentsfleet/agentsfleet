#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT
cp "$SCRIPT_DIR"/assets/*.json "$work_dir/"
passed=0

check_case() {
  local label="$1" mutation="$2" expected="$3" result=0
  jq "$mutation" "$SCRIPT_DIR/assets/dashboard-chat.json" >"$work_dir/dashboard-chat.json"
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

echo "$passed passed, 0 failed"
