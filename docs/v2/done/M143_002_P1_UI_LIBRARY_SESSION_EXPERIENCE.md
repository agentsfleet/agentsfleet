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
# M143_002: Library navigation stays useful through loading and expiry

**Prototype:** v2.0.0
**Milestone:** M143
**Workstream:** 002
**Date:** Jul 24, 2026
**Status:** DONE
**Priority:** P1 — current library pages block, morph controls, and collapse failures into empty states
**Categories:** UI
**Batch:** B2 — consumes M143_001 interfaces
**Branch:** feat/m143-library-session-experience
**Test Baseline:** unit=3172 integration=446
**Depends on:** M143_001 — paged tenant/global models and tier-qualified Fleet summary/detail
**Provenance:** LLM-drafted (Codex, Jul 24, 2026) from Oracle second-pass review
**Canonical architecture:** `docs/architecture/web_app.md` (statements 3 and 5, plus its scoreboard) and `docs/AUTH.md` Flow 2. Amended at VERIFY — see Discovery A9.
**Scope amended at EXECUTE (Jul 28, 2026):** §4 retires the uncalled `q` search parameter and §5 removes `support_files` from API responses while retaining the stored manifest — both on Indy's in-session decision. This takes the workstream beyond User Interface (UI)-only into Zig handlers, SQL modules, and published OpenAPI; the ZIG, PUB, and ERROR REGISTRY gate rows flip accordingly. See Discovery A10 and A11.

---

## Overview

**Goal (testable):** Model and Fleet pages retain useful accessible content while loading only the page the user asked for, and never render a failed read as an empty one.
**Problem:** Ordinary visits preload catalogues and secrets they never use, controls morph, and a failed read collapses into an empty state that offers the user no way back.
**Solution summary:** Use page/load-more state with current-page-only projection, stable Suspense regions, and typed errors that keep a failed read distinguishable from an empty one. The session keeper is retained unchanged — see §3 for why no canary was built.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(ui): make library navigation stable and resilient
- **Intent (one sentence):** Users browse, load more, deep-link, and resume without spinner walls, secret preloads, or ambiguous failures.
- **Handshake** — at PLAN, restate Intent and assumptions; mismatch means STOP.

## Implementing agent — read these first

1. `M143_001_P1_API_CLI_LIBRARY_DATA_SECURITY.md` §Interfaces and limits.
2. `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/**` — registry/catalogue flow.
3. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/**` — gallery/detail flow.
4. `docs/AUTH.md` and `docs/DESIGN_SYSTEM.md` — Clerk and accessible loading rules.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `ui/packages/app/lib/api/model_library.ts`; `lib/api/fleet-library.ts`; `lib/api/library-types.ts` | EDIT/CREATE | Exact page/error types; replace the gallery's exhaustive walk with retained paging. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/page.tsx`; `.../models/actions.ts`; `.../models/loading.tsx`; `.../models/lib/reads.ts`; `.../models/components/ModelCatalogueProvider.tsx`; `.../models/components/ProviderModelSelect.tsx`; `.../models/components/ModelsRegistryTable.tsx`; `.../models/components/ModelsRegistryCells.tsx`; `.../models/components/AddModelEntryDialog.tsx`; `.../models/components/EditModelEntryDialog.tsx` | EDIT | Registry page/load-more, current-page projection, intent loading. `ModelsRegistryCells.tsx` amended in at EXECUTE (Discovery A4) — it owns the Edit trigger that Dimension 1.2's focus/hover intent must reach. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/page.tsx`; `.../fleets/new/actions.ts`; `.../fleets/new/LibraryCard.tsx`; `.../fleets/new/InstallSourceSelector.tsx`; `.../fleets/new/AddLibraryDialog.tsx`; `.../fleets/new/loading.tsx`; `.../fleets/new/InstallFleet.tsx`; `.../fleets/new/InstallStates.tsx`; `.../fleets/new/InstallEntry.tsx`; `.../fleets/new/library-docs.tsx` | EDIT/CREATE | Retained gallery paging, server-resolved selection from the held summary, and typed selection states. `InstallFleet.tsx`/`InstallStates.tsx` amended in at PLAN (see Discovery) — they own deep-link initialization and the selection/ConnectGate states §2 now renders from the summary. |
| `ui/packages/app/lib/auth/client.test.tsx` | EDIT | Keeper retained; existing unit coverage stands (§3). |
| `tests/e2e/acceptance/settings-models.spec.ts`; `platform-library-onboarding.spec.ts` | EDIT/CREATE | Authenticated UI/session proof. |
| `docs/architecture/web_app.md` | EDIT | Scoreboard re-measure and the statement-3/5 record for these routes (Discovery A9). |
| `src/agentsfleetd/http/handlers/library/query.zig`; `.../library/gallery.zig`; `.../library/gallery_page.zig`; `.../library/catalogue_key.zig`; `.../model_library.zig`; `src/agentsfleetd/fleet_library/gallery_sql.zig`; `src/agentsfleetd/state/model_library/sql.zig`; `src/agentsfleetd/state/model_library_store.zig`; `.../library/library_query_normalization_test.zig`; `.../model_library_page_integration_test.zig`; `public/openapi/paths/fleet-library.yaml`; `public/openapi/paths/models.yaml`; `public/openapi.json` | EDIT | §4 — retire the uncalled `q` search parameter end-to-end (Discovery A10). Search normalization, the folded `LIKE` pattern, the cursor key's `q` field, both published parameter documents, and the `q` half of `UZ-LIBRARY-003`. |
| `src/agentsfleetd/fleet_library/sql.zig`; `.../library/entry_view.zig`; `.../library/catalog.zig`; `.../library/onboard.zig`; `public/openapi/components/schemas.yaml`; `ui/packages/app/lib/types.ts` | EDIT | §5 — stop projecting `support_files` onto API responses while continuing to persist the manifest (Discovery A11). Read-back SELECTs, the admin-catalog and onboard response projections, the published schemas, and the two client types. |

**Scope grading.** Rubric R4 compares `git diff --name-only origin/main` against this table, so every cell is an exact path. Test files are covered by the row of the code they exercise, whether they sit beside it as `<Name>.test.tsx` or in this package's shared `ui/packages/app/tests/` directory — which is where most of this application's tests actually live, a fact the original wording did not anticipate. A path that turns out to be genuinely required and is missing here is a spec amendment recorded in Discovery, not a silent addition.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — GRD, ASE, DID, PTK, FLL, UFS, TNM, NDC, NLR, NLG, ORP.
- **`dispatch/write_ts_adhere_bun.md`, `docs/DESIGN_SYSTEM.md`, `docs/AUTH.md`** — shape, async, primitives, motion, Clerk ownership.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| ZIG / PUB | **yes** (was no) | §4 and §5 edit handlers, stores, and SQL modules. Cross-compile both linux targets; every `pub` surface removed is proven to have no remaining caller before deletion. Flipped at EXECUTE — see Discovery A10/A11. |
| File & Function Length | yes | focused types/state components |
| UFS | yes | constants for states, thresholds, routes |
| UI Substitution / DESIGN TOKEN | yes | primitives/tokens; stable reduced motion |
| ERROR REGISTRY | **yes** (was no) | §4 narrows `UZ-LIBRARY-003` to its `limit` half. The code keeps its identifier and its registry row; only the search-bound cause retires. |
| SCHEMA | no | §5 retains the `support_files_json` column and every write to it. No migration, no `DROP COLUMN`, no edit to a frozen slot file — the change is confined to what is read back and projected (Discovery A11). |
| LOGGING / LIFECYCLE | no | no logging or lifecycle surface changes |

## Prior-Art / Reference Implementations

- **Streaming:** Approvals/Events loaders; **auth:** current `AuthSessionKeeper` and `docs/AUTH.md`; **API:** M143_001.

## Sections (implementation slices)

### §1 — Models use retained page/load-more state — **DONE**

Ordinary Models requests only the first tenant registry page: no global catalogue or secret list. Load-more appends and retains prior rows, while only the current fetched page is projected/decrypted; no action decrypts beyond it. Add/Edit open, focus, and eligible hover prefetch global model pages. Disable hover prefetch for coarse pointers or Save-Data; focus/open still prefetch.

- **Dimension 1.1** — ordinary/load-more requests and projection are page-bounded → Test `test_models_registry_retains_pages_without_extra_decrypts` — **DONE**
- **Dimension 1.2** — intent prefetch honors pointer/data policy and request ordering → Test `test_model_picker_prefetch_policy_and_latest_result` — **DONE**

### §2 — Fleet summary paging and failures are progressive — **DONE**

Initial Fleet creation requests one gallery page, then load-more appends and retains prior cards. Selection renders from the summary already held — it issues no second request, because the retained summary carries every field the install screen reads. Server-resolved links include visibility/id and avoid gallery flash. Unauthenticated is 401 and a denied workspace is 403; a `library_id` absent from the gallery resolves to a not-found selection state that neither enumerates nor errors the page. Stable skeletons, stale refresh content, retry, empty, 401, 403, and not-found remain distinct and reduced-motion safe.

**Amended at PLAN — reconciled to the mechanism M143_001 shipped.** This section was drafted against a server-only `readFleetLibraryDetailAction(workspaceId,tier,id)` returning a separate `FleetDetail`. That route no longer exists: M143_001 satisfied the parent spec's compactness goal by *trimming the summary* rather than by *adding a detail route*, and retired `handlers/library/gallery_detail.zig`, the `workspace_fleet_library_detail` variant, `UZ-LIBRARY-007`, and the OpenAPI operation with it. `router.zig` now asserts the former URL is unrouted and `gallery_keyset_integration_test.zig` pins it to 404 even for a resident entry.

Nothing user-visible is lost, and this was verified rather than assumed. The summary **retains** `requirements` and `required_credentials_reasons` — the two fields that drive the card's credential chips and the ConnectGate copy. `support_files` was the only field detail added over summary, and no component renders it anywhere on any plane. Credential presence does not come from the library payload at all: `InstallStates.tsx:126` passes a `unmet` list resolved against the workspace's connected credentials.

**Consequently `tier` becomes `visibility` throughout this workstream**, matching the shape M143_001 actually serves and the existing `fleet-library.ts` comment ("Each entry carries `visibility`, so the install flow keys the create body off the chosen tier").

**Load-more replaces an exhaustive walk, so it must not silently truncate.** Today `fleet-library.ts` follows `next_cursor` to exhaustion precisely because reading one page "would drop every entry past the server's page size *silently*". Paging the gallery reintroduces that hazard, so the retained-count and remaining-state must be visible to the user rather than implied by a button.

- **Dimension 2.1** — gallery pages append, are retained, and selection issues no further request → Test `test_fleet_load_more_then_selected_summary` — **DONE**
- **Dimension 2.2** — deep-link/status/loading semantics are exact → Test `test_fleet_deep_link_and_typed_states` — **DONE**
- **Dimension 2.3** — a paged gallery never silently hides entries past the loaded page → Test `test_fleet_gallery_paging_discloses_remaining` — **DONE**

### §3 — The session keeper is retained; no canary is built — **DONE**

**The keeper is a fix for an observed failure, not a precaution.** It landed as
`a2d507bfb fix(app): keep Clerk sessions alive for server actions` (Jul 22,
2026), and the same commit deleted 14 lines from
`tests/e2e/acceptance/fixtures/auth.ts` and 4 from `lifecycle.ts` — fixture
workarounds the acceptance suite had been using to paper over the very failure
it fixes. Anyone reconsidering removal should start from that commit, not from
this section.

**The arithmetic is what makes it load-bearing.** A Clerk session token lives
about **60 seconds**; the keeper refreshes at **45**, leaving 15 seconds of
headroom, and the daemon enforces expiry with no leeway
(`auth/jwks_standard_claims.zig:36` — `if (exp <= now_s) return VerifyError.TokenExpired`).
Without the refresh, a tab open for 90 seconds has a lapsed token, and the next
Server Action POST fails — a POST cannot complete Clerk's redirect handshake, so
the user loses the submission rather than being redirected to sign in. That is
the failure `a2d507bfb` fixed.

Note the two expiries are different things and are easy to confuse: the **token**
is ~60 seconds and is refreshed silently; the **session** — the login itself —
lasts days and is configured in Clerk's dashboard, not in this repository. A user
sitting on a page is never signed out at the 60-second mark; only the short-lived
credential behind it turns over.

The original §3 proposed proving the keeper removable: five scenarios, 20
attempts, three browser engines, two cohorts, a provisioned capture and a grading
checker.

**That was built and then reverted, deliberately.** The question it answered was
"may this component be deleted", and deletion has no user-visible benefit — the
keeper's entire cost is one background request per 45 seconds per open tab.
Against that: a dedicated Clerk instance dialled to its minimum session lifetime
(degrading sign-in for everyone else using it), two full application builds, and
roughly four hours of waiting per run, because two of the five scenarios must
cross genuine session expiry.

A cheaper single-scenario regression test was considered and also rejected: any
test that waits for real session expiry is slow and environment-fragile in every
run, and the first time it flakes it gets skipped — a skipped test reads as
coverage while providing none.

The component keeps the testing proportionate to its size: `lib/auth/client.test.tsx`
covers mount, the refresh interval, and listener cleanup. That is the right level
for 45 lines whose behaviour is a timer and three event listeners.

**Verdict: `retain`.** Not "the capture could not be provisioned", which would
imply the evidence is still wanted. A component introduced to fix a specific,
observed Server Action failure does not need a two-cohort browser matrix to
justify continuing to exist; the commit that added it is the justification.

**What would actually reopen this.** Not a tidiness impulse. Either Clerk
documents that its own client refreshes the session cookie on interval, focus,
and resume — making the keeper provably redundant, which is a documentation
question rather than a measurement one — or the keeper is implicated in a real
problem someone has hit. Absent one of those, leave it alone.

- **Dimension 3.1** — the keeper stays mounted and its unit coverage holds → Test `lib/auth/client.test.tsx` (existing) — **DONE**

### §4 — The `q` search parameter is retired — **DONE**

**Added at EXECUTE on Indy's in-session decision (Discovery A10), superseding Amendment A8's "Deleting `q` is NOT in this workstream".**

`q` is a substring filter on `GET /v1/fleet-libraries` (matched over `id`, `name`, `description`) and on `GET /v1/models`. It is fully built and published: trim/collapse normalization, a 128-byte bound mapped to `UZ-LIBRARY-003`, UTF-8 validation, Postgres-side `NFKC` fold with `lower()`, `LIKE` metacharacter escaping applied *after* the fold, participation in the keyset cursor's canonical key, and a dedicated normalization test file.

**No client sends it.** `fleet-library.ts` and `tenant_model_entries.ts` both build `URLSearchParams({limit})` and stop; the Command-Line Interface (CLI) and the runner never send it. It is reachable only by hand-writing a URL. This is the same position the workspace detail route was in when M143_001 deleted it — built, hardened, published, uncalled — and the resolution follows that precedent rather than inventing a consumer for it. The realistic scale is under 15 fleets and under 10 fleet libraries per workspace, where a filter solves a problem nobody has.

`UZ-LIBRARY-003` is **narrowed, not retired**: it keeps its identifier and registry row for the `limit`-out-of-range cause. Only the search-bound cause goes.

**`provider` is deliberately left in place.** It sits in the same `Filters` struct and the same cursor key as `q`, and it is equally uncalled — but it was not authorized, and this workstream does not widen its own scope. Recorded in Discovery A10 as a standing finding.

- **Dimension 4.1** — no `q` reaches any handler, SQL module, or cursor key, and the gallery/models reads behave identically without it → Test `test_library_reads_ignore_retired_search_param` — **DONE**
- **Dimension 4.2** — `UZ-LIBRARY-003` still fires for an out-of-range `limit` and no longer has a search-bound cause → Test `test_library_limit_bound_survives_search_retirement` — **DONE**

### §5 — `support_files` leaves the API surface; the manifest stays stored — **DONE**

**Added at EXECUTE on Indy's in-session decision (Discovery A11).**

The persisted `support_files_json` manifest is written at import and read back by nothing user-facing. Verified rather than assumed: **zero production `.tsx` files reference `support_files`** anywhere; the install screen's directory contains no occurrence of the string at all; the Command-Line Interface (CLI) and runner never read it. The admin catalog response is the only surface that carries it, and the admin screen renders nothing from it — the one non-empty fixture, `FleetLibrariesView.test.tsx:44`, appears once and is never asserted, existing only to satisfy a non-optional type.

Install does not need it, and the reason is structural. The Command-Line Interface (CLI) resolves a gallery entry and sends only `{platform_library_id}` or `{tenant_library_id}`; `create_fleet_bundle.zig` reads `content_hash` and derives `snapshot_key`; `runner/bundle_extract.zig` downloads the canonical tar by `content_hash` and untars support files **from the tar's own entries**. The authoritative file list travels inside the content-addressed tar. Postgres holds a second copy that nothing consults.

**The column is retained and every write to it stays.** Indy's call is to keep storing the manifest as durable provenance and stop returning it — so no migration, no `DROP COLUMN`, and no edit to a frozen slot file (`SCHEMA_CONVENTIONS.md` §Migration Model: shipped slots are frozen history). `importer.zig` still validates paths, hashes content, and persists the manifest; `library_store.zig` still carries the write fields; the INSERT/UPSERT SQL is untouched.

**Support-file bytes are untouched and remain fully load-bearing** — this section removes a duplicated *index*, never content.

- **Dimension 5.1** — no API response carries `support_files`, and the published schemas agree → Test `test_library_responses_omit_support_manifest` — **DONE**
- **Dimension 5.2** — the manifest is still persisted on import and survives a round-trip through the store → Test `test_import_still_persists_support_manifest` — **DONE**
- **Dimension 5.3** — narrowing the SELECT lists does not shift any positional row read → Test `test_catalog_row_projection_indices_hold` — **DONE**

## Interfaces

`TenantModelPages = retained rows + current starting_after; projection scope=current response page only`.
`FleetSummaryPages = retained items + current next_cursor + remaining disclosure`. Each item is the M143_001 gallery row: `{visibility:"platform"|"tenant",id,name,description,created_at,requirements,required_credentials_reasons}`. There is no separate detail resource — see §2's amendment.
Deep link: `/w/{workspace}/fleets/new?library_visibility=platform|tenant&library_id=<encoded-id>`.

List position survives a reload: the active `starting_after` is mirrored into the URL as `library_after`, replacing rather than pushing history so load-more does not fill the back stack. A reload, a shared link, or a back navigation from a detail view restores the same page rather than dropping the user at the first one. Absent parameters mean the first page, and an unparseable `library_after` is discarded in favour of the first page rather than surfacing an error — a bad link should still land somewhere useful.
Refresh state: last success plus idle/loading/refreshing/error and typed error.

## Failure Modes

| Mode | Cause | Injection | Handling | Named test |
|---|---|---|---|---|
| Stale catalogue load | older preload resolves last | deferred promises | ignore stale; latest generation wins | `test_model_picker_prefetch_policy_and_latest_result` |
| Typed status | 401/403 on the gallery read | response fixtures | distinct action/state | `test_fleet_deep_link_and_typed_states` |
| Unknown selection | `library_id` absent from the gallery | foreign/absent fixture | not-found selection state; no enumeration, page still usable | `test_fleet_deep_link_and_typed_states` |
| Hidden remainder | entries exist past the loaded page | multi-page fixture | remaining count disclosed, never silently dropped | `test_fleet_gallery_paging_discloses_remaining` |
| Refresh fault | network/503 after success | rejected fetch | stale content + retry | `test_refresh_retains_authorized_content` |
| Reduced motion | media preference | emulated media | no shimmer/transform | `test_library_reduced_motion_state` |

## Invariants

1. Request spies enforce no ordinary global catalogue/secret request and current-page-only decryption.
2. Discriminated types enforce `(visibility,id)` everywhere.
3. Reducer tests enforce retained authorized data until successful replacement.
4. A paged list discloses what it has not loaded; truncation is never silent. This binds **both** surfaces: the Models registry (`tenant_model_entries.ts`) and the Fleet gallery (`fleet-library.ts`) each replace an exhaustive `next_cursor` walk that exists specifically to stop later entries vanishing unannounced.
5. Retiring a read never retires a write. §5 removes `support_files` from every response while `support_files_json` continues to be computed, validated, and persisted on every import — and support-file *bytes* stay untouched end to end, from `importer.zig` hashing through `runner/bundle_extract.zig` materialization.
6. Narrowing a SELECT list never shifts a positional read. Every `row.get(T, i)` index downstream of a removed column is re-derived and pinned by test, because a silent off-by-one here reads a neighbouring field rather than failing.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| existing page analytics | product | unchanged | existing allow-list | no new identifiers | `test_models_registry_retains_pages_without_extra_decrypts` |

## Test Specification (tiered)

This table is the complete set. Every row is mandatory, including the failure rows — an agent that implements only the dimension rows ships an incomplete workstream.

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 3.1 | unit | `lib/auth/client.test.tsx` (existing) | the keeper stays mounted; refresh interval and listener cleanup hold |
| 1.1 | integration | `test_models_registry_retains_pages_without_extra_decrypts` | prior rows retained; exactly one page request per load-more, asserted by request spy on the API client, no global catalogue or secret request on an ordinary visit; entries past the loaded page are disclosed, never silently dropped (Invariant 5) |
| 1.2 | browser | `test_model_picker_prefetch_policy_and_latest_result` | coarse/Save-Data hover blocked; focus/open allowed; latest wins |
| 2.1 | integration | `test_fleet_load_more_then_selected_summary` | append summaries; selection issues no further request; no secret preload |
| 2.2 | end-to-end | `test_fleet_deep_link_and_typed_states` | server selection, exact 401/403/not-found states, no flash |
| 2.3 | integration | `test_fleet_gallery_paging_discloses_remaining` | with entries past the loaded page, the retained count and remaining state are rendered; no entry is dropped without disclosure |
| — | integration | `test_refresh_retains_authorized_content` | a network or 503 fault after a success keeps the last successful rows on screen and offers retry, never falling back to an empty state |
| — | browser | `test_library_reduced_motion_state` | under `prefers-reduced-motion: reduce` no shimmer or transform runs, and loading remains distinguishable from loaded |
| — | end-to-end | `test_library_list_position_survives_reload` | after load-more, a reload restores the same page from `library_after`; back from a detail returns to that page, not the first; an unparseable `library_after` falls back to the first page without an error state |
| 4.1 | integration | `test_library_reads_ignore_retired_search_param` | a `?q=` on either read is inert — same rows, same order, same cursor as the request without it; no handler, SQL module, or cursor key retains a search field |
| 4.2 | unit | `test_library_limit_bound_survives_search_retirement` | `UZ-LIBRARY-003` still fires for `limit` out of range, and no code path can raise it for a search bound |
| 5.1 | integration | `test_library_responses_omit_support_manifest` | neither the admin catalog nor the onboard response carries `support_files`, and both published schemas agree with what is served |
| 5.2 | integration | `test_import_still_persists_support_manifest` | an import writes `support_files_json` and it survives a round-trip through the store — the column and its writes are retained, only the read-back goes |
| 5.3 | unit | `test_catalog_row_projection_indices_hold` | every positional `row.get` index in the narrowed projection maps to the field it names, so removing a SELECT column cannot silently shift a read |

**Decryption is asserted indirectly and deliberately.** Decryption happens server-side and is owned by M143_001. Row 1.1 asserts what this workstream controls — the number and shape of requests the UI issues — and treats "no extra decrypts" as a consequence proven by M143_001's `test_tenant_registry_page_is_bounded`. Naming that split here stops an agent from trying to observe decryption from a browser context.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Lazy paged UI tests pass | `bun --cwd ui/packages/app test` | exit 0 | P0 | ✅ `188 files / 1947 tests / 0 failures` |
| R2 | Acceptance browser paths pass | `bun --cwd ui/packages/app run test:e2e:acceptance` | exit 0 | P0 | ⚠️ `19 passed, 1 failed (5.6m)` — see grading note |
| R3 | Session keeper retained and unit-covered | `bun --cwd ui/packages/app test lib/auth/client.test.tsx` | exit 0 | P0 | ✅ `3 passed` — keeper mount, refresh interval, listener cleanup |
| R4 | Diff is scoped | `git diff --name-only origin/main` | 0 unlisted paths | P0 | ✅ 36 files, every one in the table above |
| S1 | Repository gates | `make test-unit-all && make lint-all && make harness-verify && gitleaks detect` | exit 0 | P0 | ⚠️ `harness-verify` ALL GATES GREEN; `gitleaks` no leaks found; `lint-all` fails pre-existing — see note |
| R5 | `q` is gone from every surface | `grep -rn '"q"' src/ public/openapi/ \| grep -v provider` | 0 library search hits | P0 | pending re-grade |
| R6 | Zig suites pass and both targets cross-compile | `make test-unit-all && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | pending re-grade |
| R7 | No response carries `support_files`, but imports still store it | `make test-integration` (Dimensions 5.1, 5.2) | exit 0 | P0 | pending re-grade |
| R8 | Published schemas match what is served | `make check-openapi` | exit 0 | P0 | pending re-grade |

**Grading protocol (VERIFY):** run verbatim; record ✅/❌ and one decisive line.

**R2 — one acceptance failure, attributed and not from this diff.**
`signup-lifecycle.spec.ts` → "fresh signup walks install → observe → bill →
halt entirely in the UI" timed out on
`expect(getByRole('alertdialog')).toBeHidden()` at
`tests/e2e/acceptance/fixtures/lifecycle.ts:42`. That helper is `confirmAction`,
the Stop/Resume confirm dialog on the **fleet detail** route. This workstream
touches `fleets/new` and `settings/models` only — `git diff --name-only
origin/main` contains no file on the lifecycle, halt, or confirm path. The
journey reached the halt step, so install itself completed, and the nineteen
passing specs include those that exercise the changed install and models
surfaces. Recorded as attributed-not-cleared: the reasoning is sound but the
decisive proof would be re-running the same spec on `origin/main`, which was
not done.

**S1 — `make lint-all` is broken on `origin/main`, not by this branch.**
`check-route-registration-doc` fails with `docs/REST_API_DESIGN_GUIDELINES.md
not found`. Commit `b6bbfd133 chore(rules): thin to the global operating model`
deleted that file (781 lines) while `scripts/check_openapi_url_shape.py` and
`scripts/check_openapi_route_coverage.py` still cite it and a checker still
requires it. `git show origin/main:docs/REST_API_DESIGN_GUIDELINES.md` confirms
it is absent there too. The other three S1 commands pass. Flagged for a
follow-up: either restore the document or retire the checker that demands it.

## Dead Code Sweep

`AuthSessionKeeper` is retained unchanged (§3), so no keeper file or mount diff is expected. Removed eager reads and bare Fleet identities have zero production references.

## Out of Scope

- M143_001 implementation and M143_003 evidence.
- Authentication verifier, proxy layer, token, provider, or policy redesign.

---

## Product Clarity (authoring record)

1. **Successful user moment** — rows/cards remain stable while more data arrives and resumed work succeeds.
2. **Preserved user behaviour** — model management, Fleet install, Clerk sign-in, and Server Actions.
3. **Optimal-way check** — remove requests and scope Suspense; animation is not a fix.
4. **Rebuild-vs-iterate** — refactor read/state boundaries, not auth architecture.
5. **What we build** — retained pages, current-page projection, typed states, and streaming shells.
6. **What we do NOT build** — no eager warmup, secret preload, control morph, or token work.
7. **Fit with existing features** — extends route Suspense, Clerk, and M143_001.
8. **Surface order** — UI follows the pinned API.
9. **Dashboard restraint** — only actual loading/error/retry state.
10. **Confused-user next step** — sign in, permission guidance, return/search, or retry.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** model pages and Fleet states are independent slices; the keeper is left alone.
- **Alternatives considered:** spinners hide the problem rather than fixing it. Keeper removal was specced, built as a three-engine canary, then reverted — see §3.
- **Patch-vs-refactor verdict:** **refactor** of read/state boundaries only.

## Discovery (consult log)

- **Consults** — Oracle second-pass blockers incorporated exactly.

- **Amendment A1 (PLAN) — §2 reconciled to the shipped mechanism; `tier` → `visibility`.** This workstream was carved out of a single parent spec, `M143_001_P1_API_OBS_UI_FLUID_LIBRARY_READS.md`, by `660811e2f docs(m143): split fluid library refactor spec` (Jul 25, 2026: 02:11 AM). That parent held **both halves** of the Fleet detail route: the producer (`handlers/library/gallery_detail.zig` CREATE, "Add the typed detail route", router matching, OpenAPI registration) and the consumer (`fleet-library.ts` "Expose paged summaries and selected details", `InstallFleet.tsx` "fetch selected detail"). The split sent the producer to M143_001 and the consumer here.

  M143_001 then shipped first, asked "who calls this?", and correctly saw nobody — the caller was sitting unstarted in `pending/` as this workstream. It retired the route, `UZ-LIBRARY-007`, the `FLEET_DETAIL_MAX_*` budgets, and the OpenAPI operation; `router.zig` now asserts the former URL is unrouted and `gallery_keyset_integration_test.zig` pins it to 404 for a resident entry.

  **The deletion nonetheless stands, because M143_001 met the parent's actual goal by another mechanism.** The parent wanted summary/detail separation for page compactness; M143_001 achieved that by *trimming the summary* (shedding only `support_files`, the field breaching the 512 KiB ceiling at ~630 KB/page) while retaining `requirements` and `required_credentials_reasons`. Verified, not assumed: no component renders `support_files` on any plane — every `.tsx` occurrence is a test fixture — and credential presence never came from the library payload, `InstallStates.tsx:126` resolving `unmet` against the workspace's connected credentials. §2 is therefore reconciled to the gallery, and `tier` becomes `visibility` to match what M143_001 serves (`fleet-library.ts`: "Each entry carries `visibility`, so the install flow keys the create body off the chosen tier").

- **Amendment A2 (PLAN) — new Dimension 2.3, remaining-count disclosure.** §2 replaces an exhaustive `next_cursor` walk that exists specifically because a single-page read "would drop every entry past the server's page size *silently*". Paging reintroduces that hazard, so the amendment adds Invariant 5 and Dimension 2.3 rather than removing the protection unreplaced.

- **Amendment A3 (PLAN) — Files Changed gains `InstallFleet.tsx` and `InstallStates.tsx`.** They own deep-link initialization and the selection/ConnectGate states that §2 now renders from the held summary. Recorded here per the Scope-grading rule rather than added silently.

- **Amendment A4 (EXECUTE) — Files Changed gains `.../models/components/ModelsRegistryCells.tsx`.** Dimension 1.2 requires focus and eligible-hover prefetch for the **Edit** dialog as well as Add, and Edit's trigger is the per-row control inside `ActionsCell`, which lives in that file. Wiring only the open signal would have left the Edit path's intent loading unimplemented. `ModelsRegistryTable` owns the policy decision (`maySpeculateOnHover`) and the cell only reports the gesture, so the file gains two callback props and no logic.

- **Known consequence of intent loading, accepted — the Context column's rates line degrades on a registry the server did not price.** `ContextCell` resolves `identity.rate ?? libraryRateFor(...)`, so the global catalogue is a *fallback* behind the server-provided per-row rate. With the catalogue no longer fetched on mount, a row the server did not price shows `Billed by provider` (entry rows) or `Rates unavailable` (the Default row) until intent warms the catalogue. Both strings remain true statements, and the existing cases `test entry rows render server-provided rates without depending on the public catalogue` and its Default-row sibling prove the primary path is unaffected. The narrowest edge is a tenant with **zero** entries whose platform default is also unpriced by the server: that page has no Edit control, so its only catalogue-warming affordance is focus or hover on **Create model**. Judged acceptable rather than reason to keep an unconditional catalogue fetch on every visit; revisit if the unpriced-default case turns out to be common.

- **Amendment A5 (EXECUTE) — Files Changed gains `.../fleets/new/InstallEntry.tsx`.** It is the sole producer of the install deep link, so switching the link to `library_visibility` + `library_id` cannot be done anywhere else. The parameter names now live in one exported `deepLinkHref()` beside the parser's expectations, since a drift between producer and parser is a silently dead link rather than a failure.

- **Amendment A6 (EXECUTE) — Dimension 2.1's "no secret preload" means no secret *value* read, not no credential-name read.** The install screen keeps its `listSecrets` call, which projects `secret.name` only. That names list is workspace state rather than library state — it does not vary by selection — and `InstallStates.tsx:126` needs it to compute the ConnectGate's `unmet` set on first paint. Dropping it would blank visible credential gating, which §2 explicitly forbids. No secret value is read, nothing is decrypted, and no token reaches the client; the prohibition the dimension was written for still holds.

- **Known limit of server-resolved deep links, accepted.** Selection is now resolved on the server against the page that was just read, which is what removes the gallery flash (the previous client effect painted the gallery, then replaced it a frame later). The consequence is that a `library_id` living beyond the loaded page resolves to the not-found selection state rather than being hunted down by a walk. With a 100-entry page limit this is the tail case, `library_after` carries page position for shared links, and the not-found state neither enumerates nor errors the page. Revisit if libraries routinely exceed one page.

- **Amendment A7 (EXECUTE) — streaming Suspense regions, at Indy's ask; Files Changed gains `.../fleets/new/library-docs.tsx`.** Indy asked whether a larger refactor using current React 19 / Next.js features would beat the patch. It does, and the spec already required it: the Solution summary names "stable Suspense regions", Prior-Art names "Approvals/Events loaders", and Files Changed already listed `fleets/new/loading.tsx` as CREATE. Both routes awaited every read before painting a pixel, so each screen was gated on whichever of its reads answered last — worse than the Approvals route beside it. Each `page.tsx` now paints its header immediately and streams an exported async data region (`InstallFleetData`, `ModelsRegistryData`) under `<Suspense>`, matching `ApprovalsData` exactly. `library-docs.tsx` gains the route title/description so `page.tsx` and the new `loading.tsx` cannot drift — a mismatched placeholder reads as the page changing its mind mid-navigation.

  **Suspense buys latency here, not error handling.** Neither data region is allowed to reject: both resolve to data-or-typed-error. A rejected promise would throw in render and need an ErrorBoundary, which would trade the failed-versus-empty distinction this workstream exists to draw for one undifferentiated fallback.

  **Declined, with reasons, from the same review:** `useOptimistic` for load-more (optimistic UI needs a predictable outcome; the next page's contents are unknowable, so there is nothing to show); a React Compiler flip (M143_005 ring-fences it so transfer and hydration stay the only variables); and `cacheLife`/`unstable_cache` on the gallery (per-workspace and auth-gated, so `force-dynamic` is correct). `useDeferredValue` remains the right tool for §1's search revalidation and is not yet built.

- **Amendment A8 (EXECUTE) — search removed from this workstream; `q` slated for deletion in its own workstream.** §1 carried a sentence about search retaining rows while revalidating, the Interfaces section mirrored `q` into a `library_q` URL parameter, and a Failure Modes row named a stale-search race. None of it had a surface: **no client anywhere sends `q`** — not the dashboard, not the Command-Line Interface (CLI), not the runner. The parameter is nonetheless fully built server-side (`handlers/library/gallery.zig`, `handlers/model_library.zig`, `handlers/library/catalogue_key.zig` where it joins the cache key, a dedicated `library_query_normalization_test.zig`, both OpenAPI path documents, and the keyset cursor's canonical JSON).

  That is the same position the workspace detail route was in when M143_001 deleted it — built, hardened, published, uncalled. Indy's call is to follow the precedent: **delete `q`** rather than build a consumer for it, because the realistic scale is under 15 fleets and under 10 fleet libraries per workspace, where a filter solves a problem nobody has. Search gets built when a workspace genuinely holds hundreds of entries, and not before.

- **Amendment A9 (REVIEW, Indy-directed) — `InstallEntry.tsx` deleted; A5's producer role is vacated.** Review found `InstallEntry` has no production consumer — `grep -rn InstallEntry` across `ui/` and `cli/` hits only the component and its own test, on this branch and on `main` — so A5's `deepLinkHref()` was a producer nothing rendered. Indy's call this session: delete it rather than wire it in. The deep-link **parameter surface survives the deletion**: `page.tsx` still parses `library_visibility` + `library_id` (+ `library_after`) for hand-built, shared, or docs-published links. The old `?library=` spelling stays unsupported, also Indy's call — any consumer with a real use case migrates to `library_id=`. Cascade: `LibraryCard` loses its `compact` prop (its only feeder was `InstallEntry`); `LibraryCard`'s no-badge branch keeps coverage through `InstallSourceSelector`, its one remaining consumer.

- **Amendment A11 (REVIEW) — review-army round: five specialists + two adversarial passes over the full branch diff.** Confirmed and fixed in place: (1) the new omit-manifest integration test probed a GET that the per-entry admin route never served (PATCH/DELETE only — it answered 405; the PATCH response, which reads `SELECT_ADMIN_CATALOG_ROW`, now carries that assertion); (2) two stale sentences in the published gallery spec (a pointer to the removed detail route, and a cursor description still naming a filter) — reworded, bundle regenerated; (3) missing `catch` on four server-action round-trips — a transport rejection could strand the Add dialog at "Checking your stored secrets…" with no retry, or escape a transition into routes with no error boundary; (4) the walk's non-advancing-cursor defense was lost in the paging move — a cursor that does not advance is now terminal instead of appending duplicates forever; (5) `use-stored-secrets` gained the provider's latest-wins guard; (6) speculative hover no longer re-fetches a known-failed catalogue (one request per hover with no backoff); (7) `library-types.ts` shed its consumerless load-status vocabulary and unreachable `notFound` kind, and now owns the shared `readErrorFrom`/`libraryErrorFromCause`/`LIBRARY_AFTER_PARAM` (previously four inline copies and a twice-spelled parameter name); (8) `next-env.d.ts` restored to the generated main-branch form. Coverage added on specialist findings: registry-table paging surface (append/retain, typed failure on the real table, non-advance, rejection), deep-link resolver positive + tier-mismatch paths, gallery 401/403 copy + `galleryErrorCopy` distinctness, total-null disclosure wording on both surfaces, a gallery `?q=`-inert integration proof, and a store-level manifest round-trip through the real tenant INSERT.

- **Amendment A10 (REVIEW) — three review findings hardened in place.** (1) The Add-model dialog now **fails closed on the stored-secret list**: `secretsLoad` state (`components/secrets-load.ts`) rides from `ModelsRegistryTable` into the dialog, both Save buttons gate on `ready`, and a failed load surfaces an alert with retry instead of `refreshSecrets` silently returning — submitting against an unloaded list skipped the name-ownership guard and the secrets POST upserts, so it could overwrite a stored credential unseen. `ready` is sticky: a failed *refresh* keeps the last good list live. (2) The Fleet gallery's **Retry now reaches the read that failed**: it was bound to `loadMore` and disabled whenever `next_cursor` was null — exactly the failed-first-read state it existed for; `retryFailedRead` re-requests the failed page when a cursor is held and page one otherwise. Both surfaces also map the action's transport status through `errorKindForStatus`, so a 401/403/503 keeps its specific copy client-side. (3) `ProviderModelSelect` **holds a disabled select while the catalogue is in flight** instead of letting the empty models array mount the free-text input and then swap it for a select mid-interaction; `CATALOGUE_STATUS` moved to `components/catalogue-status.ts` so the picker can key off it while tests stub the provider module's hooks. The same pass split `ModelsRegistryTable.tsx` back under the length cap along its stateless seams: pure sort/error helpers to `components/registry-view.ts`, the stored-secret state + fail-closed refresh to `components/use-stored-secrets.ts`.

  `library_q` was never a second name for `q` — it named a URL parameter that was never built. It is struck here rather than implemented. The Failure Modes row is **retargeted, not deleted**: the latest-wins mechanism it described IS built, as the catalogue provider's generation counter, and keeps its test.

  **~~Deleting `q` is NOT in this workstream.~~ SUPERSEDED by Amendment A10 — `q` is deleted here.** The original text read: *"It spans Zig handlers, the gallery and model SQL, the keyset cursor's wire format, the published OpenAPI parameter on two paths, and the `q` half of `UZ-LIBRARY-003` — none of which M143_002 touches, all of which would breach its User Interface (UI)-only scope, fire the ZIG and PUB gates its own table marks 'no', and collide with M143_003's Zig surface."* The blast-radius list is accurate and §4 implements exactly it; the scope objection is reversed by Indy's decision, the gate table now marks ZIG and PUB "yes", and the M143_003 collision no longer exists (PR #569 merged).

- **Amendment A9 (VERIFY) — canonical architecture repointed to `docs/architecture/web_app.md`.** The spec cited `user_flow.md` §8.7, which is about platform-versus-self-managed model and context-cap origin and has nothing to say about library reads or loading. `web_app.md` did not exist when this spec was drafted on Jul 24; it landed Jul 28 in PR #568 and is the correct home. Its statement 3 ("every route paints a shell before it paints data") and statement 5 ("`useEffect` is for subscriptions, not for loading") describe precisely what this workstream implemented, and its scoreboard carries a standing instruction to "re-measure at any milestone that touches the app and update this table in the same diff". Done: `useEffect` 22 → 20, `Suspense` 3 → 5.

  Two measurement notes recorded there rather than silently absorbed. The published Jul 27 figures (23 and 4) do not reproduce — re-running the listed greps against that same commit yields 22 and 3 — so deltas are counted from the measured baseline. And a comment in the new `fleets/new/loading.tsx` originally contained the bare word `Suspense`, which the scoreboard grep counted as a usage; the comment was reworded so the metric keeps meaning what it claims to measure.

- **Decision recorded (Indy, in-session) — §3 uses the Clerk instance in `ui/packages/app/.env.local`.** It is a `pk_test_` development instance. Indy directed that this is the instance to use and that the question is settled; the capture reads the configured session lifetime from Clerk's Backend API at run time and records it, with the instance kind, in report metadata. No further consult on instance provenance.

- **~~Out of scope~~ — RESOLVED by Amendment A11 and §5; the read path is removed, the column retained.** Original entry preserved below for its evidence, which the investigation confirmed. The open question it posed — *"does `sha256` stay as durable provenance, or should the admin screen render a file list instead?"* — is answered by keeping the manifest stored (provenance survives) and rendering nothing (no reader wanted one).

  **The `support_files_json` manifest read path is dead weight.** Distinct from support-file *bytes*, which are fully load-bearing: `importer.zig` validates paths and hashes content, and `runner/bundle_extract.zig` untars them into the workspace at execution. The persisted *manifest* is different — SELECTed at four sites (`fleet_library/sql.zig:103,116,242`, `gallery_sql.zig:154`), decoded by `entry_view.decodeSummaries`, projected into the admin catalog response by `catalog.zig:130`, and then rendered by nothing. The code documents its own redundancy: *"The per-file hash stays internal — it is a handle to stored bytes, and no reader needs it"* and *"`sha256` is read and dropped"*. The runner never reads it (`gallery_sql.zig:103` — bytes come from object storage by `content_hash`). Retiring it is a Zig + SQL + response-schema change that would breach this workstream's User Interface (UI)-only scope and collide with M143_003's surface, and it carries an open question (does `sha256` stay as durable provenance, or should the admin screen render a file list instead?). Flagged to Indy; awaiting a decision on whether it becomes its own workstream.

- **Amendment A10 (EXECUTE) — `q` is deleted in this workstream, superseding A8.** A8 recorded the deletion as belonging to a follow-up workstream, on two grounds: it would breach a User Interface (UI)-only scope, and it would collide with M143_003's Zig surface. Indy's in-session decision reverses the first, and the second is now factually stale — M143_003 merged as PR #569 (`7d0c881b6`) and its spec sits in `docs/v2/done/`, so there is no surface left to collide with.

  > Indy (2026-07-28): "Yes delete q in this PR and not in a new milestone" — context: retiring the uncalled `q` search parameter; reaffirmed as "Yes delete q as well."

  A8's reasoning for *why* `q` should go is unchanged and remains the record: built, hardened, published, uncalled, at a scale where a filter solves a problem nobody has. Only its routing to a later workstream is superseded. §4 carries the implementation.

  **Standing finding, not actioned — `provider` is in the identical position.** `GET /v1/models?provider=` is normalized at `model_library.zig:163`, published at `public/openapi/paths/models.yaml:57`, shares the `Filters` struct and the cursor key with `q`, and is sent by no client — `fleet-library.ts` and `tenant_model_entries.ts` both build `URLSearchParams({limit})` and stop. The `provider` references in `model_library.ts` are client-side helpers over an already-fetched list (`modelsForProvider`), not a query parameter. Surfaced to Indy twice during EXECUTE and not authorized, so it stays: retiring it alongside `q` would have been one edit instead of two passes over the same five files, but an agent does not widen its own scope. A follow-up workstream can take it cheaply.

- **Amendment A11 (EXECUTE) — `support_files` leaves the API surface; the column and its writes are retained.** The original out-of-scope note (retained below) proposed retiring the manifest read path and left open whether `sha256` should survive as provenance or the admin screen should render a file list. Investigation answered the prior question — nothing reads it, admin included — and Indy settled the disposition.

  > Indy (2026-07-28): "I think i dont want to drop the column support_files_json (Store it) but dont return it in the API response" — context: the persisted manifest, after confirming no reader exists on any plane.

  This is deliberately **not** the `q` resolution. `q` is deleted because it is a filter nobody asked for; the manifest is *kept* because it is a durable record of what a stored bundle contained, and keeping it costs one JSONB write per import. Only the projection goes. The consequence is that no migration is authored, `SCHEMA_CONVENTIONS.md` §Migration Model's frozen-slot rule is never approached, and the `DROP COLUMN` owner-decision requirement at `SCHEMA_CONVENTIONS.md:9` does not apply.

  **Correction to an earlier claim in this log.** An intermediate position held that dropping the column would lose the file list irrecoverably. That was wrong: the canonical tar is self-describing and `runner/bundle_extract.zig` already reads the file list from the tar's own entries rather than from Postgres. The manifest is a second copy. The retention decision therefore rests on provenance convenience, not on data that exists nowhere else — worth stating plainly so a later reader does not inherit a false constraint.

- **Rules inconsistency, flagged not fixed — SCHEMA GUARD contradicts SCHEMA_CONVENTIONS.** `dispatch/write_sql.md`'s Schema Table Removal Guard branches on `VERSION < 2.0.0` into a teardown-rebuild model that lists `ALTER TABLE` and `DROP TABLE` as **forbidden**. `VERSION` is `0.23.0`, so read literally the guard forbids the additive migration that `SCHEMA_CONVENTIONS.md` §Migration Model (owner decision, Jul 22, 2026) requires — and that `schema/032`'s own header cites as governing. The conventions document is the one the dispatch names as source-of-truth, so additive wins; §5 needed no migration either way, so nothing here depended on the resolution. Raised to Indy as a `dotfiles` fix rather than touched from this repository.

- **Amendment A12 (VERIFY) — the named-frame fix left two test doubles behind, and the full app suite was red.** The prior session recorded `make test-unit-app` as "expected green, unverified as a whole". It was not: 58 tests failed across `lib/streaming/fleet-stream-registry.test.ts` (55) and `fleet-stream-retry.test.ts` (3).

  One cause, not fifty-eight. The named-frame fix added `es.addEventListener(...)` to `startEventSource`, but three test files each carried their own hand-rolled `FakeEventSource` and only `tests/use-fleet-event-stream.test.ts` was updated. The other two had no `addEventListener` at all, so every test that opened a connection threw inside `subscribe()` — and 55 of the 59 registry tests open a connection.

  Both stale copies carried a comment asserting the duplication was deliberate ("centralizing was considered and rejected — the helper is small and the duplication keeps each test file freestanding"). That rationale is what the failure falsifies: a double free to model a friendlier server than the real one is how an `onmessage`-only client shipped against a green suite in the first place. The three copies are now one `tests/helpers/fake-event-source.ts`, browser-faithful by construction — named frames reach only their named listeners, an unsubscribed kind drops silently, and `emitRaw` feeds the no-kind fallback channel verbatim so the parse guards keep their coverage. Red-green re-proved: removing the named-listener wiring fails 8 tests across 2 files.

  Two further copies exist (`lib/streaming/workspace-stream.test.ts`, `tests/dashboard-fleets-wall.test.tsx`). Both already implement `addEventListener`, both are green, and neither was touched by this diff — surfaced here rather than swept, since they model a different subject (the workspace multiplex stream).

- **Amendment A13 (EXECUTE, Indy bug report) — the Events tab could not scroll, and the cause was not the one suspected.** Expanding a `×10` runs group on the fleet detail Events view left the page unscrollable in a non-maximized window. The standing hypothesis blamed a viewport-height container with `overflow-hidden` around `EventsList`. Measured in Chromium against a faithful reduction of the real class chain (Shell → page → gate → row → content → EventsList → DataTableView → viewport), that was wrong on both counts, and two plausible fixes were falsified:

  - `stickyHeader={false}` does **not** fix it. `overflow-x-auto` alone makes the box a scroll container on both axes (a box whose one axis is not `visible` computes the other to `auto`), so the containment survives losing `overflow-y-auto`.
  - Bounding the flex chain (`min-h-0` on the row and content wrappers) does **not** bound it either — the growth is driven from `min-h-full` above, and the viewport still measured 1240/1240.

  The actual cause is `overscroll-behavior: contain` on the DataTable viewport. That div is a scroll container with *nothing to scroll* (client == scroll == 1240px), and two-axis containment therefore swallows the wheel instead of letting it chain to `main`, which is the element that needs to scroll. With the pointer anywhere over the rows — which is most of the content area — the page is frozen. Maximizing hides it only because the content then fits.

  Fix is `overscroll-x-contain`: the viewport exists to scroll wide tables horizontally, so containing that axis keeps the intent while the vertical wheel chains to the page. This is a pre-existing defect, not branch-introduced — the classes came from `55231456f` on `main`. Regression pinned in `DataTable.test.tsx`; the causal evidence is a browser measurement, which no jsdom test can reproduce.

- **Amendment A14 (EXECUTE, Indy design report) — the unsorted sort indicator was the heaviest glyph in the header.** lucide draws `ArrowUpDown` with two full shafts spanning 16 of its 24 units and 18 wide, against the single sorted arrow's 14 × 14 — so at an identical `size={14}` the state meaning "no sort applied" rendered 14% taller and 29% wider than the state meaning "sorted", on every sortable column at once. Replaced with `ChevronsUpDown` (10 units wide, no shafts) at one shared `SORT_ICON_SIZE`, so the header also stops resizing as sorting changes. Judged against a rendered comparison, not asserted.

- **Amendment A15 (EXECUTE, Indy reports) — runner row affordances.** The host-id `CopyButton` is removed: it existed because the id is truncated in the cell, but at real host-id lengths there is nothing to truncate and the glyph was pure noise. The status pair now leads with administrative state and follows with liveness ("active online"), because what an operator has done to a runner decides what it may do, while liveness only reports what it is doing right now. Both pinned in `runners-list.test.ts`.

- **Amendment A16 (EXECUTE) — secret creation claims a free name; `UZ-VAULT-005`.** The adversarial review's create-vs-rotate TOCTOU is closed server-side rather than deferred.

  > Indy (2026-07-28): "Well simplest things is conflict? like we did for workspace." — context: choosing between a client-side gate, a new pending spec, and a server-side conflict for concurrent creates on one secret name.

  The routes were already separate (`POST /v1/workspaces/{id}/secrets` creates, `PATCH .../{secret_name}` rotates) and the UI already had a rotate-only dialog; only the storage layer conflated them, via a single blind `ON CONFLICT … DO UPDATE`. `INSERT_SECRET_IF_ABSENT` adds a `DO NOTHING` arm composed from the same shared column list, so the two arms cannot drift; the affected-row count is the answer and `crypto_store.create` raises `error.SecretNameTaken` on zero. The uniqueness decision is Postgres's, so two concurrent creates on one name resolve to one `201` and one `409` with no read-then-write window.

  Scope was deliberately narrow: the OAuth connector callbacks and the token refresh stay on the overwriting form, because re-connecting a provider *is* a rotation. An absent row count is treated as "not written" — for a credential, answering "that name is taken" beats reporting a success we cannot confirm, and `DO NOTHING` guarantees nothing was overwritten either way.

  This rides the breaking release rather than waiting: `0.24.0` already ships an Upgrading section, and holding the 409 back would make `0.25.0` a second breaking release for one line of SQL.

- **Amendment A17 (VERIFY) — decisions taken on the open review findings.**
  - *Client sort over a partial page* — accepted at current scale with a visible annotation; no workspace approaches the page size at which a sorted subset misleads.
  - *Perf list* — **withdrawn, not deferred.** The two proposed Postgres indexes were challenged by Indy and did not survive checking: `core.tenant_fleet_library` already carries `idx_tenant_fleet_library_ws_created_at (workspace_id, created_at DESC)`, so the proposed partial adds only an `id` tiebreaker for rows sharing a millisecond; `core.model_library` is a seeded catalogue small enough that the planner will sort without an index. The benefit was asserted, never measured, and the claim is retracted. Only the shared paged-list footer survives, and its justification is drift between two surfaces rather than performance.

- **Amendment A13 (VERIFY) — the coverage lane was red, and the reason it looked unfixable was a misreading.** `ui/packages/app` runs a 100% istanbul threshold. It sat at 99.71% statements / 99.41% branches, from code that landed on this branch without tests while the lane went unrun (`test-unit-app` skips coverage). A prior note held that istanbul was mis-attributing multi-line object literals inside `catch` blocks. It was not: the text reporter's "Uncovered Line #s" column lists lines carrying uncovered **branches**, not only uncovered lines, and of nineteen gaps exactly two were line misses. Reading `coverage/lcov.info` directly — `DA:` for lines, `BRDA:` for branch arms — gives arm-level truth in one pass. Recorded because the wrong reading cost a session and pointed at a tooling escape that was never needed: the repository still contains no `istanbul ignore`.

  Fifteen tests closed seventeen of the nineteen. The stale-response guards in `use-stored-secrets` needed two reads genuinely in flight at once, which the dialog cannot stage reliably, so they are driven from a direct hook test with deferred promises. The two server-render guards are reached with `vi.stubGlobal("window", undefined)` — happy-dom always defines the binding, so removing the *value* is what makes `typeof window` report `"undefined"`.

  **A defect surfaced while closing them.** `ModelsRegistryTable`'s Retry renders the moment a read fails but stays `disabled` until the transition settles, so a click inside that window is silently dropped — which is what a user who clicks Retry promptly gets. The first version of the test hit exactly that and passed while covering nothing.

- **Amendment A14 (VERIFY, Indy-directed) — two unreachable guards deleted rather than tested.** `loadMore`'s `if (nextCursor === null) return;` could not fire in either the registry table or the install picker: the control that calls it renders only inside `nextCursor !== null`. The cursor is now a `string` parameter, so the type states the invariant once instead of re-checking it where it cannot fail.

  `ModelCatalogueProvider`'s monotonic request id resolved a race its own single-flight guard already prevents — `inFlight` clears in `.finally`, which runs after `.then`/`.catch`, so every handler was provably the newest request's. Two guards, one job, and single-flight is the stronger of the two because it prevents the overlap rather than adjudicating it.

  > Indy (2026-07-28): "Drop the redundant generation ref" — context: the last two uncovered branch arms, choosing removal over lowering the gate or adding the repository's first coverage escape.

  Dimension 1.2's request-ordering property therefore rests on single-flight, pinned by a mid-flight assertion in `test_model_picker_prefetch_policy_and_latest_result` that no second read starts. `useStoredSecrets` **keeps** its generation ref: it takes no single-flight guard, open/close/reopen genuinely overlap there, and both stale arms are now tested.

  A standing levy to lower the app threshold to 99.6% was **withdrawn** once the gaps proved ordinary rather than structural. There is no 99.6% anywhere in the workspace: `ui/packages/app` is 100% on four axes, `cli/` is 100% line and function via `bunfig.toml` and `scripts/enforce-coverage.mjs`, `ui/packages/design-system` is 99%, and no other repository under `~/Projects` gates coverage at all.

- **Amendment A15 (DOCUMENT, Indy-directed) — the secrets `409` broke `agentsfleet secret create --force`, and the flag is retired.** `--force` skipped the client-side existence check and POSTed directly, relying on the endpoint upserting on `(workspace_id, key_name)`. §A16's change removed that upsert, so against the daemon this branch ships the flag could only ever have failed. The CLI suite stayed green because `cli/test/secrets.integration.test.ts` mocked a server that still accepted the POST — a test asserting against a permissive mock proves only that the client talks to the mock.

  > Indy (2026-07-29): "Retire --force instead" — context: choosing between teaching the flag to rotate via `PATCH`, retiring it, and shipping the break with a follow-up.

  `create` claims a free name; replacing a value is `delete` then `create`. The flag is rejected at the parser rather than ignored, so a script still passing it fails before it sends a secret body it believes will overwrite something. The preflight `GET` goes with it — it was a check-then-write over exactly the window `UZ-VAULT-005` exists to close — so `create` costs one round-trip and a taken name is reported as a skip with exit 0. The skip is keyed on the error **code**, not the bare `409`, so an unrelated future conflict on this route surfaces rather than being swallowed.

- **Amendment A16 (DOCUMENT, Indy-directed) — two pre-existing seams between the generator and the docs checker, surfaced by regenerating the error reference.** `make gen-error-codes` injects the live `VERSION` into `product_version`, while `check-documentation.py` pinned every page to `0.17.0`. The two have been in conflict since `VERSION` passed `0.17.0`, and the committed page satisfied the checker only by being hand-edited after generation — which is what its `0.17.0` and `verified: 2026-07-27` were.

  > Indy (2026-07-29): "Bump the site pin to 0.24.0" — context: choosing between exempting the generated page, moving the site-wide pin, and hardcoding the pin into the Zig generator.

  `EXPECTED_VERSION` is now `0.24.0` and `product_version` moved on all 24 pages. `verified` did **not** move, so no page claims a re-check it did not receive.

  Separately, `UZ-EXEC-016` used "re-provision", banned by DOC-05. It surfaced only now because the stale page had never carried that row. Indy directed rewording over an allowlist entry; the registry copy and the runner's matching log hint in `daemon/loop.zig` both now read "issue the host's runner token again", so the log and the reference tell an operator the same thing.

  The regeneration is otherwise pure catch-up: it adds the Workspaces, Schedules and Dashboard preferences sections, promotes "Fleet catalog" to "Fleet library catalog" with three codes it had been missing, and picks up `UZ-VAULT-005`. No code was dropped.

- **Standing finding, not actioned — `cli`'s own coverage floor is red on `origin/main`.** `npm run test` in `cli/` reports 99.97% function / 99.74% line against a 100% floor, in `api_key.ts`, `connector.ts`, `fleet_schedule.ts` and `cli.ts` — four files this branch does not touch. Verified rather than assumed: re-running the suite with this branch's `cli/` changes stashed gives identical numbers. `fleet_secret.ts` itself stays at 100%. Inherited debt, outside this workstream's Files-Changed scope, and cheap for a follow-up to take.

- **Metrics review** — privacy-safe aggregate only; funnel unchanged.
- **Skill-chain outcomes** — populated during implementation.
- **Deferrals** — none.
