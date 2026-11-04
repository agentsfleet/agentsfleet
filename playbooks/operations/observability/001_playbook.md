# Provision Grafana Observability

**Owner:** 🦉 Orly, with 🤠 Indy for account access and alert routing
**Applies to:** development and production independently

This runbook installs one environment-scoped folder, six focused dashboards,
and four Grafana-managed alerts. It never guesses a Grafana Cloud namespace or
Prometheus datasource.

**Read before applying to production.** Both environments use the same Grafana stack, namespace, datasource and ingest credential.
No series carries a `deployment.environment` attribute.

Applying to production today produces a second dashboard reading the same mixed series as the first.
Confirm active producers before interpreting environment-specific pages. See
`docs/architecture/observability.md` §Service Level Objectives.

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

- The environment folder contains Overview, Chat latency, API & Database,
  Scheduler, Models and Sandbox; their navigation links work.
- Queries use the pinned Prometheus datasource. Chat latency uses a selectable
  Tempo datasource and fleet identifier; that selection does not filter the
  aggregate metric pages.
- All four alert rules match the repository expressions.
- Native renders agree with the dashboard's queries, labels and units.
- `agentsfleet_api_in_flight_requests` returns at least one series.
- Empty samples remain absent. Measured zero remains zero. Neither proves that
  the complete export path is healthy.
- Check collector logs for downstream refusal as well as SDK export-loss
  counters. An acknowledged batch can still fail after entering its collector.
- Attribute omissions are separate from lost records. An unmapped provider
  omits `gen_ai.provider.name`, not the complete span.
- `runner-silent` subtracts last-seen epochs from evaluation time and uses the
  freshest reading per runner.
- No token appears in process arguments or logs.

The Grafana stack is shared across environments. A page's deployment title
selects its configured datasource; it does not manufacture a missing
resource/environment label. Do not claim dev/prod isolation without one.

The daemon pushes runtime families through its Fly collector. The runner uses
its dedicated host collector for its own spans and metric families. A family
with no activity can have no series; verify a real lease before diagnosing it.

### Why the JSON stays in the repository

Six `dashboard*.json` definitions and `alerts.json` are the reproducible source
for apply, review and drift checks. Grafana stores the applied copies. Changes
made in its editor must be reconciled back into these assets.

## Collector metric conversion

Daemon counters and histograms accumulate in the source process before reaching the shared Fly collector endpoint.
Each collector forwards these cumulative totals unchanged.
Changing collectors therefore cannot split one daemon's running total.

The metrics pipeline retains memory admission, `delta_to_cumulative`, then batching for older producers during rollout.
The runner's isolated host collector still converts its delta histograms.
Traces and logs retain their pipelines.

The processor retains at most 10,000 streams and expires idle state after five
minutes. A restart or expired stream starts a new cumulative window. This state
is in memory; the existing exporter queue is also in memory.

After deployment, read the delivery histogram in Prometheus and check the
collector logs for metrics-ingest refusals. Configuration validation does not
prove that the remote backend accepted an export.
