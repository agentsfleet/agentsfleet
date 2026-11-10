---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Preflight Readiness

## What this does

`00_gate.sh` proves the vault holds what the next step needs, without printing a value.

| `STAGE` | Proves | Must be green before |
|---|---|---|
| `bootstrap` | Inputs that create infrastructure | [Step 03](../03_priming_infra/001_playbook.md) |
| `deployment` | Provider outputs the first deployment reads | Any control-plane deployment |

`ENV` is `all`, `dev` or `prod`; the default is `all`.
`VAULT_DEV` and `VAULT_PROD` default to `ZMB_CD_DEV` and `ZMB_CD_PROD`.
The gate needs `op` signed in, or `OP_SERVICE_ACCOUNT_TOKEN` set.

`deploy-dev.yml` and `release.yml` rerun the `deployment` gate in their `check-credentials` job.

## Before you begin

[Step 01](../01_bootstrap/001_playbook.md) is done.

## Bootstrap inputs

Development vault `ZMB_CD_DEV`:

| Item | Fields |
|---|---|
| `fly-api-token`, `posthog-dev` | `credential` |
| `clerk-dev` | `publishable-key`, `secret-key`, `webhook-secret`, `issuer` |
| `e2e-fixtures-email` | `regular`, `admin` |
| `agentsfleet-admin` | `username`, `credential` |
| `encryption-master-key`, `auth-session-code-pepper` | `credential` |

Development also reads `ZMB_CD_PROD/discord-ci-webhook/credential`; both environments post to one channel.

Production vault `ZMB_CD_PROD`:

| Item | Fields |
|---|---|
| `cloudflare-api-token`, `fly-api-token`, `npm-publish-token`, `vercel-api-token` | `credential` |
| `vercel-bypass-website`, `vercel-bypass-agents`, `vercel-bypass-app` | `credential` |
| `discord-ci-webhook`, `discord-release-webhook`, `posthog-prod` | `credential` |
| `clerk-prod` | `publishable-key`, `secret-key`, `webhook-secret`, `issuer` |
| `e2e-fixtures-email` | `regular`, `admin` |
| `agentsfleet-admin` | `username`, `credential` |
| `encryption-master-key`, `auth-session-code-pepper` | `credential` |
| `tailscale` | `oauth-client-id`, `oauth-secret` |

## Deployment inputs

The `deployment` stage adds these, per environment (`<env>` is `dev` or `prod`):

| Item | Fields |
|---|---|
| `approval-signing-secret` | `credential` (from step 01) |
| `planetscale-<env>` | `api-connection-string` (port `6432`), `migrator-connection-string` (port `5432`) |
| `nats-<env>` | `api-url` |
| `grafana-<env>` | `otlp-endpoint`, `instance-id`, `api-key` |
| `cloudflare-tunnel-<env>` | `credential` |
| `cloudflare-r2` | `account-id`, `access-key-id`, `secret-access-key`, `bucket` |
| `toolbox-release-key` | `private-key`, `password`; always read from `ZMB_CD_PROD` |

The stage also fails when the two PlanetScale strings are equal or name the wrong port.
With `ENV=all` or `ENV=prod` it runs `03_vercel_envs.sh`, which checks Vercel variables on preview and production targets.

## Values created after the first deployment

No gate requires these. Never store a placeholder.

| Value | Created by |
|---|---|
| `agentsfleet-admin/platform_admin_workspace_id`, `agentsfleet-admin/api-key` | `operations/admin_bootstrap` |
| `github-app`, `slack-app`, `zoho-app`, `jira-app`, `linear-app` | the matching registration playbook |
| `qstash` | `operations/qstash_registration` |
| `smtp-relay` (`host`, `port`, `username`, `password`, `from_address`) | `operations/smtp_relay_registration`, after the relay verifies the sending domain |
| `agentsfleet-fleets-investigation-service-token` | `operations/observability` |
| runner `tailscale-hostname`, `deploy-user` | the environment's runner bootstrap step |
| runner `runner-token` | the dashboard **Add runner** action |

The first deployment starts the connector broker in static-only mode.
Admin bootstrap sets the workspace pointer and restarts the service.

## Steps

1. The Human applies `playbooks/founding/02_preflight/tailnet-policy.hujson` in the Tailscale console. Every embedded Secure Shell (SSH) test must pass.
2. The Agent runs the `bootstrap` gate:

   ```bash
   ENV=all STAGE=bootstrap ./playbooks/founding/02_preflight/00_gate.sh
   ```

   The last line is `✅ 002_preflight check complete (env: all, stage: bootstrap)`.
3. After step 03 stores provider outputs, the Agent runs the `deployment` gate:

   ```bash
   ENV=all STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh
   ```

   The last line is `✅ 002_preflight check complete (env: all, stage: deployment)`.

## Common problems

| Output | Fix |
|---|---|
| `✗ INVALID: <env> postgres api (PgBouncer) must name port 6432` | Store the PgBouncer string from [step 03](../03_priming_infra/001_playbook.md#2-create-datastores) |
| `❌ 1Password auth missing` | Run `op signin` or set `OP_SERVICE_ACCOUNT_TOKEN` |
| `✗ MISSING: op://<VAULT>/<ITEM>/<FIELD>` | Add that field, then rerun |
| `✗ INVALID URL: op://<VAULT>/<ITEM>/<FIELD>` | Store an `https://` URL with no whitespace |

## Verify it works

Record the local gate output.
Each deployment records its `check-credentials` job URL for the same inputs.
