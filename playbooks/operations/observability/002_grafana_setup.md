# Grafana Observability Setup

## Datasources Required

### 1. Prometheus (Grafana Cloud or self-hosted)

Scrapes `agentsfleetd` at `/metrics`. All `agent_*` counters, histograms, and gauges.

### 2. Tempo (Grafana Cloud)

Receives OTLP traces from `agentsfleetd` background flush thread.
Config: `GRAFANA_OTLP_ENDPOINT`, `GRAFANA_OTLP_INSTANCE_ID`, `GRAFANA_OTLP_API_KEY`.

**Manual / out-of-scope of the automated gate:** no gate verifies Tempo. Set the
three `GRAFANA_OTLP_*` env vars on the worker and wire the Tempo datasource in the
Grafana UI by hand; `03_dashboard.sh` checks only the Prometheus + PostgreSQL
datasources and the dashboard panel counts.

### 3. PostgreSQL

Direct connection to the agent database for `usage_ledger` queries.

**Setup:**
1. In Grafana, add a PostgreSQL datasource.
2. Connection: use the same `DATABASE_URL` as the worker (read-only replica preferred).
3. Name it `agentsfleet-postgres` (the dashboard references this name).

## Dashboard Import

Import `agent_run_breakdown.json` via Grafana UI:
1. Go to Dashboards > Import.
2. Upload `deploy/grafana/agent_run_breakdown.json`.
3. Select datasources when prompted:
   - `prometheus` → your Prometheus datasource
   - `agentsfleet-postgres` → your PostgreSQL datasource
4. Dashboard loads with template variables for `workspace_id` and time range.

## Panels

| Panel | Source | Query |
|-------|--------|-------|
| Token consumption by workspace | Prometheus | `agent_agent_tokens_by_workspace_total` |
| Run outcomes by workspace | Prometheus | `agent_runs_completed_by_workspace_total` / `blocked` |
| Score-gated run rate | PostgreSQL | `usage_ledger WHERE lifecycle_event = 'run_not_billable_score_gated'` |
| Top-N runs by token consumption | PostgreSQL | `usage_ledger ORDER BY token_count DESC LIMIT 10` |
| Per-stage token breakdown | PostgreSQL | `usage_ledger WHERE source = 'runtime_stage' GROUP BY actor` |
| Run trace lookup | Tempo | Link to `{run.id="<run_id>"}` trace search |
