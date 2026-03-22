# Security Model

This is the primary security entrypoint for UseZombie.

It explains what we protect, why specific controls exist, and where to configure each software boundary.

## Security Objectives

1. Prevent unauthorized access to control-plane data and secrets.
2. Enforce least privilege between API, worker, callback, and data stores.
3. Fail closed on unsafe runtime posture (missing role separation, degraded queue dependencies).
4. Keep credentials short-lived or environment-scoped, never persisted in artifacts.

## Control Boundaries

1. Network boundary (Tailscale ACL + ingress restrictions)
2. Data boundary (Postgres role separation + vault schema restrictions)
3. Queue boundary (Redis ACL, TLS, role-separated credentials)
4. Identity boundary (Clerk JWT for API and operator login path)
5. GitHub boundary (GitHub App installation tokens, no PAT fallback)

## Software Guides

1. [Postgres Security](docs/security/POSTGRES.md)
2. [Redis Security](docs/security/REDIS.md)
3. [Tailscale Security](docs/security/TAILSCALE.md)
4. [GitHub App Security](docs/security/GITHUB_APP.md)
5. [Clerk Security](docs/security/CLERK.md)

## Core Runtime Guardrails

1. API/worker DB URLs must be explicitly role-separated (`DATABASE_URL_API`, `DATABASE_URL_WORKER`) and must differ.
2. API/worker Redis URLs must be explicitly role-separated (`REDIS_URL_API`, `REDIS_URL_WORKER`) and must differ.
3. Redis role URLs must use `rediss://` in hardened environments.
4. Readiness and doctor paths fail closed when Redis dependency checks fail.
5. Shared env fallback for role URLs is not allowed for hardened mode.

## Verification Entry Points

1. `zombied doctor` for security posture checks.
2. `/readyz` for runtime dependency posture.
3. `playbooks/M3_001_DEPLOY_DEV.md` and `playbooks/M3_002_DEPLOY_PROD.md` for rollout/operator checklists.

## Scope Notes

1. This document is policy-level and intentionally short.
2. Detailed software setup and rationale live in `docs/security/*.md`.
3. Infrastructure-only controls (for example Tailscale ACL deployment) are documented here but enforced outside application code.
