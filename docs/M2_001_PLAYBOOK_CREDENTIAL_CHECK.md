# M2_001: Playbook — Credential Validation

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

Script: `scripts/check-credentials.sh` — runs anywhere `op` CLI is available (local, CI, agent terminal).
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
| `clerk-prod` | `publishable-key` | Railway PROD env var |
| `clerk-prod` | `secret-key` | Railway PROD env var |
| `planetscale-prod` | `connection-string` | Railway PROD `DATABASE_URL` |
| `upstash-prod` | `url` | Railway PROD `REDIS_URL` |
| `tailscale` | `authkey` | OVHCloud worker provision |
| `zombie-worker-ant` | `ssh-private-key` | CI → worker deploy SSH |
| `zombie-worker-bird` | `ssh-private-key` | CI → worker deploy SSH |
| `discord-ci-webhook` | `credential` | `deploy-dev.yml` + `release.yml` notify |

### 1.2 Vault: `ZMB_CD_DEV`

| Item | Field | Used by |
|---|---|---|
| `clerk-dev` | `publishable-key` | Railway DEV env var |
| `clerk-dev` | `secret-key` | Railway DEV env var |
| `vercel-api-token` | `credential` | Vercel env var setup |
| `planetscale-dev` | `connection-string` | Railway DEV `DATABASE_URL` |
| `upstash-dev` | `url` | Railway DEV `REDIS_URL` |

---

## 2.0 Validation Steps

### 2.1 Run the Check

Run from any terminal where `op` is authenticated:

```bash
# Check all vaults
./scripts/check-credentials.sh

# Check a specific vault
ENV=dev  ./scripts/check-credentials.sh
ENV=prod ./scripts/check-credentials.sh
```

Works on: local machine, CI runner, agent session, any context with `op` CLI.

### 2.2 Interpret Output

The workflow prints one line per item:

```
✓ op://ZMB_CD_PROD/cloudflare-api-token/credential
✗ MISSING: op://ZMB_CD_PROD/discord-ci-webhook/credential
✗ MISSING: op://ZMB_CD_DEV/planetscale-dev/connection-string
```

For every `✗ MISSING` line: add the item to the vault, re-run.

### 2.3 Connectivity Test

After all items are present, run live connectivity checks:

```bash
# Postgres DEV
DB=$(op read "op://ZMB_CD_DEV/planetscale-dev/connection-string")
psql "$DB" -c "SELECT 1" && echo "✓ postgres dev"

# Redis DEV
REDIS=$(op read "op://ZMB_CD_DEV/upstash-dev/url")
redis-cli -u "$REDIS" PING && echo "✓ redis dev"

# Discord webhook
WEBHOOK=$(op read "op://ZMB_CD_PROD/discord-ci-webhook/credential")
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
| `planetscale-prod` | `connection-string` | PlanetScale dashboard → your DB → Connect → copy Postgres connection string |
| `upstash-prod` | `url` | Upstash dashboard → Redis → `usezombie-cache` → Details → copy Redis URL (`rediss://...`) |
| `tailscale` | `authkey` | Tailscale admin → Settings → Keys → Generate auth key (reusable, no expiry for CI) |
| `zombie-worker-ant` | `ssh-private-key` | Already in vault ✅ — add public key to `~/.ssh/authorized_keys` on the node |
| `zombie-worker-bird` | `ssh-private-key` | Already in vault ✅ — add public key to `~/.ssh/authorized_keys` on the node |

**ZMB_CD_DEV — create these:**

| Item name | Field | How to get the value |
|---|---|---|
| `planetscale-dev` | `connection-string` | PlanetScale → `usezombie-dev` DB → Connect → copy Postgres connection string |
| `upstash-dev` | `url` | Upstash → Redis → `usezombie-dev` → Details → copy Redis URL (`rediss://...`) |
