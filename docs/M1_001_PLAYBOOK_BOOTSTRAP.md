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
- [ ] **Railway** — sign up, connect GitHub repo, create DEV and PROD services

### 1.2 Generate Root API Keys

One key per service:

| Service | What to generate | Where |
|---|---|---|
| 1Password | Service account token for `usezombie-ci` | 1Password → Service Accounts |
| Vercel | Account API token (Full Account scope) | Vercel → Account Settings → Tokens |
| Vercel (`usezombie-website`) | Deployment Protection bypass secret | Vercel → project → Settings → Deployment Protection |
| Vercel (`usezombie-agents-sh`) | Deployment Protection bypass secret | Vercel → project → Settings → Deployment Protection |
| Vercel (`usezombie-app`) | Deployment Protection bypass secret | Vercel → project → Settings → Deployment Protection |
| Cloudflare | API token with Zone:Edit + Zone:Read (all zones) | CF → My Profile → API Tokens |
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

> "Milestone 1 complete. Here are the root API keys: [paste keys]. Store them in 1Password vaults `ZMB_CD_PROD` / `ZMB_CD_DEV` per `docs/M1_001_PLAYBOOK_BOOTSTRAP.md §2.0`, then run `./scripts/check-credentials.sh` and proceed with `docs/M2_001_PLAYBOOK_CREDENTIAL_CHECK.md`."

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

After the first CI push to GHCR (triggered automatically on the first merge to `main`), set the package visibility and grant the repo write access so subsequent pushes succeed:

1. Go to `https://github.com/orgs/<org>/packages/container/<service>`
2. **Settings → Change visibility** → set to `Private` (or `Public` if open source)
3. **Settings → Manage Actions access → Add repository** → select the repo → set role to **Write**

This is a one-time human step. Without it, `docker push` in CI fails with a 403.

### 2.3a Railway Services — DEV and PROD

**Human does once in Railway dashboard:**

1. New Project → **Deploy from Docker Image** (not "Deploy from GitHub source")
2. **DEV service:** image = `ghcr.io/<org>/<service>:dev-latest`
   - Source type: Docker Image
   - Auto-deploy: Railway polls or use a deploy hook (see M2_002 §2.3)
3. **PROD service:** image = `ghcr.io/<org>/<service>:latest`
   - Source type: Docker Image
   - Auto-deploy: triggered by Railway deploy hook called from `release.yml`
4. Set the Railway deploy hook URL for each service and store in vault:
   - `railway-deploy-hook-dev` → `credential` in `ZMB_CD_DEV`
   - `railway-deploy-hook-prod` → `credential` in `ZMB_CD_PROD`

> Do **not** connect Railway to the GitHub branch for source-based builds. Our model builds and cross-compiles binaries in CI, packages them into a Docker image, and pushes to GHCR. Railway pulls from GHCR only.

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

Once 2.4 is verified, agent runs `./scripts/check-credentials.sh` (M2_001) to confirm all vault items are present before executing `docs/M2_002_PLAYBOOK_PRIMING_INFRA.md`.

All vault items the agent will need are listed in `docs/M2_001_PLAYBOOK_CREDENTIAL_CHECK.md §1.0` and `§4.0`. Review that list now and create any missing items in 1Password before the handoff — it avoids mid-execution failures.

---

## 4.0 Reuse for a New Startup

1. Choose vault names for the new project (e.g. `ABC_CD_DEV`, `ABC_CD_PROD`)
2. Set GitHub repo variables `VAULT_DEV` / `VAULT_PROD` to those names
3. Replace domain names, Vercel project names, service account name in this doc
4. Milestone 1 checklist is otherwise identical — same services, same key types

**Pattern: humans own identity, agents own configuration.**
