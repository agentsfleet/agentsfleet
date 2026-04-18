# P1_API_M28_002: Tenant API Key Management — Multi-Key, Rotatable, Self-Service

> Scope note: this milestone covers the API and middleware only. The `zombiectl api-keys` subcommand is explicitly deferred to a future CLI milestone (see Out of Scope). The filename intentionally omits `_CLI` to reflect that.

**Prototype:** v0.18.0
**Milestone:** M28
**Workstream:** 002
**Date:** Apr 18, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — Tenant API keys are the primary programmatic auth mechanism; the current `API_KEY` env var is a bootstrap-only mechanism with no per-tenant isolation, no audit identity, and no self-service. This blocks production onboarding.
**Batch:** B1
**Branch:** feat/m28-api-keys
**Depends on:** M24_001 (REST workspace-scoped routes, done), M18_002 (middleware migration, done)

---

## Overview

**Goal (testable):** A tenant admin calls `POST /v1/api-keys` to mint a named API key; the raw key (`zmb_t_`-prefixed, 70 chars = 6-char prefix + 64 hex chars) is returned once; the SHA-256 hash is stored in `core.api_keys`. Subsequent requests with `Authorization: Bearer zmb_t_...` resolve to a `principal` with `.mode = .api_key`, `.role = .admin`, `.user_id`, and `.tenant_id` — enabling per-tenant isolation, per-key revocation, and audit identity without env-var restarts. The `API_KEY` env var remains as a bootstrap fallback but is no longer the primary admin auth path.

**Problem:** Three observable symptoms: (1) A manually inserted admin user has no way to mint an API key — the only mechanism is the `API_KEY` env var which requires a server restart and grants global admin across all tenants. (2) `core.tenants.api_key_hash` is a single hash per tenant (placeholder value like `'managed'` or `'callback'`) — no multi-key, no rotation, no revocation. (3) `principal.user_id` is null when authenticating via `API_KEY` env var — audit logs cannot attribute actions to a specific admin.

**Solution summary:** Introduce a `core.api_keys` table (tenant-scoped, named keys, SHA-256 hash, active/revoked lifecycle). Add a new middleware (`tenant_api_key`) that looks up the key hash from the DB on each request, populating `principal` with user_id + tenant_id from the key row. Expose CRUD endpoints under `/v1/api-keys`. The existing `agent_keys.zig` handler (renamed from `external_agents.zig` — see §0 below) is the reference implementation — this workstream replicates its pattern at the tenant-admin level. The existing `admin_api_key` middleware (env-var based) stays as a bootstrap fallback for initial setup before any keys exist in the DB.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/031_api_keys.sql` | CREATE | New table `core.api_keys` with tenant scoping, key_hash, name, lifecycle. (Slot 030 is already taken by `zombie_execution_telemetry`.) |
| `schema/027_core_external_agents.sql` | DELETE | Renamed to `schema/027_core_agent_keys.sql` — table `core.agent_keys` (pre-v2.0 teardown, full file replace) |
| `schema/027_core_agent_keys.sql` | CREATE | Replacement: `core.agent_keys` (renamed from `core.external_agents`) |
| `schema/embed.zig` | MODIFY | Update `@embedFile` for 027 rename; add `@embedFile` for 031_api_keys.sql |
| `src/cmd/common.zig` | MODIFY | Add migration entry for 031 |
| `src/auth/middleware/tenant_api_key.zig` | CREATE | DB-backed API key lookup middleware — populates principal with user_id + tenant_id |
| `src/auth/middleware/mod.zig` | MODIFY | Add TenantApiKey to registry, wire into chains |
| `src/auth/middleware/bearer_or_api_key.zig` | MODIFY | Add DB lookup path: if Bearer token starts with `zmb_t_`, delegate to tenant_api_key |
| `src/auth/middleware/admin_api_key.zig` | MODIFY | Document as bootstrap-only; add log warning when env-var key is used |
| `src/http/handlers/agent_keys.zig` | CREATE | Renamed from `external_agents.zig` — CRUD handlers for workspace agent keys |
| `src/http/handlers/external_agents.zig` | DELETE | Renamed to `agent_keys.zig` |
| `src/http/handlers/api_keys.zig` | CREATE | CRUD handlers: create, list, revoke, delete for tenant API keys |
| `src/http/route_table.zig` | MODIFY | Register new routes for /v1/api-keys |
| `src/http/route_table_invoke.zig` | MODIFY | Invoke handlers for api-key routes |
| `src/http/router.zig` | MODIFY | Add route variants for api-key endpoints |
| `public/openapi.json` | MODIFY | Document /v1/api-keys endpoints (POST create, GET list, PATCH revoke, DELETE) — per api_handler_guide.md §4 the OpenAPI spec is the fifth (and public) route registration |
| `src/errors/error_entries.zig` | MODIFY | Add tenant-API-key codes starting at `UZ-APIKEY-003` (001/002 pre-exist for workspace agent keys): ERR_APIKEY_NOT_FOUND (UZ-APIKEY-003), ERR_APIKEY_REVOKED (UZ-APIKEY-004), ERR_APIKEY_NAME_TAKEN (UZ-APIKEY-005), ERR_APIKEY_ALREADY_REVOKED (UZ-APIKEY-006), ERR_APIKEY_READONLY_FIELD (UZ-APIKEY-007) |
| `src/db/test_fixtures.zig` | MODIFY | Add api_keys table to test fixtures |
| `src/main.zig` | MODIFY | Add test import for new files |
| `src/cmd/serve.zig` | MODIFY | Wire tenant_api_key middleware into registry |

## Authoritative References (must comply)

This spec inherits, and MUST NOT contradict, the following repository documents. When resolving any design question below, consult these first:

- `docs/REST_API_DESIGN_GUIDELINES.md` — resource naming, HTTP method semantics, "avoid verbs in URLs" (§7), list-response envelope (`items` + `total`), error status codes (§10), snake_case + `_at` suffix conventions. This is why revocation is modeled as `PATCH /v1/api-keys/{id}` with `{ "active": false }` (partial update of lifecycle state) rather than a verb in the path like `/revoke`, and why `DELETE /v1/api-keys/{id}` retains pure removal semantics.
- `docs/nostromo/api_handler_guide.md` — handler shape (`innerXxx(hx: Hx, ...)`), `hx.ok` / `hx.fail` envelope usage, route registration in five places (router.zig → route_table.zig → route_table_invoke.zig → openapi.json), and the "NEVER call `common.writeJson` / `common.errorResponse`" rules. All api-key handlers defined below MUST follow this shape verbatim.
- `docs/v2/done/M11_002_HX_HANDLER_CONTEXT.md` — the `Hx` context type, how middleware populates `hx.principal`, and why handlers receive `Hx` by value. The tenant_api_key middleware MUST populate `hx.principal.user_id` and `hx.principal.tenant_id` such that every CRUD handler can authorize and tenant-scope without re-parsing the token.

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

### §0 — Rename: external_agents → agent_keys

**Status:** IN_PROGRESS (code shipped; integration verification 0.1-0.4 pending a live DB run; 0.5 DONE)

Rename `core.external_agents` → `core.agent_keys`, `/v1/workspaces/{ws}/external-agents` → `/v1/workspaces/{ws}/agent-keys`, and `external_agents.zig` → `agent_keys.zig`. The name "external agents" is cryptic in logs and docs — "agent keys" reads naturally ("mint an agent key for your LangGraph automation" vs "create an external agent"). The `zmb_` key prefix and all key semantics remain unchanged.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 0.1 | PENDING | `schema/027_core_agent_keys.sql` | DDL applied | Table `core.agent_keys` exists (not `core.external_agents`); all columns, constraints, indexes, and grants preserved | integration |
| 0.2 | PENDING | `POST /v1/workspaces/{ws}/agent-keys` | Same body as old endpoint | 201 with `zmb_` key; auth with key still works | integration |
| 0.3 | PENDING | `GET /v1/workspaces/{ws}/agent-keys` | operator JWT | 200 with items list; same shape as old response | integration |
| 0.4 | PENDING | `DELETE /v1/workspaces/{ws}/agent-keys/{id}` | operator JWT + agent_id | 204; row removed | integration |
| 0.5 | DONE | Orphan sweep | grep for `external_agents`, `external-agents` across src/ | Zero hits in non-historical files (two archaeology comments remain in `agent_keys.zig:1` header and `router.zig:69` enum comment, same pattern as 027 schema header — intentional) | manual |

### §1 — Schema: core.api_keys table

**Status:** PENDING

New table for tenant-scoped API keys with named keys, SHA-256 hash storage, and active/revoked lifecycle. Replaces the single `core.tenants.api_key_hash` column which remains for backward compatibility but is no longer the primary auth path.

**DDL (authoritative shape — RULE SGR + RULE NSQ):**

```sql
CREATE TABLE core.api_keys (
    id           uuid        PRIMARY KEY,                            -- UUIDv7
    tenant_id    uuid        NOT NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    key_name     text        NOT NULL,
    key_hash     text        NOT NULL,                               -- SHA-256 hex of the raw zmb_t_ key (64 chars; matches the sibling core.agent_keys convention)
    created_by   text        NOT NULL,                               -- OIDC sub of the admin who minted it (opaque provider string; there is no local core.users table)
    active       boolean     NOT NULL DEFAULT true,
    revoked_at   timestamptz NULL,
    last_used_at timestamptz NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT api_keys_name_per_tenant_uniq UNIQUE (tenant_id, key_name),
    CONSTRAINT api_keys_hash_uniq            UNIQUE (key_hash),
    CONSTRAINT api_keys_revoked_iff_inactive CHECK ((active = false) = (revoked_at IS NOT NULL))
);
CREATE INDEX api_keys_tenant_active_idx ON core.api_keys (tenant_id, active);
CREATE INDEX api_keys_key_hash_idx      ON core.api_keys (key_hash) WHERE active = true;

GRANT SELECT, INSERT, UPDATE, DELETE ON core.api_keys TO api_runtime;
```

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `schema/031_api_keys.sql` | DDL applied via migration runner | Table `core.api_keys` exists with columns: id, tenant_id, key_name, key_hash, created_by, active, revoked_at, last_used_at, created_at, updated_at; UNIQUE(tenant_id, key_name); GRANT on api_runtime | integration |
| 1.2 | PENDING | `src/cmd/common.zig` | Migration array after adding entry 031 | `canonicalMigrations()` length incremented; index 30 points at 031 content | unit |
| 1.3 | PENDING | `schema/embed.zig` | Build after adding `@embedFile` | `zig build` compiles; embedded constant accessible | unit |

### §2 — Tenant API Key Middleware

**Status:** PENDING

DB-backed middleware that resolves `zmb_t_`-prefixed Bearer tokens. Looks up SHA-256 hash in `core.api_keys`, populates `principal` with `.mode = .api_key`, `.role = .admin`, `.user_id = row.created_by`, `.tenant_id = row.tenant_id`. Rejects revoked keys. Falls back to env-var `admin_api_key` middleware for non-`zmb_t_` tokens.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `src/auth/middleware/tenant_api_key.zig:execute` | `Authorization: Bearer zmb_t_{valid_key}` + matching row via host `LookupFn` (active=true) | `.next`, principal = { .mode=.api_key, .role=.admin, .user_id=row.created_by, .tenant_id=row.tenant_id } | unit (mock LookupFn) |
| 2.2 | PENDING | `src/auth/middleware/tenant_api_key.zig:execute` | `Authorization: Bearer zmb_t_{revoked_key}` + row with active=false | `.short_circuit`, ERR_APIKEY_REVOKED, 401 | unit (mock LookupFn) |
| 2.3 | PENDING | `src/auth/middleware/tenant_api_key.zig:execute` | `Authorization: Bearer zmb_t_{unknown_key}` + LookupFn returns null | `.short_circuit`, ERR_UNAUTHORIZED, 401 | unit (mock LookupFn) |
| 2.4 | PENDING | `src/auth/middleware/bearer_or_api_key.zig` | `Authorization: Bearer zmb_t_xxx` | Delegates to tenant_api_key (not env-var rotation) | unit |

### §3 — CRUD Endpoints

**Status:** PENDING

Four endpoints for tenant API key lifecycle: create (mint), list, revoke, delete. Create follows the `agent_keys.zig` pattern — generate raw key, store hash, return raw key once. List returns metadata only (never key_hash). Revoke is modeled as `PATCH /v1/api-keys/{id}` with body `{ "active": false }` — a partial lifecycle update per REST_API_DESIGN_GUIDELINES.md §4/§7 (no verbs in URLs; PATCH is the correct method for a state transition). The handler also stamps `revoked_at`. Delete (`DELETE /v1/api-keys/{id}`) is resource removal and requires the key to be revoked first (409 otherwise). Every CRUD handler MUST filter `WHERE tenant_id = principal.tenant_id` on every SQL query — this is the tenant-scoped analog of RULE WAUTH. Omitting the filter would leak cross-tenant key rows; enforced by dim 3.7. All handlers follow `docs/nostromo/api_handler_guide.md` — `innerXxx(hx: Hx, ...)` signature, `hx.ok` / `hx.fail` envelope only; no direct `common.writeJson` or `common.errorResponse` calls.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `POST /v1/api-keys` | `{ key_name: "ci-pipeline", description: "GH Actions" }` + operator JWT | 201: `{ id, key_name, key: "zmb_t_...", created_at }`; core.api_keys row has SHA-256 of raw key | integration |
| 3.2 | PENDING | `GET /v1/api-keys` | operator JWT | 200: `{ items: [{ id, key_name, active, created_at, last_used_at, revoked_at }], total: N }` — no key_hash in response | integration |
| 3.3 | PENDING | `PATCH /v1/api-keys/{id}` body `{ "active": false }` | operator JWT + active key id | 200: `{ id, active: false, revoked_at }`; key no longer authenticates. PATCH (partial lifecycle update) — no verb in URL per REST_API_DESIGN_GUIDELINES.md §7 | integration |
| 3.4 | PENDING | `DELETE /v1/api-keys/{id}` | operator JWT + revoked key id | 204; row removed from core.api_keys | integration |
| 3.5 | PENDING | `POST /v1/api-keys` | `{ key_name: "ci-pipeline" }` twice with same name | Second call: 409 ERR_APIKEY_NAME_TAKEN | integration |
| 3.6 | PENDING | `POST /v1/api-keys` | user-role JWT (not operator/admin) | 403 ERR_FORBIDDEN — RULE BIL | integration |
| 3.7 | PENDING | `GET /v1/api-keys` | operator JWT for Tenant A, with keys existing in Tenant B | 200 with only Tenant A keys; zero Tenant B rows leaked — SQL handlers MUST filter `WHERE tenant_id = $1` on every query (analog of RULE WAUTH for tenant-scoped routes) | integration |

### §4 — Auth Flow Integration

**Status:** PENDING

Wire tenant_api_key into the middleware chain. The `bearer_or_api_key` middleware gains a third auth path: (1) JWT/OIDC → OIDC verifier, (2) `zmb_t_` prefix → tenant_api_key DB lookup, (3) other Bearer → env-var admin_api_key rotation (bootstrap fallback). Update serve.zig to pass pool to middleware registry for DB access. Update `admin_api_key` middleware to emit a structured log on every use (audit trail for env-var auth).

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | Full middleware chain | `Bearer zmb_t_{valid}` → protected endpoint | Principal populated; endpoint handler receives principal.user_id and principal.tenant_id non-null | integration |
| 4.2 | PENDING | Full middleware chain | `Bearer {env-var-key}` → protected endpoint | Principal populated with .user_id=null, .tenant_id=null; log warning emitted | integration |
| 4.3 | PENDING | `src/cmd/serve.zig` | Startup with API_KEY env var set | Log: "startup.bootstrap_api_key status=warning message=env-var API_KEY is bootstrap-only; migrate to tenant API keys" | integration |

### §5 — Observability (RULE OBS)

**Status:** PENDING

Every lifecycle event on `core.api_keys` MUST emit a structured log event so operators can attribute actions to an admin user without reading the DB. Event names, fields, and the metric taxonomy are fixed below so dashboards and alerts don't drift from the implementation.

**Event catalog** (emitted via the existing structured logger; one line per event):

| Event name                                | When                                                              | Fields (all required unless noted)                                                                                  |
|-------------------------------------------|-------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------|
| `api_key.created`                         | Immediately after successful INSERT in `innerCreateApiKey`        | `req_id`, `tenant_id`, `actor_user_id`, `api_key_id`, `key_name`, `key_prefix` (always `"zmb_t_"`)                  |
| `api_key.revoked`                         | On successful PATCH `{active:false}`                              | `req_id`, `tenant_id`, `actor_user_id`, `api_key_id`, `key_name`, `reason` (`"manual"` for this milestone)          |
| `api_key.deleted`                         | On successful DELETE                                              | `req_id`, `tenant_id`, `actor_user_id`, `api_key_id`, `key_name`                                                    |
| `api_key.auth_succeeded`                  | On every successful middleware match                              | `req_id`, `tenant_id`, `api_key_id`, `key_name` *(the `last_used_at_write_enqueued` bool is **DEFERRED** — see Out of Scope)* |
| `api_key.auth_rejected`                   | Any middleware short-circuit (unknown / revoked)                  | `req_id`, `reason` (`"unknown"`\|`"revoked"`), `key_prefix` (never the raw key, never the hash)                      |
| `api_key.last_used_update_failed` *(DEFERRED)* | Deferred UPDATE failed post-response — **DEFERRED to async-stamping workstream (see Out of Scope)** | `req_id`, `tenant_id`, `api_key_id`, `error_class` (Zig error name)                                                 |
| `api_key.bootstrap_env_var_used`          | Env-var `API_KEY` auth matched (from `admin_api_key.zig`)         | `req_id`, `note`=`"migrate to tenant API keys"`                                                                     |

**Metric catalog** (Prometheus-style, all counters unless noted; labels in parens):

- `apikey_created_total{tenant_id}`
- `apikey_revoked_total{tenant_id}`
- `apikey_deleted_total{tenant_id}`
- `apikey_auth_total{tenant_id,result="succeeded|rejected"}`
- `apikey_auth_rejected_total{reason="unknown|revoked"}`
- `apikey_last_used_update_failures_total`
- `apikey_bootstrap_env_var_total` (should trend to zero after migration)

**Hard rules:**
- Event payloads MUST NOT contain the raw key, the hex suffix, or the `key_hash` bytes. Only `api_key_id` (UUID) and `key_prefix` (`"zmb_t_"`) are loggable identifiers.
- Every write path (create / revoke / delete) emits exactly one event on the happy path — no double-emits from the handler and the middleware.
- Every middleware rejection path emits exactly one `api_key.auth_rejected` event with a bounded `reason` enum.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | PENDING | `innerCreateApiKey` log capture | POST /v1/api-keys (happy path) | Exactly one `api_key.created` event with all required fields; zero raw-key leaks in the captured log buffer | integration |
| 5.2 | PENDING | `innerPatchApiKey` log capture | PATCH {active:false} | Exactly one `api_key.revoked` event; zero on an already-revoked PATCH (409 path emits no event) | integration |
| 5.3 | PENDING | `innerDeleteApiKey` log capture | DELETE on revoked key | Exactly one `api_key.deleted` event | integration |
| 5.4 | PENDING | `tenant_api_key.zig` log capture | Bearer unknown key → 401 | Exactly one `api_key.auth_rejected` event with `reason="unknown"`, no `auth_succeeded` event, raw token never logged | integration |
| 5.5 | PENDING | Metrics registry | Run 5.1 + 5.4 in the same test process | `apikey_created_total` == 1, `apikey_auth_rejected_total{reason="unknown"}` == 1, scrape endpoint exposes both | integration |
| 5.6 | PENDING | Secret-hygiene sweep | grep captured log buffer for the raw key suffix used in dim 5.1 | Zero matches — the raw key is never in any emitted event | unit |

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
// Shape follows docs/nostromo/api_handler_guide.md: innerXxx(hx: Hx, ...),
// path params after req, returns void, responses via hx.ok / hx.fail only.
pub fn innerCreateApiKey(hx: Hx, req: *httpz.Request) void
pub fn innerListApiKeys(hx: Hx, req: *httpz.Request) void // req for `page`/`page_size`/`sort` query params
pub fn innerPatchApiKey(hx: Hx, req: *httpz.Request, key_id: []const u8) void // PATCH; body { active: false } → revoke
pub fn innerDeleteApiKey(hx: Hx, key_id: []const u8) void
```

### Input Contracts

**POST /v1/api-keys**

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| key_name | string | 1–64 chars, alphanumeric + hyphens + underscores | `"ci-pipeline"` |
| description | string | 0–256 chars, optional | `"GH Actions deploys"` |

**GET /v1/api-keys** (pagination — per REST_API_DESIGN_GUIDELINES.md §9)

| Field | Type | Constraints | Default | Example |
|-------|------|-------------|---------|---------|
| page | query param | integer ≥ 1 | `1` | `2` |
| page_size | query param | integer, 1 ≤ n ≤ 100 (server cap) | `25` | `50` |
| sort | query param | one of `created_at`, `-created_at`, `key_name`, `-key_name` — deterministic total order required so pagination is stable | `-created_at` | `-created_at` |

Server MUST reject `page_size > 100` with `400 ERR_INVALID_REQUEST`. Sort must be a deterministic total order (tie-break on `id` if the chosen column has duplicates) so paginated results never re-order between requests.

**PATCH /v1/api-keys/{id}** (partial lifecycle update — revoke a key)

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| id | path param | UUIDv7 | `"0195b4ba-..."` |
| active | bool (body) | must be `false` — re-activation is not supported; mint a new key instead. `true` → 409 ERR_APIKEY_READONLY_FIELD. Missing / non-bool / extra top-level fields → 400 ERR_INVALID_REQUEST. | `false` |

**DELETE /v1/api-keys/{id}** (hard delete; requires key to already be revoked)

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| id | path param | UUIDv7 | `"0195b4ba-..."` |

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
| items | array | always | `[{ id, key_name, active, created_at, last_used_at, revoked_at }]` (length ≤ page_size) |
| total | i32 | always | `47` (total rows across all pages, for this tenant) |
| page | i32 | always | `2` (echoes request `page`; defaults to `1`) |
| page_size | i32 | always | `25` (echoes request `page_size`; defaults to `25`, server cap `100`) |

**PATCH /v1/api-keys/{id} → 200**

| Field | Type | When | Example |
|-------|------|------|---------|
| id | string | always | UUIDv7 |
| active | bool | always | `false` |
| revoked_at | i64 | always | millis timestamp |

**DELETE /v1/api-keys/{id} → 204** (no body; requires key to be revoked — returns 409 `ERR_INVALID_REQUEST` with message "Active key must be revoked before deletion" otherwise)

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Key name already exists for this tenant | 409 with ERR_APIKEY_NAME_TAKEN | `{"error_code":"UZ-APIKEY-005","message":"Key name already exists in this tenant"}` |
| Key not found | 404 with ERR_APIKEY_NOT_FOUND | `{"error_code":"UZ-APIKEY-003","message":"API key not found"}` |
| Key is revoked (auth attempt) | 401 with ERR_APIKEY_REVOKED | `{"error_code":"UZ-APIKEY-004","message":"API key has been revoked"}` |
| Revoke on already-revoked key | 409 with ERR_APIKEY_ALREADY_REVOKED | `{"error_code":"UZ-APIKEY-006","message":"API key is already revoked"}` |
| Insufficient role (user role) | 403 with ERR_FORBIDDEN | `{"error_code":"UZ-AUTH-003","message":"Workspace access denied"}` |
| DB unavailable | 503 with ERR_INTERNAL | `{"error_code":"UZ-INTERNAL-001","message":"Database unavailable"}` |
| Malformed input (empty/oversized key_name) | 400 with ERR_INVALID_REQUEST | `{"error_code":"UZ-BAD-001","message":"key_name must be 1–64 chars"}` |
| DELETE on active (non-revoked) key | 409 with ERR_APIKEY_MUST_REVOKE_FIRST | `{"error_code":"UZ-APIKEY-008","message":"Active API key must be revoked before deletion"}` |
| PATCH `{"active": true}` (attempt to re-activate) | 409 with ERR_APIKEY_READONLY_FIELD | `{"error_code":"UZ-APIKEY-007","message":"active cannot be set to true; mint a new key instead"}` |
| PATCH with malformed body (missing `active`, non-bool, extra fields) | 400 with ERR_INVALID_REQUEST | `{"error_code":"UZ-BAD-001","message":"PATCH body must be {\"active\": false}"}` |
| GET with `page_size > 100` | 400 with ERR_INVALID_REQUEST | `{"error_code":"UZ-BAD-001","message":"page_size must be between 1 and 100"}` |

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
| `last_used_at` update failure *(DEFERRED)* | Stamping UPDATE fails (DB blip, pool pressure) — **DEFERRED to async-stamping workstream (see Out of Scope).** In M28_002 the middleware does not write `last_used_at`, so this failure mode cannot occur. | Handler logs structured warning (`api_key.last_used_update_failed`) and returns success to caller — the auth result is never affected by the stamping write. Stamping is best-effort metadata, not on the critical path. | Request still succeeds; observability may miss one touch; metric `apikey_last_used_update_failures_total` increments |

**Platform constraints:**
- `zmb_t_` prefix must be distinct from `zmb_` (agent_keys) to route middleware correctly at parse time — no DB query needed for routing.
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
| GRANT on new table (RULE SGR) | 031_api_keys.sql includes GRANT SELECT, INSERT, UPDATE, DELETE ON core.api_keys TO api_runtime |
| Key prefix distinct from agent_keys | `zmb_t_` prefix — grep confirms no overlap with `zmb_` routing logic |
| Zero concurrent pool connections per request (RULE CNX) | Handler acquires one connection, defers release |

---

## Invariants (Hard Guardrails)

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | Raw API key never stored in any table or log | comptime: key generation function returns allocated string; handler writes only hash to DB — no write-to-DB function accepts raw key |
| 2 | `zmb_t_` prefix is exactly 6 chars | `KEY_PREFIX = "zmb_t_"` as named constant; `comptime { std.debug.assert(KEY_PREFIX.len == 6); }` (z-m-b-underscore-t-underscore) |
| 3 | Every key row has non-null tenant_id and created_by | NOT NULL on both columns in DDL |
| 4 | key_hash is globally unique | UNIQUE constraint on key_hash column |

---

## Test Specification

**Status:** PENDING

### Tier Coverage Matrix (bound to /write-unit-test)

When EXECUTE begins, run `/write-unit-test` with this spec as input. The skill's tier taxonomy maps to sections below — every tier MUST have at least one row. A tier with zero rows is a gap and blocks VERIFY.

| Tier                         | Covered by                                      | Rows |
|------------------------------|-------------------------------------------------|------|
| Happy path                   | Integration Tests                               | 9    |
| Edge cases (boundaries)      | Edge Case Tests                                 | 6    |
| Error paths                  | Negative Tests + Error Contracts table          | 11   |
| Concurrency                  | Concurrency Tests                               | 2    |
| Integration (cross-module)   | Integration Tests                               | 9    |
| Regression                   | Regression Tests                                | 5    |
| Leak detection               | Leak Detection Tests                            | 4    |
| Security                     | Security Tests + Key Entropy & Crackability     | 11   |
| Fidelity (shape conformance) | Fidelity Tests                                  | 3    |
| Constants (no magic strings) | Constants Tests                                 | 2    |
| Performance                  | Performance Tests                               | 2    |
| API contract compliance      | API Contract Tests                              | 2    |
| Spec-claim traceability      | Spec-Claim Tracing                              | 12   |
| Observability emission       | §5 dims 5.1–5.6 (referenced from this section)  | 6    |

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| generateTenantApiKey produces 70-char zmb_t_ key | 3.1 | `api_keys.zig:generateTenantApiKey` | alloc | `"zmb_t_" + 64 hex chars`, total length 70 (6-char prefix + 64 hex) |
| sha256Hex of generated key matches stored hash | 3.1 | `api_key.zig:sha256Hex` | raw key | hex matches key_hash written to DB |
| isTenantApiKey returns true for zmb_t_ prefix | 2.4 | `tenant_api_key.zig:isTenantApiKey` | `"zmb_t_abc"` | true |
| isTenantApiKey returns false for zmb_ prefix | 2.4 | `tenant_api_key.zig:isTenantApiKey` | `"zmb_abc"` | false |
| key_name validation rejects empty | 3.5 | `api_keys.zig:validateKeyName` | `""` | error |
| key_name validation rejects >64 chars | 3.5 | `api_keys.zig:validateKeyName` | 65-char string | error |
| key_name validation rejects special chars | 3.5 | `api_keys.zig:validateKeyName` | `"key with spaces"` | error |
| key_name validation accepts hyphens/underscores | 3.1 | `api_keys.zig:validateKeyName` | `"ci-pipeline_v2"` | ok |
| Key bytes come from `std.crypto.random` (CSPRNG) | 1 (invariant) | `api_keys.zig:generateTenantApiKey` | injected deterministic RNG vs default path | Default path calls `std.crypto.random.bytes(&buf)` (test asserts the function reaches the CSPRNG branch; never `std.rand.DefaultPrng` seeded from timestamp) |

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
| last_used_at updated after response flush *(DEFERRED)* | 2.1 | **DEFERRED to async-stamping workstream (see Out of Scope).** In M28_002 the middleware does not write `last_used_at`; the column stays NULL and `GET /v1/api-keys` returns it as null. Originally specified the in-process post-flush hook design; replaced by the Redis-queue + zombie-worker reaper design captured under Out of Scope. No test in the §3 integration suite asserts a non-null `last_used_at`. |
| Cross-tenant GET isolation | 3.7 | DB | Seed Tenant A + Tenant B each with 2 keys → GET /v1/api-keys with Tenant A operator JWT | 200; `items` contains exactly Tenant A's 2 keys; zero Tenant B rows present. Repeat with Tenant B JWT and assert the mirror — proves `WHERE tenant_id = $1` is on every SELECT. |

### Negative Tests (error paths that MUST fail)

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| Auth with revoked key | 2.2 | Bearer zmb_t_{revoked} | 401 ERR_APIKEY_REVOKED |
| Auth with unknown key | 2.3 | Bearer zmb_t_{unknown} | 401 ERR_UNAUTHORIZED |
| Create key with no auth | 3.6 | POST /v1/api-keys with no Authorization | 401 |
| Delete active key without revoke | 3.4 | DELETE /v1/api-keys/{id} on active key | 409 ERR_INVALID_REQUEST |
| Revoke already-revoked key | 3.3 | PATCH /v1/api-keys/{id} body `{ "active": false }` on revoked key | 409 ERR_APIKEY_ALREADY_REVOKED (resource exists but is in wrong state — not a not-found) |
| PATCH attempts to re-activate a revoked key | 3.3 | PATCH /v1/api-keys/{id} body `{ "active": true }` | 409 ERR_APIKEY_READONLY_FIELD |
| PATCH with missing `active` field | 3.3 | PATCH /v1/api-keys/{id} body `{}` | 400 ERR_INVALID_REQUEST |
| PATCH with non-bool `active` | 3.3 | PATCH /v1/api-keys/{id} body `{ "active": "no" }` | 400 ERR_INVALID_REQUEST |
| PATCH with extra fields | 3.3 | PATCH /v1/api-keys/{id} body `{ "active": false, "key_name": "x" }` | 400 ERR_INVALID_REQUEST |
| Concurrent revoke is idempotent | 3.3 | Two PATCH `{ "active": false }` requests for the same key in parallel | Both complete with deterministic outcome: exactly one returns 200 with the initial `revoked_at`; the other returns 409 ERR_APIKEY_ALREADY_REVOKED. DB state has a single `revoked_at` from the first write. No cross-call corruption, no duplicate audit events. |
| GET with oversized page_size | 3.2 | `GET /v1/api-keys?page_size=500` | 400 ERR_INVALID_REQUEST |

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
| external_agents → agent_keys zmb_ keys still work | zmb_ prefix routing unchanged | `agent_keys.zig` |
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
| Deferred `last_used_at` hook alloc/free *(DEFERRED)* | 2.1 | **DEFERRED to async-stamping workstream (see Out of Scope).** Originally: std.testing.allocator detects zero leaks for the post-flush hook path (including the "hook fires but DB fails" branch); the fresh connection is released on every code path |

### Concurrency Tests

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| Concurrent revoke is idempotent | 3.3 | (see Negative Tests above — duplicated in the matrix, single implementation) | — |
| Concurrent CREATE with identical key_name | 3.5 | Two parallel POST /v1/api-keys with `{ "key_name": "ci-pipeline" }` for the same tenant, fired before either has committed | Exactly one request returns 201; the other returns 409 ERR_APIKEY_NAME_TAKEN. DB ends with exactly one row. UNIQUE(tenant_id, key_name) enforces this even under race; test asserts the second caller sees 409 and never observes a mid-insert 500 |

### Security Tests

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| SQL-injection-shaped key_name | 3.5 / invariant | POST with `{ "key_name": "x'; DROP TABLE core.api_keys; --" }` | 400 ERR_INVALID_REQUEST from `validateKeyName` (special chars rejected *before* SQL). Post-test: `SELECT count(*) FROM core.api_keys` is unchanged. Also verifies that even if validation were bypassed, parameterized queries would still not execute the payload |
| Raw key never logged | §5.6 | Mint a key with a known hex suffix, scrape every structured log emitted during the request, grep for the suffix | Zero matches — raw key appears only in the HTTP 201 response body |
| Constant-time compare used on hash lookup | RULE CTM | Unit: call the key-lookup helper with two keys whose hashes differ at byte 0 vs byte 31; measure relative timing | Relative variance below threshold (proves `std.crypto.timing_safe.eql` or equivalent is used, not plain `std.mem.eql`) |

#### Key Entropy & Crackability Tests (minted keys are truly random)

**Honest framing:** you cannot unit-test that a key "cannot be cracked" — cracking is bounded by cryptanalysis, not tests. What we CAN test are the concrete properties that make brute force infeasible: a true CSPRNG source, high statistical randomness, no collisions at scale, no bit bias, and a keyspace large enough that brute force is physically impossible. The table below tests each of those properties; the final row documents the keyspace math so reviewers see the actual safety bound.

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| Entropy source is the OS CSPRNG | invariant #1 | Call stack inspection / symbol grep on `generateTenantApiKey` | The function reaches `std.crypto.random.bytes(&buf)` (OS-backed `getrandom` / `/dev/urandom`) — NOT `std.rand.DefaultPrng`, NOT `std.rand.Xoshiro256`, NOT anything seeded from `std.time.timestamp()`. Also listed under Unit Tests ("Key bytes come from std.crypto.random") — this row cross-references that test |
| Shannon entropy ≥ 7.9 bits/byte on a large sample | security | Mint 10 000 keys, concatenate the 64-hex-char payloads into a single byte buffer, compute Shannon entropy over it | Entropy ≥ 7.95 bits/byte (theoretical max 8.0). A stuck bit, repeated byte, or reduced-state PRNG would drop this below 7.5 and fail the test |
| Zero collisions at scale mints | security | Mint N keys and collect every `key_hash`. Sample size is optimize-mode-aware via a `comptime` switch: `N = 100_000` in Debug (dev/CI default), `N = 1_000_000` in `ReleaseFast`/`ReleaseSafe`. Runs as part of `make test` — no separate `make test-entropy` target. Debug-mode 100k is statistically meaningful (birthday-bound collision probability at 100k draws over 2²⁵⁶ ≈ 10⁻⁶⁶) while keeping the debug test suite fast (< 5 s on Apple Silicon debug builds, ~1 s in ReleaseFast at 1M). | 0 duplicate hashes at the mode-selected N. Any duplicate is proof of a broken generator, not bad luck |
| Bit-level balance across 10k keys | security | For each of the 256 bit positions across 10 000 sampled keys, count how many keys have that bit set | Every bit position is set in 45–55% of keys (50% ± 5%, tolerance for 10k sample). Catches stuck-bit regressions, masked-byte bugs, or any non-uniform bit distribution |
| Inter-key byte independence | security | Pearson correlation coefficient between byte `i` of key `N` and byte `i` of key `N+1`, across 10 000 consecutive mints, for all 32 byte positions | \|r\| < 0.05 for every position. A time-seeded or LCG-style PRNG would produce visible correlation between adjacent outputs; a CSPRNG produces none |
| Keyspace size bound (documentation test) | invariant #1 | Assert at build time (`comptime`): key entropy = 64 hex chars × 4 bits = 256 bits; keyspace = 2^256 ≈ 1.16×10⁷⁷ | Even at an optimistic 10¹⁵ guesses/second, brute-forcing the keyspace takes ~10⁵⁴ years (≈ 10⁴⁴× the age of the universe). Test is a static assertion on the constants `KEY_HEX_LEN = 64` and `KEY_ENTROPY_BITS = 256` so neither can silently shrink |
| Timing-equivalence: unknown vs revoked auth | security | Issue 1 000 auth requests split evenly between unknown-key and revoked-key inputs, measure response-time distributions | Medians within 3 ms of each other. Proves the middleware does the same DB lookup in both cases (no short-circuit on "unknown" that would leak "this key exists but is revoked" via timing) |
| Brute-force regeneration with seeded PRNGs fails | security / adversarial | **Adversarial test.** Mint one real key via `generateTenantApiKey` at time `T`. Then attempt to regenerate the same 32 random bytes using each of the weak/deterministic PRNGs Zig makes easy to reach for, across a 10-iteration brute-force window around `T`. See pseudocode below. | Zero matches across all iterations. A match means someone swapped the CSPRNG for a seeded PRNG; the test fails loud and blocks merge |

**Pseudocode for the brute-force regeneration test** (referenced by the row above):

```zig
// Spec-level pseudocode — implementer to port to Zig test idioms.
test "minted key cannot be regenerated by seeded PRNGs in a 10-iter brute force" {
    const target = try generateTenantApiKey(std.testing.allocator);
    defer std.testing.allocator.free(target);
    const mint_ns = std.time.nanoTimestamp();

    // Candidate seeds an attacker could plausibly try:
    //   - exact second / millisecond / nanosecond of mint
    //   - ±5 second window around mint (clock skew, log scraping)
    const seed_candidates = [_]u64{
        @intCast(@divTrunc(mint_ns, std.time.ns_per_s)),   // unix seconds
        @intCast(@divTrunc(mint_ns, std.time.ns_per_ms)),  // unix millis
        @intCast(mint_ns),                                  // unix nanos
        @intCast(@divTrunc(mint_ns, std.time.ns_per_s) - 1),
        @intCast(@divTrunc(mint_ns, std.time.ns_per_s) + 1),
        // …extend with a ±5s sweep in the real test
    };

    // PRNG families that a sloppy implementer might reach for:
    const prng_families = .{ std.rand.DefaultPrng, std.rand.Xoshiro256, std.rand.Sfc64 };

    inline for (prng_families) |PrngT| {
        for (seed_candidates) |seed| {
            var i: usize = 0;
            while (i < 10) : (i += 1) {                     // 10-iter brute-force loop
                var prng = PrngT.init(seed +% i);
                var buf: [32]u8 = undefined;
                prng.random().bytes(&buf);
                const candidate = try hexPrefix("zmb_t_", &buf, std.testing.allocator);
                defer std.testing.allocator.free(candidate);
                try std.testing.expect(!std.mem.eql(u8, candidate, target));
            }
        }
    }
}
```

**What this proves:**
- The key does NOT come from `std.rand.DefaultPrng`, `Xoshiro256`, or `Sfc64` seeded with any plausible timestamp-derived value, even across a 10-iteration search per seed.
- Future refactors that accidentally swap `std.crypto.random.bytes` for a seeded PRNG will fail this test loudly, with a reproducible delta against the actual mint.
- Combined with the entropy-source test above ("must reach `std.crypto.random.bytes`"), this closes both directions: the positive assertion *and* the adversarial search.

**What it does NOT prove** (honest limits):
- A sufficiently motivated attacker who controls the machine's entropy pool could still compromise the CSPRNG. That is an OS / kernel / VM-isolation problem, not a test problem.
- Brute-forcing 2²⁵⁶ is infeasible regardless; this test exists to catch regressions, not to exceed the cryptographic bound.

### Fidelity Tests (response shape exactly matches Output Contract)

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| POST response has exactly `{id, key_name, key, created_at}` | 3.1 | Successful create | Response object keys, sorted, equal `["created_at","id","key","key_name"]` — no extras, no `key_hash`, no `tenant_id`, no `created_by` |
| GET response has exactly `{items, total, page, page_size}` at top level, and each item has exactly `{id, key_name, active, created_at, last_used_at, revoked_at}` | 3.2 | Successful list | Top-level key set and per-item key set match verbatim. No `key_hash` at any nesting level |
| PATCH response has exactly `{id, active, revoked_at}` | 3.3 | Successful revoke | Response key set equals `["active","id","revoked_at"]`. `active` is literal `false`. `revoked_at` is non-null |

### Constants Tests

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| No hardcoded `"zmb_t_"` outside the KEY_PREFIX constant site | invariant #2 | `grep -rn '"zmb_t_"' src/` | Exactly one hit: the `KEY_PREFIX` declaration in `api_keys.zig`. Every other site must reference the constant |
| Error codes declared exactly once | RULE NSQ-equivalent | `grep -c '"UZ-APIKEY-' src/errors/error_entries.zig` and `grep -rn '"UZ-APIKEY-' src/` | Each of UZ-APIKEY-003..007 (tenant codes; 001/002 pre-exist for agent keys) declared exactly once in `error_entries.zig`; every other reference is through the registry symbol, not the literal string |

### Performance Tests

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| Auth hot-path p95 budget | RULE perf | `make bench` against `/healthz` behind tenant-api-key auth (local, `API_BENCH_CONCURRENCY=16`, 30s) | p95 ≤ 20 ms, p99 ≤ 50 ms. Delta vs baseline (`admin_api_key` env-var auth) ≤ +5 ms — proves adding the DB lookup did not regress the hot path meaningfully |
| Deferred stamping does not inflate response time *(DEFERRED)* | RULE perf | **DEFERRED to async-stamping workstream (see Out of Scope).** Originally: same bench, measure between two runs: stamping enabled vs `pending_apikey_touch` forced to null | Response-time medians within ±2 ms (the stamping truly happens after flush; the client cannot observe it) |

### API Contract Tests

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| openapi.json paths match router table | RULE HGD | Parse `public/openapi.json`, extract paths; diff against `src/http/router.zig` `match()` arms for `/v1/api-keys*` | Set equality — every documented path has a router arm and vice versa. No drift |
| openapi.json response schemas match Output Contracts | RULE HGD | For each endpoint, assert the OpenAPI response schema's `required` and `properties` sets match the Fidelity Tests expected shapes | Exact match. Fails loud if someone adds a field to the handler without adding it to OpenAPI (or vice versa) |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| Mint named API key via POST /v1/api-keys | Create + auth with tenant API key | integration |
| Raw key shown once, hash stored | List keys omits key_hash | integration |
| zmb_t_ Bearer resolves to principal with user_id + tenant_id | Auth with tenant API key | integration |
| Per-tenant isolation (key scoped to tenant) | Duplicate key_name across tenants succeeds + Cross-tenant GET isolation | integration |
| Per-key revocation | Revoke key blocks auth | integration |
| API_KEY env var still works as bootstrap fallback | Env-var key still works | integration |
| user-role cannot create keys | User role blocked | integration |

---

## Execution Plan (Ordered)

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | Rename: delete `schema/027_core_external_agents.sql`, create `schema/027_core_agent_keys.sql` (table=agent_keys), update `embed.zig` + `common.zig` | `zig build` compiles |
| 2 | Rename: `external_agents.zig` → `agent_keys.zig`, update all route registrations (router, route_table, route_table_invoke), update imports in execute.zig, integration_grants.zig, main.zig | `zig build && zig build test` |
| 3 | Orphan sweep: `grep -rn "external_agents" src/` — zero hits | grep returns empty |
| 4 | Schema: create `031_api_keys.sql`, update `embed.zig` + `common.zig` migration array | `zig build` compiles |
| 5 | Error codes: add ERR_APIKEY_NOT_FOUND, ERR_APIKEY_REVOKED, ERR_APIKEY_NAME_TAKEN, ERR_APIKEY_ALREADY_REVOKED, ERR_APIKEY_READONLY_FIELD to error_entries.zig | `zig build test` (error_registry unit tests pass) |
| 6 | Tenant API key middleware: `tenant_api_key.zig` — DB lookup, principal population, revoked rejection | Unit tests pass |
| 7 | Wire middleware: update `bearer_or_api_key.zig` to route `zmb_t_` → tenant_api_key; update `mod.zig` registry; update `serve.zig` | `zig build` compiles; existing auth tests pass |
| 8 | CRUD handlers: `api_keys.zig` — create, list, revoke, delete | Unit tests pass |
| 9 | Routes: register `/v1/api-keys` in router.zig + route_table.zig + route_table_invoke.zig + document in public/openapi.json (the five places from api_handler_guide.md §4) | `zig build` compiles; `jq '.paths["/v1/api-keys"]' public/openapi.json` returns a non-null object |
| 10 | Integration tests: full auth flow, revoke, delete, duplicate name, role gate | `make test-integration` |
| 11 | Bootstrap log warning: add structured warning in `admin_api_key.zig` when env-var key matches | Unit test for log emission |
| 12 | Dead code sweep: verify no orphaned references to old single-hash auth path | `grep -rn api_key_hash src/` — only schema references remain |
| 13 | Cross-compile + lint + gitleaks | `zig build -Dtarget=x86_64-linux && make lint && gitleaks detect` |

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
grep -rn "external_agents" src/ --include="*.zig" | head -5
echo "E4a: external_agents in src/ (empty = pass)"
grep -rn "external-agents" src/ --include="*.zig" | head -5
echo "E4b: external-agents in src/ (empty = pass)"
grep -rn "api_key_hash" src/ --include="*.zig" | head -5
echo "E4c: api_key_hash in src/ should only appear in test fixtures"

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

| File to delete | Verify deleted |
|---------------|----------------|
| `src/http/handlers/external_agents.zig` | `test ! -f src/http/handlers/external_agents.zig` |
| `schema/027_core_external_agents.sql` | `test ! -f schema/027_core_external_agents.sql` |

**2. Orphaned references — zero remaining imports or uses.**

| Deleted symbol or import | Grep command | Expected |
|-------------------------|--------------|----------|
| `external_agents` | `grep -rn "external_agents" src/ --include="*.zig"` | 0 matches |
| `external-agents` | `grep -rn "external-agents" src/ --include="*.zig"` | 0 matches |
| `external_agents_sql` (embed) | `grep -rn "external_agents_sql" src/ --include="*.zig"` | 0 matches |

**3. main.zig test discovery — update imports.**

Add `_ = @import("auth/middleware/tenant_api_key.zig");` and `_ = @import("http/handlers/api_keys.zig");` to `src/main.zig`. Replace `_ = @import("http/handlers/external_agents.zig");` with `_ = @import("http/handlers/agent_keys.zig");`.

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
- **`last_used_at` async stamping — DEFERRED to a follow-on workstream.** For M28_002, `core.api_keys.last_used_at` is provisioned by DDL but the API never writes to it; the column stays NULL until the stamping workstream ships. **Intended design (do not implement in M28_002):** on successful auth, the middleware enqueues `(api_key_id, touched_at_ns)` onto a Redis queue; a zombie worker process drains, batches, and applies `UPDATE core.api_keys SET last_used_at = ...` asynchronously. This decouples the auth hot path from DB write latency and avoids cross-thread lifecycle shims on the httpz response path. Every spec row that depends on this mechanism is marked **(DEFERRED)** inline and is not required for M28_002 acceptance: §5 event field `last_used_at_write_enqueued`; §5 event `api_key.last_used_update_failed`; Failure Mode row "`last_used_at` update failure"; Leak test "Deferred `last_used_at` hook alloc/free"; Perf test "Deferred stamping does not inflate response time". The `last_used_at` field still appears in `GET /v1/api-keys` responses (§3.2) and returns `null` until the stamping workstream ships.
