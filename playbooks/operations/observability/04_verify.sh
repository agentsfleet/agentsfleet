#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

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

for dashboard_asset in "$SCRIPT_DIR"/assets/dashboard*.json; do
expected_dashboard="$work_dir/dashboard.json"
obs_render_dashboard "$expected_dashboard" "$SCRIPT_DIR" "$dashboard_asset"
dashboard_name="$(jq -r .uid "$expected_dashboard")"
dashboard="$(
  obs_get_json \
    "/apis/dashboard.grafana.app/v1/namespaces/$OBS_NAMESPACE/dashboards/$dashboard_name"
)"

expected_queries="$work_dir/expected-queries"
actual_queries="$work_dir/actual-queries"
# `targets // []` on BOTH sides: a panel that queries nothing — the declared-gap
# text panel — carries an empty array in the asset, and Grafana drops the field
# entirely on the way back. Iterating the absent field is what turned a
# faithfully applied dashboard into "queries drifted".
jq -S '[.panels[] | {id,title,description,type,gridPos,unit:.fieldConfig.defaults.unit,noValue:(.fieldConfig.defaults.noValue // null),displayName:(.fieldConfig.defaults.displayName // null),datasource, targets:[(.targets // [])[] | {refId,expr,query,queryType,tableType,datasource,limit,spss,legendFormat:(.legendFormat // ""),instant:(.instant // false),range:(.range // false),format:(.format // "time_series")}], transformations:(.transformations // [])}] | sort_by(.id)' \
  "$expected_dashboard" >"$expected_queries"
jq -S '[.spec.panels[] | {id,title,description,type,gridPos,unit:.fieldConfig.defaults.unit,noValue:(.fieldConfig.defaults.noValue // null),displayName:(.fieldConfig.defaults.displayName // null),datasource, targets:[(.targets // [])[] | {refId,expr,query,queryType,tableType,datasource,limit,spss,legendFormat:(.legendFormat // ""),instant:(.instant // false),range:(.range // false),format:(.format // "time_series")}], transformations:(.transformations // [])}] | sort_by(.id)' \
  <<<"$dashboard" >"$actual_queries"

if ! jq -e \
  --arg uid "$dashboard_name" \
  --arg folder "$OBS_FOLDER_NAME" \
  --arg datasource "$OBS_PROMETHEUS_UID" \
  '
    .metadata.annotations["grafana.app/folder"] == $folder and
    .metadata.name == $uid and
    ([.spec.panels[].datasource.uid] | all(. == $datasource or . == "${tempo}"))
  ' <<<"$dashboard" >/dev/null; then
  echo "ERROR: $OBS_ENVIRONMENT Grafana dashboard metadata drifted" >&2
  exit 1
fi
if ! cmp -s "$expected_queries" "$actual_queries"; then
  echo "ERROR: $OBS_ENVIRONMENT Grafana dashboard content drifted" >&2
  exit 1
fi

# A textbox query is its saved value; datasource queries define the allowed type.
if ! jq -e --slurpfile expected "$expected_dashboard" \
  'def definitions: .list | map(
     {name,type,regex:(.regex // ""),multi:(.multi // false),includeAll:(.includeAll // false)} +
     (if .type == "textbox" then {} else {query:(.query // "")} end)
   ) | sort_by(.name);
   (.spec.templating | definitions) == ($expected[0].templating | definitions)' <<<"$dashboard" >/dev/null; then
  echo "ERROR: Grafana dashboard variables drifted" >&2
  exit 1
fi

done

alerts="$work_dir/alerts.json"
obs_render_alerts "$alerts" "$SCRIPT_DIR"

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
    --arg dashboard "$OBS_DASHBOARD_NAME$(jq -r '.dashboardSuffix // ""' <<<"$expected")" \
    --argjson panel "$(jq -r '.panelId' <<<"$expected")" \
    '
      .metadata.name == $name and
      .metadata.annotations["grafana.app/folder"] == $folder and
      .spec.expressions.A.datasourceUID == $datasource and
      .spec.expressions.A.model.expr == $expression and
      .spec.expressions.A.source == true and
      .spec.panelRef.dashboardUID == $dashboard and .spec.panelRef.panelID == $panel
    ' <<<"$actual" >/dev/null; then
    echo "ERROR: Grafana alert drifted: $name" >&2
    exit 1
  fi
done < <(jq -c '.[]' "$alerts")

echo "PASS: $OBS_ENVIRONMENT Grafana resources match the repository"
