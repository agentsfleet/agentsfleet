# M249_004: Complete recorded data and one-call Usage

**Prototype:** v2.0.0
**Milestone:** M249
**Workstream:** 004
**Date:** Nov 13, 2026
**Status:** IN_PROGRESS
**Priority:** P1, approved screens need authoritative data
**Categories:** API, DOCS, UI
**Batch:** B3, data work follows the merged UI checkpoint (PR #844); M249_003 is parked
**Depends on:** M249_001 (PR #840, `646fc536e`), M249_002 (PR #839, `baa26abed`) and the UI checkpoint (PR #844, `a1dcc2224`), all merged. Parked M249_003 blocks nothing here
**Provenance:** Agent-generated from the UI data inventory and Indy's requested Claude handoff
**Canonical architecture:** `docs/DESIGN_SYSTEM.md`; `docs/architecture/context_lifecycle.md`; `docs/architecture/api_resources.md`

## Overview

**Goal (testable):** Usage loads from one authoritative report request, including model labels. The refreshed UI displays complete recorded usage, key metadata, secret access, revision labels, changes, outputs without inferred facts.

**Problem:** Several approved components have no authoritative backing field or complete projection. They currently show supported facts or explicit unavailable states.

**Solution summary:** Extend existing producers and read responses where facts exist, then wire the refreshed components. Remove Usage's separate model-catalogue dependency. Retain current setup-reuse semantics and unavailable states where a producer remains undefined. The broader Plugin model is a separate discussion.

**Design flow:** Authenticated read → existing account/workspace authorization → stored ledger, policy or event facts → typed response → existing UI adapter → approved component.

Implementation runs in its own worktree on `feat/m249-recorded-data-completion`, cut from fetched main `a1dcc222488e4522f8b92208a4f7df8461fe7fbf` (PR #844 merge).

Usage, key metadata, secret access and recorded work remain valid. Measured context budget is parked separately; M249_003 is parked as the rename workstream.
Use the retained API names and payloads merged in PR #844. Keep the Plugin UI disabled and its backend APIs intact until the separate product discussion reopens it.

The original daily-chart work moved to M249_003 §6. The new one-call requirement in §2a remains here and must not reimplement those completed aggregates.
Parked M249_003 §7 records the separate-session product discussion. It does not authorize new capability bindings or provider actions in this workstream.

## PR Intent & comprehension handshake

- **PR title (eventual):** Complete recorded data behind refreshed UI
- **Intent:** Replace documented unavailable states with verified data while preserving the approved layouts and access boundaries.
- **Authoring handshake:** Complete backend/API gaps behind M249_001. ASSUMPTIONS I'M MAKING: existing resource authorization and workspace-backed credentials remain authoritative.
- **Implementer handshake:** Record both merge SHAs, the fetched base, renamed paths and each field's producer before implementation.

## Implementing agent — read these first

1. `docs/v2/evidence/M249_VERIFICATION.md` — actual data differences and UI boundaries.
2. `ui/packages/app/lib/api/billing_report.ts` — complete settled totals versus paginated activity details.
3. `rustd/crates/daemon/afd_billing/src/report/sql/aggregates.sql` — filtered ledger aggregates and settlement cutoff.
4. `rustd/crates/daemon/afd_api_wire/src/tenant.rs` and `secret.rs` — current account-key and secret metadata.
5. `rustd/crates/daemon/afd_api_wire/src/plugin/output.rs` — current revision metadata. M249_003 §7 links the deferred product questions.

## Product Clarity (authoring record)

1. **Successful user moment** — Usage totals reconcile with its daily chart; resource and Summary labels describe actual recorded facts.
2. **Preserved user behaviour** — Create/revoke keys, replace secrets, select revisions, inspect activity and use model credentials in their existing workspace.
3. **Optimal-way check** — Extend existing scoped reads rather than adding parallel APIs or reconstructing authoritative totals in the browser.
4. **Rebuild-vs-iterate** — Extend projections and their producers. Keep the approved component structure.
5. **What we build** — Daily usage and fleet/model projections, safe metadata and supported policy and event read fields with UI adapters.
6. **What we do NOT build** — Capability plugins, real uploads, mention invocation, platform administration or a new secret authorization model.
7. **Fit with existing features** — Use the existing settled ledger, effective policies, immutable revisions and native chat ownership.
8. **Surface order** — Backend contract and producer tests first, then adapters and UI in the same follow-up PR.
9. **Dashboard restraint** — Unknown data stays unavailable. A related link, configured capacity or secret reference cannot prove a recorded outcome.
10. **Confused-user next step** — Show the specific unavailable fact; retain existing activity details, resource management and supported retry actions.

## Files Changed (blast radius)

Paths were re-inventoried against `a1dcc2224`; §1's inventory records the exact files each Section touches.

| File or family | Action | Why |
|---|---|---|
| This spec and its eventual `done/` path | EDIT/MOVE | Record field decisions, tests and completion |
| `docs/v2/evidence/M249_004_CLAUDE_HANDOFF.md` | EDIT | Correct stale ordering against parked M249_003 |
| `docs/v2/evidence/{M249_VISUAL_BACKLOG,M249_VERIFICATION}.md`, `docs/v2/done/M249_004_SECTION_5_MEASURED_CONTEXT_BUDGET.md`, `docs/v2/parked/README.md` | EDIT | Links follow this spec's move to `active/` |
| `rustd/rust-toolchain.toml`, `rustd/Cargo.toml`, `playbooks/operations/ci_rust_images/versions.env`, `README.md` | EDIT | Rust pin moves to 1.100.0, at the owner's direction |
| `docker-compose.yml`, `compose.dev-infra.yml` | EDIT | Postgres 19rc2; development NATS, Mailpit and QStash catch up to the pins the test stack already uses |
| Rust sources clippy 1.100 rewrites (`map_or_default`, redundant clones) | EDIT | Mechanical lint fixes the new toolchain requires |
| `rustd/Cargo.toml`, `rustd/Cargo.lock`; `af_stream/src/h3/{client,server}.rs`; `afd_cron/src/verifier.rs` and the HMAC fixtures that sign with it | EDIT | Every crate to its latest release the dependency graph admits; `tokio-quiche` 0.23 drops `H3Event::NewFlow` and `jsonwebtoken` 12 refuses empty HMAC keys |
| `package.json`, `cli/package.json`, `ui/packages/*/package.json`, `bun.lock`, `cli/bun.lock`, `patches/@assistant-ui%2F{react,store}@*.patch` | EDIT/MOVE | Every JavaScript package to its latest release; both local assistant-ui patches carried forward unchanged |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryView{,.test}.tsx` | EDIT | The Platform default card names its model only; "No provider key needed" misled beside "Platform default" (owner, Nov 15, 2026) |
| `ui/packages/app/tests/e2e/acceptance/**`, `cli/test/acceptance/**` and their fixtures | EDIT/ADD | Stale Usage and Secrets specs, plus the ten user journeys §7 lists |
| `ui/packages/app/lib/model-labels{,-server}.ts`, `components/domain/models/ModelLabels.tsx`, `app/(dashboard)/layout.tsx` and their tests | ADD/EDIT | One shared model-label store, read once per sign-in or refresh, replacing Usage's own catalogue call |
| `docs/architecture/billing_reporting.md` | EDIT | The one-request loader and the label store |
| `ui/packages/app/vitest.config.ts`, `ui/packages/design-system/vitest.config.ts` | EDIT | Pin the test zone to UTC so time tests grade the same off a UTC machine (owner approval, Nov 15, 2026) |
| `docs/v2/evidence/M249_004_SOURCE_INVENTORY.md` | ADD | §1 field inventory, request counts, timings and query plans |
| `rustd/crates/daemon/afd_billing/src/report/{types.rs,queries.rs,sql/aggregates.sql}` | EDIT | Whole-period daily and fleet/model projections |
| `rustd/crates/daemon/afd_api_wire/src/billing/mod.rs` | EDIT | Typed report fields |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/billing_report/{mod.rs,view.rs}` | EDIT | Existing authorized report response |
| `ui/packages/app/lib/api/billing_report.ts` | EDIT | Decode and validate complete projections |
| `ui/packages/app/app/(dashboard)/settings/billing/components/{BillingReportView.tsx,BillingSpending.tsx}` | EDIT | Chart and complete fleet/model facts |
| `ui/packages/app/app/(dashboard)/settings/billing/{page.tsx,loading.tsx,lib/load-report.ts,components/**}` and billing tests | EDIT | Plumbing only: one report request, the streamed data region (`loading.tsx` gives way to the page's `Suspense`), and `UsageReportView`, which names models from the shared label store. The Usage components stay as main has them; Codex owns their visual changes |
| `ui/packages/app/app/(dashboard)/layout.tsx` and shell read boundaries | REVIEW; EDIT only for confirmed blocking reads | First-load profiling must include parent layout reads, not only the report component |
| `model.platform_models` metadata lookup and its existing read owner | PLAN before EDIT | Resolve only report model labels, with provider/model identity and no full-catalogue read |
| `rustd/crates/daemon/afd_api_wire/src/tenant.rs` and `afd_api_tenant/src/handler/tenant/api_key.rs` | EDIT | Safe account-key metadata |
| `ui/packages/app/lib/api/api_keys.ts` and `app/(dashboard)/settings/security/components/ApiKeyList.tsx` | EDIT | Metadata without credential exposure |
| `rustd/crates/daemon/afd_api_wire/src/secret.rs` and existing secret handlers | EDIT | Effective-access read projection, if defined |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/secret-row-cells.tsx` and its existing management dialog | EDIT | Available-to facts from existing policy |
| `rustd/crates/daemon/afd_api_wire/src/plugin/output.rs` and the existing collection projection | EDIT | Head revision ordinal without per-card requests; no invented capability-binding model |
| `ui/packages/app/lib/api/draft-plugin.ts` and `components/domain/plugins/**` | EDIT after interface agreement | Consume the current published-setup metadata and recorded revision identity |
| `ui/packages/app/components/domain/conversation/summary/{WorkContextSections.tsx,recorded-changes.ts}`, `setup/composer/ChatComposer.tsx` and usage components | EDIT | Confirmed work, Plugin provenance; preserve the UI refactor |
| Identified key-authentication, event-publication producers | EDIT | Record missing facts at their actual owner |
| Canonical schema files, if new durable facts are needed | EDIT | Fresh-development schema; no migration bridge |
| `public/openapi.json` | GENERATE | Generated contract, never hand-edited |
| Existing tests for each changed producer, handler and component | EDIT | Complete data, refusals, races and unavailable states |
| `docs/v2/evidence/M249_VERIFICATION.md`, `docs/DESIGN_SYSTEM.md`, `docs/architecture/{api_resources,web_app}.md` | EDIT | Update resolved data differences and retained unknowns |
| `docs/v2/evidence/M249_PLUGIN_PRODUCT_REASSESSMENT.md` | EDIT | Preserve the deferred Fleet/template/Plugin questions without adding implementation scope |
| Separate `agentsfleet/docs` branch and changelog | EDIT | Public contract and visible behaviour |

Resolve family entries to exact files during PLAN. Unidentified producers block their implementation Section.

## Applicable Rules

- Repository instructions, auth and HTTP standards preserve authorization, bounded inputs and problem responses.
- `docs/RUST_ERROR_STANDARD.md` governs fallible Rust signatures and generated error shells.
- TypeScript and test standards preserve strict decoding, current coverage floors and native Make checks.
- SPEC-STATUS, SPEC-GOLDEN, SPEC-TESTS and SPEC-MOVE govern readiness and completion.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| `orly doctor`, `orly gate work`, `orly gate verify`, `orly gate pr` | Yes | Installed rules and final candidate evidence |
| `make lint-all` | Yes | All changed languages and generated consumers |
| `make test-unit-all` | Yes | Existing coverage floors, including TypeScript 100 percent |
| `make test-integration-rustd` | Yes | Real ledger, account and workspace boundaries |
| `make check-version` | Yes | Version and generated contract agreement |
| `make dry-app` and `make acceptance-codex` | Yes | Refreshed adapters and authenticated resource flows |
| Existing bundle-size checks | Yes | Keep caps; any excess needs owner approval |
| `gitleaks detect --redact --no-banner` | Yes | No credentials in source or evidence |
| File ≤350 lines; function ≤50; method ≤70 | Yes | Keep domain projections separate |

## Prior-Art / Reference Implementations

- The existing billing aggregate reads the entire filtered ledger; paginated activities remain detail navigation.
- Key creation already returns secret bytes once. Listing and usage metadata must retain that boundary.
- Secret summaries classify server-side without decrypting every secret for display.
- Revision detail and history already provide decimal revision numbers. Collection metadata should reuse that source.
- Native chat and shared design-system components own rendering, disclosure, timing and focus.
- Usage currently calls `getModelLibrary` only to build display labels in `settings/billing/lib/load-report.ts`. Its `Promise.all` makes that optional lookup delay the entire report.
- Billing already reads aggregates and paginated activities within one read-only snapshot in `afd_billing/src/report/mod.rs`. Fleet count does not create a loop of report requests.

## Performance requirement for Sections 1, 2 and 3

Claude must investigate the slow first Usage load before selecting a query design. Trace browser requests, parent layout reads, handler work and database plans.
Compare one initial report request with the current report-plus-catalogue dependency. Measure cold and repeat loads with four fleets and larger representative data.

Prefer one coherent scoped read where it supplies complete data efficiently. Compare a set-based SQL projection against a fixed batch of reads using actual query plans.
Do not force unrelated operations into one statement if it duplicates ledger rows, widens access or increases latency. No per-fleet, per-model or per-key request loop is allowed.

Section 1 records the source inventory, timing and query plans. Section 2 owns the one-call Usage response and exact accounting.
Section 3 uses scoped batch metadata reads and measures its call count; it must not restore a separate catalogue dependency to Usage.

## Sections (implementation slices)

### §1 — Reconcile source facts and merged contracts

Inventory each field as available, adapter-only, backend projection, producer change or unresolved design. Record exact files and authorization before editing.

Follow the merged lifecycle and Plugin vocabulary. This workstream does not own those renames.

- **Dimension 1.1** — Every displayed fact names a stored source → Test `ui_projection_inventory_matches_contract` (review): field table cites producer, storage, authorization, adapter, request count and measured query plan.
- **Dimension 1.2** — Unsupported facts remain explicit → Test `unavailable_projection_never_becomes_zero` (unit): omitted facts show unavailable; measured zero remains zero.

### §2 — Complete settled usage (moved to M249_003 §6)

Indy's Nov 14, 2026 request to align Usage with the uploaded proposal moved this slice into the interface follow-up recorded in parked M249_003 §6.
That workstream owns its implementation and verification; the description below records its original scope and is not a second implementation task here.

Extend the existing report with UTC daily buckets and each fleet's complete model set. Apply the same filters and snapshot cutoff as totals.

Use current inclusive time bounds unless the existing contract changes explicitly. Do not silently drop the final timestamp when grouping UTC days.

Use exact integer monetary units. Fill empty days only within an authoritative complete result; distinguish empty results from unavailable data.

Preserve setup charges, null/deleted fleet identities, historical cutoffs and unpriced usage. Never aggregate a loaded activity page into a complete chart.

- **Dimension 2.1** — Daily totals reconcile → Test `daily_spend_reconciles_filtered_snapshot` (integration): midnight, month end, late settlement and filters sum to report totals.
- **Dimension 2.2** — Paging cannot change aggregates → Test `daily_spend_ignores_activity_cursor` (integration): all cursors return identical daily and fleet/model facts.
- **Dimension 2.3** — Historical names survive deletion → Test `usage_keeps_deleted_fleet_models` (integration): deleted fleet charges retain ledger identity and all recorded models.

### §2a — One authoritative initial Usage request

The owner requested one billing-report call instead of fetching model labels through `GET /v1/models` separately.
Use the existing `GET /v1/tenants/me/billing/reports/current` route and `billing:read` authorization. Do not introduce another reporting endpoint.

The report response is unchanged: totals, daily buckets, fleet/model breakdowns, filter choices and the first activity page. Billing carries no display labels.

Model display names come from one shared label store (owner decision, Nov 15, 2026). The dashboard layout reads the platform catalogue once per sign-in or refresh and hands the labels to a client provider.
Next does not re-render a layout on client navigation, so the labels stay in the browser for the session. They arrive in the first server render, so no screen swaps an identifier for a name after paint.

Keep the recorded model identifier as every aggregation key and filter value. Display the label when known, otherwise the identifier. A label never rewrites identity or computes money from current catalogue prices.
If several providers list one identifier, label it only when every listing supplies the same nonempty name. Unknown, retired or ambiguous models show their recorded identifier.

A failed label read leaves identifiers on screen; it never hides or zeroes settled usage, and it never fails the page.
Every screen that names a model reads the same store; the Usage loader makes no catalogue request.

Remove `getModelLibrary` from the Usage loader. The initial Usage-specific service read is one report request, regardless of whether four or more fleets appear.
Common authenticated-shell reads remain separately measured; do not misreport them as per-fleet Usage requests.

The current dashboard layout awaits workspace, balance and invite reads before returning its frame. A page-level Suspense boundary cannot bypass a blocked parent layout.
Include this path in the cold-load profile. Stream confirmed optional blockers with explicit loading states; do not substitute false empty workspace or zero-balance data.

Keep individual charge requests behind an explicit activity disclosure. Pagination and filter changes request the corresponding report page; they preserve the existing snapshot rules.
Render the Usage frame before its data region resolves, using the existing loading indicator and Suspense. Defer collapsed charge-detail code with a client dynamic import.

Use the shared local-time utilities for displayed instants. Accounting bucket identities and stored schedule rules retain their established semantics.

- **Dimension 2a.1** — One initial report request → Test `usage_first_load_has_one_report_request` (browser): four fleets require one report call, no model-catalogue call, no fleet-detail calls and no charge call before disclosure.
- **Dimension 2a.2** — Labels are complete or explicitly unavailable → Test `unknown_model_shows_recorded_identifier` (unit, shared label store): known, advanced, absent, prototype-named and provider-ambiguous models retain correct identifiers and amounts; a failed label read shows identifiers.
- **Dimension 2a.3** — Paging and filters retain accounting → Test `billing_labels_do_not_change_snapshot_totals` (integration): labels, cursors and filter changes preserve the existing complete aggregates and exact cutoff behavior.
- **Dimension 2a.4** — Slow data does not hide the frame → Test `usage_shell_streams_before_report_and_details_load_on_intent` (server-render/browser): delayed report and optional parent-shell reads retain truthful loading states; details load only when opened.
- **Dimension 2a.5** — Performance evidence identifies the wait → Test `usage_four_fleet_request_waterfall` (browser/profile): record first and repeat load, service duration, response size, browser rendering and query plans; separate local controlled delay from live machine evidence.
- **Dimension 2a.6** — Failure remains recoverable → Test `usage_single_report_failure_retains_filters_and_retry` (unit/browser): timeout, malformed JSON, empty data and denied access preserve truthful error or empty states and existing recovery.

### §3 — Safe key metadata and effective secret access

Add safe key display metadata at creation if needed. Never derive a suffix from a digest or return recoverable key bytes.

M249_003 §5.6 already implements the nullable creation-time display suffix. Reuse that field and its proofs; do not add a second suffix producer.

Identify the successful-authentication producer before adding last-used metadata. Define write frequency and ordering there; failed or revoked authentication cannot advance it.

Distinguish never-used from unavailable metadata. Preserve one-time reveal, revocation, paging and account authorization.

Project effective secret access only from the execution policy that actually governs access. A textual secret reference does not grant permission.

Keep workspace-backed storage, hidden values, named-field replacement and active-model protection. New editable secret access rules need separate design approval.

- **Dimension 3.1** — Metadata cannot reveal credentials → Test `api_key_metadata_is_safe_and_account_scoped` (integration): foreign account reads fail; list, errors and logs contain no key or digest; one bounded page read supplies metadata without per-key queries.
- **Dimension 3.2** — Last use reflects accepted authentication → Test `api_key_last_use_requires_success` (integration): successful use advances; failed, revoked and out-of-order writes do not falsify recency.
- **Dimension 3.3** — Effective access stays workspace-scoped → Test `secret_access_projection_matches_execution_policy` (integration): foreign fleets and references alone cannot add access.

### §4 — Revision labels and recorded work

Expose the head revision ordinal in Plugin collection metadata from the existing revision row. Use M249_003's current names and avoid per-card history requests.

Preserve immutable revision identity, complete reusable setups and independently saved fleets.
The separate product discussion may later change that model. Do not infer new connection, resource or action-grant fields before that decision.

Inspect event producers for changed paths, line totals and publication receipts. Add fields only with a confirmed producer and workspace/fleet/run identity.

Local writes are changes, not publication. A related PR URL does not prove this run published an output.

- **Dimension 4.1** — Revision labels use the actual head → Test `plugin_card_ordinal_matches_pinned_head` (integration): list ordinal and head identity agree across publication and deletion.
- **Dimension 4.2** — Work totals require recorded evidence → Test `summary_uses_confirmed_run_evidence` (unit/integration): partial tool text and unrelated URLs cannot become authoritative totals or published outputs.
- **Dimension 4.3** — Reload preserves facts → Test `recorded_summary_survives_reload` (browser): stream and persisted replay render the same confirmed changes and outputs.

### §5 — Measured context budget: parked

The owner parked only this section. Its producer questions, semantics and Dimension 5.1 are preserved in [the parked section](../done/M249_004_SECTION_5_MEASURED_CONTEXT_BUDGET.md).
The remaining recorded-data and one-call Usage sections stay valid.

> Owner: "I think you could move the Dimension 5, in the M249_004 to parked, since the rest of the spec is valid"

### §7 — Acceptance covers the user journeys

The owner asked that dashboard and CLI acceptance cover the product from the user's side, and chose to close every gap the coverage review found in this PR.

Fix the specs this branch outdates: Usage columns and empty daily state, Secrets labels, and the design-coherence access cell.

Add or extend these journeys, each driven the way a user drives it:

1. A run's charge appears in Usage: fleet and model filters, charge details and model display names.
2. Add, replace and delete a secret; "Available to" names the fleet whose setup declares it.
3. Describe a fleet in the browser chat, review the proposal and save it, with the authoring model's name shown.
4. Sign-up, teammate invites, private setup chat and workspace creation run in the development acceptance lane.
5. Exhausted credit: the banner, the balance and refused runs.
6. API keys from the CLI: create, use, revoke and refusal; the dashboard's masked ending matches the key.
7. CLI schedules: create, update, sync and delete.
8. CLI `usage show`: readable rows, paging and the fleet's charge row.
9. Connect an app and disconnect it; the connection-failure page.
10. Deny an approval; search a live fleet's memory.
11. Save a fleet with a schedule; the scheduler's callback wakes it into its chat with a reply, and the user keeps chatting.
12. Refine a saved fleet from the CLI: one improve-chat message, a review showing the new schedule, an explicit apply to the next setup revision, and the schedule listed.

A journey that needs an external provider uses the repository's existing test double or live development credential; none weakens an assertion to pass.

- **Dimension 7.1** — Stale specs match the shipped interface → Test `settings-billing`, `secrets-lifecycle`, `design-coherence` (acceptance): pass against the local stack.
- **Dimension 7.2** — Every listed journey runs end to end → Test the specs named in the PR's acceptance table (acceptance): `make acceptance-local` passes both suites.

### §6 — Implementation boundaries

The UI checkpoint merged in PR #844; no parallel UI worktree remains. This workstream starts from that merge and preserves its presentation, auth, shared progress, time utilities and domain-folder layout.

M249_003 is parked, not pending. Nothing here waits for its merge. Plugin revision labels consume the published-setup representation already on main, and the Plugin UI stays disabled.

The Fleet/template/Plugin reassessment belongs to a separate session. It supplies questions and examples, not capability fields or provider implementation tasks.

Generate OpenAPI once, after the backend wire types settle.

## Interfaces

Extend existing read responses where possible. Record exact wire names, null/absent meanings, units and bounds before implementation.

Generate OpenAPI from Rust and update strict UI decoders together. Preserve existing problems, permissions and paging.

New public endpoints require a demonstrated need and the repository's route inventory, documentation and contract tests.

## Failure Modes

| Mode | Kind | Handling | Test |
|---|---|---|---|
| Paginated activity mistaken for complete usage | Failure | Whole-period server projection; chart unavailable on projection failure | `daily_spend_ignores_activity_cursor` |
| Settlement crosses a cutoff | Failure | Same snapshot and time bounds across daily totals and report | `daily_spend_reconciles_filtered_snapshot` |
| Foreign account or workspace metadata | Security | Existing authorization refuses access before projection | `api_key_metadata_is_safe_and_account_scoped`, `secret_access_projection_matches_execution_policy` |
| Secret reference mistaken for permission | Security | Read governing policy; omit unsupported editable controls | `secret_access_projection_matches_execution_policy` |
| Truncated output or related URL mistaken for publication | Failure | Require confirmed event evidence from this run | `summary_uses_confirmed_run_evidence` |
| Optional catalogue delays the whole report | Performance | Labels come from the shared store; no Usage catalogue request | `usage_first_load_has_one_report_request` |
| Providers share a model identifier | Data integrity | The label store names an identifier only when every listing agrees; otherwise it shows the identifier | `unknown_model_shows_recorded_identifier` |

## Invariants

1. Backend authorization governs every projection; hidden controls do not grant or revoke authority.
2. Credentials remain workspace-backed; no primary vault, copying or secret enumeration layer appears.
3. Exact monetary units and ledger cutoffs remain consistent across the report.
4. No compatibility aliases, backfill, dual reads or guarded cutover. Reset only task-owned development databases.
5. Approved layouts, Manrope, Phosphor and native assistant-ui ownership remain in place.

## Metrics & Observability

Use existing request and producer signals. Do not log raw secrets, key material, tool payloads or private file contents.

Any new producer metric needs bounded labels, an existing signal owner and a test in its implementation Section.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Source inventory and unavailable semantics (§1) | Review field inventory; `make test-unit-all` | No inferred values | P0 | NOT RUN |
| R2 | Complete daily/fleet/model usage (§2) | `make test-integration-rustd` | Same filters and reconciled exact totals | P0 | NOT RUN |
| R2a | One-call Usage and initial load (§2a) | Integration, server-render and browser tests named in §2a | One initial Usage report request; no catalogue dependency | P0 | NOT RUN |
| R3 | Safe scoped metadata and access (§3) | `make test-integration-rustd` | No exposure or authority expansion | P0 | NOT RUN |
| R4 | Authoritative revision/work facts (§4) | `make test-integration-rustd`; `make dry-app` | Confirmed facts survive reload | P1 | NOT RUN |
| R5 | Measured context budget (§5) | Parked separately | No implementation required in this workstream | P1 | DEFERRED at owner request |
| S1 | Declared commands green | `make lint-all`; `make test-unit-all`; `make test-integration-rustd`; `make check-version` | Exit 0 | P0 | NOT RUN |
| S2 | No secrets | `gitleaks detect --redact --no-banner` | Exit 0 | P0 | NOT RUN |
| S3 | Authenticated runtime and repository gates | `make acceptance-codex`; `orly gate pr` | Exit 0 | P0 | NOT RUN |

Keep this spec pending until producer decisions and review blockers are resolved. Move it to done only after its implementation rubric passes.

## Dead Code Sweep

Remove replaced unavailable-only branches after real projections cover them. Preserve unavailable handling for errors and unsupported observations.

Record exact deletions after the merged-base inventory; no implementation deletion is authorized by a guessed symbol name.

## Out of Scope

- Remaining M249_001 visual findings and cleanup: they must finish in the UI PR.
- CLI grouping, discovery and lifecycle: Claude's M249_002.
- Plugin public vocabulary belongs to M249_003. Capability content, resource bindings and invocation require the separate discussion recorded in its §7.
- Fleet-to-fleet steering and activity: M239_001; capability Plugins do not require that delegation feature.
- Real uploads, a new Secrets editor, new secret grants, a second execution engine and platform administration UI.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Existing projections plus scoped producers; Usage first, then metadata, recorded work.
- **Alternatives considered:** Browser aggregation cannot prove complete daily totals. A parallel reporting API duplicates authorization and paging ownership.
- **Patch-vs-refactor verdict:** Extend existing domain reads. Refactor only a producer or projection whose present structure prevents a coherent contract.

## Discovery (consult log)

- **Source:** UI HEAD `1bebcb701e8aba4a368734df51314c7e188f21f9` and working changes; fetched main `4f3159269644b2a1d814fbcc9119f01830b7eaf8`.
- **Coordination:** `origin/feat/m249-cli-api-discovery` already reserves M249_003. This spec uses M249_004 to avoid collision.
- **Model reviews:** Authoring draft; independent EGM review and resolution remain pending. This document carries no implementation-readiness claim.
- **Open blockers:** Exact last-use, effective-access, publication producers need the merged-base inventory. Unknown semantics must be resolved before their Sections start.
- **Merge order (superseded Nov 15, 2026):** UI (#840, #844) and CLI (#839) merged. M249_003 is parked, so M249_004 neither runs alongside it nor waits for its merge.
- **Implementation base (Nov 15, 2026):** `a1dcc222488e4522f8b92208a4f7df8461fe7fbf`, fetched main containing PR #844.
- **One-call Usage request (Nov 14, 2026):** "why the getModelLib is called and we combine the billingreport to just 1 api call? and udpate the spec?" The owner corrected the target to M249_004 and requested this change in the UI PR.
- **Plugin reassessment (Nov 14, 2026):** "not a replacement of a fleet, but an actual Plugin with an use case and fix them". M249_003 defines that behavior; this workstream supplies the corresponding recorded data.
- **Service examples (Nov 14, 2026):** "agentphone, agentcreditcard, agentemail". The owner requested use-case-driven and adversarial thinking; provider scope and invocation remain product decisions rather than implemented facts.
- **Parallel work (Nov 14, 2026):** "claude able to run this and fix in parallel while your UI fix". Section 6 separates independent work from interfaces awaiting product decisions.

- **Latest owner boundary (Nov 14, 2026):** Finish the current work and retain Fleet/template/Plugin reasoning for a separate session. The phone, email and card examples do not expand this implementation scope.

- **Toolchain folded in (Nov 15, 2026):** the owner directed the Rust, Postgres and development-image version moves into this PR rather than a separate worktree.
- **Model labels move to a shared store (Nov 15, 2026):** the owner chose one browser-held label store over a `model_labels` report field, so the billing API stays unchanged. Labels arrive in the first server render, so no visible swap; they load once per sign-in or refresh and stay for the session.
- **Journeys 11 and 12 (Nov 15, 2026):** the owner asked whether a schedule added at setup wakes the fleet into its chat and keeps chatting, and whether a setup can be refined from the CLI; neither was covered, so both join §7.
- **Acceptance scope (Nov 15, 2026):** the owner asked to "ensure your acceptance-local passes and the acceptance of e2e and cli are covered fully from an user experience use case perspecive", then chose "All 10 in this PR" for the coverage review's gaps (§7).
- **Package updates (Nov 15, 2026):** every JavaScript package and every Rust crate moves to its latest admissible release in this PR. `libc` and `parking_lot` 0.13 stay behind because `tokio-quiche` 0.23.0 still caps `libc` below 0.2.190; `quick-xml` and `matchit` follow `object_store` 0.14.2 and `axum` 0.8.9, already the latest.
- **Design sign-off (Nov 15, 2026):** the owner approved the one-request Usage design, the Plugin card ordinal and the secret access projection. API key authentication stays read-only, so Dimension 3.2 records its producer design without adding a write.

> Owner (Indy): "also move rust to 1.1.100, since we have moved up, and ci images, and postgres in your local to 19.rc2 if there is one or later check bun as well and let me know. update the nats, mailpit, qstash too."

> Owner (Indy): "this has to go in this PR agentsfleet-toolchain-bump not a separate worktree and approved."

> Owner (Indy): "For model_labels this may be needed in fleet, fleet setup chat, billing and others - i think. If so you can run this query once async when you load or login (store it in the browsers cache or so -- you tell me how. Then every one who needs it will call this store and use it. ... so the billing just focusses on what it is. If the labels ar enot there its perfectly fine to show the gpt-x"

> Owner (Indy): "A visible swap - i think i prefer this is avoided"

> Owner (Indy): "Keep it on load of a browser when i login or refresh you load once and it stays on"

> Owner (Indy): "can you update to the latest packages"

> Owner (Indy): "I want all the rust crates to be updted in this PR"

> Owner (Indy): "is there an end to end fleet setup to schedule added and save, adn fleet chat gets the scheudler callback and you process in the fleet chat that you go one? and you keep chatting. is there a way to refine the fleet setup in cli, is there an use case for that wheret its done"

> Owner (Indy): "This the `No provider key needed` must be removed, since its misleading and you already say it as Platform default"

> Owner (Indy): "claude to works from your PR merged on the UI refresh you did with gaps delta on api needed on the backend and wire it into the ui you created (post your PR merge)."

> Owner (Indy): "I dont want any of the legacy backfill crap"

> Owner (Indy): "i dont need a guarded cutover"

> Owner (Indy): "there is no primary vault"

- **Usage controls (Nov 15, 2026):** the owner relayed Codex-informed Usage changes: a month/year Period selector, the Fleet default "All Fleets" covering setup and saved-fleet usage with each fleet after it, and no More filters, Charge details or Add credit. A later relay assigned those visual changes to Codex; this workstream keeps the backend, the API and the plumbing that keeps Usage working, and hands Codex the request and response delta.
- **Reader's time zone (Nov 15, 2026), deferred:** I first cut Usage months on UTC without asking, and the owner objected. A zone-cut report (a required IANA `timezone`, `month=YYYY-MM`, local daily buckets) was then built and tested, compared with Fireworks' and Ona's billing APIs, which take a zone, and with OpenAI's and Anthropic's, which stay on UTC. The owner then deferred it: the report stays on UTC for now and the zone work is dropped from this PR. Fleet spend limits also floor their month at UTC (`afd_billing/src/limits/select_fleet_spend.sql:5`).
- **Codex handoff (Nov 15, 2026):** the report contract is unchanged and stays on UTC. Codex's Period selector names a UTC month and asks for it with `period=month&through_at=<that month's last millisecond>`, as main's `previousBillingMonth` does today.
- **Billing refactor (Nov 15, 2026):** the owner asked for a large refactor of the billing area on both sides, for clarity and performance.

> Owner (Indy, relaying Codex): "The Period control: a month/year selector, such as “November 2026”. The filter dropdown’s default text changes exactly “All Fleets”. … The Removal of  “More filters”. The Removal of “Charge details” and “Add credit” from the Usage UI. Their design will be handled later."

> Owner (Indy): "I never agreed for utc"

> Owner (Indy, relaying Codex): "Codex owns the Usage and Security visual changes. Please handle only the backend/API work in M249_004. … Do not reconstruct daily totals from paginated activities or merely relabel existing buckets. Keep the charge API available for its later UI design."

> Owner (Indy): "I donot need your visual edits, so drop that full, donot save that stash or patch. Codex will be responsible for it."

> Owner (Indy): "I think for now use UTC, no big change needed, and push the branch"

> Owner (Indy): "no timezonee" … "indy needs to think more" … "so we test and fix this" … "later"

- **SQL and schema review gate (Nov 15, 2026):** every new or changed statement and every table change is shown to the owner and waits for sign-off before it is written. The two statements already on this branch, `afd_fleet_lifecycle/src/sql/select_secret_access.sql` and the `afd_billing` aggregates rewrite, await the owner's review; a PR opens only after sign-off. This branch changes no table.
- **Test zone (Nov 15, 2026):** pre-push refused the branch on main's own time tests, which assume a UTC machine, from an IST laptop. The owner chose to pin `TZ=UTC` in both Vitest configs over pushing with an environment override.

> Owner (Indy): "I want to review the queries you do and signoff, on the sql, table changes when you start next"

> Owner (Indy): "i still need to review the sql, so i didnt sign off"

> Owner (Indy), choosing among the push options: "Pin TZ=UTC in vitest config"

> Owner (Indy): "in your PR ensure that you do a large refactor on the billing area and see how we can better it. You are free to unleash and embark on a large refactor in Rust side and the NExt js side onthe code to make it better performant too."

## Claude handoff

Read [the handoff prompt](../evidence/M249_004_CLAUDE_HANDOFF.md). Implementation starts from `a1dcc2224`, main containing PR #844.

The Plugin UI and backend implementation are retained for reuse. Do not reactivate the Plugin UI while completing the independent data work.

## Latest implementation boundaries

M249_003 is parked as the rename workstream. Only Section 5 of this workstream is parked; Sections 1, 2, 2a, 3 and 4 remain in scope, and Sections 1–3 retain the performance investigation above.
