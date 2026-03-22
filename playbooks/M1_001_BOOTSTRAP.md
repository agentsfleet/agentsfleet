# M1_001: Playbook — Bootstrap

**Milestone:** M1
**Workstream:** 001
**Updated:** Mar 19, 2026
**Owner (1.0):** Human — one-time per startup
**Owner (2.0):** Agent — executes after 1.0 handoff

Reusable across startups. Replace `ZMB` vault prefix, domain names, and service names per project.

---

## 1.0 Human Prerequisites

Create accounts and generate one root API key per service. Hand off to agent when done.

### 1.1 Create Accounts

- [ ] **GitHub** — create org + repo
- [ ] **1Password Teams** — create team, create two vaults (names become `VAULT_DEV` and `VAULT_PROD` GitHub variables); create service account (e.g. `<project>-ci`) with access to both vaults; copy its token
- [ ] **Vercel** — sign up, connect GitHub repo, create projects per the spec
- [ ] **Cloudflare** — add domains, set nameservers
- [ ] **Codecov** — connect GitHub repo
- [ ] **npm** — create org
- [ ] **Fly.io** — sign up at fly.io (use Google/GitHub). Add a payment method (required even on free tier). Install `flyctl` locally: `curl -L https://fly.io/install.sh | sh`. Run `fly auth login` (browser OAuth, one-time per machine). All service setup is agent-executed via CLI (see M2_002 §2.0).
- [ ] **Cloudflare Tunnel** — no separate sign-up needed; tunnels are created under your existing Cloudflare account. Install `cloudflared` locally (`mise install cloudflared` or `brew install cloudflared`). Run `cloudflared tunnel login` (browser OAuth, one-time per machine — selects which Cloudflare zone to authorize). Agent then creates tunnels, stores credentials in vault, and routes DNS via `cloudflared` CLI (see M2_002 §2.4).

### 1.2 Generate Root API Keys

One key per service:

| Service | What to generate | Where |
|---|---|---|
| 1Password | Service account token for `usezombie-ci` | 1Password → Service Accounts |
| Vercel | Account API token (Full Account scope) | Vercel → Account Settings → Tokens |
| Vercel (`usezombie-website`) | Deployment Protection bypass secret | Vercel → project → Settings → Deployment Protection |
| Vercel (`usezombie-agents-sh`) | Deployment Protection bypass secret | Vercel → project → Settings → Deployment Protection |
| Vercel (`usezombie-app`) | Deployment Protection bypass secret | Vercel → project → Settings → Deployment Protection |
| Cloudflare | API token with Zone:Edit + DNS:Edit + Transform Rules:Edit (all zones) | CF → My Profile → API Tokens → Create Token |
| Fly.io | Deploy token (org-scoped) | `fly tokens create deploy -o <org>` → copy output |
| Clerk (DEV instance) | Publishable key + Secret key | Clerk dashboard → DEV instance → API Keys |
| Clerk (PROD instance) | Publishable key + Secret key | Clerk dashboard → PROD instance → API Keys |
| GitHub App | App ID + PEM private key | GitHub → Settings → Developer settings → GitHub Apps → New GitHub App |
| npm | Granular publish token | npmjs.org → Access Tokens |
| Codecov | Repo token | Codecov → repo settings |
| gitleaks | License key | gitleaks.io |

### 1.3a Generate Encryption Master Key

Generate a 32-byte (64 hex char) AES-256 key for at-rest secret encryption. Run once per environment:

```bash
# DEV key
openssl rand -hex 32

# PROD key (run separately — must differ from DEV)
openssl rand -hex 32
```

Store each output as `credential` in its respective vault item (`encryption-master-key`). Never reuse between environments.

### 1.3 Hand Off to Agent

Give the agent:
- The `OP_SERVICE_ACCOUNT_TOKEN` (1Password service account token)
- All other raw API keys from 1.2

Hand-off message:

> "Milestone 1 complete. Here are the root API keys: [paste keys]. Store them in 1Password vaults `ZMB_CD_PROD` / `ZMB_CD_DEV` per `playbooks/M1_001_BOOTSTRAP.md §2.0`, then run `./playbooks/gates/check-credentials.sh` and proceed with `playbooks/M2_001_PREFLIGHT.md`."

---

## 2.0 Agent Steps

Agent executes these steps immediately after receiving the hand-off from 1.3.

### 2.1 Store Keys in 1Password Vaults

**Vault: `ZMB_CD_PROD`** — production and CI secrets

| Item | Field | Value source |
|---|---|---|
| `cloudflare-api-token` | `credential` | CF API token from human |
| `npm-publish-token` | `credential` | npm token from human |
| `vercel-bypass-website` | `credential` | Vercel → `usezombie-website` → Deployment Protection → Bypass |
| `vercel-bypass-agents` | `credential` | Vercel → `usezombie-agents-sh` → Deployment Protection → Bypass |
| `vercel-bypass-app` | `credential` | Vercel → `usezombie-app` → Deployment Protection → Bypass |
| `clerk-prod` | `publishable-key`, `secret-key` | Clerk PROD instance API Keys |
| `github-app` | `app-id` | GitHub App → App ID (numeric) |
| `github-app` | `private-key` | GitHub App → Generate a private key → PEM contents |
| `encryption-master-key` | `credential` | `openssl rand -hex 32` — PROD key (see §1.3a) |
| `zombied-prod-server-1` | `hostname`, `ssh-private-key`, `deploy-user` | on server provision |
| `zombied-prod-server-2` | `hostname`, `ssh-private-key`, `deploy-user` | on server provision |

**Vault: `ZMB_CD_DEV`** — dev secrets

| Item | Field | Value source |
|---|---|---|
| `clerk-dev` | `publishable-key`, `secret-key` | Clerk DEV instance API Keys |
| `vercel-api-token` | `credential` | Vercel Account Settings → Tokens |
| `github-app` | `app-id` | Same GitHub App — reuse PROD app-id for DEV |
| `github-app` | `private-key` | Same GitHub App PEM |
| `encryption-master-key` | `credential` | `openssl rand -hex 32` — DEV key (must differ from PROD) |

### 2.2 Set GitHub Secrets and Variables

GitHub repo → Settings → Secrets and Variables → Actions:

**Secrets** (sensitive — never visible after saving):

| Secret | Value |
|---|---|
| `OP_SERVICE_ACCOUNT_TOKEN` | 1Password service account token |
| `CODECOV_TOKEN` | Codecov repo token |
| `GITLEAKS_LICENSE` | gitleaks license key |

> Only these three live in GitHub as secrets. All other secrets are fetched from 1Password at runtime via `op://` URIs.

**Variables** (non-sensitive — visible, used to parameterise vault names):

| Variable | Value | Where defined |
|---|---|---|
| `VAULT_DEV` | e.g. `ZMB_CD_DEV` | name you chose when creating the vault |
| `VAULT_PROD` | e.g. `ZMB_CD_PROD` | name you chose when creating the vault |

> Workflows reference these as `${{ vars.VAULT_DEV }}` / `${{ vars.VAULT_PROD }}` — no hardcoded vault names anywhere in CI. Scripts fall back to `ZMB_CD_DEV` / `ZMB_CD_PROD` if the env var is not set locally.

### 2.3 GHCR Package Permissions

After the first CI push to GHCR (triggered automatically on the first merge to `main`), set the package visibility:

1. Go to `https://github.com/orgs/<org>/packages/container/<service>/settings`
2. **Change visibility → Public** — Fly.io and any consumer can pull without credentials
3. **Manage Actions access → Add repository** → select the repo → set role to **Write**

This is a one-time human step. GitHub has no API endpoint to change org package visibility — it must be done in the UI. Once public, it persists across all future CI pushes.

> **Why public?** The image contains only the compiled binary. All secrets come from env vars at runtime. There is no secret in the image.

### 2.3a Fly.io — API + Worker Services (Agent-executed via CLI)

**Architecture:**
```
Cloudflare Edge (api-dev.usezombie.com)
    │ Cloudflare Tunnel — encrypted, origin-shielded
    ▼
cloudflared-dev (Fly app, 2 machines for HA)
    │ Fly private network / 6PN (internal only, no public port)
    ▼
zombied-dev.internal:3000  ← Fly anycast LB (automatic)
    ├── Machine 1 (iad, shared-cpu-1x 512MB)
    └── Machine 2 (iad, shared-cpu-1x 512MB)  ← auto-scaled up to N

zombied-dev-worker (separate Fly app, scaled independently)
    └── Machine 1..N running `zombied worker`
```

**Why Fly.io:**
- No `*.fly.dev` public domain created when `[http_service]` is omitted — only the Cloudflare Tunnel is the ingress. True origin shielding.
- Built-in anycast load balancing across all machines — no LB config.
- Auto-scaling: set `min_machines_running` + `auto_stop_machines` in `fly.toml`.
- Static outbound IP included — needed for PlanetScale/Upstash IP allowlisting.
- `iad` region co-locates with PlanetScale `aws-us-east-2` → ~5ms DB latency.

Agent executes via Fly CLI (see M2_002 §2.0 for full steps):

```bash
# Authenticate (human does fly auth login once; agent uses deploy token from vault)
export FLY_API_TOKEN=$(op read "op://$VAULT_DEV/fly-api-token/credential")

# Create apps
fly apps create zombied-dev       --org <org>
fly apps create cloudflared-dev   --org <org>
fly apps create zombied-dev-worker --org <org>

# Set secrets from vault
fly secrets set \
  DATABASE_URL_API="$(op read 'op://$VAULT_DEV/planetscale-dev/api-connection-string')" \
  DATABASE_URL_WORKER="$(op read 'op://$VAULT_DEV/planetscale-dev/worker-connection-string')" \
  REDIS_URL_API="$(op read 'op://$VAULT_DEV/upstash-dev/api-url')" \
  REDIS_URL_WORKER="$(op read 'op://$VAULT_DEV/upstash-dev/worker-url')" \
  ENCRYPTION_MASTER_KEY="$(op read 'op://$VAULT_DEV/encryption-master-key/credential')" \
  GITHUB_APP_ID="$(op read 'op://$VAULT_DEV/github-app/app-id')" \
  GITHUB_APP_PRIVATE_KEY="$(op read 'op://$VAULT_DEV/github-app/private-key')" \
  OIDC_JWKS_URL="https://winning-wombat-65.clerk.accounts.dev/.well-known/jwks.json" \
  OIDC_ISSUER="https://winning-wombat-65.clerk.accounts.dev" \
  --app zombied-dev

# Deploy from GHCR
fly deploy --app zombied-dev --image ghcr.io/usezombie/zombied:dev-latest

# Scale to 2 machines for HA
fly scale count 2 --app zombied-dev
```

CI triggers redeployments via `fly deploy --image` using the deploy token. Store in vault and set GitHub Actions vars:
- `fly-api-token` → `ZMB_CD_DEV` and `ZMB_CD_PROD` vaults
- `FLY_API_TOKEN` → GitHub Actions secret (or load from 1Password via OP_SERVICE_ACCOUNT_TOKEN)

### 2.3b Cloudflare Tunnel — Origin Shield (Agent-executed)

Cloudflare Tunnel routes all traffic from `api-dev.usezombie.com` → Fly private network. No public port on Fly. No bypass possible.

```bash
# Create tunnel (stores credentials locally; agent saves to vault)
cloudflared tunnel create zombied-dev
# Output: tunnel ID e.g. abc123...

# Store tunnel credentials in vault
op item create --vault "$VAULT_DEV" --title cloudflare-tunnel-dev \
  --category "API Credential" \
  "credential=$(cat ~/.cloudflared/<tunnel-id>.json | base64)"

# Create DNS CNAME → tunnel (origin-shielded, no public Fly endpoint)
cloudflared tunnel route dns zombied-dev api-dev.usezombie.com
```

`cloudflared` config deployed as a Fly app connects to `zombied-dev.internal:3000` via Fly's private 6PN network. The Fly app has no `[http_service]` — no public endpoint is created.

Cloudflare SSL/TLS mode: **Full (Strict)**. No Transform Rules needed — the tunnel handles routing.

### 2.3c Cloudflare DNS — Managed Records

For non-API DNS (website, app, etc.) — agent sets CNAME records via Cloudflare API:

```bash
CF_TOKEN=$(op read "op://$VAULT_PROD/cloudflare-api-token/credential")
ZONE_ID=<from 2.6 below>

# These point to Vercel (not Fly — API traffic goes via tunnel)
curl -X POST "https://api.cloudflare.com/client/v4/zones/$ZONE_ID/dns_records" \
  -H "Authorization: Bearer $CF_TOKEN" -H "Content-Type: application/json" \
  -d '{"type":"CNAME","name":"app","content":"cname.vercel-dns.com","proxied":true,"ttl":1}'
```

Cloudflare API token needs: Zone:Edit + DNS:Edit + Transform Rules:Edit permissions.

### 2.6 Cloudflare — Zone Discovery

```bash
# VAULT_PROD must be set in your environment (or export it first)
CF_TOKEN=$(op read "op://$VAULT_PROD/cloudflare-api-token/credential")
curl -s -H "Authorization: Bearer $CF_TOKEN" \
  https://api.cloudflare.com/client/v4/zones | jq '.result[] | {name, id}'
```

### 2.7 Vercel — Set Env Vars

Agent reads project IDs and API token from 1Password, sets via Vercel API (`PATCH /v9/projects/{id}/env`).

**`usezombie-app`:**

| Variable | Preview | Production |
|---|---|---|
| `NEXT_PUBLIC_API_URL` | `https://api.dev.usezombie.com` | `https://api.usezombie.com` |
| `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | Clerk DEV publishable key | Clerk PROD publishable key |
| `CLERK_SECRET_KEY` | Clerk DEV secret key | Clerk PROD secret key |

**`usezombie-agents-sh`** and **`usezombie-website`:**

| Variable | Preview | Production |
|---|---|---|
| `VITE_APP_BASE_URL` | `https://app.dev.usezombie.com` | `https://app.usezombie.com` |

---

## 3.0 Handoff to Milestone 2

Once 2.4 is verified, agent runs `./playbooks/gates/check-credentials.sh` (M2_001) to confirm all vault items are present before executing `playbooks/M2_002_PRIMING_INFRA.md`.

All vault items the agent will need are listed in `playbooks/M2_001_PREFLIGHT.md §1.0` and `§4.0`. Review that list now and create any missing items in 1Password before the handoff — it avoids mid-execution failures.

---

## 4.0 Reuse for a New Startup

1. Choose vault names for the new project (e.g. `ABC_CD_DEV`, `ABC_CD_PROD`)
2. Set GitHub repo variables `VAULT_DEV` / `VAULT_PROD` to those names
3. Replace domain names, Vercel project names, service account name in this doc
4. Milestone 1 checklist is otherwise identical — same services, same key types

**Pattern: humans own identity, agents own configuration.**
