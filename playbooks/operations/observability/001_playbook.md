# Playbook — Grafana Observability Stack

**Updated:** Jul 24, 2026
**Prerequisite:** Grafana Cloud account (or self-hosted Grafana). Prometheus scraping `agentsfleetd` at `/metrics`.

Bootstrap the Grafana observability stack so operators can reach `agentsfleetd`
telemetry from Grafana rather than from database queries.

> **Dashboards are not provisioned here.** The dashboard JSON under
> `deploy/grafana/` was deleted alongside the telemetry semantic-conventions
> cutover: every panel queried a metric family or table that no longer existed.
> Dashboard authoring is owned by its own workstream and will reintroduce both
> the JSON artefacts and the import step. This playbook covers datasources and
> scrape verification only — the parts that stay true regardless of which
> dashboards exist.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Human | Provide Grafana credentials |
| 1.0 | Agent | Verify the Prometheus datasource scrapes the `agentsfleet_*` namespace |

After step 0 the agent runs step 1 without human intervention.

---

## 0.0 Human: Provide Grafana Access

**Goal:** Agent has credentials to configure Grafana datasources.

1. Create a Grafana service account with `Editor` role
2. Generate a service account token
3. Store in vault:

```
Vault: ZMB_CD_DEV (or ZMB_CD_PROD for production)
Item: grafana-observability
Fields:
  grafana-url → https://your-instance.grafana.net (or self-hosted URL)
  grafana-sa-token → gsa_xxxxxxxxxxxx
```

4. Signal agent: "Grafana credentials ready"

### Acceptance

```bash
op read "op://ZMB_CD_DEV/grafana-observability/grafana-url"
op read "op://ZMB_CD_DEV/grafana-observability/grafana-sa-token"
# Both return non-empty values.
```

---

## 1.0 Agent: Verify Prometheus Datasource

**Goal:** Confirm Prometheus is scraping `agentsfleetd`.

Every Prometheus family the daemon renders carries the `agentsfleet_` prefix —
one process exposes one namespace. Probe a family that renders on **every**
scrape, so an empty result distinguishes "not scraped" from "nothing has
happened yet"; the runner, durable-memory, and Redis-pool families are all
activity-gated and would make the check ambiguous.

```bash
GRAFANA_URL=$(op read "op://ZMB_CD_DEV/grafana-observability/grafana-url")
GRAFANA_TOKEN=$(op read "op://ZMB_CD_DEV/grafana-observability/grafana-sa-token")

# List datasources and find Prometheus
curl -sH "Authorization: Bearer $GRAFANA_TOKEN" "$GRAFANA_URL/api/datasources" | jq '.[].name'

# Query an unconditionally-rendered family to verify the scrape is working
curl -sH "Authorization: Bearer $GRAFANA_TOKEN" \
  "$GRAFANA_URL/api/datasources/proxy/1/api/v1/query?query=agentsfleet_api_in_flight_requests" | jq '.data.result'
```

### Acceptance

- `agentsfleet_api_in_flight_requests` returns at least one result.
- If not: check Prometheus scrape config targets include `agentsfleetd:PORT/metrics`.

---

## Gate

```bash
bash playbooks/operations/observability/00_gate.sh
```

Runs credential resolution and the scrape check above. Both must pass.
