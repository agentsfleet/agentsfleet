#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "$SCRIPT_DIR/common.sh"

obs_select_environment
obs_open_session
trap obs_close_session EXIT

datasource="$(obs_get_json "/api/datasources/uid/$OBS_PROMETHEUS_UID")"
if ! jq -e \
  --arg uid "$OBS_PROMETHEUS_UID" \
  '.uid == $uid and .type == "prometheus"' \
  <<<"$datasource" >/dev/null; then
  echo "ERROR: $OBS_PROMETHEUS_UID is not a Prometheus datasource" >&2
  exit 1
fi

query_result="$(
  obs_get_query \
    "/api/datasources/proxy/uid/$OBS_PROMETHEUS_UID/api/v1/query" \
    "agentsfleet_api_in_flight_requests"
)"
if ! jq -e \
  '.status == "success" and (.data.result | length > 0)' \
  <<<"$query_result" >/dev/null; then
  echo "ERROR: Prometheus does not scrape agentsfleet_api_in_flight_requests" >&2
  exit 1
fi

echo "PASS: $OBS_ENVIRONMENT Prometheus datasource scrapes agentsfleetd"
