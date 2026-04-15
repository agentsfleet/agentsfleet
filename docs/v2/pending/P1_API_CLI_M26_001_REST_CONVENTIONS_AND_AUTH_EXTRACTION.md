# M26_001: REST API Conventions Cleanup + Auth Module Extraction

**Prototype:** v0.10.0
**Milestone:** M26
**Workstream:** 001
**Date:** Apr 16, 2026
**Status:** PENDING
**Priority:** P1 — Customer-facing API surface hygiene + internal auth boundary
**Batch:** B1
**Branch:** feat/m26-rest-and-auth-cleanup — added when work begins
**Depends on:** M24_001 (workspace-scoped routes landed)

---

## Overview

**Goal (testable):** All 42 HTTP routes in `router.zig` match `docs/REST_API_DESIGN_GUIDELINES.md` conventions (plural nouns, `items`/`total` list envelope, GET for read-only queries, camelCase field names) AND are present in `public/openapi.json` AND the auth helpers (`authorizeWorkspace`, `getZombieWorkspaceId`, `authorizeWorkspaceAndSetTenantContext`) live in `src/auth/rbac/` with no reverse dependency on `src/http/handlers/common.zig`.

**Problem:** An internal audit against REST_API_DESIGN_GUIDELINES.md and api_handler_guide.md found:
1. List endpoints return collection-keyed envelopes (`.zombies`, `.agents`, `.grants`, `.events`) instead of standardized `.items`/`.total` (RAD §8).
2. `POST /v1/memory/recall` and `POST /v1/memory/list` use POST for read-only queries (RAD §4 violation).
3. `request_id` (snake_case) leaks through memory responses; spec mandates camelCase (RAD §1).
4. One handler (`integration_grants_workspace.zig`) writes raw `res.status = 204` instead of using `hx.ok(.no_content, .{})` per handler guide §3.
5. `public/openapi.json` documents 25 of 42 routes; 17 customer-facing endpoints are missing and the stale `/v1/runs/*` family (removed in M10_001) is still listed. SDK generation is broken.
6. Auth RBAC helpers live in `src/http/handlers/common.zig`, coupling the auth layer to the HTTP handler layer. Blocks a clean `test-auth` module boundary.

**Solution summary:** One branch, one PR. Normalize the 4 list envelopes to `{ items, total, [cursor] }`. Convert 2 memory read endpoints from POST to GET with query params. Rename `request_id` → `requestId` in memory responses. Replace the inline 204. Regenerate `public/openapi.json` against the live route table. Move the three auth helpers into `src/auth/rbac/` and have handlers import them from there. zombiectl CLI callers updated in lockstep so nothing breaks end-to-end. Pre-v2.0 teardown era (VERSION=0.9.0) — breaking wire-format changes are acceptable; no 410 stubs, no backward-compat shims.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/http/handlers/zombie_api.zig` | MODIFY | `.zombies` → `.items`/`.total` |
| `src/http/handlers/external_agents.zig` | MODIFY | `.agents` → `.items`/`.total` |
| `src/http/handlers/integration_grants_workspace.zig` | MODIFY | `.grants` → `.items`/`.total` + inline-204 fix |
| `src/http/handlers/zombie_activity_api.zig` | MODIFY | `.events` → `.items`/`.total`/`.cursor` |
| `src/http/handlers/memory_http.zig` | MODIFY | `request_id` → `requestId`; GET handlers for recall/list |
| `src/http/router.zig` | MODIFY | `memory_recall`/`memory_list` matched for GET, not POST |
| `src/http/route_table_invoke.zig` | MODIFY | method check for memory_recall/list → GET |
| `src/auth/rbac/workspace.zig` | CREATE | `authorizeWorkspace`, `getZombieWorkspaceId`, `authorizeWorkspaceAndSetTenantContext` |
| `src/auth/rbac/mod.zig` | CREATE | re-export public RBAC surface |
| `src/http/handlers/common.zig` | MODIFY | delete the three moved functions; update imports |
| `src/http/handlers/*.zig` (~10 handler files) | MODIFY | switch to `auth.rbac.authorizeWorkspace(...)` |
| `public/openapi.json` | MODIFY | regenerate — 42 routes, remove `/v1/runs/*` |
| `zombiectl/src/commands/zombie.js` | MODIFY | `res.zombies`/`res.events` → `res.items` |
| `zombiectl/src/commands/agent_external.js` | MODIFY | `res.agents` → `res.items` |
| `zombiectl/src/commands/grant.js` | MODIFY | `res.grants` → `res.items` |
| `zombiectl/src/lib/http.js` | MODIFY | prefer `requestId` in ApiError read |
| `/Users/kishore/Projects/docs/changelog.mdx` | MODIFY | v0.10.0 `<Update>` block |
| `docs/nostromo/LOG_APR_16_*.md` | CREATE | Ripley's log for the milestone |

## Applicable Rules

- RULE FLL — 350-line gate on every touched .zig/.js file.
- RULE XCC — cross-compile x86_64-linux + aarch64-linux before commit.
- RULE ORP — orphan sweep for the three moved auth helpers across schema/Zig/JS/tests/docs.
- RULE EP4 — pre-v2.0 carve-out: removed endpoints return 404, not 410 stubs. Memory POST→GET is a method change, not a removal; old POST returns 405 via standard method-not-allowed.
- Standard lint set (`make lint`, `make check-pg-drain`, `gitleaks`).

No schema changes — Schema Table Removal Guard does not fire.

---

## Sections (implementation slices)

### §1 — List envelope standardization

**Status:** PENDING

Normalize four GET list handlers to return `{ items: [...], total: N }` (plus `cursor` where pagination already exists). Eliminates the per-resource collection key so SDKs can share a `Paginated<T>` generic.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `zombie_api.zig:innerListZombies` | authed GET with 3 zombies | body has `items: [3]`, `total: 3`, no `zombies` key | integration |
| 1.2 | PENDING | `external_agents.zig:innerListExternalAgents` | authed GET | body has `items`/`total` | integration |
| 1.3 | PENDING | `integration_grants_workspace.zig:innerListGrants` | authed GET | body has `items`/`total` | integration |
| 1.4 | PENDING | `zombie_activity_api.zig:innerListActivity` | authed GET with cursor | body has `items`/`total`/`cursor` (not `events`/`next_cursor`) | integration |

### §2 — Memory endpoints POST → GET

**Status:** PENDING

`/v1/memory/recall` and `/v1/memory/list` are read-only queries that currently POST a body. Convert to GET with query params. `store` stays POST, `forget` stays DELETE.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `router.zig:match` | `GET /v1/memory/recall?zombie_id=...&query=...&limit=10` | returns `.memory_recall` variant | unit |
| 2.2 | PENDING | `memory_http.zig:innerMemoryRecall` | GET with query params | same response shape as prior POST | integration |
| 2.3 | PENDING | `memory_http.zig:innerMemoryList` | `GET /v1/memory/list?zombie_id=...&category=...` | `items`/`total` response | integration |
| 2.4 | PENDING | `route_table_invoke.zig:invokeMemoryRecall` | `POST /v1/memory/recall` | 405 Method Not Allowed | integration |

### §3 — Field casing: `request_id` → `requestId`

**Status:** PENDING

Memory responses return `request_id` (snake_case). Per RAD §1 use lowerCamelCase for acronyms. Other handlers already use `req_id` internally but don't leak it to the wire; memory is the one outlier that does.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `memory_http.zig:94,173,248,306` | any memory request | response has `requestId`, no `request_id` | integration |
| 3.2 | PENDING | `zombiectl/src/lib/http.js:44` | any ApiError | reads `requestId` first | unit |

### §4 — Inline 204 → `hx.ok(.no_content, .{})`

**Status:** PENDING

Single-line handler-guide violation.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `integration_grants_workspace.zig:141` | authed DELETE grant | 204 response via `hx.ok(.no_content, .{})` | integration |

### §5 — OpenAPI regeneration

**Status:** PENDING

Document every route in `router.zig` Route enum. Remove stale `/v1/runs/*`.

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | PENDING | `public/openapi.json` | N/A | 42 paths × methods match router enum exactly | contract |
| 5.2 | PENDING | `public/openapi.json` | grep `/v1/runs` | zero matches | contract |
| 5.3 | PENDING | `public/openapi.json` | all list response schemas | use `items`/`total` shape | contract |
| 5.4 | PENDING | `public/openapi.json` | memory recall/list | method is GET with query params | contract |

### §6 — Auth RBAC helpers → `src/auth/rbac/`

**Status:** PENDING

Move the three workspace-authorization helpers out of `src/http/handlers/common.zig` into a new `src/auth/rbac/workspace.zig`. Handlers import from `auth.rbac`. No behavior change; pure relocation + import rewrites. Keeps `test-auth` target green (already proves auth module compiles standalone).

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 6.1 | PENDING | `src/auth/rbac/workspace.zig` | compile | exports `authorizeWorkspace`, `getZombieWorkspaceId`, `authorizeWorkspaceAndSetTenantContext` | unit |
| 6.2 | PENDING | all handlers using these fns | `grep -rn common.authorizeWorkspace src/http/handlers/` | 0 matches post-refactor | contract |
| 6.3 | PENDING | `build.zig:test-auth` | `zig build test-auth` | passes — auth module still compiles without HTTP layer | contract |
| 6.4 | PENDING | existing auth/workspace tests | `make test` | all pass, zero regressions | unit + integration |

### §7 — zombiectl consumer updates

**Status:** PENDING

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 7.1 | PENDING | `zombiectl/src/commands/zombie.js` | list + activity | reads `res.items` | unit |
| 7.2 | PENDING | `zombiectl/src/commands/agent_external.js` | list | reads `res.items` | unit |
| 7.3 | PENDING | `zombiectl/src/commands/grant.js` | list | reads `res.items` | unit |
| 7.4 | PENDING | `zombiectl/src/commands/memory.js` (if present) | recall/list | uses GET with query params | unit |

---

## Interfaces

### Response envelope (list endpoints)

```json
{ "items": [...], "total": 42, "cursor": "opaque-or-null" }
```

`cursor` is present only for endpoints with cursor-based pagination (activity, telemetry). `total` reflects the count of `items` in the current response (not the total across pages).

### Memory GET endpoints

```
GET /v1/memory/recall?zombie_id={id}&query={text}&limit={n}
GET /v1/memory/list?zombie_id={id}&category={cat}&limit={n}
```

All params URL-encoded. `limit` optional, default 10, max 100.

### Auth RBAC module

```zig
// src/auth/rbac/workspace.zig
pub fn authorizeWorkspace(principal: AuthPrincipal, workspace_id_str: []const u8) !void;
pub fn getZombieWorkspaceId(conn: *pg.Conn, zombie_id: []const u8) !?[]const u8;
pub fn authorizeWorkspaceAndSetTenantContext(
    hx: *anyopaque,
    conn: *pg.Conn,
    workspace_id_str: []const u8,
) !void;
```

Signatures unchanged from current implementation; only location and import path differ.

### Error Contracts (unchanged — relocations only)

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Workspace UUID malformed | `ERR_INVALID_REQUEST` | 400 |
| Principal workspace ≠ path workspace | `ERR_FORBIDDEN` | 403 |
| Zombie not found | `ERR_ZOMBIE_NOT_FOUND` | 404 |
| Memory recall without `zombie_id` query param | `ERR_INVALID_REQUEST` | 400 |
| POST to memory_recall/list path | standard 405 | 405 |

---

## Failure Modes

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| zombiectl against pre-M26 server | CLI deployed before server update | `res.items` is undefined | CLI crashes on `.items.map` — acceptable; zombiectl and server ship together |
| Old CLI against post-M26 server | server deployed first | `res.zombies` is undefined | old CLI shows empty list — breaking but pre-v2.0 |
| Missing required query param on memory GET | `GET /v1/memory/recall` without `zombie_id` | 400 ERR_INVALID_REQUEST | explicit error |
| Auth RBAC import circular | `src/auth/rbac` imports `src/http` | compile fails | caught by `zig build` + `test-auth` |

**Platform constraints:** None beyond standard Zig + httpz behavior.

---

## Implementation Constraints (Enforceable)

| Constraint | How to verify |
|-----------|---------------|
| Every touched .zig and .js file ≤ 350 lines | `wc -l` on diff files (RULE FLL, excluding tests/vendor/.md) |
| Cross-compiles x86_64-linux + aarch64-linux | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| `test-auth` target passes (auth compiles without HTTP layer) | `zig build test-auth` |
| openapi.json has entry for every Route enum variant | ad-hoc check script: iterate `Route` variants, grep openapi.json |
| No `common.authorizeWorkspace` refs after §6 | `grep -rn 'common.authorizeWorkspace' src/ --include='*.zig'` → 0 hits |
| No `res.zombies`/`.agents`/`.grants`/`.events` in zombiectl | `grep -rnE 'res\.(zombies\|agents\|grants\|events)' zombiectl/src/` → 0 hits |
| No `request_id` in memory response paths | `grep -n 'request_id' src/http/handlers/memory_http.zig` → 0 hits |

---

## Invariants (Hard Guardrails)

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | Auth RBAC module does not import `src/http/` | `test-auth` build target links only `src/auth/**`; cycle causes link failure |
| 2 | Every Route enum variant has matching openapi.json path+method | Manual contract verification in §5 eval commands |

---

## Test Specification

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| memory_recall_get_router_match | 2.1 | `router.zig:match` | `GET /v1/memory/recall?...` | `.memory_recall` variant |
| http_error_reads_requestId | 3.2 | `zombiectl/src/lib/http.js:ApiError` | `{error:{requestId:"abc"}}` | `.requestId === "abc"` |

### Integration Tests

| Test name | Dim | Infra needed | Input | Expected |
|-----------|-----|-------------|-------|----------|
| list_zombies_items_envelope | 1.1 | DB | `GET /v1/workspaces/{w}/zombies` | body.items is array, body.total is int |
| list_activity_items_cursor | 1.4 | DB | `GET .../activity` | items/total/cursor keys |
| memory_recall_get_200 | 2.2 | DB | `GET /v1/memory/recall?zombie_id=X&query=Y` | 200 + items |
| memory_recall_post_405 | 2.4 | DB | `POST /v1/memory/recall` | 405 |
| memory_response_requestId | 3.1 | DB | any memory response | `requestId` present, `request_id` absent |
| delete_grant_204 | 4.1 | DB | `DELETE /v1/.../integration-grants/{g}` | 204, empty body |
| authorize_workspace_from_rbac | 6.1, 6.4 | none | call `auth.rbac.authorizeWorkspace(...)` | same behavior as pre-refactor `common.authorizeWorkspace` |

### Negative Tests

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| memory_recall_missing_zombie_id | 2.2 | `GET /v1/memory/recall?query=x` | `ERR_INVALID_REQUEST` 400 |
| memory_list_bad_limit | 2.3 | `GET /v1/memory/list?zombie_id=X&limit=abc` | `ERR_INVALID_REQUEST` 400 |

### Regression Tests

| Test name | What it guards | File |
|-----------|---------------|------|
| m24_cross_workspace_idor | RBAC still blocks cross-workspace access after helpers move | `m24_001_cross_workspace_idor_test.zig` |
| test-auth standalone | auth module still compiles without HTTP layer | `build.zig:test-auth` |

### Leak Detection Tests

| Test name | Dim | What it proves |
|-----------|-----|---------------|
| memory_recall_get_no_leaks | 2.2 | std.testing.allocator detects zero leaks on GET path (query param dup/free) |

### Spec-Claim Tracing

| Spec claim | Test that proves it | Test type |
|-----------|-------------------|-----------|
| "42 routes match openapi" | §5.1 eval script | contract |
| "auth compiles standalone" | `zig build test-auth` | contract |
| "no res.zombies in CLI" | §7 grep | contract |
| "breaking OK pre-v2.0" | VERSION file + acceptance | manual |

---

## Execution Plan (Ordered)

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | §1 list envelope changes + matching integration test updates | `zig build test-integration` for touched handler tests |
| 2 | §3 request_id rename in memory handler | `zig build` |
| 3 | §4 inline-204 fix | `zig build` |
| 4 | §2 memory POST→GET: router, invoke, handler, test updates | `zig build test-integration` memory tests green |
| 5 | §7 zombiectl CLI updates (envelope + requestId + memory GET) | manual `node zombiectl/bin/zombiectl zombies list` against dev or local |
| 6 | §6 auth RBAC extraction: create src/auth/rbac/workspace.zig, move fns, update handler imports | `zig build && zig build test-auth` |
| 7 | §5 openapi.json regeneration | manual diff review + contract grep |
| 8 | Cross-compile + full lint + gitleaks | all pass |
| 9 | Fresh-DB integration: `make down && make up && make test-integration` | green |

---

## Acceptance Criteria

- [ ] All 4 list endpoints return `items`/`total` shape — verify: integration tests in §1 green
- [ ] Memory `recall`/`list` respond to GET, 405 on POST — verify: tests in §2 green
- [ ] `grep -n request_id src/http/handlers/memory_http.zig` → 0 — verify: `grep -n "request_id" src/http/handlers/memory_http.zig; echo $?` non-zero exit or empty
- [ ] `grep -rn 'hx.res.status = 204' src/http/` → 0 — verify: same grep
- [ ] `public/openapi.json` has entry for every Route variant, no `/v1/runs/*` — verify: §5 contract script
- [ ] `grep -rn 'common.authorizeWorkspace\|common.getZombieWorkspaceId' src/` → 0 — verify: grep
- [ ] `zig build test-auth` passes — verify: command
- [ ] `zombiectl` builds and list commands work against refactored server — verify: local smoke
- [ ] `make test` + `make down && make up && make test-integration` both pass — verify: CI + local
- [ ] `make lint`, `make check-pg-drain`, cross-compile both targets, `gitleaks detect` all pass

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: list envelope grep — zero hits for old keys in handlers
grep -rnE '\.zombies = |\.agents = |\.grants = |\.events = ' src/http/handlers/ --include="*.zig" | grep -v _test.zig
echo "E1: list envelope (empty = pass)"

# E2: request_id in memory
grep -n 'request_id' src/http/handlers/memory_http.zig
echo "E2: request_id (empty = pass)"

# E3: inline 204
grep -rn 'hx.res.status = 204\|res\.status = 204' src/http/ --include="*.zig"
echo "E3: inline 204 (empty = pass)"

# E4: common.authorizeWorkspace refs
grep -rn 'common\.authorizeWorkspace\|common\.getZombieWorkspaceId\|common\.authorizeWorkspaceAndSetTenantContext' src/ --include="*.zig"
echo "E4: moved auth fn refs (empty = pass)"

# E5: stale /v1/runs/ in openapi
grep '/v1/runs' public/openapi.json
echo "E5: stale runs (empty = pass)"

# E6: old envelope keys in zombiectl
grep -rnE 'res\.(zombies|agents|grants|events)\b' zombiectl/src/
echo "E6: CLI old keys (empty = pass)"

# E7: build
zig build 2>&1 | tail -5; echo "build=$?"

# E8: full unit tests
make test 2>&1 | tail -5

# E9: integration tests (fresh DB)
make down && make up && make test-integration 2>&1 | tail -10

# E10: auth standalone
zig build test-auth 2>&1 | tail -5; echo "test-auth=$?"

# E11: cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E12: lint
make lint 2>&1 | tail -10

# E13: pg drain
make check-pg-drain 2>&1 | tail -5

# E14: gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E15: 350-line gate on diff
git diff --name-only origin/main \
  | grep -v -E '\.md$|^vendor/|_test\.|\.test\.|\.spec\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' \
  | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines" }'
```

---

## Dead Code Sweep

Three functions relocated (not deleted) — no files deleted, but `common.zig` shrinks.

| Symbol | Grep command | Expected |
|--------|--------------|----------|
| `common.authorizeWorkspace` | `grep -rn 'common\.authorizeWorkspace' src/ --include='*.zig'` | 0 in non-historical files |
| `common.getZombieWorkspaceId` | `grep -rn 'common\.getZombieWorkspaceId' src/ --include='*.zig'` | 0 |
| `common.authorizeWorkspaceAndSetTenantContext` | `grep -rn 'common\.authorizeWorkspaceAndSetTenantContext' src/ --include='*.zig'` | 0 |

No files deleted. No `@embedFile` changes. No migration array changes. Schema guard does not fire.

---

## Verification Evidence

Filled during VERIFY phase.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | TBD | |
| Integration tests (fresh DB) | `make down && make up && make test-integration` | TBD | |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | TBD | |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | TBD | |
| Lint | `make lint` | TBD | |
| Pg drain | `make check-pg-drain` | TBD | |
| Gitleaks | `gitleaks detect` | TBD | |
| test-auth standalone | `zig build test-auth` | TBD | |
| 350L gate | diff script | TBD | |
| Orphan sweep | E4 grep | TBD | |

---

## Out of Scope

- Full auth-as-separate-zig-build (producing a distinct `libauth.a` or separate exe target). This milestone establishes the src/auth/rbac/ boundary and keeps `test-auth` green; packaging as an independent artifact is a follow-up milestone.
- Making `AuthCtx.WriteErrorFn` fully generic over HTTP library. Kept as-is; auth layer continues to use the existing function-pointer indirection.
- OpenAPI 3.1 migration. Current spec stays on 3.0.x.
- Field-level camelCase audit outside memory endpoints. Other handlers already emit snake_case response fields broadly (`zombie_id`, `created_at`); a repo-wide camelCase migration is a separate scoping discussion.
- Pagination uniformity (cursor everywhere vs offset+limit). Only normalized where endpoints already paginate.
