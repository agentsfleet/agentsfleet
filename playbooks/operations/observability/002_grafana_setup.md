# Grafana Observability Setup

## Datasources Required

### 1. Prometheus (Grafana Cloud or self-hosted)

Scrapes `agentsfleetd` at `/metrics`. Every family the daemon renders carries the
`agentsfleet_` prefix — one process exposes one namespace, so a scrape config
targeting `agentsfleetd` needs no per-family allowlist.

Some families render only once their subsystem has been active (per-runner
counters, durable-memory counters, Redis-pool gauges). Absent series on a fresh
process are expected; downstream scrapers treat them as zero.

### 2. Tempo (Grafana Cloud)

Receives OpenTelemetry Protocol (OTLP) traces from the `agentsfleetd` background
flush thread.
Config: `GRAFANA_OTLP_ENDPOINT`, `GRAFANA_OTLP_INSTANCE_ID`, `GRAFANA_OTLP_API_KEY`.

**Manual / out-of-scope of the automated gate:** no gate verifies Tempo. Set the
three `GRAFANA_OTLP_*` env vars on the worker and wire the Tempo datasource in the
Grafana UI by hand.

### 3. OpenTelemetry Collector (for OTLP metrics)

The metric signal is pushed via OTLP with DELTA temporality. Reading it in
Grafana needs a Collector running the `deltatocumulative` processor, which
converts to cumulative before Grafana Cloud Mimir. Until that Collector is
provisioned, the OTLP metric series are not queryable — the scraped
`agentsfleet_*` families above are unaffected and remain the reliable signal.

Provisioning the Collector is infrastructure work, unowned by this playbook.

## Dashboards

None are provisioned here. The dashboard JSON under `deploy/grafana/` was
deleted alongside the telemetry semantic-conventions cutover — every panel
queried either a Prometheus family no source emitted or a `billing.usage_ledger`
table that no migration ever created. Rather than repoint panels onto a schema
that was itself being rewritten, dashboard authoring moved to its own
workstream, which owns reintroducing the JSON artefacts and the import step.

Until then, query the `agentsfleet_*` namespace directly in Grafana Explore.
