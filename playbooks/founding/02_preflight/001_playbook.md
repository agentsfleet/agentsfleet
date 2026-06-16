# M2_001: Playbook — Preflight Readiness

**Milestone:** M2
**Workstream:** 001
**Updated:** Apr 02, 2026
**Owner:** Agent
**Status:** ✅ DONE — credential gate passed Apr 02, 2026; all vault items present; M2_002 gate lifted.
**Prerequisite:** `playbooks/founding/01_bootstrap/001_playbook.md` complete.
**Gate:** M2_002 (PRIMING_INFRA) must not start until every check below passes.

This workstream is the eval/feedback harness for Milestone 2. It validates that every
credential the agent needs is present in the correct 1Password vault and returns a non-empty
value. Run this before any infrastructure step. Fail loud — surface every missing item,
not just the first one.

Script: `playbooks/founding/02_preflight/00_gate.sh` — milestone/workstream check runner that runs anywhere `op` CLI is available (local, CI, agent terminal).
Vault names: set `VAULT_DEV` and `VAULT_PROD` as GitHub Actions repository variables (Settings → Variables). Scripts fall back to `ZMB_CD_DEV` / `ZMB_CD_PROD` if not set locally.

---

## 1.0 Required Vault Items

Every `op://` reference the agent will use across M2_002 and the deploy pipelines.

### 1.1 Vault: `ZMB_CD_PROD`

| Item | Field | Used by |
|---|---|---|
| `cloudflare-api-token` | `credential` | DNS setup |
| `npm-publish-token` | `credential` | `release.yml` npm publish |
| `vercel-bypass-website` | `credential` | `smoke-post-deploy.yml` |
| `vercel-bypass-agents` | `credential` | `smoke-post-deploy.yml` |
| `vercel-bypass-app` | `credential` | `smoke-post-deploy.yml` |
| `posthog-prod` | `credential` | Website, app, agentsfleetd, worker, and CLI PostHog env injection |
| `clerk-prod` | `publishable-key` | Fly.io PROD `CLERK_PUBLISHABLE_KEY` |
| `clerk-prod` | `secret-key` | Fly.io PROD `CLERK_SECRET_KEY` |
| `clerk-prod` | `webhook-secret` | Fly.io PROD `CLERK_WEBHOOK_SECRET` (Svix signing key for `/v1/auth/identity-events/clerk`) |
| `clerk-prod` | `jwks-url` | Fly.io PROD `OIDC_JWKS_URL` |
| `clerk-prod` | `issuer` | Fly.io PROD `OIDC_ISSUER` |
| `github-app` | `app-id` | Fly.io PROD + DEV `GITHUB_APP_ID` |
| `github-app` | `private-key` | Fly.io PROD + DEV `GITHUB_APP_PRIVATE_KEY` |
| `encryption-master-key` | `credential` | Fly.io PROD `ENCRYPTION_MASTER_KEY` |
| `auth-session-code-pepper` | `credential` | Fly.io PROD `AUTH_SESSION_CODE_PEPPER` — `agentsfleetd` loads at boot via `src/state/vault.zig`; process fails fast if missing. Used to keyed-HMAC the CLI-login verification code (defeats offline brute-force from a Redis dump). |
| `audit-log-pepper` | `credential` | Fly.io PROD `AUDIT_LOG_PEPPER` — `agentsfleetd` loads at boot; fails fast if missing. Used to keyed-HMAC `session_id` in the `.auth_audit` log scope (pseudonymization across audit events). |
| `planetscale-prod` | `api-connection-string` | Fly.io PROD `DATABASE_URL_API` |
| `planetscale-prod` | `migrator-connection-string` | Fly.io PROD `DATABASE_URL_MIGRATOR` (release migrations) |
| `upstash-prod` | `api-url` | Fly.io PROD `REDIS_URL_API` |
| `grafana-prod` | `otlp-endpoint` | Fly.io PROD `GRAFANA_OTLP_ENDPOINT` (OTLP traces/metrics export) |
| `grafana-prod` | `instance-id` | Fly.io PROD `GRAFANA_OTLP_INSTANCE_ID` |
| `grafana-prod` | `api-key` | Fly.io PROD `GRAFANA_OTLP_API_KEY` |
| `tailscale` | `authkey` | worker node provision |
| `zombie-prod-worker-ant` | `ssh-private-key` | CI → worker deploy SSH |
| `zombie-prod-worker-ant` | `runner-token` | `agentsfleet-runner` daemon auth (admin-minted `agt_r`). Initial value is the placeholder `agt_rFAKE_REPLACE_BEFORE_PROD_WORKER_READY_TRUE`; owned/replaced by `07_runner_bootstrap_prod` once a real token is minted. |
| `zombie-prod-worker-bird` | `ssh-private-key` | CI → worker deploy SSH |
| `discord-ci-webhook` | `credential` | `deploy-dev.yml` + `release.yml` notify |
| `fly-api-token` | `credential` | `release.yml` → `fly deploy --app agentsfleetd-prod` (see M2_002 §2.6) |
| `cloudflare-tunnel-prod` | `credential` | Cloudflare Tunnel credentials for PROD origin shield (see M2_002 §2.4) |

### 1.2 Vault: `ZMB_CD_DEV`

| Item | Field | Used by |
|---|---|---|
| `clerk-dev` | `publishable-key` | Fly.io DEV `CLERK_PUBLISHABLE_KEY` |
| `clerk-dev` | `secret-key` | Fly.io DEV `CLERK_SECRET_KEY` |
| `clerk-dev` | `webhook-secret` | Fly.io DEV `CLERK_WEBHOOK_SECRET` (Svix signing key for `/v1/auth/identity-events/clerk`) |
| `clerk-dev` | `jwks-url` | Fly.io DEV `OIDC_JWKS_URL` |
| `clerk-dev` | `issuer` | Fly.io DEV `OIDC_ISSUER` |
| `github-app` | `app-id` | Fly.io DEV `GITHUB_APP_ID` |
| `github-app` | `private-key` | Fly.io DEV `GITHUB_APP_PRIVATE_KEY` |
| `encryption-master-key` | `credential` | Fly.io DEV `ENCRYPTION_MASTER_KEY` |
| `auth-session-code-pepper` | `credential` | Fly.io DEV `AUTH_SESSION_CODE_PEPPER` — `agentsfleetd` loads at boot via `src/state/vault.zig`; process fails fast if missing. Used to keyed-HMAC the CLI-login verification code (defeats offline brute-force from a Redis dump). |
| `audit-log-pepper` | `credential` | Fly.io DEV `AUDIT_LOG_PEPPER` — `agentsfleetd` loads at boot; fails fast if missing. Used to keyed-HMAC `session_id` in the `.auth_audit` log scope (pseudonymization across audit events). |
| `e2e-fixtures-email/regular` | `email`, `password` | Playwright + Vitest e2e suites under `ui/packages/app/tests/e2e/` and the CLI acceptance suite `cli/test/acceptance/lifecycle-after-login.spec.ts` — regular-tenant-member Clerk DEV identity. |
| `e2e-fixtures-email/admin` | `email`, `password` | Same suites — tenant-admin-role Clerk DEV identity (used by scenarios that require admin permissions). |
| `vercel-api-token` | `credential` | Vercel env var setup |
| `posthog-dev` | `credential` | Website, app, agentsfleetd, worker, and CLI PostHog env injection |
| `planetscale-dev` | `api-connection-string` | Fly.io DEV `DATABASE_URL_API` |
| `planetscale-dev` | `migrator-connection-string` | Fly.io DEV `DATABASE_URL_MIGRATOR` (`agentsfleetd migrate`) |
| `upstash-dev` | `api-url` | Fly.io DEV `REDIS_URL_API` |
| `grafana-dev` | `otlp-endpoint` | Fly.io DEV `GRAFANA_OTLP_ENDPOINT` (OTLP traces/metrics export) |
| `grafana-dev` | `instance-id` | Fly.io DEV `GRAFANA_OTLP_INSTANCE_ID` |
| `grafana-dev` | `api-key` | Fly.io DEV `GRAFANA_OTLP_API_KEY` |
| `zombie-dev-worker-ant` | `runner-token` | `agentsfleet-runner` daemon auth (admin-minted `agt_r`). Initial value is the placeholder `agt_rFAKE_REPLACE_BEFORE_DEV_WORKER_READY_TRUE`; owned/replaced by `06_runner_bootstrap_dev` once a real token is minted. |
| `fly-api-token` | `credential` | `deploy-dev.yml` → `fly deploy --app agentsfleetd-dev` (see M2_002 §2.6) |
| `cloudflare-tunnel-dev` | `credential` | Cloudflare Tunnel credentials for DEV origin shield (see M2_002 §2.4) |

---

## 2.0 Validation Steps (Chronological)

Checks are split into ordered sections under `playbooks/founding/02_preflight/` and executed by `playbooks/founding/02_preflight/00_gate.sh`.

| Section | Script | Purpose | Blocks startup? | Playbook dependency |
|---|---|---|---|---|
| `1` | `playbooks/founding/02_preflight/01_tools_and_auth.sh` | Local prerequisites (`op` binary + 1Password auth/session) | Yes | M1 complete → before any M2 work |
| `2` | `playbooks/founding/02_preflight/02_credentials.sh` | Procurement readiness gate (all required `op://` refs + API/worker/migrator DB role separation + Redis separation) | Yes | Gate for M2_002 infra priming |

Notes:
- `OP_SERVICE_ACCOUNT_TOKEN` is the preferred non-interactive auth for agents/CI.
- `gh` / `glab` auth is reported as advisory in section `1` (non-blocking).
- GitHub PAT is **not** required for this credential gate.

### 2.1 Run the Check

Run from any terminal where `op` is authenticated:

```bash
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"

# Run full chronological gate (section 1 -> 2) for both envs
./playbooks/founding/02_preflight/00_gate.sh

# Optional: be gentler with 1Password API when rate-limited
OP_READ_RETRIES=2 OP_READ_BASE_DELAY_SECONDS=2 ./playbooks/founding/02_preflight/00_gate.sh

# Check a specific env (still runs section 1 -> 2)
ENV=dev  ./playbooks/founding/02_preflight/00_gate.sh
ENV=prod ./playbooks/founding/02_preflight/00_gate.sh

# Run only startup preflight
SECTIONS=1 ./playbooks/founding/02_preflight/00_gate.sh

# Run only procurement readiness gate (after section 1 passes)
SECTIONS=2 ./playbooks/founding/02_preflight/00_gate.sh
```

Works on: local machine, CI runner, agent session, any context with `op` CLI.

### 2.2 Interpret Output

The workflow prints one line per item:

```
✓ op://$VAULT_PROD/cloudflare-api-token/credential
✗ MISSING: op://$VAULT_PROD/discord-ci-webhook/credential
✗ MISSING: op://$VAULT_DEV/planetscale-dev/api-connection-string
✗ MISSING: op://$VAULT_DEV/planetscale-dev/migrator-connection-string
```

For every `✗ MISSING` line: add the item to the vault, re-run.

### 2.3 Connectivity Test

After all items are present, run live connectivity checks. These require `psql`, `docker`, `curl`, and `jq` on `PATH` — the section-1 tools gate (`01_tools_and_auth.sh`) only verifies `op`, so confirm them first (`command -v psql docker curl jq`).

```bash
# Postgres DEV
DB_API=$(op read "op://$VAULT_DEV/planetscale-dev/api-connection-string")
DB_MIGRATOR=$(op read "op://$VAULT_DEV/planetscale-dev/migrator-connection-string")
psql "$DB_API" -c "SELECT 1" && echo "✓ postgres dev api"
psql "$DB_MIGRATOR" -c "SELECT 1" && echo "✓ postgres dev migrator"

# Redis DEV
REDIS_API=$(op read "op://$VAULT_DEV/upstash-dev/api-url")
docker run --rm redis:7-alpine redis-cli -u "$REDIS_API" PING && echo "✓ redis dev api"

# Discord webhook
WEBHOOK=$(op read "op://$VAULT_PROD/discord-ci-webhook/credential")
curl -sf -X POST "$WEBHOOK" \
  -H "Content-Type: application/json" \
  -d '{"content":"✅ credential check passed"}' && echo "✓ discord"
```

---

## 3.0 Acceptance Criteria

- [x] 3.1 `check-credentials.yml` workflow exits 0 — all items present in vaults
- [x] 3.2 Postgres DEV connectivity confirmed (DEV deploy active; `agentsfleetd-dev` running)
- [x] 3.3 Redis DEV connectivity confirmed (DEV deploy active; `agentsfleetd-dev` running)
- [x] 3.4 Discord webhook fires successfully (CI notify jobs active)
- [x] 3.5 No `✗ MISSING` lines in workflow output

Gate: all 3.x must pass before `playbooks/founding/03_priming_infra/001_playbook.md` begins.

---

## 4.0 What to Create in 1Password

Items not yet in the vault that block M2_002. Create these before re-running:

**ZMB_CD_PROD — create these:**

| Item name | Field | How to get the value |
|---|---|---|
| `discord-ci-webhook` | `credential` | Discord → Server Settings → Integrations → Webhooks → New Webhook → Copy URL |
| `posthog-prod` | `credential` | PostHog project API key shared by website, app, agentsfleetd, worker, and CLI |
| `planetscale-prod` | `api-connection-string` | PlanetScale dashboard → create/get `api_runtime` connection string |
| `planetscale-prod` | `migrator-connection-string` | PlanetScale dashboard → create/get `db_migrator` connection string |
| `upstash-prod` | `api-url` | Upstash dashboard → Redis → `agentsfleet-cache` → create/get API role URL (`rediss://...`) |
| `grafana-prod` | `otlp-endpoint` | Grafana Cloud → Stack → OTLP → endpoint URL (`https://otlp-gateway-*.grafana.net/otlp`) |
| `grafana-prod` | `instance-id` | Grafana Cloud → Stack → OTLP → instance ID (numeric) |
| `grafana-prod` | `api-key` | Grafana Cloud → Access Policies → token with `metrics:write` + `traces:write` |
| `tailscale` | `authkey` | Tailscale admin → Settings → Keys → Generate auth key (reusable, no expiry for CI) |
| `zombie-prod-worker-ant` | `ssh-private-key` | Already in vault ✅ — add public key to `~/.ssh/authorized_keys` on the node |
| `zombie-prod-worker-ant` | `runner-token` | Seed with placeholder `agt_rFAKE_REPLACE_BEFORE_PROD_WORKER_READY_TRUE`; replaced by `07_runner_bootstrap_prod` once a real `agt_r` is admin-minted. |
| `zombie-prod-worker-bird` | `ssh-private-key` | Already in vault ✅ — add public key to `~/.ssh/authorized_keys` on the node |

**ZMB_CD_DEV — create these:**

| Item name | Field | How to get the value |
|---|---|---|
| `planetscale-dev` | `api-connection-string` | PlanetScale → `agentsfleet-dev` DB → create/get `api_runtime` connection string |
| `planetscale-dev` | `migrator-connection-string` | PlanetScale → `agentsfleet-dev` DB → create/get `db_migrator` connection string |
| `upstash-dev` | `api-url` | Upstash → Redis → `agentsfleet-dev` → create/get API role URL (`rediss://...`) |
| `grafana-dev` | `otlp-endpoint` | Grafana Cloud → Stack → OTLP → endpoint URL (`https://otlp-gateway-*.grafana.net/otlp`) |
| `grafana-dev` | `instance-id` | Grafana Cloud → Stack → OTLP → instance ID (numeric) |
| `grafana-dev` | `api-key` | Grafana Cloud → Access Policies → token with `metrics:write` + `traces:write` |
| `zombie-dev-worker-ant` | `runner-token` | Seed with placeholder `agt_rFAKE_REPLACE_BEFORE_DEV_WORKER_READY_TRUE`; replaced by `06_runner_bootstrap_dev` once a real `agt_r` is admin-minted. |
| `fly-api-token` | `credential` | `fly tokens create deploy -o agentsfleet` — copy output. Scoped to org, used by CI to deploy. |
| `cloudflare-tunnel-dev` | `credential` | Agent-created: `cloudflared tunnel create agentsfleetd-dev` → base64-encode the credentials JSON → store here (see M2_002 §2.4). |
| `posthog-dev` | `credential` | PostHog project API key shared by website, app, agentsfleetd, worker, and CLI |

**ZMB_CD_PROD — create these (add to existing list):**

| Item name | Field | How to get the value |
|---|---|---|
| `fly-api-token` | `credential` | Same deploy token as DEV if org-scoped, or create a separate one (`fly tokens create deploy -o agentsfleet`) for PROD isolation. |
| `cloudflare-tunnel-prod` | `credential` | Agent-created: `cloudflared tunnel create agentsfleetd-prod` → base64-encode credentials JSON → store here (see M2_002 §2.4). |
