# M2_001: Playbook — Infrastructure Priming

**Milestone:** M2
**Workstream:** 001
**Updated:** Mar 19, 2026
**Owner:** Agent
**Prerequisite:** `playbooks/001_bootstrap/001_playbook.md` complete — vaults created, GitHub Secrets set, API keys stored in 1Password.

Reusable across startups. Replace `ZMB` vault prefix and service names per project.

Credential gate before this playbook:

```bash
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"

# startup preflight (M2_001 section 1)
SECTIONS=1 ./playbooks/gates/m2_001/run.sh

# procurement readiness gate (M2_001 section 2, must pass)
SECTIONS=2 ./playbooks/gates/m2_001/run.sh
```

---

## Sequence Overview

```
Bootstrap (`playbooks/001_bootstrap/001_playbook.md`) — human + agent bootstrap
    └── Milestone 2 (this doc) — agent infra priming
        ├── 1.0 Container pipeline (GHCR)
        ├── 2.0 Fly.io — API + Worker services (recommended)
        │     ├── 2.1 Fly apps: zombied-dev, zombied-dev-worker, cloudflared-dev
        │     ├── 2.2 Cloudflare Tunnel (origin shield — no public Fly port)
        │     └── 2.3 Auto-scaling configuration
        ├── 3.0 Data-plane bootstrap (PlanetScale + Upstash)
        └── 4.0 Worker infrastructure (OVHCloud + Tailscale — defer to v2/scale)
            └── Milestone 3 deployment execution:
                ├── playbooks/004_deploy_dev/001_playbook.md
                └── playbooks/005_deploy_prod/001_playbook.md
```

**Human vs Agent split:**

| Step | Owner | Why |
|------|-------|-----|
| Create Fly.io account, add payment | Human | Requires browser + billing |
| `fly auth login` | Human | Requires browser OAuth |
| Generate deploy token, store in vault | Human | Requires Fly dashboard |
| Create Fly apps, set secrets, deploy | Agent | Fully scriptable via `flyctl` |
| `cloudflared tunnel login` | Human | Requires browser OAuth (one-time per machine) |
| Create Cloudflare Tunnel, store credentials, route DNS | Agent | `cloudflared tunnel create/route dns`, credentials to vault |
| Deploy `cloudflared-dev` Fly app | Human | One-time infra bootstrap — `fly deploy --app cloudflared-dev`. Not CI-driven; only redeploy if tunnel config changes. |
| Set DNS CNAME records | Agent | Cloudflare API |
| Configure auto-scaling in fly.toml | Agent | Config file + `fly deploy` |
| PlanetScale schema migrations | Agent | `psql` + migration files |
| Upstash stream bootstrap | Agent | `docker run --rm redis:7-alpine redis-cli` |

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
export FLY_API_TOKEN=$(op read "op://$VAULT_DEV/fly-api-token/credential")

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
    DATABASE_URL_API="$(op read 'op://$VAULT_DEV/planetscale-dev/api-connection-string')" \
    DATABASE_URL_WORKER="$(op read 'op://$VAULT_DEV/planetscale-dev/worker-connection-string')" \
    DATABASE_URL_MIGRATOR="$(op read 'op://$VAULT_DEV/planetscale-dev/migrator-connection-string')" \
    REDIS_URL_API="$(op read 'op://$VAULT_DEV/upstash-dev/api-url')" \
    REDIS_URL_WORKER="$(op read 'op://$VAULT_DEV/upstash-dev/worker-url')" \
    ENCRYPTION_MASTER_KEY="$(op read 'op://$VAULT_DEV/encryption-master-key/credential')" \
    GITHUB_APP_ID="$(op read 'op://$VAULT_DEV/github-app/app-id')" \
    GITHUB_APP_PRIVATE_KEY="$(op read 'op://$VAULT_DEV/github-app/private-key')" \
    POSTHOG_API_KEY="$(op read 'op://$VAULT_DEV/posthog-dev/credential')" \
    GRAFANA_OTLP_ENDPOINT="$(op read 'op://$VAULT_DEV/grafana-dev/otlp-endpoint')" \
    GRAFANA_OTLP_INSTANCE_ID="$(op read 'op://$VAULT_DEV/grafana-dev/instance-id')" \
    GRAFANA_OTLP_API_KEY="$(op read 'op://$VAULT_DEV/grafana-dev/api-key')" \
    OIDC_PROVIDER=clerk \
    OIDC_JWKS_URL="$(op read 'op://$VAULT_DEV/clerk-dev/jwks-url')" \
    OIDC_ISSUER="$(op read 'op://$VAULT_DEV/clerk-dev/issuer')" \
    PORT=3000 \
    ENVIRONMENT=dev \
    MIGRATE_ON_START=0 \
    --app "$APP"
done

# PROD — same pattern with $VAULT_PROD values
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
op item create --vault "$VAULT_DEV" --title cloudflare-tunnel-dev \
  --category "API Credential" \
  "tunnel-id=<TUNNEL_ID>" \
  "credentials-json-b64=$TUNNEL_CREDS"

# Route tunnel to domain (creates CNAME <TUNNEL_ID>.cfargotunnel.com automatically)
cloudflared tunnel route dns zombied-dev api-dev.usezombie.com

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
  - hostname: api-dev.usezombie.com
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

**Deploy cloudflared-dev (Human — one-time):**

```bash
export FLY_API_TOKEN=$(op read "op://$VAULT_DEV/fly-api-token/credential")
fly deploy --app cloudflared-dev \
  --config deploy/fly/cloudflared-dev/fly.toml \
  --wait-timeout 120
```

This is infrastructure, not application code. Do not add to CI. Only redeploy if `deploy/fly/cloudflared-dev/config.yml` changes.

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
op item create --vault "$VAULT_DEV" --title fly-api-token \
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
curl -sf https://api-dev.usezombie.com/healthz
curl -sf https://api-dev.usezombie.com/readyz | jq '.ready'

# Confirm no direct Fly access (should time out or refuse)
curl -sf https://zombied-dev.fly.dev/healthz  # expected: connection refused / 404

# Fly machine status
fly status --app zombied-dev
fly logs   --app zombied-dev
```

---

## 3.0 Data-Plane Bootstrap

### 3.1 Postgres — Roles in Migrations

Roles (`db_migrator`, `api_runtime`, `worker_runtime`, `ops_readonly_human`, `ops_readonly_agent`) are defined in `schema/002_vault_schema.sql` with `IF NOT EXISTS` guards — idempotent. All grants across tables are in subsequent migration files. Run all migrations in order:

```bash
DATABASE_URL=$(op read "op://$VAULT_DEV/planetscale-dev/migrator-connection-string")
for f in schema/*.sql; do
  echo "applying $f..."
  psql "$DATABASE_URL" -f "$f"
done
```

Role contract (`schema/002_vault_schema.sql`):
- `db_migrator` — DDL authority for `core/agent/billing/vault/audit/ops_ro`
- `api_runtime` — runtime DML on API-owned tables only
- `worker_runtime` — runtime DML for worker execution paths
- `ops_readonly_human`, `ops_readonly_agent` — read-only access via `ops_ro` views only

### 3.2 Redis — Stream Bootstrap (Upstash)

Redis is hosted on Upstash (DEV and PROD). ACL is managed via Upstash dashboard — no custom ACL commands needed.

Stream setup — run once per environment:

```bash
REDIS_URL=$(op read "op://$VAULT_DEV/upstash-dev/api-url")
docker run --rm redis:7-alpine redis-cli -u "$REDIS_URL" XGROUP CREATE run_queue workers 0 MKSTREAM
```

For PROD, swap `$VAULT_DEV/upstash-dev` for `$VAULT_PROD/upstash-prod`.

For local docker-compose Redis, static credentials are configured in `docker-compose.yml`.

---

## 4.0 Worker Infrastructure + Deployment Handoff

Worker bootstrap, CI deployment handoff, and startup reuse guidance moved to:

- `playbooks/003_priming_infra/002_workers_and_handoff.md`
- `playbooks/006_worker_bootstrap_dev/001_playbook.md`
- `playbooks/007_worker_bootstrap_prod/001_playbook.md`
- `playbooks/004_deploy_dev/001_playbook.md`
- `playbooks/005_deploy_prod/001_playbook.md`

Keep this file focused on infra priming (container, Fly, Cloudflare tunnel, data plane) so it stays under the repository line-limit gate.
