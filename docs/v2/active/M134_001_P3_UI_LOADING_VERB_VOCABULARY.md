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

# M134_001: Dashboard loaders speak a waiting vocabulary, not "Loading"

**Prototype:** v2.0.0
**Milestone:** M134
**Workstream:** 001
**Date:** Jul 19, 2026
**Status:** IN_PROGRESS
**Priority:** P3 — cosmetic polish on a shipped, working loader; nothing is broken today
**Categories:** UI
**Batch:** B1 — standalone; no other workstream touches the loading chrome
**Branch:** feat/m134-loading-verbs
**Test Baseline:** unit=2795 integration=369
**Depends on:** none
**Provenance:** agent-generated (pre-spec, Indy chat request Jul 19, 2026 — "use several random words like claude uses for Loading")
**Canonical architecture:** `docs/DESIGN_SYSTEM.md` — Spinner is the system's indeterminate loading affordance

---

## Overview

**Goal (testable):** every dashboard route loader renders one randomly-picked present-participle verb plus the route title ("Wrangling Fleets…") while announcing the stable accessible name "Loading Fleets".
**Problem:** every wait in the dashboard reads the same static word. A static "Loading" carries no signal about whether the product is working or wedged, and the loader is one of the most-seen surfaces in the app.
**Solution summary:** a small waiting vocabulary in the app layer, plus a client-only label component that freezes one verb at mount. `RouteLoading` and the title-less workspace-home fallback consume it. The `Spinner` label prop widens from `string` to `ReactNode` so the design-system primitive stays server-renderable while the impure pick is isolated in a client leaf. Visible text becomes playful; the announced accessible name stays plain.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(ui): give dashboard loaders a waiting vocabulary
- **Intent (one sentence):** a wait should read as the product doing something, without costing screen-reader users clarity or the route title its anti-wobble guarantee.
- **Handshake** — restated: replace the one static loader word with a random verb per mount, keep the route title, keep the announced name plain. `ASSUMPTIONS I'M MAKING: 1. The verb is picked once per loader mount and never rotates on a timer (Indy chose "one random verb per page load"). 2. The route title stays in the visible phrase — the verb prefixes it, it does not replace it. 3. Vocabulary flavour is mixed fleet-metaphor + whimsical (Indy's choice). 4. Screen-reader output must not change; whimsy is visual only.`

## Implementing agent — read these first

1. `ui/packages/app/components/layout/RouteLoading.tsx` — the shared fallback every titled route delegates to; the single edit point for titled loaders.
2. `ui/packages/design-system/src/design-system/Spinner.tsx` — the loading primitive; its `label` vs `srLabel` split is what makes a stable accessible name possible.
3. `ui/packages/app/tests/loading-states.test.ts` — the existing per-segment loader ledger; explains why each loader paints its own title (no wobble on navigation).
4. `docs/DESIGN_SYSTEM.md` — Spinner-vs-Skeleton selection rule; this spec must not blur it.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/components/layout/loading-verbs.ts` | CREATE | The vocabulary plus the three pure helpers (pick, visible phrase, accessible name). |
| `ui/packages/app/components/layout/LoadingVerbLabel.tsx` | CREATE | Client leaf that freezes one verb at mount and owns the hydration escape hatch. |
| `ui/packages/app/components/layout/RouteLoading.tsx` | EDIT | Renders the verbed label; pins `aria-label` to the plain name. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/loading.tsx` | EDIT | Title-less dashboard fallback gets the bare-verb slot. |
| `ui/packages/design-system/src/design-system/Spinner.tsx` | EDIT | `label` widens `string` → `ReactNode` so a client label can be passed in. |
| `ui/packages/app/tests/loading-verbs.test.ts` | CREATE | Vocabulary, picker, phrase, accessible-name, and render coverage. |
| `ui/packages/app/tests/fleets-routes.test.ts` | EDIT | Existing loader assertion pinned the retired static copy; re-pinned to the new contract. |
| `ui/packages/app/app/(dashboard)/admin/models/loading.tsx` | EDIT | Comment-only: its claim about the rendered spinner text was made false by this diff (RULE NLR). |
| `ui/packages/app/app/(dashboard)/admin/runners/loading.tsx` | EDIT | Comment-only: same stale claim about the rendered spinner text. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (the vocabulary and both copy templates are named constants, never inline literals), **NDC** (no dead code: the retired static strings leave no orphan), **ORP** (orphan sweep on the removed copy).
- **`dispatch/write_ts_adhere_bun.md`** — TS FILE SHAPE at PLAN; `const` discipline; UI Component Substitution (reuse `Spinner`, do not hand-roll loader markup).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` in the diff | N/A |
| PUB / Struct-Shape | no — no Zig pub surface | N/A |
| File & Function Length (≤350/≤50/≤70) | yes | Every touched file stays far under 350; largest new file is the vocabulary module. |
| UFS (repeated/semantic literals) | yes | `LOADING_VERBS` plus `loadingPhrase` / `loadingAccessibleName` centralise every string; no call site inlines copy. |
| UI Substitution / DESIGN TOKEN | yes | Existing `Spinner` primitive reused unchanged in shape; no new class strings, no arbitrary `*-[...]` utilities added. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | No logging, no lifecycle, no error codes, no schema in the diff. |

## Prior-Art / Reference Implementations

- **Reference:** `ui/packages/design-system/src/design-system/Spinner.tsx` — the `label` (visible) / `srLabel` (announced) split already models "visible text ≠ announced text"; this spec extends that idea with `aria-label` rather than inventing a parallel mechanism.
- **Reference:** `ui/packages/app/app/(dashboard)/w/[workspaceId]/loading.tsx` — establishes that a multi-route fallback must not claim a route name; the title-less verb slot preserves that.

## Sections (implementation slices)

### §1 — The waiting vocabulary

A single module owning the verb list and the copy templates, so no call site inlines a loader string and the list can grow without touching components. **Implementation default:** every verb is present-participle and object-free, because each must read correctly in BOTH slots — "Wrangling Fleets…" and a bare "Wrangling…"; a verb needing an object strands the title-less fallback.

- **Dimension 1.1** — the vocabulary is duplicate-free and large enough that repeats are uncommon → Test `test_vocabulary_is_unique_and_deep`
- **Dimension 1.2** — every verb matches the present-participle, capitalised shape → Test `test_verbs_read_as_english_in_both_slots`
- **Dimension 1.3** — the picker only ever returns a vocabulary member, and every member is reachable → Test `test_picker_covers_whole_vocabulary`
- **Dimension 1.4** — visible phrase includes the title when present and never leaves a dangling space when absent → Test `test_phrase_titled_and_titleless`

### §2 — The client-frozen pick

The impure pick is isolated in one client leaf so the design-system primitive and the route fallbacks stay server-renderable. **Implementation default:** freeze the verb in mount state rather than rotating on a timer — Indy's choice, and a rotating word would retext a `role=status` live region on a timer and force assistive tech to re-announce mid-wait.

- **Dimension 2.1** — the verb is chosen once per mount and does not change afterwards → Test `test_verb_frozen_at_mount`
- **Dimension 2.2** — a server/client disagreement on the word does not log a hydration mismatch → Test `test_no_hydration_warning_on_verb_mismatch`

### §3 — Route loaders adopt it without losing what they guarantee

Both loader shapes consume the vocabulary while keeping their existing promises: the titled fallback still paints its exact route title (no header wobble), and the multi-route fallback still claims no route name. **Implementation default:** pin the announced name with `aria-label` on the status element, so the visible whimsy never reaches assistive tech.

- **Dimension 3.1** — a titled loader renders exactly one verb plus its route title → Test `test_route_loading_renders_verb_and_title`
- **Dimension 3.2** — the announced name stays the plain wording and contains no verb → Test `test_accessible_name_is_plain`
- **Dimension 3.3** — the title-less fallback renders a bare verb and no route name → Test `test_titleless_fallback_claims_no_route`

## Interfaces

```
// ui/packages/app/components/layout/loading-verbs.ts
export const LOADING_VERBS: readonly string[]        // present-participle, capitalised, object-free
export type LoadingVerb = (typeof LOADING_VERBS)[number]
export function pickLoadingVerb(): LoadingVerb       // impure; callers freeze in mount state
export function loadingPhrase(verb: string, title?: string): string
        // title → "Wrangling Fleets…"   no title → "Wrangling…"
export function loadingAccessibleName(title?: string): string
        // title → "Loading Fleets"      no title → "Loading"

// ui/packages/app/components/layout/LoadingVerbLabel.tsx   ("use client")
export function LoadingVerbLabel(props: { title?: string }): JSX.Element

// ui/packages/design-system/src/design-system/Spinner.tsx  (widened, backwards-compatible)
label?: ReactNode   // was: string
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Hydration mismatch | Server streams verb A, client picks verb B on a hard page load | Intended; `suppressHydrationWarning` on the text node. React keeps the server word, no console error. |
| Screen-reader whimsy | Visible verb reaching assistive tech | `aria-label` on the status element pins the announced name; the visible phrase is never the accessible name. |
| Dangling separator | Title-less slot formatting the phrase with an empty title | `loadingPhrase` branches on falsy title → bare "Wrangling…", asserted for both `undefined` and `""`. |
| Off-by-one picker | `Math.floor(random * length)` indexing the vocabulary | Boundary test mocks `Math.random()` at 0 and 0.999… and pins first/last verb; no `undefined` reachable. |
| Route title wobble | A verb replacing rather than prefixing the title | The title stays rendered in `PageHeader`; the verb only prefixes the spinner phrase. |

## Invariants

1. No loader copy is inlined at a call site — every visible/announced loader string comes from `loading-verbs.ts`. Enforced by test: the render assertions match against `LOADING_VERBS` members, so an inlined literal fails the "exactly one verb" count.
2. The announced accessible name contains no vocabulary verb. Enforced by `test_accessible_name_is_plain`, which asserts the name against every member of the list.
3. Every vocabulary entry is present-participle and capitalised. Enforced by a regex assertion over the whole list, so a future append cannot break the title-less slot.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | cosmetic copy change to an existing loader; no funnel, view, or action event added, renamed, or removed | none | none | none |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_vocabulary_is_unique_and_deep` | Set size equals list length; length ≥ 10. |
| 1.2 | unit | `test_verbs_read_as_english_in_both_slots` | Every entry matches capitalised present-participle shape. |
| 1.3 | unit | `test_picker_covers_whole_vocabulary` | 200 draws all in-vocabulary; 5000 draws reach every entry; `Math.random()`→0 gives first, →0.999… gives last (never `undefined`). |
| 1.4 | unit | `test_phrase_titled_and_titleless` | `("Wrangling","Fleets")`→`"Wrangling Fleets…"`; `("Wrangling")` and `("Wrangling","")`→`"Wrangling…"`. |
| 2.1 | unit | `test_verb_frozen_at_mount` | Re-rendering the mounted label yields the same verb it first rendered. |
| 2.2 | unit | `test_no_hydration_warning_on_verb_mismatch` | Hydrating server markup whose verb differs from the client pick logs no React mismatch error. |
| 3.1 | unit | `test_route_loading_renders_verb_and_title` | `RouteLoading{title:"Fleets"}` markup contains "Fleets" and exactly one `"<verb> Fleets…"`; retired static copy absent. |
| 3.2 | unit | `test_accessible_name_is_plain` | `aria-label="Loading Fleets"` present; name contains no vocabulary verb. |
| 3.3 | unit | `test_titleless_fallback_claims_no_route` | Workspace-home fallback has `aria-label="Loading"` and exactly one bare `"<verb>…"`. |
| regression | unit | `loading-states.test.ts` (existing, unchanged) | Every segment loader still paints its own route title — the anti-wobble guarantee survives. |
| regression | unit | `fleets-routes.test.ts` (re-pinned) | Fleets loader keeps `role=status` and the branded WakePulse dot; announced name is the plain wording. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A titled loader shows a random verb + its route title (§1, §3) | `cd ui/packages/app && bunx vitest run tests/loading-verbs.test.ts` | exit 0 | P0 | ✅ `Tests 14 passed (14)` |
| R2 | Announced name stays plain for both loader shapes (§3) | `cd ui/packages/app && bunx vitest run tests/loading-verbs.test.ts -t "Accessible"` | ≥1 passed, 0 failed | P0 | ✅ `Tests 2 passed \| 12 skipped (14)` |
| R3 | No loader inlines a static label at a call site (Invariant 1) | `grep -rnE '[^-]label="Loading' ui/packages/app` | 0 matches | P1 | ✅ `0 matches` |
| R4 | Diff stays inside Files Changed | `git diff --cached --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | ✅ 10 paths = 9 table rows + this spec |
| S1 | App suite passes | `cd ui/packages/app && bunx vitest run` | exit 0 | P0 | ✅ `Tests 1637 passed (1637)` |
| S2 | Design-system suite passes (Spinner prop widened) | `cd ui/packages/design-system && bunx vitest run` | exit 0 | P0 | ✅ `Tests 461 passed (461)` |
| S3 | Types clean in both packages | `cd ui/packages/app && bunx tsc --noEmit` | exit 0 | P0 | ✅ no output |
| S4 | Lint clean | `make lint-apps-ds-ctl` | exit 0 | P0 | ✅ `Lint passed` ×3 (app, design-system, agentsfleet) |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` |
| S6 | No oversize source file | `git diff --cached --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ❌ `553 ui/packages/app/tests/fleets-routes.test.ts` — **pre-existing** (549 at `origin/main`); see Discovery gate-flag triage, awaiting Indy's call |
| S7 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | ✅ `0 matches` |
| S8 | CONFORM gates green | `make harness-verify` | ALL GATES GREEN | P0 | ✅ `ALL GATES GREEN` (9 gates) |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| static loader label `label="Loading…"` at loader call sites | `grep -rnE '[^-]label="Loading' ui/packages/app` | 0 matches |

## Out of Scope

- **Non-route loading text.** The "Load more" pagination button (`FleetWall.tsx`) and in-button spinners keep their existing copy — a whimsical verb on a button the user just clicked reads as a bug, not charm.
- **Rotating the verb on a timer.** Explicitly rejected by Indy in favour of one pick per mount.
- **Skeleton loaders.** The detail/settings routes use `Skeleton`, which has no text slot; the Spinner-vs-Skeleton selection rule is unchanged.
- **Localisation of the vocabulary.** The product has no i18n layer today; adding one for loader copy would be the tail wagging the dog.

---

## Product Clarity (authoring record)

1. **Successful user moment** — Indy navigates to Fleets, the page takes half a second, and the loader says "Mustering Fleets…". Next navigation it says "Herding Fleets…". The wait feels like the product working rather than the page stalling.
2. **Preserved user behaviour** — every loader still paints its exact route title instantly (no header wobble); screen-reader users hear the identical "Loading <route>" they heard before; the branded WakePulse dot and Spinner-vs-Skeleton split are untouched.
3. **Optimal-way check** — the most direct shape: one vocabulary module, one client leaf, two call sites. The gap to unconstrained-optimal is that a hard page load reuses the server's word rather than re-picking on the client; re-picking would either flash a placeholder or spam a live region, both worse than a slightly less-random first paint.
4. **Rebuild-vs-iterate** — iterate. The loading chrome was consolidated recently and works; this changes one string source, not the structure.
5. **What we build** — a verb vocabulary with pure copy helpers, a client label that freezes one pick, and the `aria-label` pin on both loader shapes.
6. **What we do NOT build** — timer rotation, per-route bespoke vocabularies, button/inline loader copy changes, i18n.
7. **Fit with existing features** — compounds with the recent loader consolidation (one `RouteLoading` for every titled route) — that is precisely why this is a two-call-site change. It must not destabilise the anti-wobble title guarantee, which the existing `loading-states.test.ts` ledger pins.
8. **Surface order** — UI-only; the CLI has its own spinner conventions and is untouched.
9. **Dashboard restraint** — no new controls or claims; this changes copy on an existing affordance only.
10. **Confused-user next step** — none needed; the phrase always names the route being loaded, and the announced name is unchanged.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections split by concern — pure vocabulary (testable without React), the impure client-frozen pick (where hydration risk lives), and adoption at the two call sites (where the existing guarantees must survive). The split keeps all the randomness in one leaf.
- **Alternatives considered:** (a) pick the verb at module scope — one word per JS bundle load, so every navigation in a session shows the same verb; rejected as barely-random. (b) rotate on a timer — rejected by Indy, and it retexts a live region on a schedule. (c) put the vocabulary in the design system — rejected: it is product copy, and `design-system` stays copy-free.
- **Patch-vs-refactor verdict:** this is a **patch** because the loading chrome's structure is already right; only its string source changes. The one structural edit — widening `Spinner.label` to `ReactNode` — is a strict, backwards-compatible widening that every existing caller satisfies unchanged.

## Discovery (consult log)

- **Consults** — Product-shape consult with Indy (Jul 19, 2026): asked rotation-vs-frozen and vocabulary flavour. Indy chose **one random verb per page load** (over rotate-with-title and rotate-without-title) and **mixed** fleet-metaphor + whimsical flavour (over either alone). No architecture consult needed — no stream, channel, namespace, queue, or schema is named by this diff.
- **Metrics review** — no analytics/funnel playbook update required: the diff changes loader copy only and adds, renames, or removes no event.
- **Skill-chain outcomes** — pending: `/write-unit-test`, `/review`, `kishore-babysit-prs`.
- **Gate-flag triage (LENGTH, pre-existing)** — `ui/packages/app/tests/fleets-routes.test.ts` is 553 lines, over the 350 cap. It measured **549 lines at `origin/main`**, so the overage predates this spec; the diff adds 4 lines to one existing assertion. Splitting an unrelated 549-line test file is outside this spec's Files-Changed scope and would bury a P3 copy change under a test refactor. Flagged for Indy rather than silently absorbed or unilaterally fixed.
- **Non-vacuity check (test rigour)** — `test_no_hydration_warning_on_verb_mismatch` initially passed with `suppressHydrationWarning` removed, i.e. it proved nothing: the assertion ran `JSON.stringify` over a console.error arg list containing an `Error`, which serialises to `{}` and swallowed the message. Fixed to stringify with `String()` and to additionally assert the server's word survives in the DOM. Re-verified by mutation: the test now fails with the escape hatch removed and passes with it present.
- **Ordering deviation (process)** — code was written before the spec was committed, inverting the required CHORE(open)-then-EXECUTE order. No scope was lost (the spec documents what shipped and every Dimension carries a test), but the sequence did not follow `AGENTS.md`. Recorded rather than concealed.
- **Deferrals** — none.
