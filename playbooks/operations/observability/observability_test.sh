#!/usr/bin/env bash

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/test_search.sh
source "$SCRIPT_DIR/../../lib/test_search.sh"
GATE="$SCRIPT_DIR/00_gate.sh"
PROVIDER_DIR="$SCRIPT_DIR/providers/grafana"
passed=0
failed=0
work_dir="$(mktemp -d)"
stub_dir="$work_dir/bin"
calls="$work_dir/calls"
captures="$work_dir/captures"
mkdir -p "$stub_dir" "$captures"
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
method=GET
output_file=""
input_file=""
write_status=0
previous=""
for argument in "$@"; do
  case "$previous" in
    --request) method="$argument" ;;
    --output) output_file="$argument" ;;
    --data-binary) input_file="${argument#@}" ;;
  esac
  [ "$argument" = "--write-out" ] && write_status=1
  previous="$argument"
done
url="${*: -1}"
status=200
body='{}'

case "$url" in
  */api/datasources/uid/prometheus-main)
    body="{\"uid\":\"prometheus-main\",\"type\":\"${MOCK_PROM_TYPE:-prometheus}\"}"
    ;;
  */api/datasources/proxy/uid/prometheus-main/api/v1/query)
    body='{"status":"success","data":{"result":[{"value":[1,"0"]}]}}'
    ;;
  */folders/agentsfleet-dev | */dashboards/agentsfleet-runtime-dev | */alertrules/*-dev)
    if [ "${MOCK_MODE:-create}" = "create" ]; then
      status=404
    else
      body='{"metadata":{"resourceVersion":"7"},"spec":{}}'
    fi
    ;;
  */folders | */dashboards | */alertrules)
    status=201
    body='{"metadata":{"resourceVersion":"1"}}'
    ;;
esac

if [ "$method" != "GET" ] && [ -n "${MOCK_WRITE_STATUS:-}" ]; then
  status="$MOCK_WRITE_STATUS"
fi

if [ "$method" != "GET" ] && [ -n "$input_file" ]; then
  capture="$CAPTURES/$(basename "$input_file").$method.json"
  cp "$input_file" "$capture"
fi
if [ -n "$output_file" ]; then
  printf '%s\n' "$body" >"$output_file"
else
  printf '%s\n' "$body"
fi
if [ "$write_status" -eq 1 ]; then
  printf '%s' "$status"
fi
STUB

cat >"$stub_dir/rg" <<'STUB'
#!/usr/bin/env bash
echo "ERROR: production playbooks must not require rg" >&2
exit 127
STUB

chmod +x "$stub_dir/op" "$stub_dir/curl" "$stub_dir/rg"

run_script() {
  : >"$calls"
  rm -f "$captures"/*
  env \
    PATH="$stub_dir:$PATH" \
    CALLS="$calls" \
    CAPTURES="$captures" \
    OBS_ENV=dev \
    ALLOW_VAULT_READS=1 \
    ALLOW_OBSERVABILITY_WRITES=1 \
    "$@" 2>&1
}

test_should_validate_assets() {
  local name="test_should_validate_assets"
  local output status=0
  output="$(run_script bash "$PROVIDER_DIR/assets_check.sh")" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  else
    ok "$name"
  fi
}

test_should_verify_prometheus_without_exposing_token() {
  local name="test_should_verify_prometheus_without_exposing_token"
  local output status=0
  output="$(run_script bash "$GATE" check dev grafana)" || status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif rg --quiet 'grafana-secret' "$calls"; then
    bad "$name" "Grafana token appeared in process arguments"
  else
    ok "$name"
  fi
}

test_should_reject_wrong_datasource_type() {
  local name="test_should_reject_wrong_datasource_type"
  local output status=0
  output="$(
    run_script MOCK_PROM_TYPE=loki bash "$GATE" check dev grafana
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a non-Prometheus datasource passed"
  else
    ok "$name"
  fi
}

test_should_create_dashboard_and_folder() {
  local name="test_should_create_dashboard_and_folder"
  local output status=0
  output="$(run_script MOCK_MODE=create bash "$PROVIDER_DIR/resources.sh")" ||
    status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(rg -c -- '--request POST' "$calls")" -ne 2 ]; then
    bad "$name" "expected one folder and one dashboard create"
  elif rg --quiet 'grafana-secret' "$calls"; then
    bad "$name" "Grafana token appeared in process arguments"
  else
    ok "$name"
  fi
}

test_should_create_alerts_with_source_threshold() {
  local name="test_should_create_alerts_with_source_threshold"
  local output status=0
  output="$(run_script MOCK_MODE=create bash "$PROVIDER_DIR/alerts.sh")" ||
    status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(rg -c -- '--request POST' "$calls")" -ne 6 ]; then
    bad "$name" "expected six alert creates"
  elif ! rg --quiet \
    'agentsfleet_runner_last_seen_seconds > 90' "$captures"; then
    bad "$name" "runner threshold was not derived from source"
  elif rg --quiet 'grafana-secret' "$calls"; then
    bad "$name" "Grafana token appeared in process arguments"
  else
    ok "$name"
  fi
}

test_should_update_existing_resources_with_versions() {
  local name="test_should_update_existing_resources_with_versions"
  local output status=0
  output="$(run_script MOCK_MODE=update bash "$PROVIDER_DIR/resources.sh")" ||
    status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(rg -c -- '--request PUT' "$calls")" -ne 2 ]; then
    bad "$name" "expected one folder and one dashboard update"
  elif ! rg --quiet '"resourceVersion": "7"' "$captures"; then
    bad "$name" "dashboard update omitted the current resource version"
  else
    ok "$name"
  fi
}

test_should_update_existing_alerts_with_versions() {
  local name="test_should_update_existing_alerts_with_versions"
  local output status=0
  output="$(run_script MOCK_MODE=update bash "$PROVIDER_DIR/alerts.sh")" ||
    status=$?
  if [ "$status" -ne 0 ]; then
    bad "$name" "$output"
  elif [ "$(rg -c -- '--request PUT' "$calls")" -ne 6 ]; then
    bad "$name" "expected six alert updates"
  elif [ "$(rg -l 'resourceVersion.*7' \
    "$captures" | wc -l | tr -d ' ')" -ne 6 ]; then
    bad "$name" "an alert update omitted the current resource version"
  else
    ok "$name"
  fi
}

test_should_fail_when_grafana_rejects_a_write() {
  local name="test_should_fail_when_grafana_rejects_a_write"
  local output status=0
  output="$(
    run_script MOCK_MODE=create MOCK_WRITE_STATUS=500 \
      bash "$PROVIDER_DIR/resources.sh"
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "a rejected Grafana write passed"
  else
    ok "$name"
  fi
}

test_should_require_write_approval() {
  local name="test_should_require_write_approval"
  local output status=0
  output="$(
    run_script ALLOW_OBSERVABILITY_WRITES=0 bash "$GATE" apply dev grafana
  )" || status=$?
  if [ "$status" -eq 0 ]; then
    bad "$name" "Grafana writes ran without approval"
  else
    ok "$name"
  fi
}

test_should_reject_unknown_provider() {
  local name="test_should_reject_unknown_provider"
  local output status=0
  output="$(run_script bash "$GATE" check dev elastic)" || status=$?
  if [ "$status" -ne 2 ]; then
    bad "$name" "an unsupported provider did not fail with usage status"
  elif [ -s "$calls" ]; then
    bad "$name" "an unsupported provider reached Grafana"
  else
    ok "$name"
  fi
}

test_should_reject_invalid_gate_inputs() {
  local name="test_should_reject_invalid_gate_inputs"
  local output status arguments
  local cases=(
    ''
    'inspect dev grafana'
    'check staging grafana'
  )

  for arguments in "${cases[@]}"; do
    status=0
    read -r -a argv <<<"$arguments"
    output="$(run_script bash "$GATE" "${argv[@]}")" || status=$?
    if [ "$status" -ne 2 ]; then
      bad "$name" "invalid input '$arguments' did not fail with usage status"
      return
    fi
    if [ -s "$calls" ]; then
      bad "$name" "invalid input '$arguments' reached Grafana"
      return
    fi
  done
  ok "$name"
}

test_should_validate_assets
test_should_verify_prometheus_without_exposing_token
test_should_reject_wrong_datasource_type
test_should_create_dashboard_and_folder
test_should_create_alerts_with_source_threshold
test_should_update_existing_resources_with_versions
test_should_update_existing_alerts_with_versions
test_should_fail_when_grafana_rejects_a_write
test_should_require_write_approval
test_should_reject_unknown_provider
test_should_reject_invalid_gate_inputs

printf '\n%d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
