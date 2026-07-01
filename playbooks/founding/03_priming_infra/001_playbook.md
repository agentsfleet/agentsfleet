# M2_001: Playbook — Infrastructure Priming

**Milestone:** M2
**Workstream:** 001
**Updated:** Mar 19, 2026
**Owner:** Agent
**Prerequisite:** `playbooks/founding/01_bootstrap/001_playbook.md` complete — vaults created, GitHub Secrets set, API keys stored in 1Password.

Reusable across startups. Replace `ZMB` vault prefix and service names per project.

Credential gate before this playbook:

```bash
export VAULT_DEV="${VAULT_DEV:-ZMB_CD_DEV}"
export VAULT_PROD="${VAULT_PROD:-ZMB_CD_PROD}"

# startup preflight (M2_001 section 1)
SECTIONS=1 ./playbooks/founding/02_preflight/00_gate.sh

# procurement readiness gate (M2_001 section 2, must pass)
SECTIONS=2 ./playbooks/founding/02_preflight/00_gate.sh
```

---

## Sequence Overview

```
Bootstrap (`playbooks/founding/01_bootstrap/001_playbook.md`) — human + agent bootstrap
    └── Milestone 2 (this doc) — agent infra priming
        ├── 1.0 Container pipeline (GHCR)
        ├── 2.0 Fly.io — API service (recommended)
        │     ├── 2.1 Fly apps: agentsfleetd-dev, cloudflared-dev
        │     ├── 2.2 Cloudflare Tunnel (origin shield — no public Fly port)
        │     └── 2.3 Auto-scaling configuration
        ├── 3.0 Data-plane bootstrap (PlanetScale + Upstash)
        └── 4.0 Worker infrastructure (OVHCloud + Tailscale — defer to v2/scale)
            └── Milestone 3 deployment execution:
                ├── playbooks/founding/04_deploy_dev/001_playbook.md
                └── playbooks/founding/05_deploy_prod/001_playbook.md
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

---

## 1.0 Container Pipeline

**Goal:** GHCR image push works end-to-end.

Already wired in `release.yml` — agent verifies, does not re-create.

**Verify:**
```bash
# Confirm Dockerfile uses binary-copy model (no zig build inside Docker)
grep "COPY dist/agentsfleetd" Dockerfile

# Confirm docker job depends on binaries job
grep "needs:.*binaries" .github/workflows/release.yml
```

**Production image:** `ghcr.io/agentsfleet/agentsfleetd:{version}` and `ghcr.io/agentsfleet/agentsfleetd:latest`
**Dev image:** `ghcr.io/agentsfleet/agentsfleetd:dev-latest` (built on every main push via `deploy-dev.yml`)

---

## 2.0 Fly.io Services

**Owner: Agent** (human does one-time `fly auth login` and stores deploy token — see M1_001 §1.2)

### 2.1 Create Fly Apps (Agent)

```bash
export FLY_API_TOKEN=$(op read "op://$VAULT_DEV/fly-api-token/credential")

# Create the two DEV apps (API + tunnel connector)
fly apps create agentsfleetd-dev      --org agentsfleet
fly apps create cloudflared-dev  --org agentsfleet

# Repeat for PROD
fly apps create agentsfleetd-prod      --org agentsfleet
fly apps create cloudflared-prod  --org agentsfleet
```

### 2.2 Set Secrets from 1Password (Agent)

> **Important:** `DATABASE_URL` and `REDIS_URL` are not valid for `agentsfleetd serve`. Use role-separated vars. See `docs/CONFIGURATION.md`.

```bash
# DEV API
for APP in agentsfleetd-dev; do
  fly secrets set \
    DATABASE_URL_API="$(op read 'op://$VAULT_DEV/planetscale-dev/api-connection-string')" \
    DATABASE_URL_MIGRATOR="$(op read 'op://$VAULT_DEV/planetscale-dev/migrator-connection-string')" \
    REDIS_URL_API="$(op read 'op://$VAULT_DEV/upstash-dev/api-url')" \
    ENCRYPTION_MASTER_KEY="$(op read 'op://$VAULT_DEV/encryption-master-key/credential')" \
    GITHUB_APP_ID="$(op read 'op://$VAULT_DEV/github-app/app-id')" \
    GITHUB_APP_PRIVATE_KEY="$(op read 'op://$VAULT_DEV/github-app/private-key')" \
    POSTHOG_API_KEY="$(op read 'op://$VAULT_DEV/posthog-dev/credential')" \
    GRAFANA_OTLP_ENDPOINT="$(op read 'op://$VAULT_DEV/grafana-dev/otlp-endpoint')" \
    GRAFANA_OTLP_INSTANCE_ID="$(op read 'op://$VAULT_DEV/grafana-dev/instance-id')" \
    GRAFANA_OTLP_API_KEY="$(op read 'op://$VAULT_DEV/grafana-dev/api-key')" \
    OIDC_PROVIDER=clerk \
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
fly deploy --app agentsfleetd-dev \
  --image ghcr.io/agentsfleet/agentsfleetd:dev-latest \
  --regions iad \
  --ha=false   # start with 1 machine, scale after verify

# Verify
fly status --app agentsfleetd-dev
```

`fly.toml` for the API app (no public port — tunnel is the only ingress):

```toml
app = "agentsfleetd-dev"
primary_region = "iad"

[build]
  image = "ghcr.io/agentsfleet/agentsfleetd:dev-latest"

[[vm]]
  size = "shared-cpu-1x"
  memory = "512mb"

# NO [http_service] block — suppresses *.fly.dev public domain entirely.
# All traffic enters via Cloudflare Tunnel (§2.4).
# Internal-only: accessible at agentsfleetd-dev.internal:3000 within Fly 6PN.

[metrics]
  port = 9091
  path = "/metrics"
```

There is no Fly "worker" app. Execution runs on the host-resident `agentsfleet-runner` daemon on a bare-metal node — it leases work over HTTPS, runs the agent in a bubblewrap-sandboxed forked child, and reports back over a Tailscale control plane. The runner host is bootstrapped separately (§4.0 of this doc; canonical playbooks `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md` for DEV and `07_runner_bootstrap_prod/001_playbook.md` for PROD) and CI-deployed via `deploy-dev.yml`. Only the API (`agentsfleetd serve`) deploys to Fly. The single-process `agentsfleetd worker` subcommand and the standalone sandbox sidecar were folded into `agentsfleet-runner` by the M80_002 cutover — see `docs/architecture/runner_fleet.md`.

### 2.4 Cloudflare Tunnel — Origin Shield (Agent)

Tunnel replaces CNAME. All traffic: Cloudflare edge → encrypted tunnel → Fly private network. No public Fly port. No bypass.

```bash
# Create tunnel (run locally, credentials stored in ~/.cloudflared/) and capture its id
# Output line: "Created tunnel agentsfleetd-dev with id <uuid>"
TUNNEL_ID=$(cloudflared tunnel create agentsfleetd-dev | grep -oE '[0-9a-f-]{36}')

# Store credentials in vault
TUNNEL_CREDS=$(cat ~/.cloudflared/"$TUNNEL_ID".json | base64)
op item create --vault "$VAULT_DEV" --title cloudflare-tunnel-dev \
  --category "API Credential" \
  "tunnel-id=$TUNNEL_ID" \
  "credentials-json-b64=$TUNNEL_CREDS"

# Route tunnel to domain (creates CNAME $TUNNEL_ID.cfargotunnel.com automatically)
cloudflared tunnel route dns agentsfleetd-dev api-dev.agentsfleet.net

# Repeat for PROD
cloudflared tunnel create agentsfleetd-prod
cloudflared tunnel route dns agentsfleetd-prod api.agentsfleet.net
```

`cloudflared` config deployed as a Fly app (`cloudflared-dev`):

```yaml
# config.yml — baked into cloudflared Fly image or mounted as secret
tunnel: <TUNNEL_ID>
credentials-file: /etc/cloudflared/credentials.json

ingress:
  - hostname: api-dev.agentsfleet.net
    service: http://agentsfleetd-dev.internal:3000  # Fly 6PN private DNS
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
# Positional path = build context (the dir holding config.yml + Dockerfile).
# Required by flyctl >=0.4.5x; do NOT use `--config` from repo root — the Dockerfile
# `COPY config.yml` then fails with "/config.yml not found".
fly deploy deploy/fly/cloudflared-dev --app cloudflared-dev \
  --wait-timeout 120
```

This is infrastructure, not application code. Do not add to CI. Only redeploy if `deploy/fly/cloudflared-dev/config.yml` changes.

### 2.5 Auto-Scaling (Agent)

```bash
# Scale API to 2 machines for HA (both in iad)
fly scale count 2 --app agentsfleetd-dev

# Auto-scaling: scale up to 5 on load, never scale below 1
fly autoscale set min=1 max=5 --app agentsfleetd-dev

# Execution capacity is the host-resident agentsfleet-runner (bare-metal), not a
# Fly app — provision/scale it via the runner bootstrap playbooks (06_/07_).
```

For PROD multi-region (future):
```bash
# Add a second region for global HA
fly regions add lhr --app agentsfleetd-prod     # London for EU users
fly scale count 2 --app agentsfleetd-prod       # 1 machine per region
```

### 2.6 CI Wiring (Agent)

```bash
# Store Fly deploy token in vault
fly tokens create deploy -o agentsfleet --name ci-deploy
op item create --vault "$VAULT_DEV" --title fly-api-token \
  --category "API Credential" "credential=<token>"

# Set GitHub Actions vars
gh variable set FLY_APP_DEV --body "agentsfleetd-dev" --repo agentsfleet/agentsfleet
gh variable set FLY_APP_PROD --body "agentsfleetd-prod" --repo agentsfleet/agentsfleet
```

> The `DEV_WORKER_READY` / `PROD_WORKER_READY` GitHub vars are **not** set here — they stay `false` and are flipped later by `06_runner_bootstrap_dev` / `07_runner_bootstrap_prod` once a real `agt_r` runner-token is minted. The worker deploy path stays dark until then.

CI deploy step in `deploy-dev.yml`:
```yaml
- name: Deploy to Fly.io DEV
  run: fly deploy --app ${{ vars.FLY_APP_DEV }} --image ghcr.io/agentsfleet/agentsfleetd:dev-latest --wait-timeout 120
  env:
    FLY_API_TOKEN: ${{ secrets.FLY_API_TOKEN }}
```

### 2.7 Verify

```bash
# Tunnel health
cloudflared tunnel info agentsfleetd-dev

# API reachable via Cloudflare (not direct Fly)
curl -sf https://api-dev.agentsfleet.net/healthz
curl -sf https://api-dev.agentsfleet.net/readyz | jq '.ready'

# Confirm no direct Fly access (should time out or refuse)
curl -sf https://agentsfleetd-dev.fly.dev/healthz  # expected: connection refused / 404

# Fly machine status
fly status --app agentsfleetd-dev
fly logs   --app agentsfleetd-dev
```

---

## 3.0 Data-Plane Bootstrap

### 3.1 Postgres — Roles in Migrations

Roles (`db_migrator`, `api_runtime`, `memory_runtime`, `ops_readonly_human`, `ops_readonly_agent`) are defined in `schema/002_vault_schema.sql` with `IF NOT EXISTS` guards — idempotent. All grants across tables are in subsequent migration files. Run all migrations in order:

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
- `memory_runtime` — runtime DML on the agent-memory schema
- `ops_readonly_human`, `ops_readonly_agent` — read-only access via `ops_ro` views only

### 3.2 Redis — Streams (Upstash)

Redis is hosted on Upstash (DEV and PROD). ACL is managed via the Upstash dashboard — no custom ACL commands needed.

No manual stream bootstrap is required. agentsfleetd creates each agent's event stream (`agent:{agent_id}:events`) and its `agent_lease` consumer group on demand, synchronously, when a agent is created (`POST /v1/workspaces/{ws}/agents` → `ensureEventStream` in `src/agentsfleetd/http/handlers/agents/create_stream.zig`). The call is idempotent (`XGROUP CREATE … MKSTREAM`, `BUSYGROUP`-tolerant) with a bounded retry, so an empty cache self-heals on the first agent created after deploy.

For local docker-compose Redis, static credentials are configured in `docker-compose.yml`.

### 3.3 Clerk — Session Token Customization

Agentd's OIDC verifier checks `aud` **only when `OIDC_AUDIENCE` is set** (`src/auth/jwks.zig` — the audience comparison is skipped when the configured audience is null). Clerk's *default* session token does not carry `aud`, so the dashboard ran a second JWT shape (the api-template Bearer) through every fetch pre-M74_002 §9. Customizing the session token to add `aud`, `metadata.tenant_id`, and the `scopes` claim collapses the dashboard's runtime auth to one JWT — the same `useAuth().getToken()` value that `clerkMiddleware()` already reads from the `__session` cookie. The tenant-context claim (`metadata.tenant_id`) and the capability claim (`scopes`) are load-bearing; `aud` becomes load-bearing once `OIDC_AUDIENCE` is set (below).

**`OIDC_AUDIENCE` is wired in CI, not the vault.** It is set as a per-env literal in the `flyctl secrets set` step (alongside `OIDC_PROVIDER="clerk"`): `deploy-dev.yml` sets `https://api-dev.agentsfleet.net`, `release.yml` sets `https://api.agentsfleet.net`. It is **not** a 1Password field — the vault has no `clerk-{dev,prod}/audience`. (Historically `OIDC_AUDIENCE` was unset on both envs, so the aud check was a no-op; M74_002 §9 wires it.)

**Per-env audience — three surfaces must agree.** agentsfleetd checks `aud` on every bearer it receives, no matter which Clerk mechanism minted it. Three places carry the per-env audience and MUST hold the same value for that env:

1. **`OIDC_AUDIENCE`** — the CI literal in `deploy-dev.yml` / `release.yml` (what agentsfleetd compares against).
2. **Clerk → Sessions → Customize session token** — the `aud` claim on the *default* session token (feeds the new dashboard, D45 `auth().getToken()`).
3. **Clerk → JWT Templates → `api`** — the `aud` claim on the api-template token (feeds the CLI carve-out D47 + the currently-deployed pre-§9 dashboard).

Current values (confirmed 2026-05-20): DEV all three = `https://api-dev.agentsfleet.net`; PROD all three = `https://api.agentsfleet.net`.

Because surfaces 2 and 3 carry the **same** per-env `aud`, enabling `OIDC_AUDIENCE` is transition-safe across the old→new dashboard code swap and does not break the CLI. The hazard is editing one surface without the others: any token whose `aud` ≠ `OIDC_AUDIENCE` is fail-closed with a loud 401 `AudienceMismatch` (never silent). When rotating the audience for an env, change all three together; the CI step couples `OIDC_AUDIENCE` + image deploy atomically, so the only human ordering rule is to set the two Clerk `aud` claims before the deploy that ships `OIDC_AUDIENCE`.

**One-time setup (per env, Clerk dashboard — human):**

1. Sign in to the Clerk dashboard for the target instance (`dashboard.clerk.com` → select `clerk-dev` or `clerk-prod`).
2. Navigate to **Sessions → Customize session token**.
3. Paste the claims JSON below. Replace `<AUDIENCE>` with the env-specific value (see *Per-env audience* above). The top-level **`scopes`** claim is the capability axis (M104_001) — agentsfleetd parses it onto `principal.scopes` and the `requireScope` gate enforces it. It is sourced from `user.public_metadata.scopes`, a space-delimited string of `resource:action` scopes written at signup (see *Scope provisioning* below).
   ```json
   {
     "aud": "<AUDIENCE>",
     "scopes": "{{user.public_metadata.scopes}}",
     "metadata": {
       "tenant_id": "{{user.public_metadata.tenant_id}}"
     }
   }
   ```
4. Click **Save**. The Clerk UI applies the new template to all subsequently-minted session tokens; existing browser sessions continue with the pre-customization shape until next token refresh (~60s) or sign-out.

**Scope provisioning (M104_001 — how a user gets capabilities, no manual step per user):**

Authorization is scope-based: a request is allowed only if `principal.scopes` (parsed from the `scopes` claim above) carries the route's required scope. The owner and runner grants are applied **in code** at principal construction (`src/agentsfleetd/auth/scopes.zig::DefaultGrant`, keyed by credential source — never gate-checked); operator and collaborator scope sets are written **manually** in Clerk:

- **Owner at signup** — the `user.created` Clerk webhook handler (auth-plane event `/v1/auth/identity-events/clerk`) writes the **`.tenant`** default grant's scope list into the new user's `public_metadata.scopes` (alongside `tenant_id`). It carries every tenant capability (`fleet:admin`, `credential:write`, `apikey:admin`, `workspace:admin`, `template:write`, …), so a fresh owner can create workspaces and fleets immediately — **no manual Clerk role flip**, the historical `user→operator` auto-promotion is gone.
- **Collaborator (manual)** — write a read-only scope set onto `public_metadata.scopes`: `fleet:read`, `fleetkey:read`, `grant:read`, `connector:read`, `billing:read`, `approval:read`. No code grant; a collaborator who must act gets `fleet:write` etc. added explicitly.
- **Platform operator (manual)** — write `runner:enroll`, `runner:write`, `stream:read`, `model:admin`, `platform-key:admin`, `platform-template:write`, `workspace:any` onto the agentsfleet operator user's `public_metadata.scopes` — the one place a human edits scopes, replacing the old `platform_admin` boolean flip.

The scope strings in `public_metadata.scopes` MUST match the catalogue in `scopes.zig` verbatim (RULE UFS); an unknown string is ignored (grants nothing, deny-by-absence).

**Verification (per env, after save):**

1. Sign out of `app.agentsfleet.net` (or `app.dev.agentsfleet.net`) in a clean browser session; sign in again to force a fresh token.
2. Open DevTools → Application → Cookies → `__session`; copy the value.
3. Decode at `jwt.io` (or `jwt-cli decode`). Confirm the payload carries:
   - `aud` matches the env's `OIDC_AUDIENCE`.
   - `metadata.tenant_id` is non-empty (post-bootstrap).
   - `scopes` is a non-empty space-delimited capability string (e.g. carries `fleet:admin`/`workspace:admin` for an owner).
   - `sid` is present (proves the session token wasn't replaced by a template token).
4. Reload any `/(dashboard)/**` page; confirm no 401s in the network panel.

**Rollback procedure (I9.5):**

Clerk dashboard → **Sessions → Customize session token** → **Reset to default**. The next minted token will lack `aud`, every dashboard fetch will fail with `AudienceMismatch` on the next refresh, and operators will notice within ~60s. Re-apply the JSON above to restore. Rollback is reversible end-to-end; no agentsfleetd or schema state needs touching.

**Verification artifacts (V9.1–V9.5):**

| Check | Method | Pass criterion |
|---|---|---|
| V9.1 — feature available on plan | Clerk dashboard shows the **Customize session token** UI under Sessions | UI visible without an upgrade prompt |
| V9.2 — nested metadata renders | JWT decode after sign-in shows `metadata.tenant_id` populated from `user.public_metadata.tenant_id` | Non-empty value matches the user's `publicMetadata` |
| V9.3 — cookie size under cap | `document.cookie` value for `__session` is < 4KB | ≥30% headroom (~700 bytes typical) |
| V9.4 — `sid` present | JWT payload has `sid` field | `clerkMiddleware()` continues to validate the cookie |
| V9.5 — plan gating | Plan tier is Pro+ (Free tier blocks claim customization) | Confirm before scheduling D40 PROD apply |

> **Pre-D40 PROD checklist:** (1) the human-entered PROD Clerk `aud` claim equals `https://api.agentsfleet.net` (the literal `release.yml` sets as `OIDC_AUDIENCE`); (2) the D40 PROD Clerk customization is applied **before** the prod release that ships `OIDC_AUDIENCE` + the new dashboard code; (3) V9.5 confirmed against the current Clerk plan; (4) nkishore@megam.io DEV cookie measured for the V9.3 baseline. Note `OIDC_AUDIENCE` is no longer a vault/`fly secrets`-list item — it's a CI literal, so verify it in `release.yml`, not `fly secrets list`.

---

## 4.0 Worker Infrastructure + Deployment Handoff

Worker bootstrap, CI deployment handoff, and startup reuse guidance moved to:

- `playbooks/founding/03_priming_infra/002_workers_and_handoff.md`
- `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md`
- `playbooks/founding/07_runner_bootstrap_prod/001_playbook.md`
- `playbooks/founding/04_deploy_dev/001_playbook.md`
- `playbooks/founding/05_deploy_prod/001_playbook.md`

Keep this file focused on infra priming (container, Fly, Cloudflare tunnel, data plane) so it stays under the repository line-limit gate.
