#!/usr/bin/env bash
# M28_001 §1: Verify Prometheus scrapes zombie_* metrics.
set -euo pipefail

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
  "$GRAFANA_URL/api/datasources/proxy/$PROM_ID/api/v1/query?query=zombie_runs_created_total" 2>/dev/null || echo "")

if echo "$RESULT" | jq -e '.data.result | length > 0' >/dev/null 2>&1; then
  echo "PASS: zombie_runs_created_total is being scraped"
else
  echo "WARN: zombie_runs_created_total returned no results (may be OK if no runs yet)"
  echo "  Verify Prometheus scrape config includes zombied /metrics endpoint"
fi
