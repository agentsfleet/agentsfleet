# HANDOFF — M131_001 The Fleet Console

**Branch:** `feat/m131-fleet-console` (pushed) · **Worktree:** `~/Projects/agentsfleet-m131-fleet-console`
**Spec:** `docs/v2/active/M131_001_P1_API_UI_FLEET_CONSOLE.md` (amended — read it, esp. §8/§9 and Discovery)
**Test Baseline:** unit=2642 integration=334 → **backend now unit=2658 integration=343** (+16 / +9); UI console rebuild adds ~30 unit tests on top (re-run `make _lint_zig_test_depth` at VERIFY for the final delta).

## State: backend + client API + **UI console DONE and green**; remaining = OpenAPI, rebase+renumber, VERIFY/CHORE(close)

Commits on the branch (newest first):
- `00ac775b1` **UI: three-column console** — SkillEditor/MemoryPanel/RunsLedger/RunMetricsStrip + FleetConfig copy fix + page.tsx rebuild + all component tests + e2e + 2 analytics events. All harness gates + oxlint + tsc green; app unit tests green.
- `67af829fc` fleets-routes page mocks follow the getFleet detail contract
- `789eb2984` handoff notes
- `d16ac8eff` client API layer (getFleet+ETag, memory.ts, cost, FleetDetail/MemoryEntry, interim page.tsx)
- `3e1439df7` denormalized activity counters (migration 028 + triggers) + all 6 backend integration tests
- `0392603f0` backend: fleet read+ETag, event cost, memory forget, list query, shared http/etag.zig, catalog ETag
- `74ed4f188` CHORE(open)

**`00ac775b1` is committed but NOT pushed** (branch is 1 ahead of origin). `origin/main` is 9 commits ahead of the branch's merge-base (M132's Live Wall landed — see rebase note below).

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

## UI console — DONE (commit `00ac775b1`)

All under `.../fleets/[id]/components/` unless noted. **New:** `console-copy.ts` (named copy — RULE UFS), `SkillEditor.tsx` (+`.test.tsx`), `MemoryPanel.tsx` (+test), `RunsLedger.tsx` (+test), `RunMetricsStrip.tsx` (+test), `FleetConfig.test.tsx`, `components/domain/SteerComposer.test.tsx`, `tests/e2e/acceptance/fleet-console.spec.ts`. **Edited:** `page.tsx` (three-column rebuild), `loading.tsx`, `FleetConfig.tsx` (§7 copy), `CronCard.test.tsx` (added `test_triggers_render_read_only`), `fleets/actions.ts` (added `getFleetDetailAction`/`saveFleetSourceAction`/`forgetMemoryAction`), `lib/analytics/events.ts` (2 events), `tests/fleets-routes.test.ts` (page assertions + `/memories` mocks), `tests/helpers/dashboard-mocks.tsx` (BrainIcon), `tests/e2e/acceptance/logs-detail.spec.ts` (region selectors).

Every named unit test in the spec Test Specification exists and passes. Gotchas encountered + resolved: cost uses `formatDollars` from `settings/billing/lib/charges` (console cost = invoice — Product Clarity #6); the UI gate forbids raw `<article>/<section>/<form>` — sections are `<Section asChild><section>` (the `<Section asChild>` must be verbatim, no `key` — use a keyed `<Fragment>` in a `.map`), card rows are plain `<Card>` (renders a div); oxlint forbids `!` non-null assertions (the diff uses a flat lcs array with `?? 0`/`?? ""`). Server actions + RunMetricsStrip.tsx + several test files are **not in the spec's Files Changed table** — reconcile the table at CHORE(close) (amend, don't scope-cut).

## Remaining work (in order)

1. **OpenAPI** (task 11, NOT started — do it AFTER the rebase so `root.yaml`/`openapi.json` reconcile once): `public/openapi/paths/fleets.yaml` (GET `{id}` + events `cost_nanos`, description names the `cost_nanos`↔`credit_deducted_nanos` map), `memory.yaml` (`{key}` DELETE), catalog `etag` field + `If-Match`/412 on the fleet-library PATCH, `root.yaml` registration, regen `public/openapi.json` via `make check-openapi` (never hand-edit). Migration is internal — no OpenAPI change.
2. **Rebase + renumber** (task 12): `origin/main` is 9 ahead (M132 Live Wall). Integrate main (merge preferred — force-push needs Indy approval; the branch is pushed w/ no PR yet). **Renumber my `schema/028_fleet_activity_counters.sql` → `029`** (main already has `028_core_user_preferences.sql`), update `schema/embed.zig`, resolve conflicts in `embed.zig` + `route_table_invoke.zig` (both M131 & M132 edited) + `public/openapi/root.yaml` + `openapi.json`. **Verify DB via `make test-integration-db`** (drops everything via teardown.sql — hand-migration won't reset the `audit`-schema `schema_migrations`).
3. **VERIFY + CHORE(close)** (task 13): grade the Acceptance Rubric (R1–R11, S1–S9 — R9 grep = "no aggregate over child tables in list.zig/sql.zig", R10 = "one Sha256 under http/"), `make test-integration-db`, `make test-unit-all`, `make lint-all`, `make memleak`, `gitleaks detect`, cross-compile both linux targets, Test Delta row (`make _lint_zig_test_depth` vs baseline), `/write-unit-test` (confirm clean), `/review`, changelog `<Update>` + affected pages in `~/Projects/docs`, reconcile the spec Files-Changed table + mark Dimensions DONE + move spec to `done/`, delete this HANDOFF, open PR, `kishore-babysit-prs`.

## Fast repro of the perf win (if asked)
Seed 100 fleets × 3000 events into a workspace (uuidv7 ids via `overlay(gen_random_uuid()::text placing '7' from 15 for 1)`), then `EXPLAIN ANALYZE` the list query — 0.999ms (index scan) vs the old subselect shape's 1773ms. Triggers verified exact against 300k/600k rows incl. the renewal-delta path.
