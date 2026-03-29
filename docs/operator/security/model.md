# Security model

## Overview

UseZombie's security model is built on two principles: **fail-closed execution** and **separate runtime boundaries**. If the system cannot verify that a security prerequisite is met, it refuses to proceed.

## Runtime boundaries

The worker and executor are separate security boundaries, each with a distinct trust level and responsibility.

### Worker boundary

The worker is a **trusted** component. It handles:

- Orchestration — claiming runs, managing lifecycle state.
- Credential management — signing GitHub App JWTs, passing API keys via RPC payload.
- Billing enforcement — checking workspace credits before starting a run.
- PR creation — pushing branches and opening pull requests on behalf of the user.

The worker has access to all credentials and can make authenticated requests to GitHub, PostgreSQL, and Redis.

### Executor boundary

The executor is an **untrusted** boundary. It handles:

- Agent code execution — running the NullClaw agent runtime.
- Sandbox policy enforcement — Landlock, cgroups, network deny.
- Resource metering — tracking memory, CPU, and token usage.

The executor does **not** have access to credentials in its environment. The Anthropic API key is passed inside the `startStage` RPC payload and used only for agent API calls. The executor cannot reach the database, Redis, or GitHub directly.

## Fail-closed behavior

If any sandbox prerequisite is missing at startup, the executor refuses to start:

- Landlock not supported by the kernel: executor exits with `UZ-SANDBOX-001`.
- cgroups v2 not available: executor exits with `UZ-SANDBOX-001`.
- Unix socket path not writable: executor exits with a diagnostic error.

There is no degraded mode in production. Either the full sandbox is enforced, or the executor does not run.

## Lease-based liveness

Each run has a lease — a bounded time window during which the worker holds exclusive ownership. If the worker crashes or becomes unresponsive, the lease eventually expires, and the reconciler marks the run as failed (`UZ-EXEC-014`).

Leases prevent zombie runs from consuming resources indefinitely. They also prevent two workers from accidentally working on the same run.

<Warning>
UseZombie v1 is **not mid-session durable**. If a worker crashes during a run, the partial work is lost. The run is marked as failed, and the user must retry. Mid-session checkpointing is planned for v2.
</Warning>

## Spec injection threat model

Specs are user-authored markdown that the agent interprets. The attack surface is an adversarial spec that tries to escape the sandbox or exfiltrate data.

### Defenses in v1

| Layer | Defense |
|-------|---------|
| Spec validation | Server-side validation rejects specs that reference files outside the repository root. Path traversal patterns (`../`, absolute paths) are blocked. |
| Gate loop | Compilation failures from injected code are caught by `make lint`, `make test`, `make build`. The gate loop treats any failure as a signal to self-repair, not to proceed. |
| Sandbox | Even if the agent writes malicious code, the sandbox prevents filesystem escape, network exfiltration, and privilege escalation. |
| Human review | All PRs in v1 require human review before merge. The agent cannot merge its own work. |

### Planned for v2

| Layer | Defense |
|-------|---------|
| Static analysis gates | Semgrep rules and gitleaks scanning on every PR before it is opened. |
| Diff audit | Automated review of the diff for suspicious patterns (credential access, network calls, obfuscation). |
| Sandbox hardening | Firecracker VM isolation replacing Landlock+cgroups for stronger boundary enforcement. |

## Credential injection model

Agent execution requires three credential types, each with distinct injection paths:

### Anthropic API key

- Stored in 1Password vault (`op://ZMB_CD_DEV/anthropic-dev/credential`).
- Deployed to worker via `.env` at deploy time.
- Passed to executor via `StartStage` RPC payload `agent_config.api_key` field.
- Never exposed as executor process environment variable.
- Worker halts startup if absent (`UZ-CRED-001`).

### GitHub App installation token

- Worker signs JWT using GitHub App private key (already in vault).
- Requests short-lived installation token per-run, scoped to target repo.
- Token TTL: 1 hour (GitHub default), refreshed if run approaches 55-minute mark.
- Held in memory only, never persisted to database.
- Token request failure classifies as `policy_deny` (`UZ-CRED-002`).

### Package registry network allowlist

- Phase 1: executor network policy extended from `deny_all` to allowlist for known registries (npmjs.org, pypi.org, crates.io, pkg.go.dev).
- Phase 2: internal package mirror replaces allowlist for supply chain security.
- Network policy is hardcoded to `deny_all` in v1 (`network.zig`). There is no env var to configure it.

## Credential detection signals

| Signal | Meaning |
|--------|---------|
| `UZ-CRED-001` | Anthropic API key missing at worker startup |
| `UZ-CRED-002` | GitHub App installation token request failed |

## RBAC boundary

UseZombie treats role as part of the authenticated identity contract:

- `user` can access normal workspace operations.
- `operator` can manage harnesses, skill secrets, agent scoring views, and workspace scoring configuration.
- `admin` can perform billing lifecycle mutations and API-key-backed administrative operations.

Enforcement is server-side. Hidden CLI commands are not treated as a security boundary. The contract is pinned by live HTTP integration tests that prove `harness`, `skill-secret`, and admin billing-event routes reject under-scoped JWTs with `INSUFFICIENT_ROLE`.

## Provider security details

### Clerk (OIDC/JWKS authentication)

API identity verification uses a centralized OIDC issuer with signed JWT validation. The JWKS verification core (`src/auth/jwks.zig`) is provider-agnostic; runtime routing supports `clerk` and `custom` providers, selected via `OIDC_PROVIDER`.

**Authentication flow (API request):**

1. Extract Bearer token from Authorization header.
2. Split JWT into header, payload, signature (base64url).
3. Reject if `alg` is not `RS256`.
4. Lookup `kid` in cached JWKS (HTTP fetch from active OIDC provider if cache expired, 6hr TTL).
5. Verify RS256 signature against JWKS public key.
6. Parse standard claims: `sub`, `iss`, `aud`, `exp`.
7. Check issuer match, audience match, token not expired.
8. Normalize provider claims into canonical identity contract: `tenant_id`, `workspace_id`, `org_id`, `role`, `audience`, `scopes`.
9. Authorize workspace access: query DB for workspace ownership by `tenant_id`.

**CLI login flow:**

1. CLI calls `POST /v1/auth/sessions` (unauthenticated) and receives `session_id` + `login_url`.
2. Browser opens Clerk sign-in/sign-up UI.
3. After Clerk authentication, the website calls `POST /v1/auth/sessions/<id>/complete` with the Clerk JWT.
4. CLI polls `GET /v1/auth/sessions/<id>` every 2s (5-min TTL) until `status: complete`.
5. JWT stored to `~/.config/zombiectl/credentials.json`.

Session store is ephemeral (in-memory, 5-min TTL, max 64 concurrent). Session IDs are 24-character hex (96 bits CSPRNG).

**Endpoint auth policy:**

| Endpoint | Auth Required |
|---|---|
| `GET /healthz`, `GET /readyz`, `GET /metrics` | No |
| `POST /v1/auth/sessions`, `GET /v1/auth/sessions/:id` | No |
| `POST /v1/auth/sessions/:id/complete` | Yes (JWT) |
| All other `/v1/*` | Yes |

Auth accepts OIDC JWT via the configured provider, or bearer `API_KEY` for issued API keys.

**Role claim normalization:**

JWTs must normalize a `role` claim (`user`, `operator`, `admin`). Accepted sources: Clerk top-level `role`, Clerk `metadata.role`, custom OIDC top-level `role`, custom OIDC nested/namespaced `role`. Unknown roles rejected with 403 `ERR_UNSUPPORTED_ROLE`. API-key auth maps to `admin`.

**Attack prevention:** `alg:none` (CVE-2015-9235), `alg` switching (CVE-2016-5431), missing `kid` bypass, JWKS poisoning, session hijacking (96-bit CSPRNG + 5-min TTL).

**Required configuration:**

| Env Var | Required | Default |
|---|---|---|
| `OIDC_PROVIDER` | No | `clerk` |
| `OIDC_JWKS_URL` | Yes* | Active-provider JWKS URL |
| `OIDC_ISSUER` | No | Active-provider issuer check |
| `OIDC_AUDIENCE` | No | Active-provider audience check |
| `API_KEY` | No | Bearer API key auth when issued |
| `APP_URL` | No | `https://app.usezombie.com` |

### GitHub App (token lifecycle and permissions)

Static PAT usage over-scopes repository access and increases blast radius. UseZombie uses GitHub App installation tokens exclusively — no PAT fallback.

**Token lifecycle:**

- Worker signs JWT using GitHub App private key from vault.
- Requests short-lived installation token per-run, scoped to target repo.
- Token TTL: 1 hour (GitHub default), refreshed if run approaches 55-minute mark.
- Held in memory only, never persisted to database.
- Token request failure classifies as `policy_deny` (`UZ-CRED-002`).

**Permissions model:** Tokens are generated per-run and scoped to the installed repository. PR/push operations succeed only for the installed repository scope. No credential reuse across unrelated repositories.

**Required configuration:**

| Env Var | Required |
|---|---|
| `GITHUB_APP_ID` | Yes |
| `GITHUB_APP_PRIVATE_KEY` | Yes |

### PostgreSQL (connection security and role separation)

Postgres stores control-plane state and encrypted secret material (`vault` schema). Role separation prevents cross-boundary access.

**Role separation:**

| Role | Privilege |
|---|---|
| `api_runtime` | Runtime DML only; no DDL |
| `worker_runtime` | Worker DML scope; no DDL |
| `db_migrator` | Migration control-plane role with DDL authority |
| `ops_readonly_human`, `ops_readonly_agent` | `ops_ro` views only |

API and worker DB URLs must both be present and must differ. Migration/startup policy is fail-closed on unsafe states (partial/failed migration blocks serving).

**Required configuration:**

| Env Var | Purpose |
|---|---|
| `DATABASE_URL_API` | `postgres://api_runtime:...` |
| `DATABASE_URL_WORKER` | `postgres://worker_runtime:...` |
| `DATABASE_URL_MIGRATOR` | `postgres://db_migrator:...` |

TLS required in non-local environments (`sslmode=require`).

**Verification:** `api_runtime` cannot execute DDL. Readonly roles cannot `SELECT` from `vault.secrets`. Startup fails when migration state is partial/failed/unsafe.

### Redis (connection security and ACLs)

Redis is the queue coordination plane. Separate credentials and ACLs prevent cross-role access.

**ACL baseline:**

```text
ACL SETUSER api_user on >api_password ~run_queue +xadd +xgroup +ping
ACL SETUSER worker_user on >worker_password ~run_queue +xreadgroup +xack +xautoclaim +xgroup +ping +xinfo
ACL SETUSER default off
```

API role cannot steal worker messages (`XREADGROUP` denied by ACL). Worker role cannot write outside queue scope. Default user is disabled.

**Required configuration:**

| Env Var | Purpose |
|---|---|
| `REDIS_URL_API` | `rediss://api_user:<pass>@<host>:6379` |
| `REDIS_URL_WORKER` | `rediss://worker_user:<pass>@<host>:6379` |
| `REDIS_TLS_CA_CERT_FILE` | Optional local self-signed CA path |

TLS required in hardened environments (`rediss://`). API/worker URLs must be explicitly set and must differ. Readiness checks verify queue operability, not just socket reachability.

### Tailscale (network access model)

Application-layer auth alone is insufficient. Tailscale provides network-level isolation so data stores and workers are unreachable from the public internet.

**Network model:**

- API and worker nodes run inside a Tailscale tailnet.
- ACLs restrict east-west traffic by role and port.
- Data stores (Postgres, Redis) only allow connections from trusted tailnet sources.
- Ingress exposes only the API endpoint; workers and data stores are not publicly reachable.

For managed Postgres/Redis, IP allowlists are configured to trusted egress/Tailscale paths.

**Verification:** External probes to Postgres/Redis must fail. Workers must not be publicly reachable. API is reachable only through the intended ingress path.

## Free plan billing boundary

The free-plan credit model is fail-closed:

1. Before admitting run, sync, or operator harness execution, the server reconciles workspace billing state and checks remaining free credit.
2. If credit is exhausted, the request is rejected before execution starts with `UZ-BILLING-005`.
3. Runtime usage is deducted only when a run reaches a completed billable outcome.
4. Failed, interrupted, or otherwise incomplete runs do not consume free credit.

This avoids hidden overdraft paths and brittle mid-run termination policies.
