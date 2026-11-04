#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROVIDER_DIR="$SCRIPT_DIR"
VERIFY="$PROVIDER_DIR/04_verify.sh"
passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
mkdir -p "$stub_dir"
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
      */loki-datasource-uid) printf 'loki-main\n' ;;
      *) exit 1 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB

cat >"$stub_dir/curl" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
url="${*: -1}"

case "$url" in
  */folders/agentsfleet-dev)
    title='agentsfleet — development'
    [ "${MOCK_DRIFT:-}" != folder ] || title=wrong
    jq -n --arg title "$title" '{spec:{title:$title}}'
    ;;
  */dashboards/agentsfleet-runtime-dev*)
    dashboard_name="${url##*/}"
    suffix="${dashboard_name#agentsfleet-runtime-dev}"
    dashboard_asset="$PROVIDER_DIR/assets/dashboard$suffix.json"
    # The stub renders through the SAME function the apply and the drift check
    # use. A fourth hand-rolled copy of the substitution list is how a
    # placeholder added to the asset made this stub serve a dashboard that
    # matched nothing, and the suite reported drift that did not exist.
    rendered="$(mktemp)"
    (
      # shellcheck source=lib.sh
      source "$PROVIDER_DIR/lib.sh"
      OBS_PROMETHEUS_UID=prometheus-main
      OBS_ENVIRONMENT=development
      OBS_DASHBOARD_NAME=agentsfleet-runtime-dev
      obs_render_dashboard "$rendered" "$PROVIDER_DIR" "$dashboard_asset"
    )
    spec="$(cat "$rendered")"
    rm -f "$rendered"
    if [ "${MOCK_DRIFT:-}" = dashboard_query ]; then
      spec="$(
        jq '(first(.panels[] | select((.targets // []) | length > 0)) |
             .targets[0].expr) = "drift"' <<<"$spec"
      )"
    fi
    if [ "${MOCK_DRIFT:-}" = null_to_zero ]; then
      spec="$(jq '.panels[0].fieldConfig.defaults.noValue = "0"' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = panel_unit ]; then
      spec="$(jq '.panels[0].fieldConfig.defaults.unit = "bytes"' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = panel_title ]; then
      spec="$(jq '.panels[0].title = "drift"' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = panel_description ]; then
      spec="$(jq '.panels[0].description = "drift"' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = panel_override ]; then
      spec="$(jq '.panels[0].fieldConfig.overrides = [{matcher:{id:"byName",options:"value"},properties:[{id:"displayName",value:"drift"}]}]' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = panel_legend ]; then
      spec="$(jq '.panels[0].targets[0].legendFormat = "drift"' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = table_mode ]; then
      spec="$(jq '(.panels[] | select(.type == "table") | .targets[0].instant) = false' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = tempo_query ]; then
      spec="$(jq '(.panels[] | select(.datasource.type == "tempo") | .targets[0].query) = "drift"' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = tempo_reducer ]; then
      spec="$(jq '(.panels[] | select(.datasource.type == "tempo") | .transformations[1].options.reducers) = ["mean"]' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = tempo_variable ]; then
      spec="$(jq '(.templating.list[] | select(.name == "tempo") | .query) = "prometheus"' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = reordered_defaults ]; then
      spec="$(jq '(.panels[].targets[] | .editorMode) = "code" | walk(if type == "object" then to_entries | reverse | from_entries else . end)' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = selected_variables ]; then
      spec="$(jq '(.templating.list[] | select(.name == "tempo")) |= (.current={text:"Dev traces",value:"tempo-selected"} | .options=[{text:"Dev traces",value:"tempo-selected",selected:true}]) | (.templating.list[] | select(.name == "fleet_id")) |= (.query="fleet-selected" | .current={text:"fleet-selected",value:"fleet-selected"}) | .templating.list[] |= (.regex="" | .multi=(.multi // false) | .includeAll=(.includeAll // false))' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = tempo_variable_type ]; then
      spec="$(jq '(.templating.list[] | select(.name == "tempo") | .type) = "textbox"' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = tempo_variable_multi ]; then
      spec="$(jq '(.templating.list[] | select(.name == "tempo") | .multi) = true' <<<"$spec")"
    fi
    if [ "${MOCK_DRIFT:-}" = tempo_variable_all ]; then
      spec="$(jq '(.templating.list[] | select(.name == "tempo") | .includeAll) = true' <<<"$spec")"
    fi
    folder=agentsfleet-dev
    [ "${MOCK_DRIFT:-}" != dashboard_metadata ] || folder=wrong
    jq -n \
      --arg folder "$folder" \
      --arg name "$dashboard_name" \
      --argjson spec "$spec" \
      '{
        metadata:{name:$name,annotations:{"grafana.app/folder":$folder}},
        spec:$spec
      }'
    ;;
  */alertrules/*-dev)
    name="${url##*/}"
    base_name="${name%-dev}"
    # Rendered through the same function the apply and the drift check use,
    # for the reason the dashboard branch above gives.
    rendered="$(mktemp)"
    (
      # shellcheck source=lib.sh
      source "$PROVIDER_DIR/lib.sh"
      obs_render_alerts "$rendered" "$PROVIDER_DIR"
    )
    alert="$(jq -c --arg name "$base_name" '.[] | select(.name == $name)' "$rendered")"
    link="$(jq -r '.dashboardSuffix // ""' <<<"$alert")"
    panel="$(jq -r .panelId <<<"$alert")"
    [ "${MOCK_DRIFT:-}" != alert_link ] || panel=999
    expression="$(
      jq -r --arg name "$base_name" '.[] | select(.name == $name) | .expr' "$rendered"
    )"
    rm -f "$rendered"
    [ "${MOCK_DRIFT:-}" != alert ] || expression=drift
    jq -n \
      --arg name "$name" \
      --arg expression "$expression" \
      --arg dashboard "agentsfleet-runtime-dev$link" \
      --argjson panel "$panel" \
      '{
        metadata:{
          name:$name,
          annotations:{"grafana.app/folder":"agentsfleet-dev"},
        },
        spec:{panelRef:{dashboardUID:$dashboard,panelID:$panel},expressions:{A:{
          datasourceUID:"prometheus-main",
          model:{expr:$expression},
          source:true
        }}}
      }'
    ;;
  *)
    printf '{}\n'
    ;;
esac
STUB

chmod +x "$stub_dir/op" "$stub_dir/curl"

run_verify() {
  : >"$calls"
  env \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    PROVIDER_DIR="$PROVIDER_DIR" \
    OBS_ENV=dev \
    ALLOW_VAULT_READS=1 \
    "$@" \
    bash "$VERIFY" 2>&1
}

test_should_accept_matching_resources() {
  local name="test_should_accept_matching_resources"
  local output status=0
  output="$(run_verify)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [[ "$output" != *"resources match the repository"* ]]; then
    bad "$name" "verification omitted its success verdict"
  elif grep -Eq grafana-secret "$calls"; then
    bad "$name" "Grafana token appeared in process arguments"
  else
    ok "$name"
  fi
}

test_should_reject_each_resource_drift() {
  local name="test_should_reject_each_resource_drift"
  local drift drift_expected expected output status
  local -a cases=(
    'folder|folder drifted'
    'dashboard_metadata|dashboard metadata drifted'
    'dashboard_query|dashboard content drifted'
    'tempo_query|dashboard content drifted'
    'table_mode|dashboard content drifted'
    'panel_unit|dashboard content drifted'
    'null_to_zero|dashboard content drifted'
    'panel_title|dashboard content drifted'
    'panel_description|dashboard content drifted'
    'panel_override|dashboard content drifted'
    'panel_legend|dashboard content drifted'
    'tempo_reducer|dashboard content drifted'
    'tempo_variable|dashboard variables drifted'
    'tempo_variable_type|dashboard variables drifted'
    'tempo_variable_multi|dashboard variables drifted'
    'tempo_variable_all|dashboard variables drifted'
    'alert|Grafana alert drifted'
    'alert_link|Grafana alert drifted'
  )

  for drift_expected in "${cases[@]}"; do
    drift="${drift_expected%%|*}"
    expected="${drift_expected#*|}"
    status=0
    output="$(run_verify MOCK_DRIFT="$drift")" || status=$?
    if [ "$status" -eq 0 ] || [[ "$output" != *"$expected"* ]]; then
      bad "$name" "$drift did not fail closed: $output"
      return
    fi
  done
  ok "$name"
}

test_should_accept_reordered_objects_and_editor_defaults() {
  local output mode status
  for mode in reordered_defaults selected_variables; do
    status=0
    output="$(run_verify MOCK_DRIFT="$mode")" || status=$?
    if [ "$status" -ne 0 ] || [[ "$output" != *"resources match the repository"* ]]; then
      bad test_should_accept_reordered_objects_and_editor_defaults "$mode: $output"
      return
    fi
  done
  ok test_should_accept_reordered_objects_and_editor_defaults
}

test_should_accept_matching_resources
test_should_accept_reordered_objects_and_editor_defaults
test_should_reject_each_resource_drift

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
