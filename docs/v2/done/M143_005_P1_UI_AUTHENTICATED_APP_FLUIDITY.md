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
# M143_005: Authenticated routes stay light and fluid

**Prototype:** v2.0.0
**Milestone:** M143
**Workstream:** 005
**Date:** Jul 25, 2026
**Status:** DONE
**Priority:** P1 — the authenticated shell makes every route pay for broad client ownership
**Categories:** User Interface (UI)
**Batch:** B4 — cross-cutting application gate after M143_002
**Branch:** `feat/m143-authenticated-app-fluidity`
**Test Baseline:** unit=3223 integration=455
**Depends on:** M143_002 — preserve its library states and session-keeper verdict
**Provenance:** Large Language Model (LLM)-drafted (Codex, Jul 25, 2026) from Orly Chief Technology Officer review and production-build evidence
**Canonical architecture:** `docs/architecture/user_flow.md` §8.4; `docs/architecture/product_analytics.md` §Workspace group + person context

---

## Overview

**Goal (testable):** A production build proves the shared authenticated route is at most 250 Kibibytes (KiB) compressed and every inner route adds at most 100 KiB, while browser acceptance proves navigation never blanks useful content or regresses session and workspace creation behaviour.
**Problem:** The framework runtime is already about 134 KiB compressed, the lightest authenticated route is about 283 KiB, and broad client providers make ordinary inner pages hydrate shell code they do not use. Treating 100 KiB as a total-page target would hide framework bytes or force a worse user experience.
**Solution summary:** Measure framework, shared-shell, and route-owned bytes separately; move static dashboard structure back to server ownership; hydrate narrow control islands; and load closed heavy tools on eligible user intent while keeping links, visible data, stable loading regions, and recovery paths immediate.

## Pull Request — PR Intent & comprehension handshake

- **PR title (eventual):** perf(app): bound authenticated route JavaScript
- **Intent (one sentence):** The authenticated application becomes useful quickly and stays immediate across inner navigation without blank fallbacks, action loss, or weakened session continuity.
- **Handshake** — at PLAN, restate Intent and assumptions; mismatch means STOP.

## Implementing agent — read these first

1. `ui/packages/app/app/layout.tsx`, `app/(dashboard)/layout.tsx`, and `components/layout/Shell.tsx` — current provider and client-boundary ownership.
2. `ui/packages/app/components/domain/island-dynamic/**` — existing chunk-splitting wrappers and stable closed-dialog triggers; mounted dynamic wrappers are not yet intent loading.
3. `M143_002_P1_UI_LIBRARY_SESSION_EXPERIENCE.md`, `docs/AUTH.md`, and `docs/DESIGN_SYSTEM.md` — session verdict, workspace continuity, accessibility, and motion rules.
4. `~/Projects/oss/supabase/apps/studio` and its UI packages — hidden-tool dynamic loading and package side-effect declarations; do not copy its broad root provider stack.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `ui/packages/app/package.json`; `bun.lock` | EDIT | Expose the standard `size-limit` command and remove the superseded build fingerprint wrapper and dependency. |
| `ui/packages/app/.size-limit.mjs` | CREATE | Derive framework, shared, and route-owned emitted files from the current Next.js build, then hand those file sets and the four named budgets to `size-limit`. |
| `ui/packages/app/lib/analytics/posthog.ts`; `ui/packages/app/tests/posthog.test.ts` | EDIT | Keep the existing analytics queue active immediately, but defer the 73.3 kB PostHog client import until browser idle so it cannot compete with authenticated startup. |
| `.github/workflows/test.yml` | EDIT | Run the authenticated bundle gate. **Pre-approved by Indy (Jul 25, 2026)** — the implementing agent does not stop to ask again for this one file and this one purpose. Any other workflow edit still needs its own approval. |
| `ui/packages/app/app/(dashboard)/layout.tsx` | EDIT | Replace broad client-shell ownership with a server frame and narrow control islands. |
| `ui/packages/app/components/layout/Shell.tsx` | DELETE | Broad client shell; its structure moves to the server frame and its controls to islands. |
| `ui/packages/app/components/layout/ShellFrame.tsx`; `ShellControls.tsx`; `shell-sidebar-state.ts`; `MobileNavigationDialog.tsx`; `WorkspaceSwitcherMenu.tsx`; `WorkspaceSwitcherTrigger.tsx` | CREATE | Server-rendered frame, narrow controls, one independently testable collapse-state owner, shared stable workspace trigger, and deferred shell menus that replace `Shell.tsx`. |
| `ui/packages/app/components/layout/SidebarNavigation.tsx`; `WorkspaceSwitcher.tsx`; `ClientOnlyAuthUserButton.tsx`; `GettingStartedWidget.tsx` | EDIT | Narrow each to a control island without changing its visible behaviour or eagerly reaching closed menu or tooltip dependencies. |
| `ui/packages/app/components/layout/WorkspaceCreationProvider.tsx` | EDIT | Keep one lightweight client owner shared by the switcher and zero-workspace route while route content passes through as an opaque server-rendered slot. |
| `ui/packages/app/components/domain/island-dynamic/AddModelDialogDynamic.tsx`; `EditModelDialogDynamic.tsx`; `AddFleetDialogDynamic.tsx`; `EditFleetDialogDynamic.tsx`; `AddLibraryDialogDynamic.tsx`; `IntentDialogStatus.tsx` | CREATE | Intent-loaded closed tools and a visible loading/retry shell, with stable triggers and contained recovery. |
| `ui/packages/app/components/domain/island-dynamic/intent-module-loader.ts`; `intent-module-loader.test.ts`; `intent-dialogs.test.tsx`; `intent-model-dialogs.test.tsx`; `intent-shell-controls.test.tsx` | CREATE | One resettable, independently testable loader plus focused wrapper coverage owns cached preload, capability checks, failure, and retry. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider.tsx`; `AddModelEntryDialog.tsx`; `ModelsRegistryTable.tsx`; `ui/packages/app/tests/model-catalogue-provider.test.tsx` | EDIT | Reuse the shared client-capability policy directly instead of retaining a route-specific hover implementation or provider re-export. |
| `ui/packages/app/app/(dashboard)/admin/models/components/ModelsView.tsx`; `CatalogueList.tsx`; `AddModelDialog.tsx`; `EditModelDialog.tsx`; `MakeDefaultDialog.tsx` | EDIT | Keep the visible catalogue eager and detach closed model tools. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView.tsx`; `PlatformCatalogTable.tsx`; `AddFleetDialog.tsx`; `EditFleetDialog.tsx` | EDIT | Keep the visible library table eager and detach closed Fleet tools. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/InstallFleet.tsx`; `InstallSourceSelector.tsx`; `AddLibraryDialog.tsx` | EDIT | Bound install-route ownership while preserving selection and submit state. |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/ApiKeyList.tsx`; `admin/runners/components/RunnerList.tsx`; `RunnerListCells.tsx`; `RunnerDialogs.tsx`; `w/[workspaceId]/settings/models/components/ModelDetailsDialog.tsx`; `w/[workspaceId]/fleets/components/FleetWall.tsx`; `FleetTile.tsx`; `w/[workspaceId]/secrets/components/SecretsList.tsx`; `ui/packages/app/components/domain/EventsList.tsx`; `EventDetailsDialog.tsx` | EDIT | Move tooltip ownership from the shared dashboard root to one route-local list or dialog boundary per surface, including every relative-time tooltip. |
| `ui/packages/app/tests/app-shell-navigation.test.ts`; `dashboard-workspace.test.ts`; `app-components.test.ts`; `coverage-edges.test.ts`; `shell-motion.test.ts`; `island-dynamic.test.ts`; `admin-models-ui.test.ts`; `events-components.test.ts`; `add-template-dialog.test.tsx`; `fleets-install-flow.test.ts`; `ui/packages/app/components/layout/GettingStartedWidget.refresh.test.tsx`; route-component tests beside the affected files | EDIT | Boundary, workspace, tool-loading, navigation, tooltip ownership, file-ownership, motion, and failure proof. |
| `ui/packages/app/tests/e2e/acceptance/dashboard-performance.spec.ts` | CREATE | Browser proof for fluid navigation, intent loading, and lifecycle preservation. Sits beside every existing acceptance spec under `tests/e2e/acceptance/`; a file outside that directory is not collected by the acceptance config. |
| `docs/development.md`; `docs/architecture/user_flow.md`; `docs/architecture/product_analytics.md`; `docs/AUTH.md` | EDIT | Canonical measurement, shell flow, analytics binding, and provider ownership. |

**Scope grading.** Rubric R5 compares `git diff --name-only origin/main` against this table. Component tests may sit beside listed components as `<Name>.test.tsx`. Any newly required path is a spec amendment recorded in Discovery before the edit.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — GRD (ground in source truth), ASE (async safety), FLL (file and function length), UFS (unexplained fixed strings), TNM (test naming), NDC (no dead code), NLR (no legacy retention), NLG (no legacy framing), and ORP (orphan prevention).
- **`dispatch/write_ts_adhere_bun.md`, `docs/DESIGN_SYSTEM.md`, `docs/AUTH.md`** — TypeScript shape, UI primitives, motion, authentication ownership, and session continuity.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| ZIG GATE | no | no Zig files |
| Public Surface (PUB) / Struct-Shape | no | no Zig public surface |
| File & Function Length | yes | split server frame, controls, measurement, and tests by role |
| UFS | yes | named byte units, route groups, budgets, and intent states |
| UI Substitution / DESIGN TOKEN | yes | existing primitives and tokens; no visual redesign |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no backend lifecycle, registry, or schema change |

## Prior-Art / Reference Implementations

- **`agentsfleet` islands:** `ui/packages/app/components/domain/island-dynamic/**` proves chunk separation and stable closed-dialog triggers. Because those wrappers mount during hydration, they are prior art only; true intent loading also requires conditional mounting and an explicit cached preload entry.
- **Supabase Studio:** hidden panels, command tools, and editors load dynamically; its UI packages declare no package side effects. Reuse that narrow-tool pattern, not its broad application provider ownership.
- **Next App Router:** current `app/layout.tsx` and dashboard layout are already server entry points; the refactor restores their intended ownership rather than inventing a parallel shell.

## Sections (implementation slices)

### §1 — Route bytes become a deterministic release signal

The dynamic `size-limit` configuration reads `.next/build-manifest.json`, `.next/app-path-routes-manifest.json`, and every authenticated route's client-reference manifest. It derives startup-critical framework, authenticated shared, and route-owned emitted files automatically; no route or chunk list is maintained in source or tests. The installed `size-limit` and `@size-limit/file` packages own gzip measurement, totals, output, and the nonzero budget verdict. Missing entries, unreadable chunks, duplicate attribution, an unclassified emitted page, or manifest drift fail closed. Generated chunk names are evidence, never pinned source. Continuous Integration (CI) uses the repository's pinned Bun version and frozen lockfile, runs the production build once, and immediately checks that `.next` output.

The production-configured PostHog client is a separate 73.3 kB gzip lazy chunk. It is not startup-critical route JavaScript: analytics configuration and bounded queues become active immediately, while the client import starts only during browser idle. Identity, context, navigation, and product events that arrive first flush in order after the client initializes. This deferred byte cost remains recorded here rather than being hidden inside a route total or falsely described as absent.

- **Dimension 1.1** — route and chunk discovery comes only from the current production build → Test `bun run --cwd ui/packages/app build && bun run --cwd ui/packages/app size` — **DONE**
- **Dimension 1.2** — standard `size-limit` enforcement covers every discovered authenticated route → Test `bun run --cwd ui/packages/app size` — **DONE**

### §2 — The dashboard frame returns to server ownership

Static frame, header, sidebar structure, and page container render from the server layout. Collapse, mobile navigation, active route, workspace switch/create, account, theme, and analytics become narrow islands. A small external collapse-state owner coordinates only the header toggle and desktop sidebar; it never owns route content or asynchronous work. One lightweight `WorkspaceCreationProvider` remains above both `WorkspaceSwitcher` and the route slot so `NoWorkspaceEmptyState` shares the same single-flight controller. Server-rendered route content passes through that client provider as an opaque React Server Component slot: the provider does not import route modules or own layout markup. The split preserves the current Document Object Model (DOM), focus order, mobile close behaviour, scroll position, single-flight creation, close/reopen recovery, late-failure handling, and reduced motion.

- **Dimension 2.1** — route modules and layout markup are no longer owned by the broad client shell → Test `test_dashboard_shell_hydrates_only_interactive_islands` — **DONE**
- **Dimension 2.2** — navigation and workspace creation retain their complete lifecycle → Test `test_shell_navigation_and_workspace_creation_survive_boundary_split` — **DONE**

### §3 — Heavy tools load on intent, not on every visit

Visible tables, cards, selectors, and route data remain in the initial render. Only closed editors, add dialogs, and configuration tools leave the initial entry. One resettable loader abstraction owns cached preload, capability checks, failure, and retry; each wrapper binds that lifecycle to its dynamic component. The parent mounts the component only after open intent. Eligible hover and focus call `preload()` before click; coarse-pointer or Save-Data hover does not speculate, while focus and click still work. Ordinary Next links retain prefetching. A fast click mounts and loads immediately behind a stable inline trigger and never drops the action. A mounted `next/dynamic` wrapper alone does not satisfy this section.

- **Dimension 3.1** — closed heavy tools are absent from initial entries and preload on eligible intent → Test `test_closed_heavy_tools_stay_out_of_initial_entries` — **DONE**
- **Dimension 3.2** — inner navigation preserves useful content and stays within the route limit → Test `test_inner_navigation_preserves_content_and_prefetch` — **DONE**

### §4 — React 19 responsiveness is evidence-driven

Retain `useTransition` where route and mutation work must not block input. Use `useOptimistic` only for reversible operations with a proven rollback; workspace creation is not optimistic because the server assigns its Identifier (ID). Use `useEffectEvent` only where a lifecycle test proves it removes subscription churn. Already-stable external stores are unchanged unless connection-count evidence justifies the edit. React Compiler configuration stays unchanged so transfer and hydration ownership remain the only optimization variables.

- **Dimension 4.1** — transitions and eligible optimistic updates stay responsive and roll back exactly → Test `test_react19_transitions_and_optimistic_rollbacks_are_stable` — **DONE**
- **Dimension 4.2** — navigation creates no duplicate live subscription or analytics lifecycle → Test `test_navigation_does_not_duplicate_live_subscriptions` — **DONE**

## Interfaces

`size-limit` prints one named check per non-empty discovered startup file set and supports its standard `--json` output when machine-readable evidence is needed. A dashboard route with no files above the shared entry needs no separate check because its increment is zero. The only maintained inputs are the four intentional product budgets:

- Framework runtime is always reported and never hidden inside a false 100 KiB claim.
- Authentication route total is at most 225 KiB.
- Command-Line Interface (CLI) authentication route total is at most 240 KiB.
- Dashboard shared total is at most 250 KiB.
- Every dashboard route increment is at most 100 KiB; therefore an inner route total is at most 350 KiB.

Every authenticated route and startup-critical emitted chunk comes from the current build. A missing manifest, malformed entry, unreadable chunk, duplicate public route, unclassified emitted page, or empty authenticated route class is a failure, not a zero. Deferred third-party chunks are disclosed separately and must not begin loading before browser idle.

Golden path: sign-in resolves the M143_002 session state; the server layout fetches workspace/scopes and renders the frame; small controls hydrate; a prefetched link changes the route without unmounting useful shell content; eligible intent preloads a closed tool; submit completes through the existing server action; navigation leaves exactly one live subscription.

## Failure Modes

| Mode | Cause | Injection | Handling | Named test |
|---|---|---|---|---|
| False-small report | manifest entry missing or renamed | remove one emitted manifest from a local build | nonzero exit naming the missing build input | `bun run --cwd ui/packages/app size` |
| Budget regression | shared or route chunk grows | production build exceeds a configured `size-limit` ceiling | nonzero exit with the named check and byte delta | `bun run --cwd ui/packages/app size` |
| Fast click | tool preload not complete | deferred import | stable pending trigger; action retained | `test_closed_heavy_tools_stay_out_of_initial_entries` |
| Tool chunk fault | offline or rejected import | rejected loader | trigger remains; explicit retry | `test_lazy_tool_failure_preserves_trigger_and_retry` |
| Navigation race | older route response settles late | deferred navigation | latest route wins; useful content stays visible | `test_inner_navigation_preserves_content_and_prefetch` |
| Workspace late failure | close/reopen crosses rejected create | deferred action | correct dialog owns error; no duplicate create | `test_shell_navigation_and_workspace_creation_survive_boundary_split` |
| Session mismatch | shell split contradicts M143_002 verdict | retain/remove source fixtures | source-consistent mount or nonzero check | `test_shell_respects_session_keeper_verdict` |
| Duplicate stream | island remount reconnects | connection counter | one registry subscription/connection | `test_navigation_does_not_duplicate_live_subscriptions` |
| Constrained intent | coarse pointer or Save-Data | emulated capabilities | no hover speculation; focus/click works | `test_intent_loading_respects_client_capabilities` |
| Analytics competes with startup | production key makes the PostHog import reachable | hold the browser idle callback | client import stays absent while early identity/context/events queue; callback loads and flushes once | `posthog.test.ts` |

## Invariants

1. Framework bytes remain visible; 100 KiB always means route-owned increment, never total page weight.
2. Visible data and ordinary links remain immediate; no click-only blank route, disabled prefetch, or full-page spinner is introduced.
3. M143_002 owns the session-keeper decision; this workstream preserves either valid verdict.
4. Workspace creation preserves single-flight, idempotency, close/reopen, and late-failure semantics.
5. Closed tools may load later; visible tables, cards, and selectors may not.
6. Route reporting fails closed and never treats missing files as zero bytes.
7. No new mock server or manifest fixture suite is introduced. Bundle proof uses the real production build; any contrary need requires an Indy consult before editing.
8. CI builds and checks in one job under the pinned toolchain; the checker never substitutes a second build.
9. Analytics configuration is immediate, but the PostHog client cannot compete with startup; early bounded work flushes after its idle import.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| app route bundle report | build | production app build completes | generated `size-limit` check name, gzip bytes, limit, pass | no user, workspace, URL query, or chunk source content | `bun run --cwd ui/packages/app size` |
| existing navigation/workspace analytics | product | unchanged user actions | existing allow-list only | no new identifiers or event names | `test_navigation_does_not_duplicate_live_subscriptions` |

## Test Specification (tiered)

Every row is mandatory. Negative rows are not substitutes for Dimension tests.

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | integration | `bun run --cwd ui/packages/app build && bun run --cwd ui/packages/app size` | the adapter discovers current emitted route files without a maintained route or chunk list |
| 1.2 | integration | `bun run --cwd ui/packages/app size` | `size-limit` enforces auth, CLI auth, shared dashboard, and every discovered inner route |
| 2.1 | integration | `test_dashboard_shell_hydrates_only_interactive_islands` | server frame owns route children; narrow islands own only controls |
| 2.2 | end-to-end | `test_shell_navigation_and_workspace_creation_survive_boundary_split` | desktop/mobile navigation, close/reopen, late failure, and single create survive |
| 3.1 | integration | `test_closed_heavy_tools_stay_out_of_initial_entries` | closed tool entries are absent initially; eligible intent requests them |
| 3.2 | end-to-end | `test_inner_navigation_preserves_content_and_prefetch` | no blank frame, ordinary prefetch remains, latest navigation wins |
| 4.1 | integration | `test_react19_transitions_and_optimistic_rollbacks_are_stable` | controls remain responsive; eligible optimistic state restores on rejection |
| 4.2 | browser | `test_navigation_does_not_duplicate_live_subscriptions` | repeated navigation leaves one stream and one analytics lifecycle |
| — | integration | `test_lazy_tool_failure_preserves_trigger_and_retry` | failed chunk leaves an accessible stable trigger and retry |
| — | browser | `test_intent_loading_respects_client_capabilities` | coarse/Save-Data hover is quiet; focus and click load |
| — | integration | `test_shell_respects_session_keeper_verdict` | source state matches M143_002 retain/remove evidence |
| — | unit | `posthog.test.ts` | analytics queues before idle, imports once after idle, and flushes without identity or event loss |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Route report is honest | `bun run --cwd ui/packages/app build && bun run --cwd ui/packages/app size` | exit 0; framework, shared, and every non-empty authenticated route increment printed with its limit | P0 | ✅ framework 135.56 kB; shared 252.11/256 kB; largest increment 74.36/102.4 kB; deferred PostHog 73.28 kB |
| R2 | Fluidity acceptance passes | `bun run --cwd ui/packages/app test:e2e:acceptance -- dashboard-performance.spec.ts` | exit 0; no blank route/action loss/duplicate stream | P0 | ✅ 17/17 browser checks passed |
| R3 | Diff is scoped | `git diff --name-only origin/main` | 0 unlisted paths | P0 | ✅ 0 unlisted paths; no bundle script or fixture path remains |
| S1 | Repository gates pass | `make harness-verify && make test-unit-all && make test-integration && make lint-all && make memleak && make check-version` | exit 0 | P0 | ✅ all listed repository gates passed |
| S2 | Secret scan passes | `gitleaks detect` | exit 0 | P0 | ✅ 3,941 commits scanned; no leaks found |

**Grading protocol (VERIFY):** run commands verbatim; record ✅/❌ and one decisive output line. A report that omits the framework runtime or an authenticated route is a failure even when every printed number passes.

## Dead Code Sweep

Production references to deleted broad shell/provider paths, superseded eager dialog imports, and orphaned loading helpers are zero. Dynamic shims each have a production caller and test.

## Out of Scope

- Marketing-site bundles, visual redesign, API/schema/data-library changes, and authentication policy changes.
- React Compiler configuration; this workstream isolates transfer and hydration ownership.
- New analytics events, third-party performance services, service workers, and mock servers.

---

## Product Clarity (authoring record)

1. **Successful user moment** — the dashboard is useful immediately and an inner route opens without a blank frame.
2. **Preserved user behaviour** — sign-in, workspace creation, desktop/mobile navigation, theme, account, streams, live data, and reduced motion.
3. **Optimal-way check** — reduce client ownership and closed-tool reachability; hiding bytes or delaying visible content is not an optimization.
4. **Rebuild-vs-iterate** — refactor shell and route boundaries while retaining actions, design primitives, and data flows.
5. **What we build** — bundle gate, server-owned frame, narrow islands, intent-loaded closed tools, and lifecycle proof.
6. **What we do NOT build** — no marketing work, visual redesign, auth redesign, mock server, new telemetry, or global compiler flip.
7. **Fit with existing features** — compounds package tree-shaking, existing islands, and M143_002 session/loading semantics.
8. **Surface order** — authenticated UI first because no API or data shape changes.
9. **Dashboard restraint** — no performance badge or decorative loading; only real local pending/retry states.
10. **Confused-user next step** — keep working from stable content, retry the failed tool, or follow the existing sign-in/permission path.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** measurement, shell ownership, closed tools, and React lifecycle evidence are independently verifiable slices.
- **Alternatives considered:** raising budgets hides regressions; click-only loading harms interaction; a global compiler flip obscures ownership; moving authentication providers without M143_002 evidence risks session continuity.
- **Patch-vs-refactor verdict:** **refactor** the authenticated rendering boundaries; isolated lazy imports cannot remove the broad shell cost.

## Discovery (consult log)

- **Production-truth correction** — review found that automatic post-hydration account and onboarding imports were absent from the initial entry manifest even though an ordinary visit fetched them without user intent. Removing those wrappers made the honest shared entry 270.85 kB against the unchanged 256 kB limit, exposing a real 14.85 kB excess.
- **Shared tooltip ownership correction** — production chunk tracing found that the onboarding widget's two icon controls kept the complete tooltip positioning runtime in the shared authenticated entry. The widget now uses the existing design-system button with the same icon styling, accessible label, and browser hover title; route-local data tools keep the richer tooltip primitive.
- **Final interaction review** — the `AddLibraryDialogDynamic` deep-link path first painted a closed trigger while its form module loaded. A red regression reproduced the missing dialog, then the wrapper gained the same open loading/error/retry shell as the other intent-loaded tools. The follow-up design pass also found that route navigation closed a pending workspace menu without clearing its busy trigger; route changes now clear both open and activation state. The focused suites pass 3/3 for each corrected lifecycle.
- **Production analytics reachability correction** — final red-team tracing found the root instrumentation entry automatically imported a separate 73.3 kB gzip PostHog chunk whenever production analytics was configured, although route manifests did not attribute that lazy child to the four startup budgets. The client now waits for browser idle, or 1.5 seconds on browsers without the idle API, while existing bounded identity, context, navigation, and product-event queues remain active immediately; the deferred byte cost stays disclosed above. Red-team follow-up returned no findings.
- **Fail-closed route classification correction** — final red-team review found that both manifest comparisons filtered through the same three known route prefixes, so a future page group could disappear from both sides and evade measurement. The adapter now rejects every emitted non-framework page that does not belong to a measured route class.
- **Browser acceptance** — the exact acceptance lane passes 17 tests: 13 existing authentication/environment checks plus four new fluidity journeys. The new checks cover desktop and mobile navigation, workspace-dialog close/reopen and focus recovery, ordinary route prefetch, zero blank main-region mutations, Save-Data hover restraint with click loading, and a maximum of one live stream across repeated navigation.
- **Tooltip ownership** — production chunk tracing showed the dashboard-root `TooltipProvider` made the positioning runtime reachable from every authenticated route. Review then reproduced Radix rejecting a relative `Time` rendered without a provider and found two initially missed routes: events and secrets. Ownership now sits once at each route-local list or dialog boundary, covering all direct tooltips without mounting a provider per table row. `GettingStartedWidget` stays in the normal dashboard entry and renders nothing until its asynchronous progress read settles; it is not counted as an intent-loaded closed tool.
- **Review hardening** — the standard `size-limit` package evaluates every generated file set against its named current-build budget. Browser evidence treats a replaced or disconnected `<main>` as a failure, and the constrained-intent trigger records its event-local capability decision so Save-Data hover suppression is asserted without a timing window or a loader subscription that can interrupt the click sequence.
- **Review scope and recovery** — the visible `IntentDialogStatus` and shared `WorkspaceSwitcherTrigger` emerged from design review and are explicit production files above. Closed dialogs use the contained loader lifecycle so a rejected import cannot escape into the authenticated frame. The loader clears only the request that actually completed, preserving a retry started synchronously by an error subscriber.
- **Budget trust and build freshness** — Continuous Integration (CI) produces and measures one build in the same job. `size-limit` receives only file sets discovered from that `.next` tree plus four product budgets. There is no saved report, source fingerprint, duplicated route list, or second measurement implementation.
- **Authentication route reachability experiment** — the Next.js analyzer suggested that `lib/auth/client.ts` might combine provider, account, sign-in, and sign-up code. Splitting those adapters increased the measured shared entry from 270.85 kB to 273.60 kB and increased authentication-route totals, proving that Turbopack already tree-shakes the package-level named imports and that extra adapter edges duplicate bytes. The experiment was reverted; `AuthSessionKeeper` and the root provider remain unchanged.
- **Measured shell dependency reachability** — moving persistent markup to `ShellFrame` reduced the compressed authenticated shared entry by only 0.1 KiB because the eager mobile `Dialog` and workspace `DropdownMenu` still made their dependency graphs reachable from every route. The refactor therefore adds stable eager triggers and intent-loaded `MobileNavigationDialog`/`WorkspaceSwitcherMenu` content; this is the smallest measured boundary that can remove those closed dependencies without delaying visible content.
- **Broad-shell test fallout** — `coverage-edges.test.ts` mocks the dashboard layout's current `Shell` import and `shell-motion.test.ts` reads `Shell.tsx` directly. Deleting that broad client module requires both tests to follow the new `ShellFrame`/control ownership; they are added to Files Changed before the edit.
- **Nested workspace-island ownership** — the full unit gate found that `island-dynamic.test.ts` still expected the eager `WorkspaceSwitcher` trigger to own the create-dialog shim. The intent-loaded `WorkspaceSwitcherMenu` now owns that nested dialog, so the structural assertion follows the new owner while retaining its prohibition on a raw static dialog import.
- **Fleet-library async test timing** — the full unit gate also found two component tests that clicked the newly intent-loaded editor and queried its form in the same tick. The production trigger already preserves the click and loads the editor; those assertions now await the dialog field, matching the existing asynchronous add/fetch cases in the same suite.
- **Changed-file coverage** — all 2,016 application assertions passed after the timing fixes, then the 100% changed-file coverage gate rejected untested lazy-wrapper branches. Focused wrapper tests exercise loading, loaded, failure, retry, capability, and callback paths without weakening the gate. Bundle measurement now delegates gzip and limit behavior to `size-limit` and verifies the thin manifest adapter against a real build instead of copied fixtures.
- **Merged coverage edges** — the expanded suite reached 2,070/2,070 passing with complete line and function coverage, then identified five remaining branch edges: coarse-pointer hover on four eager triggers and a failed onboarding read settling after unmount. Focused interaction tests now cover those user-capability and stale-completion paths.
- **Intent test file shape** — the first consolidated interaction suite reached 515 lines. It is split by responsibility into fleet dialogs, model/editor dialogs, and shell controls so each suite stays below the TypeScript file limit and remains independently runnable.
- **Bundle tooling correction** — the first implementation grew from one 488-line checker into nine script and fixture files. That was the wrong ownership boundary: it reimplemented compression, report validation, and limit checks already owned by the installed `size-limit` package.
- **Jul 29 tooling simplification** — Indy challenged the script count, then required test static inputs to reach zero and asked why the existing package was not used. The final shape is one dynamic `.size-limit.mjs`: it discovers current Next.js route file sets, while `size-limit` owns measurement and enforcement. Both custom scripts and both replacement fixture tests are removed; the real production build is the adapter's proof.
- **Jul 29 quality-ceiling review** — Indy asked whether a larger refactor would make the application more optimized, concurrent, performant, fluid, fast, and easy to test. The selected answer is a targeted boundary refactor: server ownership for persistent structure, independent client state owners for unrelated interactions, and one resettable intent-loader lifecycle. A whole-application rewrite loses because the authentication keeper, live-stream registries, Server Actions, and design primitives already provide the required concurrency and recovery semantics; replacing them would increase regression surface without reducing authenticated-route ownership.
- **Consults** — Indy restricted the target to `ui/packages/app`, required fluid inner navigation, prohibited user-experience compromise, and requires consultation before any mock server. Two separate approvals, kept distinct because one does not imply the other: Indy approved the single `.github/workflows/test.yml` edit that adds the bundle gate (Jul 25, 2026), and separately reconfirmed “keep the preapproved bundle size”, meaning the 250/100 KiB limits stand and are not to be relaxed to make the gate pass. Every other CI change remains gated, and a failing budget is fixed by removing bytes rather than by raising a limit.
- **Batch B4 holds two workstreams** — this one and M143_004. They share a batch but no dependency: M143_004 is infrastructure and depends on nothing, this one is UI and depends on M143_002. Either may land first, and neither blocks the other. Orly's production build found about 134 KiB of framework runtime, about 283 KiB for the lightest authenticated route, and about 107–109 KiB of route-owned code on the heaviest admin routes.
- **Metrics review** — build-only aggregate bytes; no product funnel or analytics schema change.
- **Verification delta** — the Zig baseline remains unit=3223 and integration=455 because this is an application-only change. Application coverage grew from 2,004 to 2,031 passing assertions with complete line, branch, function, and statement coverage; browser acceptance adds four fluidity journeys to the 13 existing environment and authentication checks. The temporary bundle fixture assertions are absent by design: the adapter is proven against the real production build and a real missing-manifest injection.
- **Documentation and release surface** — application development, authentication ownership, and architecture pages are updated. Public API, command-line, schema, and documented user behaviour are unchanged, so no external documentation, changelog, or version change is warranted.
- **Skill-chain outcomes** — `/write-unit-test` found no uncovered changed-file path after failure, retry, concurrency, and stale-completion injection; `/write-integration-test` found no new service or real-I/O boundary and the full repository integration suite passed; gstack `/review` completed with no unresolved findings after specialist, red-team, design, and structured Codex follow-up.
- **Deferrals** — none.
