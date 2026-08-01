#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROVIDER_DIR="$SCRIPT_DIR/providers/grafana"
VERIFY="$PROVIDER_DIR/resource_verify.sh"
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
  */dashboards/agentsfleet-runtime-dev)
    spec="$(
      jq \
        --arg datasource prometheus-main \
        --arg environment development \
        --arg dashboard agentsfleet-runtime-dev \
        'walk(
          if type == "string" then
            gsub("__PROMETHEUS_UID__"; $datasource)
            | gsub("__ENVIRONMENT__"; $environment)
            | gsub("__DASHBOARD_UID__"; $dashboard)
          else . end
        )' "$PROVIDER_DIR/assets/dashboard.json"
    )"
    if [ "${MOCK_DRIFT:-}" = dashboard_query ]; then
      spec="$(jq '.panels[0].targets[0].expr = "drift"' <<<"$spec")"
    fi
    folder=agentsfleet-dev
    [ "${MOCK_DRIFT:-}" != dashboard_metadata ] || folder=wrong
    jq -n \
      --arg folder "$folder" \
      --argjson spec "$spec" \
      '{metadata:{annotations:{"grafana.app/folder":$folder}},spec:$spec}'
    ;;
  */alertrules/*-dev)
    name="${url##*/}"
    base_name="${name%-dev}"
    expression="$(
      jq -r \
        --arg name "$base_name" \
        '.[] | select(.name == $name) | .expr' \
        "$PROVIDER_DIR/assets/alerts.json" |
        sed 's/__RUNNER_OFFLINE_SECONDS__/90/g'
    )"
    [ "${MOCK_DRIFT:-}" != alert ] || expression=drift
    jq -n \
      --arg name "$name" \
      --arg expression "$expression" \
      '{
        metadata:{
          name:$name,
          annotations:{"grafana.app/folder":"agentsfleet-dev"},
          labels:{"grafana.com/group":"agentsfleet-runtime"}
        },
        spec:{expressions:{A:{
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
  elif rg --quiet grafana-secret "$calls"; then
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
    'dashboard_query|dashboard queries drifted'
    'alert|Grafana alert drifted'
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

test_should_accept_matching_resources
test_should_reject_each_resource_drift

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
