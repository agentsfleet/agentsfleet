# M249_004: Complete recorded data and one-call Usage

**Prototype:** v2.0.0
**Milestone:** M249
**Workstream:** 004
**Date:** Nov 13, 2026
**Status:** DONE
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
8. **Surface order** — Backend interface and producer tests first, then adapters and UI in the same follow-up PR.
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
| Rust sources clippy 1.100 rewrites (`map_or_default`, redundant clones, lazy defaults and single-item iterators) | EDIT | Mechanical lint fixes the new toolchain requires |
| Measured Rust tracing emit sites listed in `docs/v2/evidence/M249_RUST_COVERAGE_VALIDATION.md`; workspace manifest commentary | EDIT | Borrowed field shorthand preserves emitted values while repairing Rust 1.100's duplicate log-copy coverage mapping |
| `afr_supervisor/src/verbs/tests/{mod,tasks}.rs`; `afd_authoring/src/model/provider/resolver/{mod,tests}.rs` | ADD/EDIT | Failure tests cover task fencing, single attempts and resolver address policy without changing production behavior |
| `afr_supervisor/src/lease_loop/recovery.rs`, `lease_loop/tests/{mod.rs,recovery/**}`, `test_support/{sandbox,git_fixture}.rs`, `recovery/{transfer.rs,transfer_http/**,restore.rs,tests/**,catalog_tests.rs}`, `verbs/decision/pending/tests.rs` | ADD/EDIT | Exercise recovery ownership, authenticated data, refusals and retained receipts through test-only native, sandbox and transport fixtures; keep production SQL unchanged |
| `afr_agent/src/loop/task_tests/behavior/publication*.rs` | EDIT | Refused receipt reads, lost publication replies and invalid candidates cannot cause an unverified or duplicate publication |
| `afr_agent/src/loop/task_tests/behavior/replies.rs`; `afr_supervisor/src/downlink/policy/{mod.rs,refusal_tests.rs}` | EDIT/ADD | Source replies use native task generations; lost or malformed policy and permanent heartbeat failures stop new work |
| `afr_supervisor/src/recovery/mod.rs`, `verbs/decision/{mod.rs,tests.rs}` | EDIT/ADD | Share only the test HTTPS fixture to verify provider refusal classification and retry budgets without changing the production transport |
| `afr_supervisor/src/authoring/{meter.rs,meter_tests.rs}` | EDIT/ADD | Uncertain or interrupted observations cannot silently become known zero usage; malformed-cost rejection is checked at the leaf |
| `rustd/Cargo.toml`, `rustd/Cargo.lock`; `af_stream/src/h3/{client,server}.rs`; `afd_cron/src/verifier.rs` and the Hash-based Message Authentication Code (HMAC) fixtures that sign with it | EDIT | Every crate to its latest release the dependency graph admits; `tokio-quiche` 0.23 drops `H3Event::NewFlow` and `jsonwebtoken` 12 refuses empty HMAC keys |
| `package.json`, `cli/package.json`, `ui/packages/*/package.json`, `bun.lock`, `cli/bun.lock`, `patches/@assistant-ui%2F{react,store}@*.patch` | EDIT/MOVE | Every JavaScript package to its latest release; both local assistant-ui patches carried forward unchanged |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryView{,.test}.tsx` | EDIT | The Platform default card names its model only; "No provider key needed" misled beside "Platform default" (owner, Nov 15, 2026) |
| `ui/packages/app/tests/e2e/acceptance/**`, `cli/test/acceptance/**` and their fixtures | EDIT/ADD | Stale Usage and Secrets specs, plus the ten user journeys §7 lists |
| `ui/packages/app/lib/model-labels{,-server}.ts`, `components/domain/models/ModelLabels.tsx`, `app/(dashboard)/layout.tsx` and their tests | ADD/EDIT | One shared model-label store, read once per sign-in or refresh, replacing Usage's own catalogue call |
| `docs/architecture/billing_reporting.md` | EDIT | The one-request loader and the label store |
| `ui/packages/app/vitest.config.ts`, `ui/packages/design-system/vitest.config.ts` | EDIT | Pin the test zone to UTC so time tests grade the same off a UTC machine (owner approval, Nov 15, 2026) |
| `schema/20261108124610_billing_usage_daily.sql`, `schema/20261108124620_billing_runs_daily.sql`, `rustd/crates/daemon/afd_db/src/migration.rs` | ADD/EDIT | The daily usage rollup the report reads (owner sign-off, Nov 15, 2026) |
| `rustd/crates/daemon/afd_billing/src/ledger/{settle.rs,mod.rs,sql/settle_lines.sql,sql/add_usage_daily.sql,sql/count_run.sql}` | EDIT/ADD | The settle transaction maintains the rollup and counts each run once, with no lock |
| `rustd/crates/daemon/afd_billing/src/report/**`, `afd_billing/src/tenant/mod.rs`, `afd_billing/tests/**` | EDIT/DELETE | The report reads the rollup; the activity page, charge details and model filter are removed |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/billing_report/**`, `afd_api_tenant/src/{lib.rs,openapi.rs}`, `afd_api_wire/src/billing/**`, `afd_http/src/{route/tenant.rs,services/billing.rs}`, `afd_api/tests/**` | EDIT/DELETE | The report response and query lose `activities`, `model`, `limit` and `starting_after`; the activity-charges route is deleted |
| `rustd/crates/daemon/main/src/sweepers/billing.rs`, `docs/architecture/billing_and_provider_keys.md` | EDIT | The relay settles every 5 s; a refused line still retries within a minute |
| `ui/packages/app/app/(dashboard)/settings/billing/**`, `ui/packages/app/lib/api/{billing_report.ts,billing-report-values.ts}` and their tests | EDIT/DELETE | The dashboard stops decoding and rendering the removed activity list and charge details |
| `rustd/Cargo.toml` and the production code of the crates holding an `option_if_let_else` site | EDIT | Hand-written `if let`/`else` on `Option` becomes the combinator; the lint is denied workspace-wide |
| `docs/v2/evidence/M249_004_SOURCE_INVENTORY.md` | ADD | §1 field inventory, request counts, timings and query plans |
| `rustd/crates/daemon/afd_billing/src/report/{types.rs,queries.rs,sql/aggregates.sql}` | EDIT | Whole-period daily and fleet/model projections |
| `rustd/crates/daemon/afd_api_wire/src/billing/mod.rs` | EDIT | Typed report fields |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/billing_report/{mod.rs,view.rs}` | EDIT | Existing authorized report response |
| `ui/packages/app/lib/api/billing_report.ts` | EDIT | Decode and validate complete projections |
| `ui/packages/app/app/(dashboard)/settings/billing/components/{BillingReportView.tsx,BillingFleetTable.tsx}` | EDIT | Chart and complete fleet/model facts |
| `ui/packages/app/app/(dashboard)/settings/billing/components/{BillingDailySpend,BillingFilters,BillingReportView}.tsx`, `ui/packages/app/app/globals.css`, `ui/packages/design-system/src/design-system/{PageHeader,SectionHeader}.tsx`, `ui/packages/app/components/domain/shared/navigation/CollectionToolbar.tsx`, `ui/packages/app/app/(dashboard)/settings/members/components/{MembersView,TeamTable}.tsx` | EDIT | Match measured chart, header, tablet and Team geometry to the uploaded reference |
| `ui/packages/app/components/domain/setup/summary/SetupSummary.tsx`, both design-system export files, `ui/packages/app/tests/{setup-policy-disclosure.test.tsx,e2e/setup-policy-help.spec.ts}` | EDIT/ADD | Make policy help work by touch and keyboard through the existing dialog primitive |
| `docs/v2/evidence/M249_DESIGN_REVIEW.md` | ADD | Rendered comparisons, independent Astra review, fixes and limits |
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
| `public/openapi.json` | GENERATE | Generated interface document, never hand-edited |
| Existing tests for each changed producer, handler and component | EDIT | Complete data, refusals, races and unavailable states |
| `rustd/crates/daemon/main/tests/support/e2e_event.rs` | EDIT | Give the unsupported-event fixture the normal post-commit wake; retain its refusal, no-offer and two-second assertions |
| `rustd/crates/daemon/afd_db/tests/integration_migrate/roles.rs` | EDIT | Probe the two inherited billing rollup tables under every runtime role and verb; preserve the schema grants |
| `docs/v2/evidence/M249_VERIFICATION.md`, `docs/DESIGN_SYSTEM.md`, `docs/architecture/{api_resources,web_app,billing_reporting,billing_and_provider_keys}.md` | EDIT | Update resolved data differences and retained unknowns; repair parent prose findings without changing billing behavior |
| `docs/v2/evidence/M249_PLUGIN_PRODUCT_REASSESSMENT.md` | EDIT | Preserve the deferred Fleet/template/Plugin questions without adding implementation scope |
| Separate `agentsfleet/docs` branch and changelog | EDIT | Public interface and visible behaviour |
| `deploy/local/**`, `make/acceptance.mk`, `Makefile`, `scripts/build-acceptance.ts`, `scripts/local-dev-state.sh`, `README.md`, `docs/development.md` | EDIT/REMOVE | One `make acceptance` on Linux replaces `acceptance-local` (macOS and OrbStack, removed) and `acceptance-codex` (renamed); bootstrap reads the GitHub App fields where it uses them; orphaned shared services are removed (owner, Nov 16, 2026) |
| `ui/packages/app/app/loading.tsx` (removed), `app/{cli-auth,connectors}/loading.tsx`, `tests/loading-states.test.ts` | REMOVE/ADD | The root loader wrapped the dashboard shell in a streamed boundary that could leave a hidden second shell; the two routes that wait on reads keep their spinner (owner, Nov 16, 2026) |
| `.github/workflows/acceptance-on-demand.yml` | ADD | Run local acceptance on a hosted VM for any branch (owner, Nov 16, 2026) |
| `afd_cron/src/activation.rs`, `afd_authoring/src/creation/{mod,chat_save,schedules}.rs`, `afd_http/src/services/schedule/{mod,plane}.rs`, `afd_api_tenant/src/handler/fleet/native/schedules.rs`, `main/src/plane/{mod,surfaces}.rs`, `afd_api/tests/authoring/saving/chat_save/registration.rs` | EDIT/ADD | A typed "Save" registers its trigger schedules with the scheduler, through the same function as the dashboard save (owner, Nov 16, 2026) |
| `ui/packages/app/components/domain/activity/replies/FleetReplyActions.tsx`, `tests/fleet-thread/role-reply-parts.test.ts` | EDIT | A reply reserves its action bar's slot, so it does not move when it finishes (owner, Nov 16, 2026) |
| `ui/packages/app/components/domain/shared/chat/ChatThread.tsx`, `components/domain/shared/chat/useOpenAtLatest.ts` (new), `components/domain/setup/conversation/DraftConversation.tsx`, their unit tests, `tests/e2e/chat-typography.spec.ts`, `tests/e2e/fixtures/chat-scroll-probe.tsx`, `tests/e2e/acceptance/fleet-thread.spec.ts` | EDIT | Fleet and setup chats anchor each sent message at the top and never move on their own (owner, Nov 16, 2026) |
| `afr_supervisor/src/authoring/{turn,proposal}.rs` | EDIT | Each setup model call and proposal logs a value-free outcome (owner, Nov 16, 2026) |
| `ui/packages/app/tests/e2e/acceptance/**` | EDIT | Acceptance follows the shipped dashboard: the empty composer's start button, display-name member rows, the recovery alert, wide-only header labels, the paused Plugin pages and run figures in Run details (owner, Nov 16, 2026) |
| `docker-compose.yml`, `compose.dev-infra.yml` (removed), `scripts/local-dev-{state,test,live-test}.sh`, `make/test-infra.mk`, `.github/workflows/test-integration-rustd.yml`, `.orly/LOCAL.md`, `docs/architecture/{datastore_scaling,testing}.md`, `afd_bus` NATS doc and exclusive test, `deploy/fly/nats-{dev,prod}/Dockerfile` | EDIT/REMOVE | One datastore compose file for the integration lane and the shared development stack; the NATS nodes are `ant`, `bee` and `elk` (owner, Nov 16, 2026) |
| `playbooks/lib/platform_secret_sync.sh`, `playbooks/lib/platform_secret_sync_test.sh`, `.github/workflows/deploy-dev-verify.yml` | EDIT | Every dev deploy seeds the `typesafe` decision key into the platform workspace (owner, Nov 16, 2026) |
| `deploy/local/machine-env.sh`, `.claude/skills/orly-burst/SKILL.md`, `.agents/skills/orly-burst/SKILL.md`, `.gitignore` | ADD/EDIT | Acceptance on a fresh GitHub runner, driven by the `orly-burst` skill until orly ships `orly burst` (owner, Nov 16, 2026) |
| `rustd/Cargo.{toml,lock}`, `af_core/src/clock.rs` and the jiff call sites | EDIT | jiff and jiff-sqlx 1.0 (owner, Nov 16, 2026) |
| `afr_supervisor/src/authoring/{proposal.rs,proposal_tests.rs,tests/*}`, `afr_supervisor/Cargo.toml` | EDIT/ADD | A refused first setup names its field to the model (owner, Nov 16, 2026) |
| `ui/packages/app/**` confirmed orphaned modules and their isolated tests; `lib/models/display.ts` | REMOVE/EDIT | Owner-requested UI cleanup removes obsolete spending, model cells, action hooks, refresh controls and draft-search state; paused Plugin implementation stays available |
| `ui/packages/app/app/globals.css` unused activity-table selectors | REMOVE | Remove styles for the deleted activity table without changing the reviewed resource layouts |

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
| `make check-version` | Yes | Version and generated interface agreement |
| `make dry-app` and `make acceptance` | Yes | Refreshed adapters and authenticated resource flows |
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

### §1 — Reconcile source facts and merged interfaces

Inventory each field as available, adapter-only, backend projection, producer change or unresolved design. Record exact files and authorization before editing.

Follow the merged lifecycle and Plugin vocabulary. This workstream does not own those renames.

- **Dimension 1.1** — Every displayed fact names a stored source → Test `ui_projection_inventory_matches_contract` (review): field table cites producer, storage, authorization, adapter, request count and measured query plan.
- **Dimension 1.2** — Unsupported facts remain explicit → Test `unavailable_projection_never_becomes_zero` (unit): omitted facts show unavailable; measured zero remains zero.

### §2 — Complete settled usage (moved to M249_003 §6)

Indy's Nov 14, 2026 request to align Usage with the uploaded proposal moved this slice into the interface follow-up recorded in parked M249_003 §6.
That workstream owns its implementation and verification; the description below records its original scope and is not a second implementation task here.

Extend the existing report with UTC daily buckets and each fleet's complete model set. Apply the same filters and snapshot cutoff as totals.

Use current inclusive time bounds unless the existing interface changes explicitly. Do not silently drop the final timestamp when grouping UTC days.

Use exact integer monetary units. Fill empty days only within an authoritative complete result; distinguish empty results from unavailable data.

Preserve setup charges, null/deleted fleet identities, historical cutoffs and unpriced usage. Never aggregate a loaded activity page into a complete chart.

- **Dimension 2.1** — Daily totals reconcile → Test `daily_spend_reconciles_filtered_snapshot` (integration): midnight, month end, late settlement and filters sum to report totals.
- **Dimension 2.2** — Removed with activity paging: the report has no cursor to vary (owner: "Delete both").
- **Dimension 2.3** — Historical names survive deletion → Test `usage_keeps_deleted_fleet_models` (integration): deleted fleet charges retain ledger identity and all recorded models.

### §2a — One authoritative initial Usage request

The owner requested one billing-report call instead of fetching model labels through `GET /v1/models` separately.
Use the existing `GET /v1/tenants/me/billing/reports/current` route and `billing:read` authorization. Do not introduce another reporting endpoint.

The report response carries totals, daily buckets, fleet and model breakdowns and fleet filter choices. It has no activity page and no model filter, and it carries no display labels.

Model display names come from one shared label store (owner decision, Nov 15, 2026). The dashboard layout starts the platform catalogue read once per sign-in or refresh and hands its pending result to the client provider.
Next retains the layout on client navigation, so the browser keeps the shared names for the session.
Only name consumers suspend; the Usage totals and daily chart render from the report while its named fleet table waits.
The table first paints with resolved names or recorded identifiers after a failed label read, without a later identifier/name swap.

Keep the recorded model identifier as every aggregation key and filter value. Display the label when known, otherwise the identifier. A label never rewrites identity or computes money from current catalogue prices.
If several providers list one identifier, label it only when every listing supplies the same nonempty name. Unknown, retired or ambiguous models show their recorded identifier.

A failed label read leaves identifiers on screen; it never hides or zeroes settled usage, and it never fails the page.
Every screen that names a model reads the same store; the Usage loader makes no catalogue request.

Remove `getModelLibrary` from the Usage loader. The initial Usage-specific service read is one report request, regardless of whether four or more fleets appear.
Common authenticated-shell reads remain separately measured; do not misreport them as per-fleet Usage requests.

The current dashboard layout awaits workspace, balance and invite reads before returning its frame. A page-level Suspense boundary cannot bypass a blocked parent layout.
Include this path in the cold-load profile. Stream confirmed optional blockers with explicit loading states; do not substitute false empty workspace or zero-balance data.

The activity page and Charge details were deleted with their route, which now answers `410 Gone` (`UZ-REQ-003`). More filters and Add credit wait for their design.
A filter change drops paging and disclosure keys the report does not read, so an old link adds no request or wait.
Filter changes read one report; the shared label store is reused.
Render the Usage frame before its data region resolves, using the existing loading indicator and Suspense.

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

1. A run's charge appears in Usage: the fleet filter, the by-fleet table and model display names.
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

New public endpoints require a demonstrated need and the repository's route inventory, documentation and interface tests.

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
| R1 | Source inventory and unavailable semantics (§1) | Review field inventory; `make test-unit-all` | No inferred values | P0 | PASS: app unit tests, 4,651 passed (Nov 15); `orly gate pr` counts in the PR body |
| R2 | Complete daily/fleet/model usage (§2) | `make test-integration-rustd` | Same filters and reconciled exact totals | P0 | PASS: `afd_billing` settle, rollup and report suites (47 + 30 + 8 + 1 + 20 passed) |
| R2a | One-call Usage and initial load (§2a) | Integration, server-render and browser tests named in §2a | One initial Usage report request; no catalogue dependency | P0 | PASS: `billing-page-report`, `dashboard-labels-streaming` and report view tests (49 passed after the #845 merge) |
| R3 | Safe scoped metadata and access (§3) | `make test-integration-rustd` | No exposure or authority expansion | P0 | PASS: `api_key_metadata_is_safe_and_account_scoped`, secret access suites |
| R4 | Authoritative revision/work facts (§4) | `make test-integration-rustd`; `make dry-app` | Confirmed facts survive reload | P1 | PASS: recorded in `docs/v2/evidence/M249_POST_MERGE_RUNTIME_VALIDATION.md` |
| R5 | Measured context budget (§5) | Parked separately | No implementation required in this workstream | P1 | DEFERRED at owner request |
| S1 | Declared commands green | `make lint-all`; `make test-unit-all`; `make test-integration-rustd`; `make check-version` | Exit 0 | P0 | `orly gate pr` before the PR; counts in the PR body |
| S2 | No secrets | `gitleaks detect --redact --no-banner` | Exit 0 | P0 | PASS: `gitleaks detect --log-opts=origin/main..HEAD`, 59 commits, no leaks |
| S3 | Authenticated runtime and repository gates | `make acceptance`; `orly gate pr` | Exit 0 | P0 | DEFERRED at owner request: local acceptance stopped at TLS trust (Discovery); `orly gate pr` in the PR body |

The owner closed this spec in its PR with the local acceptance run outstanding; Discovery records why.

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
- **Patch-vs-refactor verdict:** Extend existing domain reads. Refactor only a producer or projection whose present structure prevents a coherent interface.

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
- **Reader's time zone (Nov 15, 2026), deferred:** I first cut Usage months on UTC without asking, and the owner objected. A zone-cut report (a required Internet Assigned Numbers Authority (IANA) `timezone`, `month=YYYY-MM`, local daily buckets) was then built and tested, compared with Fireworks' and Ona's billing APIs, which take a zone, and with OpenAI's and Anthropic's, which stay on UTC. The owner then deferred it: the report stays on UTC for now and the zone work is dropped from this PR. Fleet spend limits also floor their month at UTC (`afd_billing/src/limits/select_fleet_spend.sql:5`).
- **Codex handoff (Nov 15, 2026):** the report request and response are unchanged and stay on UTC. Codex's Period selector names a UTC month and asks for it with `period=month&through_at=<that month's last millisecond>`, as main's `previousBillingMonth` does today.
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

- **Docs-repository branch deferred (Nov 15, 2026):** the public pages for `SecretSummary.available_to` and `PluginCard.head_revision` wait; no `~/Projects/docs` branch in this PR.

> Owner (Indy): "skip docs-repo for now"

- **Load simulation and rollup (Nov 15, 2026):** a 1,000-fleet account's Usage read took ~87 s on the raw ledger; the owner asked for an adversarial review and then a rollup in this PR. The daily rollup reads the same month in ~170–260 ms and matched the raw statement row for row. The pre-group rewrite of the raw statement was reverted: it was slower on realistic short runs.
- **No locks (Nov 15, 2026):** the owner rejected an explicit per-run lock. A run is counted when its smallest runtime slice settles, which only one transaction can do; the adversarial cases are integration tests.
- **Report scope (Nov 15, 2026):** the owner chose to delete the activity list and the charges route, drop the report's `model` filter, keep budget months on UTC with Usage, and move the relay to 5 s.
- **Combinators and Clippy (Nov 15, 2026):** the owner asked for hand-written `if let`/`match` on `Option`/`Result` to become combinators and for Clippy to hold future code to it. `option_if_let_else` is now denied workspace-wide after its 45 sites were rewritten, and orly's FN-RS rule names the method for each intent (orly branch `feat/fn-rs-option-result-combinators`, commit `309eb32`, not yet released).

> Owner (Indy): "rollup table as its own spec - explain me this in ascii pictorial i want this in this spec and PR."

> Owner (Indy): "I do not like locks its pretty scary and locks can cause deadlocks - have we verified that. What ever adversarial tests you conducted must be part of unit and integration tests."

> Owner (Indy), choosing: "5 s", "Drop it", "Delete both", "Keep UTC", then "Signed off" on the rollup SQL.

> Owner (Indy): "Add 10 crates in this PR rewriting hand-written match/if let on Option and Result into the methods from the table, where Clippy doesn't insist. ... record the ones done, and i want the learning at the end so future crates follow that emphasized via orly. The tests can be skipped."

> Owner (Indy): "i think you can beef up clippy as well so the code is tighter"

- **orly 0.30.0 (Nov 15, 2026):** FN-RS with the method table merged as orly PR #78 (`7ed24be`); this repository picks it up on its next pin bump. Clippy also denies `or_fun_call`, `needless_collect`, `iter_on_single_items` and `useless_let_if_seq`; `afd_api_wire/src/page.rs` carries the one `expect` for utoipa's derive. `parking_lot` 0.13 is blocked by `quiche`'s `libc` cap.
- **Run count (Nov 15, 2026):** the owner signed off a second `count_run` condition for a slice logged after a later one settled. It never counted a run whose later slice settled before an already-logged earlier one, which is the relay's normal order, and four tests failed. `count_run.sql` is back to the signed-off single condition; slices are numbered by their start, so the case it guarded cannot occur. A marker column would cover it and needs a schema sign-off.
- **Wallet exhaustion event (Nov 15, 2026):** the owner asked for billing funnel events. No credit purchase path exists, so `credits_purchased` is not sent, and `spend_limit_hit` would fire per refused event. `debit_wallet.sql` returns whether its debit crossed from funded to zero or below (`RETURNING old`/`new`, Postgres 18+, which `uuidv7()` in `insert_fleet_event.sql` already requires), and `Observed` reports `wallet_exhausted` after the commit, carrying the tenant identifier only. The sweepers now share one biller.
- **Settle-sized test batches (Nov 15, 2026):** `funds_plan` and `plans` settled up to 61,440 slices in one statement and hit the lane's 5 s `statement_timeout` under parallel load. They now settle at most 1,024 slices per run, as the relay does.

> Owner (Indy): "Okay add that." and "Okay fix that, if there ia code clean in that area i want you to clean up duplicates, better way of writing the code sql so its performant, and scales code"

- **Review findings (Nov 15, 2026):** gstack review (seven specialists plus an adversarial pass) and three standards reviewers reported. The owner chose every fix, including `410 Gone` for the removed charges route:
  - The rollup tables drop their tenant foreign key, as the ledger does, so a settle for an erased payer no longer rolls back forever (`an_erased_payer_settles_into_the_rollup`). Both drop the unused `DELETE` grant, and tallies are named `*_count`.
  - `aggregates.sql` groups fleet cells and run counts by `fleet_id` instead of a NULL-safe join, which forced a nested loop over every cell and fleet.
  - `filters.models` is removed: the report refuses `model`, so it offers no model choices.
  - `activity_count` now counts settled runs, on the period totals and fleet rows only; it is absent on model and day rows because one run spans several of each. It was a required count of billed work items on every group.
  - `GET /v1/tenants/me/billing/activities/{activity_id}/charges` stays in the route table and answers `410 Gone` with `UZ-REQ-003`.
  - Secrets: "Available to" is answered for custom secrets only, since a provider key reaches a run through the fleet's model; a failed access read leaves access unknown instead of failing the list; a JSON null credential name is skipped.
  - Restored zeroizing of checkpoint data keys in `afr_supervisor` restore, which the `needless_collect` rewrite had dropped.
  - Model labels page the catalogue at 100 per request; `UsageReportView` is removed; acceptance fleets use purpose names and are cleaned up by identifier.
  - Deleted tests of removed features (24 hunks) are kept deleted, justified by the owner's removals; surviving behaviour keeps its tests.
- **PR #845 merged into this branch (Nov 15, 2026):** Codex's visual follow-up was cut before the API cleanup. Its visuals stand; its activity loader, charge-detail helpers and the "Active model filter" banner are dropped, since the report refuses `model` and the banner would claim a filter the totals do not apply.

> Owner (Indy), choosing every listed fix: "Drop tenant foreign key, Drop the DELETE grant, Rename counters to *_count, Hashable join in aggregates.sql", "Remove filters.models, Record activity_count's meaning, Skip null credential names, Answer 410 Gone, not 404", "Drop UsageReportView, Purpose names for test fleets, Add the missing tests, Stop label read blocking", and "Keep — justified" for the deleted tests.

> Owner (Indy): "Yes these are valid findings i recommend you to fix this … in this PR" (provider-key access and the failed access read)

- **Edge-case journeys (Nov 15, 2026):** the owner asked for acceptance that covers repeated saves, a schedule's reply and adding Slack. `setup-revision-journey` saves v1 then v2 on one fleet and proves a stale v1 session is refused (`409 UZ-AUTHOR-002`). `schedule-wake-journey` already waits for the fleet's reply text. `connect-app-journey` adds Slack: the local deployment configures no Slack app, so it asserts the unconfigured state and `503 UZ-CONN-001`; a configured deployment drives connect, consent and disconnect.
- **Open follow-up, not in this PR:** the stale-save refusal reads "Finish or stop the current work before changing setup.", which misdescribes a save from an older revision. Its copy needs its own change.

> Owner (Indy): "also ensure the make acceptance-local an dany other robust journey changes on edges cases like repeated save v1, v2, and setting up schedule and seeing if you received a message back, and adding slack and so on."

- **Closed with local acceptance outstanding (Nov 16, 2026):** `make acceptance-local` built and started the stack, then stopped at the preflight sign-in: `net::ERR_CERT_AUTHORITY_INVALID at https://localhost:45304/sign-in`. 2 failed, 7 passed and 112 did not run. `deploy/local/certificate.sh` adds the local Caddy certificate to the macOS keychain, which can need an approval nobody gave during the unattended run. This branch changes nothing under `deploy/` or the acceptance sign-in fixtures. The new journeys pass their type and lint checks but have not run end to end. A rerun with the keychain approval settles Dimension 7.2.
- **PR #845 landed rebased (Nov 16, 2026):** the base branch received #845 with six commits the earlier local merge lacked. Its fleet-retention fix, supported-key URL filter, unused-code removal and runner tests are kept; `filters.models` stays removed and the data-key wiping in `afr_supervisor` restore stays.

> Owner (Indy): "i think move the spec to done"

> Owner (Indy): "it must be done in the PR"

- **Local acceptance fixed in this PR (Nov 16, 2026):** three changes since the last recorded pass (Oct 22–23) stopped `make acceptance-local` on a Mac. `eca828b09` made the keychain trust temporary and scoped to `localhost`, which Chromium skips, so the browser never trusted the Caddy root. `55a6e51fd` put BoringSSL under the runner stream, which needs cmake and clang the OrbStack machine never installed. M244 dropped dragonfly, and an old container then failed every start as an unexpected service. Every worktree's Caddy now signs with one root kept beside the shared stack, trusted once per machine for the TLS policy; the machine installs both tools, and orphans are removed as `make/test-infra.mk` already does.

> Owner (Indy): "yes go ahead with the fix"

> Owner (Indy): "ensure its permanent, and not ephermeal, you keep handrolling"

> Owner (Indy): "the fix must go in this PR"

> Owner (Indy): "i cant prompt for every tree"

- **Doubled dashboard shell (Nov 16, 2026):** the root `app/loading.tsx` wrapped the dashboard layout in a streamed boundary. When Clerk updated during hydration, React rendered the shell on the client and the server's late copy stayed hidden in the page, duplicating every shell identifier. The root loader is removed; `cli-auth` and `connectors/complete`, the two routes that wait on reads without their own loader, keep the full-screen spinner. The first paint of a dashboard load now waits for the shell's reads.

> Owner (Indy): "Yes remove the app/loading.tsx, is there anyone else using it? ... if so safely remove."

> Owner (Indy): "I want to go with the option on gh" (the on-demand acceptance workflow, in this PR)

- **Setup saves and jiff (Nov 16, 2026):** the daemon refused an invalid first setup with one fixed sentence and the runner told the model only "The proposal is invalid", so the default model guessed until its turn ran out. The runner now proves a first setup with the daemon's validator and returns its message. jiff and jiff-sqlx move to 1.0; `gix-date` still pins jiff 0.2.
- **Burst acceptance (Nov 16, 2026):** `scripts/acceptance-burst.sh` runs `make acceptance-codex` on AWS Spot or the GitHub workflow, assuming the `aws` command-line tool is installed and signed in. The signed toolbox is amd64 only, so AWS uses x86 Spot types with a capacity fallback. E2E Networks waits on how these two work.

> Owner (Indy): "I see jiff is 1.0, is it updated in ours? ... Make this change in this PR"

> Owner (Indy): "Is the setup-chat save failure in acceptance, if so fix it and push and tiggger workflow."

> Owner (Indy): "in this PR lets do AWS spot and Github Actions. Based on how it works we do E2E Networks"

> Owner (Indy): "Build it in this PR"

> Owner (Indy): "ensure its just a script assuming aws is installed." (the separate test harness was removed)

> Owner (Indy): "i think make it as orly-burst" (the skill name)

- **Acceptance runs only on Linux (Nov 16, 2026):** the Mac path failed on Wi-Fi drops, keychain trust and the OrbStack machine, so `make acceptance-local` and its OrbStack, keychain, toolbox-signing and cross-build code are removed. `acceptance-codex` is renamed `make acceptance`, which the on-demand workflow runs. The AWS launcher staged code and results in S3 and kept the 1Password token in SSM Parameter Store, which the owner never agreed to; it is removed, and nothing was created in AWS (`ParameterNotFound`, `NoSuchEntity`, no bucket). AWS Spot and E2E Networks return through `orly burst` over a per-run SSH session that stores nothing (agentsfleet/orly#79). Bootstrap now reads the six GitHub App fields inside `configureConnector`, from the caller when all are set and otherwise with `op`, and only on a fresh acceptance database. Two specs passed display names where a fleet name must be a kebab slug, so draft creation refused them with `UZ-REQ-001`.

> Owner (Indy): "since the mac orbstack isnt workign reliably"

> Owner (Indy): "I dont want too many targets, keep things simple"

> Owner (Indy): "so in the CI which target will run, rename it clearly"

> Owner (Indy): "crap i didnt agree to S3 bucket"

- **Typed "Save" needs the setup edit check (Nov 16, 2026):** a typed "Save" runs one setup turn. The turn completes only when the model answers without a proposal. On "Save" the default model kept proposing edits, and the edit-intent check that refuses them was unavailable (`UZ-AUTHOR-008`), because acceptance never configured a decision model. One failed save made the full eight model calls in 40 seconds and ended `provider_failed`. Before Nov 3 there was no such check, and an accepted proposal ended the turn by itself. Bootstrap now adds a TypeSafe decision model, stores its key in the platform workspace and names it in the runner's policy. TypeSafe answers `jev-latest` with the version that served it, `jev-1.14.0`; the profile asks for `jev-latest` and accepts only that reviewed version, so a new version is ignored until it is added; one probe call per message scored "Save" 0.11 and "Rename it to billing-digest" 0.81.

> Owner (Indy), choosing how typed "Save" is fixed: "Configure the classifier"

> Owner (Indy): "Typed "Save" -  vault updated op://ZMB_CD_DEV/typesafe/credential"

Typed "Save" still depends on the edit check: without a decision model it fails, while the "Save and publish" flap saves directly. The owner kept that behaviour rather than adding a daemon or UI fallback.

> Owner (Indy), on a fallback for typed "Save": "Keep it as now"

> Owner (Indy), on the requested model: "yes ask jev-latest"

- **One datastore compose file (Nov 16, 2026):** `compose.dev-infra.yml` and `docker-compose.yml` declared the same six services with the same images; `docker-compose.yml` also carried an `agentsfleetd` service nothing started. The integration lane and `make up` now share `docker-compose.yml`, every port binds to loopback, and the dev stack reads its service names from its own snapshot, so a stack created before the rename keeps reusing its containers. The deployed NATS images still pin 2.15.2 while the tested stack runs 2.15.3.

> Owner (Indy), on merging the two compose files: "This PR (#846)"

> Owner (Indy): "as opposed to nats-1, 2 3 name good 3 character ones" (chosen: `ant`, `bee`, `elk`)

- **The remaining acceptance failures (Nov 16, 2026):** a triage of run 43851827412 (91 passed, 18 failed) found thirteen tests asserting markup, copy or flows the dashboard had changed, two races with the lazily loaded workspace menu, an unconfigured Linear app, and four product findings. Fixed here: a typed "Save" never registered its schedules (only the dashboard save's native path did); a finished reply jumped 44 px when its action bar mounted. Recorded for later: the chat stops following the bottom on resize while the composer holds text, and a first setup turn in `install-fleet-cli` spends its eight calls; the new per-call events name the cause on the next run.

> Owner (Indy), on the batch: "or doing on a piece meal type of approach." (answered with one batch)

> Owner (Indy), on the resize finding: "I will test and ask for this change if needed."

> Owner (Indy): "I need the acceptance-e2e and accepatance-cli to suceed to figure out accordinly the GH action workflow must succeed"

- **Chat scroll model (Nov 16, 2026):** the thread followed the bottom while a reply streamed, except while the composer held text, so a resize while typing left a reply's last lines under the composer (fleet-thread.spec.ts). The owner compared Claude, which anchors each sent message at the top and lets the reply grow downward, and chose that model: on send the sent turn scrolls to the top over a reserve, streaming and resizes never move the view, Jump to latest follows a long reply, and opening a chat still lands at its latest turn. Open for the owner: on phones the composer is not sticky, so a long reply pushes it below the screen.

> Owner (Indy), on the scroll model: "Match Claude"

- **Phone composer pinned (Nov 16, 2026):** without follow, a long reply pushed the phone composer and Jump to latest below the screen, because the composer was sticky only at 681 px and wider. The owner chose to pin it at every width, which reverses M249 visual decision D2 ("Phone composer scrolls with the page", `docs/v2/evidence/M249_EXCEPTIONS.md`), made while the thread still followed the bottom.

> Owner (Indy), on the phone composer: "Pin it on phones (Recommended)"

> Owner (Indy): "Just donot clutter the code, make it small modular and performant in the Nextjs"

Every dev deploy now seeds the `typesafe` key into dev's platform workspace through `platform_secret_sync.sh`. Dev also needs the decision model in its catalogue and named in its runners' policy before typed "Save" can finish there.

> Owner (Indy): "Typesafe i want yuou to fix it up in the platform-secrets.sh or waht ever key so it gets set in this PR when we merge it goes as secret"

> Owner (Indy), choosing among the push options: "Pin TZ=UTC in vitest config"

> Owner (Indy): "in your PR ensure that you do a large refactor on the billing area and see how we can better it. You are free to unleash and embark on a large refactor in Rust side and the NExt js side onthe code to make it better performant too."

## Claude handoff

Read [the handoff prompt](../evidence/M249_004_CLAUDE_HANDOFF.md). Implementation starts from `a1dcc2224`, main containing PR #844.

The Plugin UI and backend implementation are retained for reuse. Do not reactivate the Plugin UI while completing the independent data work.

## Latest implementation boundaries

M249_003 is parked as the rename workstream. Only Section 5 of this workstream is parked; Sections 1, 2, 2a, 3 and 4 remain in scope, and Sections 1–3 retain the performance investigation above.


### Codex visual and loading follow-up

Originally built on Claude checkpoint `cad48e0beb8d09dc8742baf7137d893aabdd4dea`; the current UI is rebased onto the trimmed report at `6edd08d84`.
The named-month selector uses `period=month` and the existing `through_at` cutoff; no timezone or month query field is introduced.
The owner's timezone deferral quoted above remains in force.

The [uploaded reference](../../ux.html) governs Usage and Security geometry; its sample records are not stored product facts.
All Fleets includes setup and saved-fleet usage, followed by individual fleets.
Old model-filtered URL fields never reach the trimmed report API.
Older month navigation remains reachable with a bounded list.

The shared label request starts alongside the existing shell reads; only the fleet table waits for its promise.
Settled totals and daily bars do not wait for model names. No extra catalogue read is added to Usage.
The UI refuses a report containing more than 31 daily buckets instead of clipping or expanding it.
The owner requested injected-data, 100-fleet, dropdown and performance evidence; see the [runtime results](../evidence/M249_POST_MERGE_RUNTIME_VALIDATION.md).

The visual implementation and its frontend checks are complete in the stacked UI follow-up.
Its inherited Secrets helper import stays in the client-safe types module; the production bundle guard and all route budgets pass.
The rebased dry lane passes 4,575 unit tests and 134 browser tests.
Overall workstream closure remains active, with final root checks and wider review/acceptance limits recorded in the evidence.

The owner expanded this follow-up to repair the remaining Rust coverage failure:

> "Move the test-integration coverage up, so we could merge"

The measured failure is 106,183 of 109,225 lines covered, 97.2149 percent against the unchanged 98 percent floor.
The standalone reproduction establishes false misses in tracing's alternate log expansion.
The repair uses borrowed field shorthand, with the same event names, fields, formatting, levels and logging features.
It changes no billing producer, query or response behavior.
The first local instrumented run passed 7,178 tests but remained below the line floor at 97.3226 percent; the final six-shard CI result is still required.
Exact source scope and results belong in the [Rust coverage evidence](../evidence/M249_RUST_COVERAGE_VALIDATION.md).

The supplied parent removes the Usage activities collection, per-model/day activity counts, obsolete report parameters and the activity-charges route.
The visual follow-up consumes that report, adapts its decoder and controls, and retains one report request per period or fleet change.

The owner requested an elaborate comparison with the uploaded reference and a second GPT-6 Astra review after repairs.
The review covers 13 pages and states across three viewports and both themes, with 156 comparison captures.
Shared CSS corrections retain existing report reads and the label cache.

Astra found one additional touch-help defect, which now uses the existing dialog primitive.
Its focused follow-up passed 24 interaction sequences with unchanged policy state, no writes and no page errors.
The [design review](../evidence/M249_DESIGN_REVIEW.md) records measurements, approved differences and final checks.

> Owner (Indy): "Yes i need you to optimize and provide performant load, especially when you see multiple calls for something"


## Rebased UI cleanup

The owner supplied `6edd08d84` and requested rebasing PR #845 onto it while retaining the reviewed visuals.
The rebased UI consumes the trimmed report without activity details, pagination or model filtering.
It retains the shared label store, one report request per control change and the approved chart geometry.

The owner also requested a search for unused UI files, exports and functionality.
The [cleanup evidence](../evidence/M249_UI_CLEANUP.md) records removed modules, retained reusable code, source review and final checks.
The repeated visual matrix covers all 13 states and both themes after integrating the new API.

The UI delta contains no SQL or table changes relative to the supplied parent.
The parent's two daily rollups and settlement statements remain Claude's implementation.
