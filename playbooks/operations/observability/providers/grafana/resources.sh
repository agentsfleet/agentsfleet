#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

obs_select_environment
obs_require_write_approval
obs_open_session

work_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$work_dir"
  obs_close_session
}
trap cleanup EXIT

require_status() {
  local resource="$1"
  local actual="$2"
  shift 2
  local expected
  for expected in "$@"; do
    [ "$actual" = "$expected" ] && return 0
  done
  echo "ERROR: $resource returned HTTP $actual" >&2
  exit 1
}

folder_path="/apis/folder.grafana.app/v1/namespaces/$OBS_NAMESPACE/folders"
folder_payload="$work_dir/folder.json"
folder_response="$work_dir/folder-response.json"
jq -n \
  --arg name "$OBS_FOLDER_NAME" \
  --arg title "agentsfleet — $OBS_ENVIRONMENT" \
  '{metadata:{name:$name},spec:{title:$title}}' >"$folder_payload"

folder_status="$(
  obs_get_status "$folder_path/$OBS_FOLDER_NAME" "$folder_response"
)"
case "$folder_status" in
  200)
    if ! jq -e \
      --arg title "agentsfleet — $OBS_ENVIRONMENT" \
      '.spec.title == $title' "$folder_response" >/dev/null; then
      folder_status="$(
        obs_write_json PUT "$folder_path/$OBS_FOLDER_NAME" \
          "$folder_payload" "$folder_response"
      )"
      require_status "Grafana folder update" "$folder_status" 200
    fi
    ;;
  404)
    folder_status="$(
      obs_write_json POST "$folder_path" "$folder_payload" "$folder_response"
    )"
    require_status "Grafana folder create" "$folder_status" 200 201
    ;;
  *)
    require_status "Grafana folder lookup" "$folder_status" 200 404
    ;;
esac

dashboard_spec="$work_dir/dashboard-spec.json"
dashboard_payload="$work_dir/dashboard.json"
dashboard_response="$work_dir/dashboard-response.json"
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
  )' "$SCRIPT_DIR/assets/dashboard.json" >"$dashboard_spec"
jq -n \
  --arg name "$OBS_DASHBOARD_NAME" \
  --arg folder "$OBS_FOLDER_NAME" \
  --slurpfile spec "$dashboard_spec" \
  '{
    kind:"Dashboard",
    apiVersion:"dashboard.grafana.app/v1",
    metadata:{name:$name,annotations:{"grafana.app/folder":$folder}},
    spec:$spec[0]
  }' >"$dashboard_payload"

dashboard_path="/apis/dashboard.grafana.app/v1/namespaces/$OBS_NAMESPACE/dashboards"
dashboard_status="$(
  obs_get_status "$dashboard_path/$OBS_DASHBOARD_NAME" "$dashboard_response"
)"
case "$dashboard_status" in
  200)
    resource_version="$(jq -r '.metadata.resourceVersion // empty' \
      "$dashboard_response")"
    if [ -n "$resource_version" ]; then
      jq --arg version "$resource_version" \
        '.metadata.resourceVersion = $version' \
        "$dashboard_payload" >"$work_dir/dashboard-versioned.json"
      mv "$work_dir/dashboard-versioned.json" "$dashboard_payload"
    fi
    dashboard_status="$(
      obs_write_json PUT "$dashboard_path/$OBS_DASHBOARD_NAME" \
        "$dashboard_payload" "$dashboard_response"
    )"
    require_status "Grafana dashboard update" "$dashboard_status" 200
    ;;
  404)
    dashboard_status="$(
      obs_write_json POST "$dashboard_path" \
        "$dashboard_payload" "$dashboard_response"
    )"
    require_status "Grafana dashboard create" "$dashboard_status" 200 201
    ;;
  *)
    require_status "Grafana dashboard lookup" "$dashboard_status" 200 404
    ;;
esac

echo "PASS: $OBS_ENVIRONMENT Grafana folder and dashboard are current"
