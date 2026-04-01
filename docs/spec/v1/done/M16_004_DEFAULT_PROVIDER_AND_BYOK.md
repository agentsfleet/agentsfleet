# M16_004: Default Provider and BYOK (Bring Your Own Key)

**Prototype:** v1.0.0
**Milestone:** M16
**Workstream:** 004
**Date:** Mar 30, 2026
**Status:** DONE
**Priority:** P0 — new users cannot run agents without a key; M16_003 soft-fails to empty, blocking day-one use
**Batch:** B2 — depends on M16_003 (credential injection plumbing must exist)
**Depends on:** M16_003 (vault.secrets load/store path and executor injection are live)
**Branch:** feat/m16-004-default-provider-byok

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
3. `WorkerError.CredentialDenied` — no env fallback in any mode

---

## 1.0 Platform Key Reference Table

**Status:** DONE

A new table `platform_llm_keys` holds one active row per provider. It points to the admin
workspace that holds the real key. No key material is stored here — only a reference.

**Dimensions:**
- 1.1 ✅ Add `platform_llm_keys` table to schema migration (`019_platform_llm_keys.sql`); `UNIQUE(provider)` constraint; GRANTs for `worker_runtime` (SELECT) and `api_runtime` (SELECT/INSERT/UPDATE)
- 1.2 ✅ `worker_stage_executor.zig`: `resolveLlmApiKey()` helper — workspace BYOK → platform_llm_keys lookup → `CredentialDenied`; logs `api_key_source={workspace|platform}`
- 1.3 ✅ `ERR_CRED_PLATFORM_KEY_MISSING = "UZ-CRED-003"` added to `errors/codes.zig` with actionable hint
- 1.4 ✅ Always hard-fail with `WorkerError.CredentialDenied` when both workspace and platform key missing — no env fallback in any mode

---

## 2.0 Admin Platform Key Management API

**Status:** DONE

Admin-only endpoints to designate which workspace holds the platform default key for a given
provider. Only users with `admin` role may call these.

`PUT /v1/admin/platform-keys` — upsert the platform default for a provider:
```json
{ "provider": "kimi", "source_workspace_id": "ws_123..." }
```

`DELETE /v1/admin/platform-keys/{provider}` — deactivate (sets `active = false`).

`GET /v1/admin/platform-keys` — list all rows. Never returns key material.

**Dimensions:**
- 2.1 ✅ `admin_platform_keys_http.zig`: validates `provider` (1–32 chars) and `source_workspace_id` (UUIDv7); returns `UZ-REQ-001` for invalid input
- 2.2 ✅ PUT upserts via `INSERT ... ON CONFLICT(provider) DO UPDATE`; DELETE sets `active = false`; GET returns all rows ordered by provider
- 2.3 ✅ Routes wired under `/v1/admin/` behind `requireRole(.admin)` check
- 2.4 ✅ Admin workspace key managed via standard BYOK endpoint — no special write path

---

## 3.0 Workspace Credential API (BYOK)

**Status:** DONE

Users set their own LLM API key via a dedicated HTTP endpoint. The key is stored encrypted in
`vault.secrets` for their workspace under `{provider}_api_key`. Once set, it takes priority over
the platform default in the key resolution order.

Route: `PUT /v1/workspaces/{workspace_id}/credentials/llm`
Request body: `{ "provider": "anthropic", "api_key": "sk-ant-..." }`
Response: `204 No Content`

`DELETE` removes both `{provider}_api_key` and `llm_provider_preference`; subsequent runs fall
back to platform default.

`GET` returns `{"provider": "anthropic", "has_key": true}` — never the key value.

**Dimensions:**
- 3.1 ✅ `workspace_credentials_http.zig`: validates `provider` (1–32 chars) and `api_key` (1–256 chars); returns `UZ-REQ-001` for invalid input
- 3.2 ✅ PUT stores `{provider}_api_key` and `llm_provider_preference` via `crypto_store.store`; DELETE removes both rows from `vault.secrets`
- 3.3 ✅ All three methods behind `workspace_guards.enforce(.{ .minimum_role = .operator })`; GET returns has_key without key material

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 Admin sets `platform_llm_keys` row pointing to their workspace; new user workspace with no own key runs successfully; log shows `api_key_source=platform provider=kimi`
- [x] 4.2 Admin deletes platform key; next run on dependent workspace fails with `CredentialDenied` (not a silent empty key)
- [x] 4.3 Admin updates their own workspace key via BYOK; subsequent run on dependent workspace picks up new key — no migration, no restart
- [x] 4.4 User BYOK: `PUT /v1/workspaces/{id}/credentials/llm` with own key; next run shows `api_key_source=workspace`; platform key not read
- [x] 4.5 User deletes their BYOK key; run falls back to platform default or CredentialDenied
- [x] 4.6 API key never appears in response bodies, logs, or `CorrelationContext`

---

## 5.0 Out of Scope

- UI / zombiectl CLI for key management (deferred to M18)
- Per-workspace rate limiting or quota against the platform key (deferred to M17)
- Multi-provider fallback chain (first available platform key wins — single active row per provider)
- Key usage analytics per workspace (deferred)
- Granular per-workspace platform key grants (all workspaces share the same platform default — no per-workspace grant table in v1)
- Key validation test-connection on PUT (deferred — would require provider SDK call at store time)
