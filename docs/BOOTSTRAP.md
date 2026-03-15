# Operator Bootstrap

**Human job:** create accounts + generate one root API key per service (~15 min). Hand off to agent.
**Agent job:** everything else — store keys in 1Password, configure Vercel, Cloudflare, GitHub, CI.

---

## Phase 1: Human (one-time per startup)

### 1. Create accounts

- [ ] **GitHub** — create org + repo
- [ ] **1Password Teams** — create team, create two vaults: `ZMB_CD_DEV`, `ZMB_CD_PROD`; create service account (e.g. `usezombie-ci`) with access to both vaults; copy its token
- [ ] **Vercel** — sign up, connect GitHub repo, create projects per the spec
- [ ] **Cloudflare** — add domains, set nameservers
- [ ] **Codecov** — connect GitHub repo
- [ ] **npm** — create org

### 2. Generate root API keys (one per service)

| Service | What to generate | Where |
|---|---|---|
| 1Password | Service account token for `usezombie-ci` | 1Password → Service Accounts |
| Cloudflare | API token with Zone:Edit + Zone:Read (all zones) | CF → My Profile → API Tokens |
| Vercel (website) | Deployment Protection bypass secret | Vercel → project → Settings → Deployment Protection |
| Vercel (app) | Deployment Protection bypass secret | Vercel → project → Settings → Deployment Protection |
| npm | Granular publish token | npmjs.org → Access Tokens |
| Codecov | Repo token | Codecov → repo settings |
| gitleaks | License key | gitleaks.io |

### 3. Hand off to agent

Give the agent:
- The `OP_SERVICE_ACCOUNT_TOKEN` (1Password service account token)
- All other raw API keys from step 2
- This message:

> "Bootstrap Phase 1 complete. Here are the root API keys: [paste keys]. Store them in 1Password vaults `ZMB_CD_PROD` / `ZMB_CD_DEV` per `docs/BOOTSTRAP.md §Vault structure`, then configure Vercel env vars, Cloudflare DNS, and GitHub Secrets."

---

## Phase 2: Agent

### Vault structure to create

**Vault: `ZMB_CD_PROD`** — production and CI secrets

| Item | Field | Value source |
|---|---|---|
| `cloudflare-token` | `credential` | CF API token from human |
| `npm-publish-token` | `credential` | npm token from human |
| `vercel-bypass-website` | `credential` | Vercel bypass secret (website project) |
| `vercel-bypass-app` | `credential` | Vercel bypass secret (app project) |
| `zombied-prod-server-1` | `hostname`, `ssh-private-key`, `deploy-user` | on server provision |
| `zombied-prod-server-2` | `hostname`, `ssh-private-key`, `deploy-user` | on server provision |

**Vault: `ZMB_CD_DEV`** — dev secrets

| Item | Field | Value source |
|---|---|---|
| `zombied-dev-server` | `hostname`, `ssh-private-key`, `deploy-user` | on server provision |

### GitHub Secrets to set

GitHub repo → Settings → Secrets and Variables → Actions:

| Secret | Value |
|---|---|
| `OP_SERVICE_ACCOUNT_TOKEN` | 1Password service account token |
| `CODECOV_TOKEN` | Codecov repo token |
| `GITLEAKS_LICENSE` | gitleaks license key |

> Only these three live in GitHub. All other secrets are fetched from 1Password at runtime via `op://` URIs.

### Cloudflare — zone discovery

```bash
zsh -i -c "op read 'op://ZMB_CD_PROD/cloudflare-token/credential'" | \
  xargs -I{} curl -s -H "Authorization: Bearer {}" \
  https://api.cloudflare.com/client/v4/zones | jq '.result[] | {name, id}'
```

### Vercel — env var scoping

**`usezombie-app`** → Settings → Environment Variables:

| Variable | Preview | Production |
|---|---|---|
| `NEXT_PUBLIC_API_URL` | `https://api.dev.usezombie.com` | `https://api.usezombie.com` |

**`usezombie-agents-sh`** and **`usezombie-website`** → Settings → Environment Variables:

| Variable | Preview | Production |
|---|---|---|
| `VITE_APP_BASE_URL` | `https://app.dev.usezombie.com` | `https://app.usezombie.com` |

---

## Reuse for a new startup

1. Replace vault prefix (`ZMB` → `<PROJECT>`)
2. Replace domain names, Vercel project names, service account name
3. Phase 1 checklist is identical — same services, same key types
4. Hand off after Phase 1 with the same message

**Pattern: humans own identity, agents own configuration.**
