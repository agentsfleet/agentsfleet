---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Set up Grafana observability

## What this does

Installs one environment folder, six dashboards and four Grafana-managed alerts from `assets/`.
The six `dashboard*.json` files and `alerts.json` are the source for apply, review and drift checks.
Reconcile any change made in the Grafana editor back into them.
Each run targets `ENV=dev` or `ENV=prod`; the gate refuses `all`.

Both environments share one Grafana stack, namespace, datasource and ingest credential.
No series carries `deployment.environment`, so a production apply adds a second dashboard over the same mixed series.
Never claim development and production isolation; see `docs/architecture/observability.md` §Service Level Objectives.

## Before you begin

🤠 Indy does these per environment.

1. Set `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_EXPORTER_OTLP_HEADERS` for `agentsfleetd`.
   The daemon pushes metrics through its Fly collector; nothing scrapes it.
2. Create a Grafana service account that reads the datasource and manages folders, dashboards and alert rules.
3. Enable Grafana's alert-rule resource API; Grafana 12.4 may need the `kubernetesAlertingRules` feature toggle.
4. Confirm a notification policy routes alerts labelled `service=agentsfleetd`.
5. Store these fields in item `agentsfleet-fleets-investigation-service-token`, in the vault `VAULT_DEV` or `VAULT_PROD` names:
   `grafana-url` (HTTPS), `grafana-sa-token`, `grafana-namespace`, `prometheus-datasource-uid`, `loki-datasource-uid`.
   The namespace is `default` for self-hosted organization 1.
   Grafana Cloud uses `stacks-<stack-id>`; copy it from the stack's Swagger page.
6. Tell the agent: `Grafana access and alert routing are ready for <environment>.`

## Steps

Run each command for `dev`, then repeat with `prod`.

1. 🦉 The agent checks assets, credentials and live data without writing.

   ```bash
   ALLOW_VAULT_READS=1 ACTION=check ENV=dev ./playbooks/operations/observability/00_gate.sh
   ```

   ```text
   PASS: Grafana assets are valid and reference source-owned metrics
   PASS: development Grafana credentials are present
   PASS: development Prometheus datasource scrapes agentsfleetd
   PASS: development Loki datasource returns agentsfleetd logs
   PASS: observability check completed for dev
   ```

2. 🤠 Indy authorizes the Grafana changes.
3. 🦉 The agent applies them.

   ```bash
   ALLOW_VAULT_READS=1 ALLOW_OBSERVABILITY_WRITES=1 \
     ACTION=apply ENV=dev ./playbooks/operations/observability/00_gate.sh
   ```

   ```text
   PASS: development Grafana alerts are current
   PASS: observability apply completed for dev
   ```

4. 🦉 The agent runs the read-only drift check.

   ```bash
   ALLOW_VAULT_READS=1 ACTION=verify ENV=dev ./playbooks/operations/observability/00_gate.sh
   ```

   ```text
   PASS: development Grafana resources match the repository
   PASS: observability verify completed for dev
   ```

## Verify it works

- The folder holds Overview, Chat latency, API & Database, Scheduler, Models and Sandbox, and their links work.
- Queries use the pinned Prometheus datasource.
  Chat latency also takes a Tempo datasource and fleet identifier, which do not filter the other pages.
- The four alert rules match `alerts.json`; `runner-silent` uses the freshest last-seen epoch per runner.
- `agentsfleet_api_in_flight_requests` returns at least one series.
- No token appears in process arguments or logs.

A missing series is not zero, and a zero does not prove the export path healthy.
A family with no activity has no series; check a real lease first.
An unmapped provider omits `gen_ai.provider.name`, not the whole span.
Check collector logs for downstream and metrics-ingest refusals, plus SDK export-loss counters.

## Collector metric conversion

- The daemon accumulates cumulative totals in process; the Fly collector forwards them unchanged.
- The Fly collector keeps `memory_limiter`, `delta_to_cumulative`, then `batch` for older producers.
- The runner's host collector (`deploy/baremetal/otelcol-runner.yml`) still converts the runner's delta histograms.
- `delta_to_cumulative` keeps at most 10,000 streams in memory and drops a stream idle for five minutes.
  A restart or expired stream starts a new cumulative window.
- The exporter queue is also in memory.

## Related pages

- [Read a chat's round trip at p95 and p99](002_round_trip_percentiles.md)
