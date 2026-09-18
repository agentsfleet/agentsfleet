#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

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
obs_render_dashboard "$dashboard_spec" "$SCRIPT_DIR"
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

# ---------------------------------------------------------------------------
# Alert rules. Same session, same environment, same approval — a dashboard
# applied without its rules is half a deploy, and the two were never
# separately runnable in practice.
# ---------------------------------------------------------------------------

alerts_path="/apis/rules.alerting.grafana.app/v0alpha1/namespaces/$OBS_NAMESPACE/alertrules"

# NO GROUP LABELS. The API refuses them on both paths — "cannot set group when
# creating a new rule" on POST, "cannot set group when updating un-grouped rule"
# on PUT — and returns HTTP 403 for what is a validation rule, not a permission.
# Grouping in this API version is its own resource, `rulesequences`, which the
# same endpoint lists. Six rules that each carry `trigger.interval` evaluate the
# same grouped or not, so nothing is added here to buy an ordering nobody reads.
alerts="$work_dir/alerts.json"
obs_render_alerts "$alerts" "$SCRIPT_DIR"

while IFS= read -r alert; do
  base_name="$(jq -r '.name' <<<"$alert")"
  name="$base_name-$OBS_ENV"
  response="$work_dir/$name-response.json"
  payload="$work_dir/$name.json"
  status="$(obs_get_status "$alerts_path/$name" "$response")"

  jq -n \
    --argjson alert "$alert" \
    --arg name "$name" \
    --arg folder "$OBS_FOLDER_NAME" \
    --arg environment "$OBS_ENVIRONMENT" \
    --arg datasource "$OBS_PROMETHEUS_UID" \
    --arg dashboard "$OBS_DASHBOARD_NAME" \
    '{
      kind:"AlertRule",
      apiVersion:"rules.alerting.grafana.app/v0alpha1",
      metadata:{
        name:$name,
        annotations:{
          "grafana.app/folder":$folder,
          "grafana.com/provenance":"api"
        },
      },
      spec:{
        title:($alert.title + " — " + $environment),
        trigger:{interval:"1m"},
        labels:{
          service:"agentsfleetd",
          environment:$environment,
          severity:$alert.severity
        },
        annotations:{summary:$alert.summary},
        for:$alert.for,
        noDataState:$alert.noDataState,
        execErrState:$alert.execErrState,
        panelRef:{
          dashboardUID:$dashboard,
          panelID:$alert.panelId
        },
        expressions:{
          A:{
            queryType:"",
            relativeTimeRange:{from:"10m",to:"0s"},
            datasourceUID:$datasource,
            model:{
              editorMode:"code",
              expr:$alert.expr,
              instant:true,
              intervalMs:60000,
              legendFormat:"__auto",
              maxDataPoints:43200,
              range:false,
              refId:"A"
            },
            source:true
          }
        }
      }
    }' >"$payload"

  case "$status" in
    200)
      resource_version="$(jq -r '.metadata.resourceVersion // empty' \
        "$response")"
      if [ -z "$resource_version" ]; then
        echo "ERROR: $name has no Grafana resource version" >&2
        exit 1
      fi
      jq --arg version "$resource_version" \
        '.metadata.resourceVersion = $version' \
        "$payload" >"$work_dir/$name-versioned.json"
      mv "$work_dir/$name-versioned.json" "$payload"
      status="$(
        obs_write_json PUT "$alerts_path/$name" "$payload" "$response"
      )"
      [ "$status" = "200" ] || {
        echo "ERROR: alert update returned HTTP $status for $name" >&2
        exit 1
      }
      ;;
    404)
      status="$(obs_write_json POST "$alerts_path" "$payload" "$response")"
      case "$status" in
        200 | 201) ;;
        *)
          echo "ERROR: alert create returned HTTP $status for $name" >&2
          exit 1
          ;;
      esac
      ;;
    *)
      echo "ERROR: alert lookup returned HTTP $status for $name" >&2
      exit 1
      ;;
  esac

  echo "OK: $name"
done < <(jq -c '.[]' "$alerts")

echo "PASS: $OBS_ENVIRONMENT Grafana alerts are current"
