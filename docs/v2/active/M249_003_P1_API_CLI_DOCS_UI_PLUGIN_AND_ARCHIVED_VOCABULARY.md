# M249_003: Plugin vocabulary and interface follow-up

**Prototype:** v2.0.0
**Milestone:** M249
**Workstream:** 003
**Date:** Nov 12, 2026
**Status:** IN_PROGRESS
**Priority:** P1, one public vocabulary across API, CLI, UI and docs
**Categories:** API, CLI, DOCS, UI
**Batch:** B3, after M249_002
**Depends on:** M249_001 (UI refresh) and M249_002 (CLI grouping) merged; this branch rebases onto both before editing UI callers
**Provenance:** Split from the authored M249_002 on Nov 12, 2026 at Indy's direction; on Nov 13, 2026 the CLI vocabulary renames, Plugin readers and `fleet delete` composition moved here from M249_002 after EGM review (see Discovery)
**Canonical architecture:** `docs/architecture/api_resources.md`; `docs/architecture/context_lifecycle.md`

## Overview

**Goal (testable):** Fleet-setup routes, payloads, permissions, commands and browser links use Plugins, with removed spellings rejected. The requested auth and summary refinements match the shared theme, and Usage renders complete daily charges and fleet/model facts. Fleet lifecycle behavior remains in M249_002 §6.

**Problem:** The API still says `fleet-libraries`, `library_id` and `library:write`, while the approved interface says Plugins. A platform-wide library catalogue (`/v1/admin/fleet-libraries`, `platform-library:write`, `authoring.library_entries.platform_slug`) exists that Flow 1 does not use.

**Solution summary:** Rename the routes, wire types and permission in one coordinated change. Remove the platform catalogue and the redundant `/owned` collection. Edit the fresh-development schema files in place and regenerate OpenAPI.

Point the CLI and refreshed UI adapters at the renamed routes. Indy permits resetting the development database; there is no alias, database backfill or dual read. The approved one-time identity-provider grant replacement is specified in §1a.

## PR Intent & comprehension handshake

- **PR title (eventual):** Align Plugin vocabulary and complete the requested interface follow-up
- **Intent:** One Plugin vocabulary with unchanged authority and immutable setups; finish the requested auth, summary and Usage presentation using recorded facts.
- **Implementer handshake:** Before editing, record the fetched base, the M249_001 and M249_002 merge commits, and the production/test split of every token in the blast inventory below.
- **Readiness checkpoint (Nov 13, 2026):** Fetched base `646fc536e`; M249_001 merged in `646fc536e` (PR #840), M249_002 in `baa26abed` (PR #839). Both dependencies are satisfied. Indy approved the exact identity-provider grant replacement. The final readiness review and targeted recheck resolved all four major findings and four editorial findings.

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_http/src/route/workspace.rs` and `route/admin.rs`: the library routes.
2. `rustd/crates/daemon/afd_auth/src/scope.rs`: `LibraryWrite`, `PlatformLibraryWrite`, `HIERARCHY` and the `KNOWN_BITS` assertion a removed variant changes.
3. `rustd/crates/daemon/afd_api_tenant/src/handler/library/` and `afd_api_operator/src/handler/admin/libraries.rs`, `library_stage.rs`: the handlers.
4. `schema/20261108121700_authoring_library_entries.sql`, `schema/20261108121800_authoring_library_revisions.sql`: `platform_slug` and the library tables.
5. `docs/RUST_ERROR_STANDARD.md` before touching a fallible signature; `docs/DESIGN_SYSTEM.md` and the merged M249_001 spec before touching UI adapters.

## Files Changed (blast radius)

| File or family | Action | Why |
|---|---|---|
| This spec (`pending/` → `active/` → `done/`) | MOVE/EDIT | Grade the delivered workstream |
| `rustd/crates/daemon/afd_http/src/route/workspace.rs`, `route/admin.rs`, `route_inventory.rs`, `route_meta_total.rs` | EDIT | Plugin paths; admin catalogue and `/owned` removed; inventory and counts |
| `rustd/crates/daemon/afd_api_tenant/src/handler/library/*` | MOVE/EDIT | `handler/plugin/*` |
| `rustd/crates/daemon/afd_api_operator/src/handler/admin/libraries.rs`, `library_stage.rs` | DELETE | Platform catalogue |
| `rustd/crates/daemon/afd_authoring/src/library/**`, `types/**`, `creation/**`, `candidate/source.rs`, `draft/**`, `sql/**`, `funnel.rs`, `lib.rs`, `setup.rs`, `privacy.rs`, `review/context.rs`, `replay/mod.rs` | EDIT/MOVE | Plugin names, UUIDv7 source types and account-only lookup; platform implementation DELETE |
| `rustd/crates/daemon/afd_api_wire/src/{authoring,fleet}/**`, `lib.rs`, `admin_library_native.rs` | EDIT/DELETE | Source/publication receipts, module exports; admin catalogue input DELETE |
| `rustd/crates/daemon/afd_api_tenant/src/handler/{draft,fleet/native}/**`, `handler/{mod,paging}.rs`, `lib.rs`, `openapi.rs`; matching operator registration files | EDIT | Decode/render sources, native creation and route/OpenAPI collectors |
| `rustd/crates/daemon/afd_http/src/openapi/{mod,path,query}.rs` | EDIT | UUIDv7 Plugin parameters and removed admin shapes |
| `rustd/crates/daemon/afd_cron/src/recipe/mod.rs`, `rustd/crates/protocol/af_fleet_runtime/src/definition/mod.rs`, `schema/20261108122000_fleet_revisions.sql` | EDIT | Current fleet-setup documentation |
| `ui/packages/website/src/lib/{llms-text,marketing-copy}.ts` and corresponding tests; `public/llms.txt`, `agentsfleet-manifest.json` | EDIT | Public Plugin vocabulary |
| `rustd/crates/daemon/afd_api_wire/src/library/**` | EDIT | `plugin_id`, `plugin` source kind |
| `rustd/crates/daemon/afd_auth/src/scope.rs`, `tests/scope_catalogue.rs` | EDIT | `plugins:write`; `platform-library:write` removed |
| `rustd/crates/protocol/af_observability/src/product/telemetry/authoring*.rs`, `product/properties.rs` | EDIT | Fleet-setup publication event and property names; model catalogue metrics remain unchanged |
| `rustd/crates/foundation/af_core/src/error_code/**`, `problem/**` | EDIT | Hints that say library |
| `schema/20261108121700_authoring_library_entries.sql`, `20261108121800_authoring_library_revisions.sql` | EDIT | Drop `platform_slug`; Plugin names (pre-production in-place edit) |
| `schema/20261108123600_authoring_drafts.sql`, `rustd/crates/daemon/afd_db/src/migration.rs` | EDIT | Rename draft source columns and foreign keys; register the renamed schema files |
| `rustd/crates/**/src/**/*.sql` referencing those tables | EDIT | Statement renames |
| `rustd/crates/daemon/afd_api/tests/**`, `afd_authoring/tests/**` | EDIT | New paths, names, status; removed-route negatives |
| `public/openapi.json` | GENERATE | `--no-banner`, never by hand |
| `cli/src/lib/api-paths.ts`, `cli/src/commands/library/*` → `cli/src/commands/plugins/*`, `cli/src/commands/plugins-{read,inspect}.ts`, `cli/src/program/plugins.command.ts`, `cli/src/program/tree/index.ts`, `cli/src/commands/platform/**`, `cli/src/program/tree/{plugins,chat,fleet,platform*}.command.ts`, `cli/src/commands/fleet/fleet.ts`, `cli/src/commands/chat/*`, `cli/src/errors/index.ts` | EDIT/MOVE/DELETE | Plugin vocabulary, readers, `platform fleet-library` removed |
| `cli/src/program/tree/platform-library.command.ts` | DELETE | With its `platform-library:write` leaves |
| `cli/test/scoped-discovery.unit.test.ts`, `cli/test/completions.unit.test.ts` | EDIT | Drop the `platform-library:write` cases |
| `cli/test/**`, `cli/test/acceptance/**` | EDIT | New spellings; §2b proofs |
| `ui/packages/app/lib/api/draft-library.ts` and affected adapters, types and `lib/auth/scopes.ts` | EDIT | After the M249_001 merge; layouts untouched |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/**` → `plugins/**`, `components/layout/sidebar-navigation-data.ts`, and browser-route callers/tests | MOVE/EDIT | Public browser URLs use Plugins; the removed route has no redirect or alias |
| Affected docs, playbooks and changelog | EDIT | Current spellings |
| `~/Projects/docs` branch `chore/m249-plugin-vocabulary-changelog` | EDIT | Public API reference, after merge |
| `ui/packages/app/app/(auth)/layout.tsx`, `app/layout.tsx`, `app/globals.css`, `lib/clerkAppearance.ts`, `lib/auth-control-appearance.ts` | EDIT/ADD | Requested auth theme and copy; preserve native recovery flows |
| `ui/packages/app/components/domain/chat/{SetupSummary,SetupQuestions}.tsx`, relevant browser tests and fixtures | EDIT | Summary type scale, wrapping and representative access data |
| `ui/packages/app/components/account/**`, `lib/auth/{client.ts,client.test.tsx,account-controls.tsx}`, `lib/profile-appearance.ts`, security page/components, TeamTable, DraftSidebarHistory, and their tests | EDIT/ADD/DELETE | Owner-requested identity card, remove repeated profile widget, align counts and verify fleet disclosure |
| `ui/packages/app/components/domain/chat/{SetupReview,SetupAccessSummary,SetupSummary,FleetSetupDetails,DraftConversation}.tsx`, setup tests and browser fixtures | EDIT/ADD | Compact setup summary and grouped review of name, source, access, secrets and schedules |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/{ProviderModelSelect,AddModelEntryDialog,ModelsRegistryTable,ModelCatalogueProvider,CatalogueBrowseControls,CatalogueReadControls,ModelDetailsDialog}.tsx`, model-picker tests/helpers and `resource-settings.spec.ts` | EDIT/MOVE | Remove the unapproved advanced-model switch and retain complete model choices, paging and recovery |
| `docs/AUTH.md`, `docs/DESIGN_SYSTEM.md`, implementation evidence | EDIT | Record the owner's revised account and setup design and new proof |
| `rustd/crates/daemon/afd_billing/src/report/{mod.rs,types.rs,queries.rs,sql/aggregates.sql}` | EDIT | Complete daily and per-fleet model projections in the existing report |
| `rustd/crates/daemon/afd_api_wire/src/billing/{mod.rs,query.rs}`, `afd_api_tenant/src/handler/tenant/billing_report/{view.rs,input/**}` | EDIT | Daily wire data, fleet model sets and seven-day period |
| `ui/packages/app/lib/api/billing_report.ts`, `billing-report-values.ts`, billing page components/helpers and their existing tests | EDIT/ADD | Decode projections, draw daily chart and show fleet models |
| Billing integration tests, API report tests, wire/OpenAPI registration and browser fixtures | EDIT | Exact sums, filters, paging, historical names and visual evidence |
| `docs/v2/pending/M249_004_P1_API_DOCS_UI_RECORDED_DATA_COMPLETION.md` | EDIT | Mark only its Usage slice as moved into this requested follow-up |

**Current blast inventory:** [`M249_003_BLAST_INVENTORY.tsv`](../evidence/M249_003_BLAST_INVENTORY.tsv) records every matching file at fetched base `646fc536e9e504d0bda2db1742aab21e3e3896a8`, its production/test role, semantic classification and intended action. The case-insensitive `library|libraries` scan includes identifier substrings, filenames represented in source, current documentation and historical records.

The listed fleet-setup consumers and their module registrations belong to this scope. Rename only those meanings within mixed files; retain unrelated model catalogues, package imports, generic software-library prose and literal rejection-test inputs. History remains readable. Recheck the inventory after moving files and after generation.

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
| `make dry-app` | Yes | Setup, Plugin publication and lifecycle flows on the refreshed UI |
| `make acceptance-local SUITE=cli` | Yes | CLI against the renamed API |
| `gitleaks detect --redact --no-banner` | Yes | No credential bytes |

## Prior-Art / Reference Implementations

- Immutable library definitions, draft source pinning and publication ETags already implement Flow 1; this renames them without replacing them.
- Existing `KNOWN_BITS` compile-time assertion in `scope.rs` proves the scope catalogue stays total after a variant is removed.

## Sections (implementation slices)

### §1 — Plugin routes, wire and permission

`/v1/workspaces/{workspace_id}/fleet-libraries[/{library_id}[/revisions[/{revision_id}]]]` becomes `/plugins[/{plugin_id}[/revisions[/{revision_id}]]]`. `library` source kind and `library_id` become `plugin` and `plugin_id`; `revision_id` stays.

`library:write` becomes `plugins:write` with the same authority and narrowing. `/fleet-libraries/owned` is removed; the ordinary collection lists the account-owned entries. Definition content, pinned versions, review digests and optimistic publication checks are unchanged.

- **Dimension 1.1** — DONE — Plugin reuse pins a full revision → Test `plugin_revision_saves_an_independent_fleet` (integration): a draft from `(plugin_id, revision_id)` saves a fleet whose definition digest equals the revision's.
- **Dimension 1.2** — DONE — Renaming cannot broaden access → Test `plugin_permission_preserves_narrowed_authority` (integration): a key without `plugins:write` gets 403 on publish, revise and delete; a reader keeps list and show.
- **Dimension 1.3** — DONE — Revisions stay immutable → Test `plugin_revision_and_installed_copy_do_not_follow_updates` (integration): publishing revision 2 or deleting the Plugin leaves a fleet saved from revision 1 byte-identical.

### §1a — Existing development identity grants

For each existing Clerk `public_metadata.scopes` space-separated string, replace only the complete `library:write` token with `plugins:write` and remove the complete `platform-library:write` token.
Preserve every other scope, narrowing and metadata field. A user without `library:write` receives no new grant; the operation is idempotent.
Missing scope claims remain missing; malformed scope metadata stops the transition for investigation rather than receiving a default grant.

At development deployment, pause API traffic, apply the approved metadata replacement and start the renamed API with empty capability caches before resuming traffic.
Do not rely on the resolver's one-minute freshness or fifteen-minute outage fallback to complete the transition.

Browser session tokens carry their own scopes and bypass the capability resolver. An already-issued token carrying only `library:write` must refuse Plugin writes; resetting the resolver cannot refresh that token.

Before resuming traffic, verify the provider's configured maximum session-token lifetime and retain the pause for that duration after the last metadata replacement; the current deployment uses 60 seconds and the backend applies zero expiry leeway.
Refresh the validation browser session through its native provider flow and confirm a newly issued token authorizes the writer. Test that the pre-transition token fails closed and the refreshed narrowed reader still cannot publish, revise or delete.

Do not revoke sessions, reinterpret old claims or widen scopes. A provider configuration that cannot establish that expiry bound blocks deployment, not implementation.

Record before/after scope sets privately for verification without committing identity metadata or credentials.
Verify an owner writer, a narrowed reader, an account API key and an operator; replaying the transformation must make no further change.
Use a reviewed one-off deployment helper outside the repository; do not add a migration branch or compatibility parser to production provisioning.
Before deployment, exercise that helper on isolated metadata fixtures covering the cases in Dimension 1a.1 and record the results with the deployment runbook.
The current approval covers this plan and implementation, not an immediate mutation of live identities before the renamed deployment is ready.

- **Dimension 1a.1** — DONE — Existing grants retain their authority → Test `plugin_grant_transition_is_exact_and_idempotent` (deployment-helper fixture check): old writers retain write authority, narrowed readers gain none, removed platform grants disappear, unrelated metadata remains unchanged, and replay is a no-op. Before/after development authorization checks remain a deployment requirement.

### §2 — Platform catalogue removal

Delete `/v1/admin/fleet-libraries*`, the operator handlers, `publish/platform.rs`, `Scope::PlatformLibraryWrite`, `platform_slug` and its unique index, and the `platform fleet-library` CLI family with its discovery row.

Every remaining Plugin belongs to one account: `tenant_id` is required, and collection, detail, history, reuse and mutation queries enforce the requesting workspace's account.
Remove the now-constant `visibility` discriminator from storage, wire cards, authoring types, CLI formatting and UI adapters, following the single-valued-field rule (DFS).
Remove platform visibility branches from entry lookup and draft creation as well as collection queries; no lookup by platform slug survives.

Rename `authoring.library_entries` and `authoring.library_revisions` to `authoring.plugin_entries` and `authoring.plugin_revisions`, including their schema filenames and migration registry entries.
Rename revision `library_entry_id` to `plugin_entry_id` and draft pointers `source_library_revision_id`/`library_revision_id` to `source_plugin_revision_id`/`plugin_revision_id`.
Rename dependent indexes, constraints, functions and triggers consistently; retain append-only enforcement, deferred head references, cascade deletion of revisions and both draft pointers' `ON DELETE SET NULL` behavior.

- **Dimension 2.1** — DONE — The catalogue stays absent → Test `platform_catalogue_routes_and_commands_are_removed` (integration + CLI unit): the old admin paths return 404 under an operator key, `platform-library:write` grants nothing, and `platform fleet-library` fails parsing.
- **Dimension 2.2** — DONE — Account ownership covers every read and reuse → Test `plugins_remain_private_across_accounts` (integration): an account-B workspace cannot list, read, read revisions, reuse, revise or delete account-A's Plugin. Detail/history/reuse hide its existence with the existing unavailable response; another authorized workspace in account A retains the existing access. An operator's explicit workspace-crossing grant retains its existing audited behavior.
- **Dimension 2.3** — DONE — Removed workspace routes do not serve → Test `removed_workspace_library_routes_return_not_found` (integration): with a valid authorized credential, old collection, `/owned`, item, revision-list and revision-item paths return 404 for their formerly supported methods.

### §2b — CLI Plugin vocabulary and readers

M249_002 §7 already renamed the CLI group to `plugins` (`list`, `delete <plugin_id>`) and the chat flags to `--plugin`/`--plugin-etag`.
This Section points those existing commands at the renamed routes; it does not add a second delete command.
`--revision`, `--publish` and `--publish-to` stay.

Add `show <plugin_id>`, `revisions <plugin_id> [--starting-after] [--limit]`, and `revisions <plugin_id> <revision_id>`.
`plugins list` uses the ordinary collection, which after §1 lists only account-owned entries.
Both identifiers require UUIDv7 consistently in API paths, draft source selections and CLI arguments; platform slugs and other opaque strings are rejected before lookup.

`show` prints `card.name`, `card.id`, `card.head_revision_id` and the response `etag`.
The revision list prints each item's `id`, decimal `revision`, `canonical_digest` and `created_at`, preserving the API's cursor and limit behavior.
When `next_cursor` is present, human output includes `agentsfleet plugins revisions <plugin_id> --starting-after <next_cursor>` and retains an explicitly selected limit.
Revision detail prints `revision.revision`, `revision.canonical_digest` and `review.review_digest`; JSON passes complete bodies through.

`api-paths.ts` gains revision path helpers.
Discovery's `platform-library:write` row and the `platform fleet-library` family are deleted with §2.

- **Dimension 2b.1** — DONE — CLI Plugin reuse pins a revision → Tests `a pinned Plugin revision is reviewed and saved with no first message and no prompt` and `removed_group_spellings_are_refused` (unit): `chat --plugin P --revision R --no-input` sends `P` and `R` unchanged in the draft-creation body; `chat --library P` exits 4 with `UNKNOWN_OPTION`.
- **Dimension 2b.2** — DONE — Plugin readers hit the renamed routes → Test `plugins_show_and_revisions_read_renamed_routes` (unit): `show`, `revisions` and `revisions <r>` request `/plugins/P`, `/plugins/P/revisions` and `/plugins/P/revisions/R`, print the stubbed `etag` and digests, and a 404 renders the block with its `UZ-` code and exit 3.
- **Dimension 2b.3** — DONE — Reader pagination and selection remain explicit → Test `plugin_readers_validate_ids_and_preserve_paging` (unit + integration): invalid Plugin/revision IDs are rejected; list requests forward cursor/limit; human output uses summary fields and a usable continuation command; detail output uses the nested revision; JSON retains every response field.

### §2c, §3 — moved

The fleet lifecycle (archive as a new state, delete in any status, no stop, resume or kill) moved to M249_002 §6 at Indy's direction on Nov 13, 2026 (see Discovery).

### §4 — Coordinated callers

Regenerate OpenAPI. Update CLI paths, the refreshed UI's adapters and types, docs and the public reference together.

Move the workspace browser route from `/w/{workspace_id}/library` to `/w/{workspace_id}/plugins` and update navigation and links.
An authenticated request to the removed browser route returns the application's not-found page, without an alias or redirect.
Rerun setup, Plugin publication and lifecycle browser tests on the refreshed UI.

- **Dimension 4.1** — PENDING — Every caller uses the renamed surface → Test `plugin_surfaces_match_cli_ui_and_openapi` (integration + browser): create, revise, reuse and delete succeed through the CLI and the UI against the new paths, and OpenAPI lists no `fleet-libraries` path.
- **Dimension 4.2** — DONE — One vocabulary → Source scan `rg --hidden -n`, recorded in [the classified audit](../evidence/M249_003_SOURCE_AUDIT.tsv): no active fleet-setup declaration or caller uses `fleet-libraries`, `library_id`, `library:write`, or `platform-library`. Report every match and classify it as an active declaration/caller, an explicit rejection-test input or assertion, or a historical record.

  Scan the entire worktree, including hidden paths, excluding Git internals and the generated audit itself. Ripgrep retains its standard ignore rules.

  Negative tests retain literal removed spellings so their intent remains readable. Current implementation/review documents may describe the transition; they must not advertise removed paths as supported. Unrelated model/software library terminology is excluded by meaning, never by a blanket directory exclusion.
- **Dimension 4.3** — PENDING — Plugin navigation uses the renamed browser route → Test `plugin_navigation_rejects_removed_library_route` (browser): the Plugins link opens `/w/W/plugins`; the authenticated `/w/W/library` request renders not found; existing Plugin actions remain reachable at the new route.
- **Dimension 4.4** — DONE — Failed publication preserves state → Test `plugin_publication_preserves_conflicts_and_revocation` (integration): a stale ETag retains the existing conflict status/code, and a provider grant revoked after review refuses publication. Neither case advances the head or changes installed copies; a new authorized review with the current ETag can publish once.

### §5 — Requested auth and summary refinements

Use the shared appearance adapter for sign-in, signup and native password recovery, with 44-pixel form fields and the existing responsive action heights.
Revise sign-in copy around tasks, connected tools and reviewable results. Preserve local auth routes, keyboard focus, invalid-input correction and provider-owned verification.
Setup summary and success criteria use the shared 13-pixel scale and wrap long identifiers without widening the panel.

Review Team and Security against production components and the [uploaded proposal](/workspace/artifacts/m249-proposal/visual-review-candidate-ed7c4032b-desktop-a68d0c18f6e545dd94f5ad4161786bd3/proposal.html). The owner's Nov 14 polish request replaces the repeated Profile button and embedded provider widget with avatar, name and email above API keys and the existing provider-backed session list. Session sign-out retains confirmation and recent verification.
The deleted development draft cannot supply a reproducible failure; the owner will provide a replacement later. No success claim is made for that deleted draft.

- **Dimension 5.1** — DONE — Native auth retains its behavior and shared theme → Existing `auth-theme.spec.ts` plus sign-in/sign-up accessibility checks in both browser projects; capture both themes and phone/desktop widths.
- **Dimension 5.2** — DONE — Summary remains readable → Existing `chat-alignment.spec.ts` renders populated access requirements, verifies consistent list/purpose scale and preserves unsent input; inspect long content at narrow widths.
- **Dimension 5.3** — DONE — Account identity precedes API keys and sessions; no repeated profile button or provider widget. Loading, absent metadata and sign-out remain explicit → `account-controls.test.tsx` (unit) and `account-settings.spec.ts` (browser) cover phone/desktop and both themes.
- **Dimension 5.4** — DONE — Team counts align with their heading at each width. The fleet list retains paging and gains an accessible disclosure; navigation and deletion keep their current ownership → `resource-settings.spec.ts`, `chat-alignment.spec.ts` (browser), existing `RecentChats.test.tsx` (component integration).
- **Dimension 5.5** — DONE — Fleet setup has a compact summary and grouped review of name, source revision, runtime model, purpose, instructions, repositories, tools, network, secrets/connections, spending and schedules. Proposed triggers remain distinct from saved registration, sync failure and next run. Show secret names only. Unknown fields remain unknown. Opening and closing review preserves the unsent composer → `setup-review.test.tsx` (component integration), `setup-summary.test.tsx` (unit) and `chat-alignment.spec.ts` (browser).
- **Dimension 5.6** — PENDING — The owner's screenshot identifies the missing masked key hint and Last used metadata. The current API returns neither; stored hashes cannot recover an existing key's suffix. The new API Keys fixture demonstrates the name only. Define and implement recorded metadata before claiming this defect fixed; the proposed row is `/workspace/artifacts/m249-design-approval/api-key-row-proposal.html`.
- **Dimension 5.7** — DONE — Remove Show advanced models from the approved form. All provider model choices remain selectable without that switch; retain current selections, catalogue paging and retry → Existing `provider-model-select.test.tsx`, `models-registry-add.test.tsx` (unit), catalogue provider/paging tests (component integration), and `resource-settings.spec.ts` (browser).

The owner requested "Fleet setup, first show me a design, so i can approve."
The replacement setup page remains a proposal at `/workspace/artifacts/m249-design-approval/fleet-setup-proposal.html`; approval is pending before further setup implementation.

### §6 — Usage parity with complete recorded data

This moves only M249_004 §2 into the present owner-requested follow-up. Its other data-completeness work remains separate.
The [uploaded `proposal.html`](/workspace/artifacts/m249-proposal/visual-review-candidate-ed7c4032b-desktop-a68d0c18f6e545dd94f5ad4161786bd3/proposal.html) Usage view supplies the visual reference: period/fleet controls, totals strip, daily bars and a Fleet/Model/Activities/Charged table.
Its sample values are illustration data, never production defaults or an aggregation source.

Extend the existing tenant report and its read-only snapshot; retain all current authorization, filters, inclusive bounds and activity pagination.
Add a required `daily` array of `{day, totals}` objects, where `day` is the UTC `YYYY-MM-DD` date of ledger `created_at` and `totals` uses the existing exact totals shape.
Return every calendar day from the report's `from_at` through `through_at`, ascending, with zero totals for days without matching settled rows.

Daily `activity_count` counts distinct `(work_type, work_id)` keys with matching lines on that day; an activity spanning midnight counts once on each day and once in the period. It is deliberately not additive. Every other daily total sums exactly to its period counterpart.

Use the same `created_at` period bounds and `settled_at <= through_at` cutoff as current totals; late settlements must not silently move between days.

Add a required `models` array only to each `spending.by_fleet` item, containing sorted distinct nonempty recorded model identifiers from the same filtered ledger set.
Retain deleted fleet identifiers/names and null-fleet setup charges. Never derive complete aggregates from the paginated activity array or current fleet definitions.

Add `period=week`, meaning midnight UTC six calendar days before the cutoff's UTC date through the inclusive cutoff.
Keep `month` and `today`, cursor scope matching and historical month navigation. The primary period control includes Last 7 days; unsupported values still fail validation.

Render daily bars, daily high and date-axis labels from `daily`, keeping exact integer nanos until formatting or bounded display scaling.
The chart exposes accessible day/charge text and distinguishes an authoritative zero from a failed report load.
The Model column lists all recorded models, resolving current display labels when available and preserving the identifier otherwise.
Preserve detailed charge inspection and existing model/setup filters in the secondary controls.

- **Dimension 6.1** — DONE — Daily sums reconcile → Test `daily_spend_reconciles_filtered_snapshot` (integration): UTC midnight, month end, seven-day periods, late settlement and all filters reconcile additive totals exactly; a work item spanning midnight counts once per day and once per period; days with no charges are zero.
- **Dimension 6.2** — DONE — Paging preserves complete aggregates → Test `daily_spend_ignores_activity_cursor` (integration): all activity pages retain identical daily totals and fleet/model sets for a fixed cutoff.
- **Dimension 6.3** — DONE — Historical attribution survives → Test `usage_keeps_deleted_fleet_models` (integration): deletion retains recorded fleet identity and all matching models; setup and unpriced usage retain their existing accounting.
- **Dimension 6.4** — DONE — Browser parity uses real response fields → Browser checks show filtered bars and Model cells in both themes and narrow widths; zero usage and unavailable responses remain distinct, and values beyond JavaScript's safe integer range format exactly.

## Interfaces

`/v1/workspaces/{workspace_id}/plugins`, `/plugins/{plugin_id}`, `/plugins/{plugin_id}/revisions`, `/plugins/{plugin_id}/revisions/{revision_id}`. Scope `plugins:write`. Account API keys stay `/v1/api-keys`; platform defaults stay `/v1/admin/platform-keys`.

## Failure Modes

| Mode | Kind | Handling | Test |
|---|---|---|---|
| Scope rename broadens access | Security | Same ladder position; explicit negatives | `plugin_permission_preserves_narrowed_authority` |
| Plugin write conflicts or access is lost | Security | ETags and scoped refusal kept; installed copies untouched | `plugin_publication_preserves_conflicts_and_revocation` |
| An old route still serves | Failure | Route and inventory removal; 404 negative | `platform_catalogue_routes_and_commands_are_removed`; `removed_workspace_library_routes_return_not_found`; `plugin_navigation_rejects_removed_library_route` |
| A caller misses the rename | Failure | Cross-surface caller test | `plugin_surfaces_match_cli_ui_and_openapi` |

## Invariants

1. Backend authorization is authoritative (enforced by the existing scope gate tests plus Dimension 1.2).
2. Plugins remain complete setups; revisions and installed copies never follow later edits (Dimension 1.3).
3. The scope catalogue stays total (the `KNOWN_BITS` compile-time assertion).
4. No alias, database backfill or dual read (Dimension 4.2's source scan); identity grants use only the approved exact replacement (§1a).

## Metrics & Observability

Rename only fleet-setup publication events and their properties under `af_observability`, together with their declarations, tests and consumers. The `metrics/declared/library.rs` family and its labels/producers measure the model catalogue and remain unchanged. No new signal.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Native API scope and lifecycle | `make test-integration-rustd` | exit 0 | P0 | ✅ 1,779 regular and 2 exclusive checks passed; 10 final Plugin checks passed |
| R2 | Unit gates | `make test-unit-all` | exit 0 | P0 | ✅ All unit lanes passed |
| R3 | Refreshed UI callers | `make dry-app` | exit 0 | P0 | ✅ 94 passed |
| R4 | CLI acceptance | `make acceptance-local SUITE=cli` | exit 0 | P0 | ❌ local acceptance requires macOS with a dedicated OrbStack Linux machine |
| S1 | Lint | `make lint-all` | exit 0 | P0 | ✅ make lint-all exited 0 |
| S2 | Version | `make check-version` | exit 0 | P0 | ✅ all versions match 0.77.0 |
| S3 | Gates | `orly gate work`; `verify`; `pr` | exit 0 | P0 | ❌ work and verify passed; pr not run under the cloud shipping rule |
| S4 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | ✅ no leaks found |

## Dead Code Sweep

Remove the admin catalogue handlers, `publish/platform.rs`, the platform CLI family, `platform_slug`, `/owned` and fleet-setup `library`-named types once callers move. Preserve unrelated model-catalogue types such as `LibraryModel` and `ModelLibrary`, software-library terms and external package names. Historical changelogs stay readable.

## Out of Scope

- Flow 2 capability composition, `@plugin` invocation and bindings (M239).
- Backend data-completeness work beyond the Usage slice explicitly included in §6.
- New Plugin ownership, sharing or ACL models; new platform UI; a primary vault.

## Product Clarity (authoring record)

1. **Successful user moment** — The API, CLI and UI all say Plugin.
2. **Preserved user behaviour** — Authority, immutable revisions, private history and recovery.
3. **Optimal-way check** — A rename on existing resources; no new resource model.
4. **Rebuild-vs-iterate** — Iterate.
5. **What we build** — Renamed routes and payloads, catalogue removal, regenerated OpenAPI and callers; shared auth appearance and copy, readable summaries, daily Usage data and fleet/model attribution.
6. **What we do NOT build** — Aliases, migrations, capability systems.
7. **Fit with existing features** — Flow 1 drafts, publication ETags, lifecycle PATCH/DELETE.
8. **Surface order** — API and schema, then CLI and UI callers, then docs.
9. **Dashboard restraint** — Plugin adapters retain their layout; auth, summary and Usage change only as requested in §§5–6.
10. **Confused-user next step** — An old path returns 404 with the problem type; old CLI spellings already fail in M249_002.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Coordinated Plugin vocabulary sections plus the requested interface and Usage sections, merged as one PR so no deployed state mixes vocabularies.
- **Alternatives considered:** Shipping with M249_002 made one ~1000-file review (rejected by Indy); per-surface PRs would leave the API and callers disagreeing between merges.
- **Patch-vs-refactor verdict:** Coordinated rename with targeted removals.

## Discovery (consult log)

- **Consults:** Inventory taken at `4f3159269`; scope closure read from `afd_auth/src/scope.rs`.
- **Model reviews:** Initial zoom-out, comprehension, design, structural, style and intermediate zoom-out reviews completed on Nov 13, 2026 (UTC). The final implementation-readiness review found four source-grounding gaps and four editorial issues; the amended spec resolves token renewal, daily count semantics, concrete file scope and excluded model metrics. Initial findings are recorded in [the readiness record](../evidence/M249_003_READINESS_REVIEW.md); the [final review](../evidence/M249_003_FINAL_READINESS.md) records the targeted resolution pass.
- **Resolved blocker (Nov 13, 2026):** Existing Clerk grants survive a database reset, so code renaming alone loses existing writer access. Indy replied "1. approved" to the exact replacement proposal: replace `library:write` with `plugins:write`, remove `platform-library:write`, preserve all other grants. §1a records the bounded exception and deployment verification; live identities have not been changed.
- **Owner-requested follow-up (Nov 13, 2026):** The original request includes auth theme/copy, Usage alignment, Team/Security review and a live fleet check. Indy supplied the requested visual-review ZIP and then stated the database was torn down: "i will give you a nother one later" and "so move on to the next". The deleted draft is no longer a live-review target.

> Owner decision (Indy, Nov 12, 2026: 11:55 PM): selected "Split into two PRs (my call)" — PR B is "the Plugin + archived rename across Rust, SQL, OpenAPI, CLI flags, UI adapters and docs, rebased onto A".

> Review transfer (Nov 13, 2026): EGM design, readiness and codebase reviews of M249_002 found that the CLI Plugin and Archive renames, the Plugin readers and the `fleet delete` composition contradicted the selected split above and left interim mismatches (`fleet archive` printing "killed", `plugins list` widening to the platform catalogue). They move here unchanged in intent; §2c carries the reviewers' mechanism and the open owner decision.

> Owner decision (Indy, Nov 13, 2026), on the open §2c question of composing archive into delete: "There are two separate options = Archive, and Delete the user can invoke both of then individually. I shouldnt say archive and delte. there no sense to stop and delete. They are independent." Then: "In my view stop must be removed". Later the same day Indy removed `fleet resume` and `fleet kill` too and moved the whole lifecycle into M249_002 ("i want this in this PR"); it is M249_002 §6.

> Owner decisions carried from the authored spec: "Yes flow 1"; "Permanent stop, retain history, no Restore"; "there is no primary vault"; "i dont need a guarded cutover"; "i am only in development".

## Session Notes 1 — implementation verification

The candidate implements the Plugin vocabulary, auth theme, summary typography and complete Usage projections.
The base is `646fc536e9e504d0bda2db1742aab21e3e3896a8`; both dependency workstreams are merged.
[The validation record](../evidence/M249_003_IMPLEMENTATION_VALIDATION.md) records baseline counts, candidate results, review coverage and outstanding checks.

The workstream stays active while authenticated acceptance remains incomplete.
No deployment or live identity update has occurred.
Repository instructions require committing companion API pages after the application change merges.
This task keeps the companion documentation branch together until that merge.
