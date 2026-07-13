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

# M128_001: Platform catalog read route and the /admin/fleet-libraries table

**Prototype:** v2.0.0
**Milestone:** M128
**Workstream:** 001
**Date:** Jul 13, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing; without it the platform catalog is unreadable from the dashboard, so an operator cannot tell which first-party fleets are live and which are still empty shells
**Categories:** API, UI
**Batch:** B1 — single workstream, no parallel siblings
**Branch:** feat/m128-platform-catalog-table — added at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** M127_001 (done) — it built the onboard surface and named this list route as its explicit follow-up
**Provenance:** LLM-drafted (claude-opus-4-8, Jul 13, 2026)
**Canonical architecture:** `docs/architecture/fleet_bundles.md` §Two-tier catalog

---

## Overview

**Goal (testable):** An operator holding `platform-library:write` opens `/admin/fleet-libraries` and sees every row of the platform catalog — including the seeded rows no one has onboarded — each labelled Pending or Materialized from its `content_hash`; onboarding a Pending row from its own row action refreshes the table and flips that row to Materialized without a page reload.

**Problem:** The Fleet libraries page is a submit box with no memory. An operator lands on "Nothing onboarded yet in this session" whether the catalog holds four live fleets or zero, because the page state is React state, not catalog state. The four first-party rows — `github-pr-reviewer`, `platform-ops`, `zoho-sprint-daily-summarizer`, `zoho-recruiter-daily-summarizer` — exist in `core.fleet_library` with a `NULL` `content_hash`, which makes them invisible in every workspace gallery, and the operator whose job is to fix that has no way to see it. Answering "what is live?" today means opening a Structured Query Language (SQL) console against production; answering "did my onboard work?" means walking to a workspace gallery in another tab. Re-onboarding is the only way to probe a row's state, and it is a network fetch of a whole repository.

**Solution summary:** Give the platform catalog the read route it never had — `GET /v1/admin/fleet-libraries`, a second method on the existing route, gated on the same `platform-library:write` scope and mirroring `admin_models` (which already serves GET+POST off one route with per-method scopes). It returns every catalog row, materialized or not, with metadata only: never the stored markdown bodies, never an object-store key. The dashboard page then reads that route server-side and renders a table with a Status column derived solely from `content_hash` presence, and each Pending row carries an Onboard action that opens the existing dialog prefilled with that row's repository. Success revalidates the page, so the table — not a session-scoped card — is what confirms the work. No schema change, no new scope, no change to the onboard route.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api,ui): platform catalog read route and admin table
- **Intent (one sentence):** Let a platform operator see the whole fleet catalog and its live-or-empty status on one screen, and onboard a missing bundle straight from the row that reports it missing.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/admin_models.zig` + `src/agentsfleetd/http/route_table_invoke.zig` (`invokeAdminModels`) — the exact pattern this route mirrors: one route, per-method dispatch in the invoke fn, `respondMethodNotAllowed` on the else arm, per-method scopes resolved in `route_scopes.zig`.
2. `src/agentsfleetd/http/handlers/library/gallery.zig` + `src/agentsfleetd/fleet_bundle/sql.zig` — the read that already exists over this table. The admin list is the same projection minus the `content_hash IS NOT NULL` filter, and it inherits the gallery's discipline of never returning object-store keys or markdown bodies.
3. `ui/packages/app/app/(dashboard)/admin/models/page.tsx` and `components/CatalogueList.tsx` — the scope-guarded admin page that *reads* and renders a table with row actions. The Fleet libraries page becomes this shape.
4. `public/openapi/paths/fleet-library.yaml` — where the new method is documented. `make check-openapi` runs a route-coverage check; an undocumented route fails the build.
5. `docs/REST_API_DESIGN_GUIDELINES.md` §Collections and §Errors — the response envelope and error-code conventions the new route conforms to.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/fleet_bundle/sql.zig` | EDIT | Add the admin-catalog projection — every row, no `content_hash` filter |
| `src/agentsfleetd/http/handlers/library/catalog.zig` | CREATE | The `GET /v1/admin/fleet-libraries` handler |
| `src/agentsfleetd/http/handlers/library/api.zig` | EDIT | Export the new inner handler alongside the onboard/gallery ones |
| `src/agentsfleetd/http/route_table_invoke_library.zig` | EDIT | `invokePlatformLibraryOnboard` becomes a method dispatcher (GET → catalog, POST → onboard, else → 405) |
| `src/agentsfleetd/http/route_scopes.zig` | EDIT | `.admin_fleet_library` resolves per method; both arms require `platform-library:write` |
| `src/agentsfleetd/http/router.zig` | EDIT | Route-match test gains the GET assertion; the path matcher itself is already method-agnostic |
| `src/agentsfleetd/http/handlers/library/catalog_integration_test.zig` | CREATE | Real-stack tests: unmaterialized rows present, bodies absent, scope enforced |
| `public/openapi/paths/fleet-library.yaml` | EDIT | Document the GET method + response schema |
| `public/openapi/components/**` | EDIT | The catalog-entry schema the GET response references |
| `ui/packages/app/lib/api/fleet-library.ts` | EDIT | Platform catalog read client |
| `ui/packages/app/lib/types.ts` | EDIT | `PlatformCatalogEntry` / `PlatformCatalogResponse` |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/page.tsx` | EDIT | Server-side catalog read; passes entries to the view |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView.tsx` | EDIT | Renders the table; the session-scoped result card and its empty copy are removed |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/PlatformCatalogTable.tsx` | CREATE | The table: identity, source, status, row action |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/OnboardPlatformLibraryDialog.tsx` | EDIT | Accepts a prefilled repository + controlled open; success revalidates instead of lifting state |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/actions.ts` | EDIT | Onboard action revalidates the admin path on success |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/library-copy.ts` | EDIT | Status labels, column headers, row-action copy (UFS) |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/loading.tsx` | EDIT | Skeleton matches the table, not the empty card |
| `ui/packages/app/tests/admin-fleet-libraries-page.test.ts` | EDIT | Page now reads: cover the read path, its failures, and the scope guard |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/PlatformCatalogTable.test.tsx` | CREATE | Status derivation, empty state, row-action wiring |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/OnboardPlatformLibraryDialog.test.tsx` | EDIT | Prefill + controlled-open behaviour |
| `ui/packages/app/tests/e2e/acceptance/platform-library-onboarding.spec.ts` | EDIT | The user path now ends in the table, not a card |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (status labels, column headers, route path, scope string → named constants), NDC (no delete/edit plumbing built ahead of the routes that would back it), NLR (the files this diff touches leave cleaner — the session-card state goes, it is not left beside the table), ORP (the removed copy constants and lifted-state prop are swept), TST-NAM (test identifiers carry no milestone id), FLL (file-length cap on the view once the table lands — split rather than grow).
- `dispatch/write_zig.md` — the backend half: `conn.query()` → `.drain()` in the same function, tagged-union results, `errdefer` on the allocation path, cross-compile both linux targets.
- `docs/REST_API_DESIGN_GUIDELINES.md` — the new GET conforms to the collection-response and error-envelope conventions; §7 route-registration facts stay true (`make check-route-registration-doc`).
- `dispatch/write_ts_adhere_bun.md` — the User Interface (UI) half: FILE SHAPE DECISION at PLAN, design-system primitives only, no arbitrary Tailwind values.
- Analytics single-source discipline (`ui/packages/app/lib/analytics/events.ts` header) — this diff adds no event; the existing `platform_library_onboarded` fires unchanged from the row action.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — new handler + sql + invoke dispatch | pg-drain in the same fn; cross-compile `x86_64-linux` and `aarch64-linux`; `make memleak` over the new query path |
| PUB / Struct-Shape | yes — new `pub` inner handler + response structs | FILE SHAPE DECISION per new pub surface at PLAN; response struct mirrors `gallery.zig`'s shape discipline |
| File & Function Length (≤350/≤50/≤70) | yes | `catalog.zig` stays a single-purpose file; `FleetLibrariesView.tsx` splits the table into its own component rather than absorbing it |
| UFS (repeated/semantic literals) | yes | Status labels, headers, the admin path, and the scope string each live once; the wire strings for status are derived, never re-spelled |
| UI Substitution / DESIGN TOKEN | yes | `@agentsfleet/design-system` table/badge/button primitives; theme tokens only |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING yes; ERROR REGISTRY reuse; SCHEMA no | Handler logs per `docs/LOGGING_STANDARD.md`; reuses existing `UZ-AUTH-022` and store error codes — mints no new code; **no schema change at all** |
| MILESTONE-ID | yes | Spec id in commit messages; source comments cite the spec only where the reason is non-obvious |

## Prior-Art / Reference Implementations

- **Reference:** `admin_models` end to end — `/v1/admin/models` (GET+POST on one route, per-method scopes in `route_scopes.zig`, method dispatch in `invokeAdminModels`) paired with the `admin/models` page and its `CatalogueList` table. This spec is that shape applied to the fleet catalog. The one justified divergence: `admin_models` splits read and write scopes (`model:read` / `model:admin`), and this route does not — see §1's implementation default.

## Sections (implementation slices)

### §1 — The catalog read route

The platform catalog is the only admin-plane resource with no read route, which is why its dashboard page cannot render anything it did not just create. Add `GET` to the existing `admin_fleet_library` route: every row of `core.fleet_library`, materialized or not, ordered by id, metadata only.

**Implementation default:** GET requires `platform-library:write`, the same scope as POST — no new `platform-library:read` rung. M127_001 established this scope as independent with no hierarchy, and every scope is provisioned by hand in Clerk (`docs/AUTH.md` §Manually-provisioned); adding a read rung would silently lock every existing operator out of the page until a human re-provisioned their metadata, to serve a read-only auditor role nobody has asked for. The route stays per-method in `route_scopes.zig` anyway, so splitting it later is a one-line change.

**Implementation default:** no pagination, no filter, no search — the catalog is a curated first-party list, ordered by id. Pagination before there is a catalog large enough to need it is a control without evidence.

- **Dimension 1.1** — GET returns every catalog row, including rows whose `content_hash` is `NULL` → Test `test_admin_catalog_lists_unmaterialized_rows`
- **Dimension 1.2** — each entry carries its identity, source repository, visibility tier, `content_hash` (nullable), requirements, support-file count, and `updated_at` → Test `test_admin_catalog_entry_shape`
- **Dimension 1.3** — the response never carries `skill_markdown`, `trigger_markdown`, or any object-store key → Test `test_admin_catalog_omits_bodies_and_storage_keys`
- **Dimension 1.4** — a token without `platform-library:write` gets 403 `UZ-AUTH-022`; an unsupported method on the path gets 405 → Test `test_admin_catalog_scope_and_method_enforced`
- **Dimension 1.5** — the GET method is documented in the split OpenAPI source and the route-coverage check passes → Test `test_openapi_covers_admin_catalog_get`

### §2 — Client and types

The dashboard needs one function and one entry type; nothing else in the app should ever spell the admin path or re-derive what "materialized" means.

**Implementation default:** `materialized` is not a wire field — the server returns `content_hash` (nullable) and the UI derives status from its presence in exactly one place. Two sources for one fact is how a table starts lying.

- **Dimension 2.1** — the client GETs the admin path with the bearer token and returns the parsed entries → Test `test_platform_catalog_client_gets_admin_endpoint`
- **Dimension 2.2** — status derivation is a single exported function: a null/empty `content_hash` is Pending, anything else is Materialized → Test `test_catalog_status_derives_from_content_hash`

### §3 — The table

The surface an operator actually reads. One row per catalog entry, sorted as the server sorted it, with the Status column as the answer to the question the page exists to answer.

**Implementation default:** the content hash renders truncated with the full value available on the row — an operator compares hashes to confirm a re-onboard changed something, so it must be readable, but it must not dominate the row.

- **Dimension 3.1** — the page reads the catalog server-side and renders one row per entry, identified by catalog id → Test `test_admin_fleet_libraries_renders_catalog_rows`
- **Dimension 3.2** — a row with no `content_hash` renders the Pending status; a row with one renders Materialized and its truncated hash → Test `test_catalog_row_status_rendering`
- **Dimension 3.3** — the empty state renders only when the catalog is genuinely empty, and says so — no session-scoped copy survives → Test `test_catalog_empty_state_only_when_no_rows`
- **Dimension 3.4** — a read failure renders the mapped `presentError` presentation instead of an empty table pretending the catalog is empty → Test `test_catalog_read_failure_surfaces_error`

### §4 — Onboard from the row

The operator's actual job: see a Pending row, fill it. Today that means retyping a repository the table is already showing. The row action closes that loop, and success is confirmed by the table itself rather than a card that vanishes on reload.

**Implementation default:** the dialog is unchanged in behaviour — same validation, same double-submit guard, same error mapping — and gains only a prefilled repository and controlled open. A second onboarding form would be a second place for the validation to drift.

- **Dimension 4.1** — a Pending row's action opens the onboard dialog prefilled with that row's source repository → Test `test_pending_row_action_prefills_repository`
- **Dimension 4.2** — a Materialized row's action re-onboards from source (same route, upsert semantics), and is labelled as such → Test `test_materialized_row_action_reonboards`
- **Dimension 4.3** — a successful onboard revalidates the page: the table re-reads and the row flips to Materialized without a manual reload → Test `test_onboard_success_revalidates_catalog`
- **Dimension 4.4** — the still-available manual onboard (a repository not seeded in the catalog) adds a new row to the table on success → Test `test_manual_onboard_adds_row`

## Interfaces

```
GET /v1/admin/fleet-libraries          (new; scope platform-library:write)
  Authorization: Bearer <JWT carrying platform-library:write>
  → 200 { "entries": [ {
        "id", "name", "description",
        "source_repo", "source_ref",
        "visibility",                     // "public"
        "content_hash": string | null,    // null ⇒ seeded, never onboarded
        "requirements": { "credentials": [], "tools": [], "network_hosts": [],
                          "trigger_present": bool },
        "support_file_count": number,
        "updated_at": number              // epoch ms
      } ] }
  → 403 UZ-AUTH-022 (scope missing) · 405 on unsupported methods
  NEVER returns: skill_markdown, trigger_markdown, support_files_json bodies,
  or any object-store key.

POST /v1/admin/fleet-libraries         (unchanged — M127_001 owns it)

listPlatformFleetLibrary(token)                    (lib/api/fleet-library.ts)
catalogStatus(entry): "pending" | "materialized"   (single derivation site)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Scope missing (page) | non-operator hits the URL | server redirect to the settings notice, unchanged from M127 → `test_admin_fleet_libraries_redirects_without_scope` |
| Scope missing (route) | token without the scope calls GET | 403 `UZ-AUTH-022`, no rows → `test_admin_catalog_scope_and_method_enforced` |
| Wrong method | PUT/DELETE on the path | 405, no handler invoked → `test_admin_catalog_scope_and_method_enforced` |
| Session expired | 401 from backend on the read | page redirects to sign-in → `test_admin_fleet_libraries_redirects_without_scope` |
| Catalog read fails | database unavailable, malformed row | mapped `UZ-…` presentation on the page; the table is NOT rendered empty → `test_catalog_read_failure_surfaces_error` |
| Empty catalog | fresh database, seed not applied | the empty state, stating the catalog holds no entries → `test_catalog_empty_state_only_when_no_rows` |
| Onboard fails from a row | bad bundle, network, revoked scope | dialog stays open with the mapped error; the row keeps its prior status → `test_catalog_row_action_failure_keeps_status` |
| Stale table after onboard | success without revalidation | the action revalidates the admin path; the table re-reads → `test_onboard_success_revalidates_catalog` |
| Malformed manifest | `support_files_json` unparseable | the count degrades to zero rather than failing the whole list → `test_admin_catalog_entry_shape` |

## Invariants

1. The catalog read response can never carry bundle bodies or storage keys — enforced at compile time: the response struct has no such field, and the projection selects no such column (asserted by `test_admin_catalog_omits_bodies_and_storage_keys`).
2. Materialized-vs-Pending is derived from `content_hash` presence in exactly one function — enforced by there being no second derivation to drift from, asserted by a grep in the table test.
3. The scope string appears once in the UI codebase (`lib/auth/scopes.ts`) and once in the backend (`route_scopes.zig`) — enforced by the UFS gate plus the existing M127 grep assertion.
4. The browser never holds the api-audience token — the catalog read happens in a server component via `withToken`; enforced by the module directive and the absence of a client-bundle import of `lib/api/client`.
5. The User Interface (UI) scope guard stays defence-in-depth; the backend `requireScope` remains the authoritative gate — enforced by the backend, asserted end-to-end.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no new product/operator signal | — | — | — | — | — |

This spec adds no analytics event and renames none. The existing `platform_library_onboarded` fires unchanged when the row action submits, carrying the same `source_kind` / `outcome` / `entry_id` props — a row-initiated onboard is the same product action as a typed one, and a `initiated_from` prop would be a counter nobody has a decision riding on (dashboard restraint, Product Clarity item 9). No funnel changes, so no analytics/funnel playbook update is required. The new route logs per `docs/LOGGING_STANDARD.md` and is covered by existing request metrics.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_admin_catalog_lists_unmaterialized_rows` | seeded row with `content_hash IS NULL` → present in the GET response; the gallery route still hides it |
| 1.2 | integration | `test_admin_catalog_entry_shape` | one materialized + one seeded row → both carry id/source/visibility/requirements/support_file_count/updated_at; a malformed manifest degrades its count to 0, list still 200 |
| 1.3 | integration | `test_admin_catalog_omits_bodies_and_storage_keys` | a materialized row with stored markdown → response JSON has no `skill_markdown`/`trigger_markdown`/storage-key field |
| 1.4 | integration | `test_admin_catalog_scope_and_method_enforced` | token without the scope → 403 `UZ-AUTH-022`; `PUT` on the path → 405; scoped GET → 200 |
| 1.5 | unit | `test_openapi_covers_admin_catalog_get` | the route-coverage check finds GET documented for the path (`make check-openapi` exit 0) |
| 2.1 | unit | `test_platform_catalog_client_gets_admin_endpoint` | `(token)` → GET `/v1/admin/fleet-libraries` with bearer header; parsed entries returned |
| 2.2 | unit | `test_catalog_status_derives_from_content_hash` | `null` → pending; `""` → pending; `"abc…"` → materialized |
| 3.1 | unit | `test_admin_fleet_libraries_renders_catalog_rows` | four mocked entries → four rows, keyed by catalog id, in server order |
| 3.2 | unit | `test_catalog_row_status_rendering` | pending row → Pending label, no hash; materialized row → Materialized label + truncated hash |
| 3.3 | unit | `test_catalog_empty_state_only_when_no_rows` | zero entries → empty state; one entry → no empty state anywhere in the tree |
| 3.4 | unit | `test_catalog_read_failure_surfaces_error` | read throws `ApiError` → mapped presentation rendered; no empty table |
| 4.1 | unit | `test_pending_row_action_prefills_repository` | click a Pending row's action → dialog open with that row's repository in the field |
| 4.2 | unit | `test_materialized_row_action_reonboards` | click a Materialized row's action → dialog labelled re-onboard, same repository, same route on submit |
| 4.3 | unit | `test_onboard_success_revalidates_catalog` | successful action → the admin path is revalidated exactly once; no lifted result card exists |
| 4.4 | unit | `test_manual_onboard_adds_row` | manual onboard of an unseeded repository → after revalidation the new entry is a row |
| 4.x | unit | `test_catalog_row_action_failure_keeps_status` | onboard fails from a Pending row → dialog open with the mapped error; the row still reads Pending |
| e2e | e2e | `test_e2e_platform_catalog_table_and_row_onboard` | operator fixture: table lists the seeded rows as Pending; onboarding one from its row flips it to Materialized; it then appears in a workspace gallery |

Regression: the M127 scope-gating tests (`test_admin_fleet_libraries_redirects_without_scope`, the nav gate) pass unchanged; the tenant onboarding flow, workspace gallery, and `GET /v1/fleets/bundles` are untouched — the gallery's `content_hash IS NOT NULL` filter stays exactly as it is, and an integration assertion pins that the admin list and the gallery disagree on an unmaterialized row *by design*. Idempotency: Dimension 4.2 covers re-onboard/upsert.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The catalog read route returns unmaterialized rows and leaks no bodies (§1) | `make test-integration` | exit 0 | P0 | |
| R2 | The operator sees status per row and onboards from it (§3, §4) | `cd ui/packages/app && bun run test:coverage` | exit 0 | P0 | |
| R3 | Non-operators still never see the surface (regression on M127) | `cd ui/packages/app && bunx vitest run tests/admin-fleet-libraries-page.test.ts` | exit 0 | P0 | |
| R4 | The route is documented and route-coverage passes (§1.5) | `make check-openapi` | exit 0 | P0 | |
| R5 | The full operator path works end to end (§4) | `cd ui/packages/app && bunx playwright test --config=playwright.acceptance.config.ts platform-library-onboarding.spec.ts` | exit 0 | P0 | |
| R6 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Zig unit lane passes | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S5 | No leaks on the new query path | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file added | `git diff --name-only origin/main \| grep -v -E '\.md$\|^docs/\|\.test\.\|_test\.\|/tests?/' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep (removed session-card copy) | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `EMPTY_TITLE` ("Nothing onboarded yet in this session") | `grep -rn "Nothing onboarded yet" ui/packages/app` | 0 matches |
| `onOnboarded` (the lifted-state prop on the dialog) | `grep -rn "onOnboarded" ui/packages/app` | 0 matches |
| `OnboardedPlatformLibraryEntry` (if the result card was its only consumer) | `grep -rn "OnboardedPlatformLibraryEntry" ui/packages/app` | only the action's return type, or 0 matches |

## Out of Scope

- **Editing a catalog row's metadata** (the "pencil"): the curated description and per-credential reasons are seed-owned, and there is no `PATCH` route. A metadata-edit surface needs its own route, its own audit story, and a rule about what an operator may override on a materialized row — that is a spec, not a column.
- **Deleting or deprecating a row:** no `DELETE` route exists, and the destructive semantics against a materialized row (workspaces have installed it) are undesigned.
- **A `platform-library:read` scope rung:** deferred by design — see §1's implementation default. Revisit if a read-only operator role is ever real.
- **Pagination, search, or sort controls:** the catalog is a curated first-party list; controls before the data needs them are dashboard clutter.
- **A `agentsfleet` Command-Line Interface (CLI) verb for listing the catalog:** no demonstrated operator demand; the dashboard is the surface.
- **The tenant (workspace) library gaining the same table:** the workspace gallery already reads its own entries; this spec is the platform tier only.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy opens Fleet libraries and sees four rows, three of them reading **Pending**. He clicks Onboard on the `zoho-sprint-daily-summarizer` row without retyping anything, and watches that row flip to **Materialized** with a content hash. He now knows, without leaving the page, exactly what the platform catalog offers every tenant.
2. **Preserved user behaviour** — the scope gate and nav entry behave exactly as M127 shipped them; the onboard dialog validates, guards, and reports errors identically; the tenant onboarding flow, workspace gallery, and install flow are untouched.
3. **Optimal-way check** — the most direct shape: one method added to a route that already exists, and the page reads it. The gap to unconstrained-optimal is that the operator still cannot *fix* a bad row from here (no edit, no delete) — acceptable, because those need routes that do not exist and a design for destructive semantics against installed fleets.
4. **Rebuild-vs-iterate** — iterate. The backend read is a projection of a query already written; the page is the `admin/models` shape applied to a second resource. A rebuild would be inventing a catalog-management plane before anyone has managed a catalog.
5. **What we build** — one GET route (+ its OpenAPI entry), one Structured Query Language (SQL) projection, one client function, one status derivation, one table component, and a row action that prefills the dialog we already have.
6. **What we do NOT build** — row edit (no route, undesigned override semantics), row delete (no route, destructive against installed fleets), a read-only scope rung (locks out today's operators to serve a role nobody has), pagination/search (no data volume to justify it), a Command-Line Interface (CLI) verb (no demand).
7. **Fit with existing features** — compounds M127's onboard surface and the M103 two-tier catalog; the one thing it must not destabilize is the workspace gallery's `content_hash IS NOT NULL` filter, which is precisely the fact this table exists to expose. Admin list and gallery disagreeing on an unmaterialized row is the feature, and a test pins it.
8. **Surface order** — UI-first, with the minimum backend that makes the UI honest. The API half exists only because the page cannot tell the truth without it.
9. **Dashboard restraint** — the table shows only what the route actually returns. No install counts, no health, no "last used", no usage claims — there is no counter behind any of them. No edit or delete affordance is rendered, because no route would back it and a disabled button is a promise.
10. **Confused-user next step** — a Pending row is self-explanatory and carries the action that resolves it. A failed onboard keeps the `UZ-…` code and `presentError`'s suggestion. A non-operator gets the settings notice naming the requirement. Nothing here routes to a ticket.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, one Pull Request (PR), split backend → plumbing → table → row action, so each Dimension lands with its test and the backend is provable before the UI depends on it.
- **Alternatives considered:** (a) *UI-only, deriving status from the workspace gallery* — the gallery only returns materialized rows, so a Pending row is invisible to it by construction; the page could show what is live but never what is missing, which is the actual question. Rejected. (b) *A full catalog-management plane in one go* (GET + PATCH + DELETE + audit) — rejected: destructive and override semantics against fleets tenants have already installed need their own design, and bundling them buries this read behind that argument.
- **Patch-vs-refactor verdict:** this is a **patch** — it adds a method to an existing route and a second instance of an established admin-table pattern, reshaping nothing. The named larger step, if operators ever need to curate rather than just populate, is the catalog-management spec sketched in alternative (b).

## Discovery (consult log)

- **Consults** —
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
