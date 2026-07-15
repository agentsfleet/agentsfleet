# HANDOFF — M131_001 The Fleet Console

**Branch:** `feat/m131-fleet-console` (pushed) · **Worktree:** `~/Projects/agentsfleet-m131-fleet-console`
**Spec:** `docs/v2/active/M131_001_P1_API_UI_FLEET_CONSOLE.md` (amended — read it, esp. §8/§9 and Discovery)
**Test Baseline:** unit=2642 integration=334 → **now unit=2658 integration=343** (+16 / +9)

## State: backend + client API DONE and green; UI console rebuild NOT started

Four commits on the branch (newest first):
- `d16ac8eff` client API layer (getFleet+ETag, memory.ts, cost, FleetDetail/MemoryEntry, interim page.tsx)
- `3e1439df7` denormalized activity counters (migration 028 + triggers) + all 6 backend integration tests
- `0392603f0` backend: fleet read+ETag, event cost, memory forget, list query, shared http/etag.zig, catalog ETag
- `74ed4f188` CHORE(open)

**Verified green:** `make lint-zig` · `make test-integration-db` (full DB suite) · `zig build test` · cross-compile both linux targets · app `tsc --noEmit` (0 errors) · `bunx vitest run lib/api …` (252 UI tests). Memleak / gitleaks / full `make lint-all` NOT yet run (do at VERIFY).

## What shipped (backend, all committed + tested)

| § | What | Where |
|---|---|---|
| §1 | `GET …/fleets/{id}` single-fleet read + ETag header; 404-not-403 cross-workspace | `handlers/fleets/get.zig`, `sql.zig`, route GET arm in `route_table_invoke.zig`, scope in `route_scopes.zig` |
| §2 | `cost_nanos` on the event row (summed telemetry subselect, index-bounded ≤2 rows) | `state/fleet_events_store.zig`, `handlers/fleets/events.zig` |
| §4 | If-Match on PATCH → 412 + current etag; fresh etag on 200 | `handlers/fleets/patch.zig` + `patch_body.zig` + `patch_txn.zig` (split for FLL) |
| §5 | tenant memory forget `DELETE …/memories/{key}` (fleet:write, 204/404, fleet-isolated) | `memory/fleet_memory.zig` deleteEntry, `memory/handler.zig` innerDeleteMemory, route plumbing (6 places) |
| §8 | **denormalized counters** `events_processed`+`budget_used_nanos` on `core.fleets`, maintained by migration-028 triggers. Measured: 1773ms → **0.999ms** at 300k events. | `schema/005` (columns), `schema/028_fleet_activity_counters.sql` (triggers+backfill), `embed.zig`, `handlers/fleets/sql.zig`, `list.zig` |
| §9 | shared `http/etag.zig` capability; **catalog row** (`library/catalog_patch.zig`) is the 2nd adopter — a stale re-send there is destructive (repoints source → nulls bundle → unpublishes), now 412'd | `http/etag.zig`, `library/catalog.zig` (RowState + rowSurface + entry etag), `library/catalog_patch.zig`, `fleet_library/sql.zig` |

New error codes: `UZ-AGT-014` (fleet source stale), `UZ-CATALOG-005` (catalog stale), `UZ-MEM-004` (memory not found). RFC 7807 writer split to `handlers/problem_response.zig`, gained the `etag` 412 extension.

Client API (committed): `lib/api/client.ts` (`requestWithEtag`+`etagFrom`), `errors.ts` (`.etag`), `fleets.ts` (`getFleet`→`{fleet,etag}`, `saveFleetSource`), `lib/api/memory.ts` (new), `events.ts` (`cost_nanos`), `types.ts` (`FleetDetail`, `MemoryEntry`). `page.tsx` is **adapted, not rebuilt** — it compiles against the new `getFleet` but is still the old single-column panel stack.

## ⚠️ Load-bearing gotchas for the next agent

1. **Migration 028 collides with M132's 028** (`028_core_user_preferences.sql`, in the parallel `agentsfleet-m132-live-wall` worktree). Whichever rebases onto `main` second **renumbers to 029** (there's a contiguity test in `cmd/common.zig` — no gaps allowed). Note in `embed.zig`.
2. **Editing an applied migration doesn't re-run it.** The counter columns live in `schema/005` (SCHEMA GUARD blocks `ALTER TABLE` pre-2.0; teardown-rebuild re-runs 005). Local test DB: `schema_migrations` lives in the `audit` schema — a partial `DROP SCHEMA core` won't reset it. Use `make test-integration-db` (runs `teardown.sql` which drops everything) rather than hand-migrating.
3. **The counter triggers guard the `::uuid` cast** — telemetry `fleet_id` is TEXT with no FK, so a non-UUID id (test fixtures like `"fleet-telem-a"`) updates nothing rather than erroring. Don't remove the regex guard.
4. **The test harness `Response` captures status+body only, NOT headers** — the GET ETag is proven via the PATCH body round-trip + `etag.zig` unit tests, not a header assertion. If you need header assertions, extend the harness (`test_http_message.zig` uses `client.fetch` which drops headers).
5. **`bash audits/ufs.sh` (pre-commit) is slow (~2min+)** — commit in the background or it times out the tool call. Numeric test literals need `// pin test: literal is the contract`.
6. The `getFleet` return changed to `{ fleet, etag }` — any NEW caller must destructure.

## Remaining work (UI-heavy — spec §3/§4/§5/§6/§7 + OpenAPI + e2e)

Rebuild `page.tsx` into the **three-column console** and its components (all NEW under `.../fleets/[id]/components/`):
- `console-copy.ts` (named copy constants — RULE UFS), `SkillEditor.tsx` (§4: viewer→edit, next-wake save dialog, "what changes" diff, If-Match 412 reload-and-rediff via `saveFleetSource` + `ApiError.etag`), `MemoryPanel.tsx` (§5: content/category/updated_at + forget), `RunsLedger.tsx` (§6: events + cost column + client 7-day rollup over `?since=7d` + lifetime `budget_used_nanos`; null cost renders `—`, counts as a wake with 0 spend), `RunMetricsStrip.tsx` (§3: tokens·wall·cost server truth). Edit `FleetConfig.tsx` (§7: remove the stale "endpoints don't exist" copy; delete-confirm states the memory trap). Reuse `FleetThread`/`SteerComposer` **unchanged** (middle column).
- Per-component `.test.tsx` (spec Test Specification has the exact test names + asserts), plus `tests/e2e/acceptance/fleet-console.spec.ts` and selector updates in `logs-detail.spec.ts` + `fleet-thread.spec.ts`.
- Analytics events `fleet_source_saved` / `fleet_memory_forgotten` in `lib/analytics/events.ts` (spec Metrics table — no source/content in properties).
- **OpenAPI** (task 9, not started): `public/openapi/paths/fleets.yaml` (GET `{id}` + events `cost_nanos`), `memory.yaml` (`{key}` DELETE), `root.yaml`, regen `public/openapi.json` via `make check-openapi` (never hand-edit). Also add the catalog `etag` field + `If-Match`/412 to the fleet-library path. Migration 028 note: no OpenAPI change (internal).
- **VERIFY + CHORE(close):** grade the Acceptance Rubric (R1–R11, S1–S9 — note R9/R10 greps changed: §8 is now "no aggregate over child tables", R10 is "one Sha256 under http/"), `make memleak`, `gitleaks detect`, `/write-unit-test`, `/review`, changelog `<Update>` in `~/Projects/docs`, spec Dimensions → DONE + move to `done/`, `kishore-babysit-prs` after PR.

## Fast repro of the perf win (if asked)
Seed 100 fleets × 3000 events into a workspace (uuidv7 ids via `overlay(gen_random_uuid()::text placing '7' from 15 for 1)`), then `EXPLAIN ANALYZE` the list query — 0.999ms (index scan) vs the old subselect shape's 1773ms. Triggers verified exact against 300k/600k rows incl. the renewal-delta path.
