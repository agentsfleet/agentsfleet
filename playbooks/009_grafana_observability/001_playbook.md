# M28_001: Playbook — Grafana Observability Stack

**Milestone:** M28
**Workstream:** 001
**Updated:** Apr 05, 2026
**Prerequisite:** Grafana Cloud account (or self-hosted Grafana). Zombie database accessible. Prometheus scraping `zombied` at `/metrics`.

Bootstrap the Grafana observability stack so operators can diagnose runs, token usage, and billing decisions from dashboards — not DB queries.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Human | Provide Grafana credentials + database read-only URL |
| 1.0 | Agent | Verify Prometheus datasource scrapes `zombie_*` metrics |
| 2.0 | Agent | Create PostgreSQL datasource for `usage_ledger` queries |
| 3.0 | Agent | Import `agent_run_breakdown.json` dashboard |
| 4.0 | Agent | Verify all 7 panels render without errors |

After step 0 the agent runs steps 1–4 in sequence without human intervention.

---

## 0.0 Human: Provide Grafana Access

**Goal:** Agent has credentials to configure Grafana datasources and import dashboards.

1. Create a Grafana service account with `Editor` role
2. Generate a service account token
3. Store in vault:

```
Vault: ZMB_CD_DEV (or ZMB_CD_PROD for production)
Item: grafana-observability
Fields:
  grafana-url → https://your-instance.grafana.net (or self-hosted URL)
  grafana-sa-token → gsa_xxxxxxxxxxxx
  db-readonly-url → postgresql://readonly:password@host:5432/zombie
```

4. Signal agent: "Grafana credentials ready"

### Acceptance

```bash
op read "op://ZMB_CD_DEV/grafana-observability/grafana-url"
op read "op://ZMB_CD_DEV/grafana-observability/grafana-sa-token"
op read "op://ZMB_CD_DEV/grafana-observability/db-readonly-url"
# All three return non-empty values.
```

---

## 1.0 Agent: Verify Prometheus Datasource

**Goal:** Confirm Prometheus is scraping `zombied` and `zombie_*` metrics exist.

```bash
GRAFANA_URL=$(op read "op://ZMB_CD_DEV/grafana-observability/grafana-url")
GRAFANA_TOKEN=$(op read "op://ZMB_CD_DEV/grafana-observability/grafana-sa-token")

# List datasources and find Prometheus
curl -sH "Authorization: Bearer $GRAFANA_TOKEN" "$GRAFANA_URL/api/datasources" | jq '.[].name'

# Query a known metric to verify scrape is working
curl -sH "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/datasources/proxy/1/api/v1/query?query=zombie_runs_created_total" | jq '.data.result'
```

### Acceptance

- `zombie_runs_created_total` returns at least one result.
- If not: check Prometheus scrape config targets include `zombied:PORT/metrics`.

---

## 2.0 Agent: Create PostgreSQL Datasource

**Goal:** Grafana can query `billing.usage_ledger` directly.

```bash
DB_URL=$(op read "op://ZMB_CD_DEV/grafana-observability/db-readonly-url")

# Create datasource via Grafana API
curl -X POST -H "Authorization: Bearer $GRAFANA_TOKEN" \
  -H "Content-Type: application/json" \
  "$GRAFANA_URL/api/datasources" \
  -d "{
    \"name\": \"zombie-postgres\",
    \"type\": \"postgres\",
    \"url\": \"$(echo $DB_URL | sed 's|postgresql://[^@]*@||')\",
    \"database\": \"zombie\",
    \"user\": \"$(echo $DB_URL | grep -oP '://\K[^:]+' )\",
    \"secureJsonData\": { \"password\": \"$(echo $DB_URL | grep -oP '://[^:]+:\K[^@]+')\" },
    \"jsonData\": { \"sslmode\": \"require\", \"postgresVersion\": 1500 },
    \"access\": \"proxy\"
  }"
```

### Acceptance

```bash
# Test connection
curl -sH "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/datasources/name/zombie-postgres" | jq '.id'
# Returns a numeric datasource ID (not an error).
```

---

## 3.0 Agent: Import Dashboard

**Goal:** The `agent_run_breakdown.json` dashboard is importable and loads in Grafana.

```bash
# Get datasource UIDs
PROM_UID=$(curl -sH "Authorization: Bearer $GRAFANA_TOKEN" "$GRAFANA_URL/api/datasources/name/Prometheus" | jq -r '.uid')
PG_UID=$(curl -sH "Authorization: Bearer $GRAFANA_TOKEN" "$GRAFANA_URL/api/datasources/name/zombie-postgres" | jq -r '.uid')

# Import dashboard
jq --arg prom "$PROM_UID" --arg pg "$PG_UID" \
  '{ "dashboard": ., "inputs": [{"name":"DS_PROMETHEUS","type":"datasource","pluginId":"prometheus","value":$prom},{"name":"DS_POSTGRES","type":"datasource","pluginId":"postgres","value":$pg}], "overwrite": true }' \
  docs/grafana/agent_run_breakdown.json | \
  curl -X POST -H "Authorization: Bearer $GRAFANA_TOKEN" \
    -H "Content-Type: application/json" \
    "$GRAFANA_URL/api/dashboards/import" -d @-
```

### Acceptance

```bash
curl -sH "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/dashboards/uid/zombie-run-breakdown" | jq '.dashboard.title'
# Returns "Agent Run Breakdown"
```

---

## 4.0 Agent: Verify Panels

**Goal:** All 7 panels render without query errors.

| # | Panel | Verify |
|---|-------|--------|
| 1 | Token consumption by workspace | `zombie_agent_tokens_by_workspace_total` returns data or empty (not error) |
| 2 | Run outcomes by workspace | `zombie_runs_completed_by_workspace_total` queryable |
| 3 | Score-gated run rate | SQL query on `usage_ledger` returns without error |
| 4 | Top-10 runs by token consumption | SQL query returns table (may be empty) |
| 5 | Per-stage token breakdown | SQL query groups by actor |
| 6 | Workspace metrics overflow | `zombie_workspace_metrics_overflow_total` queryable |

### Acceptance

All panels load without red error banners. Empty data is acceptable (no runs yet). Query errors are not.

---

## Gate

```bash
# Verify dashboard exists and has expected panel count
PANELS=$(curl -sH "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/dashboards/uid/zombie-run-breakdown" | jq '.dashboard.panels | length')
[ "$PANELS" -ge 7 ] && echo "PASS: $PANELS panels" || echo "FAIL: expected >= 7 panels, got $PANELS"
```
