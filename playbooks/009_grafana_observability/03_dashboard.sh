#!/usr/bin/env bash
# M28_001 §3: Import dashboard and verify panels.
set -euo pipefail

VAULT="${VAULT_DEV:-ZMB_CD_DEV}"
GRAFANA_URL=$(op read "op://$VAULT/grafana-observability/grafana-url")
GRAFANA_TOKEN=$(op read "op://$VAULT/grafana-observability/grafana-sa-token")
DB_URL=$(op read "op://$VAULT/grafana-observability/db-readonly-url")

REPO_ROOT="$(git rev-parse --show-toplevel)"
DASHBOARD_JSON="$REPO_ROOT/deploy/grafana/agent_run_breakdown.json"

echo "=== Step 1: Ensure PostgreSQL datasource ==="

PG_EXISTS=$(curl -sf -H "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/datasources/name/zombie-postgres" 2>/dev/null | jq -r '.id // empty' || echo "")

if [ -z "$PG_EXISTS" ]; then
  echo "  Creating zombie-postgres datasource..."
  # Parse connection parts from DATABASE_URL
  DB_HOST=$(echo "$DB_URL" | sed -E 's|postgresql://[^@]+@([^/]+)/.*|\1|')
  DB_NAME=$(echo "$DB_URL" | sed -E 's|postgresql://[^@]+@[^/]+/([^?]+).*|\1|')
  DB_USER=$(echo "$DB_URL" | sed -E 's|postgresql://([^:]+):.*|\1|')
  DB_PASS=$(echo "$DB_URL" | sed -E 's|postgresql://[^:]+:([^@]+)@.*|\1|')

  curl -sf -X POST -H "Authorization: Bearer $GRAFANA_TOKEN" \
    -H "Content-Type: application/json" \
    "$GRAFANA_URL/api/datasources" \
    -d "{
      \"name\": \"zombie-postgres\",
      \"type\": \"postgres\",
      \"url\": \"$DB_HOST\",
      \"database\": \"$DB_NAME\",
      \"user\": \"$DB_USER\",
      \"secureJsonData\": { \"password\": \"$DB_PASS\" },
      \"jsonData\": { \"sslmode\": \"require\", \"postgresVersion\": 1500 },
      \"access\": \"proxy\"
    }" >/dev/null
  echo "  Created."
else
  echo "  zombie-postgres datasource exists (ID: $PG_EXISTS)"
fi

echo "=== Step 2: Import dashboard ==="

# Get datasource UIDs
PROM_UID=$(curl -sf -H "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/datasources" | jq -r '[.[] | select(.type == "prometheus")][0].uid // "prometheus"')
PG_UID=$(curl -sf -H "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/datasources/name/zombie-postgres" | jq -r '.uid // empty')

if [ -z "$PG_UID" ]; then
  echo "FAIL: zombie-postgres datasource UID not found"
  exit 1
fi

jq --arg prom "$PROM_UID" --arg pg "$PG_UID" \
  '{ "dashboard": ., "inputs": [
    {"name":"DS_PROMETHEUS","type":"datasource","pluginId":"prometheus","value":$prom},
    {"name":"DS_POSTGRES","type":"datasource","pluginId":"postgres","value":$pg}
  ], "overwrite": true }' \
  "$DASHBOARD_JSON" | \
  curl -sf -X POST -H "Authorization: Bearer $GRAFANA_TOKEN" \
    -H "Content-Type: application/json" \
    "$GRAFANA_URL/api/dashboards/import" -d @- >/dev/null

echo "  Dashboard imported."

echo "=== Step 3: Verify panels ==="

PANELS=$(curl -sf -H "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/dashboards/uid/zombie-run-breakdown" | jq '.dashboard.panels | length')

if [ "$PANELS" -ge 7 ]; then
  echo "PASS: dashboard has $PANELS panels (expected >= 7)"
else
  echo "FAIL: dashboard has $PANELS panels (expected >= 7)"
  exit 1
fi
