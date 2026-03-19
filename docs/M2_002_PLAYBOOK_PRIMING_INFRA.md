# M2_001: Playbook — Infrastructure Priming

**Milestone:** M2
**Workstream:** 001
**Updated:** Mar 19, 2026
**Owner:** Agent
**Prerequisite:** `docs/M1_001_PLAYBOOK_BOOTSTRAP.md` Milestone 1 complete — vaults created, GitHub Secrets set, API keys stored in 1Password.

Reusable across startups. Replace `ZMB` vault prefix and service names per project.

---

## Sequence Overview

```
Milestone 1 (M1_001_PLAYBOOK_BOOTSTRAP.md) — human + agent bootstrap
    └── Milestone 2 (this doc) — agent infra priming
        ├── 1.0 Container pipeline
        ├── 2.0 Railway (DEV + PROD services)
        ├── 3.0 Data-plane bootstrap
        └── 4.0 Worker infrastructure (OVHCloud + Tailscale)
            └── Milestone 3 deployment execution:
                ├── docs/M3_001_PLAYBOOK_DEPLOY_DEV.md
                └── docs/M3_002_PLAYBOOK_DEPLOY_PROD.md
```

---

## 1.0 Container Pipeline

**Goal:** GHCR image push works end-to-end.

Already wired in `release.yml` — agent verifies, does not re-create.

**Verify:**
```bash
# Confirm Dockerfile uses binary-copy model (no zig build inside Docker)
grep "COPY dist/zombied" Dockerfile

# Confirm docker job depends on binaries job
grep "needs:.*binaries" .github/workflows/release.yml
```

**Production image:** `ghcr.io/usezombie/zombied:{version}` and `ghcr.io/usezombie/zombied:latest`
**Dev image:** `ghcr.io/usezombie/zombied:dev-latest` (built on every main push via `deploy-dev.yml`)

---

## 2.0 Railway Services

### 2.1 Connect Railway to GitHub Repo

Human does once in Railway dashboard: New Project → Deploy from GitHub → select repo.

**DEV service:** watches `main` branch — auto-deploys on every push — runs `zombied serve`.
**PROD service:** watches `ghcr.io/usezombie/zombied:latest` — deploys when a new release tag is pushed.

> GitHub integration is preferred over deploy hooks — no secret webhook URLs to manage.

### 2.2 Set Railway Env Vars

Agent reads from 1Password and sets via `railway variables set` or Railway dashboard:

**DEV service** (reads from `$VAULT_DEV`):
```bash
DATABASE_URL     = op://$VAULT_DEV/planetscale-dev/connection-string
REDIS_URL        = op://$VAULT_DEV/upstash-dev/url
CLERK_SECRET_KEY = op://$VAULT_DEV/clerk-dev/secret-key
PORT             = 3000
MIGRATE_ON_START = 0
ENVIRONMENT      = dev
```

**PROD service** (reads from `$VAULT_PROD`):
```bash
DATABASE_URL     = op://$VAULT_PROD/planetscale-prod/connection-string
REDIS_URL        = op://$VAULT_PROD/upstash-prod/url
CLERK_SECRET_KEY = op://$VAULT_PROD/clerk-prod/secret-key
PORT             = 3000
MIGRATE_ON_START = 0
ENVIRONMENT      = prod
```

Full env var contract: `docs/CONFIGURATION.md`.

### 2.3 Verify Railway Deploy

```bash
curl -sf https://dev.api.usezombie.com/healthz
curl -sf https://dev.api.usezombie.com/readyz | jq '.ready'
```

---

## 3.0 Data-Plane Bootstrap

### 3.1 Postgres — Roles in Migrations

Roles (`api_accessor`, `worker_accessor`, `callback_accessor`, `vault_accessor`) are already defined in `schema/002_vault_schema.sql` with `IF NOT EXISTS` guards — idempotent. All grants across tables are in subsequent migration files. Run all migrations in order:

```bash
DATABASE_URL=$(op read "op://$VAULT_DEV/planetscale-dev/connection-string")
for f in schema/*.sql; do
  echo "applying $f..."
  psql "$DATABASE_URL" -f "$f"
done
```

Role contract (`schema/002_vault_schema.sql`):
- `api_accessor` — read/write on public tables, no access to `vault.secrets`
- `worker_accessor` — inherits `api_accessor`, read/write on `vault.secrets`
- `callback_accessor` — inherits `vault_accessor`, write on callback table

### 3.2 Redis — Stream Bootstrap (Upstash)

Redis is hosted on Upstash (DEV and PROD). ACL is managed via Upstash dashboard — no custom ACL commands needed.

Stream setup — run once per environment:

```bash
REDIS_URL=$(op read "op://$VAULT_DEV/upstash-dev/url")
redis-cli -u "$REDIS_URL" XGROUP CREATE run_queue workers 0 MKSTREAM
```

For PROD, swap `$VAULT_DEV/upstash-dev` for `$VAULT_PROD/upstash-prod`.

For local docker-compose Redis, static credentials are configured in `docker-compose.yml`.

---

## 4.0 Worker Infrastructure (OVHCloud + Tailscale)

**DEV:** No dedicated workers — workers run as a local process. Skip for DEV.

**PROD only:**

Worker naming: alphabetical animals (`zombie-worker-ant`, `zombie-worker-bird`, ...).

### 4.1 Human: Tailscale Setup

Tailscale creates a private mesh network (tailnet) so workers have no public SSH.

**One-time account setup (human):**

1. Create a Tailscale account at [tailscale.com](https://login.tailscale.com/start)
2. In admin console: Settings → Keys → Generate auth key
   - Enable **Reusable** (CI will use it for multiple nodes)
   - Optionally enable **Ephemeral** (nodes auto-expire when offline)
3. Store the auth key: `$VAULT_PROD/tailscale/authkey` in 1Password ✅ (already done)

**Join your Mac to the tailnet (human, once):**

```bash
# Start the daemon (requires sudo)
sudo tailscaled &

# Join using the vault key (VAULT_PROD defaults to ZMB_CD_PROD)
tailscale up --authkey "$(op read "op://$VAULT_PROD/tailscale/authkey")"

# Verify
tailscale status
```

After this, your Mac can reach any worker node by its Tailscale hostname (e.g. `ssh zombie-worker-ant`).

### 4.2 Human: Provision OVHCloud Bare-Metal

1. Order bare-metal nodes from OVHCloud (Beauharnois CA)
2. Install Debian Trixie, apply worker security baseline:
   - Tailnet-only SSH access
   - Public SSH disabled
   - Node-scoped deploy key in `~/.ssh/authorized_keys`
3. Name each node: `zombie-worker-ant`, `zombie-worker-bird`, ...

### 4.3 Human: Store Worker SSH Keys

Each node's SSH private key is stored in its own vault item (`zombie-worker-ant/ssh-private-key`, `zombie-worker-bird/ssh-private-key`). ✅ Already done in `ZMB_CD_PROD`.

Add the corresponding public key to `~/.ssh/authorized_keys` on each node so CI can SSH in.

### 4.4 Agent: Join Workers to Tailnet

Run once per node at provision time:

```bash
TAILSCALE_AUTHKEY=$(op read "op://$VAULT_PROD/tailscale/authkey")
curl -fsSL https://tailscale.com/install.sh | sh
tailscale up --authkey "$TAILSCALE_AUTHKEY" --hostname zombie-worker-ant
```

Repeat for each node, changing the hostname.

### 4.5 Agent: Deploy zombied Worker to PROD

Each node's SSH key is stored in its own vault item:

```bash
for node in zombie-worker-ant zombie-worker-bird; do
  KEY=$(op read "op://$VAULT_PROD/$node/ssh-private-key")
  ssh -i <(echo "$KEY") "$node" "cd /opt/zombie && ./deploy.sh"
done
```

`deploy.sh` on each node: pulls `ghcr.io/usezombie/zombied:latest`, restarts `zombied worker`.

---

## 5.0 Handoff: DEV Deployment Execution (M3_001)

After M2 infra priming is complete, execute DEV rollout using:

- `docs/M3_001_PLAYBOOK_DEPLOY_DEV.md`

Do not duplicate DEV deploy execution here.

---

## 6.0 Handoff: PROD Deployment Execution (M3_002)

After DEV rollout is green (M3_001), execute PROD rollout using:

- `docs/M3_002_PLAYBOOK_DEPLOY_PROD.md`

Do not duplicate PROD deploy execution here.

---

## 7.0 Reuse for a New Startup

1. Replace `ZMB` vault prefix with `<PROJECT>` everywhere
2. Replace service names (`usezombie`, `zombied`, etc.)
3. Replace domains (`usezombie.com`, `api.usezombie.com`, etc.)
4. Sections 1.0–6.0 are identical — this doc is the full execution playbook

**Pattern:** `M1_001_PLAYBOOK_BOOTSTRAP.md` = human identity + agent key storage. `M2_002_PLAYBOOK_PRIMING_INFRA.md` = agent infrastructure execution.
