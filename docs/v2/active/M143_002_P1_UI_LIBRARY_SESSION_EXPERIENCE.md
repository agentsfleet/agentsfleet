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
**Status:** IN_PROGRESS
**Priority:** P1 — current library pages block, morph controls, and collapse failures into empty states
**Categories:** UI
**Batch:** B2 — consumes M143_001 interfaces
**Branch:** feat/m143-library-session-experience
**Test Baseline:** unit=3172 integration=446
**Depends on:** M143_001 — paged tenant/global models and tier-qualified Fleet summary/detail
**Provenance:** LLM-drafted (Codex, Jul 24, 2026) from Oracle second-pass review
**Canonical architecture:** `docs/architecture/web_app.md` (statements 3 and 5, plus its scoreboard) and `docs/AUTH.md` Flow 2. Amended at VERIFY — see Discovery A9.

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

**Scope grading.** Rubric R4 compares `git diff --name-only origin/main` against this table, so every cell is an exact path. Test files are covered by the row of the code they exercise, whether they sit beside it as `<Name>.test.tsx` or in this package's shared `ui/packages/app/tests/` directory — which is where most of this application's tests actually live, a fact the original wording did not anticipate. A path that turns out to be genuinely required and is missing here is a spec amendment recorded in Discovery, not a silent addition.

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

Ordinary Models requests only the first tenant registry page: no global catalogue or secret list. Load-more appends and retains prior rows, while only the current fetched page is projected/decrypted; no action decrypts beyond it. Add/Edit open, focus, and eligible hover prefetch global model pages. Disable hover prefetch for coarse pointers or Save-Data; focus/open still prefetch.

- **Dimension 1.1** — ordinary/load-more requests and projection are page-bounded → Test `test_models_registry_retains_pages_without_extra_decrypts`
- **Dimension 1.2** — intent prefetch honors pointer/data policy and request ordering → Test `test_model_picker_prefetch_policy_and_latest_result`

### §2 — Fleet summary paging and failures are progressive

Initial Fleet creation requests one gallery page, then load-more appends and retains prior cards. Selection renders from the summary already held — it issues no second request, because the retained summary carries every field the install screen reads. Server-resolved links include visibility/id and avoid gallery flash. Unauthenticated is 401 and a denied workspace is 403; a `library_id` absent from the gallery resolves to a not-found selection state that neither enumerates nor errors the page. Stable skeletons, stale refresh content, retry, empty, 401, 403, and not-found remain distinct and reduced-motion safe.

**Amended at PLAN — reconciled to the mechanism M143_001 shipped.** This section was drafted against a server-only `readFleetLibraryDetailAction(workspaceId,tier,id)` returning a separate `FleetDetail`. That route no longer exists: M143_001 satisfied the parent spec's compactness goal by *trimming the summary* rather than by *adding a detail route*, and retired `handlers/library/gallery_detail.zig`, the `workspace_fleet_library_detail` variant, `UZ-LIBRARY-007`, and the OpenAPI operation with it. `router.zig` now asserts the former URL is unrouted and `gallery_keyset_integration_test.zig` pins it to 404 even for a resident entry.

Nothing user-visible is lost, and this was verified rather than assumed. The summary **retains** `requirements` and `required_credentials_reasons` — the two fields that drive the card's credential chips and the ConnectGate copy. `support_files` was the only field detail added over summary, and no component renders it anywhere on any plane. Credential presence does not come from the library payload at all: `InstallStates.tsx:126` passes a `unmet` list resolved against the workspace's connected credentials.

**Consequently `tier` becomes `visibility` throughout this workstream**, matching the shape M143_001 actually serves and the existing `fleet-library.ts` comment ("Each entry carries `visibility`, so the install flow keys the create body off the chosen tier").

**Load-more replaces an exhaustive walk, so it must not silently truncate.** Today `fleet-library.ts` follows `next_cursor` to exhaustion precisely because reading one page "would drop every entry past the server's page size *silently*". Paging the gallery reintroduces that hazard, so the retained-count and remaining-state must be visible to the user rather than implied by a button.

- **Dimension 2.1** — gallery pages append, are retained, and selection issues no further request → Test `test_fleet_load_more_then_selected_summary`
- **Dimension 2.2** — deep-link/status/loading semantics are exact → Test `test_fleet_deep_link_and_typed_states`
- **Dimension 2.3** — a paged gallery never silently hides entries past the loaded page → Test `test_fleet_gallery_paging_discloses_remaining`

### §3 — The session keeper is retained; no canary is built

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

- **Dimension 3.1** — the keeper stays mounted and its unit coverage holds → Test `lib/auth/client.test.tsx` (existing)

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

**Decryption is asserted indirectly and deliberately.** Decryption happens server-side and is owned by M143_001. Row 1.1 asserts what this workstream controls — the number and shape of requests the UI issues — and treats "no extra decrypts" as a consequence proven by M143_001's `test_tenant_registry_page_is_bounded`. Naming that split here stops an agent from trying to observe decryption from a browser context.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Lazy paged UI tests pass | `bun --cwd ui/packages/app test` | exit 0 | P0 | |
| R2 | Acceptance browser paths pass | `bun --cwd ui/packages/app run test:e2e:acceptance` | exit 0 | P0 | |
| R3 | Session keeper retained and unit-covered | `bun --cwd ui/packages/app test lib/auth/client.test.tsx` | exit 0 | P0 | |
| R4 | Diff is scoped | `git diff --name-only origin/main` | 0 unlisted paths | P0 | |
| S1 | Repository gates | `make test-unit-all && make lint-all && make harness-verify && gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run verbatim; record ✅/❌ and one decisive line.

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

  `library_q` was never a second name for `q` — it named a URL parameter that was never built. It is struck here rather than implemented. The Failure Modes row is **retargeted, not deleted**: the latest-wins mechanism it described IS built, as the catalogue provider's generation counter, and keeps its test.

  **Deleting `q` is NOT in this workstream.** It spans Zig handlers, the gallery and model SQL, the keyset cursor's wire format, the published OpenAPI parameter on two paths, and the `q` half of `UZ-LIBRARY-003` — none of which M143_002 touches, all of which would breach its User Interface (UI)-only scope, fire the ZIG and PUB gates its own table marks "no", and collide with M143_003's Zig surface. Recorded here for the follow-up workstream.

- **Amendment A9 (VERIFY) — canonical architecture repointed to `docs/architecture/web_app.md`.** The spec cited `user_flow.md` §8.7, which is about platform-versus-self-managed model and context-cap origin and has nothing to say about library reads or loading. `web_app.md` did not exist when this spec was drafted on Jul 24; it landed Jul 28 in PR #568 and is the correct home. Its statement 3 ("every route paints a shell before it paints data") and statement 5 ("`useEffect` is for subscriptions, not for loading") describe precisely what this workstream implemented, and its scoreboard carries a standing instruction to "re-measure at any milestone that touches the app and update this table in the same diff". Done: `useEffect` 22 → 20, `Suspense` 3 → 5.

  Two measurement notes recorded there rather than silently absorbed. The published Jul 27 figures (23 and 4) do not reproduce — re-running the listed greps against that same commit yields 22 and 3 — so deltas are counted from the measured baseline. And a comment in the new `fleets/new/loading.tsx` originally contained the bare word `Suspense`, which the scoreboard grep counted as a usage; the comment was reworded so the metric keeps meaning what it claims to measure.

- **Decision recorded (Indy, in-session) — §3 uses the Clerk instance in `ui/packages/app/.env.local`.** It is a `pk_test_` development instance. Indy directed that this is the instance to use and that the question is settled; the capture reads the configured session lifetime from Clerk's Backend API at run time and records it, with the instance kind, in report metadata. No further consult on instance provenance.

- **Out of scope, recorded so it is not lost — the `support_files_json` manifest read path is dead weight.** Distinct from support-file *bytes*, which are fully load-bearing: `importer.zig` validates paths and hashes content, and `runner/bundle_extract.zig` untars them into the workspace at execution. The persisted *manifest* is different — SELECTed at four sites (`fleet_library/sql.zig:103,116,242`, `gallery_sql.zig:154`), decoded by `entry_view.decodeSummaries`, projected into the admin catalog response by `catalog.zig:130`, and then rendered by nothing. The code documents its own redundancy: *"The per-file hash stays internal — it is a handle to stored bytes, and no reader needs it"* and *"`sha256` is read and dropped"*. The runner never reads it (`gallery_sql.zig:103` — bytes come from object storage by `content_hash`). Retiring it is a Zig + SQL + response-schema change that would breach this workstream's User Interface (UI)-only scope and collide with M143_003's surface, and it carries an open question (does `sha256` stay as durable provenance, or should the admin screen render a file list instead?). Flagged to Indy; awaiting a decision on whether it becomes its own workstream.

- **Metrics review** — privacy-safe aggregate only; funnel unchanged.
- **Skill-chain outcomes** — populated during implementation.
- **Deferrals** — none.
