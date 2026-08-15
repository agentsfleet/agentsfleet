# Preflight Readiness

**Updated:** Jul 31, 2026
**Owner:** Human
**Executor:** Agent runs the gates; Pipeline repeats the deployment gate
**Prerequisite:** `playbooks/founding/01_bootstrap/001_playbook.md`

The gate has two chronological stages:

1. `bootstrap` proves the inputs needed to create infrastructure.
2. `deployment` proves the provider outputs needed by the first application
   deployment.

Values generated only after the first deployment are deliberately absent from
both stages. Their owning runbooks gate them later.

## Execution and verification

| Order | Executor | Action | Verifier | Required evidence |
|---|---|---|---|---|
| 1 | Human | Create provider accounts, service tokens, Clerk applications, and two administrator login identities. | Human | Account names and non-secret identifiers recorded. |
| 2 | Human | Store bootstrap fields in 1Password. | Bootstrap gate | Green output for development and production. |
| 3 | Human | Apply `tailnet-policy.hujson` in Tailscale. | Tailscale Secure Shell (SSH) tests | Every embedded test passes. |
| 4 | Agent | Run the `bootstrap` gate for both environments. | `00_gate.sh` | Green local output. |
| 5 | Human and Agent | Complete infrastructure priming and store generated outputs. | Deployment gate | Green output for both environments. |
| 6 | Pipeline | Repeat the `deployment` gate before each control-plane deployment. | `check-credentials` job | Green job URL. |

## Bootstrap inputs

Development vault `ZMB_CD_DEV`:

| Item | Fields |
|---|---|
| `fly-api-token` | `credential` |
| `posthog-dev` | `credential` |
| `clerk-dev` | `publishable-key`, `secret-key`, `webhook-secret`, `issuer` |
| `e2e-fixtures-email` | `regular`, `admin` |
| `agentsfleet-admin` | `username`, `credential` |
| `encryption-master-key` | `credential` |
| `auth-session-code-pepper`, `audit-log-pepper` | `credential` |

Production vault `ZMB_CD_PROD`:

| Item | Fields |
|---|---|
| `cloudflare-api-token`, `fly-api-token`, `npm-publish-token`, `vercel-api-token` | `credential` |
| `vercel-bypass-website`, `vercel-bypass-agents`, `vercel-bypass-app` | `credential` |
| `discord-ci-webhook`, `discord-release-webhook`, `posthog-prod` | `credential` |
| `clerk-prod` | `publishable-key`, `secret-key`, `webhook-secret`, `issuer` |
| `e2e-fixtures-email` | `regular`, `admin` |
| `agentsfleet-admin` | `username`, `credential` |
| `encryption-master-key` | `credential` |
| `auth-session-code-pepper`, `audit-log-pepper` | `credential` |
| `tailscale` | `oauth-client-id`, `oauth-secret` |

Development deployment preflight also reads
`ZMB_CD_PROD/discord-ci-webhook/credential`, because development verdicts post
to the shared public community channel rather than a duplicate development
webhook.

Run:

```bash
ENV=all STAGE=bootstrap \
  ./playbooks/founding/02_preflight/00_gate.sh
```

## Deployment inputs

The `deployment` gate adds the fields needed to start the applications. Step 01
creates the approval signing secret; infrastructure priming records the provider
outputs:

| Environment item | Fields |
|---|---|
| `cloudflare-r2` | `account-id`, `access-key-id`, `secret-access-key`, `bucket` |
| `cloudflare-tunnel-dev`, `cloudflare-tunnel-prod` | `credential` |
| `planetscale-dev`, `planetscale-prod` | `api-connection-string`, `migrator-connection-string` |
| `upstash-dev`, `upstash-prod` | `api-url`, `url` |
| `approval-signing-secret` | `credential` |
| `grafana-dev`, `grafana-prod` | `otlp-endpoint`, `instance-id`, `api-key` |

Run:

```bash
ENV=all STAGE=deployment \
  ./playbooks/founding/02_preflight/00_gate.sh
```

The deployment stage also verifies that:

- the runtime and migrator database strings differ;
- `03_vercel_envs.sh` confirms required Vercel variables exist on preview and
  production targets for `ENV=all` and `ENV=prod`;
- URLs have valid shapes;
- no secret value is printed.

## Post-deploy outputs

Do not invent placeholders for these values:

| Value | Created by |
|---|---|
| `agentsfleet-admin/platform_admin_workspace_id` | `operations/admin_bootstrap` after Clerk signup |
| `agentsfleet-admin/api-key` | `operations/admin_bootstrap` from the dashboard's one-time reveal |
| `github-app`, `slack-app`, `zoho-app`, `jira-app`, `linear-app` | the matching provider registration playbook |
| `qstash` | `operations/qstash_registration` |
| `grafana-observability` | `operations/observability` |
| runner `tailscale-hostname` and `deploy-user` | the environment runner-bootstrap step |
| runner `runner-token` | the dashboard **Add runner** action |

The initial deployment does not need an admin workspace pointer. `agentsfleetd`
starts its connector broker in static-only mode; admin bootstrap sets the
pointer and restarts the service before provider connectors are enabled.

## Required result

- The `bootstrap` gate is green before infrastructure priming.
- The `deployment` gate is green before either initial control-plane
  deployment.
- No generated post-deploy field is required early or represented by a fake
  value.
- The Agent records the local gate output; each deployment records the
  `check-credentials` job URL for the same inputs.
