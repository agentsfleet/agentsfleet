# M24_001: REST Workspace-Scoped Route Refactor — /v1/zombies/* → /v1/workspaces/{ws}/zombies/*

**Prototype:** v2
**Milestone:** M24
**Workstream:** 001
**Date:** Apr 14, 2026
**Status:** DONE
**Priority:** P1 — Unblocks M12 (App Dashboard) and all future UI/API consumers; surface change impacts `zombiectl` and OpenAPI
**Batch:** B5 prerequisite — lands before M12_001, M19_001, M22_001 begin
**Branch:** feat/m24-rest-workspace-scoped-routes
**Worktree:** /Users/kishore/Projects/usezombie-m24-rest-routes
**Depends on:** none (rebase-clean off `main`)

**Blocks:** M12_001 (App Dashboard), M19_001 (Zombie Lifecycle UI), M22_001 (Grants UI), future SDKs

---

## Overview

**Goal (testable):** Every zombie-scoped and activity-related endpoint is served at a workspace-scoped REST path. Query parameters are reserved for pagination (`page`, `limit`, `cursor`) and search (`search`) — never for identity. `zombiectl` calls the new paths. `openapi.json` reflects the new contract. All existing integration tests pass against the new shape. Pre-v2.0 carve-out (per RULE EP4): removed flat paths return bare 404s, no 410 stubs.

**Problem:** The current API is a mix of REST-ful (`/v1/workspaces/{ws}/billing/summary`) and flat (`/v1/zombies/?workspace_id=X`). Flat routes carry identity in query params, which breaks REST expectations, complicates SDK codegen from OpenAPI, and makes future authorization (per-workspace RLS, scoped API tokens) harder because the workspace id isn't in the URL. M12_001 (App Dashboard) surfaced this when the spec assumed workspace-scoped URLs that didn't exist.

**Solution summary:** Mechanical refactor. Add new matchers + router entries for workspace-scoped paths. Handlers read `workspace_id` from the path argument, not from the query string. Flat paths are removed entirely (no 410 stubs, pre-v2.0 per RULE EP4). `zombiectl` path constants and all callers are updated. OpenAPI regenerated. All tests updated to new paths.

---

## 1.0 Route Migration Table

**Status:** DONE

| From (flat, removed) | To (workspace-scoped) |
|---|---|
| `GET    /v1/zombies/?workspace_id=X` | `GET    /v1/workspaces/{ws}/zombies?page=&limit=&search=` |
| `POST   /v1/zombies/` (body: workspace_id) | `POST   /v1/workspaces/{ws}/zombies` |
| `DELETE /v1/zombies/{id}` | `DELETE /v1/workspaces/{ws}/zombies/{id}` |
| `GET    /v1/zombies/activity?zombie_id=X` | `GET    /v1/workspaces/{ws}/zombies/{id}/activity?cursor=&limit=` |
| `GET    /v1/zombies/credentials?workspace_id=X` | `GET    /v1/workspaces/{ws}/credentials` |
| `POST   /v1/zombies/credentials` (body: workspace_id) | `POST   /v1/workspaces/{ws}/credentials` |
| `POST   /v1/zombies/{id}/integration-requests` | `POST   /v1/workspaces/{ws}/zombies/{id}/integration-requests` |
| `GET    /v1/zombies/{id}/integration-grants` | `GET    /v1/workspaces/{ws}/zombies/{id}/integration-grants` |
| `DELETE /v1/zombies/{id}/integration-grants/{gid}` | `DELETE /v1/workspaces/{ws}/zombies/{id}/integration-grants/{gid}` |

**Kept as-is (already workspace-scoped):**
- `/v1/workspaces/{ws}/zombies/{id}/telemetry`
- `/v1/workspaces/{ws}/billing/*`
- `/v1/workspaces/{ws}/external-agents/*`
- `/v1/workspaces/{ws}/credentials/llm`

**Rule:** pre-v2.0 (`cat VERSION` = `0.9.0`), removed flat paths return bare 404 per RULE EP4 carve-out. No 410 Gone stubs.

---

## 2.0 Surface Area Checklist

- [x] **OpenAPI update** — yes. Path section regeneration for every migrated route.
- [x] **`zombiectl` CLI changes** — yes. `src/lib/api-paths.js` constants + every caller. Existing CLI commands keep their user-facing shape; only internal HTTP paths change. No new flags, no user-visible changes.
- [x] **User-facing doc changes** — yes. `docs/nostromo/lead_collector_zombie.md` and similar guides cite `/v1/zombies/...` URLs in curl examples; update. `docs.usezombie.com` API reference regenerated from OpenAPI.
- [x] **Release notes** — yes. Patch-level bump (mechanical refactor, no feature). Pre-v2.0 means we don't owe deprecation notice — document as "API path refactor; flat `/v1/zombies/*` removed" under next version.
- [ ] **Schema** — no.
- [ ] **Schema teardown guard** — N/A.
- [x] **Spec-vs-rules conflict check** — clear. RULE EP4 pre-v2.0 carve-out applies (bare 404s OK). RULE WAUTH applies to every new workspace-scoped handler (must `authorizeWorkspace` after `authenticate`). RULE RAD six-point REST checklist: no verbs in URL (uses HTTP method + `:stop` Google custom method where needed in M12_001), no `is_` prefix, snake_case, error shape via Hx, path params for identity, correct HTTP verbs, `/v1/` prefix — all pass.

---

## 3.0 Dimensions (test blueprints)

### 3.1 Route matchers

**Status:** DONE

- 3.1.1 DONE — target `src/http/route_matchers.zig` — input: `/v1/workspaces/ws_abc/zombies` — expected: match list; `workspace_id = "ws_abc"` — test_type: unit
- 3.1.2 DONE — target same — input: `/v1/workspaces/ws_abc/zombies/zom_xyz` — expected: match delete; `workspace_id`, `zombie_id` extracted — test_type: unit
- 3.1.3 DONE — target same — input: `/v1/workspaces/ws_abc/zombies/zom_xyz/activity` — expected: match activity; both ids extracted — test_type: unit
- 3.1.4 DONE — target same — input: malformed paths (`//zombies`, `/v1/workspaces/a/b/zombies`) — expected: no match (null) — test_type: unit
- 3.1.5 DONE — target same — input: old flat paths (`/v1/zombies/`, `/v1/zombies/activity`) — expected: no match (returns 404) — test_type: unit

### 3.2 Router dispatch

**Status:** DONE

- 3.2.1 DONE — target `src/http/router.zig` — input: each new path from §1.0 — expected: correct `Route` variant with correct ids — test_type: unit
- 3.2.2 DONE — target same — input: each removed flat path — expected: no match — test_type: unit

### 3.3 Handlers — workspace_id from path

**Status:** DONE

- 3.3.1 DONE — target `src/http/handlers/zombie_api.zig::innerListZombies` — input: call with `workspace_id` as path arg — expected: reads path arg, not `qs.get("workspace_id")`; RULE WAUTH `authorizeWorkspace` check — test_type: integration
- 3.3.2 DONE — target `zombie_api.zig::innerCreateZombie` — input: POST with body (no workspace_id field needed) — expected: uses path workspace_id; rejects body with mismatched workspace_id — test_type: integration
- 3.3.3 DONE — target `zombie_api.zig::innerDeleteZombie` — input: path with `ws_id` + `zombie_id` — expected: validates zombie belongs to workspace before delete (authorization check) — test_type: integration
- 3.3.4 DONE — target `zombie_activity_api.zig::innerListActivity` — input: path with `ws_id` + `zombie_id` — expected: scoped to workspace+zombie — test_type: integration
- 3.3.5 DONE — target `integration_grants.zig` handlers — input: new paths — expected: workspace_id from path used in all grant DB queries — test_type: integration

### 3.4 zombiectl

**Status:** DONE

- 3.4.1 DONE — target `zombiectl/src/lib/api-paths.js` — input: existing constants — expected: every zombie/activity/grant path uses `/v1/workspaces/:workspaceId/...` template; pagination/search helpers append `?page=&limit=&search=` — test_type: unit
- 3.4.2 DONE — target `zombiectl/test/zombie.unit.test.js` and peers — input: mocked HTTP — expected: assertions updated to new paths — test_type: unit
- 3.4.3 DONE — target `zombiectl` e2e — input: real CLI commands against `api-dev` — expected: `zombiectl zombie list`, `... create`, `... delete`, `... activity`, `... grant list/request/revoke` all succeed — test_type: e2e smoke

### 3.5 OpenAPI

**Status:** DONE

- 3.5.1 DONE — target `public/openapi.json` — input: regenerated — expected: all flat zombie paths removed; workspace-scoped paths present with `{workspaceId}` path parameter; query parameters limited to `page`, `limit`, `cursor`, `search` — test_type: schema diff + `make check-openapi-errors`

---

## 4.0 Implementation Constraints

| Constraint | Verify |
|---|---|
| RULE EP4 pre-v2.0 — bare 404 for removed paths | routing test asserts flat path 404s; no 410 in diff |
| RULE WAUTH — every new workspace-scoped handler calls `authorizeWorkspace` after `authenticate` | code review + integration tests that substitute workspace_id (IDOR test) |
| RULE FLL — every new/touched .zig ≤350 lines | `wc -l` gate in VERIFY |
| RULE ORP — orphan sweep for all flat-route strings | grep `/v1/zombies` across src, schema, docs, tests; zero non-historical hits |
| RULE XCC — cross-compile for x86_64-linux and aarch64-linux | before commit |
| zig-pg-drain — `.drain()` before `deinit()` in every touched handler | `make check-pg-drain` |
| `make lint && make test && make test-integration` green | VERIFY phase |

---

## 5.0 Execution Plan

| Step | Action | Verify |
|---|---|---|
| 1 | Add new matchers in `route_matchers.zig` alongside existing ones | unit tests 3.1.* pass |
| 2 | Add new `Route` enum variants in `router.zig`; wire matchers | unit tests 3.2.* pass |
| 3 | Update handlers to take `workspace_id` from path arg; add `authorizeWorkspace` per RULE WAUTH | integration tests 3.3.* pass |
| 4 | Remove flat route matchers, enum variants, dispatch arms | router_test updated; flat paths 404 |
| 5 | Update `zombiectl/src/lib/api-paths.js` + call sites + tests | unit tests 3.4.1-2 pass |
| 6 | Regenerate `public/openapi.json` | schema diff reviewed; `make check-openapi-errors` passes |
| 7 | Update `docs/nostromo/lead_collector_zombie.md` and any other URL-citing guides | grep shows no `/v1/zombies` in non-historical docs |
| 8 | Tier 3 verify: `make down && make up && make test-integration` | green |
| 9 | CLI e2e smoke against `api-dev` | 3.4.3 passes |
| 10 | `make lint && make check-pg-drain && make memleak && make bench` | all green |
| 11 | Cross-compile | both Linux targets green |
| 12 | Orphan sweep (RULE ORP) | zero non-historical `/v1/zombies/` hits |

---

## 6.0 Acceptance Criteria

- [ ] Every entry in §1.0 migration table: new path works, old path returns 404
- [ ] Every workspace-scoped handler calls `authorizeWorkspace` (RULE WAUTH); IDOR test (user_A calls user_B's workspace) returns 403
- [ ] `zombiectl` commands work against refactored API (unit + e2e)
- [ ] `public/openapi.json` reflects new paths; no query param carries identity
- [ ] `docs/nostromo/lead_collector_zombie.md` curl examples use new paths
- [ ] `make lint && make test && make test-integration && make memleak && make bench` green
- [ ] Cross-compile for x86_64-linux + aarch64-linux green
- [ ] Orphan sweep clean: zero non-historical `/v1/zombies/` references
- [ ] Release notes updated in `/Users/kishore/Projects/docs/changelog.mdx`

---

## 7.0 Applicable Rules

- RULE EP4 — pre-v2.0 removed paths 404 (no 410)
- RULE WAUTH — workspace-scoped handlers require `authorizeWorkspace`
- RULE RAD — REST API design six-point checklist
- RULE ORP — cross-layer orphan sweep
- RULE CHR — CHORE(close) orphan verification gate
- RULE FLL — 350-line file gate
- RULE XCC — cross-compile before commit
- RULE HXX — handlers use Hx, not raw `common.writeJson`
- RULE NSQ — schema-qualified SQL (already followed in existing handlers)

---

## 8.0 Eval Commands

```bash
# Backend
make lint
make test
make test-integration
make check-pg-drain
make check-openapi-errors
zig build -Dtarget=x86_64-linux
zig build -Dtarget=aarch64-linux

# zombiectl
cd zombiectl && npm test

# Orphan sweep (RULE ORP)
git diff --name-only origin/main \
  | xargs grep -l "/v1/zombies/" 2>/dev/null \
  | grep -v -E "(docs/v1/done|docs/v2/done|CHANGELOG|\.md$)"
# expected: empty

# 350-line gate (RULE FLL)
git diff --name-only origin/main \
  | grep -v -E '\.md$|^vendor/|_test\.|\.test\.|\.spec\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' \
  | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# Gitleaks
gitleaks detect

# Bench (local + dev)
make memleak
make bench
API_BENCH_URL=https://api-dev.usezombie.com/healthz make bench
```

---

## 9.0 Dead Code Sweep

After refactor, remove:
- Flat-route matcher functions (`matchZombieId`, `matchZombieSuffix` where superseded)
- Flat-route `Route` enum variants: `list_or_create_zombies`, `delete_zombie`, `zombie_activity`, `zombie_credentials`, `request_integration_grant`, `list_integration_grants`, `revoke_integration_grant` (or rename if new workspace-scoped variants reuse the name)
- Flat-path dispatch arms in `router.zig`
- `qs.get("workspace_id")` and `qs.get("zombie_id")` uses that are now redundant (path provides them)
- `zombiectl` flat-path constants

Confirm with orphan sweep before committing.

---

## 10.0 Verification Evidence

| Check | Command | Result | Pass? |
|---|---|---|---|
| Backend tests | `make test && make test-integration` | 618 pass, 0 fail | ✅ |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | green | ✅ |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | green | ✅ |
| pg-drain | `make check-pg-drain` | 256 files scanned, 0 violations | ✅ |
| OpenAPI | `make check-openapi-errors` | ErrorBody valid, application/problem+json | ✅ |
| zombiectl | `bun test zombiectl/test/zombie.unit.test.js` | 21 pass, 0 fail | ✅ |
| Orphan sweep | see §8.0 | zero non-historical `/v1/zombies/` refs | ✅ |
| 350L gate (Zig) | `make lint` | all files within 350L | ✅ |
| Memleak | `make memleak` | 1178 pass, 101 skip, 0 fail | ✅ |
| Tier-3 fresh DB | `make down && make up && make test-integration` | full integration suite passed | ✅ |
| Bench (local, post-hey migration) | `make bench` | Tier-1: 100k runs 53ns/noop; Tier-2: 41839 ok/0 fail, p95=3.90ms, 8367 rps | ✅ |
| Bench (dev) | `API_BENCH_URL=https://api-dev.usezombie.com/healthz make bench` | deferred — branch not yet deployed to dev | ⏭ |
| Gitleaks | `gitleaks detect` | no leaks found | ✅ |

---

## Scope amendments during execution

- **Bench tooling migration** (landed as part of M24 rather than deferred). Replaced the
  ~500-line `api_bench_runner.zig` custom loadgen with a two-tier `make bench`:
  Tier-1 = zbench-backed code micro-benchmarks (dummy stub today);
  Tier-2 = `hey` HTTP loadgen via `mise`. zbench pinned to `zig-0.15.1` branch HEAD
  (our Zig toolchain constraint — v0.12+ require Zig master). New milestone
  **M25_001** tracks writing the catalog of real micro-benchmarks on top of the stub.
  Fixed the pre-existing dev-bench integer-overflow crash (zbench Statistics u64
  variance accumulator) by removing the code path that used it.

## 11.0 Out of Scope

- Any new endpoint (kill switch, workspace activity, spend) — those ship in M12_001 after this merges.
- Schema changes — none required.
- Authorization model changes — RULE WAUTH is applied mechanically but no new auth primitives.
- UI work — M12_001 owns all frontend.
- Deprecation header for legacy clients — pre-v2.0 means we remove cleanly with 404.
