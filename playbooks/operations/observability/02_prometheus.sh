#!/usr/bin/env bash
# Verify Prometheus scrapes the agentsfleetd metric namespace.
#
# Reads the vault, so it carries the same approval + auth gates as every other
# script under playbooks/operations/ (enforced by `make check-playbooks`).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../lib/common.sh
source "$SCRIPT_DIR/../../lib/common.sh"

playbooks_require_vault_read_approval
playbooks_require_op_auth

VAULT="${VAULT_DEV:-ZMB_CD_DEV}"
GRAFANA_URL=$(op read "op://$VAULT/grafana-observability/grafana-url")
GRAFANA_TOKEN=$(op read "op://$VAULT/grafana-observability/grafana-sa-token")

echo "Checking Prometheus datasource at $GRAFANA_URL"

# Find Prometheus datasource
DS_LIST=$(curl -sf -H "Authorization: Bearer $GRAFANA_TOKEN" "$GRAFANA_URL/api/datasources" 2>/dev/null || echo "[]")
PROM_ID=$(echo "$DS_LIST" | jq -r '[.[] | select(.type == "prometheus")][0].id // empty')

if [ -z "$PROM_ID" ]; then
  echo "FAIL: no Prometheus datasource found in Grafana"
  exit 1
fi
echo "  Prometheus datasource ID: $PROM_ID"

# Probe a family the daemon renders unconditionally, so an empty result means
# "not scraped" rather than "nothing has happened yet". Activity-gated families
# (runner, durable memory, Redis pool) would make this check ambiguous.
PROBE_METRIC="agentsfleet_api_in_flight_requests"

RESULT=$(curl -sf -H "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/datasources/proxy/$PROM_ID/api/v1/query?query=$PROBE_METRIC" 2>/dev/null || echo "")

if echo "$RESULT" | jq -e '.data.result | length > 0' >/dev/null 2>&1; then
  echo "PASS: $PROBE_METRIC is being scraped"
else
  echo "FAIL: $PROBE_METRIC returned no results"
  echo "  This family renders on every scrape, so an empty result means the"
  echo "  scrape config does not reach agentsfleetd's /metrics endpoint."
  exit 1
fi
