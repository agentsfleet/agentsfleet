# Grafana Observability Setup

## Datasources Required

### 1. Prometheus (Grafana Cloud or self-hosted)

Scrapes `zombied` at `/metrics`. All `zombie_*` counters, histograms, and gauges.

### 2. Tempo (Grafana Cloud)

Receives OTLP traces from `zombied` background flush thread.
Config: `GRAFANA_OTLP_ENDPOINT`, `GRAFANA_OTLP_INSTANCE_ID`, `GRAFANA_OTLP_API_KEY`.

### 3. PostgreSQL

Direct connection to the zombie database for `usage_ledger` queries.

**Setup:**
1. In Grafana, add a PostgreSQL datasource.
2. Connection: use the same `DATABASE_URL` as the worker (read-only replica preferred).
3. Name it `zombie-postgres` (the dashboard references this name).

## Dashboard Import

Import `agent_run_breakdown.json` via Grafana UI:
1. Go to Dashboards > Import.
2. Upload `docs/grafana/agent_run_breakdown.json`.
3. Select datasources when prompted:
   - `prometheus` → your Prometheus datasource
   - `zombie-postgres` → your PostgreSQL datasource
4. Dashboard loads with template variables for `workspace_id` and time range.

## Panels

| Panel | Source | Query |
|-------|--------|-------|
| Token consumption by workspace | Prometheus | `zombie_agent_tokens_by_workspace_total` |
| Run outcomes by workspace | Prometheus | `zombie_runs_completed_by_workspace_total` / `blocked` |
| Gate repair loop distribution | Prometheus | `histogram_quantile(0.95, zombie_gate_repair_loops_per_run_bucket)` |
| Score-gated run rate | PostgreSQL | `usage_ledger WHERE lifecycle_event = 'run_not_billable_score_gated'` |
| Top-N runs by token consumption | PostgreSQL | `usage_ledger ORDER BY token_count DESC LIMIT 10` |
| Per-stage token breakdown | PostgreSQL | `usage_ledger WHERE source = 'runtime_stage' GROUP BY actor` |
| Run trace lookup | Tempo | Link to `{run.id="<run_id>"}` trace search |
