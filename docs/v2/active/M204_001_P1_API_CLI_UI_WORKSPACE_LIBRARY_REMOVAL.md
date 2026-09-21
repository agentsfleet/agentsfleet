<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M204_001: A workspace can list and remove the Fleet libraries it onboarded

**Prototype:** v2.0.0
**Milestone:** M204
**Workstream:** 001
**Date:** Sep 21, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator-facing: an onboarded entry is today unremovable, and the pile-up breaks the acceptance suite.
**Categories:** API, CLI, UI
**Batch:** B1 — single stream; the grant precedes the handler, the handler precedes both clients.
**Branch:** `feat/m204-workspace-library-removal`
**Baseline revision:** 76eb9c2d305a480ea8437e65d63efbcaf7ba6de7
**Test Baseline:** `unit=2640 integration=561` at the comparison revision — derived, not measured: the branch's measured `unit=2662 integration=570` (both lanes exit 0) less the 22 unit and 9 integration tests this diff adds and the 0 it removes. A baseline worktree needs a full Rust build and the machine had 16 GiB free against a 34 GiB `target/` per worktree. **Final:** unit=2662 (+22) integration=570 (+9); TypeScript app 2913 · cli 1702 at `bd89852e1`, which carries workstream 002 too; 001's own delta is app +32 · cli +10.
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M204_001-76eb9c2d3.md`
**Depends on:** none
**Provenance:** LLM-drafted (claude-opus-5[1m], Sep 21, 2026)
**Canonical architecture:** `docs/architecture/fleet_bundles.md` §Two-tier Fleet library catalog

---

## Overview

**Goal (testable):** A workspace admin holding `library:write` lists the entries their workspace onboarded and removes one; the entry leaves the gallery permanently, fleets installed from it keep running, and the same bundle can be onboarded again afterwards.

**Problem:** Onboarding a Fleet library into a workspace is a one-way door. `agentsfleet library add` and the dashboard's Add dialog write a row into `core.tenant_fleet_library`, and nothing removes it. The entry shows as an install card on `/w/{ws}/fleets/new` forever, mixed in with the platform catalogue, and no page lists what the workspace actually owns. Observed in the fixture workspace: six gallery cards against one published platform row in `/admin/fleet-libraries` — the other five are tenant rows left by automated tests, three of them near-identical `github-pr-reviewer` entries. The pile-up is not cosmetic: the gallery orders `created_at DESC` and pages behind "Load more", so a seeded card sinks off page one and five acceptance specs time out waiting for its Install button.

**Solution summary:** Add the removal half of the workspace tier. `GET /v1/workspaces/{workspace_id}/library-entries` is the workspace's own collection and `DELETE …/library-entries/{entry_id}` removes one row, every statement scoped by `workspace_id`. Two collections rather than one filtered read, because that is what the models domain already does with the same two-tier problem (§0b). The dashboard grows a page at `/w/{workspaceId}/library` listing only what this workspace owns, with the removal action; the command-line interface (CLI) grows `agentsfleet library list` and `agentsfleet library remove`. The acceptance suite gets a per-run library sweep in its existing global teardown, which unblocks the five failing specs. Fleets installed from an entry are untouched: install copies the bundle into the fleet row and nothing points back.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api,cli,app): a workspace removes the Fleet libraries it onboarded
- **Intent (one sentence):** An operator who onboards a Fleet library into their workspace can see what they own and take it back out, instead of living with it forever among the platform entries.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch with the Intent above → STOP and reconcile before any edit.

## §0 — The reversal this spec opens with

`schema/460_tenant_fleet_library.sql:49` records a decision this spec reverses: *"No DELETE: an onboarded entry is retired by visibility, and the rows leave with their workspace through the cascade above."* **The decision is hard DELETE** — not retire-by-visibility, not both. Four reasons, each read from source on `main`:

1. **The retire path was never built, and building it costs a second reversal.** A tenant row's `visibility` is always the single value `"tenant"` — `VISIBILITY_TENANT` at `catalogue/import/tenant.rs:46`, bound unconditionally at `:76`, asserted at `:187`. The gallery's `tenant_arm!` (`catalogue/gallery/sql.rs`) filters on `workspace_id = $2::uuid` and reads no `visibility` predicate at all. The publish gate the comment borrows its language from is the *platform* tier's `{draft, public}` (`docs/architecture/fleet_bundles.md:16`, `:84`), which the tenant tier does not participate in. "Retired by visibility" is not a mechanism left unwired; it is one that would have to be invented, in the read path, to honour a sentence.
2. **An unlisted row is a permanent tombstone.** `UNIQUE (workspace_id, content_hash)` (slot 460:38) plus `ON CONFLICT … DO NOTHING` (`INSERT_OR_EXISTING`, `import/tenant.rs`) means a second onboard of the same bytes changes nothing and answers the existing identifier. Retire an entry and that workspace can never onboard that bundle again — the re-onboard converges onto the retired row and leaves it retired. Fixing that means `DO UPDATE`, reversing a second deliberate decision documented in the same file ("the second call CHANGES NOTHING rather than refreshing the row"). One reversal is a decision; two to preserve the wording of the first is ceremony. It also breaks the acceptance suite, whose whole shape is re-onboarding one stable name per run.
3. **Delete cannot disturb an installed fleet.** `SELECT_TENANT_INSTALL` (`afd_fleet_lifecycle/src/sql/install.rs:26-29`) reads `skill_markdown, trigger_markdown, content_hash` out of the row at install time and the fleet carries its own copy. `LibrarySource` (`install.rs`) is the only direction the two connect in, consumed at install and not retained. `grep -rn "tenant_fleet_library" schema/` outside slot 460 returns one comment in `schema/450_fleet_library.sql:17` and no foreign key. §4 makes that a test rather than a claim.
4. **The grant, not the comment, is the real gate.** Slot 460:51 grants `SELECT, INSERT, UPDATE` to `api_runtime` and stops — that is what refuses a DELETE today. (The `UPDATE` in it is already unexercised: `grep -rn "tenant_fleet_library" rustd/crates/` finds four statements, all `INSERT` or `SELECT`, so the entry is immutable after onboarding and no edit can reach an installed fleet — see §4.) Lifting it is one additive slot with prior art: `schema/551_connector_installs_delete_privilege.sql` is a GRANT-DELETE-only file whose header argues this kind of reversal. `cat VERSION` is `0.49.0`, above RULE SCH's `0.30.0` anchor, so slot 460 stays frozen history — its comment included — and the superseding decision lands in the new slot's header and in the architecture doc.

**Why not (b) retire by visibility:** reasons 1 and 2 — it preserves the sentence and breaks re-onboarding. **Why not (c) both:** a second verb, a second state and a second test set bought for a use nobody has named, and whatever it unlists inherits the tombstone of reason 2.

## §0b — The shape the models domain already settled

`core.model_library` (platform, admin-curated) and `core.tenant_model_entries` (a tenant's own registry) are the same two-tier problem, solved and shipped. Three rules come out of it, read from `rustd/crates/afd_http/src/route/tenant.rs:112-128` and the Models page:

1. **Two collections, never a tier filter.** `ModelLibrary` is `("/v1/models", Scopes::Always(NONE))`; `ModelEntries` is `("/v1/tenants/me/models", Scopes::rw(SECRET_READ, SECRET_WRITE))`; `ModelEntry` is `("/v1/tenants/me/models/{id}", Scopes::Always(SECRET_WRITE))` with verbs `[Patch, Delete]`. Separate paths, separate response shapes, separate scopes — no query parameter selects a tier anywhere in that family. This spec follows it: the merged gallery stays exactly as it is, and the workspace's own entries get their own collection.
2. **The platform tier reaches a tenant page only where it is in effect for that tenant, as one synthesized locked row.** `ModelsRegistryTable.tsx:95` computes `showDefaultRow = !hasActiveEntry || platformDefaultAvailable` and renders a single `kind: "default"` row built from the tenant response's own `platform_default` field — never from rows of the platform collection — and hides it outright when it is "neither in effect nor configurable-from-here", because "a self-managed tenant with no platform default was shown a row it cannot act on and does not need". The N-row platform catalogue is not rendered as rows at all: `ModelCatalogueProvider.tsx` loads `/v1/models` **on intent**, to feed the Add dialog's picker.
3. **A tenant-entry `DELETE` is idempotent, answers `204`, and mints no error code for a bad path segment.** `delete_tenant_model_entry` (`handler/tenant/model_entry/write.rs:182-225`) documents "deleting an id that doesn't exist (already removed, or never existed) still returns 204", and a malformed identifier goes through plain `Refusal::malformed` (`model_entry/input.rs:68-69`), exactly as `afd_http::handler::parse_fleet_id` does.

## Implementing agent — read these first

1. `rustd/crates/afd_api_tenant/src/handler/workspace_library.rs` — the module the new verb sits beside; its header states the two-layer isolation boundary the removal must repeat, and at 287 lines against a 350-line cap it is why §2 builds elsewhere. Then the precedent it copies, before the route table, the handler or the page: `rustd/crates/afd_http/src/route/tenant.rs` (the `ModelLibrary` / `ModelEntries` / `ModelEntry` rows), `rustd/crates/afd_api_tenant/src/handler/tenant/model_entry/write.rs`, and `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable.tsx` — where §0b's three rules come from.
2. `schema/551_connector_installs_delete_privilege.sql` and `schema/900_purge_delete_grants.sql` — prior art for a grant-only additive slot that reverses an earlier slot's stated position, and argues it in the header; 551 is the closer match, granting one privilege on one table. Read with `ui/packages/app/tests/e2e/acceptance/fixtures/teardown.ts`, whose `sweepLeakedFixtureFleets` is the sweep-by-ownership pattern §5 mirrors, destructive-target guard included.
3. `docs/REST_API_DESIGN_GUIDELINES.md` — §1 URL design, §2 method semantics (DELETE idempotent, 204 on already-deleted), §3 filtering, §5 error registry, §6 OpenAPI, §7 route registration. Read with `docs/RUST_ERROR_STANDARD.md`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/917_tenant_fleet_library_delete_grant.sql` (CREATE) · `rustd/crates/afd_db/src/migration.rs` · `rustd/crates/afd_http/src/route/workspace.rs` · `rustd/crates/afd_api/tests/{route_inventory,route_meta_total}.rs` · `rustd/crates/afd_db/tests/{db_suite,migration_slot_grants,integration_ledger_key}.rs` (EDIT) | CREATE / EDIT | The slot grants `DELETE` to `api_runtime` and its header records the reversal of slot 460's position; the migration-array entry follows it; the route table gains `LibraryEntries` and `LibraryEntry`, mirroring `ModelEntries` / `ModelEntry`. Registration has a FIFTH place the REST guide's §7 omits: `route_inventory.rs`'s `INVENTORY` array plus a `WorkspaceRoute::ALL.len()` count, and `route_meta_total.rs`'s `POST_PORT_ADDITIONS`. The `afd_db` test edits are the migration-slot assertions the new slot moves, and an M203 fixture that pinned the upgrade to exactly `[916]`. |
| `rustd/crates/afd_library/src/catalogue/entries.rs` · `catalogue/entries/sql.rs` (CREATE) · `catalogue/mod.rs` · `src/lib.rs` · `rustd/crates/afd_wire/src/workspace_library.rs` (EDIT) | CREATE / EDIT | The owned-entry list statement, the workspace-scoped delete statement and their store entry points — in their own module, because `catalogue/store.rs` is the platform tier (`Libraries` over `core.fleet_library`, its three contexts reading "platform library") and tenant statements there would blur the tier boundary. Plus the owned-entry row and its list envelope, mirroring `afd_wire/src/tenant_model_entry.rs`. The merged gallery in `catalogue/gallery.rs` and `gallery/sql.rs` is not touched. |
| `rustd/crates/afd_api_tenant/src/handler/library_entry.rs` · `library_entry/{list,remove,tests}.rs` (CREATE) · `src/{lib.rs,handler/mod.rs,openapi.rs}` · `rustd/crates/afd_observability/src/metrics/label/library.rs` · `docs/metrics.census.tsv` · `docs/architecture/observability.md` (EDIT) | CREATE / EDIT | The owned-entry collection and its single-entry removal, in their own module rather than inside `workspace_library.rs` — that module is at 287 lines against a 350 cap and serves a different collection. The three edits are the wiring the routes do not exist without: the dispatch arm at `lib.rs:89`, the module declaration at `handler/mod.rs:16`, and the utoipa `paths(...)` registration at `openapi.rs:91`. The new `workspace_entries` surface label is a census change: the label set is closed, so its ceilings in `docs/metrics.census.tsv` and its allowed values in `observability.md` move with it. |
| `rustd/crates/afd_library/tests/integration_catalogue.rs` · `integration_catalogue/entry_privileges.rs` · `rustd/crates/afd_api/tests/integration_workspace_library_entries.rs` · `workspace_library_entries_live/{fixture,events}.rs` · `fleet_lifecycle_live/{fixture,library_removal}.rs` (CREATE) · `rustd/crates/afd_api/tests/integration_fleet_lifecycle.rs` · `harness/{mod,support}.rs` · `tenant_plane_suite.rs` · `rustd/crates/afd_core/tests/error_code.rs` · `docs/architecture/web_app.md` · `playbooks/operations/acceptance/baselines/M204_001-76eb9c2d3.md` · `public/openapi.json` | CREATE / EDIT | Removal, cross-workspace invisibility, re-onboarding and the untouched gallery; the proof that removal leaves an installed fleet running; and the two new operations. `integration_fleet_lifecycle.rs` sat at the 350-line cap, so its fixture splits to a sibling before anything is added; the owned collection's own live suite is new, and the harness binds the refusal envelope's field to one name so two suites cannot compare two absent keys. `error_code.rs` gains the audit's own `intentional-fake` marker on the four-digit fixture it asserts is refused; `web_app.md`'s `useOptimistic` row counts the new page. The document is the `openapi` subcommand's stdout redirected (`agentsfleetd/src/cli.rs:241`), regenerated rather than hand-edited. |
| `cli/src/commands/fleet_library_list.ts` · `fleet_library_remove.ts` (CREATE) · `cli/src/program/tree/fleet.command.ts` · `cli/src/program/tree/flags.ts` · `cli/src/lib/api-paths.ts` (EDIT) · `cli/test/library-entries.unit.test.ts` (CREATE) · `cli/test/acceptance/fixtures/command-matrix.ts` (EDIT) | CREATE / EDIT | `agentsfleet library list` and `library remove`, registered under `library`, with the single-entry path builder. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/` (`page.tsx`, `loading.tsx`, `actions.ts`, `copy.ts`, `lib/reads.ts`, `actions.test.ts`, `components/WorkspaceLibraryList.tsx`) · `lib/api/{fleet-library,library-types}.ts` · `fleets/new/{LibraryCard.tsx,LibraryCard.test.tsx}` · `tests/library-entries-client.test.ts` (EDIT) | CREATE / EDIT | The workspace Fleet library page with its loading state, server action, copy as named constants (RULE UFS), owned-entry list and removal confirmation; plus the owned-collection client and its row type. It mirrors `secrets/`, whose `lib/reads.ts` split keeps the page component off the fetch — and whose list holds the design system's `ConfirmDialog` inline, so no separate `RemoveLibraryDialog.tsx` is built. The row type lands in `library-types.ts` rather than `lib/types.ts`, which is 440 lines against a 350 cap. |
| `ui/packages/app/components/layout/SidebarNavigation.tsx` · `tests/e2e/acceptance/fixtures/teardown.ts` · `tests/e2e/acceptance/global-teardown.ts` · `tests/e2e-teardown-sweep.test.ts` · `docs/architecture/fleet_bundles.md` (EDIT) · `tests/e2e/acceptance/workspace-library.spec.ts` (CREATE) | EDIT / CREATE | The navigation entry; `sweepLeakedFixtureLibraries`, mirroring the fleet sweep and called beside it; the record that the tenant tier has a collection and a removal verb and that slot 460's position is superseded; and the end-to-end walk of the new page. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (refusal sentences, tier values, page copy and command descriptions are named constants); **NDC** (the owned collection ships with both clients calling it); **ORP** (nothing renamed or removed — the sweep is the `N/A` assertion below); **FLL** (`workspace_library.rs` is 287 lines pre-diff, which is why §2 builds elsewhere); **ERR** (no code is minted; `audits/error-codes.sh` stays green, which is what proves it); **SGR** (the slot grants exactly the privilege its callers exercise); **LOG** (one scoped event, no bundle content); **MSID** (no milestone marker reaches source); **TSC**/**TSJ** (both CLI files); **UIS**/**DTK** (the dashboard page); **ITF** (integration rows run against the real schema).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** (a new public surface, the sections listed above) · **`docs/RUST_ERROR_STANDARD.md`** (a private `ErrorKind` plus `afd_core::error_shell!`, never a hand-written error type) · **`docs/SCHEMA_CONVENTIONS.md`** §Migration Model (`VERSION` is `0.49.0`, so the grant is a new numbered slot registered in `afd_db/src/migration.rs`; slot 460 is frozen) · **`docs/LOGGING_STANDARD.md`**, **`dispatch/write_ts_adhere_bun.md`**, **`docs/DOCUMENTATION_RULES.md`** and **`docs/CHANGELOG_VOICE.md`** for the event, the TypeScript surfaces and §6's prose.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| SCHEMA GUARD | yes — a new `schema/*.sql` slot | Additive grant only; no `ALTER`, no `DROP`, no edit to slot 460. Registered in the migration array in the same commit. |
| ERROR REGISTRY | no — the diff declares no new code | The two refusals are `Refusal::malformed` (§0b rule 3) and the existing `UZ-LIBRARY-006`. `bash audits/error-codes.sh` still runs and must stay green, which is what proves nothing was minted by accident. |
| LENGTH GATE | yes — `workspace_library.rs` at 287 lines | The new collection and its removal land in `handler/library_entry.rs` and its submodules, so `workspace_library.rs` is not touched at all and the cap is never approached. Re-measure both with `wc -l` before COMMIT. |
| UFS GATE · UI GATE · DESIGN TOKEN GATE | yes — refusals, copy and command descriptions; a new page and two components | Each string a named `const`, declared once and imported rather than respelled. Design-system primitives only (the list mirrors `SecretsList.tsx`, the dialog `RenameSecretDialog.tsx`); token utilities only, no arbitrary values. `bash audits/design-tokens.sh` before COMMIT. |
| LOGGING · MILESTONE-ID · PUB / FILE SHAPE | yes — the removal handler; every source file; a new Rust module | One scoped event carrying outcome and registry code, with no markdown, hash or source reference in the line; `M204` appears in the spec, changelog and Pull Request prose only; and `library_entry.rs` is declared at PLAN as operations-over-value, exposing its two handlers and their refusal constants and nothing else. |
| GREPTILE GATE · Architecture consult | yes — every EXECUTE turn; the tenant tier gains a collection and a verb | Read the rule-code gloss legend plus the sections Applicable Rules names, per `docs/EXECUTE_DOC_READS.md`; read `docs/architecture/fleet_bundles.md` before the surface is written and update it in the same Pull Request. ZIG GATE does not fire — no `*.zig` in the diff. |

## Prior-Art / Reference Implementations

- **Models (the two-tier precedent):** `route/tenant.rs` · `handler/tenant/model_entry/write.rs` · `ModelsRegistryTable.tsx` · `ModelCatalogueProvider.tsx` — §0b states the three rules this spec takes from them; every divergence is named there or in Out of Scope. Secondary: `afd_api_tenant/src/handler/secret.rs` with `Secret` in `afd_http/src/route/workspace.rs` — a workspace-scoped single-resource route carrying `Verb::Delete` under a write-only scope; the removal mirrors its route row, scope shape and refusal vocabulary. And `schema/551_connector_installs_delete_privilege.sql` — a grant-only additive slot arguing its reversal in the header and citing RULE SGR for granting exactly what callers exercise.
- **CLI:** the "7 Pillars" of command-line developer experience per `docs/TEMPLATE.md` Prior-Art, read against `cli/src/commands/fleet_library.ts`. Aligned: command → handler → errors split; handler purity (output through the `Output` service, no `console.log`, no `process.exit`); output as a service; structured `CliError`; the three-tier pyramid. Divergence: no `Idempotency-Key` — the verb is DELETE and idempotent by §2 of the REST guide.
- **UI and end-to-end:** `w/[workspaceId]/secrets/` — a workspace-scoped list of owned things with a destructive row action behind a dialog and a server action; and `sweepLeakedFixtureFleets` in `fixtures/teardown.ts`, which sweeps by ownership so it cannot fall behind the specs the way a hand-maintained prefix list did.

## Sections (implementation slices)

### §1 — The datastore may delete

The grant slot 460 withheld, plus the statement that exercises it. Nothing above works until `api_runtime` holds `DELETE`, and this is the one place §0's decision is enforced. **Implementation default:** a new slot `schema/917_tenant_fleet_library_delete_grant.sql`, because `cat VERSION` is `0.49.0` — above RULE SCH's anchor — so shipped slots are frozen and the reversal is recorded in the new header rather than by editing slot 460.

- **Dimension 1.1** — DONE — The slot is single-concern, grants `DELETE` on `core.tenant_fleet_library` to `api_runtime` and nothing else, and is registered in the migration array → Test `test_delete_grant_slot_is_registered_and_single_privilege`
- **Dimension 1.2** — DONE — A pool authenticating as a login role holding only `api_runtime` removes a row through `remove_entry`, and the same role is still refused a privilege the slot did not grant → Test `test_api_runtime_holds_delete_on_tenant_fleet_library`
- **Dimension 1.3** — DONE — The delete statement filters on both `id` and `workspace_id`, matching `SELECT_TENANT_INSTALL`'s shape, and an identifier this workspace does not own removes no row → Test `test_tenant_library_delete_statement_is_workspace_scoped`

### §2 — The tenant surface: the workspace's own collection, and its removal verb

The public endpoint half, built to §0b rule 1: a second collection, not a filter on the gallery. `LibraryEntries` and `LibraryEntry` join the workspace route table exactly as `ModelEntries` / `ModelEntry` sit in the tenant one. **Implementation default:** the pair lands in a new `handler/library_entry.rs` module rather than inside `workspace_library.rs` — that module serves a different collection and is at 287 lines against a 350 cap. **Implementation default:** onboarding stays at `POST /v1/workspaces/{ws}/fleet-libraries`; moving it onto the new collection would break a shipped endpoint and the CLI's `library add`, and a second spelling of one verb is a compatibility alias the rules forbid. Out of Scope names the follow-up. **Implementation default:** removing an entry another workspace owns answers `204`, identically to removing one already gone — every statement carries `workspace_id`, so telling them apart would need a second unscoped read whose only effect is to leak that the identifier exists somewhere. §0b rule 3 is the precedent.

- **Dimension 2.1** — DONE — The route table carries `LibraryEntries` at `workspace_path!("/library-entries")` with `[Get]` and `LibraryEntry` at `workspace_path!("/library-entries/{entry_id}")` with `[Delete]` under `Scopes::Always(LIBRARY_WRITE)` → Test `test_library_entry_routes_mirror_the_model_entry_pair`
- **Dimension 2.2** — DONE — `GET` answers this workspace's own entries and nothing else, in its own envelope, paging on its own cursor → Test `test_owned_collection_answers_only_this_workspace`
- **Dimension 2.3** — DONE — `DELETE` on an owned entry answers `204` and the row leaves both reads; a replay answers `204` again, and two simultaneous ones both succeed removing one row → Test `test_delete_is_idempotent_under_replay_and_concurrency`
- **Dimension 2.4** — DONE — An `entry_id` that is not a UUIDv7 earns `Refusal::malformed`, and a caller without `library:write` is refused before any statement runs → Test `test_delete_refusals_are_malformed_and_scoped`
- **Dimension 2.5** — DONE — The OpenAPI document carries both operations and neither collides with `delete_platform_fleet_library` → Test `test_openapi_declares_the_library_entry_operations`
- **Dimension 2.6** — DONE — The removal emits its scoped event carrying outcome and code and no bundle content → Test `test_removal_emits_a_scoped_event_without_bundle_content`
- **Dimension 2.7** — DONE — The merged gallery and the platform tier's delete are both untouched → Test `test_gallery_and_platform_delete_are_unchanged`

### §3 — The dashboard page

A page at `/w/{workspaceId}/library` listing what this workspace owns, with the removal action. **Implementation default:** platform entries do **not** appear — not read-only, not greyed — and the rule is a property of the request, not a client filter that can drift, because the page reads the owned collection. This is §0b rule 2 applied: the Models page renders a platform row only where the platform tier is *in effect for that tenant* (the default), hides even that when it is "neither in effect nor configurable-from-here", and never mirrors the N-row platform catalogue as rows. A Fleet library has no in-effect-by-default concept, so the rule yields nothing to show; the installable platform catalogue keeps its home in the gallery at `/fleets/new`, the way `/v1/models` keeps its home in the Add dialog's picker.

- **Dimension 3.1** — DONE — The page lists owned entries with name, source reference and onboarding time, and no platform entry appears even when published platform rows exist → Test `test_workspace_library_page_lists_owned_entries`
- **Dimension 3.2** — DONE — A workspace owning nothing sees an empty state naming the command that adds one; removal needs a confirmation naming the entry, and a failed removal leaves the row and surfaces the refusal → Test `test_remove_requires_confirmation_and_survives_failure`
- **Dimension 3.4** — DONE — One verbose entry does not set the height of the cards beside it: a description past three lines is clamped and credentials past three become a counted overflow chip naming them on hover, while an entry already inside both bounds renders unchanged → Test `test_a_gallery_card_is_bounded`
- **Dimension 3.3** — DONE (written and committed; its proof runs in `deploy-dev-acceptance.yml`, after a deploy to dev, since a worktree cannot stand up the acceptance stack) — The whole walk works in a browser: onboard, reach the page from the navigation, remove, and the gallery no longer offers it → Test `test_workspace_library_page_walk`

### §4 — Removal does not disturb an installed fleet

The assertion §0 reason 3 rests on, made a test. Nothing propagates in the other direction either, and that is not a gap this spec leaves: a tenant library entry is immutable once onboarded — four statements touch the table and none is an `UPDATE` — so there is no edit that could need propagating, and a fleet's own copy is edited in place through `PATCH_FLEET` (`afd_fleet_lifecycle/src/sql.rs:232`), which sets `source_markdown` and `trigger_markdown` scoped by `workspace_id`. The library is a template; the fleet is the thing you change. **Implementation default:** integration tier against the real schema, because what is being proven is the absence of a database-level relationship and a unit test cannot observe that.

- **Dimension 4.1** — DONE — A fleet installed from an entry keeps its own markdown and content hash after the entry is removed, and still starts and reaches a terminal state → Test `test_installed_fleet_survives_library_removal`
- **Dimension 4.2** — DONE — Installing from a removed identifier is refused exactly as one that never existed → Test `test_install_from_a_removed_entry_is_refused`
- **Dimension 4.3** — DONE — Re-onboarding the same bytes mints a new row with a new identifier → Test `test_reonboard_after_removal_mints_a_new_entry`

### §5 — The command-line interface, and the acceptance teardown it unblocks

Both clients of the new surface, plus the sweep this workstream exists to make possible. **Implementation default:** bare `agentsfleet library` keeps its meaning — the merged gallery `install --library` resolves against — and the new verbs are explicit; redefining the bare command would change a shipped command's meaning for every caller. **Implementation default:** the sweep runs in the existing `global-teardown.ts` beside `sweepLeakedFixtureFleets`, by ownership rather than name prefix, behind the same destructive-target guard, because a hand-maintained name list is the thing `teardown.ts` already records falling behind the specs.

- **Dimension 5.1** — DONE — `library list` prints owned entries, `--format json` prints them as data, an empty workspace gets a state naming `library add`, and bare `agentsfleet library` still issues the merged-gallery request unchanged → Test `test_library_list_prints_owned_entries_or_an_empty_state`
- **Dimension 5.2** — DONE — `library remove <entry_id>` removes the entry, reports it, succeeds again on replay, and on a malformed identifier exits non-zero with the server's sentence rather than a stack trace → Test `test_library_remove_reports_success_and_is_idempotent`
- **Dimension 5.3** — DONE — The sweep clears every tenant entry in fixture-owned workspaces, nothing outside the guard, and leaves a gallery of platform rows → Test `test_library_sweep_is_bounded_and_clears_tenant_rows`
- **Dimension 5.4** — DONE (written and committed; its proof runs in `deploy-dev-acceptance.yml`, after a deploy to dev, since a worktree cannot stand up the acceptance stack) — The real binary walks add → list → remove → list in a subprocess, and after a swept run `installViaUI` finds the seeded card on page one without a "Load more" click → Test `test_library_remove_subprocess_walk`

### §6 — Documentation

The public surface changes, so the docs branch is part of the work. **Implementation default:** `~/Projects/docs` work happens on its own branch `chore/m204-workspace-library-removal-changelog`, cut from `main` there and never edited through this worktree.

- **Dimension 6.1** — DONE — The architecture doc records the removal verb and that slot 460's position is superseded, and the diff carries one new changelog entry → Test `test_architecture_and_changelog_record_the_removal`
- **Dimension 6.2** — DONE — The four `~/Projects/docs` pages are revised on their own branch → Test `test_docs_branch_carries_the_four_pages`

## Interfaces

```
GET /v1/workspaces/{workspace_id}/library-entries
  Route row: LibraryEntries, verbs [Get],
    Scopes::rw(FLEET_READ, LIBRARY_WRITE)
  operation_id: list_workspace_library_entries
  200 -> { entries: [...], next_cursor }. This workspace's own
    onboarded entries and nothing else: name, source_kind,
    source_ref, content_hash, created_at. Its own envelope and its
    own cursor — mirroring GET /v1/tenants/me/models, which is a
    different shape from GET /v1/models on purpose.

DELETE /v1/workspaces/{workspace_id}/library-entries/{entry_id}
  Route row: LibraryEntry, verbs [Delete],
    Scopes::Always(LIBRARY_WRITE) — no read half, as ModelEntry.
  operation_id: delete_workspace_library_entry, distinct from the
    platform tier's delete_platform_fleet_library
    (afd_api_operator/src/handler/admin/libraries.rs)
  204 -> the entry is gone, or was never this workspace's. One
    response on purpose: every statement carries workspace_id, so
    separating them needs an unscoped read whose only effect is to
    leak that the identifier exists somewhere. Idempotent, per
    delete_tenant_model_entry's own documented behaviour.
  400 -> Refusal::malformed for an entry_id that is not a UUIDv7,
    as model_entry/input.rs and parse_fleet_id both do.
  401 / 403 the standard auth and scope refusals · 500
    UZ-LIBRARY-006 a transient statement failure.
  No body on success. No soft-delete marker, no visibility flip.

Error registry: NO new code. The two refusals this surface adds
  are a malformed path segment (Refusal::malformed, the shape
  every other path identifier in this daemon uses) and a transient
  statement failure (the existing UZ-LIBRARY-006).

UNCHANGED: GET and POST /v1/workspaces/{ws}/fleet-libraries — the
  merged gallery and the onboarding verb, byte-identical. It is
  the install picker, the analogue of /v1/models feeding the Add
  dialog, and this spec does not touch it.

CLI: agentsfleet library (unchanged — merged gallery) · library
  list · library add (unchanged) · library remove <entry_id>
Dashboard: /w/{workspaceId}/library — owned entries and removal.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Malformed identifier | `entry_id` is not a UUIDv7 | Refused before any statement by `Refusal::malformed`, the shape `model_entry/input.rs` and `parse_fleet_id` both use. No family code is minted. A cursor the owned collection did not issue, or one issued for a different page size, earns the existing `UZ-LIBRARY-001` / `UZ-LIBRARY-002` the gallery walk already answers. |
| Foreign entry | The identifier names another workspace's entry | The scoped statement matches nothing; `204`, indistinguishable from already-removed. Nothing in the response or log confirms the identifier exists. |
| Already removed, or missing scope, or not this workspace | The `DELETE` is replayed; or the caller lacks `library:write`; or does not own the workspace in the path | A replay answers `204` again with no row change, idempotent per REST guide §2. The other two answer `403`, from the route's scope check or from `WorkspaceContext`, before the handler body runs. |
| Grant missing, or datastore unreachable | Migrated without slot 917; or the pool is exhausted or the connection lost mid-statement | Refused by the datastore; `500` `UZ-LIBRARY-006`, with the failing statement's context in the scoped log, and no partial state — one row, one statement. Dimension 1.2 keeps a migrated database out of the first case. |
| Removal races an install, or the sweep over-reaches | An install resolves the entry as the removal commits; or the acceptance sweep is pointed outside the fixture set | Either install order is correct — it has already copied the bundle (§4), or resolves nothing and is refused as an unknown identifier, with no torn state. The sweep's destructive-target guard refuses, nothing is removed, and the teardown logs it without throwing. |

## Invariants

1. **Every statement against `core.tenant_fleet_library` in this diff carries a `workspace_id` predicate** — enforced by a unit test asserting the substring on each statement constant (Dimension 1.3), so a statement added later without it fails that test; and **a fleet's bundle never depends on a library row**, enforced by the absence of a foreign key plus Dimension 4.1 against the real schema — asserted, not assumed; and **`api_runtime` holds exactly the privileges its callers exercise on this table**, Dimensions 1.1 and 1.2 granting `DELETE` and nothing else and reading the privilege back from the integration lane.
4. **No new error code is introduced**, which `bash audits/error-codes.sh` confirms by finding nothing new to check; **`DELETE` is idempotent**, Dimension 2.3 issuing the same request twice and asserting identical status and row state; **the merged gallery is unchanged**, Dimension 2.7 plus the existing `workspace_gallery.rs` cases which must stay passing unamended; and **removal is permanent** — no soft-delete column, marker row or visibility flip, enforced by Dimension 4.3, since a re-onboard can only mint a new identifier if the old row is gone.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `library.remove` | ops | The removal handler finishes, on every path including refusals | workspace identifier, library identifier, outcome, registry code on refusal, duration | No skill or trigger markdown, no support-file manifest, no content hash, no source reference, no raw email/token/key material | `test_removal_emits_a_scoped_event_without_bundle_content` |
| `library.read` (existing) | ops | The owned-collection read reports through the existing library read-outcome family, as the gallery walk does | The surface label distinguishing the owned read from the gallery; nothing else changes | Unchanged | `test_owned_collection_answers_only_this_workspace` |

**Analytics/funnel:** the removal is operator maintenance, not a step in the install funnel, and adds no product analytics event. No analytics/funnel playbook update is required; Discovery records the reason.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_delete_grant_slot_is_registered_and_single_privilege` | The slot text holds exactly one `GRANT`, naming `DELETE` and `core.tenant_fleet_library`, and no `ALTER` or `DROP`; the migration array contains the filename and `version_of` derives 917. |
| 1.2 | integration | `test_api_runtime_holds_delete_on_tenant_fleet_library` | A pool authenticating as a probe login role holding only `api_runtime` removes a seeded row through `remove_entry` → `true`, and the row is gone; the same role's `TRUNCATE` on the table is refused. Asserted by running the statements, never by reading `has_table_privilege` — `integration_purge_privileges.rs` records why the two disagree. |
| 1.3 | unit | `test_tenant_library_delete_statement_is_workspace_scoped` | The statement constant contains both `id = $1` and `workspace_id = $2`; and against the live schema, workspace A deleting B's identifier affects 0 rows while B still reads its own. |
| 2.1 | unit | `test_library_entry_routes_mirror_the_model_entry_pair` | `LibraryEntries` → template `/v1/workspaces/{workspace_id}/library-entries`, verbs `[Get]`; `LibraryEntry` → `…/library-entries/{entry_id}`, verbs `[Delete]`, scope `Always(LIBRARY_WRITE)`. |
| 2.2 | integration | `test_owned_collection_answers_only_this_workspace` | Workspace A with 1 own entry, workspace B with 2, against 2 published platform rows: A's `GET` → exactly 1 item, the onboarded identifier, no platform row and nothing of B's; a cursor this collection did not issue → 400 `UZ-LIBRARY-001`. |
| 2.3 | integration | `test_delete_is_idempotent_under_replay_and_concurrency` | Onboard, `DELETE` → 204, empty body, and the follow-up owned read omits the entry; the same `DELETE` twice → 204 both times, row count unchanged after the second; two simultaneous `DELETE`s → both 204, one row removed, no error in either. |
| 2.4 | unit | `test_delete_refusals_are_malformed_and_scoped` | `entry_id` `"not-a-uuid"` → 400 through `Refusal::malformed` and no `UZ-LIBRARY-*` code in the body; a `fleet:read`-only token → 403 and the row still exists. |
| 2.5 | unit | `test_openapi_declares_the_library_entry_operations` | `list_workspace_library_entries` and `delete_workspace_library_entry` each appear once; `delete_platform_fleet_library` still appears once. |
| 2.6 | unit | `test_removal_emits_a_scoped_event_without_bundle_content` | The event carries outcome and code; its serialised form contains none of `skill_markdown`, `trigger_markdown`, `content_hash`, `source_ref`. |
| 2.7 | integration | `test_gallery_and_platform_delete_are_unchanged` | `GET /v1/workspaces/{ws}/fleet-libraries` is item-for-item identical to the pre-diff gallery over the same fixture, and `POST` to it still onboards; `delete_platform_fleet_library` still requires `PLATFORM_LIBRARY_WRITE` and still refuses a published row with `UZ-CATALOG-003`. |
| 3.1 | unit | `test_workspace_library_page_lists_owned_entries` | Two owned entries in the fixture → two rows, each with name, source reference and onboarding time; a platform item cannot appear, because the page reads the owned collection and the issued request was to `/library-entries`. |
| 3.2 | unit | `test_remove_requires_confirmation_and_survives_failure` | An empty response → the empty state renders and its text contains `library add`; the row action opens a dialog naming the entry and dismissing it issues no request; an action rejecting with a 500 envelope leaves the row rendered and the refusal sentence visible. |
| 3.4 | unit | `test_a_gallery_card_is_bounded` | A five-sentence, five-credential entry → the description carries `line-clamp-3`, exactly three `requires:` chips render, and a `+2` chip's `title` is `jira, slack`; a one-credential entry → one chip and no overflow chip; no credentials → no chip row. |
| 3.3 | e2e | `test_workspace_library_page_walk` | `workspace-library.spec.ts`: sign in, onboard, reach `/w/{ws}/library` from the navigation, remove, assert the row is gone and `/fleets/new` no longer offers it. |
| 4.1 | integration | `test_installed_fleet_survives_library_removal` | Install from an entry, remove it → the fleet row's `skill_markdown` and `content_hash` are byte-identical to before, and the fleet still starts and reaches a terminal state. |
| 4.2 | integration | `test_install_from_a_removed_entry_is_refused` | Install by the removed identifier → the refusal an identifier that never existed earns; no fleet row created. |
| 4.3 | integration | `test_reonboard_after_removal_mints_a_new_entry` | Onboard bytes B, remove, onboard B again → a row exists, its identifier differs from the first, and it appears in the gallery. |
| 5.1 | unit | `test_library_list_prints_owned_entries_or_an_empty_state` | A two-item fixture → two table rows; `--format json` → `{ items: [...] }` with two entries and no table; an empty response → the empty-state sentence, the `library add` hint, exit 0; bare `agentsfleet library` issues the merged-gallery request, as before. |
| 5.2 | unit | `test_library_remove_reports_success_and_is_idempotent` | A 204 response → a success line naming the identifier, exit 0; two consecutive 204s → exit 0 both times, identical output; a 400 malformed response → non-zero exit, the server's detail on the error stream, no stack trace. |
| 5.3 | integration | `test_library_sweep_is_bounded_and_clears_tenant_rows` | Pointed at a non-fixture workspace the sweep removes 0 rows and logs the refusal; at a fixture workspace it removes every tenant entry across every page — `test_library_sweep_drains_every_page_not_just_the_first` pins the drain, and after `bun global-teardown.ts` that workspace's owned collection is empty. |
| 5.4 | e2e | `test_library_remove_subprocess_walk` | The real binary in a subprocess: `library add`, `library list` shows it, `library remove`, `library list` no longer shows it; and after a swept run `installViaUI` locates the seeded card's Install button with no "Load more" click. |
| 6.1 | unit | `test_architecture_and_changelog_record_the_removal` | `docs/architecture/fleet_bundles.md` contains `delete_workspace_library_entry` and a sentence superseding slot 460's position; the diff against the comparison revision adds exactly one new `<Update>` block. |
| 6.2 | manual | `test_docs_branch_carries_the_four_pages` | Procedure: in `~/Projects/docs` on `chore/m204-workspace-library-removal-changelog`, `git diff --name-only main` lists `fleets/library.mdx`, `cli/agentsfleet.mdx`, `api-reference/error-codes.mdx`, `changelog.mdx`. Required person: Indy, who owns that repository's branch. Evidence: the branch name and command output in Pull Request Session Notes. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | `api_runtime` may delete a tenant library row, and only that (§1) | `grep -c GRANT schema/917_tenant_fleet_library_delete_grant.sql; grep -cE 'ALTER\|DROP' schema/917_tenant_fleet_library_delete_grant.sql` | `1` then `0` | P0 | ✅ `1` then `0` |
| R2 | Both new operations exist, and the platform one is untouched (§2) | `grep -c '"operationId": "list_workspace_library_entries"\|"operationId": "delete_workspace_library_entry"' public/openapi.json; grep -c '"operationId": "delete_platform_fleet_library"' public/openapi.json` | `2` then `1` | P0 | ✅ `2` then `1` |
| R3 | The diff mints no error code and the registry stays clean (§2) | `bash audits/error-codes.sh; git diff origin/main...HEAD -- rustd/crates/afd_core/src/error_code/ \| grep -c '^+.*ErrorCode::declare'` | exit 0, then `0` | P0 | ✅ `OK: ERROR REGISTRY GATE: clean`, then `0` |
| R4 | Every tenant-library statement in the diff is workspace-scoped (§1, §2) | `cargo test -p afd_library -p afd_api_tenant workspace_scoped` | exit 0 | P0 | ✅ `test_tenant_library_delete_statement_is_workspace_scoped ... ok` |
| R5 | Removing an entry leaves a fleet installed from it running (§4) | `cargo test -p afd_api --test tenant_plane survives_library_removal -- --ignored` | exit 0 | P0 | ✅ `test_installed_fleet_survives_library_removal ... ok` |
| R6 | The workspace page lists only what this workspace owns (§3) | `cd ui/packages/app && bun run test -- library/` | exit 0 | P0 | ✅ `Test Files 6 passed (6) · Tests 49 passed (49)` |
| R7 | Both new command verbs work against the real binary (§5) | `cd ui/packages/app && bun run test:e2e:acceptance -- workspace-library` — the suite runs in `.github/workflows/deploy-dev-acceptance.yml`, which fires after a deploy to dev rather than on the Pull Request | exit 0 | P0 | ⏳ grades post-deploy — needs the acceptance stack and fixture users provisioned against Clerk; not runnable in a worktree |
| R8 | The acceptance suite finds its seeded card on page one after a swept run (§5) | `cd ui/packages/app && bun run test:e2e:acceptance -- fleet-count` — same workflow, same constraint | exit 0 | P0 | ⏳ grades post-deploy — same constraint as R7 |
| R9 | The unfiltered gallery is unchanged (§2, Invariant 4) | `cargo test -p afd_library --test integration_catalogue gallery_merges -- --ignored` | exit 0 | P0 | ✅ `the_gallery_merges_two_libraries_under_one_order_and_walks_it_by_seek ... ok` |
| R10 | The docs branch carries the four revised pages (§6) | `git -C ~/Projects/docs diff --name-only main...chore/m204-workspace-library-removal-changelog` | 4 paths: `fleets/library.mdx`, `cli/agentsfleet.mdx`, `api-reference/error-codes.mdx`, `changelog.mdx` | P1 | ✅ the four paths, exactly |
| R11 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ `0` paths missing, after the table was reconciled |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ `ALL GATES GREEN` |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `exit=0` — 2662 Rust passed / 0 failed; TypeScript coverage floor 100% on all four metrics |
| S3a | Lint green | `make lint-all` | exit 0 | P0 | ✅ `exit=0` |
| S3b | Integration lane green (live Postgres and Dragonfly) | `make test-integration-rustd` | exit 0 | P0 | ✅ `exit=0` — 8 new cases, the grant probe and both ledger-key cases all `ok` |
| S3c | Version sync | `make check-version` | exit 0 | P0 | ✅ `exit=0` |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found`, 5799 commits scanned |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ `0` paths missing, after the table was reconciled |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | ✅ `git diff --name-status --diff-filter=DR origin/main...HEAD` prints nothing — the diff deletes and renames no file |

**Command source rule:** every declared `conform` and `verify.*` invocation from `.oracle/orly.json` appears verbatim above with an Expected value. **Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ plus one decisive output line. Repository-command rows point at the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 whose scope moves whole into a named successor spec is marked `MOVED to M{N}_{NNN} R{n}` — never ✅ — and only when that spec exists carrying the criterion as its own P0, both specs record the mapping, and Discovery holds the owner's verbatim quote authorising it.

## Dead Code Sweep

N/A — no files deleted. Nothing is renamed or removed: the grant is additive, the owned collection and the removal verb are new paths, the merged gallery is untouched, and both command verbs sit beside `library` and `library add` rather than replacing them. The one thing superseded is a sentence in frozen migration history (`schema/460_tenant_fleet_library.sql:49`), which RULE SCH forbids editing at this version; Dimension 6.1 keeps it from being the last word a reader finds.

## Out of Scope

- **The platform tier, and the gallery's search box, sort and paging.** `/admin/fleet-libraries`, `delete_platform_fleet_library` and `PLATFORM_LIBRARY_WRITE` stay admin-only and untouched; Dimension 2.7 proves it. `FleetWall.test.tsx`'s `test_wall_header_has_no_search` asserts the search box's absence deliberately, and `created_at DESC` behind "Load more" stays — the pile-up is why their shape hurt, so removing the rows is the fix at the source and re-ordering the wall treats the symptom. **In scope by owner direction (Indy, Sep 21, 2026, from a screenshot): the gallery CARD's own bounds**, which are a different defect from the wall's order — one verbose entry set the height of every card in its row. Dimension 3.4 bounds it.
- **Unlisting without removing, bulk removal, and entry editing.** Unlisting is rejected in §0 as option (c); if a "keep the provenance, hide the card" need is ever named, it is a follow-up spec with that need written down. One entry per request — the REST guide §3 has a bulk convention if a caller ever needs one, and none does. Renaming or re-pointing stays impossible and `INSERT_OR_EXISTING`'s "changes nothing" decision stands; remove and re-onboard is the path, which Dimension 4.3 proves works.
- **Moving the onboarding verb onto the new collection.** `POST /v1/workspaces/{ws}/fleet-libraries` stays where it is, so create and remove sit on different paths — the one place this spec diverges from §0b rule 1's shape. Moving it breaks a shipped endpoint and the CLI's `library add`, and running both spellings at once is a compatibility alias the rules forbid. A follow-up spec makes the move with its migration, or records that the split is fine.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator opens their workspace's Fleet library page, sees the three near-identical `github-pr-reviewer` entries a test run left behind, removes two, and the install gallery goes back to the one bundle they meant to keep plus the platform catalogue.
2. **Preserved user behaviour** — `agentsfleet library` still prints the merged gallery; `library add` is unchanged; `install --library <id>` resolves against the same list; the gallery endpoint answers exactly what it answers today and still onboards on `POST`; the admin page and platform delete are untouched. Breaking any of these is a redesign, not this spec.
3. **Optimal-way check** — The most direct route to moment #1 is a list of what you own and a button that removes one, which is what this builds. The gap to the unconstrained-optimal shape: entries are still discovered after the fact rather than never accumulating, since the source is onboarding without cleanup. §5 closes that for our own suite, not for a careless customer. Acceptable now, because a removal verb is the prerequisite for any cleanup anyone could build, theirs included.
4. **Rebuild-vs-iterate** — Iterate. The tenant tier has identity, a domain key, workspace-scoped reads and a working install path; it is missing one verb. A rebuild would invent a lifecycle state machine for a table whose whole problem is that a row cannot leave it.
5. **What we build** — A grant slot; an owned-entry collection and a `DELETE` endpoint; one dashboard page with a list and a removal dialog; two command verbs; a teardown sweep; four docs pages and a changelog entry.
6. **What we do NOT build** — Unlisting (option (c)); bulk removal; entry editing; a gallery search box; any platform-tier change. Each is one line in Out of Scope with its reason.
7. **Fit with existing features** — Compounds with `library add` (M199 made it reachable from a terminal; this makes it reversible) and with the install flow, the reader of the gallery this cleans up. It must not destabilise install: a fleet already installed has to keep running, which §4 makes four tests rather than an assurance.
8. **Surface order** — Both, in one Pull Request. The repository default is command-line-first and the verbs are the smaller surface, but the observed problem is a wall of cards in a browser, and shipping the page a milestone later leaves the person who hit it without the fix. The endpoint lands first inside the stream; the clients follow.
9. **Dashboard restraint** — The page shows entries and one action. No counters, no "N fleets installed from this" badge, because nothing counts that today and inventing a number to fill the column is the failure this rule names. No bulk-select toolbar until someone has removed enough entries one at a time to ask for one.
10. **Confused-user next step** — Can't find what they onboarded: `agentsfleet library list`, which prints identifiers and names the page. Tried to remove something not theirs: they get `204` and an unchanged list, so the next move is `library list` to see what is theirs. Removed the wrong entry: `agentsfleet library add` with the same source, and Dimension 4.3 is the proof that works.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Six Sections in one Workstream, ordered by what cannot be observed without the thing under it — the grant (§1) before the endpoint (§2), the endpoint before its clients (§3, §5). §4 stands alone because it proves an absence, and that proof is what licenses the removal at all. §6 is its own Section because the docs repository branch is separate work in a separate repository, not a commit tail.
- **Alternatives considered:** (i) *The larger refactor* — give tenant entries a real lifecycle (`active` / `retired` / `removed`) matching the platform publish gate. Rejected: that gate exists because a platform draft must be invisible and uninstallable to every tenant, a problem the tenant tier does not have — its rows are already invisible outside their workspace by the `workspace_id` predicate. A state machine to express one state invents the problem. (ii) *The smaller patch* — a sweep in the acceptance teardown alone, reaching the datastore directly, with no public surface. Rejected: it fixes our suite and leaves every operator where they are, which is the reported problem; and a test harness reaching past the API into a schema it does not own is a second thing to maintain.
- **Patch-vs-refactor verdict:** this is a **patch** because the tier is missing one verb, not a design. Its one structural change — a `handler/library_entry.rs` module of its own — is the shape `ModelEntries` / `ModelEntry` already ship in, not an invention.

## Discovery (consult log)

- **Owner decisions** — > Indy (2026-09-21): "yes i accept a hard DELETE of a fleet library as its is disconnected from a fleet." — context: §0's choice of option (a) over (b) retire-by-visibility and (c) both. Accepted; §0 stands as the decision, not a recommendation. > Indy (2026-09-21), from a screenshot of the install gallery: "on the Dashboard, anything we could do to get rid of this dis propotional fleet library entry?" — asked which of three fixes; chose "Clamp in the card" over shortening the seeded copy and over letting cards size to content. Dimension 3.4 is that decision; Out of Scope records the widening.
- **Consults** — *Architecture:* `docs/architecture/fleet_bundles.md` §Two-tier Fleet library catalog, read at authoring on `main` at `555154672`; it confirms `visibility ∈ {draft, public}` is the **platform** publish gate (`:16`, `:84`), the finding §0 reason 1 rests on. Updating it is Dimension 6.1. *Legacy-Design:* `schema/460_tenant_fleet_library.sql:49` records "No DELETE: an onboarded entry is retired by visibility". §0 argues the reversal from four source-read facts and is the record of that consult; it is an agent recommendation, not an owner decision, and Indy's acceptance of option (a) over (b) and (c) is the one decision this spec asks for before CHORE(open). *Prior-art (models):* `route/tenant.rs:112-128`, `handler/tenant/model_entry/write.rs:182-225`, `model_entry/input.rs:68-69` and `ModelsRegistryTable.tsx:95` read at Indy's direction after the first draft proposed a `?tier=` filter. §0b records the three rules; the draft's tier parameter and its new `UZ-LIBRARY-007` code were both withdrawn as divergences from a shipped shape with no reason behind them. *Gate-flag triage:* none fired at authoring — no file was edited.
- **Metrics review** — One new operational event, `library.remove`, declared with its privacy guard and test proof. No product analytics event is added: the removal is operator maintenance, not a funnel step, so no analytics/funnel playbook update is required.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review` and `orly-babysit-prs` pending, in the order CHORE(close) sets. **Deferrals** — none. Nothing in this spec is deferred; Out of Scope names what was never in it, with a reason for each.
