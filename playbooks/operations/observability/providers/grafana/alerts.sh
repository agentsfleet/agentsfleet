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

alerts_path="/apis/rules.alerting.grafana.app/v0alpha1/namespaces/$OBS_NAMESPACE/alertrules"
offline_seconds="$(obs_runner_offline_seconds)"
alerts="$work_dir/alerts.json"
jq --arg threshold "$offline_seconds" \
  'walk(
    if type == "string" then
      gsub("__RUNNER_OFFLINE_SECONDS__"; $threshold)
    else . end
  )' "$SCRIPT_DIR/assets/alerts.json" >"$alerts"

index=0
while IFS= read -r alert; do
  base_name="$(jq -r '.name' <<<"$alert")"
  name="$base_name-$OBS_ENV"
  response="$work_dir/$name-response.json"
  payload="$work_dir/$name.json"
  status="$(obs_get_status "$alerts_path/$name" "$response")"

  jq -n \
    --argjson alert "$alert" \
    --arg name "$name" \
    --arg group_index "$index" \
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
        labels:{
          "grafana.com/group":"agentsfleet-runtime",
          "grafana.com/group-index":$group_index
        }
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
  index=$((index + 1))
done < <(jq -c '.[]' "$alerts")

echo "PASS: $OBS_ENVIRONMENT Grafana alerts are current"
