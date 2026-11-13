# M249_003: Plugin vocabulary

**Prototype:** v2.0.0
**Milestone:** M249
**Workstream:** 003
**Date:** Nov 12, 2026
**Status:** PENDING
**Priority:** P1, one public vocabulary across API, CLI, UI and docs
**Categories:** API, CLI, DOCS, UI
**Batch:** B3, after M249_002
**Depends on:** M249_001 (UI refresh) and M249_002 (CLI grouping) merged; this branch rebases onto both before editing UI callers
**Provenance:** Split from the authored M249_002 on Nov 12, 2026 at Indy's direction; on Nov 13, 2026 the CLI vocabulary renames, Plugin readers and `fleet delete` composition moved here from M249_002 after EGM review (see Discovery)
**Canonical architecture:** `docs/architecture/api_resources.md`; `docs/architecture/context_lifecycle.md`

## Overview

**Goal (testable):** Every public route, payload, permission, status, CLI path, UI adapter, OpenAPI document and public doc names Plugins, and no removed spelling serves a request. The fleet lifecycle (archive, delete, no stop or resume) ships in M249_002 §6.

**Problem:** The API still says `fleet-libraries`, `library_id` and `library:write`, while the approved interface says Plugins. A platform-wide library catalogue (`/v1/admin/fleet-libraries`, `platform-library:write`, `authoring.library_entries.platform_slug`) exists that Flow 1 does not use.

**Solution summary:** Rename the routes, wire types and permission in one coordinated change. Remove the platform catalogue and the redundant `/owned` collection. Edit the fresh-development schema files in place and regenerate OpenAPI. Point the CLI and the refreshed UI's adapters at the renamed routes. Indy permits resetting the development database; there is no alias, backfill, dual read or guarded cutover.

## PR Intent & comprehension handshake

- **PR title (eventual):** Rename Fleet libraries to Plugins
- **Intent:** One vocabulary, the same authority and the same immutable Flow 1 semantics.
- **Implementer handshake:** Before editing, record the fetched base, the M249_001 and M249_002 merge commits, and the production/test split of every token in the blast inventory below.

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_http/src/route/workspace.rs` and `route/admin.rs`: the library routes.
2. `rustd/crates/daemon/afd_auth/src/scope.rs`: `LibraryWrite`, `PlatformLibraryWrite`, `HIERARCHY` and the `KNOWN_BITS` assertion a removed variant changes.
3. `rustd/crates/daemon/afd_api_tenant/src/handler/library/` and `afd_api_operator/src/handler/admin/libraries.rs`, `library_stage.rs`: the handlers.
5. `schema/20261108121700_authoring_library_entries.sql`, `schema/20261108121800_authoring_library_revisions.sql`: `platform_slug` and the library tables.
6. `docs/RUST_ERROR_STANDARD.md` before touching a fallible signature; `docs/DESIGN_SYSTEM.md` and the merged M249_001 spec before touching UI adapters.

## Files Changed (blast radius)

| File or family | Action | Why |
|---|---|---|
| This spec (`pending/` → `active/` → `done/`) | MOVE/EDIT | Grade the delivered workstream |
| `rustd/crates/daemon/afd_http/src/route/workspace.rs`, `route/admin.rs`, `route_inventory.rs`, `route_meta_total.rs` | EDIT | Plugin paths; admin catalogue and `/owned` removed; inventory and counts |
| `rustd/crates/daemon/afd_api_tenant/src/handler/library/*` | MOVE/EDIT | `handler/plugin/*` |
| `rustd/crates/daemon/afd_api_operator/src/handler/admin/libraries.rs`, `library_stage.rs` | DELETE | Platform catalogue |
| `rustd/crates/daemon/afd_authoring/src/library/**` | EDIT | Plugin names; `publish/platform.rs` DELETE |
| `rustd/crates/daemon/afd_api_wire/src/library/**` | EDIT | `plugin_id`, `plugin` source kind |
| `rustd/crates/daemon/afd_auth/src/scope.rs`, `tests/scope_catalogue.rs` | EDIT | `plugins:write`; `platform-library:write` removed |
| `rustd/crates/protocol/af_observability/src/metrics/declared/library.rs`, `product/telemetry/authoring*.rs` | EDIT | Metric and event names |
| `rustd/crates/foundation/af_core/src/error_code/**`, `problem/**` | EDIT | Hints that say library |
| `schema/20261108121700_authoring_library_entries.sql`, `20261108121800_authoring_library_revisions.sql` | EDIT | Drop `platform_slug`; Plugin names (pre-production in-place edit) |
| `rustd/crates/**/src/**/*.sql` referencing those tables | EDIT | Statement renames |
| `rustd/crates/daemon/afd_api/tests/**`, `afd_authoring/tests/**` | EDIT | New paths, names, status; removed-route negatives |
| `public/openapi.json` | GENERATE | `--no-banner`, never by hand |
| `cli/src/lib/api-paths.ts`, `cli/src/commands/library/*` → `cli/src/commands/plugins/*`, `cli/src/commands/platform/**`, `cli/src/program/tree/{plugins,chat,fleet,platform*}.command.ts`, `cli/src/commands/fleet/fleet.ts`, `cli/src/commands/chat/*`, `cli/src/errors/index.ts` | EDIT/MOVE/DELETE | Plugin vocabulary, readers, `platform fleet-library` removed |
| `cli/src/program/tree/platform-library.command.ts` | DELETE | With its `platform-library:write` leaves |
| `cli/test/scoped-discovery.unit.test.ts`, `cli/test/completions.unit.test.ts` | EDIT | Drop the `platform-library:write` cases |
| `cli/test/**`, `cli/test/acceptance/**` | EDIT | New spellings; §2b proofs |
| `ui/packages/app/lib/api/draft-library.ts` and affected adapters, types and `lib/auth/scopes.ts` | EDIT | After the M249_001 merge; layouts untouched |
| Affected docs, playbooks and changelog | EDIT | Current spellings |
| `~/Projects/docs` branch `chore/m249-plugin-vocabulary-changelog` | EDIT | Public API reference, after merge |

**Blast inventory at `4f3159269`** (`git grep -l -w -i -E <token> -- rustd cli ui schema public docs`, excluding `docs/v2/done`, `docs/v1` and changelogs): `library|libraries` 303 production / 399 test files; `library_id|library_etag|library-etag` 51 / 38; `fleet-libraries|fleet_libraries` 35 / 33; `platform-library|platform_library|platform_slug` 19 / 4. The implementer replaces these counts with the per-file list at the rebased base before editing.

## Applicable Rules

- `.orly/standards/domains/http.md`, `http-surface.md`, `http-responses.md`, `sql.md`, `auth.md`; `languages/rust.md` (ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE); `languages/typescript.md`; `tests.md`.
- LOCAL.md: new route updates `INVENTORY` and `route_meta_total.rs`; schema edits in place before the first production deploy; no `DEFAULT`/`CHECK` string literals.
- NLR / NLG: no aliases, no dual reads.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| `orly gate work`, `verify`, `pr` | Yes | Hooks, then by hand at CHORE(close) |
| `make lint-all`, `make test-unit-all`, `make check-version` | Yes | Full lanes |
| `make test-integration-rustd` | Yes | Fresh schema; real authorization |
| `make dry-app` | Yes | Setup, Plugin publication and lifecycle journeys on the refreshed UI |
| `make acceptance-local SUITE=cli` | Yes | CLI against the renamed API |
| `gitleaks detect --redact --no-banner` | Yes | No credential bytes |

## Prior-Art / Reference Implementations

- Immutable library definitions, draft source pinning and publication ETags already implement Flow 1; this renames them without replacing them.
- Existing `KNOWN_BITS` compile-time assertion in `scope.rs` proves the scope catalogue stays total after a variant is removed.

## Sections (implementation slices)

### §1 — Plugin routes, wire and permission

`/v1/workspaces/{workspace_id}/fleet-libraries[/{library_id}[/revisions[/{revision_id}]]]` becomes `/plugins[/{plugin_id}[/revisions[/{revision_id}]]]`. `library` source kind and `library_id` become `plugin` and `plugin_id`; `revision_id` stays. `library:write` becomes `plugins:write` with the same authority and narrowing. `/fleet-libraries/owned` is removed; the ordinary collection lists the account-owned entries. Definition content, pinned versions, review digests and optimistic publication checks are unchanged.

- **Dimension 1.1** — PENDING — Plugin reuse pins a full revision → Test `plugin_revision_saves_an_independent_fleet` (integration): a draft from `(plugin_id, revision_id)` saves a fleet whose definition digest equals the revision's.
- **Dimension 1.2** — PENDING — Renaming cannot broaden access → Test `plugin_permission_preserves_narrowed_authority` (integration): a key without `plugins:write` gets 403 on publish, revise and delete; a reader keeps list and show.
- **Dimension 1.3** — PENDING — Revisions stay immutable → Test `plugin_revision_and_installed_copy_do_not_follow_updates` (integration): publishing revision 2 or deleting the Plugin leaves a fleet saved from revision 1 byte-identical.

### §2 — Platform catalogue removal

Delete `/v1/admin/fleet-libraries*`, the operator handlers, `publish/platform.rs`, `Scope::PlatformLibraryWrite`, `platform_slug` and its unique index, and the `platform fleet-library` CLI family with its discovery row.

- **Dimension 2.1** — PENDING — The catalogue stays absent → Test `platform_catalogue_routes_and_commands_are_removed` (integration + CLI unit): the old admin paths return 404 under an operator key, `platform-library:write` grants nothing, and `platform fleet-library` fails parsing.

### §2b — CLI Plugin vocabulary and readers

`library` becomes `plugins` with `list [--starting-after] [--limit]` (the ordinary collection, which after §1 lists only account-owned entries), `show <plugin_id>`, `revisions <plugin_id> [--starting-after] [--limit]`, `revisions <plugin_id> <revision_id>` and `delete <plugin_id>`. `chat --library` becomes `--plugin`, `--library-etag` becomes `--plugin-etag`; `--revision`, `--publish` and `--publish-to` stay. `plugin_id` accepts a UUIDv7; `revision_id` must be one. `show` prints `name`, `id`, `head_revision_id` and `etag`; a revision prints `revision`, `canonical_digest` and `review.review_digest`; JSON passes bodies through. `api-paths.ts` gains the revision path helpers. Discovery's `platform-library:write` row and the `platform fleet-library` family are deleted with §2.

- **Dimension 2b.1** — PENDING — CLI Plugin reuse pins a revision → Test `chat_plugin_flags_pin_the_selected_revision` (unit): `chat --plugin P --revision R --no-input` sends `P` and `R` unchanged in the draft-creation body; `chat --library P` exits 4 with `UNKNOWN_OPTION`.
- **Dimension 2b.2** — PENDING — Plugin readers hit the renamed routes → Test `plugins_show_and_revisions_read_renamed_routes` (unit): `show`, `revisions` and `revisions <r>` request `/plugins/P`, `/plugins/P/revisions` and `/plugins/P/revisions/R`, print the stubbed `etag` and digests, and a 404 renders the block with its `UZ-` code and exit 3.

### §2c, §3 — moved

The fleet lifecycle (archive as a new state, delete in any status, no stop, resume or kill) moved to M249_002 §6 at Indy's direction on Nov 13, 2026 (see Discovery).

### §4 — Coordinated callers

Regenerate OpenAPI. Update CLI paths, the refreshed UI's adapters and types, docs and the public reference together. Rerun setup, Plugin publication and lifecycle journeys on the refreshed UI.

- **Dimension 4.1** — PENDING — Every caller uses the renamed surface → Test `plugin_surfaces_match_cli_ui_and_openapi` (integration + browser): create, revise, reuse and delete succeed through the CLI and the UI against the new paths, and OpenAPI lists no `fleet-libraries` path.
- **Dimension 4.2** — PENDING — One vocabulary → Test `removed_public_spellings_have_only_historical_references` (source scan): `fleet-libraries`, `library_id`, `library:write`, and `platform-library` appear only in changelog history and `docs/v2/done`.

## Interfaces

`/v1/workspaces/{workspace_id}/plugins`, `/plugins/{plugin_id}`, `/plugins/{plugin_id}/revisions`, `/plugins/{plugin_id}/revisions/{revision_id}`. Scope `plugins:write`. Account API keys stay `/v1/api-keys`; platform defaults stay `/v1/admin/platform-keys`.

## Failure Modes

| Mode | Kind | Handling | Test |
|---|---|---|---|
| Scope rename broadens access | Security | Same ladder position; explicit negatives | `plugin_permission_preserves_narrowed_authority` |
| Plugin write conflicts or access is lost | Security | ETags and scoped refusal kept; installed copies untouched | `plugin_revision_and_installed_copy_do_not_follow_updates` |
| An old route still serves | Failure | Route and inventory removal; 404 negative | `platform_catalogue_routes_and_commands_are_removed` |
| A caller misses the rename | Failure | Cross-surface caller test | `plugin_surfaces_match_cli_ui_and_openapi` |

## Invariants

1. Backend authorization is authoritative (enforced by the existing scope gate tests plus Dimension 1.2).
2. Plugins remain complete setups; revisions and installed copies never follow later edits (Dimension 1.3).
3. The scope catalogue stays total (the `KNOWN_BITS` compile-time assertion).
4. No alias, backfill or dual read (Dimension 4.2's source scan).

## Metrics & Observability

Metric and product-event names under `af_observability` that say `library` are renamed to `plugin` in the same commit as their declarations and census rows; no new signal.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Native API scope and lifecycle | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | Unit gates | `make test-unit-all` | exit 0 | P0 | |
| R3 | Refreshed UI callers | `make dry-app` | exit 0 | P0 | |
| R4 | CLI acceptance | `make acceptance-local SUITE=cli` | exit 0 | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Version | `make check-version` | exit 0 | P0 | |
| S3 | Gates | `orly gate work`; `verify`; `pr` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

Remove the admin catalogue handlers, `publish/platform.rs`, the platform CLI family, `platform_slug`, `/owned` and every `library`-named type once callers move. Historical changelogs stay readable.

## Out of Scope

- Flow 2 capability composition, `@plugin` invocation and bindings (M239).
- The backend data-completeness work for the refreshed UI.
- New Plugin ownership, sharing or ACL models; new platform UI; a primary vault.

## Product Clarity (authoring record)

1. **Successful user moment** — The API, CLI and UI all say Plugin.
2. **Preserved user behaviour** — Authority, immutable revisions, private history and recovery.
3. **Optimal-way check** — A rename on existing resources; no new resource model.
4. **Rebuild-vs-iterate** — Iterate.
5. **What we build** — Renamed routes and payloads, catalogue removal, regenerated OpenAPI and callers.
6. **What we do NOT build** — Aliases, migrations, capability systems.
7. **Fit with existing features** — Flow 1 drafts, publication ETags, lifecycle PATCH/DELETE.
8. **Surface order** — API and schema, then CLI and UI callers, then docs.
9. **Dashboard restraint** — Adapters change; layouts do not.
10. **Confused-user next step** — An old path returns 404 with the problem type; old CLI spellings already fail in M249_002.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Four Sections by surface, merged as one PR so no deployed state mixes vocabularies.
- **Alternatives considered:** Shipping with M249_002 made one ~1000-file review (rejected by Indy); per-surface PRs would leave the API and callers disagreeing between merges.
- **Patch-vs-refactor verdict:** Coordinated rename with targeted removals.

## Discovery (consult log)

- **Consults:** Inventory taken at `4f3159269`; scope closure read from `afd_auth/src/scope.rs`.
- **Model reviews:** EGM review required before this spec moves to `active/`.
- **Open blockers:** M249_001 and M249_002 must merge first.

> Owner decision (Indy, Nov 12, 2026: 11:55 PM): selected "Split into two PRs (my call)" — PR B is "the Plugin + archived rename across Rust, SQL, OpenAPI, CLI flags, UI adapters and docs, rebased onto A".

> Review transfer (Nov 13, 2026): EGM design, readiness and codebase reviews of M249_002 found that the CLI Plugin and Archive renames, the Plugin readers and the `fleet delete` composition contradicted the selected split above and left interim mismatches (`fleet archive` printing "killed", `plugins list` widening to the platform catalogue). They move here unchanged in intent; §2c carries the reviewers' mechanism and the open owner decision.

> Owner decision (Indy, Nov 13, 2026), on the open §2c question of composing archive into delete: "There are two separate options = Archive, and Delete the user can invoke both of then individually. I shouldnt say archive and delte. there no sense to stop and delete. They are independent." Then: "In my view stop must be removed". Later the same day Indy removed `fleet resume` and `fleet kill` too and moved the whole lifecycle into M249_002 ("i want this in this PR"); it is M249_002 §6.

> Owner decisions carried from the authored spec: "Yes flow 1"; "Permanent stop, retain history, no Restore"; "there is no primary vault"; "i dont need a guarded cutover"; "i am only in development".
