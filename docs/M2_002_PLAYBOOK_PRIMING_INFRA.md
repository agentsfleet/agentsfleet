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

### 2.1 Connect Railway to GHCR Image

Human does once in Railway dashboard per service. **Do not use "Deploy from GitHub source"** — this project uses pre-built binaries packaged into Docker images and pushed to GHCR. Railway must pull from the registry, not build from source.

**DEV service:**
1. Railway dashboard → New Service → **Deploy from Docker Image**
2. Image: `ghcr.io/<org>/<service>:dev-latest`
3. Generate a Railway deploy hook URL for this service (Service → Settings → Deploy Hook → Generate)
4. Store the hook URL: `op://ZMB_CD_DEV/railway-deploy-hook-dev/credential`

**PROD service:**
1. Railway dashboard → New Service → **Deploy from Docker Image**
2. Image: `ghcr.io/<org>/<service>:latest`
3. Generate a Railway deploy hook URL for this service
4. Store the hook URL: `op://ZMB_CD_PROD/railway-deploy-hook-prod/credential`

> CI calls the deploy hook after pushing each image. Railway pulls the new image and restarts the service. No source build happens on Railway.

### 2.2 Set Railway Env Vars

Agent reads from 1Password and sets via Railway dashboard or `railway variables set`.

> **Important:** `DATABASE_URL` and `REDIS_URL` are **not** valid for `zombied serve`. Use the role-separated vars below. See `docs/CONFIGURATION.md` for the full contract.

**DEV service** — set these in Railway DEV service environment:

```
# Storage — role-separated (required)
DATABASE_URL_API     = <planetscale-dev connection-string>
DATABASE_URL_WORKER  = <planetscale-dev connection-string>   # same DB, different role in practice

REDIS_URL_API        = <upstash-dev url>
REDIS_URL_WORKER     = <upstash-dev url>

# Secrets (required)
ENCRYPTION_MASTER_KEY = <encryption-master-key credential from ZMB_CD_DEV>

# GitHub App (required)
GITHUB_APP_ID          = <github-app app-id from ZMB_CD_DEV>
GITHUB_APP_PRIVATE_KEY = <github-app private-key from ZMB_CD_DEV>

# Auth (required — Clerk OIDC)
OIDC_PROVIDER  = clerk
OIDC_JWKS_URL  = https://<clerk-dev-domain>/.well-known/jwks.json
OIDC_ISSUER    = https://<clerk-dev-domain>

# Server
PORT             = 3000
MIGRATE_ON_START = 0
ENVIRONMENT      = dev
```

**PROD service** — set these in Railway PROD service environment:

```
# Storage — role-separated (required)
DATABASE_URL_API     = <planetscale-prod connection-string>
DATABASE_URL_WORKER  = <planetscale-prod connection-string>

REDIS_URL_API        = <upstash-prod url>
REDIS_URL_WORKER     = <upstash-prod url>

# Secrets (required)
ENCRYPTION_MASTER_KEY = <encryption-master-key credential from ZMB_CD_PROD>

# GitHub App (required)
GITHUB_APP_ID          = <github-app app-id from ZMB_CD_PROD>
GITHUB_APP_PRIVATE_KEY = <github-app private-key from ZMB_CD_PROD>

# Auth (required — Clerk OIDC)
OIDC_PROVIDER  = clerk
OIDC_JWKS_URL  = https://<clerk-prod-domain>/.well-known/jwks.json
OIDC_ISSUER    = https://<clerk-prod-domain>

# Server
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

### 4.5 Human: Install Docker on Each Worker Node

SSH into each node and install Docker (one-time):

```bash
# On each worker node (Debian Trixie)
apt-get update
apt-get install -y docker.io
systemctl enable docker
systemctl start docker

# Verify
docker --version
```

Also authenticate Docker to GHCR on each node so `docker pull` succeeds:

```bash
# Generate a GitHub PAT with read:packages scope (human does this once)
# Then on each node:
echo "<GITHUB_PAT>" | docker login ghcr.io -u <github-org> --password-stdin
```

Store the GHCR pull token in the vault: `op://ZMB_CD_PROD/ghcr-pull-token/credential`.

### 4.6 Human: Bootstrap /opt/zombie/ on Each Worker Node

Run once per node:

```bash
mkdir -p /opt/zombie
chown deploy-user:deploy-user /opt/zombie
```

Create `/opt/zombie/deploy.sh` on each node:

```bash
cat > /opt/zombie/deploy.sh << 'EOF'
#!/usr/bin/env bash
set -euo pipefail

IMAGE="ghcr.io/<org>/<service>:latest"

echo "Pulling $IMAGE..."
docker pull "$IMAGE"

echo "Restarting zombied-worker..."
docker stop zombied-worker 2>/dev/null || true
docker rm   zombied-worker 2>/dev/null || true

docker run -d \
  --name zombied-worker \
  --restart unless-stopped \
  --env-file /opt/zombie/.env \
  "$IMAGE" \
  zombied worker

echo "Done. Container status:"
docker ps --filter name=zombied-worker
EOF
chmod +x /opt/zombie/deploy.sh
```

Create `/opt/zombie/.env` on each node with the required runtime env vars (see §2.2 PROD vars — same set, no PORT/MIGRATE_ON_START needed for worker-only nodes).

> `/opt/zombie/.env` contains secrets — set permissions to `600` and owned by `deploy-user` only.

```bash
chmod 600 /opt/zombie/.env
```

### 4.7 Agent: Verify deploy.sh Before First Release

Before cutting the first release tag, SSH into each node and do a dry run:

```bash
for node in zombie-worker-ant zombie-worker-bird; do
  KEY=$(op read "op://$VAULT_PROD/$node/ssh-private-key")
  ssh -i <(echo "$KEY") "$node" "ls -la /opt/zombie/deploy.sh && /opt/zombie/deploy.sh"
done
```

This confirms Docker auth, image pull, and container restart work before CI depends on it.

### 4.8 Agent: Run Deploy via CI (Normal Path)

After bootstrap, all subsequent deploys go through CI (`release.yml`):

```bash
for node in zombie-worker-ant zombie-worker-bird; do
  KEY=$(op read "op://$VAULT_PROD/$node/ssh-private-key")
  ssh -i <(echo "$KEY") "$node" "cd /opt/zombie && ./deploy.sh"
done
```

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
