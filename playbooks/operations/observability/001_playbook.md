# Provision Grafana Observability

**Owner:** 🦉 Orly, with 🤠 Indy for account access and alert routing
**Applies to:** development and production independently

This runbook installs one environment-scoped folder, one runtime dashboard,
and four Grafana-managed alerts. It never guesses a Grafana Cloud namespace or
Prometheus datasource.

**Read before applying to production.** The two environments resolve to the
same Grafana stack, namespace, datasource and ingest credential, and no series
carries a `deployment.environment` attribute. Applying to production today
produces a second dashboard reading the same mixed series as the first. The
development apply is correct only because the production Fly application runs
no machines. See `docs/architecture/observability.md` §Service Level
Objectives.

## 🤠 Indy handoff

For each environment:

1. Confirm the `agentsfleetd` OTLP export settings (`OTEL_EXPORTER_OTLP_ENDPOINT`
   and `OTEL_EXPORTER_OTLP_HEADERS`) are set for the environment — runtime metric families arrive via the daemon's
   own exporter push; nothing scrapes the daemon. The gate's check arm probes
   the datasource for live series and names any family that is absent.
2. Create a Grafana service account that can read the selected datasource and
   manage folders, dashboards, and alert rules.
3. Enable Grafana's current alert-rule resource API. Grafana 12.4 installations
   may require the `kubernetesAlertingRules` feature toggle.
4. Confirm a notification policy routes alerts labelled
   `service=agentsfleetd`.
5. Store:

   | Vault | Item | Fields |
   |---|---|---|
   | `ZMB_CD_DEV` | `agentsfleet-fleets-investigation-service-token` | `grafana-url`, `grafana-sa-token`, `grafana-namespace`, `prometheus-datasource-uid`, `loki-datasource-uid` |
   | `ZMB_CD_PROD` | `agentsfleet-fleets-investigation-service-token` | `grafana-url`, `grafana-sa-token`, `grafana-namespace`, `prometheus-datasource-uid`, `loki-datasource-uid` |

For self-hosted Grafana organization 1, the namespace is `default`. Grafana
Cloud uses `stacks-<stack-id>`; copy it from that stack's Swagger page rather
than deriving it.

Tell Orly: `Grafana access and alert routing are ready for <environment>.`

## 🦉 Orly execution

Inspect without writes:

```bash
ALLOW_VAULT_READS=1 \
  ACTION=check ENV=dev ./playbooks/operations/observability/00_gate.sh
```

After Indy authorizes the Grafana changes:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_OBSERVABILITY_WRITES=1 \
  ACTION=apply ENV=dev ./playbooks/operations/observability/00_gate.sh
```

Run the read-only drift check after the apply:

```bash
ALLOW_VAULT_READS=1 \
  ACTION=verify ENV=dev ./playbooks/operations/observability/00_gate.sh
```

Repeat the three commands with `prod`. The provider allowlist currently accepts
only `grafana`.

## Acceptance

- The environment folder and `agentsfleet-runtime-<env>` dashboard exist.
- Metric panels use the pinned Prometheus datasource; round-trip span panels use the declared Tempo datasource selector.
- All four alert rules match the repository expressions.
- `agentsfleet_api_in_flight_requests` returns at least one series.
- The Grafana token never appears in process arguments or logs.
- The four Service Level Indicator panels render: admission availability,
  runner success, work picked up inside the replay floor, and worst runner
  heartbeat age.
- Both error-budget burn panels carry their unproven marker.
- The shared-tenant warning is NOT on the dashboard. It was a text panel until
  Sep 18, 2026; the warning above this list is where an operator reads it. A
  dashboard panel was a second copy. Every census family has a producer, so
  there is no producer-gap list to show.
- `runner-silent` is quiet while runners heartbeat. The family reports a Unix
  epoch, so the rule subtracts it from evaluation time and takes the freshest
  reading per runner; a rule missing either half alerts on a healthy fleet.

The dashboard reads runtime families the daemon pushes over the OpenTelemetry
Protocol (OTLP) — its single metrics egress; there is no scrape path. The
export credentials live in `grafana-dev` and `grafana-prod`, separate from the
provisioning service-account items above.

## Collector metric conversion

Both Fly collectors convert delta sums and explicit histograms to cumulative
metrics before Grafana ingest. The metrics pipeline runs memory admission,
`delta_to_cumulative`, then batching. Traces and logs retain their pipelines.

The processor retains at most 10,000 streams and expires idle state after five
minutes. A restart or expired stream starts a new cumulative window. This state
is in memory; the existing exporter queue is also in memory.

The existing `check-deploy-safety` lane runs these proofs automatically in an
isolated Docker fixture. For a local Collector contrib binary matching the
deployed release, run:

```bash
make check-collector-temporality COLLECTOR_BINARY=/path/to/otelcol-contrib
```

The test uses loopback transports and the actual configured processor chain.
It checks two delta windows, existing cumulative counters and gauges in both
environments without using vault credentials or contacting Grafana.

After deployment, read the delivery histogram in Prometheus and check the
collector logs for metrics-ingest refusals. Local conversion tests do not prove
that the remote backend accepted an export.
