#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

obs_select_environment
obs_open_session

work_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$work_dir"
  obs_close_session
}
trap cleanup EXIT

folder="$(
  obs_get_json \
    "/apis/folder.grafana.app/v1/namespaces/$OBS_NAMESPACE/folders/$OBS_FOLDER_NAME"
)"
if ! jq -e \
  --arg title "agentsfleet — $OBS_ENVIRONMENT" \
  '.spec.title == $title' <<<"$folder" >/dev/null; then
  echo "ERROR: $OBS_ENVIRONMENT Grafana folder drifted" >&2
  exit 1
fi

dashboard="$(
  obs_get_json \
    "/apis/dashboard.grafana.app/v1/namespaces/$OBS_NAMESPACE/dashboards/$OBS_DASHBOARD_NAME"
)"
expected_dashboard="$work_dir/dashboard.json"
jq \
  --arg datasource "$OBS_PROMETHEUS_UID" \
  --arg environment "$OBS_ENVIRONMENT" \
  --arg dashboard "$OBS_DASHBOARD_NAME" \
  'walk(
    if type == "string" then
      gsub("__PROMETHEUS_UID__"; $datasource)
      | gsub("__ENVIRONMENT__"; $environment)
      | gsub("__DASHBOARD_UID__"; $dashboard)
    else . end
  )' "$SCRIPT_DIR/assets/dashboard.json" >"$expected_dashboard"

expected_queries="$work_dir/expected-queries"
actual_queries="$work_dir/actual-queries"
jq -r '[.panels[].targets[].expr] | sort[]' \
  "$expected_dashboard" >"$expected_queries"
jq -r '[.spec.panels[].targets[].expr] | sort[]' \
  <<<"$dashboard" >"$actual_queries"

if ! jq -e \
  --arg uid "$OBS_DASHBOARD_NAME" \
  --arg folder "$OBS_FOLDER_NAME" \
  --arg datasource "$OBS_PROMETHEUS_UID" \
  '
    .metadata.annotations["grafana.app/folder"] == $folder and
    .spec.uid == $uid and
    ([.spec.panels[].datasource.uid] | all(. == $datasource))
  ' <<<"$dashboard" >/dev/null; then
  echo "ERROR: $OBS_ENVIRONMENT Grafana dashboard metadata drifted" >&2
  exit 1
fi
if ! cmp -s "$expected_queries" "$actual_queries"; then
  echo "ERROR: $OBS_ENVIRONMENT Grafana dashboard queries drifted" >&2
  exit 1
fi

offline_seconds="$(obs_runner_offline_seconds)"
alerts="$work_dir/alerts.json"
jq --arg threshold "$offline_seconds" \
  'walk(
    if type == "string" then
      gsub("__RUNNER_OFFLINE_SECONDS__"; $threshold)
    else . end
  )' "$SCRIPT_DIR/assets/alerts.json" >"$alerts"

while IFS= read -r expected; do
  base_name="$(jq -r '.name' <<<"$expected")"
  name="$base_name-$OBS_ENV"
  actual="$(
    obs_get_json \
      "/apis/rules.alerting.grafana.app/v0alpha1/namespaces/$OBS_NAMESPACE/alertrules/$name"
  )"
  if ! jq -e \
    --arg name "$name" \
    --arg folder "$OBS_FOLDER_NAME" \
    --arg datasource "$OBS_PROMETHEUS_UID" \
    --arg expression "$(jq -r '.expr' <<<"$expected")" \
    '
      .metadata.name == $name and
      .metadata.annotations["grafana.app/folder"] == $folder and
      .metadata.labels["grafana.com/group"] == "agentsfleet-runtime" and
      .spec.expressions.A.datasourceUID == $datasource and
      .spec.expressions.A.model.expr == $expression and
      .spec.expressions.A.source == true
    ' <<<"$actual" >/dev/null; then
    echo "ERROR: Grafana alert drifted: $name" >&2
    exit 1
  fi
done < <(jq -c '.[]' "$alerts")

echo "PASS: $OBS_ENVIRONMENT Grafana resources match the repository"
