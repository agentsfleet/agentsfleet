# M45_001: Credential Vault — Structured `{host, api_token}` Schema + API + UI

**Prototype:** v2.0.0
**Milestone:** M45
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — launch-blocking. M41 Context Layering depends on `secrets_map` being a structured object, not a single string. M43 Webhook Ingest depends on `secrets.github.webhook_secret` shape. The current `--value` (single-string) credential storage is the chokepoint that prevents both.
**Categories:** API, CLI, UI
**Batch:** B1 — parallel with M40, M41, M42, M43, M44.
**Branch:** feat/m45-vault-structured (to be created)
**Depends on:** none structurally — this is a schema migration with a dual-read window.

**Canonical architecture:** `docs/ARCHITECHTURE.md` §10 (capability table — credential vault row), §12 step 2-4 (credential add flow).

---

## Implementing agent — read these first

1. `zombiectl/src/commands/zombie_credential.js` — current `--value` single-string CLI shape
2. `samples/platform-ops/README.md:41-90` — what structured `{host, api_token}` looks like (the README documents the *intended* shape that doesn't actually work yet)
3. `src/zombie/event_loop_helpers.zig:30-120` — current `resolveFirstCredential` path (to be replaced by structured resolver)
4. `schema/002_vault_schema.sql` — existing vault schema (extend, don't replace)

---

## Overview

**Goal (testable):** Operator runs:

```
zombiectl credential add fly --host api.machines.dev --api-token "$FLY_API_TOKEN"
zombiectl credential add upstash --host api.upstash.com --api-token "$UPSTASH_MGMT_TOKEN"
zombiectl credential add slack --host slack.com --bot-token "$SLACK_BOT_TOKEN"
zombiectl credential add github --api-token "$GITHUB_PAT" --webhook-secret "$WEBHOOK_SECRET"
```

The vault stores each credential as a structured record (JSON object with named fields). On `executor.createExecution`, the worker fetches all credentials referenced by the zombie's `credentials:` list and assembles a `secrets_map` shape: `{ fly: {host, api_token}, upstash: {host, api_token}, slack: {host, bot_token}, github: {api_token, webhook_secret} }`. The tool bridge (M41) substitutes `${secrets.fly.api_token}` etc. The dashboard's Credentials page lets operators add, list, and delete credentials with field-level forms; values are write-only (UI never reads them back).

**Problem:** Today `zombiectl credential add <name> --value <string>` stores a single string. The worker fetches via `resolveFirstCredential` and passes it to `startStage` as a single `api_key` field — that's the LLM provider key path. There's no schema for tool-level multi-field credentials. M37 sample's README documents the structured form aspirationally; the code doesn't support it.

**Solution summary:** Schema migration: `vault.secrets` gets a `data_jsonb` column for structured records. CLI extends to accept named field flags (`--host`, `--api-token`, `--bot-token`, etc., with the canonical fields per type). API extends to accept JSON body for credential add. UI extends the Add Credential modal with a Type selector that renders type-specific fields. Worker's resolver returns a `secrets_map` instead of a single string. M48 BYOK provider config consumes the same structured shape under a different type discriminator (`type=llm_provider`).

Dual-read window: the new column is added alongside the legacy `value` column; the resolver reads `data_jsonb` first, falls back to `{value}` synthesized from legacy. Background migration moves all existing rows to the new column; once empty, drop the legacy.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `schema/0NN_vault_secrets_structured.sql` | NEW | Add `data_jsonb` column, indexes |
| `schema/embed.zig` | EXTEND | Register new schema slot |
| `src/cmd/common.zig` | EXTEND | Migration array |
| `src/state/vault.zig` | EXTEND | `storeStructured(name, data: jsonb)` + `resolveStructured(name) → jsonb`. Dual-read shim. |
| `src/zombie/event_loop_helpers.zig` | EDIT | Replace `resolveFirstCredential` with `resolveSecretsMap(zombie_credentials)` returning `{ [name]: {fields} }` |
| `src/http/handlers/credentials/add.zig` | EDIT | Accept `data: jsonb` body alongside legacy `value: string` |
| `src/http/handlers/credentials/list.zig` | EDIT | Return field NAMES per credential (never values; values are write-only) |
| `zombiectl/src/commands/zombie_credential.js` | EDIT | Accept type-specific field flags; emit canonical JSON body |
| `ui/packages/app/src/routes/credentials/index.tsx` | EDIT | Add type selector + per-type field forms |
| `ui/packages/app/src/components/credentials/AddCredentialModal.tsx` | EDIT | Field-level form per type |
| `tests/integration/vault_structured_test.zig` | NEW | E2E: add structured cred → resolve into secrets_map → values match |
| `samples/fixtures/m45-credential-fixtures.json` | NEW | Canonical type definitions (fly, upstash, slack, github, llm_provider) |

---

## Sections (implementation slices)

### §1 — Schema migration

```sql
-- Add structured data column
ALTER TABLE vault.secrets ADD COLUMN data_jsonb jsonb;

-- Existing rows: migrate `value` (text) into `data_jsonb` as {"value": "<original>"}
-- Done in app code at first read, not in this migration (avoid long lock)

-- Index for type queries
CREATE INDEX vault_secrets_workspace_name_idx ON vault.secrets(workspace_id, name);
```

**Implementation default**: keep `value` column for the dual-read window. A follow-up migration drops `value` after all rows have `data_jsonb` populated.

### §2 — Canonical credential types

`samples/fixtures/m45-credential-fixtures.json`:

```json
{
  "fly": {
    "fields": ["host", "api_token"],
    "defaults": {"host": "api.machines.dev"}
  },
  "upstash": {
    "fields": ["host", "api_token"],
    "defaults": {"host": "api.upstash.com"}
  },
  "slack": {
    "fields": ["host", "bot_token"],
    "defaults": {"host": "slack.com"}
  },
  "github": {
    "fields": ["api_token", "webhook_secret"],
    "defaults": {}
  },
  "llm_provider": {
    "fields": ["provider", "api_key", "model"],
    "defaults": {}
  }
}
```

Used by CLI and UI to know which fields prompt for which type. Extensible — adding a type means appending here.

### §3 — CLI: type-specific field flags

```
zombiectl credential add <type> [flags]
  --host <value>          (when type uses it)
  --api-token <value>
  --bot-token <value>     (slack)
  --webhook-secret <value> (github)
  --api-key <value>       (llm_provider)
  --model <value>         (llm_provider)
  --provider <value>      (llm_provider)
```

CLI looks up the type from the fixture file (or hard-coded), validates required fields are present, emits canonical JSON.

**Implementation default**: support env-var fallback per flag (e.g., `--api-token` reads `$ZOMBIE_CRED_API_TOKEN` if flag not provided). Avoids tokens in shell history.

### §4 — API: structured body

```
PUT /v1/workspaces/{ws}/credentials/{name}
  body: { type: "fly" | "upstash" | "slack" | "github" | "llm_provider", data: {...} }
  → 200 { name, type, fields: ["host", "api_token"] }   # field names, not values
```

API encrypts `data` via existing crypto_store (KMS envelope) before INSERT.

### §5 — Worker resolver

`src/zombie/event_loop_helpers.zig`:

```
resolveSecretsMap(zombie_credentials: []string) → secrets_map: { [name]: { [field]: string } }
  for each name in zombie_credentials:
    decrypted = vault.resolveStructured(name)
    secrets_map[name] = decrypted
  return secrets_map
```

Pass to `executor.createExecution` (M41 contract).

### §6 — UI Add Credential modal

Type selector dropdown → render fields per type (from the fixtures file). Each field is a password-masked input (write-only). Submit → POST canonical body. Success → close modal, refresh list.

List view: shows credential name + type + field NAMES (not values). Operator can delete a credential but not view values.

**Implementation default**: storybook entries for each type's modal state for visual review.

### §7 — Dual-read shim

`src/state/vault.zig::resolveStructured(name)`:

```
1. SELECT data_jsonb, value FROM vault.secrets WHERE name = $1
2. If data_jsonb IS NOT NULL: return data_jsonb
3. Else if value IS NOT NULL: return {"value": value}  (legacy synthetic)
4. Else: return null (not found)
```

The legacy synthetic shape lets old code paths keep working until M41 is done switching to `secrets_map`. Once all rows have `data_jsonb`, drop the legacy column and the shim.

---

## Interfaces

```
HTTP:
  PUT /v1/workspaces/{ws}/credentials/{name}
    body: { type: string, data: object }
    → 200 { name, type, field_names: [string] }

  GET /v1/workspaces/{ws}/credentials
    → 200 { items: [{ name, type, field_names: [string], created_at }] }

  DELETE /v1/workspaces/{ws}/credentials/{name}
    → 204

CLI:
  zombiectl credential add <name> [type-specific flags]
  zombiectl credential list
  zombiectl credential delete <name>

Internal API:
  vault.storeStructured(workspace_id, name, type, data: jsonb)
  vault.resolveStructured(workspace_id, name) → jsonb
  vault.resolveSecretsMap(workspace_id, names: [string]) → { [name]: jsonb }
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| Required field missing on add | User forgot `--api-token` | CLI errors before HTTP: "Type `fly` requires `--api-token`" |
| Type not in fixtures | New type not yet added | API rejects with `unknown_credential_type`, hint to add to fixtures |
| Resolver: credential not found | Zombie config references nonexistent name | M41 surfaces `secret_not_found` to agent (handled there) |
| Dual-read conflict (both columns set) | Migration race | `data_jsonb` wins; legacy ignored |
| KMS decrypt failure | Key rotation gap | API 500 with `vault_decrypt_failed`; ops alert |

---

## Invariants

1. **Values are write-only via API.** No endpoint returns credential values. `field_names` only.
2. **Encrypted at rest.** `data_jsonb` is always KMS-enveloped before INSERT.
3. **One canonical type per credential.** Type discriminator is immutable post-create; rename means delete + re-add.
4. **Dual-read order**: structured wins. Legacy is read-only fallback during the migration window.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_add_structured_fly` | `credential add fly --host x --api-token y` → vault has `{type:"fly", data:{host:"x", api_token:"y"}}` |
| `test_required_field_missing` | `credential add fly --host x` (no token) → CLI fails before HTTP |
| `test_resolve_secrets_map` | Add fly + upstash + slack → resolveSecretsMap returns all three with correct fields |
| `test_dual_read_legacy_synthetic` | Insert legacy row (only `value` set) → resolveStructured returns `{value: ...}` |
| `test_dual_read_structured_wins` | Both columns set → resolveStructured returns `data_jsonb` content |
| `test_list_no_values_in_response` | Add credential with sensitive value → GET /credentials → response contains no token bytes (regex grep) |
| `test_delete_purges_kms` | Delete → underlying KMS envelope is also deleted (no orphan) |
| `test_e2e_zombie_uses_resolved_creds` | Install zombie → steer → assert `${secrets.fly.api_token}` substituted at tool bridge with vault value |

---

## Acceptance Criteria

- [ ] `make test-integration` passes the 8 tests above
- [ ] All 5 canonical types (fly, upstash, slack, github, llm_provider) listed in `samples/fixtures/m45-credential-fixtures.json`
- [ ] Manual: add a fly credential via UI → see it in `zombiectl credential list` → delete via CLI → confirm gone in UI
- [ ] Audit grep: capture HTTP response bodies for `GET /credentials` → 0 matches for any added credential value
- [ ] `make check-pg-drain` clean
- [ ] Cross-compile clean: x86_64-linux + aarch64-linux
- [ ] Migration tested: existing pre-M45 credentials (single-string) keep working via dual-read shim
