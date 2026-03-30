# M16_004: Default Provider and BYOK (Bring Your Own Key)

**Prototype:** v1.0.0
**Milestone:** M16
**Workstream:** 004
**Date:** Mar 30, 2026
**Status:** PENDING
**Priority:** P0 — new users cannot run agents without a key; M16_003 soft-fails to empty, blocking day-one use
**Batch:** B2 — depends on M16_003 (credential injection plumbing must exist)
**Depends on:** M16_003 (vault.secrets load/store path and executor injection are live)

---

## Design Intent

The platform admin (workspace owner, e.g. Kishore) holds a provider key (e.g. Kimi 2.5 free pass)
in their own workspace's `vault.secrets` — the same encrypted store every workspace uses.

New user workspaces are granted read access to that key via a lightweight DB reference
(`platform_llm_keys` table). The admin's key is never copied. Every run that falls through to the
platform default reads live from the admin workspace's `vault.secrets`. If the admin revokes
access or replaces the key, all dependent workspaces see the change on the next run with no
migration needed.

Key resolution order (evaluated in `worker_stage_executor.zig` per run):
1. Workspace's own `vault.secrets` `{provider}_api_key` → BYOK (workspace pays its own provider)
2. Active row in `platform_llm_keys` → read key from the designated admin workspace's `vault.secrets`
3. Executor process env fallback (existing dev-mode path — unchanged)

---

## 1.0 Platform Key Reference Table

**Status:** PENDING

A new table `platform_llm_keys` holds one active row per provider. It points to the admin
workspace that holds the real key. No key material is stored here — only a reference.

```sql
CREATE TABLE platform_llm_keys (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider      TEXT NOT NULL UNIQUE,          -- 'kimi', 'fireworks', 'anthropic', etc.
    source_workspace_id UUID NOT NULL REFERENCES workspaces(workspace_id),
    active        BOOLEAN NOT NULL DEFAULT true,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

**Dimensions:**
- 1.1 PENDING Add `platform_llm_keys` table to schema migration; add `UNIQUE(provider)` constraint; seed one row for the initial provider via migration or admin API
- 1.2 PENDING `worker_stage_executor.zig`: after workspace key NotFound, query `platform_llm_keys WHERE provider = $1 AND active = true`; if found, call `crypto_store.load(conn, alloc, source_workspace_id, "{provider}_api_key")`; log `api_key_source=platform provider={s} source_workspace_id={s}`
- 1.3 PENDING Add `ERR_CRED_PLATFORM_KEY_MISSING = "UZ-CRED-003"` to `errors/codes.zig`; hint: "No active platform key for provider. Admin must set one via PUT /v1/admin/platform-keys or the workspace must add its own BYOK key."
- 1.4 PENDING Hard-fail the run with `WorkerError.CredentialDenied` if both workspace key and platform key are missing (current soft-fall-to-env stays only for dev mode, gated on `APP_ENV != production`)

---

## 2.0 Admin Platform Key Management API

**Status:** PENDING

Admin-only endpoints to designate which workspace holds the platform default key for a given
provider. Only users with `admin` role (checked via `ERR_INSUFFICIENT_ROLE`) may call these.

`PUT /v1/admin/platform-keys` — upsert the platform default for a provider:
```json
{ "provider": "kimi", "source_workspace_id": "ws_123..." }
```

`DELETE /v1/admin/platform-keys/{provider}` — deactivate (sets `active = false`); all dependent
workspaces immediately fall through to BYOK-only or dev fallback on next run.

`GET /v1/admin/platform-keys` — list all rows (active and inactive). Response includes
`provider`, `source_workspace_id`, `active`, `updated_at`. Never returns key material.

**Dimensions:**
- 2.1 PENDING New handler `admin_platform_keys_http.zig`: validate `provider` non-empty, `source_workspace_id` is a valid UUID referencing an existing workspace; return `UZ-REQ-001` for invalid input
- 2.2 PENDING PUT upserts `platform_llm_keys` (INSERT ... ON CONFLICT(provider) DO UPDATE); DELETE sets `active = false`; GET returns all rows ordered by provider
- 2.3 PENDING Wire routes under `/v1/admin/` behind admin role check; return `UZ-AUTH-009` if caller lacks admin role
- 2.4 PENDING The admin workspace's key itself is managed via the standard BYOK endpoint (`PUT /v1/workspaces/{id}/credentials/llm`) — no special admin key write path

---

## 3.0 Workspace Credential API (BYOK)

**Status:** PENDING

Users set their own LLM API key via a dedicated HTTP endpoint. The key is stored encrypted in
`vault.secrets` for their workspace under `{provider}_api_key`. Once set, it takes priority over
the platform default in the key resolution order.

Route: `PUT /v1/workspaces/{workspace_id}/credentials/llm`

Request body: `{ "provider": "anthropic", "api_key": "sk-ant-..." }`
Response: `204 No Content`

`DELETE /v1/workspaces/{workspace_id}/credentials/llm` removes the key; subsequent runs fall
back to platform default.

Allowed `provider` values (validated server-side): any non-empty string up to 32 chars.
The constraint is loose here — provider validation is NullClaw's job at execution time.

**Dimensions:**
- 3.1 PENDING New handler `workspace_credentials_http.zig`: validate `provider` (non-empty, max 32 chars) and `api_key` (non-empty, max 256 chars); return `UZ-REQ-001` for invalid input
- 3.2 PENDING PUT calls `crypto_store.store(conn, alloc, workspace_id, "{provider}_api_key", api_key)` and `crypto_store.store(..., "llm_provider_preference", provider)`; DELETE removes both entries
- 3.3 PENDING Wire `PUT` and `DELETE` behind workspace-owner auth (`ERR_FORBIDDEN` if caller's workspace_id != path param); `GET /v1/workspaces/{id}/credentials/llm` returns `{"provider": "anthropic", "has_key": true}` — never the key value itself

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 Admin sets `platform_llm_keys` row pointing to their workspace; new user workspace with no own key runs successfully; log shows `api_key_source=platform provider=kimi`
- [ ] 4.2 Admin deletes platform key (`DELETE /v1/admin/platform-keys/kimi`); next run on dependent workspace fails with `CredentialDenied` (not a silent empty key)
- [ ] 4.3 Admin updates their own workspace key via BYOK (`PUT /v1/workspaces/admin_ws/credentials/llm`); subsequent run on dependent workspace picks up new key — no migration, no restart
- [ ] 4.4 User BYOK: `PUT /v1/workspaces/{id}/credentials/llm` with own Anthropic key; next run shows `api_key_source=workspace`; platform key not read
- [ ] 4.5 User deletes their BYOK key; run falls back to platform default
- [ ] 4.6 API key never appears in response bodies, logs, or `CorrelationContext`

---

## 5.0 Out of Scope

- UI / zombiectl CLI for key management (deferred to M18)
- Per-workspace rate limiting or quota against the platform key (deferred to M17)
- Multi-provider fallback chain (first available platform key wins — single active row per provider)
- Key usage analytics per workspace (deferred)
- Granular per-workspace platform key grants (all workspaces share the same platform default — no per-workspace grant table in v1)
