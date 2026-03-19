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
        ├── 4.0 Worker infrastructure (OVHCloud + Tailscale)
        ├── 5.0 CI: main-push dev deploy + QA + Discord notify
        └── 6.0 First release tag → evidence capture
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

**DEV service** (reads from `ZMB_CD_DEV`):
```bash
DATABASE_URL     = op://ZMB_CD_DEV/planetscale-dev/connection-string
REDIS_URL        = op://ZMB_CD_DEV/upstash-dev/url
CLERK_SECRET_KEY = op://ZMB_CD_DEV/clerk-dev/secret-key
PORT             = 3000
MIGRATE_ON_START = 0
ENVIRONMENT      = dev
```

**PROD service** (reads from `ZMB_CD_PROD`):
```bash
DATABASE_URL     = op://ZMB_CD_PROD/planetscale-prod/connection-string
REDIS_URL        = op://ZMB_CD_PROD/upstash-prod/url
CLERK_SECRET_KEY = op://ZMB_CD_PROD/clerk-prod/secret-key
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
DATABASE_URL=$(op read "op://ZMB_CD_DEV/planetscale-dev/connection-string")
for f in schema/*.sql; do
  echo "applying $f..."
  psql "$DATABASE_URL" -f "$f"
done
```

Role contract (`schema/002_vault_schema.sql`):
- `api_accessor` — read/write on public tables, no access to `vault.secrets`
- `worker_accessor` — inherits `api_accessor`, read/write on `vault.secrets`
- `callback_accessor` — inherits `vault_accessor`, write on callback table

### 3.2 Redis — ACL Bootstrap

`schema/redis-bootstrap.sh` — idempotent, run once per environment:

```bash
API_PASS=$(op read "op://ZMB_CD_DEV/redis-acl-api-user/credential")
WORKER_PASS=$(op read "op://ZMB_CD_DEV/redis-acl-worker-user/credential")
REDIS_URL=$(op read "op://ZMB_CD_DEV/upstash-dev/url")

redis-cli -u "$REDIS_URL" XGROUP CREATE run_queue workers 0 MKSTREAM
redis-cli -u "$REDIS_URL" ACL SETUSER api_user on ">$API_PASS" "~run_queue" +xadd +xgroup +ping
redis-cli -u "$REDIS_URL" ACL SETUSER worker_user on ">$WORKER_PASS" "~run_queue" +xreadgroup +xack +xautoclaim +xgroup +ping +xinfo
redis-cli -u "$REDIS_URL" ACL SETUSER default off
```

Run for each environment — swap `ZMB_CD_DEV` for `ZMB_CD_PROD` for production.

---

## 4.0 Worker Infrastructure (OVHCloud + Tailscale)

**DEV:** No dedicated workers — workers run as a local process. Skip for DEV.

**PROD only:**

### 4.1 Provision OVHCloud Bare-Metal

Name nodes alphabetically: `zombie-prod-server-ant`, `zombie-prod-server-bird`, ...

Base: Debian Trixie, hardened. See `docs/DEPLOYMENT.md §7` for full spec.

### 4.2 Tailscale

Run once at provision time on each worker node:

```bash
TAILSCALE_AUTHKEY=$(op read "op://ZMB_CD_PROD/tailscale/authkey")
curl -fsSL https://tailscale.com/install.sh | sh
tailscale up --authkey "$TAILSCALE_AUTHKEY" --hostname zombie-prod-server-ant
```

Workers are only reachable via Tailscale — no public SSH.

### 4.3 Deploy zombied Worker to PROD

```bash
WORKER_SSH_KEY=$(op read "op://ZMB_CD_PROD/worker-ssh/private-key")
for host in zombie-prod-server-ant zombie-prod-server-bird; do
  ssh -i <(echo "$WORKER_SSH_KEY") "$host" "cd /opt/zombie && ./deploy.sh"
done
```

`deploy.sh` on each node: pulls `ghcr.io/usezombie/zombied:latest`, restarts `zombied worker`.

---

## 5.0 CI: Main-Push Dev Deploy + QA + Discord Notify

**Trigger:** push to `main`

**Flow:**
```
push to main
  → build dev-latest image → push to GHCR
  → Railway DEV auto-deploys (GitHub integration)
  → wait for /healthz green
  → run QA smoke tests against dev.api.usezombie.com
  → Discord notify: ✅ DEV green / ❌ DEV failed
```

**Workflow file:** `.github/workflows/deploy-dev.yml`

Required 1Password item:
```
DISCORD_WEBHOOK: op://ZMB_CD_PROD/discord-ci-webhook/credential
```

Discord message format:
```
✅ DEV deploy green — {branch} @ {sha}
   /healthz: ok | QA: {N} passed
   → ready for tag release
```

---

## 6.0 First Release Tag — Evidence Capture

```bash
# Bump VERSION, update CHANGELOG, tag
git tag v0.1.0 && git push origin v0.1.0
```

`release.yml` runs in order:
1. `binaries` — cross-compile linux/amd64 + linux/arm64
2. `docker` — build multi-arch image, push to GHCR as `v0.1.0` + `latest`
3. `npm` — publish `zombiectl` to npm
4. `create-release` — GitHub Release with changelog + binary attachments
5. `deploy-prod` — trigger PROD deploy (Railway watches `latest` tag)
6. `verify-prod` — smoke checks + evidence artifact upload

**Evidence artifact** attached to GitHub Release:
- `healthz` + `readyz` JSON snapshots
- `zombied doctor --format=json` output
- Acceptance flow log: `login → workspace add → run → PR created`

---

## 7.0 Reuse for a New Startup

1. Replace `ZMB` vault prefix with `<PROJECT>` everywhere
2. Replace service names (`usezombie`, `zombied`, etc.)
3. Replace domains (`usezombie.com`, `api.usezombie.com`, etc.)
4. Sections 1.0–6.0 are identical — this doc is the full execution playbook

**Pattern:** `M1_001_PLAYBOOK_BOOTSTRAP.md` = human identity + agent key storage. `M2_002_PLAYBOOK_PRIMING_INFRA.md` = agent infrastructure execution.
