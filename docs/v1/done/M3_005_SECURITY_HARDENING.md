# M3_005: Security Hardening — Service-to-Service Access Control

Date: Mar 4, 2026
Status: DONE
Priority: P0 — v1 requirement
Depends on: M3_000 (secrets/schema separation), M3_004 completion for Redis ACL validation

---

## Execution Position Recommendation

- Start immediately after M3_001/M3_000 for DB-role verification, TLS posture, and doctor checks.
- Run Redis ACL implementation/verification once M3_004 stream transport is in place.
- Treat this as split execution: `M3_005A` (can start now) and `M3_005B` (close after M3_004).

---

## Problem

The current deployment has no service-to-service access controls:

1. Any process with `DATABASE_URL` has full access to all tables including secrets.
2. No network isolation between API, worker, and data stores.
3. Redis has no authentication or ACLs.
4. Worker communicates with GitHub API using static tokens, not scoped installation tokens.
5. No TLS enforcement between services in local or dev environments.

## Solution

Layer security controls across four boundaries:

### 1. Network Layer — Tailscale

| Source | Destination | Allowed | Protocol |
|---|---|---|---|
| zombied API | Postgres | Yes | TCP 5432 (TLS) |
| zombied API | Redis | Yes | TCP 6379 (TLS) |
| zombied worker | Postgres | Yes | TCP 5432 (TLS) |
| zombied worker | Redis | Yes | TCP 6379 (TLS) |
| zombied worker | GitHub API | Yes | HTTPS 443 |
| zombied worker | LLM providers | Yes | HTTPS 443 |
| zombied API | GitHub API | Yes | HTTPS 443 (callback) |
| External | zombied API | Yes | HTTPS 443 (via load balancer) |
| External | Postgres | No | — |
| External | Redis | No | — |
| External | zombied worker | No | — |

**Implementation:**
1. API and worker nodes join the same Tailscale tailnet.
2. Tailscale ACLs restrict which nodes can reach which ports.
3. For managed Postgres (PlanetScale/Neon) and Redis (Upstash): configure IP allowlists to accept only Tailscale exit node IPs.

### 2. Database Layer — Role Separation

Per M3_000, three Postgres roles with least-privilege grants:

| Role | `public` schema | `vault` schema |
|---|---|---|
| `api_accessor` | SELECT, INSERT, UPDATE | No access |
| `worker_accessor` | SELECT, INSERT, UPDATE | SELECT, INSERT, UPDATE |
| `callback_accessor` | No access | SELECT, INSERT, UPDATE |

**Verification script:**
```sql
-- Run as api_accessor — must fail:
SET ROLE api_accessor;
SELECT * FROM vault.secrets; -- ERROR: permission denied

-- Run as worker_accessor — must succeed:
SET ROLE worker_accessor;
SELECT * FROM vault.secrets; -- OK
```

### 3. Redis Layer — ACLs

Redis 7 supports ACLs. Configure per-role access:

```
# API role: can XADD to run_queue, cannot read
ACL SETUSER api_user on >api_password ~run_queue +xadd +xgroup +ping

# Worker role: can XREADGROUP, XACK, XAUTOCLAIM
ACL SETUSER worker_user on >worker_password ~run_queue +xreadgroup +xack +xautoclaim +xgroup +ping +xinfo

# Default user disabled
ACL SETUSER default off
```

For Upstash: ACLs are managed via the Upstash console. Use separate credentials for API and worker.

**Environment variables:**
```dotenv
REDIS_URL_API=rediss://api_user:<pass>@<host>:6379
REDIS_URL_WORKER=rediss://worker_user:<pass>@<host>:6379
```

### 4. GitHub Layer — App Installation Tokens

Per M3_000 B4, replace static PATs with GitHub App installation tokens:

1. Worker generates JWT (RS256 signed with `GITHUB_APP_PRIVATE_KEY`).
2. Exchanges JWT for installation token scoped to the workspace's repository.
3. Token used for git push + PR creation.
4. Token is short-lived (1 hour max), never stored.
5. Token is scoped to specific repository — cannot access other repos.

### 5. Secret Injection Model

Secrets never persist on disk or in VM images:

| Secret | Source | Lifetime | Storage |
|---|---|---|---|
| `ENCRYPTION_MASTER_KEY` | Environment variable | Process lifetime | Memory only |
| GitHub App private key | Environment variable | Process lifetime | Memory only |
| GitHub installation token | Generated per run | 1 hour max | Memory only, discarded after use |
| LLM API key (BYOK) | `vault.secrets` | User-managed | Encrypted BYTEA in Postgres |
| DB credentials | Environment variable | Process lifetime | Memory only |
| Redis credentials | Environment variable | Process lifetime | Memory only |

## Implementation

### New Files

None — this spec coordinates work across M3_000 and M3_004.

### Modified Files

```
docker-compose.yml     — Add Redis ACL config for local dev
.env.example           — Add REDIS_URL_API, REDIS_URL_WORKER
config/redis-acl.conf  — NEW: Redis ACL rules for local dev
```

### Local Dev Security

For local dev, security is relaxed but present:

```yaml
# docker-compose.yml — Redis with ACL config
redis:
  image: redis:7-alpine
  command: redis-server --aclfile /etc/redis/users.acl
  volumes:
    - ./config/redis-acl.conf:/etc/redis/users.acl
```

### Deployment Verification

Add to `zombied doctor`:

1. Check Postgres role grants: `SELECT has_schema_privilege('vault', 'USAGE')`.
2. Check Redis ACL: `ACL WHOAMI` — verify correct user.
3. Check Tailscale status: `tailscale status --json` (optional, non-blocking).

## Acceptance Criteria

1. API process cannot read `vault.secrets`.
2. Worker process cannot be reached from external networks.
3. Redis API user cannot XREADGROUP (cannot steal messages).
4. Redis worker user cannot write arbitrary keys.
5. GitHub tokens are installation-scoped, short-lived, never stored.
6. `zombied doctor` reports security posture.
7. Local dev has Redis ACLs configured (not default user).
8. All Postgres connections use TLS in dev/prod (configurable via `?sslmode=require` in URL).

## Sequence: Control Plane to Worker Access

```
zombiectl → (HTTPS/Clerk JWT) → zombied API
  → (Postgres/TLS/api_accessor) → Postgres
  → (Redis/TLS/api_user) → Redis XADD

zombied worker
  → (Redis/TLS/worker_user) → Redis XREADGROUP
  → (Postgres/TLS/worker_accessor) → Postgres
  → (Postgres/TLS/worker_accessor) → vault.secrets (load BYOK key)
  → (HTTPS/installation token) → GitHub API (push + PR)
  → (HTTPS/BYOK key) → LLM provider (NullClaw agents)
```

## Out of Scope

- mTLS between API and worker (Tailscale provides encrypted tunnels).
- Database row-level security (RLS) — considered for multi-tenant, deferred.
- Secrets rotation automation — manual for v1.
- WAF or DDoS protection for API ingress.
