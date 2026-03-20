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
        ├── 1.0 Container pipeline (GHCR)
        ├── 2.0 Fly.io — API + Worker services (recommended)
        │     ├── 2.1 Fly apps: zombied-dev, zombied-dev-worker, cloudflared-dev
        │     ├── 2.2 Cloudflare Tunnel (origin shield — no public Fly port)
        │     └── 2.3 Auto-scaling configuration
        ├── 3.0 Data-plane bootstrap (PlanetScale + Upstash)
        └── 4.0 Worker infrastructure (OVHCloud + Tailscale — defer to v2/scale)
            └── Milestone 3 deployment execution:
                ├── docs/M3_001_PLAYBOOK_DEPLOY_DEV.md
                └── docs/M3_002_PLAYBOOK_DEPLOY_PROD.md
```

**Human vs Agent split:**

| Step | Owner | Why |
|------|-------|-----|
| Create Fly.io account, add payment | Human | Requires browser + billing |
| `fly auth login` | Human | Requires browser OAuth |
| Generate deploy token, store in vault | Human | Requires Fly dashboard |
| Create Fly apps, set secrets, deploy | Agent | Fully scriptable via `flyctl` |
| Create Cloudflare Tunnel | Agent | `cloudflared` CLI, credentials to vault |
| Set DNS CNAME records | Agent | Cloudflare API |
| Configure auto-scaling in fly.toml | Agent | Config file + `fly deploy` |
| PlanetScale schema migrations | Agent | `psql` + migration files |
| Upstash stream bootstrap | Agent | `redis-cli` |

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

## 2.0 Fly.io Services

**Owner: Agent** (human does one-time `fly auth login` and stores deploy token — see M1_001 §1.2)

### 2.1 Create Fly Apps (Agent)

```bash
export FLY_API_TOKEN=$(op read "op://ZMB_CD_DEV/fly-api-token/credential")

# Create the three DEV apps
fly apps create zombied-dev         --org <org>
fly apps create zombied-dev-worker  --org <org>
fly apps create cloudflared-dev     --org <org>

# Repeat for PROD
fly apps create zombied-prod         --org <org>
fly apps create zombied-prod-worker  --org <org>
fly apps create cloudflared-prod     --org <org>
```

### 2.2 Set Secrets from 1Password (Agent)

> **Important:** `DATABASE_URL` and `REDIS_URL` are not valid for `zombied serve`. Use role-separated vars. See `docs/CONFIGURATION.md`.

```bash
# DEV API + Worker (same secrets, separate apps)
for APP in zombied-dev zombied-dev-worker; do
  fly secrets set \
    DATABASE_URL_API="$(op read 'op://ZMB_CD_DEV/planetscale-dev/connection-string')" \
    DATABASE_URL_WORKER="$(op read 'op://ZMB_CD_DEV/planetscale-dev/connection-string')" \
    REDIS_URL_API="$(op read 'op://ZMB_CD_DEV/upstash-dev/url')" \
    REDIS_URL_WORKER="$(op read 'op://ZMB_CD_DEV/upstash-dev/url')" \
    ENCRYPTION_MASTER_KEY="$(op read 'op://ZMB_CD_DEV/zombied-local-config/encryption-master-key')" \
    GITHUB_APP_ID="$(op read 'op://ZMB_CD_DEV/github-app/app-id')" \
    GITHUB_APP_PRIVATE_KEY="$(op read 'op://ZMB_CD_DEV/github-app/private-key')" \
    OIDC_PROVIDER=clerk \
    OIDC_JWKS_URL="$(op read 'op://ZMB_CD_DEV/clerk-dev/publishable-key' | python3 -c 'import sys,base64; k=sys.stdin.read().strip().split("_")[2]; print(f\"https://{base64.b64decode(k+\"=\"*4).decode().rstrip(chr(36))}/.well-known/jwks.json\")')" \
    OIDC_ISSUER="$(op read 'op://ZMB_CD_DEV/clerk-dev/hostname' 2>/dev/null || echo 'https://winning-wombat-65.clerk.accounts.dev')" \
    PORT=3000 \
    ENVIRONMENT=dev \
    MIGRATE_ON_START=0 \
    --app "$APP"
done

# PROD — same pattern with ZMB_CD_PROD values
```

### 2.3 Deploy from GHCR (Agent)

```bash
# Deploy API
fly deploy --app zombied-dev \
  --image ghcr.io/usezombie/zombied:dev-latest \
  --regions iad \
  --ha=false   # start with 1 machine, scale after verify

# Deploy Worker (separate process, same image)
fly deploy --app zombied-dev-worker \
  --image ghcr.io/usezombie/zombied:dev-latest \
  --regions iad

# Verify
fly status --app zombied-dev
```

`fly.toml` for the API app (no public port — tunnel is the only ingress):

```toml
app = "zombied-dev"
primary_region = "iad"

[build]
  image = "ghcr.io/usezombie/zombied:dev-latest"

[[vm]]
  size = "shared-cpu-1x"
  memory = "512mb"

# NO [http_service] block — suppresses *.fly.dev public domain entirely.
# All traffic enters via Cloudflare Tunnel (§2.4).
# Internal-only: accessible at zombied-dev.internal:3000 within Fly 6PN.

[metrics]
  port = 9091
  path = "/metrics"
```

`fly.toml` for the Worker app:

```toml
app = "zombied-dev-worker"
primary_region = "iad"

[build]
  image = "ghcr.io/usezombie/zombied:dev-latest"

[[vm]]
  size = "shared-cpu-1x"
  memory = "512mb"

[processes]
  worker = "zombied worker"

# No http_service — workers consume Redis Streams, no inbound HTTP.
```

### 2.4 Cloudflare Tunnel — Origin Shield (Agent)

Tunnel replaces CNAME. All traffic: Cloudflare edge → encrypted tunnel → Fly private network. No public Fly port. No bypass.

```bash
# Create tunnel (run locally, credentials stored in ~/.cloudflared/)
cloudflared tunnel create zombied-dev
# Output: Created tunnel zombied-dev with id <TUNNEL_ID>

# Store credentials in vault
TUNNEL_CREDS=$(cat ~/.cloudflared/<TUNNEL_ID>.json | base64)
op item create --vault ZMB_CD_DEV --title cloudflare-tunnel-dev \
  --category "API Credential" \
  "tunnel-id=<TUNNEL_ID>" \
  "credentials-json-b64=$TUNNEL_CREDS"

# Route tunnel to domain (creates CNAME <TUNNEL_ID>.cfargotunnel.com automatically)
cloudflared tunnel route dns zombied-dev dev.api.usezombie.com

# Repeat for PROD
cloudflared tunnel create zombied-prod
cloudflared tunnel route dns zombied-prod api.usezombie.com
```

`cloudflared` config deployed as a Fly app (`cloudflared-dev`):

```yaml
# config.yml — baked into cloudflared Fly image or mounted as secret
tunnel: <TUNNEL_ID>
credentials-file: /etc/cloudflared/credentials.json

ingress:
  - hostname: dev.api.usezombie.com
    service: http://zombied-dev.internal:3000  # Fly 6PN private DNS
  - service: http_status:404
```

```toml
# fly.toml for cloudflared-dev
app = "cloudflared-dev"
primary_region = "iad"

[[vm]]
  size = "shared-cpu-1x"
  memory = "256mb"

[processes]
  app = "cloudflared tunnel --config /etc/cloudflared/config.yml run"
```

Cloudflare SSL/TLS: **Full (Strict)**. No Transform Rules. No CNAME hack.

### 2.5 Auto-Scaling (Agent)

```bash
# Scale API to 2 machines for HA (both in iad)
fly scale count 2 --app zombied-dev

# Auto-scaling: scale up to 5 on load, never scale below 1
fly autoscale set min=1 max=5 --app zombied-dev

# Workers — scale independently based on queue depth (manual for now)
fly scale count 1 --app zombied-dev-worker
```

For PROD multi-region (future):
```bash
# Add a second region for global HA
fly regions add lhr --app zombied-prod     # London for EU users
fly scale count 2 --app zombied-prod       # 1 machine per region
```

### 2.6 CI Wiring (Agent)

```bash
# Store Fly deploy token in vault
fly tokens create deploy -o <org> --name ci-deploy
op item create --vault ZMB_CD_DEV --title fly-api-token \
  --category "API Credential" "credential=<token>"

# Set GitHub Actions vars
gh variable set FLY_APP_DEV --body "zombied-dev" --repo usezombie/usezombie
gh variable set FLY_APP_DEV_WORKER --body "zombied-dev-worker" --repo usezombie/usezombie
gh variable set FLY_APP_PROD --body "zombied-prod" --repo usezombie/usezombie
```

CI deploy step in `deploy-dev.yml`:
```yaml
- name: Deploy to Fly.io DEV
  run: fly deploy --app ${{ vars.FLY_APP_DEV }} --image ghcr.io/usezombie/zombied:dev-latest --wait-timeout 120
  env:
    FLY_API_TOKEN: ${{ secrets.FLY_API_TOKEN }}
```

### 2.7 Verify

```bash
# Tunnel health
cloudflared tunnel info zombied-dev

# API reachable via Cloudflare (not direct Fly)
curl -sf https://dev.api.usezombie.com/healthz
curl -sf https://dev.api.usezombie.com/readyz | jq '.ready'

# Confirm no direct Fly access (should time out or refuse)
curl -sf https://zombied-dev.fly.dev/healthz  # expected: connection refused / 404

# Fly machine status
fly status --app zombied-dev
fly logs   --app zombied-dev
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
