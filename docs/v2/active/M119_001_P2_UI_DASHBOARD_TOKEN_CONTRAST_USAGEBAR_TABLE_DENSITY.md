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

# M119_001: Dashboard token contrast, a UsageBar primitive, and DataTable sticky headers

**Prototype:** v2.0.0
**Milestone:** M119
**Workstream:** 001
**Date:** Jul 07, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — dashboard visual polish, benchmarked against PlanetScale's dashboard; no functional or data-model change.
**Categories:** UI
**Batch:** B1 — standalone UI workstream; independent of M117/M118.
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, this session's design proposal, agreed by Indy)
**Canonical architecture:** no architecture change — UI tokens + presentation only. Model of record: `docs/architecture/billing_and_provider_keys.md` §6 (confirms the credit-exhausted UX contract touched by §2 is unchanged).
**Branch:** feat/m119-dashboard-polish
**Test Baseline:** unit=2377 integration=255 (via `make _lint_zig_test_depth`; UI-only scope — Zig counts are the pin, not expected to move)

---

## Overview

**Goal (testable):** Dashboard cards read defined at rest (no hover needed) via two token-value bumps; the bespoke `.app-meter` markup in `BillingBalanceCard` is replaced by a reusable `UsageBar` design-system primitive; `DataTable` gains an opt-in sticky header and a one-notch-tighter default row density, with zero visual regression to its five existing consumers.

**Problem:** A design proposal benchmarked against two PlanetScale dashboard screenshots found the `design-system` already has most of the right bones (`StatusCard`, `DataTable`, `DashboardRow`, `Badge`/`StatusPill`) but three concrete gaps: (1) card/table borders only become legible on hover — `--border` sits ~4% luminance above `--surface-1`, so the resting state reads flat; (2) there is no reusable usage/quota-meter primitive — `BillingBalanceCard.tsx` hand-rolls one via a bespoke `.app-meter` CSS class in `globals.css`, used nowhere else; (3) `DataTable`'s `<thead>` scrolls away with the body on any list longer than a screenful, losing column context.

**Solution summary:** Bump two dark-mode token values (`--border`, `--surface-1`) in `tokens.css` so every existing `border-border`/`bg-card` consumer gets a crisper resting state for free — no per-component migration. Extract a `UsageBar` primitive from the existing `.app-meter` pattern into `design-system`, migrate `BillingBalanceCard` to it, and delete the now-orphaned CSS. Add an opt-in `stickyHeader` prop to `DataTable` (bounded-height scroll container, sticky `<thead>`) and tighten the default cell padding one notch — both default-off/minimal so existing consumers (`ApiKeyList`, `CatalogueList`, `RunnerList`, `SecretsList`, `BillingUsageTab`) are unaffected except the padding change, which is uniform and intentional.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(m119): bump token contrast, extract UsageBar primitive, add DataTable sticky header
- **Intent (one sentence):** Three dashboard visual-polish deltas — crisper resting-state borders, a reusable usage-meter component replacing a one-off, and a table that keeps its header in view.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/design-system/src/tokens.css` (dark block, ~line 25-33) — the two token values to bump; `theme.css` maps them into Tailwind's `@theme inline` so no component edits are needed for §1.
2. `ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` (~line 42-60) + `ui/packages/app/app/globals.css` (~line 130-146, `.app-meter`/`.app-meter > span`/`@keyframes meter-fill`) — the exact bespoke pattern §2 extracts and replaces; the mount-fill animation + reduced-motion carve-out already exists here and should move with the primitive, not be dropped.
3. `ui/packages/design-system/src/design-system/StatusCard.tsx` — reference shape for a small, presentational, RSC-safe `design-system` primitive (props in, no state, no `asChild`).
4. `ui/packages/design-system/src/design-system/DataTable.tsx` (~line 46-73) — the existing optional-prop pattern (`isLoading`, `hideOnMobile`) to mirror when adding `stickyHeader`; confirm the wrapper's `overflow-x-auto` interaction with a new bounded-height scroll container before wiring `position: sticky`.
5. `ui/packages/design-system/src/theme.css` (~line 91-99) — `--color-pulse`/`--color-pulse-dim` are already mapped Tailwind tokens (used by the existing meter fill); `--accent` maps to `--surface-3` for the track. No new tokens needed for `UsageBar`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/design-system/src/tokens.css` | EDIT | Bump dark-mode `--border` and `--surface-1` for resting-state card/border contrast (§1) |
| `ui/packages/design-system/src/design-system/UsageBar.tsx` | CREATE | New primitive extracted from the existing `.app-meter` pattern (§2) |
| `ui/packages/design-system/src/index.ts` | EDIT | Export `UsageBar` |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx` | EDIT | Replace hand-rolled label/percentage/`.app-meter` markup with `<UsageBar>` (§2) |
| `ui/packages/app/app/globals.css` | EDIT | Remove `.app-meter`/`.app-meter > span`; retarget the `meter-fill` keyframe + reduced-motion carve-out to the primitive's class hooks (§2) |
| `ui/packages/design-system/src/design-system/DataTable.tsx` | EDIT | Add opt-in `stickyHeader` prop (keyboard-reachable scroll region); tighten default cell padding one notch (§3) |
| `ui/packages/app/app/(dashboard)/settings/billing/components/BillingUsageTab.tsx` | EDIT | Wire `stickyHeader` on the paginated usage-history table — its real consumer (§3, added at EXECUTE per RULE HLP — see Discovery) |
| `**/*.test.tsx` (co-located) | EDIT/CREATE | Cover the new token values, `UsageBar`, the migrated `BillingBalanceCard`, and `DataTable`'s sticky-header behaviour |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE HLP (don't ship `UsageBar` without a consumer — `BillingBalanceCard` is the consumer, in the same diff); RULE NDC (the orphaned `.app-meter` CSS is fully deleted, not left dead); RULE NLR (touch-it-fix-it — while migrating `BillingBalanceCard`, confirm no other bespoke meter markup exists elsewhere); RULE UFS (no new string-literal duplication; `ui/` carve-out means this is a manual check, not automated).
- **`dispatch/write_ts_adhere_bun.md`** — TS FILE SHAPE DECISION for the new `UsageBar.tsx` (component, no bound state → functions-module/component verdict, mirrors `StatusCard.tsx`); DESIGN TOKEN GATE (no arbitrary hex/utility — track uses `bg-accent`, fill uses the existing mapped `pulse`/`pulse-dim` tokens); UI Component Substitution Gate (reuses existing primitives; no raw-HTML alternative introduced).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI Substitution / DESIGN TOKEN | yes | `UsageBar` track/fill resolve only to existing mapped tokens (`bg-accent`, `pulse`/`pulse-dim`); no arbitrary Tailwind values |
| File & Function Length (≤350/≤50/≤70) | yes | All edits are small, in-place; no file approaches the cap |
| UFS | yes | No repeated literal strings introduced; manual `ui/` carve-out check |
| ZIG / SCHEMA / LOGGING (Zig) / ERROR REGISTRY | no | No backend, no schema, no Zig |

## Prior-Art / Reference Implementations

- **Reference (the exact pattern to extract):** `BillingBalanceCard.tsx` + `globals.css` `.app-meter` — the current bespoke meter is the direct source for `UsageBar`'s visual contract (8px track, full-pill radius, mint gradient fill, mount-fill animation, reduced-motion carve-out); this is extraction, not new design.
- **Reference (primitive shape):** `StatusCard.tsx` — small presentational component, `HTMLAttributes` passthrough, no `asChild`, RSC-safe.
- **Reference (opt-in prop precedent):** `DataTable.tsx`'s existing `isLoading`/`hideOnMobile` optional props — `stickyHeader` follows the same additive, default-off shape.

## Sections (implementation slices)

### §1 — Resting-state border and surface contrast ✅ DONE (all Dimensions)

Bumps two dark-mode token values so cards and tables read defined before any hover/focus interaction, matching the reference screenshots' crispness. **Implementation default:** `--border: #23292e → #2b333a` and `--surface-1: #11161a → #141a1f` in `tokens.css`'s dark block only — value-only edits, no new token names, no component changes, because every `border-border`/`bg-card` consumer already inherits from these two tokens via `theme.css`'s `@theme inline` mapping. Light-mode tokens are untouched (`ThemeToggle.tsx` forces dark; light mode is vestigial — out of scope).

- **Dimension 1.1** — dark-mode `--border` is `#2b333a` → Test `test_border_token_bumped`
- **Dimension 1.2** — dark-mode `--surface-1` is `#141a1f` → Test `test_surface1_token_bumped`
- **Dimension 1.3** — light-mode `--border`/`--surface-1` values are byte-identical to before this spec → Test `test_light_tokens_unchanged`

### §2 — A `UsageBar` primitive, replacing the bespoke `.app-meter` ✅ DONE (all Dimensions)

Extracts the existing meter markup in `BillingBalanceCard` into a reusable `design-system` component so any future usage/quota surface has a primitive to reach for instead of hand-rolling another one-off. **Implementation default:** `UsageBar` accepts an optional label, a 0-100 percentage (rendered `tabular-nums`, shown only when `label` is supplied), and an optional sub-caption node; track renders with `bg-accent` (maps to `--surface-3`), fill with the existing pulse gradient, height/radius matching the current `.app-meter` (8px, full pill) exactly — this is a lift-and-shift of a proven pattern, not a redesign. The `meter-fill` keyframe and its reduced-motion override move with the component (renamed generically, no longer billing-specific) rather than being dropped. **Amended at EXECUTE:** `label` is optional, not required (see Interfaces + Discovery) — `BillingBalanceCard`'s existing meter has no visible label/percentage (it's `aria-hidden`, and the dollar headline above it already states the value), so making `label` mandatory would have added new visible text to a card the spec commits to leaving visually unchanged.

- **Dimension 2.1** — `UsageBar` renders track+fill always, and a label + tabular-nums percentage row only when `label` is supplied; an optional sub-caption always renders when given; exported from `@agentsfleet/design-system` → Test `test_usage_bar_renders`
- **Dimension 2.2** — `BillingBalanceCard` renders its meter via `<UsageBar>`, no bespoke inline meter markup remaining → Test `test_billing_balance_uses_usage_bar`
- **Dimension 2.3** — `.app-meter` and `.app-meter > span` are fully removed from `globals.css`, zero remaining references → Test `test_app_meter_removed`
- **Dimension 2.4** — `UsageBar`'s track/fill classes resolve only to existing mapped Tailwind tokens (`bg-accent`, `pulse`/`pulse-dim`); no arbitrary hex or one-off utility → Test `test_usage_bar_no_arbitrary_colors`

### §3 — `DataTable` sticky header and row density ✅ DONE (all Dimensions)

Keeps a long table's column labels in view while scrolling, and tightens default row padding one notch for a denser, PlanetScale-like read. **Implementation default:** an opt-in `stickyHeader` boolean prop wraps the table body in a bounded-height (`overflow-y-auto`) scroll container with `<thead>` set `sticky top-0` inside it — a separate scroll axis from the existing `overflow-x-auto` wrapper, so horizontal scroll on narrow viewports is unaffected. Default (`stickyHeader` unset) renders exactly as today. Cell padding drops one notch (`py-2` → `py-1.5`) unconditionally — small, uniform, no consumer-side change needed.

- **Dimension 3.1** — `stickyHeader={true}` bounds the table height and keeps `<thead>` visually pinned while the body scrolls; `stickyHeader` unset renders identically to the pre-change markup → Test `test_data_table_sticky_header_optin`
- **Dimension 3.2** — default cell padding is `py-1.5` (was `py-2`) with no other markup change → Test `test_data_table_row_density`
- **Dimension 3.3** — all five existing `DataTable` consumers (`ApiKeyList`, `CatalogueList`, `RunnerList`, `SecretsList`, `BillingUsageTab`) still render their rows and remain clickable/keyboard-navigable where they were before → Test `test_data_table_consumers_unaffected`

## Interfaces

```
DataTable<T> gains one new optional prop (all existing props unchanged):
  stickyHeader?: boolean   // default false — bounds height + pins <thead> when true

UsageBar (new, exported from @agentsfleet/design-system):
  { label?: string; pct: number; sublabel?: React.ReactNode; className?: string }
  — pct is clamped [0,100] by the component, not the caller. `label` is
  optional (amended at EXECUTE, see Discovery): BillingBalanceCard's meter is
  intentionally unlabeled/aria-hidden today (the dollar headline above it
  already states the value) — the label+percentage row renders only when a
  caller supplies `label`, so migrating BillingBalanceCard adds no visible
  text that wasn't there before.

No HTTP/API/schema change. No change to computeReceiveCharge/computeStageCharge
or any billing data shape — BillingBalanceCard's existing props
(TenantBilling, ChargeSummary) are unchanged; only its internal rendering
of the meter changes.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Token bump breaks a snapshot/visual test pinned to the old hex | Test asserts a literal token value | Update the co-located test to the new value in the same commit |
| `stickyHeader` used without realizing it bounds height | Caller sets the prop on a table meant to grow with the page | Prop is opt-in and documented on the type; default behaviour is unchanged, so an accidental regression requires an explicit opt-in |
| `.app-meter` removed while something else still references it | Assumption that `BillingBalanceCard` was the sole consumer is wrong | Dimension 2.3's grep runs before deletion; a surviving reference blocks the removal, not silently breaks it |
| Mount-fill animation regresses to an instant jump | `meter-fill` keyframe dropped instead of retargeted during extraction | Dimension 2.1's test asserts the animation class hook is present on `UsageBar`'s fill element |

## Invariants

1. `UsageBar`'s track/fill resolve only to existing mapped design-system tokens — no inline hex, no arbitrary Tailwind value — enforced by Dimension 2.4's grep-based test.
2. `DataTable`'s default (no `stickyHeader`) render path is markup-equivalent to today for all five existing consumers except the intentional padding change — enforced by Dimension 3.3's consumer tests.
3. Light-mode token values are untouched — enforced by Dimension 1.3's byte-comparison test.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_border_token_bumped` | `tokens.css` dark block `--border` equals `#2b333a` |
| 1.2 | unit | `test_surface1_token_bumped` | `tokens.css` dark block `--surface-1` equals `#141a1f` |
| 1.3 | unit | `test_light_tokens_unchanged` | `tokens.css` light block `--border`/`--surface-1` unchanged from baseline |
| 2.1 | unit | `test_usage_bar_renders` | `<UsageBar label="Monthly run budget" pct={62} sublabel="…"/>` renders label, `62%` (tabular-nums), a track, and a fill sized to 62%; `<UsageBar pct={30}/>` (no label) renders track+fill only, no label/percentage text |
| 2.2 | unit | `test_billing_balance_uses_usage_bar` | `BillingBalanceCard` render tree contains `UsageBar`'s `data-slot` (via the overridden `balance-meter` testid); no bespoke `.app-meter` div; fill width matches `summary.meterPct`; no label/percentage text rendered |
| 2.3 | unit | `test_app_meter_removed` | `grep -rn 'app-meter' ui/packages/app/app/globals.css` → 0 matches |
| 2.4 | unit | `test_usage_bar_no_arbitrary_colors` | `UsageBar.tsx` contains no `#`-hex literal and no `[` arbitrary Tailwind value in its class strings |
| 3.1 | unit | `test_data_table_sticky_header_optin` | `stickyHeader` unset → markup matches pre-change baseline; `stickyHeader={true}` → wrapper has bounded height + `<thead>` has `sticky top-0` |
| 3.2 | unit | `test_data_table_row_density` | default `<td>`/`<th>` classes include `py-1.5`, not `py-2` |
| 3.3 | regression | `test_data_table_consumers_unaffected` | `ApiKeyList`, `CatalogueList`, `RunnerList`, `SecretsList`, `BillingUsageTab` each render their existing rows without a thrown error or missing row |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Token contrast bumped (§1) | `grep -E '\-\-border: #23292e\|\-\-surface-1: #11161a' ui/packages/design-system/src/tokens.css` | no output (old values gone from the dark block) | P2 | ✅ no output |
| R2 | `UsageBar` exported and consumed (§2) | `grep -n "UsageBar" ui/packages/design-system/src/index.ts "ui/packages/app/app/(dashboard)/settings/billing/components/BillingBalanceCard.tsx"` | both files match | P2 | ✅ both match |
| R3 | `.app-meter` fully removed (§2) | `grep -rn 'app-meter' ui/packages/app/app/globals.css` | no output | P2 | ✅ no output |
| R4 | `DataTable` sticky header opt-in present, default unchanged (§3) | inspect `DataTable.tsx` for `stickyHeader` prop | prop exists; default path unmodified | P2 | ✅ prop exists; default-path test passes unmodified |
| R5 | Diff inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the table | P0 | ✅ 13 paths, all in scope (incl. this spec, `BillingUsageTab.tsx` — the RULE HLP fix from `/review` — and co-located tests) |
| S1 | UI unit tests pass | `make test-unit-agentsfleet` (or the ui package test runner) | exit 0 | P0 | ✅ design-system 448/448, app 1204/1204 |
| S2 | Lint clean | `make lint` | exit 0 | P0 | ✅ `make lint-apps-ds-ctl` — app/design-system/agentsfleet all clean |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ no leaks found |
| S4 | No oversize file | `git diff --name-only origin/main \| grep '\.tsx\?$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no output |
| S5 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | ✅ `.app-meter` 0 matches; `meter-fill` 1 match (renamed keyframe, not orphaned) |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P2 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted | — |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `.app-meter` (CSS class) | `grep -rn "app-meter" ui/packages/app/app/globals.css ui/packages/app/app/\(dashboard\)/` | 0 matches |
| `meter-fill` keyframe (renamed, not deleted) | `grep -rn "meter-fill" ui/packages/app/app/globals.css` | 1 match — the renamed keyframe still backing `UsageBar`'s fill animation, not orphaned |

## Out of Scope

- **Light-mode token parity** — `ThemeToggle.tsx` forces dark on mount; light mode is vestigial. No light-mode changes in this spec.
- **The "compact capacity rows" pattern** (Warm runners / Queue depth) from the design proposal — no required change identified; `DashboardRow`'s existing icon-chip shape already covers it if/when a runner-capacity panel is built. Left for a future spec.
- **Any new page, route, or dashboard panel** — this spec touches existing surfaces only.
- **Any backend/schema/analytics change** — presentation-only.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A user opens any dashboard page and the cards/tables read crisp and legible before touching anything; on the Billing page the balance meter looks and behaves exactly as before, just built on a primitive that can be reused elsewhere.
2. **Preserved user behaviour** — Every existing page, link, click, and data value is unchanged; only resting-state contrast, one component's internal markup, and table row density change.
3. **Optimal-way check** — Yes: two token-value edits plus extracting one already-proven pattern into a primitive is the minimal lift; no rebuild buys more for a P2 polish pass.
4. **Rebuild-vs-iterate** — Iterate. Presentation-only; no determinism or data-model impact.
5. **What we build** — two token value bumps, one new `UsageBar` primitive + `BillingBalanceCard` migration, one opt-in `DataTable` prop + a uniform padding tighten.
6. **What we do NOT build** — any new page, any new metric/event, light-mode changes, the runner-capacity "bonus" panel, any backend/schema change.
7. **Fit with existing features** — Extends `design-system` primitives already used across settings/admin pages; must not regress `BillingBalanceCard`'s credit-exhausted alert or any of `DataTable`'s five existing consumers.
8. **Surface order** — UI-only; no CLI/API surface.
9. **Dashboard restraint** — No new card, control, or metric is added; existing surfaces become more legible and one duplicated pattern is consolidated — nothing new is exposed before its signal is real.
10. **Confused-user next step** — N/A — no new error/edge state; the existing exhausted-balance alert and "Buy credits" flow are unchanged.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One workstream, three small Sections by surface (tokens / UsageBar extraction / table structure), each independently verifiable — same shape as M117's dashboard-polish workstream.
- **Alternatives considered:** (a) Introduce a parallel "elevated" border/surface token pair used only by select components — rejected: inconsistent adoption surface, larger blast radius than a direct value bump on the base tokens. (b) Design `UsageBar` from scratch with new visual language — rejected: `.app-meter` is a proven, already-shipped pattern; extraction is lower-risk than redesign.
- **Patch-vs-refactor verdict:** a **patch** — token values, one component extraction, one opt-in prop. No architecture move.

## Discovery (consult log)

- **Consults** — Indy reviewed a design proposal (artifact) benchmarked against two PlanetScale dashboard screenshots and agreed to proceed; three deltas selected from the proposal's original three (border contrast, `UsageBar`, table structure), the "bonus" capacity-row pattern deferred (no ack needed — it was explicitly optional in the proposal, not dropped scope). Mid-EXECUTE, Indy asked for "a large refactor" for optimization/performance with no target named; flagged that this contradicts the spec's own patch-vs-refactor verdict and that there's no performance concern in scope (presentation-only diff, no data-fetching/compute) — Indy confirmed **stick to the patch**, no refactor.
- **EXECUTE reconciliations** — §2's `UsageBar.label` was pinned required in the Interfaces section at authoring; discovered at EXECUTE that `BillingBalanceCard`'s existing meter is intentionally unlabeled and `aria-hidden` (the dollar headline above it already states the value), so a required `label` would have added new visible text — amended `label` to optional (Interfaces + §2 updated in the same diff) so the migration adds zero visible change to `BillingBalanceCard`.
- **Metrics review** — no product/operator signal changes; presentation-only.
- **Skill-chain outcomes** — `/write-unit-test`: tests written per Dimension before this review (§ Test Specification), design-system 446/446 + app 1203/1203 green. `/review` (medium effort, 3 finder angles + verify): 2 confirmed findings, both fixed in this diff — (1) `stickyHeader` shipped with zero production consumers (RULE HLP) → wired onto `BillingUsageTab`'s paginated usage-history table, the real long-list case §3 was written for; (2) the sticky-header scroll region was hardcoded (`max-h-96`, no override) and not keyboard-reachable → added `tabIndex={0}` + `role="region"` + a caption-derived `aria-label`. Correctness and cleanup/reuse angles returned no findings. `kishore-babysit-prs`: not yet run — pending PR open.
- **Deferrals** — {empty at creation}
