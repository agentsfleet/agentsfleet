#!/usr/bin/env bash
# Import and verify every Grafana dashboard in deploy/grafana/.
set -euo pipefail

VAULT="${VAULT_DEV:-ZMB_CD_DEV}"
GRAFANA_URL=$(op read "op://$VAULT/grafana-observability/grafana-url")
GRAFANA_TOKEN=$(op read "op://$VAULT/grafana-observability/grafana-sa-token")
DB_URL=$(op read "op://$VAULT/grafana-observability/db-readonly-url")

REPO_ROOT="$(git rev-parse --show-toplevel)"
DASHBOARD_DIR="$REPO_ROOT/deploy/grafana"

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

echo "=== Step 2: Import + verify every dashboard in deploy/grafana ==="

# Get datasource UIDs
PROM_UID=$(curl -sf -H "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/datasources" | jq -r '[.[] | select(.type == "prometheus")][0].uid // "prometheus"')
PG_UID=$(curl -sf -H "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/datasources/name/zombie-postgres" | jq -r '.uid // empty')

if [ -z "$PG_UID" ]; then
  echo "FAIL: zombie-postgres datasource UID not found"
  exit 1
fi

# Each dashboard declares its own uid + panel set; import then verify against
# that dashboard's own panel count. Providing both datasource inputs is safe —
# Grafana ignores an input a dashboard doesn't reference (runner_fleet.json is
# Prometheus-only; agent_run_breakdown.json uses both).
shopt -s nullglob
DASHBOARDS=("$DASHBOARD_DIR"/*.json)
if [ ${#DASHBOARDS[@]} -eq 0 ]; then
  echo "FAIL: no dashboards found in $DASHBOARD_DIR"
  exit 1
fi

for DASHBOARD_JSON in "${DASHBOARDS[@]}"; do
  NAME=$(basename "$DASHBOARD_JSON")
  DASH_UID=$(jq -r '.uid' "$DASHBOARD_JSON")
  EXPECTED=$(jq '.panels | length' "$DASHBOARD_JSON")

  # Build the inputs array from THIS dashboard's own __inputs, mapping each
  # declared datasource to its resolved uid. A Prometheus-only dashboard
  # (runner_fleet.json) gets only DS_PROMETHEUS; agent_run_breakdown.json gets
  # both. Avoids passing an input a dashboard never declared.
  # --fail-with-body: non-zero on HTTP >=400 but keep Grafana's error JSON on
  # stdout so a rejected import names the dashboard AND what Grafana refused,
  # instead of failing silently under set -e.
  if ! IMPORT_RESP=$(jq --arg prom "$PROM_UID" --arg pg "$PG_UID" \
    '{ dashboard: ., overwrite: true,
       inputs: [ .__inputs[] | { name, type, pluginId,
         value: (if .name == "DS_PROMETHEUS" then $prom
                 elif .name == "DS_POSTGRES" then $pg
                 else "" end) } ] }' \
    "$DASHBOARD_JSON" | \
    curl -sS --fail-with-body -X POST -H "Authorization: Bearer $GRAFANA_TOKEN" \
      -H "Content-Type: application/json" \
      "$GRAFANA_URL/api/dashboards/import" -d @-); then
    echo "  FAIL: import of $NAME failed: $IMPORT_RESP"
    exit 1
  fi
  echo "  Imported $NAME (uid=$DASH_UID)."

  PANELS=$(curl -sf -H "Authorization: Bearer $GRAFANA_TOKEN" \
    "$GRAFANA_URL/api/dashboards/uid/$DASH_UID" | jq '.dashboard.panels | length')

  if [ "$PANELS" -ge "$EXPECTED" ]; then
    echo "  PASS: $NAME has $PANELS panels (expected >= $EXPECTED)"
  else
    echo "  FAIL: $NAME has $PANELS panels (expected >= $EXPECTED)"
    exit 1
  fi
done
