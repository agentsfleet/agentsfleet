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
**Status:** PENDING
**Priority:** P1 — current library pages block, morph controls, and collapse failures into empty states
**Categories:** UI
**Batch:** B2 — consumes M143_001 interfaces
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** M143_001 — paged tenant/global models and tier-qualified Fleet summary/detail
**Provenance:** LLM-drafted (Codex, Jul 24, 2026) from Oracle second-pass review
**Canonical architecture:** `docs/architecture/user_flow.md` §8.7 and `docs/AUTH.md` Flow 2

---

## Overview

**Goal (testable):** Model and Fleet pages retain useful accessible content while loading only the selected page/detail and surviving genuine Clerk session scenarios.
**Problem:** Ordinary visits preload catalogues/secrets, controls morph, refresh clears data, and session continuity is not proven across browser engines.
**Solution summary:** Use page/load-more state with current-page-only projection, stable Suspense regions and typed errors, plus a threshold-driven keeper canary across genuine Clerk browser lanes.

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
| `ui/packages/app/lib/api/model_library.ts`; `lib/api/fleet-library.ts`; `lib/api/library-types.ts` | EDIT/CREATE | Exact page/detail/error types. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/page.tsx`; `.../models/actions.ts`; `.../models/loading.tsx`; `.../models/lib/reads.ts`; `.../models/components/ModelCatalogueProvider.tsx`; `.../models/components/ProviderModelSelect.tsx`; `.../models/components/ModelsRegistryTable.tsx`; `.../models/components/AddModelEntryDialog.tsx`; `.../models/components/EditModelEntryDialog.tsx` | EDIT | Registry page/load-more, current-page projection, intent loading. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/page.tsx`; `.../fleets/new/actions.ts`; `.../fleets/new/LibraryCard.tsx`; `.../fleets/new/InstallSourceSelector.tsx`; `.../fleets/new/AddLibraryDialog.tsx`; `.../fleets/new/loading.tsx` | EDIT/CREATE | Stable summary/detail and `readFleetLibraryDetailAction`. |
| `ui/packages/app/lib/auth/client.ts`; `lib/auth/client.test.tsx`; `app/layout.tsx` | EDIT | Threshold-driven keeper decision. |
| `ui/packages/app/playwright.acceptance.config.ts`; `make/acceptance.mk`; `scripts/check-session-keeper-canary.ts`; `scripts/capture-session-keeper-canary.ts` | EDIT/CREATE | Three browser lanes, provisioned capture target, verdict. |
| `tests/e2e/acceptance/settings-models.spec.ts`; `platform-library-onboarding.spec.ts`; `library-session-continuity.spec.ts` | EDIT/CREATE | Authenticated UI/session proof. |
| `docs/architecture/user_flow.md`; `docs/AUTH.md` | EDIT | Paged UI and keeper verdict truth. |

**Scope grading.** Rubric R4 compares `git diff --name-only origin/main` against this table, so every cell is an exact path. Component test files sit beside their component as `<Name>.test.tsx` and are covered by their component's row. A path that turns out to be genuinely required and is missing here is a spec amendment recorded in Discovery, not a silent addition.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — GRD, ASE, DID, PTK, FLL, UFS, TNM, NDC, NLR, NLG, ORP.
- **`dispatch/write_ts_adhere_bun.md`, `docs/DESIGN_SYSTEM.md`, `docs/AUTH.md`** — shape, async, primitives, motion, Clerk ownership.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| ZIG / PUB | no | M143_001 is consumed |
| File & Function Length | yes | focused types/state components |
| UFS | yes | constants for states, thresholds, routes |
| UI Substitution / DESIGN TOKEN | yes | primitives/tokens; stable reduced motion |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no backend/schema changes |

## Prior-Art / Reference Implementations

- **Streaming:** Approvals/Events loaders; **auth:** current `AuthSessionKeeper` and `docs/AUTH.md`; **API:** M143_001.

## Sections (implementation slices)

### §1 — Models use retained page/load-more state

Ordinary Models requests only the first tenant registry page: no global catalogue or secret list. Load-more appends and retains prior rows, while only the current fetched page is projected/decrypted; no action decrypts beyond it. Add/Edit open, focus, and eligible hover prefetch global model pages. Disable hover prefetch for coarse pointers or Save-Data; focus/open still prefetch. Search retains successful rows while revalidating and rejects stale completions.

- **Dimension 1.1** — ordinary/load-more requests and projection are page-bounded → Test `test_models_registry_retains_pages_without_extra_decrypts`
- **Dimension 1.2** — intent prefetch honors pointer/data policy and request ordering → Test `test_model_picker_prefetch_policy_and_latest_result`

### §2 — Fleet summary/detail and failures are progressive

Initial Fleet creation requests one summary page, then load-more retains cards. Selection calls server-only `readFleetLibraryDetailAction(workspaceId,tier,id)`, which mints the JWT and returns M143_001 detail/presence; no initial secret list/decrypt and no token reaches the client. Server-resolved links include tier/id and avoid gallery flash. After valid workspace auth, foreign detail is 404; unauthenticated is 401 and denied workspace is 403. Stable skeletons, stale refresh content, retry, empty, 401, 403, and 404 remain distinct and reduced-motion safe.

- **Dimension 2.1** — summaries append and only selection loads detail → Test `test_fleet_load_more_then_selected_detail`
- **Dimension 2.2** — deep-link/status/loading semantics are exact → Test `test_fleet_deep_link_and_typed_states`

### §3 — Genuine Clerk canary decides keeper state

Run baseline and candidate against the same Clerk environment in desktop Chromium, Firefox, and WebKit. Each cohort has exactly 20 completed attempts for each of five scenarios per lane (100/lane/cohort): session-lifetime continuity, background expiry, offline/online, focus restoration, and resumed Server Action.

**Session lifetime is configuration, not wall clock.** The capture runs against a dedicated Clerk test instance whose session token lifetime is set to the shortest value Clerk permits, and the run records that configured lifetime in report metadata. Every expiry-crossing scenario waits on the configured lifetime, not on a production-length one. This is what makes the matrix runnable at all: 5 scenarios × 20 attempts × 3 browsers × 2 cohorts is 600 attempts, and pinning any expiry scenario to a production-length session makes the capture cost dominated by sleeping rather than testing. Clerk behaviour under test is genuine — real tokens, real refresh, real cookies — only the lifetime is shortened, and the report is invalid if its metadata does not name the instance and lifetime used.

**The decision rule is expressed in counts, because 20 samples cannot resolve a percentage point.** One failure in a 20-attempt cell moves a rate by 5 percentage points, so a "+1.0 percentage point" threshold is finer than the instrument and can never be graded honestly. Per lane/cohort/scenario record completed attempts, unexpected auth failures, recovery-required attempts, recovery successes, refresh-eligible attempts, and duplicate refreshes. `remove` requires, in the candidate cohort, across every lane and scenario:

- **zero** unexpected auth failures, against a baseline that is also zero; if any baseline cell is non-zero the report is invalid and the comparison is abandoned rather than reinterpreted,
- **every** recovery-required attempt recovered, so 20/20 where the denominator is 20 and no shortfall anywhere,
- duplicate refreshes **no greater than** the matching baseline cell in absolute count.

A zero recovery or refresh denominator passes only when its numerator is also zero; otherwise the report is invalid. The checker accepts `remove` only with zero production keeper references and `retain` only when the keeper files and mount are unchanged. After removal, any breached cell restores the mount; the report records a synthetic threshold-breach rollback check plus source diff evidence.

`retain` is always available and is not a failure. If the capture cannot be provisioned, the honest outcome is `retain` with the reason recorded, not a weakened threshold.

Provisioned `make capture-session-keeper-canary BASELINE_REF=origin/main CANDIDATE_REF=HEAD` writes the ignored aggregate JSON; it is not universal CI.

- **Dimension 3.1** — all genuine lanes and lifecycle actions meet sample rules → Test `test_clerk_canary_lane_matrix_is_complete`
- **Dimension 3.2** — checker binds verdict to source state → Test `test_session_keeper_verdict_matches_repository`

## Interfaces

`TenantModelPages = retained rows + current starting_after; projection scope=current response page only`.
`FleetSummaryPages`; `FleetDetail(workspace,tier,id)` with 401/403/404/503.
Deep link: `/w/{workspace}/fleets/new?library_tier=platform|tenant&library_id=<encoded-id>`.

List position survives a reload: the active `starting_after` and `q` are mirrored into the URL as `library_after` and `library_q`, replacing rather than pushing history so load-more does not fill the back stack. A reload, a shared link, or a back navigation from a detail view restores the same page rather than dropping the user at the first one. Absent parameters mean the first page, and an unparseable `library_after` is discarded in favour of the first page rather than surfacing an error — a bad link should still land somewhere useful.
Refresh state: last success plus idle/loading/refreshing/error and typed error.

## Failure Modes

| Mode | Cause | Injection | Handling | Named test |
|---|---|---|---|---|
| Stale search | older request resolves last | deferred promises | ignore stale; retain rows | `test_model_picker_prefetch_policy_and_latest_result` |
| Typed status | 401/403/404 | response fixtures | distinct action/state | `test_fleet_deep_link_and_typed_states` |
| Foreign detail | valid workspace auth, foreign entry | foreign fixture | 404, no enumeration | `test_fleet_deep_link_and_typed_states` |
| Refresh fault | network/503 after success | rejected fetch | stale content + retry | `test_refresh_retains_authorized_content` |
| Offline/background | cookie expiry | genuine clock/network lane | recover or explicit sign-in | `test_clerk_canary_lane_matrix_is_complete` |
| Resumed action | stale auth on mutation | genuine resumed submission | preserve form; no duplicate | `test_clerk_canary_lane_matrix_is_complete` |
| Reduced motion | media preference | emulated media | no shimmer/transform | `test_library_reduced_motion_state` |
| Verdict/source mismatch | remove with refs or retain with diff | checker fixtures/repo scan | nonzero checker | `test_session_keeper_verdict_matches_repository` |

## Invariants

1. Request spies enforce no ordinary global catalogue/secret request and current-page-only decryption.
2. Discriminated types enforce `(tier,id)` everywhere.
3. Reducer tests enforce retained authorized data until successful replacement.
4. Canary checker enforces lane counts and verdict/source consistency.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| session canary report | product/security | scenario completes | cohort, browser, scenario, aggregate outcome/count | no user/workspace/library/secret identifiers | `test_clerk_canary_lane_matrix_is_complete` |
| existing page analytics | product | unchanged | existing allow-list | no new identifiers | `test_models_registry_retains_pages_without_extra_decrypts` |

## Test Specification (tiered)

This table is the complete set. Every row is mandatory, including the failure rows — an agent that implements only the dimension rows ships an incomplete workstream.

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | integration | `test_models_registry_retains_pages_without_extra_decrypts` | prior rows retained; exactly one page request per load-more, asserted by request spy on the API client, no global catalogue or secret request on an ordinary visit |
| 1.2 | browser | `test_model_picker_prefetch_policy_and_latest_result` | coarse/Save-Data hover blocked; focus/open allowed; latest wins |
| 2.1 | integration | `test_fleet_load_more_then_selected_detail` | append summaries; one selected detail; no secret preload |
| 2.2 | end-to-end | `test_fleet_deep_link_and_typed_states` | server selection and exact statuses/no flash |
| 3.1 | end-to-end | `test_clerk_canary_lane_matrix_is_complete` | exactly 20 completed attempts per browser/cohort/scenario, valid denominators, and metadata naming the Clerk instance and configured session lifetime |
| 3.2 | integration | `test_session_keeper_verdict_matches_repository` | valid retain/remove both pass only with matching source |
| — | integration | `test_refresh_retains_authorized_content` | a network or 503 fault after a success keeps the last successful rows on screen and offers retry, never falling back to an empty state |
| — | browser | `test_library_reduced_motion_state` | under `prefers-reduced-motion: reduce` no shimmer or transform runs, and loading remains distinguishable from loaded |
| — | end-to-end | `test_library_list_position_survives_reload` | after load-more, a reload restores the same page from `library_after`/`library_q`; back from a detail returns to that page, not the first; an unparseable `library_after` falls back to the first page without an error state |

**Decryption is asserted indirectly and deliberately.** Decryption happens server-side and is owned by M143_001. Row 1.1 asserts what this workstream controls — the number and shape of requests the UI issues — and treats "no extra decrypts" as a consequence proven by M143_001's `test_tenant_registry_page_is_bounded`. Naming that split here stops an agent from trying to observe decryption from a browser context.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Lazy paged UI tests pass | `bun --cwd ui/packages/app test` | exit 0 | P0 | |
| R2 | Acceptance browser paths pass | `bun --cwd ui/packages/app run test:e2e:acceptance` | exit 0 | P0 | |
| R3 | Captured canary matches the count rule and source | `bun scripts/check-session-keeper-canary.ts --input test-results/session-keeper-canary.json --base origin/main` | exit 0 with source-consistent `decision=remove\|retain`, `rollback_check=pass`, and metadata naming the Clerk instance and configured session lifetime | P0 | |
| R4 | Diff is scoped | `git diff --name-only origin/main` | 0 unlisted paths | P0 | |
| S1 | Repository gates | `make test-unit-all && make lint-all && make harness-verify && gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run verbatim; record ✅/❌ and one decisive line. Either source-consistent canary verdict passes, and `retain` with a recorded reason is a valid P0 pass — the workstream is not blocked on being allowed to delete the keeper.

## Dead Code Sweep

For `remove`, root-wide production `AuthSessionKeeper` references are zero. For `retain`, no keeper file/mount diff is allowed. Removed eager reads and bare Fleet identities have zero production references.

## Out of Scope

- M143_001 implementation and M143_003 evidence.
- Authentication verifier, proxy layer, token, provider, or policy redesign.

---

## Product Clarity (authoring record)

1. **Successful user moment** — rows/cards remain stable while more data arrives and resumed work succeeds.
2. **Preserved user behaviour** — model management, Fleet install, Clerk sign-in, and Server Actions.
3. **Optimal-way check** — remove requests and scope Suspense; animation is not a fix.
4. **Rebuild-vs-iterate** — refactor read/state boundaries, not auth architecture.
5. **What we build** — retained pages, current-page projection, typed states, and genuine canary.
6. **What we do NOT build** — no eager warmup, secret preload, control morph, or token work.
7. **Fit with existing features** — extends route Suspense, Clerk, and M143_001.
8. **Surface order** — UI follows the pinned API.
9. **Dashboard restraint** — only actual loading/error/retry state.
10. **Confused-user next step** — sign in, permission guidance, return/search, or retry.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** model pages, Fleet states, and canary are independent slices.
- **Alternatives considered:** spinners and unconditional keeper removal lack causal proof.
- **Patch-vs-refactor verdict:** **refactor** of read/state boundaries only.

## Discovery (consult log)

- **Consults** — Oracle second-pass blockers incorporated exactly.
- **Metrics review** — privacy-safe aggregate only; funnel unchanged.
- **Skill-chain outcomes** — populated during implementation.
- **Deferrals** — none.
