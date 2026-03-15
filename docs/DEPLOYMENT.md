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
| `usezombie.com` | Vercel (`SITE_VARIANT=humans`) | website |
| `usezombie.sh` | Vercel (`SITE_VARIANT=agents`) | agents page |
| `app.usezombie.com` | Vercel | Mission Control |
| `api.usezombie.com` | Railway prod | API production |
| `dev.api.usezombie.com` | Railway dev | API development |
| `docs.usezombie.com` | Mintlify | documentation |

All proxied through Cloudflare. LB health check on `api.usezombie.com`: `GET /healthz` every 60s.

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

### 1. DNS (Cloudflare)

Agent discovers zones via CF API, creates/verifies CNAME records (see Domains table). Configures LB origin pool for `api.usezombie.com`.

### 2. Data plane

**Postgres:** create roles (`api_accessor`, `worker_accessor`, `callback_accessor`), apply migrations from `schema/`, store connection strings in 1Password.

**Redis:** create stream + consumer group + ACLs:

```
XGROUP CREATE run_queue workers 0 MKSTREAM
ACL SETUSER api_user on >... ~run_queue +xadd +xgroup +ping
ACL SETUSER worker_user on >... ~run_queue +xreadgroup +xack +xautoclaim +xgroup +ping +xinfo
ACL SETUSER default off
```

### 3. Auth (Clerk)

Configure callback URL: `{api_domain}/v1/github/callback`. Auth flow details in `USECASE.md §0`.

### 4. API (Railway)

Deploy `zombied serve`. Env vars per `CONFIGURATION.md`. Migration policy: `MIGRATE_ON_START=0` (fail-closed default). Verify `/healthz` + `/readyz`.

### 5. Workers (OVHCloud)

**Naming:** alphabetical animals — `zombie-prod-server-ant`, `zombie-prod-server-bird`, `zombie-prod-server-cat`, ...

**Base:** Debian Trixie, hardened. Tailscale for SSH — no public SSH.
**Connectivity:** Planetscale + Upstash via allowlist.
**Scaling pipeline:** see `AUTO_AGENTS.md`.

### 6. CLI distribution

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
