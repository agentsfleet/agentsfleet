# M2_001: Playbook — Preflight Readiness

**Milestone:** M2
**Workstream:** 001
**Updated:** Mar 19, 2026
**Owner:** Agent
**Prerequisite:** `docs/M1_001_PLAYBOOK_BOOTSTRAP.md` Milestone 1 complete.
**Gate:** M2_002 (PRIMING_INFRA) must not start until every check below passes.

This workstream is the eval/feedback harness for Milestone 2. It validates that every
credential the agent needs is present in the correct 1Password vault and returns a non-empty
value. Run this before any infrastructure step. Fail loud — surface every missing item,
not just the first one.

Script: `scripts/checks/m2_001/run.sh` — milestone/workstream check runner that runs anywhere `op` CLI is available (local, CI, agent terminal).
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
| `clerk-prod` | `publishable-key` | Fly.io PROD `OIDC_JWKS_URL` + `OIDC_ISSUER` |
| `clerk-prod` | `secret-key` | Fly.io PROD env var |
| `github-app` | `app-id` | Fly.io PROD + DEV `GITHUB_APP_ID` |
| `github-app` | `private-key` | Fly.io PROD + DEV `GITHUB_APP_PRIVATE_KEY` |
| `encryption-master-key` | `credential` | Fly.io PROD `ENCRYPTION_MASTER_KEY` |
| `planetscale-prod` | `api-connection-string` | Fly.io PROD `DATABASE_URL_API` |
| `planetscale-prod` | `worker-connection-string` | Fly.io PROD `DATABASE_URL_WORKER` |
| `upstash-prod` | `api-url` | Fly.io PROD `REDIS_URL_API` |
| `upstash-prod` | `worker-url` | Fly.io PROD `REDIS_URL_WORKER` |
| `tailscale` | `authkey` | worker node provision |
| `zombie-prod-worker-ant` | `ssh-private-key` | CI → worker deploy SSH |
| `zombie-prod-worker-bird` | `ssh-private-key` | CI → worker deploy SSH |
| `discord-ci-webhook` | `credential` | `deploy-dev.yml` + `release.yml` notify |
| `fly-api-token` | `credential` | `release.yml` → `fly deploy --app zombied-prod` (see M2_002 §2.6) |
| `cloudflare-tunnel-prod` | `credential` | Cloudflare Tunnel credentials for PROD origin shield (see M2_002 §2.4) |

### 1.2 Vault: `ZMB_CD_DEV`

| Item | Field | Used by |
|---|---|---|
| `clerk-dev` | `publishable-key` | Fly.io DEV `OIDC_JWKS_URL` + `OIDC_ISSUER` |
| `clerk-dev` | `secret-key` | Fly.io DEV env var |
| `github-app` | `app-id` | Fly.io DEV `GITHUB_APP_ID` |
| `github-app` | `private-key` | Fly.io DEV `GITHUB_APP_PRIVATE_KEY` |
| `encryption-master-key` | `credential` | Fly.io DEV `ENCRYPTION_MASTER_KEY` |
| `vercel-api-token` | `credential` | Vercel env var setup |
| `planetscale-dev` | `api-connection-string` | Fly.io DEV `DATABASE_URL_API` |
| `planetscale-dev` | `worker-connection-string` | Fly.io DEV `DATABASE_URL_WORKER` |
| `upstash-dev` | `api-url` | Fly.io DEV `REDIS_URL_API` |
| `upstash-dev` | `worker-url` | Fly.io DEV `REDIS_URL_WORKER` |
| `fly-api-token` | `credential` | `deploy-dev.yml` → `fly deploy --app zombied-dev` (see M2_002 §2.6) |
| `cloudflare-tunnel-dev` | `credential` | Cloudflare Tunnel credentials for DEV origin shield (see M2_002 §2.4) |

---

## 2.0 Validation Steps (Chronological)

Checks are split into ordered sections under `scripts/checks/m2_001/` and executed by `scripts/checks/m2_001/run.sh`.

| Section | Script | Purpose | Blocks startup? | Playbook dependency |
|---|---|---|---|---|
| `1` | `scripts/checks/m2_001/section-1-preflight.sh` | Local prerequisites (`op` binary + 1Password auth/session) | Yes | M1 complete → before any M2 work |
| `2` | `scripts/checks/m2_001/section-2-procurement-readiness-gate.sh` | Procurement readiness gate (all required `op://` refs + API/worker DB/Redis separation) | Yes | Gate for M2_002 infra priming |

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
./scripts/checks/m2_001/run.sh

# Check a specific env (still runs section 1 -> 2)
ENV=dev  ./scripts/checks/m2_001/run.sh
ENV=prod ./scripts/checks/m2_001/run.sh

# Run only startup preflight
SECTIONS=1 ./scripts/checks/m2_001/run.sh

# Run only procurement readiness gate (after section 1 passes)
SECTIONS=2 ./scripts/checks/m2_001/run.sh
```

Works on: local machine, CI runner, agent session, any context with `op` CLI.

### 2.2 Interpret Output

The workflow prints one line per item:

```
✓ op://$VAULT_PROD/cloudflare-api-token/credential
✗ MISSING: op://$VAULT_PROD/discord-ci-webhook/credential
✗ MISSING: op://$VAULT_DEV/planetscale-dev/api-connection-string
✗ MISSING: op://$VAULT_DEV/planetscale-dev/worker-connection-string
```

For every `✗ MISSING` line: add the item to the vault, re-run.

### 2.3 Connectivity Test

After all items are present, run live connectivity checks:

```bash
# Postgres DEV
DB_API=$(op read "op://$VAULT_DEV/planetscale-dev/api-connection-string")
DB_WORKER=$(op read "op://$VAULT_DEV/planetscale-dev/worker-connection-string")
psql "$DB_API" -c "SELECT 1" && echo "✓ postgres dev api"
psql "$DB_WORKER" -c "SELECT 1" && echo "✓ postgres dev worker"

# Redis DEV
REDIS_API=$(op read "op://$VAULT_DEV/upstash-dev/api-url")
REDIS_WORKER=$(op read "op://$VAULT_DEV/upstash-dev/worker-url")
docker run --rm redis:7-alpine redis-cli -u "$REDIS_API" PING && echo "✓ redis dev api"
docker run --rm redis:7-alpine redis-cli -u "$REDIS_WORKER" PING && echo "✓ redis dev worker"

# Discord webhook
WEBHOOK=$(op read "op://$VAULT_PROD/discord-ci-webhook/credential")
curl -sf -X POST "$WEBHOOK" \
  -H "Content-Type: application/json" \
  -d '{"content":"✅ credential check passed"}' && echo "✓ discord"
```

---

## 3.0 Acceptance Criteria

- [ ] 3.1 `check-credentials.yml` workflow exits 0 — all items present in vaults
- [ ] 3.2 Postgres DEV connectivity confirmed
- [ ] 3.3 Redis DEV connectivity confirmed
- [ ] 3.4 Discord webhook fires successfully
- [ ] 3.5 No `✗ MISSING` lines in workflow output

Gate: all 3.x must pass before `M2_002_PLAYBOOK_PRIMING_INFRA.md` begins.

---

## 4.0 What to Create in 1Password

Items not yet in the vault that block M2_002. Create these before re-running:

**ZMB_CD_PROD — create these:**

| Item name | Field | How to get the value |
|---|---|---|
| `discord-ci-webhook` | `credential` | Discord → Server Settings → Integrations → Webhooks → New Webhook → Copy URL |
| `planetscale-prod` | `api-connection-string` | PlanetScale dashboard → create/get `api_accessor` connection string |
| `planetscale-prod` | `worker-connection-string` | PlanetScale dashboard → create/get `worker_accessor` connection string |
| `upstash-prod` | `api-url` | Upstash dashboard → Redis → `usezombie-cache` → create/get API role URL (`rediss://...`) |
| `upstash-prod` | `worker-url` | Upstash dashboard → Redis → `usezombie-cache` → create/get worker role URL (`rediss://...`) |
| `tailscale` | `authkey` | Tailscale admin → Settings → Keys → Generate auth key (reusable, no expiry for CI) |
| `zombie-prod-worker-ant` | `ssh-private-key` | Already in vault ✅ — add public key to `~/.ssh/authorized_keys` on the node |
| `zombie-prod-worker-bird` | `ssh-private-key` | Already in vault ✅ — add public key to `~/.ssh/authorized_keys` on the node |

**ZMB_CD_DEV — create these:**

| Item name | Field | How to get the value |
|---|---|---|
| `planetscale-dev` | `api-connection-string` | PlanetScale → `usezombie-dev` DB → create/get `api_accessor` connection string |
| `planetscale-dev` | `worker-connection-string` | PlanetScale → `usezombie-dev` DB → create/get `worker_accessor` connection string |
| `upstash-dev` | `api-url` | Upstash → Redis → `usezombie-dev` → create/get API role URL (`rediss://...`) |
| `upstash-dev` | `worker-url` | Upstash → Redis → `usezombie-dev` → create/get worker role URL (`rediss://...`) |
| `fly-api-token` | `credential` | `fly tokens create deploy -o <org>` — copy output. Scoped to org, used by CI to deploy. |
| `cloudflare-tunnel-dev` | `credential` | Agent-created: `cloudflared tunnel create zombied-dev` → base64-encode the credentials JSON → store here (see M2_002 §2.4). |

**ZMB_CD_PROD — create these (add to existing list):**

| Item name | Field | How to get the value |
|---|---|---|
| `fly-api-token` | `credential` | Same deploy token as DEV if org-scoped, or create a separate one for PROD isolation. |
| `cloudflare-tunnel-prod` | `credential` | Agent-created: `cloudflared tunnel create zombied-prod` → base64-encode credentials JSON → store here (see M2_002 §2.4). |
