#!/usr/bin/env bash
# Verify Prometheus scrapes agent_* metrics.
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

# Query a known metric
RESULT=$(curl -sf -H "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/datasources/proxy/$PROM_ID/api/v1/query?query=agent_runs_created_total" 2>/dev/null || echo "")

if echo "$RESULT" | jq -e '.data.result | length > 0' >/dev/null 2>&1; then
  echo "PASS: agent_runs_created_total is being scraped"
else
  echo "WARN: agent_runs_created_total returned no results (may be OK if no runs yet)"
  echo "  Verify Prometheus scrape config includes agentsfleetd /metrics endpoint"
fi
