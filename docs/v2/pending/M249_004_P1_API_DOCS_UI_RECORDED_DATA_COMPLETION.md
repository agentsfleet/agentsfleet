# M249_004: Complete recorded UI data

**Prototype:** v2.0.0
**Milestone:** M249
**Workstream:** 004
**Date:** Nov 13, 2026
**Status:** PENDING
**Priority:** P1, approved screens need authoritative data
**Categories:** API, DOCS, UI
**Batch:** B3, data work may run alongside M249_003
**Depends on:** M249_001 and M249_002 before implementation; M249_003 before final Plugin wiring and this workstream's merge
**Provenance:** Agent-generated from the UI data inventory and Indy's requested Claude handoff
**Canonical architecture:** `docs/DESIGN_SYSTEM.md`; `docs/architecture/context_lifecycle.md`; `docs/architecture/api_resources.md`

## Overview

**Goal (testable):** The refreshed UI displays complete recorded usage, key metadata, secret access, revision labels, changes, outputs and context usage without inferred facts.

**Problem:** Several approved components have no authoritative backing field or complete projection. They currently show supported facts or explicit unavailable states.

**Solution summary:** Extend existing producers and read responses where facts exist, then wire the refreshed components. Retain unavailable states where a producer remains undefined.

**Design flow:** Authenticated read → existing account/workspace authorization → stored ledger, policy or event facts → typed response → existing UI adapter → approved component.

This spec ships as documentation in the UI PR. Its implementation uses a separate worktree and PR after both dependency merges.

Usage, key metadata, secret access, recorded work and context measurement can proceed while M249_003 runs.
Finish Plugin revision-label wiring against M249_003's merged names and payloads. Merge M249_003 before M249_004.

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
5. `rustd/crates/daemon/afd_api_wire/src/library/output.rs` — current revision identity and collection metadata; follow M249_003 renames.

## Product Clarity (authoring record)

1. **Successful user moment** — Usage totals reconcile with its daily chart; resource and Summary labels describe actual recorded facts.
2. **Preserved user behaviour** — Create/revoke keys, replace secrets, select revisions, inspect activity and use model credentials in their existing workspace.
3. **Optimal-way check** — Extend existing scoped reads rather than adding parallel APIs or reconstructing authoritative totals in the browser.
4. **Rebuild-vs-iterate** — Extend projections and their producers. Keep the approved component structure.
5. **What we build** — Daily usage and fleet/model projections, safe metadata and supported policy/event/context read fields with UI adapters.
6. **What we do NOT build** — Capability plugins, real uploads, mention invocation, platform administration or a new secret authorization model.
7. **Fit with existing features** — Use the existing settled ledger, effective policies, immutable revisions and native chat ownership.
8. **Surface order** — Backend contract and producer tests first, then adapters and UI in the same follow-up PR.
9. **Dashboard restraint** — Unknown data stays unavailable. A related link, configured capacity or secret reference cannot prove a recorded outcome.
10. **Confused-user next step** — Show the specific unavailable fact; retain existing activity details, resource management and supported retry actions.

## Files Changed (blast radius)

Paths reflect UI candidate `1bebcb701` plus its working changes. Re-inventory after dependency merges; M249_003 may rename library modules.

| File or family | Action | Why |
|---|---|---|
| This spec and its eventual `done/` path | EDIT/MOVE | Record field decisions, tests and completion |
| `rustd/crates/daemon/afd_billing/src/report/{types.rs,queries.rs,sql/aggregates.sql}` | EDIT | Whole-period daily and fleet/model projections |
| `rustd/crates/daemon/afd_api_wire/src/billing/mod.rs` | EDIT | Typed report fields |
| `rustd/crates/daemon/afd_api_tenant/src/handler/tenant/billing_report/{mod.rs,view.rs}` | EDIT | Existing authorized report response |
| `ui/packages/app/lib/api/billing_report.ts` | EDIT | Decode and validate complete projections |
| `ui/packages/app/app/(dashboard)/settings/billing/components/{BillingReportView.tsx,BillingSpending.tsx}` | EDIT | Chart and complete fleet/model facts |
| `rustd/crates/daemon/afd_api_wire/src/tenant.rs` and `afd_api_tenant/src/handler/tenant/api_key.rs` | EDIT | Safe account-key metadata |
| `ui/packages/app/lib/api/api_keys.ts` and `app/(dashboard)/settings/security/components/ApiKeyList.tsx` | EDIT | Metadata without credential exposure |
| `rustd/crates/daemon/afd_api_wire/src/secret.rs` and existing secret handlers | EDIT | Effective-access read projection, if defined |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/secret-row-cells.tsx` and its existing management dialog | EDIT | Available-to facts from existing policy |
| `rustd/crates/daemon/afd_api_wire/src/library/output.rs` and existing collection projection | EDIT | Head revision ordinal without per-card requests |
| `ui/packages/app/lib/api/library-types.ts` and the refreshed Plugin rows | EDIT | Decode and display authoritative revision metadata |
| `ui/packages/app/components/domain/chat/{WorkContextSections.tsx,recorded-changes.ts,ChatComposer.tsx}` | EDIT | Confirmed event facts and measured context usage |
| Identified key-authentication, event-publication and context-usage producers | EDIT | Record missing facts at their actual owner |
| Canonical schema files, if new durable facts are needed | EDIT | Fresh-development schema; no migration bridge |
| `public/openapi.json` | GENERATE | Generated contract, never hand-edited |
| Existing tests for each changed producer, handler and component | EDIT | Complete data, refusals, races and unavailable states |
| `docs/v2/evidence/M249_VERIFICATION.md` and `docs/DESIGN_SYSTEM.md` | EDIT | Update resolved data differences and retained unknowns |
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

## Sections (implementation slices)

### §1 — Reconcile source facts and merged contracts

Inventory each field as available, adapter-only, backend projection, producer change or unresolved design. Record exact files and authorization before editing.

Follow the merged lifecycle and Plugin vocabulary. This workstream does not own those renames.

- **Dimension 1.1** — Every displayed fact names a stored source → Test `ui_projection_inventory_matches_contract` (review): field table cites producer, storage, authorization and adapter.
- **Dimension 1.2** — Unsupported facts remain explicit → Test `unavailable_projection_never_becomes_zero` (unit): omitted facts show unavailable; measured zero remains zero.

### §2 — Complete settled usage

Extend the existing report with UTC daily buckets and each fleet's complete model set. Apply the same filters and snapshot cutoff as totals.

Use current inclusive time bounds unless the existing contract changes explicitly. Do not silently drop the final timestamp when grouping UTC days.

Use exact integer monetary units. Fill empty days only within an authoritative complete result; distinguish empty results from unavailable data.

Preserve setup charges, null/deleted fleet identities, historical cutoffs and unpriced usage. Never aggregate a loaded activity page into a complete chart.

- **Dimension 2.1** — Daily totals reconcile → Test `daily_spend_reconciles_filtered_snapshot` (integration): midnight, month end, late settlement and filters sum to report totals.
- **Dimension 2.2** — Paging cannot change aggregates → Test `daily_spend_ignores_activity_cursor` (integration): all cursors return identical daily and fleet/model facts.
- **Dimension 2.3** — Historical names survive deletion → Test `usage_keeps_deleted_fleet_models` (integration): deleted fleet charges retain ledger identity and all recorded models.

### §3 — Safe key metadata and effective secret access

Add safe key display metadata at creation if needed. Never derive a suffix from a digest or return recoverable key bytes.

Identify the successful-authentication producer before adding last-used metadata. Define write frequency and ordering there; failed or revoked authentication cannot advance it.

Distinguish never-used from unavailable metadata. Preserve one-time reveal, revocation, paging and account authorization.

Project effective secret access only from the execution policy that actually governs access. A textual secret reference does not grant permission.

Keep workspace-backed storage, hidden values, named-field replacement and active-model protection. New editable secret access rules need separate design approval.

- **Dimension 3.1** — Metadata cannot reveal credentials → Test `api_key_metadata_is_safe_and_account_scoped` (integration): foreign account reads fail; list, errors and logs contain no key or digest.
- **Dimension 3.2** — Last use reflects accepted authentication → Test `api_key_last_use_requires_success` (integration): successful use advances; failed, revoked and out-of-order writes do not falsify recency.
- **Dimension 3.3** — Effective access stays workspace-scoped → Test `secret_access_projection_matches_execution_policy` (integration): foreign fleets and references alone cannot add access.

### §4 — Revision labels and recorded work

Expose the head revision ordinal in Plugin collection metadata from the existing revision row. Use M249_003's merged names and avoid per-card history requests.

Preserve immutable revision identity, complete reusable setups and independently saved fleets.

Inspect event producers for changed paths, line totals and publication receipts. Add fields only with a confirmed producer and workspace/fleet/run identity.

Local writes are changes, not publication. A related PR URL does not prove this run published an output.

- **Dimension 4.1** — Revision labels use the actual head → Test `plugin_card_ordinal_matches_pinned_head` (integration): list ordinal and head identity agree across publication and deletion.
- **Dimension 4.2** — Work totals require recorded evidence → Test `summary_uses_confirmed_run_evidence` (unit/integration): partial tool text and unrelated URLs cannot become authoritative totals or published outputs.
- **Dimension 4.3** — Reload preserves facts → Test `recorded_summary_survives_reload` (browser): stream and persisted replay render the same confirmed changes and outputs.

### §5 — Measured context budget

Identify the runtime measurement producer and applicable model limit before exposing a percentage. Define units, observation time and run/model identity.

Output tokens, cost and configured capacity alone cannot prove remaining context. Model changes, compaction and stale measurements must invalidate incompatible observations.

If no producer exists, keep the unavailable state and record the required producer design. Do not invent a context accounting subsystem silently.

- **Dimension 5.1** — Budget requires a coherent observation → Test `context_budget_requires_matching_fresh_measurement` (unit/integration): stale, mismatched and absent observations show unavailable; valid zero remains zero.

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
| Model changes invalidate a context observation | Failure | Mark unavailable until a matching observation arrives | `context_budget_requires_matching_fresh_measurement` |

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
| R3 | Safe scoped metadata and access (§3) | `make test-integration-rustd` | No exposure or authority expansion | P0 | NOT RUN |
| R4 | Authoritative revision/work facts (§4) | `make test-integration-rustd`; `make dry-app` | Confirmed facts survive reload | P1 | NOT RUN |
| R5 | Measured budget or explicit unavailable state (§5) | `make test-unit-all`; `make dry-app` | No invented percentage | P1 | NOT RUN |
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
- Plugin public vocabulary and platform catalogue removal: Claude's M249_003.
- Capability composition and invocation: M239_001; no `@deploy_status` installation here.
- Real uploads, mention invocation, a new Secrets editor, new secret grants and platform administration UI.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Existing projections plus scoped producers; Usage first, then metadata, recorded work and context.
- **Alternatives considered:** Browser aggregation cannot prove complete daily totals. A parallel reporting API duplicates authorization and paging ownership.
- **Patch-vs-refactor verdict:** Extend existing domain reads. Refactor only a producer or projection whose present structure prevents a coherent contract.

## Discovery (consult log)

- **Source:** UI HEAD `1bebcb701e8aba4a368734df51314c7e188f21f9` and working changes; fetched main `4f3159269644b2a1d814fbcc9119f01830b7eaf8`.
- **Coordination:** `origin/feat/m249-cli-api-discovery` already reserves M249_003. This spec uses M249_004 to avoid collision.
- **Model reviews:** Authoring draft; independent EGM review and resolution remain pending. This document carries no implementation-readiness claim.
- **Open blockers:** Exact last-use, effective-access, publication and context producers need the merged-base inventory. Unknown semantics must be resolved before their Sections start.
- **Merge order:** Both UI and CLI must merge before implementation. CLI may merge first only with coherent current UI lifecycle callers.
- **Staggered work:** Independent data Sections may run alongside M249_003. Final Plugin wiring and M249_004's merge follow M249_003.

> Owner (Indy): "claude to works from your PR merged on the UI refresh you did with gaps delta on api needed on the backend and wire it into the ui you created (post your PR merge)."

> Owner (Indy): "I dont want any of the legacy backfill crap"

> Owner (Indy): "i dont need a guarded cutover"

> Owner (Indy): "there is no primary vault"

## Claude handoff

Read [the handoff prompt](../evidence/M249_004_CLAUDE_HANDOFF.md). Start from main containing both merged dependencies, never from this unmerged UI snapshot.
