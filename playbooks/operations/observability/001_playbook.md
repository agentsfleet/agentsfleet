# Provision Grafana Observability

**Owner:** 🦉 Orly, with 🤠 Indy for account access and alert routing
**Applies to:** development and production independently

This runbook installs one environment-scoped folder, one runtime dashboard,
and six Grafana-managed alerts. It never guesses a Grafana Cloud namespace or
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

1. Confirm the `agentsfleetd` OTLP export credentials (`GRAFANA_OTLP_*`) are
   set for the environment — runtime metric families arrive via the daemon's
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
   | `ZMB_CD_DEV` | `agentsfleet-fleets-investigation-service-token` | `grafana-url`, `grafana-sa-token`, `grafana-namespace`, `prometheus-datasource-uid` |
   | `ZMB_CD_PROD` | `agentsfleet-fleets-investigation-service-token` | `grafana-url`, `grafana-sa-token`, `grafana-namespace`, `prometheus-datasource-uid` |

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
- Every panel uses the pinned Prometheus datasource.
- All five alert rules match the repository expressions.
- `agentsfleet_api_in_flight_requests` returns at least one series.
- The Grafana token never appears in process arguments or logs.
- The four Service Level Indicator panels render: admission availability,
  runner success, work picked up inside the replay floor, and worst runner
  heartbeat age.
- Both error-budget burn panels carry their unproven marker.
- The producer-gap list and the shared-tenant warning are NOT on the dashboard.
  They were a text panel until Sep 18, 2026; the census and
  `rustd/crates/afd_observability/src/metrics/produced.rs` are the source of
  truth for what has no producer, and the warning above this list is where an
  operator reads it. A dashboard panel was a second copy of both.
- `runner-silent` is quiet while runners heartbeat. The family reports a Unix
  epoch, so the rule subtracts it from evaluation time and takes the freshest
  reading per runner; a rule missing either half alerts on a healthy fleet.

The dashboard reads runtime families the daemon pushes over the OpenTelemetry
Protocol (OTLP) — its single metrics egress; there is no scrape path. The
export credentials live in `grafana-dev` and `grafana-prod`, separate from the
provisioning service-account items above.
