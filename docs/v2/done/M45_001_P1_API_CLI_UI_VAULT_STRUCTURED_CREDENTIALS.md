# M45_001: Credential Vault — Opaque JSON Object Plaintext

**Prototype:** v2.0.0
**Milestone:** M45
**Workstream:** 001
**Date:** Apr 26, 2026
**Status:** DONE
**Priority:** P1 — launch-blocking. M41 Context Layering depends on `secrets_map` being a structured object, not a single string. M43 Webhook Ingest depends on multi-field credential shape. The current `--value` (single-string) credential storage is the chokepoint that prevents both.
**Categories:** API, CLI, UI
**Batch:** B1 — parallel with M40, M41, M42, M43, M44.
**Branch:** feat/m45-vault-structured
**Depends on:** none.

**Canonical architecture:** `docs/ARCHITECHTURE.md` (note historical typo) §10 (capability table — credential vault row), §12 step 2-4 (credential add flow).

---

## Decision summary (Apr 26, 2026)

The original draft proposed a typed credential registry (`fly`, `upstash`, `slack`, `github`, `llm_provider`) with per-type CLI flags, a `data_jsonb` column migration with a dual-read window, and a "legacy `value` column" backward-read shim. **All three were rejected by the owner.** The restated direction is below.

1. **No type registry.** A credential is an opaque JSON object addressed by name. The skill / zombie config that consumes the secret decides what fields it expects (`${secrets.<name>.<path>}`). The vault stores objects, not typed records. No fixtures file.
2. **No schema change.** `vault.secrets` already KMS-envelopes a plaintext byte slice. The plaintext becomes the canonical-stringified JSON object. No `ALTER TABLE`, no new column — Schema Guard pre-v2.0 forbids it anyway. The KMS envelope (random DEK + AES-GCM ciphertext, KEK-wrapped DEK) does not care what the plaintext bytes are.
3. **No legacy fallback.** No pre-M45 `--value` rows exist in production. The migration is a flag-day flip in CLI, API request shape, and worker resolver. Drop the dual-read shim and the "synthetic `{value: ...}`" wrapper.
4. **BYOK fold deferred to M48.** M45 lands the structured plaintext path. M48 owns the rewrite of `{provider}_api_key` + `llm_provider_preference` into one `llm` credential.

---

## Implementing agent — read these first

1. `src/http/handlers/zombies/activity.zig:88-216` — current generic credential POST/GET handlers (`{name, value}` body, `zombie:` key prefix).
2. `src/http/handlers/workspaces/credentials.zig` — BYOK PUT/GET/DELETE (untouched by M45; folded under M48).
3. `src/secrets/crypto_store.zig` — KMS envelope `store(plaintext)` / `load() → plaintext`. Plaintext is opaque bytes.
4. `src/zombie/event_loop_helpers.zig:30-51` — current `resolveFirstCredential` (returns first decrypted plaintext as a single string).
5. `zombiectl/src/commands/zombie_credential.js` — current `--value` single-string CLI shape.
6. `samples/platform-ops/README.md:41-90` — the structured-credential UX the sample assumes (the README documents the *intended* shape; until M45 ships, the runtime stores it as a single string and the example is partly aspirational).

---

## Overview

**Goal (testable):** Operator runs:

```
zombiectl credential add fly      --data '{"host":"api.machines.dev","api_token":"FLY_..."}'
zombiectl credential add upstash  --data '{"host":"api.upstash.com","api_token":"UPSTASH_..."}'
zombiectl credential add slack    --data '{"host":"slack.com","bot_token":"xoxb-..."}'
zombiectl credential add github   --data '{"api_token":"ghp_...","webhook_secret":"whsec_..."}'
```

The vault stores each credential's plaintext as the canonical-stringified JSON object inside the existing KMS envelope. On `executor.createExecution`, the worker fetches all credentials referenced by the zombie's `credentials:` list and assembles `secrets_map: { fly: {...}, upstash: {...}, slack: {...}, github: {...} }` — each value is the parsed JSON object. The tool bridge (M41 — out of scope here) substitutes `${secrets.fly.api_token}` against the parsed object. The dashboard's Credentials page lets operators add, list, and delete credentials; values are write-only (the API never returns them).

**Problem (today):** `zombiectl credential add <name> --value <string>` stores a single plaintext string. The worker's `resolveFirstCredential` returns one string and passes it to `startStage` as `api_key`. There is no way to express `${secrets.fly.api_token}` separately from `${secrets.fly.host}`.

**Solution summary:** No schema change. The plaintext bytes become a canonical JSON object's `Stringify` output. Handlers, CLI, UI, and worker change shape together. The KMS envelope is identical bytes-in / bytes-out — `crypto_store.store` and `crypto_store.load` need no edits.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `src/state/vault.zig` | NEW | `storeJson(conn, ws, name, value)` and `loadJson(alloc, conn, ws, name) → parsed`. Validates non-empty object, canonicalizes keys, encodes/decodes via `crypto_store`. |
| `src/http/handlers/zombies/activity.zig` | EDIT | `CredentialBody { name, data: std.json.Value }`. Validate object-shape, call `vault.storeJson`. Add `innerDeleteCredential`. |
| `src/http/handlers/zombies/credentials_list.zig` | NEW (extracted) | `innerListCredentials` lifted from `activity.zig` to keep both files under the 350-line gate. |
| `src/http/route_manifest.zig` | EDIT | Add `DELETE /v1/workspaces/{workspace_id}/credentials/{name}`. |
| `src/http/router.zig` | EDIT | Match `DELETE` on the named-credential route. |
| `src/http/route_table_invoke.zig` | EDIT | Dispatch `DELETE` to `innerDeleteCredential`. |
| `src/zombie/event_loop_helpers.zig` | EDIT | Add `resolveSecretsMap(alloc, pool, ws, names)` returning `[]NameValue` of parsed `std.json.Value`. Keep `resolveFirstCredential` until M41 wires `secrets_map` through `executor.createExecution`. |
| `zombiectl/src/commands/zombie_credential.js` | EDIT | `--data <json>` flag (validate parses as object), `delete` action. Drop `--value`. |
| `zombiectl/src/lib/api-paths.js` | EDIT | Add `wsCredentialPath(wsId, name)`. |
| `ui/packages/app/src/routes/credentials/index.tsx` | EDIT | List + delete. |
| `ui/packages/app/src/components/credentials/AddCredentialModal.tsx` | EDIT | Single password-masked JSON textarea + parse-on-submit, or generic key/value-row builder that emits an object. |
| `public/openapi/paths/credentials.yaml` (or canonical equivalent) | EDIT | Update POST body schema; add DELETE. Body is `{name, data: object}`. |
| `src/http/credentials_json_test.zig` | NEW | E2E: add → list → resolve `secrets_map` → delete → 404. Negative: non-object data, missing `data`, oversized payload, cross-workspace IDOR. |
| `src/state/vault_test.zig` | NEW | Unit: `storeJson` / `loadJson` round-trip; rejects non-object; canonical key order. |

**No file under `schema/`** — Schema Guard pre-v2.0 plus the no-schema-change decision means the gate never fires here.

---

## Sections (implementation slices)

### §1 — `src/state/vault.zig` (~120 LoC)

```zig
pub fn storeJson(conn: *pg.Conn, alloc: Allocator,
                 workspace_id: []const u8, name: []const u8,
                 value: std.json.Value) !void
pub fn loadJson(alloc: Allocator, conn: *pg.Conn,
                workspace_id: []const u8, name: []const u8) !ParsedJson
```

- Reject non-object (`value != .object`) with `error.NotAnObject`.
- Reject empty object with `error.EmptyObject` (operator forgot fields).
- Stringify with `std.json.Stringify.valueAlloc` using sorted keys (canonical form so cipher reuse / metric labels are stable).
- `loadJson` returns `std.json.Parsed(std.json.Value)`; caller calls `.deinit()`.
- Storage key naming preserved: `zombie:{name}` to match today's prefix and avoid conflict with BYOK rows.

### §2 — Handler `POST /v1/workspaces/{ws}/credentials`

```zig
const CredentialBody = struct {
    name: []const u8,
    data: std.json.Value,  // must be .object
};
```

- Existing validation: name non-empty + length cap unchanged.
- New validation: `data` is `.object`, non-empty, total stringified length ≤ `MAX_CREDENTIAL_VALUE_LEN` (current 8 KiB cap — apply to canonical JSON length).
- On accept: call `vault.storeJson(...)`. Response: `201 { name }` (unchanged).
- 400 on non-object / empty object / parse failure.

### §3 — Handler `DELETE /v1/workspaces/{ws}/credentials/{name}`

- Operator role required (mirror today's POST/GET).
- `DELETE FROM vault.secrets WHERE workspace_id = $1 AND key_name = $2` with `key_name = 'zombie:' || name`.
- 204 on delete (regardless of existed/not-existed — idempotent).

### §4 — Worker `resolveSecretsMap`

```zig
pub const ResolvedSecret = struct {
    name: []const u8,        // duped
    parsed: std.json.Parsed(std.json.Value), // caller deinits
};

pub fn resolveSecretsMap(alloc: Allocator, pool: *pg.Pool,
                         workspace_id: []const u8,
                         names: []const []const u8) ![]ResolvedSecret
```

- Iterates names; for each, decrypts via `crypto_store.load`, parses JSON, appends.
- Returns `error.CredentialNotFound` for any missing name (callers decide whether to surface as `secret_not_found` per M41's contract).
- Caller frees the slice + each entry's `name` and `.parsed.deinit()`.

`resolveFirstCredential` stays untouched until M41 swaps it. Both can coexist.

### §5 — CLI `zombiectl credential`

```
zombiectl credential add <name>    --data '<json-object>'   # validates parses
zombiectl credential list
zombiectl credential delete <name>
```

- `--data` must JSON-parse as object; CLI rejects bare strings/arrays/numbers before HTTP.
- Optional `--from-file <path>` reads JSON from a file (avoids shell-history exposure) — implement only if free in §5 LoC budget.
- Drop `--value` entirely. No deprecation alias — pre-M45 has no users.

### §6 — UI

- **List view**: name + created_at (today's shape). Add a Delete affordance.
- **Add modal**: a labeled JSON textarea (password-masked monospace input). Client-side JSON.parse before submit; on parse error, inline error. Submit POSTs `{name, data}`. On success, close + refresh list.

### §7 — OpenAPI

- POST body schema: `{name: string, data: object (additionalProperties: true, minProperties: 1)}`.
- DELETE response: 204; 404 not modeled (idempotent).
- List response unchanged: array of `{name, created_at}`.

---

## Interfaces

```
HTTP:
  POST   /v1/workspaces/{ws}/credentials
    body: { name: string, data: object }
    → 201 { name }
    → 400 invalid_request | credential_data_not_object | credential_data_empty | credential_data_too_long

  GET    /v1/workspaces/{ws}/credentials
    → 200 { credentials: [{ name, created_at }] }

  DELETE /v1/workspaces/{ws}/credentials/{name}
    → 204 (idempotent)

CLI:
  zombiectl credential add <name>    --data '<json-object>'
  zombiectl credential list
  zombiectl credential delete <name>

Internal API (Zig):
  vault.storeJson(conn, alloc, ws, name, std.json.Value) !void
  vault.loadJson(alloc, conn, ws, name) !std.json.Parsed(std.json.Value)
  event_loop_helpers.resolveSecretsMap(alloc, pool, ws, []const []const u8)
                                        ![]ResolvedSecret
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| `data` is not an object | Client sent a string/array/scalar | API 400 `credential_data_not_object` |
| `data` is `{}` | Empty object | API 400 `credential_data_empty` |
| Stringified data > 8 KiB | Operator pasted huge blob | API 400 `credential_data_too_long` |
| CLI `--data` not parseable | Operator typo | CLI fails before HTTP with parse error + offset |
| Resolver: name missing | Zombie config references nonexistent credential | `error.CredentialNotFound` → M41 surfaces `secret_not_found` to agent |
| KMS decrypt failure | Key rotation gap or corrupt row | API 500 `vault_decrypt_failed`; ops alert (existing behavior) |
| Decrypted bytes are not JSON | Row predates M45 (none should exist) or DB corruption | `error.MalformedCredentialPlaintext`; surfaced as 500 + log; no synthetic wrap |

---

## Invariants

1. **Values are write-only via API.** No endpoint returns the plaintext or any field of it. List returns name + timestamp only.
2. **Encrypted at rest.** Plaintext bytes (canonical JSON) live only inside the existing KMS envelope.
3. **Plaintext is a non-empty JSON object.** Strings, arrays, scalars, and `{}` are rejected at the API boundary. Anything that bypassed the boundary and ended up non-object surfaces as a 500, never silently wrapped.
4. **Canonical key order on store.** `loadJson` does not depend on it (parser ignores order), but storing canonically means re-encryption produces deterministic ciphertext-input across replicas.

---

## Test Specification

| Test | Asserts |
|---|---|
| `vault.storeJson rejects non-object` | Passing `.string`/`.array`/`.integer` → `error.NotAnObject`, no row inserted. |
| `vault.storeJson rejects empty object` | `.{}` → `error.EmptyObject`. |
| `vault.storeJson + loadJson round-trip` | Object with nested + arrays + numbers + bools + nulls survives encrypt-decrypt-parse. |
| `vault.storeJson canonicalizes keys` | Two writes with differing key order produce identical canonical plaintext (assert by re-load + stringify with sorted keys). |
| `POST /credentials accepts object` | `{name:"fly", data:{host:"x", api_token:"y"}}` → 201, vault row exists, key_name=`zombie:fly`. |
| `POST /credentials rejects {data:"x"}` | 400, `credential_data_not_object`. |
| `POST /credentials rejects {data:{}}` | 400, `credential_data_empty`. |
| `POST /credentials enforces size cap` | 9 KiB stringified → 400, `credential_data_too_long`. |
| `GET /credentials never returns data` | Add credential with sentinel value `"SENTINEL_TOKEN_123"` → response body grep for sentinel = 0 matches. |
| `DELETE /credentials is idempotent` | DELETE, DELETE → both 204. Subsequent GET shows credential gone. |
| `cross-workspace DELETE returns 403` | Operator of ws-A cannot delete ws-B credential. |
| `resolveSecretsMap returns parsed objects` | Add fly+upstash+slack → resolver returns 3 entries, each parses to object with expected keys. |
| `resolveSecretsMap on missing name` | One missing name → `error.CredentialNotFound`. |

---

## Acceptance Criteria

- [x] `make lint` clean.
- [x] `make test` clean (unit tier).
- [x] `make test-integration` passes the new tests (handler + worker + vault wrapper). Suite total: 1455/1484, 11 skipped, 18 failed + 1 leaked — all 19 issues are pre-existing in files this branch does not touch (metering_test, tenant_billing_test, pool_test, event_loop_*_test, api_integration_test); they reproduce on main without these changes and were masked previously by the BYOK-delete drain deadlock that the `pg_query` fix resolves.
- [x] `make check-pg-drain` clean.
- [x] Cross-compile clean: `x86_64-linux` + `aarch64-linux`.
- [ ] Manual: add a fly credential via UI → see it via `zombiectl credential list` → delete via CLI → confirm gone in UI. **Deferred** — UI was not booted locally during this implementation (no dev server in the worktree). Coverage rests on the unit + integration tests; smoke test recommended before tagging the next release.
- [x] Audit grep: integration test `credential POST + GET + DELETE roundtrip never echoes value` asserts the sentinel token does not appear in any GET response or POST/DELETE response body.
- [x] OpenAPI lint passes — `make openapi` runs `redocly lint`, schema parity, and parser regression tests, all green.
- [x] `samples/platform-ops/README.md` example commands now produce vault rows that match the structured shape (CLI + handler accept `{name, data: object}`).
- [x] M48 spec carries a one-line cross-reference: structured plaintext path lands in M45; BYOK fold lives under M48 (committed in `chore(m45-open)`).

---

## Out of scope (explicit)

- BYOK rewrite (M48).
- M41 tool-bridge `${secrets.x.y}` substitution — this spec exposes `resolveSecretsMap`; M41 consumes it.
- Per-credential RBAC / scopes — out of scope.
- Audit log of credential reads — exists today via `secret.retrieved` log line, no change.
- Streaming / event-sourced credential rotation — out of scope.
