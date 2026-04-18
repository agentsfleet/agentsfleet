# P1_API_CLI_M28_001: Tenant API Key Management — Multi-Key, Rotatable, Self-Service

**Prototype:** v0.18.0
**Milestone:** M28
**Workstream:** 001
**Date:** Apr 18, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — Tenant API keys are the primary programmatic auth mechanism; the current `API_KEY` env var is a bootstrap-only mechanism with no per-tenant isolation, no audit identity, and no self-service. This blocks production onboarding.
**Batch:** B1
**Branch:** feat/m28-api-keys
**Depends on:** M24_001 (REST workspace-scoped routes, done), M18_002 (middleware migration, done)

---

## Overview

**Goal (testable):** A tenant admin calls `POST /v1/api-keys` to mint a named API key; the raw key (`zmb_t_`-prefixed, 68 chars) is returned once; the SHA-256 hash is stored in `core.api_keys`. Subsequent requests with `Authorization: Bearer zmb_t_...` resolve to a `principal` with `.mode = .api_key`, `.role = .admin`, `.user_id`, and `.tenant_id` — enabling per-tenant isolation, per-key revocation, and audit identity without env-var restarts. The `API_KEY` env var remains as a bootstrap fallback but is no longer the primary admin auth path.

**Problem:** Three observable symptoms: (1) A manually inserted admin user has no way to mint an API key — the only mechanism is the `API_KEY` env var which requires a server restart and grants global admin across all tenants. (2) `core.tenants.api_key_hash` is a single hash per tenant (placeholder value like `'managed'` or `'callback'`) — no multi-key, no rotation, no revocation. (3) `principal.user_id` is null when authenticating via `API_KEY` env var — audit logs cannot attribute actions to a specific admin.

**Solution summary:** Introduce a `core.api_keys` table (tenant-scoped, named keys, SHA-256 hash, active/revoked lifecycle). Add a new middleware (`tenant_api_key`) that looks up the key hash from the DB on each request, populating `principal` with user_id + tenant_id from the key row. Expose CRUD endpoints under `/v1/api-keys`. The `external_agents.zig` handler is the reference implementation — this workstream replicates its pattern at the tenant-admin level. The existing `admin_api_key` middleware (env-var based) stays as a bootstrap fallback for initial setup before any keys exist in the DB.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/030_api_keys.sql` | CREATE | New table `core.api_keys` with tenant scoping, key_hash, name, lifecycle |
| `schema/embed.zig` | MODIFY | Add `@embedFile` for 030_api_keys.sql |
| `src/cmd/common.zig` | MODIFY | Add migration entry for 030 |
| `src/auth/middleware/tenant_api_key.zig` | CREATE | DB-backed API key lookup middleware — populates principal with user_id + tenant_id |
| `src/auth/middleware/mod.zig` | MODIFY | Add TenantApiKey to registry, wire into chains |
| `src/auth/middleware/bearer_or_api_key.zig` | MODIFY | Add DB lookup path: if Bearer token starts with `zmb_t_`, delegate to tenant_api_key |
| `src/auth/middleware/admin_api_key.zig` | MODIFY | Document as bootstrap-only; add log warning when env-var key is used |
| `src/http/handlers/api_keys.zig` | CREATE | CRUD handlers: create, list, revoke, delete |
| `src/http/route_table.zig` | MODIFY | Register new routes for /v1/api-keys |
| `src/http/route_table_invoke.zig` | MODIFY | Invoke handlers for api-key routes |
| `src/http/router.zig` | MODIFY | Add route variants for api-key endpoints |
| `src/errors/error_entries.zig` | MODIFY | Add error codes: ERR_APIKEY_NOT_FOUND, ERR_APIKEY_REVOKED, ERR_APIKEY_NAME_TAKEN |
| `src/db/test_fixtures.zig` | MODIFY | Add api_keys table to test fixtures |
| `src/main.zig` | MODIFY | Add test import for new files |
| `src/cmd/serve.zig` | MODIFY | Wire tenant_api_key middleware into registry |

## Applicable Rules

- RULE CTM — Constant-time comparison for secrets (api_key.zig already follows this)
- RULE FLL — Files ≤ 350 lines (new/touched)
- RULE XCC — Cross-compile before commit (Zig)
- RULE FLS — Flush all layers — drain all results (PgQuery wrapper)
- RULE ORP — Cross-layer orphan sweep on every rename/delete
- RULE VLT — Secrets belong in vault, not in entity tables (hash stored, not raw key)
- RULE SGR — SQL migrations must include GRANT statements
- RULE HXX — Handlers go through Hx, not raw common.writeJson
- RULE WAUTH — Every workspace-scoped handler must call authorizeWorkspace
- RULE BIL — Billing and credential endpoints require operator-minimum role
- RULE HGD — Every new handler must follow api_handler_guide.md
- RULE NSQ — Named constants, schema-qualified SQL
- RULE OBS — Every observable state must have a log/event entry
- RULE SCH — Pre-v2.0 schema removal: full teardown, no markers, no DROP (VERSION=0.18.0 < 2.0.0)

---

## Sections (implementation slices)

### §1 — Schema: core.api_keys table

**Status:** PENDING

New table for tenant-scoped API keys with named keys, SHA-256 hash storage, and active/revoked lifecycle. Replaces the single `core.tenants.api_key_hash` column which remains for backward compatibility but is no longer the primary auth path.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `schema/030_api_keys.sql` | DDL applied via migration runner | Table `core.api_keys` exists with columns: id, tenant_id, key_name, key_hash, created_by, active, revoked_at, last_used_at, created_at, updated_at; UNIQUE(tenant_id, key_name); GRANT on api_runtime | integration |
| 1.2 | PENDING | `src/cmd/common.zig` | Migration array after adding entry 030 | `canonicalMigrations()` length incremented; index 29 points at 030 content | unit |
| 1.3 | PENDING | `schema/embed.zig` | Build after adding `@embedFile` | `zig build` compiles; embedded constant accessible | unit |

### §2 — Tenant API Key Middleware

**Status:** PENDING

DB-backed middleware that resolves `zmb_t_`-prefixed Bearer tokens. Looks up SHA-256 hash in `core.api_keys`, populates `principal` with `.mode = .api_key`, `.role = .admin`, `.user_id = row.created_by`, `.tenant_id = row.tenant_id`. Rejects revoked keys. Falls back to env-var `admin_api_key` middleware for non-`zmb_t_` tokens.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `src/auth/middleware/tenant_api_key.zig:execute` | `Authorization: Bearer zmb_t_{valid_key}` + matching row in core.api_keys (active=true) | `.next`, principal = { .mode=.api_key, .role=.admin, .user_id=row.created_by, .tenant_id=row.tenant_id } | unit (mock DB) |
| 2.2 | PENDING | `src/auth/middleware/tenant_api_key.zig:execute` | `Authorization: Bearer zmb_t_{revoked_key}` + row with active=false, revoked_at set | `.short_circuit`, ERR_APIKEY_REVOKED, 401 | unit |
| 2.3 | PENDING | `src/auth/middleware/tenant_api_key.zig:execute` | `Authorization: Bearer zmb_t_{unknown_key}` + no matching row | `.short_circuit`, ERR_UNAUTHORIZED, 401 | unit |
| 2.4 | PENDING | `src/auth/middleware/bearer_or_api_key.zig` | `Authorization: Bearer zmb_t_xxx` | Delegates to tenant_api_key (not env-var rotation) | unit |

### §3 — CRUD Endpoints

**Status:** PENDING

Four endpoints for tenant API key lifecycle: create (mint), list, revoke, delete. Create follows the `external_agents.zig` pattern — generate raw key, store hash, return raw key once. List returns metadata only (never key_hash). Revoke sets active=false + revoked_at. Delete removes the row (hard delete, pre-v2.0 teardown era).

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `POST /v1/api-keys` | `{ key_name: "ci-pipeline", description: "GH Actions" }` + operator JWT | 201: `{ id, key_name, key: "zmb_t_...", created_at }`; core.api_keys row has SHA-256 of raw key | integration |
| 3.2 | PENDING | `GET /v1/api-keys` | operator JWT | 200: `{ items: [{ id, key_name, active, created_at, last_used_at, revoked_at }], total: N }` — no key_hash in response | integration |
| 3.3 | PENDING | `DELETE /v1/api-keys/{id}?action=revoke` | operator JWT + active key id | 200: `{ id, active: false, revoked_at }`; key no longer authenticates | integration |
| 3.4 | PENDING | `DELETE /v1/api-keys/{id}?action=delete` | operator JWT + revoked key id | 204; row removed from core.api_keys | integration |
| 3.5 | PENDING | `POST /v1/api-keys` | `{ key_name: "ci-pipeline" }` twice with same name | Second call: 409 ERR_APIKEY_NAME_TAKEN | integration |
| 3.6 | PENDING | `POST /v1/api-keys` | user-role JWT (not operator/admin) | 403 ERR_FORBIDDEN — RULE BIL | integration |

### §4 — Auth Flow Integration

**Status:** PENDING

Wire tenant_api_key into the middleware chain. The `bearer_or_api_key` middleware gains a third auth path: (1) JWT/OIDC → OIDC verifier, (2) `zmb_t_` prefix → tenant_api_key DB lookup, (3) other Bearer → env-var admin_api_key rotation (bootstrap fallback). Update serve.zig to pass pool to middleware registry for DB access. Update `admin_api_key` middleware to emit a structured log on every use (audit trail for env-var auth).

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | Full middleware chain | `Bearer zmb_t_{valid}` → protected endpoint | Principal populated; endpoint handler receives principal.user_id and principal.tenant_id non-null | integration |
| 4.2 | PENDING | Full middleware chain | `Bearer {env-var-key}` → protected endpoint | Principal populated with .user_id=null, .tenant_id=null; log warning emitted | integration |
| 4.3 | PENDING | `src/cmd/serve.zig` | Startup with API_KEY env var set | Log: "startup.bootstrap_api_key status=warning message=env-var API_KEY is bootstrap-only; migrate to tenant API keys" | integration |

---

## Interfaces

**Status:** PENDING

### Public Functions

```
zig
// src/auth/middleware/tenant_api_key.zig
pub const TenantApiKey = struct {
    pub fn middleware(self: *TenantApiKey) chain.Middleware(AuthCtx)
    pub fn execute(self: *TenantApiKey, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome
};

// src/http/handlers/api_keys.zig
pub fn innerCreateApiKey(hx: Hx, req: *httpz.Request) void
pub fn innerListApiKeys(hx: Hx) void
pub fn innerRevokeApiKey(hx: Hx, key_id: []const u8) void
pub fn innerDeleteApiKey(hx: Hx, key_id: []const u8) void
```

### Input Contracts

**POST /v1/api-keys**

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| key_name | string | 1–64 chars, alphanumeric + hyphens + underscores | `"ci-pipeline"` |
| description | string | 0–256 chars, optional | `"GH Actions deploys"` |

**DELETE /v1/api-keys/{id}?action={revoke|delete}**

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| id | path param | UUIDv7 | `"0195b4ba-..."` |
| action | query param | `"revoke"` or `"delete"` | `"revoke"` |

### Output Contracts

**POST /v1/api-keys → 201**

| Field | Type | When | Example |
|-------|------|------|---------|
| id | string | always | UUIDv7 |
| key_name | string | always | `"ci-pipeline"` |
| key | string | creation only (never retrievable again) | `"zmb_t_a1b2c3...64hex"` |
| created_at | i64 | always | millis timestamp |

**GET /v1/api-keys → 200**

| Field | Type | When | Example |
|-------|------|------|---------|
| items | array | always | `[{ id, key_name, active, created_at, last_used_at, revoked_at }]` |
| total | i32 | always | `3` |

**DELETE /v1/api-keys/{id}?action=revoke → 200**

| Field | Type | When | Example |
|-------|------|------|---------|
| id | string | always | UUIDv7 |
| active | bool | always | `false` |
| revoked_at | i64 | always | millis timestamp |

**DELETE /v1/api-keys/{id}?action=delete → 204** (no body)

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Key name already exists for this tenant | 409 with ERR_APIKEY_NAME_TAKEN | `{"error_code":"UZ-APIKEY-001","message":"Key name already exists in this tenant"}` |
| Key not found | 404 with ERR_APIKEY_NOT_FOUND | `{"error_code":"UZ-APIKEY-002","message":"API key not found"}` |
| Key is revoked (auth attempt) | 401 with ERR_APIKEY_REVOKED | `{"error_code":"UZ-APIKEY-003","message":"API key has been revoked"}` |
| Insufficient role (user role) | 403 with ERR_FORBIDDEN | `{"error_code":"UZ-AUTH-003","message":"Workspace access denied"}` |
| DB unavailable | 503 with ERR_INTERNAL | `{"error_code":"UZ-INTERNAL-001","message":"Database unavailable"}` |
| Malformed input (empty/oversized key_name) | 400 with ERR_INVALID_REQUEST | `{"error_code":"UZ-BAD-001","message":"key_name must be 1–64 chars"}` |
| Delete on active key (action=delete on non-revoked) | 409 with ERR_INVALID_REQUEST | `{"error_code":"UZ-BAD-001","message":"Active key must be revoked before deletion"}` |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| DB pool exhausted during key lookup | High concurrency | Middleware returns .short_circuit with 503 | 503 UZ-INTERNAL-001 |
| Key hash collision (SHA-256) | Astronomically improbable | UNIQUE constraint on key_hash rejects INSERT | 500 on create (should never happen) |
| Raw key not stored by caller | Normal — shown once | No recovery path; must revoke and create new key | New key needed |
| Env-var API_KEY matches a `zmb_t_` format | Misconfiguration | Env-var middleware runs first (current chain order); admin_api_key grants global admin | Works but bypasses tenant scoping — log warning |
| Middleware pool connection leak | Missing release in error path | Connection not returned to pool; gradual exhaustion | Intermittent 503s |
| Race: key revoked between lookup and handler execution | Revocation during request | Request succeeds (TOCTOU acceptable for revocation — not a security boundary) | Request completes normally; next request with same key fails |

**Platform constraints:**
- `zmb_t_` prefix must be distinct from `zmb_` (external_agents) to route middleware correctly at parse time — no DB query needed for routing.
- PgQuery wrapper (RULE FLS) auto-drains — no manual drain() needed.

---

## Implementation Constraints (Enforceable)

| Constraint | How to verify |
|-----------|---------------|
| Raw API key shown exactly once, never stored | grep for raw key storage — only key_hash column exists; integration test proves retrieval endpoint omits it |
| Constant-time key comparison (RULE CTM) | Uses api_key.constantTimeEql (existing) — unit test already covers |
| All handlers use Hx (RULE HXX) | grep for `common.writeJson` or `common.errorResponse` direct calls in api_keys.zig — zero matches |
| Schema-qualified SQL (RULE NSQ) | grep for unqualified table references in api_keys.zig — zero matches |
| Files ≤ 350 lines (RULE FLL) | `wc -l` on every new/touched .zig file |
| Cross-compile (RULE XCC) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| GRANT on new table (RULE SGR) | 030_api_keys.sql includes GRANT SELECT, INSERT, UPDATE, DELETE ON core.api_keys TO api_runtime |
| Key prefix distinct from external_agents | `zmb_t_` prefix — grep confirms no overlap with `zmb_` routing logic |
| Zero concurrent pool connections per request (RULE CNX) | Handler acquires one connection, defers release |

---

## Invariants (Hard Guardrails)

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | Raw API key never stored in any table or log | comptime: key generation function returns allocated string; handler writes only hash to DB — no write-to-DB function accepts raw key |
| 2 | `zmb_t_` prefix is exactly 5 chars | `KEY_PREFIX = "zmb_t_"` as named constant; `comptime { std.debug.assert(KEY_PREFIX.len == 6); }` (5 chars + underscore) |
| 3 | Every key row has non-null tenant_id and created_by | NOT NULL on both columns in DDL |
| 4 | key_hash is globally unique | UNIQUE constraint on key_hash column |

---

## Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| generateTenantApiKey produces 68-char zmb_t_ key | 3.1 | `api_keys.zig:generateTenantApiKey` | alloc | `"zmb_t_" + 64 hex chars`, total length 69 |
| sha256Hex of generated key matches stored hash | 3.1 | `api_key.zig:sha256Hex` | raw key | hex matches key_hash written to DB |
| isTenantApiKey returns true for zmb_t_ prefix | 2.4 | `tenant_api_key.zig:isTenantApiKey` | `"zmb_t_abc"` | true |
| isTenantApiKey returns false for zmb_ prefix | 2.4 | `tenant_api_key.zig:isTenantApiKey` | `"zmb_abc"` | false |
| key_name validation rejects empty | 3.5 | `api_keys.zig:validateKeyName` | `""` | error |
| key_name validation rejects >64 chars | 3.5 | `api_keys.zig:validateKeyName` | 65-char string | error |
| key_name validation rejects special chars | 3.5 | `api_keys.zig:validateKeyName` | `"key with spaces"` | error |
| key_name validation accepts hyphens/underscores | 3.1 | `api_keys.zig:validateKeyName` | `"ci-pipeline_v2"` | ok |

### Integration Tests

| Test name | Dim | Infra needed | Input | Expected |
|-----------|-----|-------------|-------|----------|
| Create + auth with tenant API key | 4.1 | DB | POST /v1/api-keys → get raw key → Bearer to protected endpoint | Principal has user_id + tenant_id |
| List keys omits key_hash | 3.2 | DB | Create key → GET /v1/api-keys | Response has no key_hash field |
| Revoke key blocks auth | 3.3 | DB | Create key → revoke → auth with same key | 401 ERR_APIKEY_REVOKED |
| Delete key removes row | 3.4 | DB | Create → revoke → delete → SELECT from core.api_keys | 0 rows |
| Duplicate key_name rejected | 3.5 | DB | POST /v1/api-keys twice with same key_name | Second: 409 |
| User role blocked from key creation | 3.6 | DB | POST /v1/api-keys with user-role JWT | 403 |
| Env-var key still works (bootstrap) | 4.2 | DB + env | Bearer with env-var key | 200 + log warning |
| last_used_at updated on auth | 2.1 | DB | Auth with key → SELECT last_used_at | Non-null, recent |

### Negative Tests (error paths that MUST fail)

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| Auth with revoked key | 2.2 | Bearer zmb_t_{revoked} | 401 ERR_APIKEY_REVOKED |
| Auth with unknown key | 2.3 | Bearer zmb_t_{unknown} | 401 ERR_UNAUTHORIZED |
| Create key with no auth | 3.6 | POST /v1/api-keys with no Authorization | 401 |
| Delete active key without revoke | 3.4 | DELETE /v1/api-keys/{id}?action=delete on active key | 409 ERR_INVALID_REQUEST |
| Revoke already-revoked key | 3.3 | DELETE /v1/api-keys/{id}?action=revoke on revoked key | 404 ERR_APIKEY_NOT_FOUND |

### Edge Case Tests (boundary values)

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| 64-char key_name (max) | 3.5 | 64-char alphanumeric name | 201 created |
| 65-char key_name (over) | 3.5 | 65-char name | 400 ERR_INVALID_REQUEST |
| 1-char key_name (min) | 3.5 | `"x"` | 201 created |
| 256-char description (max) | 3.1 | 256-char description | 201 created |
| 257-char description (over) | 3.1 | 257-char description | 400 ERR_INVALID_REQUEST |
| Key with null description | 3.1 | description omitted | 201 created, description="" |

### Regression Tests (pre-existing behavior that MUST NOT change)

| Test name | What it guards | File |
|-----------|---------------|------|
| external_agents zmb_ keys still work | zmb_ prefix routing unchanged | `external_agents.zig` |
| admin_api_key middleware still passes | Env-var bootstrap auth not broken | `admin_api_key.zig` |
| bearer_or_api_key JWT path still works | OIDC auth unaffected | `bearer_or_api_key.zig` |
| workspace_guards owner override still works | Creator auto-promote to operator | `workspace_guards.zig` |
| UZ-AUTH-002 stays 401 | Auth error status code | `error_registry.zig` |

### Leak Detection Tests

| Test name | Dim | What it proves |
|-----------|-----|---------------|
| TenantApiKey middleware alloc/free | 2.1 | std.testing.allocator detects zero leaks for execute path |
| api_keys handler alloc/free | 3.1 | std.testing.allocator detects zero leaks for create path |
| key generation alloc/free | 3.1 | std.testing.allocator detects zero leaks for generateTenantApiKey |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| Mint named API key via POST /v1/api-keys | Create + auth with tenant API key | integration |
| Raw key shown once, hash stored | List keys omits key_hash | integration |
| zmb_t_ Bearer resolves to principal with user_id + tenant_id | Auth with tenant API key | integration |
| Per-tenant isolation (key scoped to tenant) | Duplicate key_name across tenants succeeds | integration |
| Per-key revocation | Revoke key blocks auth | integration |
| API_KEY env var still works as bootstrap fallback | Env-var key still works | integration |
| user-role cannot create keys | User role blocked | integration |

---

## Execution Plan (Ordered)

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | Schema: create `030_api_keys.sql`, update `embed.zig` + `common.zig` migration array | `zig build` compiles |
| 2 | Error codes: add ERR_APIKEY_NOT_FOUND, ERR_APIKEY_REVOKED, ERR_APIKEY_NAME_TAKEN to error_entries.zig | `zig build test` (error_registry unit tests pass) |
| 3 | Tenant API key middleware: `tenant_api_key.zig` — DB lookup, principal population, revoked rejection | Unit tests pass |
| 4 | Wire middleware: update `bearer_or_api_key.zig` to route `zmb_t_` → tenant_api_key; update `mod.zig` registry; update `serve.zig` | `zig build` compiles; existing auth tests pass |
| 5 | CRUD handlers: `api_keys.zig` — create, list, revoke, delete | Unit tests pass |
| 6 | Routes: register `/v1/api-keys` in router.zig + route_table.zig + route_table_invoke.zig | `zig build` compiles |
| 7 | Integration tests: full auth flow, revoke, delete, duplicate name, role gate | `make test-integration` |
| 8 | Bootstrap log warning: add structured warning in `admin_api_key.zig` when env-var key matches | Unit test for log emission |
| 9 | Dead code sweep: verify no orphaned references to old single-hash auth path | `grep -rn api_key_hash src/` — only schema references remain |
| 10 | Cross-compile + lint + gitleaks | `zig build -Dtarget=x86_64-linux && make lint && gitleaks detect` |

---

## Acceptance Criteria

**Status:** PENDING

- [ ] `POST /v1/api-keys` returns a `zmb_t_`-prefixed raw key that authenticates on subsequent requests — verify: `make test-integration`
- [ ] Authenticated principal has `.user_id` and `.tenant_id` populated (not null) — verify: integration test asserts both fields
- [ ] `GET /v1/api-keys` never returns `key_hash` — verify: integration test asserts field absent
- [ ] Revoked key returns 401 on auth attempt — verify: integration test
- [ ] Duplicate key_name within same tenant returns 409 — verify: integration test
- [ ] `API_KEY` env var still works as bootstrap fallback — verify: existing admin_api_key tests pass
- [ ] All new files ≤ 350 lines — verify: `wc -l` on each new .zig file
- [ ] Cross-compile succeeds — verify: `zig build -Dtarget=x86_64-linux`
- [ ] `make lint` passes — verify: `make lint`

---

## Eval Commands (Post-Implementation Verification)

**Status:** PENDING

```bash
# E1: Zig build
zig build 2>&1 | head -5; echo "zig_build=$?"

# E2: Unit tests
make test 2>&1 | tail -10

# E3: Integration tests
make test-integration 2>&1 | tail -10

# E4: Dead code sweep — zero orphaned references to deleted/renamed symbols
grep -rn "api_key_hash" src/ --include="*.zig" | head -5
echo "E4: api_key_hash in src/ should only appear in test fixtures"

# E5: Memory leak test (std.testing.allocator detects leaks)
zig build test 2>&1 | grep -i "leak" | head -5
echo "E5: leak check (empty = pass)"

# E6: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E7: Lint
make lint 2>&1 | grep -E "✓|FAIL"

# E8: Gitleaks — no secrets in diff
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E9: 350-line gate (exempts .md files — RULE FLL)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'

# E10: PG drain check
make check-pg-drain 2>&1 | tail -3
```

---

## Dead Code Sweep

**Status:** PENDING

**1. Orphaned files — must be deleted from disk and git.**

N/A — no files deleted. `core.tenants.api_key_hash` column remains in schema for backward compatibility (pre-v2.0 teardown era).

**2. Orphaned references — zero remaining imports or uses.**

N/A — no symbols renamed or deleted. New symbols added only.

**3. main.zig test discovery — update imports.**

Add `_ = @import("auth/middleware/tenant_api_key.zig");` and `_ = @import("http/handlers/api_keys.zig");` to `src/main.zig`.

---

## Verification Evidence

**Status:** PENDING

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Leak detection | `zig build test \| grep leak` | | |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| Gitleaks | `gitleaks detect` | | |
| 350L gate | `wc -l` (exempts .md — RULE FLL) | | |
| Dead code sweep | `grep -rn api_key_hash src/` | | |

---

## Out of Scope

- UI for API key management (Settings page) — deferred to a follow-up UI milestone
- CLI `zombiectl api-keys` subcommand — deferred to CLI milestone
- Key usage metrics/auditing dashboard — deferred to observability milestone
- Rate limiting per key — deferred to reliability milestone
- Key expiration (auto-revoke after TTL) — deferred; keys are manually revoked for v1
- Automatic rotation (generate replacement before revoking old) — deferred to v2
- Migration of `core.tenants.api_key_hash` values into `core.api_keys` rows — backward compat, no migration needed
- `zombiectl` commands for key CRUD — CLI milestone
