#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# lib.sh loads ../../lib/common.sh before this script reads the vault.
# shellcheck source=lib.sh
source "$SCRIPT_DIR/lib.sh"

obs_select_environment
obs_require_tools
playbooks_require_vault_read_approval
playbooks_require_op_auth

missing=0
for field in \
  grafana-url \
  grafana-sa-token \
  grafana-namespace \
  prometheus-datasource-uid \
  loki-datasource-uid; do
  value="$(playbooks_read_ref_or_empty "op://$OBS_VAULT/agentsfleet-fleets-investigation-service-token/$field")"
  if [ -z "$value" ]; then
    echo "MISSING: $OBS_VAULT / agentsfleet-fleets-investigation-service-token / $field" >&2
    missing=$((missing + 1))
  else
    echo "OK: $OBS_VAULT / agentsfleet-fleets-investigation-service-token / $field"
  fi
done

if [ "$missing" -ne 0 ]; then
  echo "ERROR: $missing Grafana field(s) missing for $OBS_ENVIRONMENT" >&2
  exit 1
fi

echo "PASS: $OBS_ENVIRONMENT Grafana credentials are present"

# The datasource probe rides here rather than in a file of its own: it asks the
# same question this step already asks — do these credentials reach a Grafana
# that can answer — and a credential that resolves against a datasource which
# does not exist is not a credential that works.
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

loki_datasource="$(obs_get_json "/api/datasources/uid/$OBS_LOKI_UID")"
if ! jq -e \
  --arg uid "$OBS_LOKI_UID" \
  '.uid == $uid and .type == "loki"' \
  <<<"$loki_datasource" >/dev/null; then
  echo "ERROR: $OBS_LOKI_UID is not a Loki datasource" >&2
  exit 1
fi

loki_labels="$(
  obs_get_json \
    "/api/datasources/proxy/uid/$OBS_LOKI_UID/loki/api/v1/labels"
)"
if ! jq -e \
  '.status == "success" and (.data | index("service_name") != null)' \
  <<<"$loki_labels" >/dev/null; then
  echo "ERROR: Loki does not expose the service_name label" >&2
  exit 1
fi

echo "PASS: $OBS_ENVIRONMENT Loki datasource exposes agentsfleetd logs"
