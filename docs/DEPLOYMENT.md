# Deployment Playbook

Updated: Mar 15, 2026
Model: Agent-first. Humans bootstrap accounts (see `BOOTSTRAP.md`), agents own everything else.
Runtime env contract: `CONFIGURATION.md`. CI/release pipeline: `spec/v1/M6_005_GITHUB_CI_RELEASE_PIPELINE.md`.
Public docs source: this file informs `docs.usezombie.com` (Mintlify, repo: `usezombie/docs`).

---

## Stack

| Role | Provider |
|---|---|
| DNS, WAF, DDoS, LB | Cloudflare |
| Object storage | Cloudflare R2 |
| Website + App | Vercel |
| API (zombied serve) | Railway (auto-scales) |
| Database | Planetscale Postgres |
| Cache + Queue | Upstash Redis (TLS required) |
| Auth | Clerk |
| Workers (zombied worker) | OVHCloud bare-metal (Beauharnois CA) |
| Infra observability | Grafana Cloud |
| LLM tracing | Langfuse |
| Analytics | PostHog |
| Email | Resend |
| Docs | Mintlify |
| Communication | Discord |
| Secrets | 1Password (`op inject` local, `op://` in CI) |

Excluded: AWS, GCP, Azure, DigitalOcean.

---

## Domains

| Domain | Target | Purpose |
|---|---|---|
| `usezombie.com` | Vercel (`SITE_VARIANT=humans`) | marketing website |
| `usezombie.sh` | Vercel (`SITE_VARIANT=agents`) | agents page (same codebase, different variant) |
| `app.usezombie.com` | Vercel | Mission Control (Next.js) |
| `api.usezombie.com` | Railway prod | API production |
| `dev.api.usezombie.com` | Railway dev | API development |
| `docs.usezombie.com` | Mintlify | documentation |

All proxied through Cloudflare. LB health check on `api.usezombie.com`: `GET /healthz` every 60s.

---

## Vercel Projects

Three projects, same repo (`usezombie/usezombie`), all auto-deploy on push via GitHub integration.

| Project | Root dir | Production domain | SITE_VARIANT |
|---|---|---|---|
| `usezombie-website` | `ui/packages/website` | `usezombie.com` | `humans` |
| `usezombie-agents-sh` | `ui/packages/website` | `usezombie.sh` | `agents` |
| `usezombie-app` | `ui/packages/app` | `app.usezombie.com` | — |

### Environment variables (agent sets via Vercel API)

**`usezombie-app`** — reads `ZMB_CD_DEV` + `ZMB_CD_PROD` from 1Password:

| Variable | Preview | Production |
|---|---|---|
| `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | Clerk DEV `pk_test_…` | Clerk PROD `pk_live_…` |
| `CLERK_SECRET_KEY` | Clerk DEV `sk_test_…` | Clerk PROD `sk_live_…` |
| `NEXT_PUBLIC_API_URL` | `https://dev.api.usezombie.com` | `https://api.usezombie.com` |

> Use exact name `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` — `@clerk/nextjs` requires this specific key.

**`usezombie-website`** and **`usezombie-agents-sh`** (stateless, no API):

| Variable | Preview | Production |
|---|---|---|
| `VITE_APP_BASE_URL` | `https://app.dev.usezombie.com` | `https://app.usezombie.com` |

> `VITE_APP_BASE_URL` is a link in the website pointing users to Mission Control — not an API endpoint.

### Preview deploy behavior

- `usezombie-website` and `usezombie-agents-sh`: stateless — parallel PR previews are fully independent.
- `usezombie-app`: all PR previews share `dev.api.usezombie.com`. Preview URLs are on `*.vercel.app` — configure `zombied` CORS to allow `*.vercel.app` + `*.usezombie.com` in dev.

### Deployment Protection bypass (smoke CI)

Each project has a separate bypass token stored in `ZMB_CD_PROD`. Agent loads via `1password/load-secrets-action@v2` and injects `x-vercel-protection-bypass` header in Playwright smoke tests.

| 1Password item | Project |
|---|---|
| `vercel-bypass-website` | `usezombie-website` |
| `vercel-bypass-agents` | `usezombie-agents-sh` |
| `vercel-bypass-app` | `usezombie-app` |

---

## Environments

| Layer | LOCAL | DEV | PROD |
|---|---|---|---|
| Website/App | `bun run dev` | Vercel preview → `dev.api` | Vercel prod → `api` |
| API | `localhost:3000` | `dev.api.usezombie.com` (Railway) | `api.usezombie.com` (Railway) |
| Database | Docker Postgres | Planetscale `usezombie-dev` | Planetscale `usezombie` |
| Redis | Docker Redis | Upstash `usezombie-dev` | Upstash `usezombie-cache` |
| Auth | Clerk DEV | Clerk DEV | Clerk PROD |
| Workers | local process | — | OVHCloud bare-metal |

LOCAL and DEV share external service instances (Clerk DEV, GitHub App DEV, PostHog DEV). Only infra differs.

---

## Secrets (1Password)

Templates in `.env.{local,dev,prod}.tpl` use `op://` references:

```bash
make env              # → op inject -i .env.local.tpl -o .env -f
ENV=dev make env      # → op inject -i .env.dev.tpl -o .env -f
ENV=prod make env     # → op inject -i .env.prod.tpl -o .env -f
```

Vaults: `ZMB_CD_DEV` (dev + local), `ZMB_CD_PROD` (production + CI). Full vault structure in `BOOTSTRAP.md`.

---

## Agent Deploy Sequence

Human completes Phase 1 in `BOOTSTRAP.md` (accounts + root API keys). Agent does everything below.

### 1. GitHub Secrets

Set three secrets in repo → Settings → Secrets → Actions:

```
OP_SERVICE_ACCOUNT_TOKEN  ← 1Password service account token
CODECOV_TOKEN             ← Codecov repo token
GITLEAKS_LICENSE          ← gitleaks license key
```

All other secrets are fetched from 1Password at runtime via `op://` URIs.

### 2. DNS (Cloudflare)

Agent reads `cloudflare-api-token` from `ZMB_CD_PROD`, discovers zone IDs, creates CNAME records per Domains table above.

**Clerk PROD DNS** (run after human creates Clerk PROD instance, before Clerk DNS verification):

```bash
CF_TOKEN=$(op read "op://ZMB_CD_PROD/cloudflare-api-token/credential")
ZONE_ID=<usezombie.com zone id>

# 5 CNAMEs required by Clerk
add_cname clerk          frontend-api.clerk.services
add_cname accounts       accounts.clerk.services
add_cname clkmail        mail.<clerk-instance>.clerk.services
add_cname clk._domainkey dkim1.<clerk-instance>.clerk.services
add_cname clk2._domainkey dkim2.<clerk-instance>.clerk.services
```

Then click "Verify configuration" in Clerk dashboard.

### 3. Vercel env vars

Agent reads keys from 1Password, sets via Vercel API (`POST /v9/projects/{id}/env`):

```bash
VERCEL_TOKEN=$(op read "op://ZMB_CD_DEV/vercel-api-token/credential")

# usezombie-app — preview
CLERK_PK=$(op read "op://ZMB_CD_DEV/clerk-dev/publishable-key")
CLERK_SK=$(op read "op://ZMB_CD_DEV/clerk-dev/secret-key")
# set NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY + CLERK_SECRET_KEY + NEXT_PUBLIC_API_URL → target: preview

# usezombie-app — production
CLERK_PK=$(op read "op://ZMB_CD_PROD/clerk-prod/publishable-key")
CLERK_SK=$(op read "op://ZMB_CD_PROD/clerk-prod/secret-key")
# set NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY + CLERK_SECRET_KEY + NEXT_PUBLIC_API_URL → target: production

# usezombie-website + usezombie-agents-sh
# set VITE_APP_BASE_URL → preview: app.dev.usezombie.com, production: app.usezombie.com
```

### 4. Data plane

**Postgres:** create roles (`api_accessor`, `worker_accessor`, `callback_accessor`), apply migrations from `schema/`, store connection strings in 1Password.

**Redis:** create stream + consumer group + ACLs:

```
XGROUP CREATE run_queue workers 0 MKSTREAM
ACL SETUSER api_user on >... ~run_queue +xadd +xgroup +ping
ACL SETUSER worker_user on >... ~run_queue +xreadgroup +xack +xautoclaim +xgroup +ping +xinfo
ACL SETUSER default off
```

### 5. Auth (Clerk)

Configure callback URL: `{api_domain}/v1/github/callback`. Auth flow details in `USECASE.md §0`.

### 6. API (Railway)

Deploy `zombied serve`. Env vars per `CONFIGURATION.md`. Migration policy: `MIGRATE_ON_START=0` (fail-closed default). Verify `/healthz` + `/readyz`.

### 7. Workers (OVHCloud)

**Naming:** alphabetical animals — `zombie-prod-server-ant`, `zombie-prod-server-bird`, `zombie-prod-server-cat`, ...

**Base:** Debian Trixie, hardened. Tailscale for SSH — no public SSH.
**Connectivity:** Planetscale + Upstash via allowlist.
**Scaling pipeline:** see `AUTO_AGENTS.md`.

### 8. CLI distribution

Release workflow publishes `zombiectl` to npm. Verify: `npx zombiectl login && npx zombiectl doctor`.

---

## Network Policy (Tailscale)

- API + worker nodes: same tailnet
- API → Postgres, Redis, GitHub API
- Workers → Postgres, Redis, GitHub API, LLM providers
- External → API only (443 via Cloudflare)
- External → Postgres, Redis, Workers: **DENIED**
- TLS terminates at Cloudflare; `zombied serve` runs private HTTP behind LB

---

## Verification

```bash
curl -sS https://api.usezombie.com/healthz
curl -sS https://api.usezombie.com/readyz | jq '.queue_dependency,.ready'
npx zombiectl login && npx zombiectl doctor
zombied doctor --format=json
```

Smoke: `npx zombiectl workspace add https://github.com/indykish/terraform-provider-e2e`, sync specs, submit run, verify worker claims + PR creation.

---

## Security Checklist

- [ ] Tailscale ACLs — workers unreachable externally
- [ ] Postgres role separation — `api_accessor` cannot read `vault.secrets`
- [ ] Redis ACLs — API user cannot XREADGROUP, worker cannot write arbitrary keys
- [ ] TLS on all Postgres + Redis connections
- [ ] `ENCRYPTION_MASTER_KEY` in-memory only, never logged
- [ ] GitHub tokens: installation-scoped, 1-hour, never stored
- [ ] Cloudflare WAF + DDoS on all public endpoints
- [ ] `zombied doctor` reports security posture

---

## Local Redis TLS

For `rediss://` in local Docker dev:

```bash
mkdir -p docker/redis/tls
openssl req -x509 -nodes -newkey rsa:2048 \
  -keyout docker/redis/tls/ca.key -out docker/redis/tls/ca.crt \
  -days 3650 -subj "/CN=usezombie-local-ca"
openssl req -nodes -newkey rsa:2048 \
  -keyout docker/redis/tls/server.key -out docker/redis/tls/server.csr \
  -subj "/CN=redis" -addext "subjectAltName=DNS:redis,DNS:localhost,IP:127.0.0.1"
openssl x509 -req -in docker/redis/tls/server.csr \
  -CA docker/redis/tls/ca.crt -CAkey docker/redis/tls/ca.key -CAcreateserial \
  -out docker/redis/tls/server.crt -days 365 -copy_extensions copy
```

---

## Pending

| Item | Status |
|---|---|
| Billing (Dodo) | deferred — feature-flagged off for free launch |
| Firecracker | v2 — required before multi-tenant customer workloads |
| CORS config for app previews | configure `zombied` to allow `*.vercel.app` in dev |
