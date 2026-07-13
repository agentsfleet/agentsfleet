<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M128_001: Runtime platform catalog — create, curate, publish from /admin/fleet-libraries

**Prototype:** v2.0.0
**Milestone:** M128
**Workstream:** 001
**Date:** Jul 13, 2026
**Status:** DONE
**Priority:** P1 — operator-facing; the catalog is unreadable from the dashboard, a fleet can only be born in a migration, and a successful onboard is instantly live to every tenant with no review and no way back
**Categories:** API, UI
**Batch:** B1 — single workstream, no parallel siblings
**Branch:** feat/m128-runtime-catalog
**Test Baseline:** unit=2585 integration=311 (Zig depth gate; the app's vitest/playwright delta is reported alongside it in VERIFY)
**Depends on:** M127_001 (done) — it built the onboard write and named the read route as its follow-up
**Provenance:** LLM-drafted (claude-opus-4-8, Jul 13, 2026)
**Canonical architecture:** `docs/architecture/fleet_bundles.md` §Two-tier catalog

---

## Overview

**Goal (testable):** An operator holding `platform-library:write` manages the whole platform catalog from `/admin/fleet-libraries` without touching Structured Query Language (SQL) — add a fleet by its repository, edit its install-gate copy, publish it to every tenant, withdraw it, fetch a newer bundle, delete a draft — and no fleet reaches a tenant gallery or an install until it is explicitly published.

**Problem:** Three failures compound on one surface. **(1) The catalog is unreadable** — the page is a submit box whose only memory is React state, so an operator cannot tell whether the platform offers four live fleets or zero. **(2) A fleet can only be born in a migration** — `core.fleet_library` rows are seeded by `schema/023_fleet_library.sql` with curated copy the importer cannot derive, so a fifth first-party fleet means writing SQL and shipping a backend; onboarding only ever *fills a blank someone else punched*. **(3) Publishing is an accident** — every seeded row is already `visibility='public'`, so the instant an onboard writes a `content_hash` the bundle is live in every tenant's gallery, with no review step and, absent a delete route, no way back short of a production SQL console.

**Solution summary:** Make the catalog runtime data. The bundle's `SKILL.md` frontmatter already yields id, name, description, credentials, tools, and hosts, so **the seed rows are deleted** and creating a fleet becomes a dashboard action: point at a repository, the server fetches and validates it, and the row is born a **draft**. The one thing a bundle cannot supply — the platform's install-gate "why this fleet needs your token" copy — becomes editable via `PATCH` rather than a migration. Publishing is then explicit and reversible, and it is the *only* door to a tenant: the vestigial `visibility` column (`core.fleet_library` has always held `'public'` on every row, because tenant entries live in a different table) becomes the lifecycle field `draft | public`, which the gallery query *already* filters on. **No schema change** — `ALTER TABLE` is gate-blocked below v2.0.0 anyway; the only migration is data-only. Installs snapshot the bundle onto `core.fleets.bundle_content_hash`, so withdrawing or deleting a catalog row can never disturb a tenant already running that fleet. The domain folder is renamed `fleet_bundle` → `fleet_library`, which is what it has always held.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api,ui): runtime platform catalog — create, curate, publish
- **Intent (one sentence):** Let a platform operator run the fleet catalog entirely from the dashboard — add, edit, publish, withdraw, delete — so no fleet reaches a tenant without a deliberate publish, and no fleet needs a SQL migration to exist.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/admin_models.zig` + `route_table_invoke.zig` (`invokeAdminModels`, `invokeAdminModelById`) — the pattern to mirror: a collection route (GET/POST) and a by-id route (PATCH/DELETE), per-method dispatch, `respondMethodNotAllowed` on the else arm, per-method scopes in `route_scopes.zig`.
2. `src/agentsfleetd/fleet_bundle/sql.zig` + `library_store.zig` (renamed to `fleet_library/` by §1) — every query against `core.fleet_library` lives in one file. Read `INSERT_PLATFORM`'s `ON CONFLICT` list, `SELECT_PLATFORM_INSTALL`, `SELECT_GALLERY_PLATFORM`, and `SELECT_BUNDLES_LIST` first; §1 and §3 turn on exactly what those four do and do not filter.
3. `src/agentsfleetd/http/handlers/library/onboard.zig` — the create path: fetches the repository, validates, writes the canonical tar to object storage, derives the catalog id from the frontmatter `name:`. This spec changes what it writes, not how it fetches.
4. `ui/packages/app/app/(dashboard)/admin/models/` — the scope-guarded admin page that reads, tabulates, and edits (`page.tsx`, `CatalogueList.tsx`, `EditModelDialog.tsx`); Fleet libraries becomes this shape. Then `docs/SCHEMA_CONVENTIONS.md` §Migration Model + `schema/028_fleet_library_catalog_reconcile.sql` — why the only legal migration here is data-only, and the shape one takes.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/fleet_bundle/**` → `src/agentsfleetd/fleet_library/**` | RENAME | Holds the library store, catalog SQL, importer — `fleet_library` is what it has always been |
| `schema/023_fleet_library.sql` | EDIT | Delete the seed `INSERT`; the table DDL stays. A fresh database starts empty |
| `schema/028_fleet_library_catalog_reconcile.sql` | EDIT | Seed `INSERT` stripped (it would re-seed a fresh database, breaking Invariant 5). File and registration KEPT — `cmd/common.zig` asserts migration versions are contiguous; see Discovery |
| `schema/029_fleet_library_draft_normalize.sql` | CREATE | Data-only: bundle-less rows on a deployed database become `draft` |
| `schema/embed.zig` | EDIT | Register 29 (`canonicalMigrations()` derives from this array, so `cmd/common.zig` needs no edit) |
| `src/agentsfleetd/fleet_library/sql.zig` | EDIT | Admin list projection; `INSERT_PLATFORM` upsert semantics; publish/unpublish/delete; `SELECT_PLATFORM_INSTALL` gains the published filter |
| `src/agentsfleetd/fleet_library/library_store.zig` | EDIT | `draft`/`public` constants; the store fns behind the new routes |
| `src/agentsfleetd/http/handlers/library/{catalog,api}.zig` | CREATE/EDIT | `GET` list, `PATCH` curate/publish, `DELETE` draft; export the new inner handlers |
| `src/agentsfleetd/http/handlers/library/onboard.zig` | EDIT | Rows are born `draft`; same-id-different-repo → 409 unless `replace` |
| `src/agentsfleetd/http/{routes,router,route_table,route_table_invoke_library,route_scopes}.zig`, `errors/error_registry.zig` | EDIT | The `admin_fleet_library_by_id` route (variant, match, wiring, dispatch, scope); codes for publish-without-bundle, delete-published, id-collision |
| `src/agentsfleetd/http/handlers/library/catalog_integration_test.zig` | CREATE | Lifecycle, scope, publish gating, delete guard, 409 |
| `src/agentsfleetd/http/handlers/{library/onboard,fleet_bundles/api}_integration_test.zig` | EDIT | Seed-based assertions repointed at created rows |
| `public/openapi/paths/fleet-library.yaml`, `public/openapi/components/**` | EDIT | GET/PATCH/DELETE + the by-id path; entry and patch-body schemas |
| `ui/packages/app/lib/{api/fleet-library,types,analytics/events}.ts` | EDIT | Clients; `PlatformCatalogEntry` + status; `platform_library_published` |
| `…/admin/fleet-libraries/{page,actions,loading,library-copy}` | EDIT | Server-side read; patch/delete actions; every write revalidates; table skeleton; status labels, headers, action verbs (UFS) |
| `…/admin/fleet-libraries/components/FleetLibrariesView.tsx` | EDIT | Hosts the table; the session-scoped result card is removed |
| `…/components/{PlatformCatalogTable,EditFleetDialog,DeleteFleetDialog}.tsx` | CREATE | Rows/status/row-actions; the pencil; the destructive confirm, draft-only |
| `…/components/OnboardPlatformLibraryDialog.tsx` → `AddFleetDialog.tsx` | RENAME | Also serves "Fetch update" with a prefilled repository |
| `ui/packages/app/tests/admin-fleet-libraries-page.test.ts`, `…/components/*.test.tsx` | EDIT/CREATE | Read path, failure path, scope guard; table, Edit and Add dialogs |
| `ui/packages/app/tests/e2e/acceptance/platform-library-onboarding.spec.ts` | EDIT | The full lifecycle as one user path |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (status labels, action verbs, route paths, the `draft`/`public` wire values → named constants shared verbatim across Zig and TypeScript), NDC (every affordance has a route behind it), NLR (touched files leave cleaner — the session-card state goes, it is not left beside the table), NLG (the seed is deleted, not deprecated-in-place; the folder is renamed, not aliased), ORP (deleted seed, renamed folder, renamed dialog, removed copy constants all swept to zero), TST-NAM, FLL.
- `dispatch/write_zig.md` — `conn.query()` → `.drain()` in the same function, tagged-union results, `errdefer` placement, cross-compile both linux targets.
- `docs/SCHEMA_CONVENTIONS.md` §Migration Model — **no `ALTER TABLE`/`DROP` below v2.0.0** (`check-schema-gate`). The seed deletion is an inline DDL edit; the deployed-database reconciliation is data-only, exactly as `028` was.
- `docs/REST_API_DESIGN_GUIDELINES.md` — collection + by-id shapes, `PATCH` partial-update semantics, error envelope; §7 route-registration facts stay true (`make check-route-registration-doc`).
- `dispatch/write_ts_adhere_bun.md` — the User Interface (UI) half: FILE SHAPE DECISION at PLAN, design-system primitives only, no arbitrary Tailwind values. Analytics single-source discipline (`lib/analytics/events.ts` header) — the new event lands in `EVENTS`, `EventProps`, and `EVENT_PROP_KEYS` together.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE + PUB / Struct-Shape | yes — new handlers, SQL, routes, folder rename | pg-drain in the same fn; cross-compile both linux targets; `make memleak` over the new query paths; FILE SHAPE DECISION per new pub surface at PLAN, mirroring `gallery.zig`'s projection discipline |
| File & Function Length (≤350/≤50/≤70) | yes | `catalog.zig` holds list/patch/delete and will approach the cap — split by verb before it does; the view delegates to table + dialog components |
| UFS (repeated/semantic literals) | yes | `draft`/`public`, labels, verbs, paths each live once per runtime; the two wire values are byte-identical across Zig and TypeScript |
| UI Substitution / DESIGN TOKEN | yes | design-system table/badge/dialog/button primitives; theme tokens only |
| SCHEMA GUARD | **yes — fires loudly** | The diff deletes seed rows and adds a migration. **No `ALTER`/`DROP TABLE`/`DROP COLUMN`** — `023`'s DDL is untouched, only its `INSERT` goes; `029` is `UPDATE`-only. Removal-Guard output belongs in PLAN |
| LOGGING / ERROR REGISTRY / MILESTONE-ID | LOGGING yes; **new codes**; ID yes | Publish-without-bundle, delete-published, id-collision each need a registry `UZ-…` code — minted in the registry, never inline. Spec id in commit messages |

## Prior-Art / Reference Implementations

- **Reference:** `admin_models` end to end — `/v1/admin/models` (GET+POST) plus `/v1/admin/models/{id}` (PATCH/DELETE), per-method scopes, method dispatch in the invoke fns; paired with the `admin/models` page, its `CatalogueList` table and `EditModelDialog`. This spec is that Create-Read-Update-Delete shape applied to the fleet catalog. Justified divergence: `admin_models` splits read and write scopes; this route does not — see §2's default.

## Sections (implementation slices)

### §1 — The catalog becomes runtime data

Delete the seed. A `core.fleet_library` row is no longer born in a migration carrying copy the importer cannot derive; it is born when an operator points at a repository, because the bundle's frontmatter already supplies id, name, description, credentials, tools, and hosts. The lifecycle field is the existing `visibility` column — vestigial, since tenant entries live in `core.tenant_fleet_library`, so it has only ever held `'public'` on every row. **`draft`** = bundle stored, invisible to tenants. **`public`** = live. The folder rename lands here too: `fleet_bundle/` has held the library store, catalog SQL, and importer since M103.

**Implementation defaults.** (a) Every write stages to `draft` — `INSERT_PLATFORM`'s `ON CONFLICT` list gains `visibility`, so fetching a newer bundle for a published fleet returns it to draft rather than shipping it unreviewed; the publish gate must cover updates, not just first creation. (b) `description` is **removed** from that `ON CONFLICT` list and `required_credentials_reasons` stays out of it (it already is) — both are operator-owned after creation, and a refetch that clobbered the operator's edit would make §5's pencil a lie; a brand-new row still takes its description from the bundle at `INSERT`. (c) The deployed database keeps its four rows — they carry curated copy and are exactly what a create would produce minus the bundle; `029` only normalizes them (`content_hash IS NULL` ⇒ `draft`), because a bundle-less row claiming to be public is the accident this milestone exists to end.

- **Dimension 1.1** — DONE — a fresh database has an empty `core.fleet_library`; no migration anywhere inserts a catalog row → Test `test_fresh_database_catalog_empty`
- **Dimension 1.2** — DONE — on a database that already applied `023`, every bundle-less row becomes `draft` and no row with a bundle is touched; re-running changes nothing → Test `test_draft_normalize_is_idempotent_and_scoped`
- **Dimension 1.3** — DONE — creating from a repository writes a `draft` row whose id, name, description, credentials, tools, and hosts all come from the bundle → Test `test_create_derives_row_from_bundle`
- **Dimension 1.4** — DONE — fetching a newer bundle for a published fleet rewrites its bundle fields and returns it to `draft`, preserving the operator's description and install-gate copy → Test `test_refetch_drafts_and_preserves_curated_copy`
- **Dimension 1.5** — DONE — creating from a repository whose frontmatter id already exists under a *different* source repository is rejected 409 unless `replace` is set → Test `test_create_rejects_id_collision_without_replace`
- **Dimension 1.6** — DONE — no `fleet_bundle` path or import survives the rename; the build is green under the new folder → Test `test_no_fleet_bundle_references`

### §2 — The catalog routes

The admin plane's only resource with no read, no update, no delete. Add them: a collection route (`GET` list, `POST` create/refetch) and a by-id route (`PATCH` curate/publish, `DELETE` draft).

**Implementation defaults.** (a) Every method requires `platform-library:write` — no `platform-library:read` rung. Scopes are hand-provisioned in Clerk (`docs/AUTH.md` §Manually-provisioned), so a read rung would lock today's operators out of the page until a human re-provisioned them, to serve a read-only auditor role nobody has; `route_scopes.zig` resolves per method anyway, so splitting later is a one-line change. (b) No pagination, filter, or search — a curated first-party list, ordered by id.

- **Dimension 2.1** — DONE — `GET` returns every row, draft and published, with metadata only: never `skill_markdown`, `trigger_markdown`, or an object-store key → Test `test_admin_catalog_lists_all_rows_without_bodies`
- **Dimension 2.2** — DONE — `PATCH` updates the description and the per-credential install-gate copy, leaving bundle-derived fields untouched → Test `test_patch_updates_curated_copy_only`
- **Dimension 2.3** — DONE — `PATCH` publishes and unpublishes; publishing a row with no bundle is rejected with a registry code → Test `test_publish_requires_a_bundle`
- **Dimension 2.4** — DONE — `DELETE` removes a draft; deleting a published row is rejected with a registry code → Test `test_delete_rejects_published_row`
- **Dimension 2.5** — DONE — a token without the scope gets 403 `UZ-AUTH-022` on every method; an unsupported method gets 405 → Test `test_catalog_routes_scope_and_method_enforced`
- **Dimension 2.6** — DONE — every new method is documented in the split OpenAPI source and route coverage passes → Test `test_openapi_covers_catalog_routes`

### §3 — Publish is the only door to a tenant

An unpublished fleet must be unreachable, not merely unlisted. The gallery already filters `visibility = 'public'`, but `SELECT_PLATFORM_INSTALL` — the resolve-by-id path the install flow uses — does **not**, so today a draft could be installed by anyone who knows its id. Without this slice, Unpublish is decoration.

- **Dimension 3.1** — DONE — installing a draft fleet by id fails; publishing it makes the same install succeed → Test `test_draft_fleet_is_not_installable_by_id`
- **Dimension 3.2** — DONE — a draft appears in neither the workspace gallery nor `GET /v1/fleets/bundles`; publishing surfaces it in both → Test `test_draft_absent_from_gallery_and_bundles`
- **Dimension 3.3** — DONE — unpublishing removes it from both surfaces and blocks new installs, while a workspace that already installed it keeps running (its fleet holds its own `bundle_content_hash`) → Test `test_unpublish_leaves_existing_installs_intact`

### §4 — Clients, types, and the table

The surface an operator reads: one client per route, one status derivation, one row per entry in server order, with Status answering the question the page exists to answer.

**Implementation defaults.** (a) Status is derived, never a wire field — the server returns `visibility` and a nullable `content_hash`, and one exported function maps them to `No bundle | Draft | Published`; two sources for one fact is how a table starts lying. (b) The content hash renders truncated with the full value available on the row — an operator compares hashes to confirm a refetch changed something.

- **Dimension 4.1** — DONE — the clients call `GET`/`PATCH`/`DELETE` on the right paths with the bearer token → Test `test_catalog_clients_call_admin_endpoints`
- **Dimension 4.2** — DONE — status derivation: `public` ⇒ Published; `draft` + hash ⇒ Draft; `draft` + no hash ⇒ No bundle → Test `test_catalog_status_derivation`
- **Dimension 4.3** — DONE — the page reads the catalog server-side and renders one row per entry, keyed by catalog id, each with its status; a No-bundle row shows no hash → Test `test_catalog_table_renders_rows`, `test_catalog_row_status_rendering`
- **Dimension 4.4** — DONE — the empty state renders only when the catalog is genuinely empty, and invites the first Add → Test `test_catalog_empty_state_only_when_no_rows`
- **Dimension 4.5** — DONE — a read failure renders the mapped `presentError` presentation, never an empty table pretending the catalog is empty → Test `test_catalog_read_failure_surfaces_error`

### §5 — Row actions: add, edit, publish, fetch, delete

The operator's actual work. Every affordance has a route behind it, and no affordance is rendered for a state it cannot serve — a disabled button is a promise.

**Implementation defaults.** (a) The action set is derived from row state: **No bundle** → Fetch bundle, Edit, Delete. **Draft** → Publish, Fetch update, Edit, Delete. **Published** → Unpublish, Fetch update, Edit. No Delete on a published row — withdraw first; the route enforces the guard and the User Interface (UI) simply does not offer it. (b) Add and Fetch share one dialog — same validation, same double-submit guard, same error mapping — differing only in a prefilled repository; a second onboarding form is a second place for the validation to drift.

- **Dimension 5.1** — DONE — "Add fleet" creates from a typed repository; the new row appears as a draft after revalidation → Test `test_add_fleet_creates_draft_row`
- **Dimension 5.2** — DONE — a row's Fetch action opens the dialog prefilled with that row's repository; success returns the row to draft → Test `test_fetch_update_prefills_and_drafts`
- **Dimension 5.3** — DONE — Edit saves the description and per-credential install-gate copy; a failed save keeps the dialog open with the mapped error → Test `test_edit_dialog_saves_curated_copy`
- **Dimension 5.4** — DONE — Publish flips a draft to Published and Unpublish flips it back, each revalidating the table → Test `test_publish_unpublish_round_trip`
- **Dimension 5.5** — DONE — Delete is offered only on unpublished rows, behind a destructive confirm naming the fleet → Test `test_delete_offered_only_when_unpublished`
- **Dimension 5.6** — DONE — a create whose id collides returns 409 and the dialog offers an explicit Replace, which retries with `replace` set → Test `test_collision_offers_explicit_replace`

## Interfaces

```
GET    /v1/admin/fleet-libraries        → 200 { "entries": [ CatalogEntry ] }
POST   /v1/admin/fleet-libraries        { "source_kind", "source_ref", "replace"?: bool }
                                        → 201 CatalogEntry (always visibility="draft")
                                        → 409 id exists under a different source_repo (no `replace`)
PATCH  /v1/admin/fleet-libraries/{id}   { "description"?, "required_credentials_reasons"?,
                                          "published"?: bool }   → 200 CatalogEntry
                                        → 409 publish attempted with no bundle
DELETE /v1/admin/fleet-libraries/{id}   → 204 · 409 if the row is published

All four: scope platform-library:write · 403 UZ-AUTH-022 without it · 405 on other methods.

CatalogEntry = { id, name, description, source_repo, source_ref, support_file_count, updated_at,
                 visibility: "draft" | "public",  content_hash: string | null,  // null ⇒ no bundle yet
                 requirements: { credentials, tools, network_hosts, trigger_present },
                 required_credentials_reasons: { [credential]: string } }
NEVER returned: skill_markdown, trigger_markdown, support-file bodies, object-store keys.

catalogStatus(entry) → "no_bundle" | "draft" | "published"   (single derivation site)
EVENTS.platform_library_published  props: { entry_id, action: "published" | "unpublished" }
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Scope missing (route) | token without the scope, any method | 403 `UZ-AUTH-022` → `test_catalog_routes_scope_and_method_enforced` |
| Publish without a bundle | operator publishes a No-bundle row | 409 + registry code; row stays draft → `test_publish_requires_a_bundle` |
| Delete a published fleet | operator deletes while tenants can install it | 409 + registry code; must unpublish first → `test_delete_rejects_published_row` |
| Id collision | a repo's frontmatter name matches an existing row from another repo | 409 + registry code; the dialog offers explicit Replace → `test_create_rejects_id_collision_without_replace` |
| Draft installed by id | caller knows an unpublished id and posts an install | rejected — `SELECT_PLATFORM_INSTALL` filters on published → `test_draft_fleet_is_not_installable_by_id` |
| Invalid bundle | repo lacks a root `SKILL.md`, oversize, secret-shaped | importer `UZ-…` code; no row written; dialog stays open → `test_add_fleet_creates_draft_row` (negative arm) |
| Catalog read fails | database unavailable, malformed row | mapped presentation on the page; the table is NOT rendered empty → `test_catalog_read_failure_surfaces_error` |

## Invariants

1. **A published row always has a bundle** — `PATCH … {published:true}` rejects a `content_hash IS NULL` row, and every write stages to `draft`; enforced in the handler, asserted by `test_publish_requires_a_bundle`.
2. **A tenant can only ever reach a published fleet** — the gallery, `GET /v1/fleets/bundles`, and `SELECT_PLATFORM_INSTALL` all filter `visibility = 'public'`; enforced in SQL, in one file, asserted by §3's three tests.
3. **A catalog read can never carry bundle bodies or storage keys** — enforced at compile time: the response struct has no such field and the projection selects no such column.
4. **A refetch never destroys operator-curated copy** — `description` and `required_credentials_reasons` are absent from the `ON CONFLICT` update list; enforced by the statement itself, asserted by `test_refetch_drafts_and_preserves_curated_copy`.
5. **A catalog row is never born in SQL, and the `draft`/`public` wire values are spelled once per runtime** — no migration inserts into `core.fleet_library`; the Zig and TypeScript constants are byte-identical. Both enforced by grep assertions plus the UFS gate.
6. **The browser never holds the api-audience token** — every catalog call happens in a server component or `"use server"` action via `withToken`; enforced by the module directive.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `platform_library_published` | product | operator publishes or unpublishes a fleet | `entry_id` (the derived catalog slug), `action` (`published`/`unpublished`) | no repository free-text, no markdown, no tokens | `test_publish_emits_analytics_event` |

Publishing is the moment a fleet becomes available to every tenant — the one state change here with a decision riding on it, so it is the one event added. The existing `platform_library_onboarded` keeps firing on create/refetch, unchanged and un-renamed; nothing is emitted for edit or delete, because no decision hangs on those counts and a counter without a decision behind it is dashboard clutter (Product Clarity item 9). No funnel changes, so no analytics/funnel playbook update is required. The new routes log per `docs/LOGGING_STANDARD.md` and are covered by existing request metrics.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_fresh_database_catalog_empty` | after full migration, `core.fleet_library` holds 0 rows; a grep over `schema/` finds no `INSERT INTO core.fleet_library` |
| 1.2 | integration | `test_draft_normalize_is_idempotent_and_scoped` | pre-state {bundle-less public row, published row with a hash} → the first becomes `draft`, the second untouched; re-running changes nothing |
| 1.3 | integration | `test_create_derives_row_from_bundle` | create from a fixture repo → row with the frontmatter id/name/description, bundle's credentials/tools/hosts, `visibility='draft'` |
| 1.4 | integration | `test_refetch_drafts_and_preserves_curated_copy` | published row with an operator-edited description + reasons → refetch → new hash, `visibility='draft'`, description and reasons unchanged |
| 1.5 | integration | `test_create_rejects_id_collision_without_replace` | second repo declaring an existing name → 409 + registry code; with `replace:true` → 200 and `source_repo` is rewritten |
| 1.6 | unit | `test_no_fleet_bundle_references` | grep over `src/` finds no `fleet_bundle` import path; the build succeeds |
| 2.1 | integration | `test_admin_catalog_lists_all_rows_without_bodies` | one draft + one published + one bundle-less → all three returned; no markdown/storage-key field in the JSON; a malformed manifest degrades its count to 0 without failing the list |
| 2.2 | integration | `test_patch_updates_curated_copy_only` | PATCH description + reasons → those change; name/hash/tools/credentials unchanged |
| 2.3 | integration | `test_publish_requires_a_bundle` | publish a bundle-less row → 409 + code, still `draft`; publish a row with a hash → 200, `visibility='public'` |
| 2.4 | integration | `test_delete_rejects_published_row` | DELETE published → 409 + code, row survives; unpublish then DELETE → 204, row gone |
| 2.5 | integration | `test_catalog_routes_scope_and_method_enforced` | scopeless token on GET/POST/PATCH/DELETE → 403 `UZ-AUTH-022`; `PUT` → 405 |
| 2.6 | unit | `test_openapi_covers_catalog_routes` | route-coverage check finds every method documented (`make check-openapi` exit 0) |
| 3.1 | integration | `test_draft_fleet_is_not_installable_by_id` | install a draft by `platform_library_id` → rejected; publish → the same install succeeds |
| 3.2 | integration | `test_draft_absent_from_gallery_and_bundles` | draft absent from the workspace gallery and `GET /v1/fleets/bundles`; publish → present in both |
| 3.3 | integration | `test_unpublish_leaves_existing_installs_intact` | install, then unpublish → the installed fleet still resolves its bundle by its own `bundle_content_hash`; a new install is rejected |
| 4.1 | unit | `test_catalog_clients_call_admin_endpoints` | each client hits its path/method with the bearer header and returns the parsed body |
| 4.2 | unit | `test_catalog_status_derivation` | `public` ⇒ published; `draft`+hash ⇒ draft; `draft`+null ⇒ no_bundle; `draft`+`""` ⇒ no_bundle |
| 4.3 | unit | `test_catalog_table_renders_rows` | four mocked entries → four rows, keyed by catalog id, in server order |
| 4.3 | unit | `test_catalog_row_status_rendering` | each of the three states → its label; the No-bundle row shows no hash |
| 4.4 | unit | `test_catalog_empty_state_only_when_no_rows` | zero entries → empty state inviting Add; one entry → no empty state in the tree |
| 4.5 | unit | `test_catalog_read_failure_surfaces_error` | read throws `ApiError` → mapped presentation; no empty table |
| 5.1 | unit | `test_add_fleet_creates_draft_row` | typed repo → action called with it; revalidation requested; importer failure keeps the dialog open with the mapped `UZ-…` |
| 5.2 | unit | `test_fetch_update_prefills_and_drafts` | row Fetch → dialog prefilled with that row's repository; success revalidates; a stale requestId response is dropped |
| 5.3 | unit | `test_edit_dialog_saves_curated_copy` | edit description + one credential reason → PATCH body carries exactly those; failure keeps the dialog open with the mapped error |
| 5.4 | unit | `test_publish_unpublish_round_trip` | publish → PATCH `{published:true}` + revalidate; unpublish → `{published:false}` + revalidate |
| 5.5 | unit | `test_delete_offered_only_when_unpublished` | published row renders no Delete; draft row renders it behind a confirm naming the fleet |
| 5.6 | unit | `test_collision_offers_explicit_replace` | 409 → the dialog surfaces Replace; accepting retries with `replace:true` |
| — | unit | `test_publish_emits_analytics_event` | publish and unpublish each emit one capture with exactly the registry props |
| — | e2e | `test_e2e_platform_catalog_lifecycle` | operator: add a fleet → draft, invisible in a workspace gallery → publish → installable there → unpublish → gone from the gallery, the existing install still runs |

Regression: M127's scope-gating tests and the nav gate pass unchanged; the tenant onboarding flow and `core.tenant_fleet_library` are untouched. Idempotency: 1.2 (migration re-run) and 1.4 (refetch upsert) cover replay.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | No fleet reaches a tenant unpublished; the catalog is runtime data with a working lifecycle (§1, §2, §3) | `make test-integration` | exit 0 | P0 | ⬜ **ungraded locally** — no Postgres in this worktree, so the 11 new integration tests compile and SKIP. CI is the grading run |
| R2 | The operator runs the whole lifecycle from the table (§4, §5) | `cd ui/packages/app && bun run test:coverage` | exit 0 | P0 | ✅ `144 files · 1383 tests passed`, 100% lines/branches/functions/statements |
| R3 | Routes documented; route coverage passes (§2.6) | `make check-openapi` | exit 0 | P0 | |
| R4 | The full operator path works against a real stack (§5) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts platform-library-onboarding.spec.ts` | exit 0 | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Zig unit lane passes | `make test-unit-agentsfleetd` | exit 0 | P0 | ✅ `34/34 steps succeeded; 1625 passed` |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ zig + app lint green (pre-commit harness ran both on every commit) |
| S3 | Schema gate clean (no `ALTER`/`DROP` below v2.0.0) | `make check-schema-gate` | exit 0 | P0 | ✅ `VERSION=0.17.0, pre-v2.0 teardown convention` |
| S5 | No leaks on the new query paths | `make memleak` | exit 0 | P0 | ⬜ **ungraded locally** — needs Postgres. `_lint_zig_pg_drain` ✅ (660 files) covers the drain discipline; CI grades the leak run |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ both targets exit 0 |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` |
| S8 | No oversize source file added | `git diff --name-only origin/main \| grep -v -E '\.md$\|^docs/\|\.test\.\|_test\.\|/tests?/' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | 🟡 `Shell.tsx 459` — pre-existing on `main`, untouched by this diff (flagged in M127 too) |
| S9 | Orphan sweep — seed, renamed folder, renamed dialog all gone | Dead Code Sweep greps | 0 matches | P0 | ✅ 0 matches on every row |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted. Two renames (`fleet_bundle/` → `fleet_library/`, `OnboardPlatformLibraryDialog.tsx` → `AddFleetDialog.tsx`) are swept below.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| The catalog seed | `grep -rn "INSERT INTO core.fleet_library" schema/` | 0 matches |
| `fleet_bundle` (the renamed folder) | `grep -rn "fleet_bundle" src/ \| grep -v fleet_bundles` | 0 matches |
| `OnboardPlatformLibraryDialog` | `grep -rn "OnboardPlatformLibraryDialog" ui/packages/app` | 0 matches |
| `onOnboarded` + `EMPTY_TITLE` (the lifted-state prop and its copy) | `grep -rEn "onOnboarded\|Nothing onboarded yet" ui/packages/app` | 0 matches |

## Out of Scope

- **Renaming `src/agentsfleetd/http/handlers/fleet_bundles/`** — that folder names the public route `GET /v1/fleets/bundles`; renaming it implies an Application Programming Interface (API) rename, a separate call. Only the domain folder is renamed here.
- **A `platform-library:read` scope rung** (see §2's default — revisit when a read-only operator role is real), and **pagination / search / sort** (a curated first-party list; controls before the data needs them are clutter).
- **Bundle-authored install-gate copy (frontmatter-declared credential reasons)** — letting a repository declare its own credential reasons in frontmatter. Rejected: this copy is the platform's voice at the moment a tenant is asked for a token, and should not be controlled by whoever authored the repository.
- **Versioning / rollback of a published bundle** — a fleet has one current bundle; a bad publish is fixed by unpublishing or refetching. Staged draft-beside-published (two hashes on one row) is the shape if this becomes real, and it needs its own spec.
- **A `agentsfleet` Command-Line Interface (CLI) verb for catalog management, and the tenant library gaining the same surface** — no demonstrated demand; this spec is the platform tier, dashboard-only.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens Fleet libraries, clicks **Add fleet**, pastes `agentsfleet/zoho-sprint-daily-summarizer`, and the row appears as a **Draft** — fetched, validated, invisible to every tenant. He opens the pencil, writes the line explaining why the fleet wants a Zoho token, clicks **Publish**. Only then does it appear in a workspace's gallery. Nothing in that sentence involved a migration or a SQL console.
2. **Preserved user behaviour** — the scope gate and nav entry behave as M127 shipped them; the tenant library, workspace gallery, and install flow are untouched; a workspace already running a fleet keeps running it through an unpublish or a delete, because its install snapshotted the bundle.
3. **Optimal-way check** — the write already exists, the lifecycle column already exists, and the gallery already filters on it. The gap to unconstrained-optimal is no rollback to a previous bundle version — acceptable: a bad publish is one Unpublish away, and nobody has needed a version history yet.
4. **Rebuild-vs-iterate** — iterate, but on the *right* axis. The mud-patch would have been a read route bolted onto a seed-driven catalog, leaving the operator unable to create or curate anything without a migration. Deleting the seed is what makes the surface honest, and it costs less than the read route it replaces.
5. **What we build** — three routes (`GET`, `PATCH`, `DELETE`) plus create/refetch semantics on the existing `POST`; one data-only migration; a lifecycle on a column that already exists; a folder rename; a table; five row actions; one edit dialog; one analytics event.
6. **What we do NOT build** — a read-only scope rung (locks out today's operators for a role nobody has), pagination/search (no volume), bundle-authored gate copy (that voice is the platform's), bundle version history (unpublish is the fix), a Command-Line Interface (CLI) verb (no demand).
7. **Fit with existing features** — compounds M127's onboard write and the M103 two-tier catalog. It must not destabilize the install path: a tenant already running a fleet is untouched by anything an operator does to the catalog row it came from, and Dimension 3.3 pins exactly that.
8. **Surface order** — UI-first, with the minimum backend that makes the UI honest and the publish gate real. Every route added exists because an affordance on the page would otherwise be a lie.
9. **Dashboard restraint** — the table shows only what the routes return: identity, source, status, hash, updated. No install counts, no health, no "last used" — no counter stands behind any of them. No affordance is rendered for a state it cannot serve: a published row simply has no Delete, rather than a disabled one.
10. **Confused-user next step** — every row's state names its own next action (No bundle → Fetch bundle; Draft → Publish; Published → Unpublish). Rejections carry a registry code and `presentError`'s suggestion. A non-operator gets the settings notice naming the requirement. Nothing here routes to a ticket.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, one Pull Request (PR), split data-model → routes → tenant-facing gate → plumbing → table → actions, so the backend is provable before the UI depends on it and the publish gate is proven before any affordance claims it.
- **Alternatives considered:** (a) *read route + table only, seed retained* — this spec's own first draft. Rejected once the seed's cost was visible: the operator could see the catalog but still not create, curate, or withdraw anything without SQL, and onboarding would remain an accidental publish. (b) *keep the seed, add PATCH for the curated copy only* — rejected: it leaves the birth of a fleet in a migration, the dependency Indy named. (c) *add a `published_at` column* — rejected on the rules: `ALTER TABLE` is gate-blocked below v2.0.0, and the vestigial `visibility` column already carries this exact meaning.
- **Patch-vs-refactor verdict:** this is a **refactor** of the catalog's ownership model — rows move from migration-owned to runtime-owned — delivered through additive routes and one data-only migration. It is the smaller change *and* the more correct one: the read-route-only patch would have shipped a surface that still could not do the operator's job.

## Discovery (consult log)

- **Consults** —
  - *Publish gate + metadata edit (Indy, Jul 13, 2026).* Instantly live, or staged? Indy chose two-step **onboard → publish** — explicit, reversible, with Unpublish as the non-destructive withdraw path. And should the install-gate credential copy stay seed-owned? Indy chose **a `PATCH` route + edit dialog** — which is what made the seed redundant.
  - *The seed itself (Indy, Jul 13, 2026).* "I assume with the PATCH or edit we would have no requirement for seed? Can the onboard library be like a create library?" and "I want to think about less dependency on the static stuff in sqls" — the pivot this spec is built on. The seed rows are deleted; the catalog becomes runtime data.
  - *`schema/028` vs the contiguity tests (Indy, Jul 13, 2026).* Deleting `028` (its only job was seeding) left a version gap, and `cmd/common.zig` hard-asserts contiguous versions (`versionsContiguousFromFirst`, `last version == registered count`) — stricter than `docs/SCHEMA_CONVENTIONS.md`, which says "slot gaps are fine". Renumbering the normalize migration to `028` was rejected outright: production has already recorded version 28 as applied, so the migrator would skip it and prod would never normalize (the M127 trap again). Indy chose **keep `028`, strip its seed** — its `DELETE` of the never-published `security-reviewer` row remains true and is a no-op on a fresh database — over relaxing the tests, which would have weakened the guard that catches a forgotten `@embedFile` registration.
  - *Naming (Indy, Jul 13, 2026).* "is Re-onboard a good name?" — no. Operator verbs: **Add fleet**, **Edit**, **Publish**/**Unpublish**, **Fetch update**, **Delete**; "onboard" survives only as the internal write. "fleet_bundle (name the folder as fleet_library)" — the domain folder is renamed; `handlers/fleet_bundles/` is not, since it names the public route (Out of Scope).
- **Metrics review** — one new event, `platform_library_published` (`entry_id`, `action`, `outcome`), registered in `EVENTS` / `EventProps` / `EVENT_PROP_KEYS` together. It fires on publish AND unpublish, and on a refusal too — a publish nobody could complete is a signal, not an absence of one. `entry_id` is the catalog slug the importer derived, never the repository free-text the operator typed. The existing `platform_library_onboarded` still covers add/refetch, unchanged. Nothing is emitted for edit or delete: no decision hangs on those counts. No funnel change, so no analytics/funnel playbook update is required. **Self-caught during CHORE(close):** the event was reasoned away mid-implementation as "dashboard clutter" and the spec's Metrics table was briefly left aspirational — the spec is the rulebook, so the event was built rather than the table quietly weakened.
- **Skill-chain outcomes** —
  - `/write-unit-test`: 11 new Zig integration tests (catalog lifecycle, publish gate, delete guard, id collision, draft-not-installable, plus the three race/transaction paths the review surfaced) and 25 new app tests. Test delta: unit 2585 → 2596, integration 311 → 322. The app suite went from 1358 → 1383 tests at 100% coverage.
  - `/review` (high): **8 findings, all 8 fixed in-diff, none deferred.** Three were genuine correctness bugs. (1) Both guarded writes discarded their `RETURNING`, so a DELETE that raced a publish answered **204 while the fleet stayed live** — the guard was defeated by the exact race it exists for. (2) `decodeSummaries` swallowed `OutOfMemory` and degraded it to "no support files", in the tenant-facing gallery. (3) A curate-and-publish `PATCH` ran two unsynchronized `UPDATE`s and could half-apply. Also fixed: `respondEntry` rebuilt the entire catalog to return one row; the DELETE bypassed `hx.noContent()`; `replace` was undocumented in OpenAPI; and the edit dialog held a stale row snapshot across revalidation.
  - `kishore-babysit-prs`: pending — runs after the push.
- **Deferrals** — none. Every `/review` finding was fixed in-diff.

**Ungraded locally, by environment, not by choice.** The integration suite (R1, S5) needs Postgres and the acceptance suite (R5) needs Clerk keys + a running `agentsfleetd`; neither exists in this worktree. Those tests are written, compiled, and typechecked here, and **CI is their first real execution** — they are not being claimed as green on this agent's say-so.
