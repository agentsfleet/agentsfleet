# Provision Grafana Observability

**Owner:** 🦉 Orly, with 🤠 Indy for account access and alert routing
**Applies to:** development and production independently

This runbook installs one environment-scoped folder, one runtime dashboard,
and six Grafana-managed alerts. It never guesses a Grafana Cloud namespace or
Prometheus datasource.

## 🤠 Indy handoff

For each environment:

1. Ensure Prometheus scrapes the `agentsfleetd` `/metrics` endpoint.
2. Create a Grafana service account that can read the selected datasource and
   manage folders, dashboards, and alert rules.
3. Enable Grafana's current alert-rule resource API. Grafana 12.4 installations
   may require the `kubernetesAlertingRules` feature toggle.
4. Confirm a notification policy routes alerts labelled
   `service=agentsfleetd`.
5. Store:

   | Vault | Item | Fields |
   |---|---|---|
   | `ZMB_CD_DEV` | `grafana-observability` | `grafana-url`, `grafana-sa-token`, `grafana-namespace`, `prometheus-datasource-uid` |
   | `ZMB_CD_PROD` | `grafana-observability` | `grafana-url`, `grafana-sa-token`, `grafana-namespace`, `prometheus-datasource-uid` |

For self-hosted Grafana organization 1, the namespace is `default`. Grafana
Cloud uses `stacks-<stack-id>`; copy it from that stack's Swagger page rather
than deriving it.

Tell Orly: `Grafana access and alert routing are ready for <environment>.`

## 🦉 Orly execution

Inspect without writes:

```bash
ALLOW_VAULT_READS=1 \
  ./playbooks/operations/observability/00_gate.sh check dev grafana
```

After Indy authorizes the Grafana changes:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_OBSERVABILITY_WRITES=1 \
  ./playbooks/operations/observability/00_gate.sh apply dev grafana
```

Run the read-only drift check after the apply:

```bash
ALLOW_VAULT_READS=1 \
  ./playbooks/operations/observability/00_gate.sh verify dev grafana
```

Repeat the three commands with `prod`. The provider allowlist currently accepts
only `grafana`.

## Acceptance

- The environment folder and `agentsfleet-runtime-<env>` dashboard exist.
- Every panel uses the pinned Prometheus datasource.
- All six alert rules match the repository expressions.
- `agentsfleet_api_in_flight_requests` returns at least one series.
- The Grafana token never appears in process arguments or logs.

The dashboard uses Prometheus scrape metrics. OpenTelemetry Protocol (OTLP)
export credentials remain in `grafana-dev` and `grafana-prod`; they are a
separate runtime path.
